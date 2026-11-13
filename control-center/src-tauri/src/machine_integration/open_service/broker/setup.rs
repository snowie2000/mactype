use super::{
    super::{
        SystemServiceAction,
        action_failure::{
            ActionBlocker, ActionFailure, AppInitConflictContext, LegacyServiceBlockContext,
            RollbackOutcome,
        },
        windows::query,
    },
    installed_package::current_service_package,
};
use crate::service_contract::SystemServiceStatus;
use std::{
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    thread,
};

struct OpenServicePublishBackend;

impl crate::machine_integration::MachineBackend for OpenServicePublishBackend {
    fn new_service_status(&mut self) -> SystemServiceStatus {
        query()
    }

    fn legacy_tray_status(&mut self) -> crate::machine_integration::LegacyTrayStatus {
        crate::machine_integration::legacy_mactray::tray_status()
    }

    fn appinit_conflict(&mut self) -> Result<bool, String> {
        Ok(crate::machine_integration::registry_conflict_detected())
    }

    fn legacy_service_blocks_activation(&mut self) -> Result<bool, String> {
        crate::machine_integration::legacy_mactray::legacy_service_blocks_activation()
    }

    fn execute(
        &mut self,
        action: crate::machine_integration::MachineAction,
        profile: Option<&[u8]>,
    ) -> Result<(), ActionFailure> {
        let service_action: SystemServiceAction = action.into();
        run_setup_typed(service_action, profile)
    }
}

pub(super) fn publish_and_activate(profile: &[u8]) -> Result<(), ActionFailure> {
    // Re-validate the conflicting-environment gates inside the elevated broker,
    // not only in the unelevated caller: the UAC consent window is an arbitrary
    // interval during which a conflict can appear (TOCTOU).
    if crate::machine_integration::registry_conflict_detected() {
        return Err(ActionFailure::blocked(ActionBlocker::AppInitConflict(
            AppInitConflictContext::MachineIntegrationChanges,
        )));
    }
    if crate::machine_integration::legacy_mactray::legacy_service_blocks_activation()
        .map_err(ActionFailure::from)?
    {
        return Err(ActionFailure::blocked(
            ActionBlocker::LegacyServiceStillInstalled(LegacyServiceBlockContext::ApplyProfile),
        ));
    }
    crate::machine_integration::publish_profile_transaction_with(
        &mut OpenServicePublishBackend,
        profile,
    )
}

pub(super) fn designate_and_hold(profile: &[u8]) -> Result<(), ActionFailure> {
    if crate::machine_integration::registry_conflict_detected() {
        return Err(ActionFailure::blocked(ActionBlocker::AppInitConflict(
            AppInitConflictContext::MachineIntegrationChanges,
        )));
    }
    crate::machine_integration::designate_profile_transaction_with(
        &mut OpenServicePublishBackend,
        profile,
    )
}

pub(in crate::machine_integration::open_service) fn run_setup(
    action: SystemServiceAction,
    profile: Option<&[u8]>,
) -> Result<(), String> {
    run_setup_typed(action, profile).map_err(|failure| failure.to_string())
}

pub(super) fn run_setup_typed(
    action: SystemServiceAction,
    profile: Option<&[u8]>,
) -> Result<(), ActionFailure> {
    let verb = action.setup_verb().ok_or_else(|| {
        ActionFailure::internal("the requested action is not a setup broker verb")
    })?;
    if profile.is_some() != (action == SystemServiceAction::PublishProfile) {
        return Err(ActionFailure::internal(
            "only publish-profile accepts stdin bytes",
        ));
    }
    run_setup_process(verb, profile)
}

pub(in crate::machine_integration::open_service) fn run_restore_pinned_runtime()
-> Result<(), String> {
    run_setup_process("restore-runtime", None).map_err(|failure| failure.to_string())
}

fn run_setup_process(verb: &str, profile: Option<&[u8]>) -> Result<(), ActionFailure> {
    let setup = fixed_setup_path().map_err(|error| ActionFailure::internal_at(verb, error))?;
    let mut command = Command::new(setup);
    command
        .arg(verb)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if profile.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    let mut child = command
        .spawn()
        .map_err(|error| ActionFailure::internal_at(verb, error.to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ActionFailure::internal_at(verb, "setup broker stdout is unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ActionFailure::internal_at(verb, "setup broker stderr is unavailable"))?;
    let stdout = capture_setup_output(stdout);
    let stderr = capture_setup_output(stderr);
    if let Some(bytes) = profile {
        child
            .stdin
            .take()
            .ok_or_else(|| ActionFailure::internal_at(verb, "setup broker stdin is unavailable"))?
            .write_all(bytes)
            .map_err(|error| ActionFailure::internal_at(verb, error.to_string()))?;
    }
    let status = child
        .wait()
        .map_err(|error| ActionFailure::internal_at(verb, error.to_string()))?;
    let stdout = join_setup_output(stdout, "stdout");
    let stderr = join_setup_output(stderr, "stderr");
    if status.success() {
        Ok(())
    } else {
        Err(setup_failure(verb, status.code(), &stderr, &stdout))
    }
}

const MAX_SETUP_OUTPUT_BYTES: usize = 16 * 1024;

fn capture_setup_output(
    mut reader: impl Read + Send + 'static,
) -> thread::JoinHandle<Result<String, String>> {
    thread::spawn(move || {
        let mut captured = Vec::with_capacity(MAX_SETUP_OUTPUT_BYTES);
        let mut buffer = [0_u8; 4096];
        let mut truncated = false;
        loop {
            let read = reader
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            let available = MAX_SETUP_OUTPUT_BYTES.saturating_sub(captured.len());
            let kept = read.min(available);
            captured.extend_from_slice(&buffer[..kept]);
            truncated |= kept < read;
        }
        let mut text = String::from_utf8_lossy(&captured).trim().to_owned();
        if truncated {
            text.push_str(" [truncated]");
        }
        Ok(text)
    })
}

fn join_setup_output(capture: thread::JoinHandle<Result<String, String>>, stream: &str) -> String {
    match capture.join() {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => format!("<{stream} capture failed: {error}>"),
        Err(_) => format!("<{stream} capture thread panicked>"),
    }
}

const SETUP_ROLLBACK_FAILURE_EXIT_CODE: i32 = 3;

fn setup_failure(verb: &str, exit_code: Option<i32>, stderr: &str, stdout: &str) -> ActionFailure {
    let message = setup_failure_message(verb, exit_code, stderr, stdout);
    let failure = ActionFailure::internal_at(verb, message);
    if exit_code == Some(SETUP_ROLLBACK_FAILURE_EXIT_CODE) {
        failure.with_rollback(RollbackOutcome::Failed)
    } else {
        failure
    }
}

fn setup_failure_message(verb: &str, exit_code: Option<i32>, stderr: &str, stdout: &str) -> String {
    let status = exit_code.map_or_else(
        || "without an exit code".to_owned(),
        |code| format!("with exit code {code}"),
    );
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    if detail.is_empty() {
        format!("setup broker {verb} failed {status} without diagnostic output")
    } else {
        format!("setup broker {verb} failed {status}: {detail}")
    }
}

pub(in crate::machine_integration::open_service) fn fixed_setup_path() -> Result<PathBuf, String> {
    current_service_package()
        .map(|package| package.setup_broker)
        .map_err(|failure| failure.error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_failure_preserves_the_bounded_child_error() {
        let failure = setup_failure(
            "start",
            Some(1),
            "CreateServiceW failed with Win32 5 (Access is denied)",
            "ignored status output",
        );

        assert!(
            failure
                .detail
                .contains("setup broker start failed with exit code 1")
        );
        assert!(
            failure
                .detail
                .contains("CreateServiceW failed with Win32 5")
        );
        assert!(!failure.detail.contains("ignored status output"));
        assert_eq!(
            failure.rollback,
            RollbackOutcome::NotApplicableOrUnavailable
        );
    }

    #[test]
    fn setup_exit_code_three_marks_operation_metadata_rollback_failed() {
        let failure = setup_failure(
            "upgrade",
            Some(SETUP_ROLLBACK_FAILURE_EXIT_CODE),
            "runtime activation failed (operation); pointer restoration failed: restoration",
            "",
        );

        assert_eq!(
            failure.detail,
            "setup broker upgrade failed with exit code 3: runtime activation failed (operation); pointer restoration failed: restoration"
        );
        assert_eq!(failure.stage.as_deref(), Some("upgrade"));
        let (stage, rollback) =
            crate::machine_integration::open_service::operation_failure_metadata(
                SystemServiceAction::Upgrade,
                &failure,
            );
        assert_eq!(stage, "upgrade");
        assert_eq!(rollback.as_str(), "failed");
    }
}
