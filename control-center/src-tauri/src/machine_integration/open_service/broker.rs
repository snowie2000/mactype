mod elevation;
mod installed_package;
mod path_guard;
mod process;
mod setup;

pub(super) use elevation::{run_elevated, run_elevated_at};
pub(super) use installed_package::service_package;
#[cfg(test)]
pub(super) use installed_package::{
    current_executable_path_gate_for_test, elevated_package_preflight_for_layout,
    resolve_installed_package_for_trusted_layout, resolve_service_package_for_layouts,
    service_package_preflight_for_layouts,
};
pub(super) use path_guard::reject_reparse_ancestors;
#[cfg(test)]
pub(super) use process::{
    BrokerProcessControl, BrokerTermination, combine_broker_cleanup_error,
    terminate_broker_process_with,
};
use setup::{designate_and_hold, publish_and_activate, run_setup_typed};
pub(super) use setup::{fixed_setup_path, run_restore_pinned_runtime, run_setup};

use super::{
    BrokerResultMessage, ProfileTransferToken, SystemServiceAction,
    action_failure::{
        ActionBlocker, ActionFailure, AppInitConflictContext, LegacyServiceBlockContext,
        LegacyTrayBlockContext,
    },
    migrate_from_legacy,
    profile_transfer::{
        BrokerResultPipeWriter, KillOnCloseJob, PROFILE_PIPE_TIMEOUT,
        receive_profile_from_pipe_bounded,
    },
    remove_legacy_after_verification,
    windows::SystemMigrationBackend,
};

pub(super) fn run_privileged(
    action: SystemServiceAction,
    transfer: &ProfileTransferToken,
) -> Result<(), ActionFailure> {
    let result_writer = BrokerResultPipeWriter::connect(transfer, PROFILE_PIPE_TIMEOUT)
        .map_err(|error| ActionFailure::internal_at("connect-result-channel", error))?;
    let result = (|| {
        KillOnCloseJob::new()
            .map_err(|error| ActionFailure::internal_at("create-kill-on-close-job", error))?
            .arm_current_process()
            .map_err(|error| ActionFailure::internal_at("arm-kill-on-close-job", error))?;
        if let Err(failure) = installed_package::current_service_package() {
            let failure = ActionFailure::installation_preflight(
                failure.kind,
                failure.diagnostics,
                failure.error,
            );
            super::record_action_failure(action, None, &failure);
            return Err(failure);
        }
        if action.needs_profile_input() {
            let profile = receive_required_profile_bounded(transfer)?;
            run_profile_action(action, &profile)
        } else {
            match action {
                SystemServiceAction::DisableLegacyTrayAutostart => {
                    crate::machine_integration::legacy_migration::disable_startup_scope(
                        crate::machine_integration::legacy_migration::StartupReceiptScope::LocalMachine,
                    )
                    .map_err(|error| {
                        ActionFailure::internal_at("disable-legacy-tray-autostart", error)
                    })
                }
                SystemServiceAction::RestoreLegacyTrayAutostart => {
                    crate::machine_integration::legacy_migration::restore_startup_scope(
                        crate::machine_integration::legacy_migration::StartupReceiptScope::LocalMachine,
                    )
                    .map_err(|error| {
                        ActionFailure::internal_at("restore-legacy-tray-autostart", error)
                    })
                }
                SystemServiceAction::Install | SystemServiceAction::Start => {
                    refuse_conflicting_environment_for_activation()?;
                    run_setup_typed(action, None)
                }
                _ => run_setup_typed(action, None),
            }
        }
    })();
    let message = match &result {
        Ok(()) => BrokerResultMessage::success(action.broker_verb()),
        Err(failure) => BrokerResultMessage::from_failure(action.broker_verb(), failure),
    };
    let sent = result_writer.send(&message, PROFILE_PIPE_TIMEOUT);
    match (result, sent) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(failure), Ok(())) => Err(failure),
        (Ok(()), Err(channel)) => Err(ActionFailure::internal_at(
            "report-broker-result",
            format!("operation completed but reporting the broker result failed: {channel}"),
        )
        .with_channel_failure(channel)),
        (Err(failure), Err(channel)) => {
            let detail = format!("; reporting the broker result failed: {channel}");
            Err(failure.append_detail(detail).with_channel_failure(channel))
        }
    }
}

// The unelevated caller already gated these actions, but the UAC consent window
// is an arbitrary interval during which an AppInit entry, a legacy tray process,
// or a legacy SCM service can appear (TOCTOU). Re-validate the conflicting-
// environment gates inside the elevated broker before activating the new service.
// The migration path drives its own run_setup steps directly and never reaches
// this arm, so it is unaffected.
fn refuse_conflicting_environment_for_activation() -> Result<(), ActionFailure> {
    use crate::machine_integration::{legacy_mactray, registry_conflict_detected};
    if registry_conflict_detected() {
        return Err(ActionFailure::blocked(ActionBlocker::AppInitConflict(
            AppInitConflictContext::ServiceChange,
        )));
    }
    if legacy_mactray::tray_status().blocks_machine_change() {
        return Err(ActionFailure::blocked(ActionBlocker::LegacyTrayModeBlocks(
            LegacyTrayBlockContext::ServiceChange,
        )));
    }
    if legacy_mactray::legacy_service_blocks_activation()
        .map_err(|error| ActionFailure::internal_at("observe-legacy-service", error))?
    {
        return Err(ActionFailure::blocked(
            ActionBlocker::LegacyServiceStillInstalled(LegacyServiceBlockContext::StartNewService),
        ));
    }
    Ok(())
}

fn receive_required_profile_bounded(
    transfer: &ProfileTransferToken,
) -> Result<Vec<u8>, ActionFailure> {
    receive_profile_from_pipe_bounded(transfer, PROFILE_PIPE_TIMEOUT)
        .map_err(|error| ActionFailure::internal_at("receive-profile", error))
}

fn run_profile_action(action: SystemServiceAction, profile: &[u8]) -> Result<(), ActionFailure> {
    match action {
        SystemServiceAction::PublishProfile => publish_and_activate(profile),
        SystemServiceAction::DesignateProfile => designate_and_hold(profile),
        SystemServiceAction::MigrateFromLegacy => {
            migrate_from_legacy(&mut SystemMigrationBackend::default(), profile)
        }
        SystemServiceAction::RemoveLegacy => {
            remove_legacy_after_verification(&mut SystemMigrationBackend::default(), profile)
        }
        _ => Err(ActionFailure::internal(
            "the privileged action does not accept profile input",
        )),
    }
}
