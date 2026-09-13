use super::{
    super::{windows::query, SystemServiceAction},
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
    ) -> Result<(), String> {
        run_setup(action.into(), profile)
    }
}

pub(super) fn publish_and_activate(profile: &[u8]) -> Result<(), String> {
    // Re-validate the conflicting-environment gates inside the elevated broker,
    // not only in the unelevated caller: the UAC consent window is an arbitrary
    // interval during which a conflict can appear (TOCTOU).
    if crate::machine_integration::registry_conflict_detected() {
        return Err("AppInit conflicts block machine integration changes".to_owned());
    }
    if crate::machine_integration::legacy_mactray::legacy_service_blocks_activation()? {
        return Err(
            "a legacy MacType service is still installed; migrate it before applying the profile"
                .to_owned(),
        );
    }
    crate::machine_integration::publish_profile_transaction_with(
        &mut OpenServicePublishBackend,
        profile,
    )
}

pub(in crate::machine_integration::open_service) fn run_setup(
    action: SystemServiceAction,
    profile: Option<&[u8]>,
) -> Result<(), String> {
    let verb = action
        .setup_verb()
        .ok_or_else(|| "the requested action is not a setup broker verb".to_owned())?;
    if profile.is_some() != (action == SystemServiceAction::PublishProfile) {
        return Err("only publish-profile accepts stdin bytes".to_owned());
    }
    run_setup_process(verb, profile)
}

pub(in crate::machine_integration::open_service) fn run_restore_pinned_runtime(
) -> Result<(), String> {
    run_setup_process("restore-runtime", None)
}

fn run_setup_process(verb: &str, profile: Option<&[u8]>) -> Result<(), String> {
    let setup = fixed_setup_path()?;
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
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "setup broker stdout is unavailable".to_owned())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "setup broker stderr is unavailable".to_owned())?;
    let stdout = capture_setup_output(stdout);
    let stderr = capture_setup_output(stderr);
    if let Some(bytes) = profile {
        child
            .stdin
            .take()
            .ok_or_else(|| "setup broker stdin is unavailable".to_owned())?
            .write_all(bytes)
            .map_err(|error| error.to_string())?;
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    let stdout = join_setup_output(stdout, "stdout");
    let stderr = join_setup_output(stderr, "stderr");
    if status.success() {
        Ok(())
    } else {
        Err(setup_failure_message(verb, status.code(), &stderr, &stdout))
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
        let error = setup_failure_message(
            "start",
            Some(1),
            "CreateServiceW failed with Win32 5 (Access is denied)",
            "ignored status output",
        );

        assert!(error.contains("setup broker start failed with exit code 1"));
        assert!(error.contains("CreateServiceW failed with Win32 5"));
        assert!(!error.contains("ignored status output"));
    }
}
