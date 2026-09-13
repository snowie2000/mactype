use super::LegacyTrayProcessState;
use mactype_service_contract::StructuredServiceError;
use std::path::PathBuf;

#[cfg(windows)]
use std::path::Path;

#[cfg(windows)]
use mactype_service_platform::{
    interactive_processes, is_reparse_point, process_session_id as platform_process_session_id,
    Process, ProcessAccess,
};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct LegacyTrayProcessIdentity {
    pub(super) creation_time: u64,
    pub(super) session_id: u32,
    pub(super) path: PathBuf,
    pub(super) trusted_path: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct LegacyTrayProcessObservation {
    pub(super) image_name: String,
    pub(super) pid: u32,
    pub(super) session_id: u32,
    pub(super) identity: Result<LegacyTrayProcessIdentity, StructuredServiceError>,
}

pub(crate) fn observe_tray_process() -> LegacyTrayProcessState {
    #[cfg(windows)]
    {
        observe_windows_tray_process()
    }
    #[cfg(not(windows))]
    {
        LegacyTrayProcessState::Absent
    }
}

#[cfg(windows)]
fn observe_windows_tray_process() -> LegacyTrayProcessState {
    let current_session_id = match process_session_id(std::process::id()) {
        Ok(session_id) => session_id,
        Err(error) => return LegacyTrayProcessState::Unknown { error },
    };
    let entries = match enumerate_processes() {
        Ok(entries) => entries,
        Err(error) => return LegacyTrayProcessState::Unknown { error },
    };
    let observations = entries
        .into_iter()
        .filter(|entry| entry.image_name.eq_ignore_ascii_case("MacTray.exe"))
        .map(|entry| LegacyTrayProcessObservation {
            image_name: entry.image_name,
            pid: entry.pid,
            session_id: entry.session_id,
            identity: if entry.session_id == 0 {
                Err(service_error(
                    "legacy-tray-process-service-session",
                    "the session-zero MacTray process belongs to the legacy service",
                    None,
                ))
            } else {
                inspect_process(entry.pid)
            },
        })
        .collect();
    classify_tray_process_inventory(current_session_id, observations)
}

#[cfg(windows)]
struct EnumeratedProcess {
    image_name: String,
    pid: u32,
    session_id: u32,
}

#[cfg(windows)]
fn enumerate_processes() -> Result<Vec<EnumeratedProcess>, StructuredServiceError> {
    interactive_processes()
        .map_err(|io| {
            if io.kind() == std::io::ErrorKind::InvalidData {
                service_error(
                    "legacy-tray-process-enumeration-invalid",
                    "the running process inventory returned no process buffer",
                    None,
                )
            } else {
                io_error(
                    "legacy-tray-process-enumeration-unavailable",
                    "the running process inventory could not be enumerated",
                    io,
                )
            }
        })?
        .into_iter()
        .map(|entry| {
            let image_name = entry.name.ok_or_else(|| {
                service_error(
                    "legacy-tray-process-name-invalid",
                    "an enumerated process image name is not bounded",
                    None,
                )
            })?;
            Ok(EnumeratedProcess {
                image_name,
                pid: entry.pid,
                session_id: entry.session_id,
            })
        })
        .collect()
}

#[cfg(windows)]
pub(super) fn open_process(pid: u32) -> Result<Process, StructuredServiceError> {
    if pid == 0 {
        return Err(service_error(
            "legacy-tray-process-pid-invalid",
            "the MacTray process ID is zero",
            None,
        ));
    }
    Process::open(pid, ProcessAccess::QueryLimitedAndSynchronize).map_err(|io| {
        io_error(
            "legacy-tray-process-inaccessible",
            "the MacTray process could not be opened for identity verification",
            io,
        )
    })
}

#[cfg(windows)]
pub(super) fn process_creation_time(process: &Process) -> Result<u64, StructuredServiceError> {
    let value = process.creation_time().map_err(|io| {
        io_error(
            "legacy-tray-process-creation-time-unavailable",
            "the MacTray process creation time could not be read",
            io,
        )
    })?;
    if value == 0 {
        return Err(service_error(
            "legacy-tray-process-creation-time-invalid",
            "the MacTray process creation time is zero",
            None,
        ));
    }
    Ok(value)
}

#[cfg(windows)]
pub(super) fn process_image_path(process: &Process) -> Result<PathBuf, StructuredServiceError> {
    process.image_path_checked().map_err(|io| {
        if io.kind() == std::io::ErrorKind::InvalidData {
            service_error(
                "legacy-tray-process-path-invalid",
                "the MacTray process image path is invalid",
                None,
            )
        } else {
            io_error(
                "legacy-tray-process-path-unavailable",
                "the MacTray process image path could not be read",
                io,
            )
        }
    })
}

#[cfg(windows)]
fn inspect_process(pid: u32) -> Result<LegacyTrayProcessIdentity, StructuredServiceError> {
    let process = open_process(pid)?;
    let actual_pid = process.pid().map_err(|io| {
        io_error(
            "legacy-tray-process-pid-unavailable",
            "the opened MacTray process ID could not be read",
            io,
        )
    })?;
    if actual_pid != pid {
        return Err(service_error(
            "legacy-tray-process-pid-changed",
            "the opened MacTray process ID does not match the observed process",
            None,
        ));
    }
    let creation_time = process_creation_time(&process)?;
    let session_id = process_session_id(actual_pid)?;
    let path = process_image_path(&process)?;
    Ok(LegacyTrayProcessIdentity {
        creation_time,
        session_id,
        trusted_path: trusted_mactray_process_path(&path),
        path,
    })
}

#[cfg(windows)]
pub(super) fn process_session_id(pid: u32) -> Result<u32, StructuredServiceError> {
    platform_process_session_id(pid).map_err(|io| {
        io_error(
            "legacy-tray-process-session-unavailable",
            "the process session could not be read",
            io,
        )
    })
}

#[cfg(windows)]
pub(super) fn trusted_mactray_process_path(path: &Path) -> bool {
    let Some(expected) = super::windows::expected_mactray_path() else {
        return false;
    };
    if path_has_reparse_component(path) || path_has_reparse_component(&expected) {
        return false;
    }
    let Ok(actual) = std::fs::canonicalize(path) else {
        return false;
    };
    let Ok(expected) = std::fs::canonicalize(expected) else {
        return false;
    };
    same_windows_path(&actual, &expected)
}

#[cfg(windows)]
fn path_has_reparse_component(path: &Path) -> bool {
    path.ancestors()
        .any(|component| is_reparse_point(component).unwrap_or(true))
}

#[cfg(windows)]
pub(super) fn same_windows_path(left: &Path, right: &Path) -> bool {
    normalize_windows_path(left).eq_ignore_ascii_case(&normalize_windows_path(right))
}

#[cfg(windows)]
fn normalize_windows_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    value
        .strip_prefix(r"\\?\")
        .unwrap_or(value.as_ref())
        .replace('/', "\\")
}

#[cfg(windows)]
fn io_error(code: &str, message: &str, io: std::io::Error) -> StructuredServiceError {
    service_error(
        code,
        message,
        io.raw_os_error()
            .and_then(|value| u32::try_from(value).ok()),
    )
}

#[cfg(windows)]
fn service_error(code: &str, message: &str, win32_error: Option<u32>) -> StructuredServiceError {
    StructuredServiceError {
        code: code.to_owned(),
        message: message.to_owned(),
        win32_error,
    }
}

pub(super) fn classify_tray_process_inventory(
    current_session_id: u32,
    observations: Vec<LegacyTrayProcessObservation>,
) -> LegacyTrayProcessState {
    let mut candidates = observations
        .into_iter()
        .filter(|entry| {
            entry.image_name.eq_ignore_ascii_case("MacTray.exe") && entry.session_id != 0
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return LegacyTrayProcessState::Absent;
    }
    if candidates.len() != 1 {
        return unknown(
            "legacy-tray-process-multiple",
            "multiple interactive MacTray processes were observed",
            None,
        );
    }
    let Some(entry) = candidates.pop() else {
        return LegacyTrayProcessState::Absent;
    };
    let identity = match entry.identity {
        Ok(identity) => identity,
        Err(error) => return LegacyTrayProcessState::Unknown { error },
    };
    if entry.pid == 0 || identity.creation_time == 0 || identity.session_id != entry.session_id {
        return unknown(
            "legacy-tray-process-identity-changed",
            "the MacTray process identity changed during inspection",
            None,
        );
    }
    if !identity.trusted_path {
        return LegacyTrayProcessState::UntrustedSameName {
            session_id: Some(identity.session_id),
            path: Some(identity.path),
        };
    }
    if identity.session_id == current_session_id {
        LegacyTrayProcessState::TrustedCurrentSession {
            pid: entry.pid,
            creation_time: identity.creation_time,
            path: identity.path,
        }
    } else {
        LegacyTrayProcessState::TrustedOtherSession {
            session_id: identity.session_id,
            path: identity.path,
        }
    }
}

fn unknown(code: &str, message: &str, win32_error: Option<u32>) -> LegacyTrayProcessState {
    LegacyTrayProcessState::Unknown {
        error: StructuredServiceError {
            code: code.to_owned(),
            message: message.to_owned(),
            win32_error,
        },
    }
}
