mod appinit;
mod designation;
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
use designation::designate_run_profile_with;
use designation::execute_designation_plan_with;
pub(crate) use designation::{DesignationPlan, designation_plan, profile_publication_supported};
#[cfg(test)]
pub(crate) use legacy_mactray::LegacyTrayStartupState;
pub(crate) use legacy_mactray::{
    LegacyTrayConflictState, LegacyTrayExitRequest, LegacyTrayProcessState, LegacyTrayStatus,
};
#[cfg(test)]
use legacy_mactray::{LegacyTrayStartupCoordinator, disable_legacy_tray_startup_with};
pub use model::DesignationEffect;
use model::MachineBackend;
pub(crate) use model::{MachineAction, MachineStatus, PublicMachineAction, TrayLoginState};
pub(crate) use open_service::LegacyMacTrayStatus as LegacyServiceStatus;
pub(crate) use open_service::{
    machine_roots, management_package_state as service_management_package_state,
};
use orchestrator::{execute_machine_action_with, tray_login_with};
use publish::{designate_profile_transaction_with, publish_profile_transaction_with};
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

fn action_result(
    result: Result<(), open_service::action_failure::ActionFailure>,
) -> Result<(), String> {
    result.map_err(|failure| failure.user_message())
}

pub(crate) fn execute(action: MachineAction, profile: Option<&[u8]>) -> Result<(), String> {
    action_result(execute_machine_action_with(
        &mut SystemMachineBackend,
        action,
        profile,
    ))
}

pub(crate) fn designate_run_profile(profile: &[u8]) -> Result<DesignationEffect, String> {
    let machine_status = status(Some(profile));
    let plan = designation_plan(&machine_status);
    execute_designation_plan_with(&mut SystemMachineBackend, profile, plan)
        .map_err(|failure| failure.user_message())
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
