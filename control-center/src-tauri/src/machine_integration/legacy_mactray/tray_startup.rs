use super::{LegacyTrayStartupEntry, LegacyTrayStartupSource, LegacyTrayStartupState};
use mactype_service_contract::StructuredServiceError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub(crate) enum LegacyTrayStartupLocator {
    Registry {
        hive: String,
        view: u32,
        subkey: String,
        value_name: String,
        value_type: u32,
    },
    File {
        startup_file_path: PathBuf,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct LegacyTrayStartupArtifact {
    pub(crate) entry: LegacyTrayStartupEntry,
    pub(crate) locator: LegacyTrayStartupLocator,
    pub(crate) raw_bytes: Vec<u8>,
    pub(crate) normalized_target_path: PathBuf,
    pub(crate) user_sid: String,
    pub(crate) recorded_at: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LegacyTrayStartupScope {
    CurrentUser,
    LocalMachine,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "value")]
pub(super) enum LegacyTrayStartupObservation {
    Owned(LegacyTrayStartupArtifact),
    Untrusted(LegacyTrayStartupEntry),
    Unknown(StructuredServiceError),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "value")]
pub(super) enum StartupTargetClassification {
    Owned(PathBuf),
    Untrusted,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum StartupRestoreAction {
    Noop,
    Restore,
}

pub(super) fn classify_startup_command(
    command: &str,
    expected_path: &Path,
) -> StartupTargetClassification {
    let Some(target) = exactly_quoted_target(command) else {
        return StartupTargetClassification::Untrusted;
    };
    let target = PathBuf::from(target);
    if same_windows_path(&target, expected_path) {
        StartupTargetClassification::Owned(expected_path.to_path_buf())
    } else {
        StartupTargetClassification::Untrusted
    }
}

pub(super) fn is_legacy_tray_startup_candidate(display_name: &str, command: &str) -> bool {
    let normalized_name = display_name
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    normalized_name.contains("mactray")
        || normalized_name.contains("mactypetray")
        || command.to_ascii_lowercase().contains("mactray.exe")
}

pub(super) fn startup_source_requires_current_user_sid(source: LegacyTrayStartupSource) -> bool {
    matches!(
        source,
        LegacyTrayStartupSource::CurrentUserRun32
            | LegacyTrayStartupSource::CurrentUserRun64
            | LegacyTrayStartupSource::CurrentUserStartup
    )
}

pub(super) fn classify_startup_inventory(
    observations: Vec<LegacyTrayStartupObservation>,
) -> LegacyTrayStartupState {
    let mut owned = Vec::new();
    let mut untrusted = Vec::new();
    let mut unknown = None;
    for observation in observations {
        match observation {
            LegacyTrayStartupObservation::Owned(artifact) => owned.push(artifact.entry),
            LegacyTrayStartupObservation::Untrusted(entry) => untrusted.push(entry),
            LegacyTrayStartupObservation::Unknown(error) => {
                if unknown.is_none() {
                    unknown = Some(error);
                }
            }
        }
    }
    if let Some(error) = unknown {
        LegacyTrayStartupState::Unknown { error }
    } else if !untrusted.is_empty() {
        LegacyTrayStartupState::Untrusted { entries: untrusted }
    } else if !owned.is_empty() {
        LegacyTrayStartupState::Detected { entries: owned }
    } else {
        LegacyTrayStartupState::Absent
    }
}

pub(crate) fn plan_startup_restore(
    original: &[u8],
    current: Option<&[u8]>,
) -> Result<StartupRestoreAction, String> {
    match current {
        Some(current) if current == original => Ok(StartupRestoreAction::Noop),
        None => Ok(StartupRestoreAction::Restore),
        Some(_) => Err("legacy MacTray startup changed after migration".to_owned()),
    }
}

pub(crate) fn observe_tray_startup() -> LegacyTrayStartupState {
    #[cfg(windows)]
    {
        windows::observe()
    }
    #[cfg(not(windows))]
    {
        LegacyTrayStartupState::Absent
    }
}

pub(crate) fn observe_owned_tray_startup(
    scope: LegacyTrayStartupScope,
) -> Result<Vec<LegacyTrayStartupArtifact>, String> {
    #[cfg(windows)]
    {
        windows::observe_owned(scope).map_err(startup_error_message)
    }
    #[cfg(not(windows))]
    {
        let _ = scope;
        Err("legacy MacTray startup inspection is available only on Windows".to_owned())
    }
}

pub(crate) fn read_tray_startup_artifact_bytes(
    artifact: &LegacyTrayStartupArtifact,
) -> Result<Option<Vec<u8>>, String> {
    #[cfg(windows)]
    {
        windows::read_artifact_bytes(artifact).map_err(startup_error_message)
    }
    #[cfg(not(windows))]
    {
        let _ = artifact;
        Err("legacy MacTray startup inspection is available only on Windows".to_owned())
    }
}

pub(crate) fn remove_tray_startup_artifact_exact(
    artifact: &LegacyTrayStartupArtifact,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows::remove_artifact_exact(artifact).map_err(startup_error_message)
    }
    #[cfg(not(windows))]
    {
        let _ = artifact;
        Err("legacy MacTray startup mutation is available only on Windows".to_owned())
    }
}

pub(crate) fn restore_tray_startup_artifact_if_absent(
    artifact: &LegacyTrayStartupArtifact,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows::restore_artifact_if_absent(artifact).map_err(startup_error_message)
    }
    #[cfg(not(windows))]
    {
        let _ = artifact;
        Err("legacy MacTray startup mutation is available only on Windows".to_owned())
    }
}

#[cfg(windows)]
fn startup_error_message(error: StructuredServiceError) -> String {
    match error.win32_error {
        Some(code) => format!("{}: {} (Win32 {code})", error.code, error.message),
        None => format!("{}: {}", error.code, error.message),
    }
}

fn exactly_quoted_target(command: &str) -> Option<&str> {
    if command.len() < 3 || !command.starts_with('"') || !command.ends_with('"') {
        return None;
    }
    let target = &command[1..command.len() - 1];
    if target.is_empty()
        || target.contains('"')
        || target.contains('\0')
        || target.contains('\r')
        || target.contains('\n')
    {
        None
    } else {
        Some(target)
    }
}

pub(crate) fn same_windows_path(left: &Path, right: &Path) -> bool {
    normalize_windows_path(left).eq_ignore_ascii_case(&normalize_windows_path(right))
}

fn normalize_windows_path(path: &Path) -> String {
    let path = path.to_string_lossy().replace('/', "\\");
    path.strip_prefix(r"\\?\")
        .unwrap_or(path.as_str())
        .trim_end_matches('\\')
        .to_owned()
}

#[cfg(windows)]
mod windows;
