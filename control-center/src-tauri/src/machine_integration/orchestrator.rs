use super::{MachineAction, MachineBackend, TrayLoginState};
use crate::{
    machine_integration::open_service::action_failure::{
        ActionBlocker, ActionFailure, AppInitConflictContext, LegacyServiceBlockContext,
        LegacyTrayBlockContext,
    },
    service_contract::SystemServiceStatus,
};

pub(super) fn tray_login_with(
    backend: &mut impl MachineBackend,
    paused: bool,
    ci_smoke: bool,
    expected_profile_digest: Option<&str>,
) -> TrayLoginState {
    let status = backend.new_service_status();
    if paused {
        TrayLoginState::Paused
    } else if !ci_smoke && status.system_injection_active(expected_profile_digest) {
        TrayLoginState::UsingRunningNewService
    } else {
        TrayLoginState::Observing
    }
}

pub(super) fn execute_machine_action_with(
    backend: &mut impl MachineBackend,
    action: MachineAction,
    profile: Option<&[u8]>,
) -> Result<(), ActionFailure> {
    let profile_contract_is_valid = match action {
        MachineAction::Start
        | MachineAction::PublishProfile
        | MachineAction::DesignateProfile
        | MachineAction::MigrateFromLegacy
        | MachineAction::RemoveLegacy => profile.is_some(),
        _ => profile.is_none(),
    } && !profile.is_some_and(|bytes| {
        bytes.is_empty() || bytes.len() > mactype_service_contract::MAX_PROFILE_BYTES
    });
    if !profile_contract_is_valid {
        return Err(ActionFailure::internal(
            "the machine action has an invalid profile payload",
        ));
    }

    if !matches!(action, MachineAction::Rollback | MachineAction::Stop) {
        let legacy_tray = backend.legacy_tray_status();
        if legacy_tray.blocks_machine_change() {
            return Err(ActionFailure::blocked(ActionBlocker::LegacyTrayModeBlocks(
                LegacyTrayBlockContext::MachineIntegrationChange,
            )));
        }
    }

    let appinit_conflict = if matches!(action, MachineAction::Rollback | MachineAction::Stop) {
        false
    } else {
        backend
            .appinit_conflict()
            .map_err(|error| ActionFailure::internal_at("observe-appinit-conflict", error))?
    };
    let status = backend.new_service_status();
    if action == MachineAction::Rollback {
        return backend.execute(action, profile);
    }
    if let Some(authorized) = native_action_authorized(&status, action) {
        if !authorized {
            return Err(ActionFailure::internal(format!(
                "the current service status does not authorize {action:?}"
            )));
        }
        if appinit_conflict && action != MachineAction::Stop {
            return Err(ActionFailure::blocked(ActionBlocker::AppInitConflict(
                AppInitConflictContext::MachineIntegrationChange,
            )));
        }
        refuse_activation_with_legacy_service(backend, action)?;
        let dispatched_action = if action == MachineAction::Start && profile.is_some() {
            MachineAction::PublishProfile
        } else {
            action
        };
        return backend.execute(dispatched_action, profile);
    }
    if appinit_conflict {
        return Err(ActionFailure::blocked(ActionBlocker::AppInitConflict(
            AppInitConflictContext::MachineIntegrationChange,
        )));
    }
    if status.backend == crate::service_contract::ServiceBackend::Foreign
        || !matches!(
            status.installation,
            crate::service_contract::InstallationState::Absent
                | crate::service_contract::InstallationState::Current
                | crate::service_contract::InstallationState::Outdated
        )
        || !matches!(
            status.runtime,
            crate::service_contract::RuntimeState::Running
                | crate::service_contract::RuntimeState::Stopped
        )
    {
        return Err(ActionFailure::internal(
            "the machine integration state is foreign, transitioning, or unsafe",
        ));
    }
    refuse_activation_with_legacy_service(backend, action)?;
    backend.execute(action, profile)
}

// Fail-fast (before the UAC prompt) when a legacy MacType service is still
// installed: install/start/publish would run the new injector alongside it.
// Retirement must go through Migrate, which stops the legacy service first.
// The elevated broker re-checks this authoritatively to close the UAC-window
// TOCTOU (see open_service::broker::refuse_conflicting_environment_for_activation).
fn refuse_activation_with_legacy_service(
    backend: &mut impl MachineBackend,
    action: MachineAction,
) -> Result<(), ActionFailure> {
    if matches!(
        action,
        MachineAction::Install | MachineAction::Start | MachineAction::PublishProfile
    ) && backend
        .legacy_service_blocks_activation()
        .map_err(|error| ActionFailure::internal_at("observe-legacy-service", error))?
    {
        return Err(ActionFailure::blocked(
            ActionBlocker::LegacyServiceStillInstalled(LegacyServiceBlockContext::StartNewService),
        ));
    }
    Ok(())
}

pub(super) fn native_action_authorized(
    status: &SystemServiceStatus,
    action: MachineAction,
) -> Option<bool> {
    match action {
        MachineAction::Install => Some(status.can_install),
        MachineAction::Upgrade => Some(status.can_upgrade),
        MachineAction::Repair => Some(status.can_repair),
        MachineAction::Remove => Some(status.can_remove),
        MachineAction::Start => Some(status.can_start),
        MachineAction::Stop => Some(status.can_stop),
        MachineAction::PublishProfile
        | MachineAction::DesignateProfile
        | MachineAction::MigrateFromLegacy
        | MachineAction::Rollback
        | MachineAction::RemoveLegacy => None,
    }
}
