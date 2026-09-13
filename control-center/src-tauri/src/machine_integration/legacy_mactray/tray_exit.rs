use super::LegacyTrayProcessState;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyTrayExitRequest {
    pub(crate) pid: u32,
    #[serde(deserialize_with = "super::model::decimal_u64::deserialize")]
    pub(crate) creation_time: u64,
    pub(crate) path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LegacyTrayExitOutcome {
    Exited,
    TimedOut,
    ProtocolUnavailable,
}

pub(super) trait LegacyTrayExitBackend {
    fn observe_process(&mut self) -> LegacyTrayProcessState;
    fn request_official_exit(
        &mut self,
        expected: &LegacyTrayExitRequest,
    ) -> Result<LegacyTrayExitOutcome, String>;
}

pub(super) fn request_tray_exit_with(
    backend: &mut impl LegacyTrayExitBackend,
    expected: &LegacyTrayExitRequest,
) -> Result<(), String> {
    require_exact_identity(backend.observe_process(), expected)?;
    require_exact_identity(backend.observe_process(), expected)?;
    match backend.request_official_exit(expected)? {
        LegacyTrayExitOutcome::Exited => {}
        LegacyTrayExitOutcome::TimedOut => {
            return Err("the graceful MacTray exit request timed out".to_owned());
        }
        LegacyTrayExitOutcome::ProtocolUnavailable => {
            return Err("the official MacTray exit protocol is unavailable".to_owned());
        }
    }
    if backend.observe_process() != LegacyTrayProcessState::Absent {
        return Err("MacTray remained present after the graceful exit request".to_owned());
    }
    Ok(())
}

fn require_exact_identity(
    observed: LegacyTrayProcessState,
    expected: &LegacyTrayExitRequest,
) -> Result<(), String> {
    match observed {
        LegacyTrayProcessState::TrustedCurrentSession {
            pid,
            creation_time,
            path,
        } if pid == expected.pid
            && creation_time == expected.creation_time
            && paths_match(&path, &expected.path) =>
        {
            Ok(())
        }
        _ => Err("the observed MacTray process identity changed before graceful exit".to_owned()),
    }
}

#[cfg(windows)]
fn paths_match(left: &std::path::Path, right: &std::path::Path) -> bool {
    super::tray_process::same_windows_path(left, right)
}

#[cfg(not(windows))]
fn paths_match(left: &std::path::Path, right: &std::path::Path) -> bool {
    left == right
}

struct SystemTrayExitBackend;

impl LegacyTrayExitBackend for SystemTrayExitBackend {
    fn observe_process(&mut self) -> LegacyTrayProcessState {
        super::observe_tray_process()
    }

    fn request_official_exit(
        &mut self,
        expected: &LegacyTrayExitRequest,
    ) -> Result<LegacyTrayExitOutcome, String> {
        request_official_exit(expected)
    }
}

pub(crate) fn request_tray_exit(expected: &LegacyTrayExitRequest) -> Result<(), String> {
    request_tray_exit_with(&mut SystemTrayExitBackend, expected)
}

pub(super) fn official_exit_available(process: &LegacyTrayProcessState) -> bool {
    let LegacyTrayProcessState::TrustedCurrentSession { pid, .. } = process else {
        return false;
    };
    owned_windows(*pid).is_ok_and(|windows| !windows.is_empty())
}

#[cfg(not(windows))]
fn request_official_exit(
    _expected: &LegacyTrayExitRequest,
) -> Result<LegacyTrayExitOutcome, String> {
    Ok(LegacyTrayExitOutcome::ProtocolUnavailable)
}

#[cfg(not(windows))]
fn owned_windows(_pid: u32) -> Result<Vec<isize>, String> {
    Ok(Vec::new())
}

#[cfg(windows)]
fn request_official_exit(
    expected: &LegacyTrayExitRequest,
) -> Result<LegacyTrayExitOutcome, String> {
    use super::tray_process::{
        open_process, process_creation_time, process_image_path, process_session_id,
        trusted_mactray_process_path,
    };
    use mactype_service_platform::{register_window_message, send_notify_message, WaitOutcome};
    use std::time::Duration;

    let process = open_process(expected.pid).map_err(format_service_error)?;
    let actual_pid = process.pid().unwrap_or(0);
    if actual_pid != expected.pid
        || process_creation_time(&process).map_err(format_service_error)? != expected.creation_time
        || process_session_id(actual_pid).map_err(format_service_error)?
            != process_session_id(std::process::id()).map_err(format_service_error)?
    {
        return Err("the opened MacTray process identity changed before graceful exit".to_owned());
    }
    let path = process_image_path(&process).map_err(format_service_error)?;
    if !trusted_mactray_process_path(&path) || !paths_match(&path, &expected.path) {
        return Err("the opened MacTray process path is not the trusted installation".to_owned());
    }

    let windows = owned_windows(expected.pid)?;
    if windows.is_empty() {
        return Ok(LegacyTrayExitOutcome::ProtocolUnavailable);
    }
    let message = register_window_message("MacType_Exit_Notify")
        .map_err(|io| format!("RegisterWindowMessageW failed{}", format_win32_suffix(&io)))?;
    for window in windows {
        send_notify_message(window, message)
            .map_err(|io| format!("SendNotifyMessageW failed{}", format_win32_suffix(&io)))?;
    }
    match process.wait(Some(Duration::from_secs(5))) {
        Ok(WaitOutcome::Signaled) => Ok(LegacyTrayExitOutcome::Exited),
        Ok(WaitOutcome::TimedOut) => Ok(LegacyTrayExitOutcome::TimedOut),
        Ok(WaitOutcome::Abandoned) => {
            Err("waiting for MacTray exit returned an abandoned wait".to_owned())
        }
        Err(io) => Err(format!(
            "waiting for MacTray exit failed{}",
            format_win32_suffix(&io)
        )),
    }
}

#[cfg(windows)]
fn owned_windows(pid: u32) -> Result<Vec<mactype_service_platform::WindowHandle>, String> {
    mactype_service_platform::top_level_windows(512)
        .map(|windows| {
            windows
                .into_iter()
                .filter(|entry| entry.process_id == pid)
                .map(|entry| entry.window)
                .collect()
        })
        .map_err(|_| "the MacTray window inventory could not be enumerated".to_owned())
}

#[cfg(windows)]
fn format_win32_suffix(error: &std::io::Error) -> String {
    error
        .raw_os_error()
        .map(|win32| format!(" with {win32}"))
        .unwrap_or_else(|| format!(": {error}"))
}

#[cfg(windows)]
fn format_service_error(error: mactype_service_contract::StructuredServiceError) -> String {
    match error.win32_error {
        Some(win32) => format!("{}: {} ({win32})", error.code, error.message),
        None => format!("{}: {}", error.code, error.message),
    }
}
