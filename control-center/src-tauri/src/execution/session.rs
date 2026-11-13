use super::{
    runtime::active_runtime,
    storage::{atomic_write, data_root},
};
use crate::bounded_io::read_bounded_file;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const MAX_SESSION_TARGETS_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTarget {
    pub target: String,
    pub arguments: Vec<String>,
}

pub(super) fn launch_with_mactype_impl(target: &str, arguments: &[String]) -> Result<u32, String> {
    let target = validate_launch(target, arguments)?;
    let active =
        active_runtime().map_err(|_| "apply a profile before launching with MacType".to_owned())?;
    let loader = active.runtime_root.join("MacLoader.exe");
    Command::new(loader)
        .arg(&target)
        .args(arguments)
        .current_dir(&active.runtime_root)
        .spawn()
        .map(|child| child.id())
        .map_err(|error| error.to_string())
}

fn validate_launch(target: &str, arguments: &[String]) -> Result<PathBuf, String> {
    if arguments.len() > 32 || arguments.iter().any(|argument| argument.len() > 4096) {
        return Err(
            "manual launch accepts at most 32 arguments of 4096 characters each".to_owned(),
        );
    }
    let target = fs::canonicalize(target).map_err(|error| error.to_string())?;
    if !target.is_file()
        || !target
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Err("manual launch target must be an existing .exe file".to_owned());
    }
    Ok(target)
}

fn session_targets_path() -> Result<PathBuf, String> {
    Ok(data_root()?.join("session-targets.json"))
}

pub fn session_targets() -> Result<Vec<SessionTarget>, String> {
    let path = session_targets_path()?;
    session_targets_from(&path)
}

pub(super) fn session_targets_from(path: &Path) -> Result<Vec<SessionTarget>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = read_bounded_file(path, MAX_SESSION_TARGETS_BYTES, "session target list")?;
    let targets: Vec<SessionTarget> =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if targets.len() > 32 {
        return Err("session target list exceeds 32 entries".to_owned());
    }
    Ok(targets)
}

pub(super) fn write_session_targets_to(
    path: &Path,
    targets: &[SessionTarget],
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(targets).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_SESSION_TARGETS_BYTES {
        return Err(format!(
            "session target list exceeds its {MAX_SESSION_TARGETS_BYTES}-byte limit"
        ));
    }
    atomic_write(path, &bytes)
}

pub(super) fn register_session_target_impl(
    target: &str,
    arguments: &[String],
) -> Result<Vec<SessionTarget>, String> {
    let target = validate_launch(target, arguments)?
        .to_string_lossy()
        .into_owned();
    let mut targets = session_targets()?;
    if !targets
        .iter()
        .any(|entry| entry.target.eq_ignore_ascii_case(&target))
    {
        if targets.len() == 32 {
            return Err("session target list already contains 32 entries".to_owned());
        }
        targets.push(SessionTarget {
            target,
            arguments: arguments.to_vec(),
        });
    }
    write_session_targets_to(&session_targets_path()?, &targets)?;
    Ok(targets)
}

pub(super) fn remove_session_target_impl(target: &str) -> Result<Vec<SessionTarget>, String> {
    let mut targets = session_targets()?;
    targets.retain(|entry| !entry.target.eq_ignore_ascii_case(target));
    write_session_targets_to(&session_targets_path()?, &targets)?;
    Ok(targets)
}

pub(super) fn launch_registered_targets_impl() -> Result<Vec<u32>, String> {
    session_targets()?
        .iter()
        .map(|entry| launch_with_mactype_impl(&entry.target, &entry.arguments))
        .collect()
}
