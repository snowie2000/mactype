use super::storage::{atomic_write, data_root};
use crate::bounded_io::read_bounded_file;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

const RUNTIME_ARTIFACTS: [(&str, bool); 4] = [
    ("MacLoader.exe", true),
    ("MacType.dll", true),
    ("MacLoader64.exe", false),
    ("MacType64.dll", false),
];
const MAX_ACTIVE_RUNTIME_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ActiveRuntime {
    pub(super) runtime_root: PathBuf,
    pub(super) source_profile: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedProfile {
    pub source_profile: String,
    pub runtime_root: String,
}

pub(super) fn runtime_root() -> Result<PathBuf, String> {
    Ok(data_root()?.join("runtime"))
}

pub(super) fn active_runtime_from(base: &Path) -> Result<ActiveRuntime, String> {
    let bytes = read_bounded_file(
        &base.join("active.json"),
        MAX_ACTIVE_RUNTIME_BYTES,
        "active runtime pointer",
    )?;
    let active: ActiveRuntime =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let generations =
        fs::canonicalize(base.join("generations")).map_err(|error| error.to_string())?;
    let root = fs::canonicalize(&active.runtime_root).map_err(|error| error.to_string())?;
    if !root.starts_with(&generations)
        || !root.join("MacLoader.exe").is_file()
        || !root.join("MacType.dll").is_file()
        || !root.join("MacType.ini").is_file()
        || !root.join("profile.ini").is_file()
    {
        return Err(
            "active MacType runtime is incomplete or outside the managed directory".to_owned(),
        );
    }
    Ok(ActiveRuntime {
        runtime_root: root,
        ..active
    })
}

pub(super) fn active_runtime() -> Result<ActiveRuntime, String> {
    active_runtime_from(&runtime_root()?)
}

pub(crate) fn active_system_profile_paths() -> Result<(PathBuf, PathBuf), String> {
    let active = active_runtime()?;
    let profile = fs::canonicalize(active.runtime_root.join("profile.ini"))
        .map_err(|error| error.to_string())?;
    let length = fs::metadata(&profile)
        .map_err(|error| error.to_string())?
        .len();
    if length == 0 || length > mactype_service_contract::MAX_PROFILE_BYTES as u64 {
        return Err("active system profile must be between 1 byte and 4 MiB".to_owned());
    }
    Ok((active.source_profile, profile))
}

pub(super) fn active_system_profile_payload() -> Result<Vec<u8>, String> {
    let (_, profile) = active_system_profile_paths()?;
    active_profile_payload_at(&profile)
}

pub(super) fn active_profile_payload_at(profile: &Path) -> Result<Vec<u8>, String> {
    let bytes = read_bounded_file(
        profile,
        mactype_service_contract::MAX_PROFILE_BYTES,
        "active system profile",
    )?;
    if bytes.is_empty() {
        return Err("active system profile is outside the allowed range".to_owned());
    }
    Ok(bytes)
}

pub(super) fn active_profile_payload_for(active: &ActiveRuntime) -> Result<Vec<u8>, String> {
    active_profile_payload_at(&active.runtime_root.join("profile.ini"))
}

fn system_injection_pause_path() -> Result<PathBuf, String> {
    Ok(data_root()?.join("system-injection-paused"))
}

pub(crate) fn record_system_injection_choice(enabled: bool) -> Result<(), String> {
    let marker = system_injection_pause_path()?;
    if enabled {
        match fs::remove_file(marker) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    } else {
        atomic_write(&marker, b"paused\n")
    }
}

pub(super) fn system_injection_paused() -> bool {
    system_injection_pause_path().is_ok_and(|path| path.is_file())
}

pub(super) fn prepare_runtime_at(
    base: &Path,
    installation_root: &Path,
    source_profile: &Path,
    profile_bytes: &[u8],
) -> Result<ActiveRuntime, String> {
    if profile_bytes.is_empty() {
        return Err("runtime profile must not be empty".to_owned());
    }
    if profile_bytes.len() > mactype_service_contract::MAX_PROFILE_BYTES {
        return Err(format!(
            "runtime profile exceeds its {}-byte limit",
            mactype_service_contract::MAX_PROFILE_BYTES
        ));
    }
    let installation = fs::canonicalize(installation_root).map_err(|error| error.to_string())?;
    let mut sources = Vec::new();
    let mut fingerprint = Sha256::new();
    fingerprint.update(profile_bytes);
    for (name, required) in RUNTIME_ARTIFACTS {
        let candidate = installation.join(name);
        if !candidate.is_file() {
            if required {
                return Err(format!("{name} was not found in the selected installation"));
            }
            continue;
        }
        let source = fs::canonicalize(&candidate).map_err(|error| error.to_string())?;
        if source.parent() != Some(installation.as_path()) {
            return Err(format!("{name} resolves outside the selected installation"));
        }
        let bytes = read_bounded_file(
            &source,
            mactype_service_contract::MAX_RUNTIME_FILE_BYTES,
            &format!("runtime artifact {name}"),
        )?;
        fingerprint.update(name.as_bytes());
        fingerprint.update(&bytes);
        sources.push((name, bytes));
    }
    let digest = fingerprint.finalize();
    let id = digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let generations = base.join("generations");
    fs::create_dir_all(&generations).map_err(|error| error.to_string())?;
    let generation = generations.join(&id);
    if !generation.exists() {
        let staging = generations.join(format!(".stage-{id}-{}", std::process::id()));
        if staging.exists() {
            fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
        }
        fs::create_dir(&staging).map_err(|error| error.to_string())?;
        for (name, bytes) in &sources {
            fs::write(staging.join(name), bytes).map_err(|error| error.to_string())?;
        }
        fs::write(staging.join("profile.ini"), profile_bytes).map_err(|error| error.to_string())?;
        fs::write(
            staging.join("MacType.ini"),
            b"[General]\r\nAlternativeFile=profile.ini\r\n",
        )
        .map_err(|error| error.to_string())?;
        match fs::rename(&staging, &generation) {
            Ok(()) => {}
            Err(_) if generation.is_dir() => {
                fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    let active = ActiveRuntime {
        runtime_root: generation,
        source_profile: crate::profile::source_profile_reference(installation_root, source_profile),
    };
    atomic_write(
        &base.join("active.json"),
        &serde_json::to_vec_pretty(&active).map_err(|error| error.to_string())?,
    )?;
    active_runtime_from(base)
}

pub fn apply_profile(
    installation_root: &Path,
    source_profile: &Path,
    profile_bytes: &[u8],
) -> Result<AppliedProfile, String> {
    let active = prepare_runtime_at(
        &runtime_root()?,
        installation_root,
        source_profile,
        profile_bytes,
    )?;
    Ok(AppliedProfile {
        source_profile: active.source_profile.to_string_lossy().into_owned(),
        runtime_root: active.runtime_root.to_string_lossy().into_owned(),
    })
}
