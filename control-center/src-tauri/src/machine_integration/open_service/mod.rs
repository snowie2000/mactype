pub(super) mod action_failure;
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

use action_failure::{
    ActionBlocker, ActionFailure, ActionFailureKind, INTERNAL_OPERATION_FAILURE_PREFIX,
    InstallationPreflightKind, RollbackOutcome,
};
use broker_result::{
    BROKER_RESULT_HEADER_BYTES, BROKER_RESULT_MAGIC, BROKER_RESULT_VERSION,
    BrokerResultDisposition, BrokerResultMessage, MAX_BROKER_RESULT_BYTES,
    decode_broker_result_frame, encode_broker_result_frame,
};
use file_guard::{read_bounded_regular_file, reject_reparse_chain};
use identity::{
    LiveHealthReport, classify_owned_installation, configured_service_binary, same_path,
    select_service_health, validated_reveal_binary,
};
#[cfg(test)]
use legacy_status::legacy_migration_available;
pub(crate) use legacy_status::{LegacyMacTrayStatus, legacy_status};
pub(crate) use request::SystemServiceAction;
use request::{
    BROKER_SWITCH, BROKER_TRANSFER_SWITCH, PROFILE_TRANSFER_HEADER_BYTES, PROFILE_TRANSFER_MAGIC,
    PROFILE_TRANSFER_NONCE_BYTES, PROFILE_TRANSFER_VERSION, ProfileTransferToken,
    decode_profile_transfer_frame, encode_profile_transfer_frame,
    privileged_request_from_arguments,
};
#[cfg(test)]
use runtime::bundled_runtime_version;
use runtime::{
    BundledRuntimeManifest, MAX_BUNDLED_MANIFEST_BYTES, bundled_service_binary,
    parse_bundled_runtime_manifest,
};
use startup_lifecycle::{StartupReceiptRestorer, finish_action_with_startup_receipts};

fn management_package_state_from_kind(
    kind: InstallationPreflightKind,
) -> crate::service_contract::ServiceManagementPackageState {
    use crate::service_contract::ServiceManagementPackageState;
    match kind {
        InstallationPreflightKind::Required => ServiceManagementPackageState::NotInstalled,
        InstallationPreflightKind::Incomplete => ServiceManagementPackageState::Incomplete,
        InstallationPreflightKind::Untrusted => ServiceManagementPackageState::Untrusted,
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
            .unwrap_or_else(|failure| management_package_state_from_kind(failure.kind))
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
) -> Result<(), ActionFailure> {
    if profile.is_some() != action.needs_profile_input()
        || profile.is_some_and(|bytes| {
            bytes.is_empty() || bytes.len() > mactype_service_contract::MAX_PROFILE_BYTES
        })
    {
        return Err(ActionFailure::internal(
            "the service action has an invalid profile payload",
        ));
    }
    #[cfg(windows)]
    let service_control_center = match broker::service_package() {
        Ok(package) => package.control_center,
        Err(failure) => {
            let failure = ActionFailure::installation_preflight(
                failure.kind,
                failure.diagnostics,
                failure.error,
            );
            record_action_failure(action, profile, &failure);
            return Err(failure);
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
            Err(ActionFailure::internal_at(
                action.broker_verb(),
                "system service control is available only on Windows",
            ))
        }
    };
    let result =
        finish_action_with_startup_receipts(&mut SystemStartupReceiptRestorer, action, result);
    match result {
        Ok(()) => Ok(()),
        Err(
            failure @ ActionFailure {
                kind: ActionFailureKind::Blocked(_),
                ..
            },
        ) => Err(failure),
        Err(failure) => {
            record_action_failure(action, profile, &failure);
            Err(ActionFailure::internal(format!(
                "{INTERNAL_OPERATION_FAILURE_PREFIX}{}",
                action.broker_verb()
            )))
        }
    }
}

fn record_action_failure(
    action: SystemServiceAction,
    profile: Option<&[u8]>,
    failure: &ActionFailure,
) {
    let profile_text = profile.map(|bytes| String::from_utf8_lossy(bytes).into_owned());
    let failure = operation_failure(action, failure);
    let redactions = profile_text.as_deref().into_iter().collect::<Vec<_>>();
    let _ = crate::diagnostics::record_operation_failure(&failure, &redactions);
}

fn operation_failure_metadata(
    action: SystemServiceAction,
    failure: &ActionFailure,
) -> (&str, RollbackOutcome) {
    let stage = if failure.diagnostics().is_some() {
        "installation-preflight"
    } else {
        failure.stage.as_deref().unwrap_or(action.broker_verb())
    };
    (stage, failure.rollback)
}

fn operation_failure(
    action: SystemServiceAction,
    failure: &ActionFailure,
) -> crate::diagnostics::OperationFailure {
    let (stage, rollback) = operation_failure_metadata(action, failure);
    let modern = status();
    let legacy = super::legacy_mactray::status(super::registry_conflict_detected());
    let receipt = super::legacy_migration::current_stage_name().unwrap_or("unavailable");
    crate::diagnostics::OperationFailure {
        operation: action.broker_verb().to_owned(),
        stage: stage.to_owned(),
        error_chain: failure.detail.clone(),
        broker_exit_code: None,
        channel_failure: failure.channel_failure.clone(),
        rollback: rollback.as_str().to_owned(),
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
        installation_preflight: failure.diagnostics().cloned(),
    }
}

struct SystemStartupReceiptRestorer;

impl StartupReceiptRestorer for SystemStartupReceiptRestorer {
    fn restore_local_machine(&mut self) -> Result<(), ActionFailure> {
        #[cfg(windows)]
        {
            windows::run_elevated(SystemServiceAction::RestoreLegacyTrayAutostart, None)
        }
        #[cfg(not(windows))]
        {
            Err(ActionFailure::internal(
                "local-machine startup restoration is available only on Windows",
            ))
        }
    }

    fn restore_current_user(&mut self) -> Result<(), ActionFailure> {
        super::legacy_migration::restore_startup_scope(
            super::legacy_migration::StartupReceiptScope::CurrentUser,
        )
        .map_err(ActionFailure::from)
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
    MigrationBackend, MigrationVerification, migrate_from_legacy, migration_activation_actions,
    remove_legacy_after_verification,
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
