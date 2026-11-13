use super::{
    DesignationPlan, MachineAction, MachineBackend,
    designation::{designation_plan_with_package, designation_status_with},
};
use crate::{
    machine_integration::open_service::action_failure::{
        ActionBlocker, ActionFailure, LegacyServiceBlockContext, RollbackOutcome,
    },
    service_contract::SystemServiceStatus,
};
use std::time::Duration;

const READY_ATTEMPTS: usize = 200;
const READY_POLL: Duration = Duration::from_millis(50);

pub(super) fn publish_profile_transaction_with(
    backend: &mut impl MachineBackend,
    profile: &[u8],
) -> Result<(), ActionFailure> {
    if profile.is_empty() || profile.len() > mactype_service_contract::MAX_PROFILE_BYTES {
        return Err(ActionFailure::internal(
            "the published profile payload is outside the allowed range",
        ));
    }
    let before = backend.new_service_status();
    if before.backend == crate::service_contract::ServiceBackend::Foreign
        || !matches!(
            before.installation,
            crate::service_contract::InstallationState::Absent
                | crate::service_contract::InstallationState::Current
                | crate::service_contract::InstallationState::Outdated
        )
        || !matches!(
            before.runtime,
            crate::service_contract::RuntimeState::Running
                | crate::service_contract::RuntimeState::Stopped
        )
    {
        return Err(ActionFailure::internal(
            "the new service is foreign, transitioning, or unsafe",
        ));
    }

    let expected = mactype_service_contract::GenerationId::from_profile_bytes(profile);
    let profile_changed = before.active_profile_digest.as_deref() != Some(expected.as_str());

    if before.runtime == crate::service_contract::RuntimeState::Running {
        backend.execute(MachineAction::Stop, None)?;
    }
    if let Err(error) = backend.execute(MachineAction::PublishProfile, Some(profile)) {
        if before.runtime == crate::service_contract::RuntimeState::Running
            && let Err(restart) = backend.execute(MachineAction::Start, None)
        {
            return Err(error
                .append_detail(format!(
                    "; machine integration cleanup is unknown because the prior service could not be restarted: {restart}"
                ))
                .with_rollback(RollbackOutcome::Failed));
        }
        return Err(error);
    }

    let activation = match before.installation {
        crate::service_contract::InstallationState::Absent => backend
            .execute(MachineAction::Install, None)
            .and_then(|()| backend.execute(MachineAction::Start, None)),
        crate::service_contract::InstallationState::Outdated => backend
            .execute(MachineAction::Upgrade, None)
            .and_then(|()| backend.execute(MachineAction::Start, None)),
        crate::service_contract::InstallationState::Current => {
            backend.execute(MachineAction::Start, None)
        }
        _ => unreachable!("unsafe installation states were rejected"),
    };
    if let Err(error) = activation {
        return Err(combine_rollback_error(
            error,
            rollback_published_profile(backend, &before, profile_changed),
        ));
    }

    if let Err(error) = wait_for_published_profile_with(
        expected.as_str(),
        READY_ATTEMPTS,
        || backend.new_service_status(),
        || std::thread::sleep(READY_POLL),
    ) {
        return Err(combine_rollback_error(
            error,
            rollback_published_profile(backend, &before, profile_changed),
        ));
    }
    Ok(())
}

/// Makes `profile` the run profile without changing whether the service runs:
/// a running service switches live, an installed stopped service receives the
/// generation for its next start, and an unavailable machine step is skipped.
pub(super) fn designate_profile_transaction_with(
    backend: &mut impl MachineBackend,
    profile: &[u8],
) -> Result<(), ActionFailure> {
    if profile.is_empty() || profile.len() > mactype_service_contract::MAX_PROFILE_BYTES {
        return Err(ActionFailure::internal(
            "the designated profile payload is outside the allowed range",
        ));
    }
    let status = designation_status_with(backend);
    let plan = designation_plan_with_package(&status, backend.service_management_package_state());
    match plan {
        DesignationPlan::PublishLive => {
            if backend
                .legacy_service_blocks_activation()
                .map_err(ActionFailure::from)?
            {
                return Err(ActionFailure::blocked(
                    ActionBlocker::LegacyServiceStillInstalled(
                        LegacyServiceBlockContext::ApplyProfile,
                    ),
                ));
            }
            publish_profile_transaction_with(backend, profile)
        }
        DesignationPlan::HoldForNextStart => {
            backend.execute(MachineAction::PublishProfile, Some(profile))
        }
        DesignationPlan::KeepLocalUntilServiceStart => Ok(()),
    }
}

fn wait_for_published_profile_with(
    expected_digest: &str,
    maximum_attempts: usize,
    mut observe: impl FnMut() -> SystemServiceStatus,
    mut wait: impl FnMut(),
) -> Result<(), ActionFailure> {
    if maximum_attempts == 0 {
        return Err(ActionFailure::internal(
            "published profile verification has no polling budget",
        ));
    }
    let mut last = None;
    for attempt in 0..maximum_attempts {
        let status = observe();
        if status.system_injection_active(Some(expected_digest)) {
            return Ok(());
        }
        last = Some(status);
        if attempt + 1 < maximum_attempts {
            wait();
        }
    }
    let status = last.expect("at least one published profile observation");
    Err(ActionFailure::internal(format!(
        "the new service did not become Ready with the published profile: backend={:?}, installation={:?}, runtime={:?}, health={:?}, activeProfileDigest={}, expectedProfileDigest={expected_digest}, win32Error={}",
        status.backend,
        status.installation,
        status.runtime,
        status.health,
        status.active_profile_digest.as_deref().unwrap_or("missing"),
        status
            .win32_error
            .map_or_else(|| "none".to_owned(), |error| error.to_string())
    )))
}

fn combine_rollback_error(
    primary: ActionFailure,
    rollback: Result<(), ActionFailure>,
) -> ActionFailure {
    match rollback {
        Ok(()) => primary,
        Err(cleanup) => primary
            .append_detail(format!(
                "; machine integration cleanup is unknown: {cleanup}"
            ))
            .with_rollback(RollbackOutcome::Failed),
    }
}

fn rollback_published_profile(
    backend: &mut impl MachineBackend,
    before: &SystemServiceStatus,
    profile_changed: bool,
) -> Result<(), ActionFailure> {
    let mut failures = Vec::new();
    let mut cleanup = vec![("stop", MachineAction::Stop)];
    if profile_changed {
        cleanup.push(("profile rollback", MachineAction::Rollback));
    }
    for (label, action) in cleanup {
        if let Err(error) = backend.execute(action, None) {
            failures.push(format!("{label}: {error}"));
        }
    }
    match before.installation {
        crate::service_contract::InstallationState::Absent => {
            if let Err(error) = backend.execute(MachineAction::Remove, None) {
                failures.push(format!("remove newly installed service: {error}"));
            }
        }
        _ if before.runtime == crate::service_contract::RuntimeState::Running => {
            if let Err(error) = backend.execute(MachineAction::Start, None) {
                failures.push(format!("restart prior service: {error}"));
            }
        }
        _ => {}
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(ActionFailure::internal(failures.join("; ")))
    }
}
