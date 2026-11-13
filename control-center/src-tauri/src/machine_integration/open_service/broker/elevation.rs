use super::{
    super::{
        BROKER_SWITCH, BROKER_TRANSFER_SWITCH, BrokerResultDisposition, BrokerResultMessage,
        SystemServiceAction,
        action_failure::{ActionBlocker, ActionFailure, RollbackOutcome},
        profile_transfer::{
            BrokerResultPipeServer, PROFILE_PIPE_TIMEOUT, ProfilePipeServer,
            profile_transfer_nonce_text,
        },
    },
    installed_package::service_package,
    process::{combine_broker_cleanup_error, terminate_broker_process},
};

const BROKER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5 * 60);
use mactype_service_platform::{Process, WaitOutcome};
use std::{ffi::OsStr, path::PathBuf, time::Duration};
use windows_sys::Win32::Foundation::{ERROR_CANCELLED, STILL_ACTIVE};

#[cfg(test)]
fn with_service_package_before_elevation(
    locate: impl FnOnce() -> Result<PathBuf, String>,
    elevate: impl FnOnce(PathBuf) -> Result<(), String>,
) -> Result<(), String> {
    elevate(locate()?)
}

pub(in crate::machine_integration::open_service) fn run_elevated(
    action: SystemServiceAction,
    profile_input: Option<&[u8]>,
) -> Result<(), ActionFailure> {
    let package = service_package().map_err(|failure| {
        ActionFailure::installation_preflight(failure.kind, failure.diagnostics, failure.error)
    })?;
    run_elevated_at(action, profile_input, package.control_center)
}

pub(in crate::machine_integration::open_service) fn run_elevated_at(
    action: SystemServiceAction,
    profile_input: Option<&[u8]>,
    executable: PathBuf,
) -> Result<(), ActionFailure> {
    if profile_input.is_some() != action.needs_profile_input() {
        return Err(ActionFailure::internal(
            "the elevated service action has an invalid profile payload",
        ));
    }
    let result_transfer = BrokerResultPipeServer::create()
        .map_err(|error| ActionFailure::internal_at("create-result-channel", error))?;
    let profile_transfer = profile_input
        .map(|profile| ProfilePipeServer::create_with_nonce(profile, result_transfer.token().nonce))
        .transpose()
        .map_err(|error| ActionFailure::internal_at("create-profile-channel", error))?;
    launch_elevated_broker(action, executable, result_transfer, profile_transfer)
}

fn launch_elevated_broker(
    action: SystemServiceAction,
    executable: PathBuf,
    result_transfer: BrokerResultPipeServer,
    profile_transfer: Option<ProfilePipeServer>,
) -> Result<(), ActionFailure> {
    if profile_transfer.is_some() != action.needs_profile_input() {
        return Err(ActionFailure::internal(
            "the elevated broker has invalid profile transfer state",
        ));
    }
    let parameters = format!(
        "{BROKER_SWITCH} {} {BROKER_TRANSFER_SWITCH} {} {}",
        action.broker_verb(),
        result_transfer.token().server_pid,
        profile_transfer_nonce_text(&result_transfer.token().nonce)
    );
    let process =
        Process::launch_elevated(&executable, OsStr::new(&parameters)).map_err(|error| {
            if error.raw_os_error() == Some(ERROR_CANCELLED as i32) {
                ActionFailure::blocked(ActionBlocker::AdministratorApprovalCancelled)
            } else {
                ActionFailure::internal_at(
                    "launch-elevated-broker",
                    format!(
                        "ShellExecuteExW failed with {}",
                        error.raw_os_error().unwrap_or(0)
                    ),
                )
            }
        })?;
    let broker_pid = match process.pid() {
        Ok(pid) => pid,
        Err(_) => {
            return Err(cleanup_failure(
                "could not identify the elevated service broker process",
                terminate_broker_process(&process),
            ));
        }
    };
    // The pipe servers wait on the broker's own process object, so a broker
    // that dies mid-transfer ends the wait at once.
    if let Some(server) = profile_transfer
        && let Err(error) = server.send_to(broker_pid, Some(&process), PROFILE_PIPE_TIMEOUT)
    {
        return Err(cleanup_failure(&error, terminate_broker_process(&process)));
    }
    let broker_result =
        match result_transfer.receive_from(broker_pid, Some(&process), BROKER_TIMEOUT) {
            Ok(result) => Some(result),
            Err(error) => {
                if let Some(exit_code) = finished_exit_code(&process) {
                    return Err(broker_channel_failure(Some(exit_code), &error));
                }
                let cleanup = terminate_broker_process(&process);
                let channel_failure = broker_channel_failure(None, &error);
                return Err(match cleanup {
                    Ok(_) => channel_failure,
                    Err(cleanup_error) => channel_failure
                        .append_detail(format!("; {cleanup_error}"))
                        .with_rollback(RollbackOutcome::Failed),
                });
            }
        };
    match process.wait(Some(BROKER_TIMEOUT)) {
        Ok(WaitOutcome::Signaled) => {}
        Ok(WaitOutcome::TimedOut) => {
            return Err(cleanup_failure(
                "elevated service broker timed out",
                terminate_broker_process(&process),
            ));
        }
        Ok(WaitOutcome::Abandoned) => {
            return Err(cleanup_failure(
                "waiting for the elevated service broker returned an abandoned wait",
                terminate_broker_process(&process),
            ));
        }
        Err(error) => {
            let error = format!(
                "waiting for the elevated service broker failed with {}",
                error.raw_os_error().unwrap_or(0)
            );
            return Err(cleanup_failure(&error, terminate_broker_process(&process)));
        }
    }
    let exit_code = match process.exit_code() {
        Ok(Some(code)) => code,
        // The process has signaled, so a STILL_ACTIVE code is the value it
        // really exited with, exactly as the raw query reported it.
        Ok(None) => STILL_ACTIVE as u32,
        Err(_) => {
            return Err(ActionFailure::internal_at(
                "read-broker-exit-code",
                "could not read the elevated service broker exit code",
            ));
        }
    };
    interpret_broker_exit(action.broker_verb(), exit_code, broker_result)
}

fn interpret_broker_exit(
    expected_operation: &str,
    exit_code: u32,
    broker_result: Option<BrokerResultMessage>,
) -> Result<(), ActionFailure> {
    if broker_result
        .as_ref()
        .is_some_and(|result| result.operation != expected_operation)
    {
        return Err(ActionFailure::internal_at(
            "broker-result-operation",
            "elevated service broker reported a result for another operation",
        ));
    }
    match (exit_code, broker_result) {
        (0, Some(result)) if result.disposition == BrokerResultDisposition::Success => Ok(()),
        (0, Some(result)) => Err(ActionFailure::internal_at(
            result.stage.clone(),
            format!(
                "elevated service broker reported {} after a successful exit: {}",
                result.stage, result.error_chain
            ),
        )),
        (code, Some(result)) if result.disposition != BrokerResultDisposition::Success => {
            let detail = format!(
                "{}; elevated service broker exit code {code}",
                result.error_chain
            );
            let rollback = RollbackOutcome::from_wire(&result.rollback)
                .unwrap_or(RollbackOutcome::NotApplicableOrUnavailable);
            let mut failure = if result.disposition == BrokerResultDisposition::Blocked {
                match ActionBlocker::from_stage_code(&result.stage, None) {
                    Some(blocker) => ActionFailure::blocked_with_detail(blocker, detail),
                    None => ActionFailure::internal_at(result.stage, detail),
                }
            } else {
                ActionFailure::internal_at(result.stage, detail)
            };
            failure.rollback = rollback;
            Err(failure)
        }
        (code, Some(_)) => Err(ActionFailure::internal(format!(
            "elevated service broker failed with exit code {code} after reporting success"
        ))),
        (code, None) => Err(ActionFailure::internal(format!(
            "elevated service broker failed with exit code {code}"
        ))),
    }
}

fn cleanup_failure(
    operation_error: &str,
    cleanup: Result<super::process::BrokerTermination, String>,
) -> ActionFailure {
    ActionFailure::internal(combine_broker_cleanup_error(operation_error, cleanup))
}

fn finished_exit_code(process: &Process) -> Option<u32> {
    if process.wait(Some(Duration::ZERO)).ok()? != WaitOutcome::Signaled {
        return None;
    }
    match process.exit_code().ok()? {
        Some(code) => Some(code),
        None => Some(STILL_ACTIVE as u32),
    }
}

fn broker_channel_failure(exit_code: Option<u32>, channel_error: &str) -> ActionFailure {
    let detail = exit_code.map_or_else(
        || format!("broker result channel failed: {channel_error}"),
        |code| {
            format!(
                "elevated service broker failed with exit code {code}; broker result channel failed: {channel_error}"
            )
        },
    );
    ActionFailure::internal_at("broker-result-channel", detail).with_channel_failure(channel_error)
}

#[cfg(test)]
mod tests {
    use super::{
        broker_channel_failure, interpret_broker_exit, with_service_package_before_elevation,
    };
    use crate::machine_integration::open_service::{
        BrokerResultDisposition, BrokerResultMessage,
        action_failure::{ActionBlocker, ActionFailureKind, AppInitConflictContext},
    };
    use std::{cell::Cell, path::PathBuf};

    #[test]
    fn missing_child_detail_preserves_exit_code_and_channel_failure() {
        let error = broker_channel_failure(
            Some(21),
            "the broker result pipe closed before sending a complete frame",
        );

        assert!(error.contains("exit code 21"));
        assert!(error.contains("broker result channel failed"));
        assert!(error.contains("closed before sending a complete frame"));
    }

    #[test]
    fn blocked_broker_result_uses_the_stage_code_not_the_prose() {
        let failure = interpret_broker_exit(
            "start",
            21,
            Some(BrokerResultMessage {
                operation: "start".to_owned(),
                disposition: BrokerResultDisposition::Blocked,
                stage: "blocker/appinit-service-change".to_owned(),
                error_chain: "arbitrary detail".to_owned(),
                rollback: "not-applicable-or-unavailable".to_owned(),
                final_state: String::new(),
            }),
        )
        .unwrap_err();

        assert!(matches!(
            failure.kind,
            ActionFailureKind::Blocked(ActionBlocker::AppInitConflict(
                AppInitConflictContext::ServiceChange
            ))
        ));
        assert!(failure.detail.contains("arbitrary detail"));
    }

    #[test]
    fn missing_service_package_never_invokes_the_elevation_launcher() {
        let launched = Cell::new(false);
        let error = with_service_package_before_elevation(
            || Err("control-center-installation-required: run the installer".to_owned()),
            |_executable: PathBuf| {
                launched.set(true);
                Ok(())
            },
        )
        .unwrap_err();

        assert!(error.starts_with("control-center-installation-required:"));
        assert!(!launched.get());
    }
}
