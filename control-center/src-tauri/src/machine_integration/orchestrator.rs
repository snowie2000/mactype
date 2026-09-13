use super::{MachineAction, MachineBackend, TrayLoginState};
use crate::service_contract::SystemServiceStatus;

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

pub(super) fn tray_apply_with(
    backend: &mut impl MachineBackend,
    paused: bool,
    profile: &[u8],
) -> Result<(), String> {
    if paused {
        return Err("system injection is paused".to_owned());
    }
    if profile.is_empty() || profile.len() > mactype_service_contract::MAX_PROFILE_BYTES {
        return Err("the active profile payload is outside the allowed range".to_owned());
    }
    execute_machine_action_with(backend, MachineAction::PublishProfile, Some(profile))
}

pub(super) fn execute_machine_action_with(
    backend: &mut impl MachineBackend,
    action: MachineAction,
    profile: Option<&[u8]>,
) -> Result<(), String> {
    let profile_contract_is_valid = match action {
        MachineAction::Start
        | MachineAction::PublishProfile
        | MachineAction::MigrateFromLegacy
        | MachineAction::RemoveLegacy => profile.is_some(),
        _ => profile.is_none(),
    } && !profile.is_some_and(|bytes| {
        bytes.is_empty() || bytes.len() > mactype_service_contract::MAX_PROFILE_BYTES
    });
    if !profile_contract_is_valid {
        return Err("the machine action has an invalid profile payload".to_owned());
    }

    if !matches!(action, MachineAction::Rollback | MachineAction::Stop) {
        let legacy_tray = backend.legacy_tray_status();
        if legacy_tray.blocks_machine_change() {
            return Err(
                "the legacy MacTray tray mode blocks this machine integration change".to_owned(),
            );
        }
    }

    let appinit_conflict = if matches!(action, MachineAction::Rollback | MachineAction::Stop) {
        false
    } else {
        backend.appinit_conflict()?
    };
    let status = backend.new_service_status();
    if action == MachineAction::Rollback {
        return backend.execute(action, profile);
    }
    if let Some(authorized) = native_action_authorized(&status, action) {
        if !authorized {
            return Err(format!(
                "the current service status does not authorize {action:?}"
            ));
        }
        if appinit_conflict && action != MachineAction::Stop {
            return Err("AppInit conflicts block this machine integration change".to_owned());
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
        return Err("AppInit conflicts block this machine integration change".to_owned());
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
        return Err(
            "the machine integration state is foreign, transitioning, or unsafe".to_owned(),
        );
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
) -> Result<(), String> {
    if matches!(
        action,
        MachineAction::Install | MachineAction::Start | MachineAction::PublishProfile
    ) && backend.legacy_service_blocks_activation()?
    {
        return Err(
            "a legacy MacType service is still installed; migrate it before starting the new service"
                .to_owned(),
        );
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
        | MachineAction::MigrateFromLegacy
        | MachineAction::Rollback
        | MachineAction::RemoveLegacy => None,
    }
}
