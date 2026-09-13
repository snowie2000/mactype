mod appinit;
mod legacy_mactray;
mod legacy_migration;
mod model;
mod open_service;
mod orchestrator;
mod publish;
mod startup_coordination;
mod status;
mod system_backend;

#[cfg(test)]
use appinit::appinit_view_conflict;

pub(crate) use appinit::registry_conflict_detected;
#[cfg(test)]
pub(crate) use legacy_mactray::LegacyTrayStartupState;
#[cfg(test)]
use legacy_mactray::{disable_legacy_tray_startup_with, LegacyTrayStartupCoordinator};
pub(crate) use legacy_mactray::{
    LegacyTrayConflictState, LegacyTrayExitRequest, LegacyTrayProcessState, LegacyTrayStatus,
};
pub(crate) use model::{
    MachineAction, MachineBackend, MachineStatus, PublicMachineAction, TrayLoginState,
};
pub(crate) use open_service::LegacyMacTrayStatus as LegacyServiceStatus;
pub(crate) use open_service::{
    machine_roots, management_package_state as service_management_package_state,
};
use orchestrator::{execute_machine_action_with, tray_apply_with, tray_login_with};
pub(crate) use publish::publish_profile_transaction_with;
pub(crate) use status::status;
#[cfg(test)]
use status::{project_new_service_capabilities, project_system_injection_active};
use system_backend::SystemMachineBackend;

pub(crate) fn tray_login(
    paused: bool,
    ci_smoke: bool,
    active_profile: Option<&[u8]>,
) -> TrayLoginState {
    let expected = active_profile.map(|profile| {
        mactype_service_contract::GenerationId::from_profile_bytes(profile)
            .as_str()
            .to_owned()
    });
    tray_login_with(
        &mut SystemMachineBackend,
        paused,
        ci_smoke,
        expected.as_deref(),
    )
}

pub(crate) fn execute(action: MachineAction, profile: Option<&[u8]>) -> Result<(), String> {
    execute_machine_action_with(&mut SystemMachineBackend, action, profile)
}

pub(crate) fn tray_apply(paused: bool, profile: &[u8]) -> Result<(), String> {
    tray_apply_with(&mut SystemMachineBackend, paused, profile)
}

pub(crate) fn request_legacy_tray_exit(expected: &LegacyTrayExitRequest) -> Result<(), String> {
    legacy_mactray::request_tray_exit(expected)
}

pub(crate) fn disable_legacy_tray_startup() -> Result<(), String> {
    startup_coordination::disable()
}

pub(crate) fn dispatch_privileged_command() -> Option<i32> {
    legacy_migration::dispatch_current_user_restore_command()
        .or_else(open_service::dispatch_privileged_command)
}

#[tauri::command]
pub(crate) fn reveal_system_service() -> Result<(), String> {
    open_service::reveal_system_service()
}

#[cfg(test)]
mod tests;
