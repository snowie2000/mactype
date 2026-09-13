mod broker_result;
mod file_guard;
mod identity;
mod legacy_status;
mod request;
mod runtime;
mod startup_lifecycle;

#[cfg(test)]
use broker::{
    current_executable_path_gate_for_test, elevated_package_preflight_for_layout,
    resolve_installed_package_for_trusted_layout, resolve_service_package_for_layouts,
    service_package_preflight_for_layouts,
};

use broker_result::{
    decode_broker_result_frame, encode_broker_result_frame, BrokerResultDisposition,
    BrokerResultMessage, BROKER_RESULT_HEADER_BYTES, BROKER_RESULT_MAGIC, BROKER_RESULT_VERSION,
    MAX_BROKER_RESULT_BYTES,
};
use file_guard::{read_bounded_regular_file, reject_reparse_chain};
use identity::{
    classify_owned_installation, configured_service_binary, is_protected_service_binary, same_path,
    select_service_health, validated_reveal_binary, LiveHealthReport,
};
#[cfg(test)]
use legacy_status::legacy_migration_available;
pub(crate) use legacy_status::{legacy_status, LegacyMacTrayStatus};
pub(crate) use request::SystemServiceAction;
use request::{
    decode_profile_transfer_frame, encode_profile_transfer_frame,
    privileged_request_from_arguments, ProfileTransferToken, BROKER_SWITCH, BROKER_TRANSFER_SWITCH,
    PROFILE_TRANSFER_HEADER_BYTES, PROFILE_TRANSFER_MAGIC, PROFILE_TRANSFER_NONCE_BYTES,
    PROFILE_TRANSFER_VERSION,
};
#[cfg(test)]
use runtime::bundled_runtime_version;
use runtime::{
    bundled_service_binary, parse_bundled_runtime_manifest, BundledRuntimeManifest,
    MAX_BUNDLED_MANIFEST_BYTES,
};
use startup_lifecycle::{finish_action_with_startup_receipts, StartupReceiptRestorer};

const INSTALLATION_REQUIRED_PREFIX: &str = "control-center-installation-required:";
const INSTALLATION_INCOMPLETE_PREFIX: &str = "control-center-installation-incomplete:";
const INSTALLATION_UNTRUSTED_PREFIX: &str = "control-center-installation-untrusted:";

fn installation_preflight_failure(error: &str) -> bool {
    [
        INSTALLATION_REQUIRED_PREFIX,
        INSTALLATION_INCOMPLETE_PREFIX,
        INSTALLATION_UNTRUSTED_PREFIX,
    ]
    .iter()
    .any(|prefix| error.starts_with(prefix))
}

fn management_package_state_from_error(
    error: &str,
) -> crate::service_contract::ServiceManagementPackageState {
    use crate::service_contract::ServiceManagementPackageState;
    if error.starts_with(INSTALLATION_REQUIRED_PREFIX) {
        ServiceManagementPackageState::NotInstalled
    } else if error.starts_with(INSTALLATION_INCOMPLETE_PREFIX) {
        ServiceManagementPackageState::Incomplete
    } else {
        ServiceManagementPackageState::Untrusted
    }
}

pub(crate) fn machine_roots() -> Result<(PathBuf, PathBuf), String> {
    #[cfg(windows)]
    {
        windows::machine_roots()
    }
    #[cfg(not(windows))]
    {
        Err("machine roots are available only on Windows".to_owned())
    }
}

pub(crate) fn management_package_state() -> crate::service_contract::ServiceManagementPackageState {
    #[cfg(windows)]
    {
        broker::service_package()
            .map(|_| crate::service_contract::ServiceManagementPackageState::Ready)
            .unwrap_or_else(|failure| management_package_state_from_error(&failure.error))
    }
    #[cfg(not(windows))]
    {
        crate::service_contract::ServiceManagementPackageState::NotInstalled
    }
}

use crate::service_contract::{
    HealthState, InstallationState, RuntimeState, ServiceBackend, SystemServiceStatus,
};
use mactype_service_contract::{GenerationId, HealthReport};
use serde::Serialize;
use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(crate) fn run_action(
    action: SystemServiceAction,
    profile: Option<&[u8]>,
) -> Result<(), String> {
    if profile.is_some() != action.needs_profile_input()
        || profile.is_some_and(|bytes| {
            bytes.is_empty() || bytes.len() > mactype_service_contract::MAX_PROFILE_BYTES
        })
    {
        return Err("the service action has an invalid profile payload".to_owned());
    }
    #[cfg(windows)]
    let service_control_center = match broker::service_package() {
        Ok(package) => package.control_center,
        Err(failure) => {
            let error = failure.error;
            record_action_failure(action, profile, &error, Some(*failure.diagnostics));
            return Err(error);
        }
    };
    let result = {
        #[cfg(windows)]
        {
            windows::run_elevated_at(action, profile, service_control_center)
        }
        #[cfg(not(windows))]
        {
            let _ = profile;
            Err("system service control is available only on Windows".to_owned())
        }
    };
    let result =
        finish_action_with_startup_receipts(&mut SystemStartupReceiptRestorer, action, result);
    match result {
        Ok(()) => Ok(()),
        Err(error) if expected_action_blocker(&error) => Err(error),
        Err(error) => {
            record_action_failure(action, profile, &error, None);
            Err(format!(
                "{INTERNAL_OPERATION_FAILURE_PREFIX}{}",
                action.broker_verb()
            ))
        }
    }
}

fn record_action_failure(
    action: SystemServiceAction,
    profile: Option<&[u8]>,
    error: &str,
    installation_preflight: Option<crate::diagnostics::InstallationPreflightDiagnostics>,
) {
    let profile_text = profile.map(|bytes| String::from_utf8_lossy(bytes).into_owned());
    let failure = operation_failure(action, error, installation_preflight);
    let redactions = profile_text.as_deref().into_iter().collect::<Vec<_>>();
    let _ = crate::diagnostics::record_operation_failure(&failure, &redactions);
}

const INTERNAL_OPERATION_FAILURE_PREFIX: &str = "control-center-internal-operation-failed:";

fn expected_action_blocker(error: &str) -> bool {
    [
        "administrator approval was cancelled",
        "AppInit conflicts block",
        "AppInit registry mode conflicts",
        "the legacy MacTray tray mode blocks",
        "a legacy MacType service is still installed",
        "the fixed service name became foreign or inaccessible",
    ]
    .iter()
    .any(|prefix| error.starts_with(prefix))
        || installation_preflight_failure(error)
}

fn operation_failure(
    action: SystemServiceAction,
    error: &str,
    installation_preflight: Option<crate::diagnostics::InstallationPreflightDiagnostics>,
) -> crate::diagnostics::OperationFailure {
    let stage = if installation_preflight.is_some() {
        "installation-preflight"
    } else {
        error
            .split_once(':')
            .map(|(stage, _)| stage)
            .unwrap_or(action.broker_verb())
    };
    let channel_failure = [
        "broker result channel failed:",
        "reporting the broker result failed:",
    ]
    .iter()
    .find_map(|marker| {
        error
            .split_once(marker)
            .map(|(_, detail)| detail.trim().to_owned())
    });
    let modern = status();
    let legacy = super::legacy_mactray::status(super::registry_conflict_detected());
    let receipt = super::legacy_migration::current_stage_name().unwrap_or("unavailable");
    let rollback = if installation_preflight.is_some() {
        "not-applicable"
    } else if error.contains("rollback failed") || error.contains("restoration failed") {
        "failed"
    } else if receipt == "rollback-completed" {
        "completed"
    } else if receipt == "legacy-stopped" {
        "fail-closed-legacy-stopped"
    } else {
        "not-applicable-or-unavailable"
    };
    crate::diagnostics::OperationFailure {
        operation: action.broker_verb().to_owned(),
        stage: stage.to_owned(),
        error_chain: error.to_owned(),
        broker_exit_code: None,
        channel_failure,
        rollback: rollback.to_owned(),
        final_state: format!(
            "legacy={:?}/{:?}/win32={:?}; modern={:?}/{:?}/{:?}/win32={:?}; receipt={receipt}",
            legacy.presence,
            legacy.state,
            legacy.win32_error,
            modern.installation,
            modern.runtime,
            modern.health,
            modern.win32_error,
        ),
        installation_preflight,
    }
}

struct SystemStartupReceiptRestorer;

impl StartupReceiptRestorer for SystemStartupReceiptRestorer {
    fn restore_local_machine(&mut self) -> Result<(), String> {
        #[cfg(windows)]
        {
            windows::run_elevated(SystemServiceAction::RestoreLegacyTrayAutostart, None)
        }
        #[cfg(not(windows))]
        {
            Err("local-machine startup restoration is available only on Windows".to_owned())
        }
    }

    fn restore_current_user(&mut self) -> Result<(), String> {
        super::legacy_migration::restore_startup_scope(
            super::legacy_migration::StartupReceiptScope::CurrentUser,
        )
    }
}

pub(crate) fn dispatch_privileged_command() -> Option<i32> {
    let request = match privileged_request_from_arguments(std::env::args_os()) {
        Ok(None) => return None,
        Ok(Some(request)) => request,
        Err(_) => return Some(21),
    };
    let result = {
        #[cfg(windows)]
        {
            windows::run_privileged(request.action, &request.transfer)
        }
        #[cfg(not(windows))]
        {
            let _ = request;
            Err("system service control is available only on Windows".to_owned())
        }
    };
    Some(if result.is_ok() { 0 } else { 21 })
}

pub(crate) fn status() -> SystemServiceStatus {
    #[cfg(windows)]
    {
        windows::query()
    }
    #[cfg(not(windows))]
    {
        absent_status()
    }
}

pub(crate) fn reveal_system_service() -> Result<(), String> {
    #[cfg(windows)]
    {
        windows::reveal_system_service()
    }
    #[cfg(not(windows))]
    {
        Err("system service reveal is available only on Windows".to_owned())
    }
}

fn absent_status() -> SystemServiceStatus {
    SystemServiceStatus {
        backend: ServiceBackend::None,
        installation: InstallationState::Absent,
        runtime: RuntimeState::Stopped,
        health: HealthState::Unknown,
        binary_path: None,
        win32_error: None,
        active_profile_digest: None,
        configuration_drift: false,
        can_install: true,
        can_remove: false,
        can_start: false,
        can_stop: false,
        can_repair: false,
        can_upgrade: false,
    }
}

use migration::{
    migrate_from_legacy, migration_activation_actions, remove_legacy_after_verification,
    MigrationBackend, MigrationVerification,
};
#[cfg(windows)]
mod broker;
mod migration;
#[cfg(windows)]
mod platform;
#[cfg(windows)]
mod profile_transfer;
#[cfg(windows)]
mod windows;

#[cfg(test)]
mod tests;
