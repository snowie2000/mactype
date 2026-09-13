use super::{
    super::{
        profile_transfer::{
            profile_transfer_nonce_text, BrokerResultPipeServer, ProfilePipeServer,
            PROFILE_PIPE_TIMEOUT,
        },
        BrokerResultDisposition, SystemServiceAction, BROKER_SWITCH, BROKER_TRANSFER_SWITCH,
    },
    installed_package::service_package,
    process::{combine_broker_cleanup_error, terminate_broker_process},
};

const BROKER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5 * 60);
use mactype_service_platform::{Process, WaitOutcome};
use std::{ffi::OsStr, path::PathBuf, time::Duration};
use windows_sys::Win32::Foundation::{ERROR_CANCELLED, STILL_ACTIVE};

fn with_service_package_before_elevation(
    locate: impl FnOnce() -> Result<PathBuf, String>,
    elevate: impl FnOnce(PathBuf) -> Result<(), String>,
) -> Result<(), String> {
    elevate(locate()?)
}

pub(in crate::machine_integration::open_service) fn run_elevated(
    action: SystemServiceAction,
    profile_input: Option<&[u8]>,
) -> Result<(), String> {
    with_service_package_before_elevation(
        || {
            service_package()
                .map(|package| package.control_center)
                .map_err(|failure| failure.error)
        },
        |executable| run_elevated_at(action, profile_input, executable),
    )
}

pub(in crate::machine_integration::open_service) fn run_elevated_at(
    action: SystemServiceAction,
    profile_input: Option<&[u8]>,
    executable: PathBuf,
) -> Result<(), String> {
    if profile_input.is_some() != action.needs_profile_input() {
        return Err("the elevated service action has an invalid profile payload".to_owned());
    }
    let result_transfer = BrokerResultPipeServer::create()?;
    let profile_transfer = profile_input
        .map(|profile| ProfilePipeServer::create_with_nonce(profile, result_transfer.token().nonce))
        .transpose()?;
    launch_elevated_broker(action, executable, result_transfer, profile_transfer)
}

fn launch_elevated_broker(
    action: SystemServiceAction,
    executable: PathBuf,
    result_transfer: BrokerResultPipeServer,
    profile_transfer: Option<ProfilePipeServer>,
) -> Result<(), String> {
    if profile_transfer.is_some() != action.needs_profile_input() {
        return Err("the elevated broker has invalid profile transfer state".to_owned());
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
                "administrator approval was cancelled".to_owned()
            } else {
                format!(
                    "ShellExecuteExW failed with {}",
                    error.raw_os_error().unwrap_or(0)
                )
            }
        })?;
    let broker_pid = match process.pid() {
        Ok(pid) => pid,
        Err(_) => {
            return Err(combine_broker_cleanup_error(
                "could not identify the elevated service broker process",
                terminate_broker_process(&process),
            ));
        }
    };
    // The pipe servers wait on the broker's own process object, so a broker
    // that dies mid-transfer ends the wait at once.
    if let Some(server) = profile_transfer {
        if let Err(error) = server.send_to(broker_pid, Some(&process), PROFILE_PIPE_TIMEOUT) {
            return Err(combine_broker_cleanup_error(
                &error,
                terminate_broker_process(&process),
            ));
        }
    }
    let broker_result =
        match result_transfer.receive_from(broker_pid, Some(&process), BROKER_TIMEOUT) {
            Ok(result) => Some(result),
            Err(error) => {
                if let Some(exit_code) = finished_exit_code(&process) {
                    return Err(broker_channel_failure(Some(exit_code), &error));
                }
                let cleanup = terminate_broker_process(&process);
                return Err(combine_broker_cleanup_error(
                    &broker_channel_failure(None, &error),
                    cleanup,
                ));
            }
        };
    match process.wait(Some(BROKER_TIMEOUT)) {
        Ok(WaitOutcome::Signaled) => {}
        Ok(WaitOutcome::TimedOut) => {
            return Err(combine_broker_cleanup_error(
                "elevated service broker timed out",
                terminate_broker_process(&process),
            ));
        }
        Ok(WaitOutcome::Abandoned) => {
            return Err(combine_broker_cleanup_error(
                "waiting for the elevated service broker returned an abandoned wait",
                terminate_broker_process(&process),
            ));
        }
        Err(error) => {
            let error = format!(
                "waiting for the elevated service broker failed with {}",
                error.raw_os_error().unwrap_or(0)
            );
            return Err(combine_broker_cleanup_error(
                &error,
                terminate_broker_process(&process),
            ));
        }
    }
    let exit_code = match process.exit_code() {
        Ok(Some(code)) => code,
        // The process has signaled, so a STILL_ACTIVE code is the value it
        // really exited with, exactly as the raw query reported it.
        Ok(None) => STILL_ACTIVE as u32,
        Err(_) => return Err("could not read the elevated service broker exit code".to_owned()),
    };
    match (exit_code, broker_result) {
        (0, Some(result)) if result.disposition == BrokerResultDisposition::Success => Ok(()),
        (0, Some(result)) => Err(format!(
            "elevated service broker reported {} after a successful exit: {}",
            result.stage, result.error_chain
        )),
        (code, Some(result)) if result.disposition != BrokerResultDisposition::Success => {
            Err(format!(
                "{}; elevated service broker exit code {code}",
                result.error_chain
            ))
        }
        (code, Some(_)) => Err(format!(
            "elevated service broker failed with exit code {code} after reporting success"
        )),
        (code, None) => Err(format!(
            "elevated service broker failed with exit code {code}"
        )),
    }
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

fn broker_channel_failure(exit_code: Option<u32>, channel_error: &str) -> String {
    exit_code.map_or_else(
        || format!("broker result channel failed: {channel_error}"),
        |code| {
            format!(
                "elevated service broker failed with exit code {code}; broker result channel failed: {channel_error}"
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{broker_channel_failure, with_service_package_before_elevation};
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
