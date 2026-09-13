#[cfg(test)]
use super::storage::replace_file;
use super::{
    document::validate_entry, LegacyProfileCandidate, ProfileDocument,
    MAX_PROFILE_DIRECTORY_ENTRIES,
};
use crate::{bounded_io::read_bounded_file, installation_root};
use std::{
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

#[cfg(test)]
use std::fs::File;

pub(super) fn user_profile_root() -> Option<PathBuf> {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("MacType").join("ControlCenter").join("profiles"))
}

pub(super) fn discover_legacy_profile_at(
    root: &Path,
) -> Result<Option<LegacyProfileCandidate>, String> {
    let configuration = root.join("MacType.ini");
    if !configuration.is_file() {
        return Ok(None);
    }
    let document = ProfileDocument::open(&configuration)?;
    let primary = || LegacyProfileCandidate {
        name: configuration
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "MacType profile".to_owned()),
        path: configuration.to_string_lossy().into_owned(),
        source: "primary-file".to_owned(),
    };
    let Some(alternative) = document.raw_value("General", "AlternativeFile") else {
        return Ok(Some(primary()));
    };
    let alternative = alternative.trim().trim_matches('"');
    if alternative.is_empty() {
        return Ok(Some(primary()));
    }
    let candidate = PathBuf::from(alternative);
    let candidate = if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    };
    if !candidate.is_file()
        || !candidate
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
    {
        return Ok(None);
    }
    let managed_source_name = candidate
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("ControlCenter.ini"))
        .then(|| document.raw_value("General", "ControlCenterSourceProfile"))
        .flatten()
        .and_then(|source| Path::new(source.trim().trim_matches('"')).file_stem())
        .map(|name| name.to_string_lossy().into_owned());
    Ok(Some(LegacyProfileCandidate {
        name: managed_source_name.unwrap_or_else(|| {
            candidate
                .file_stem()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "MacType profile".to_owned())
        }),
        path: candidate.to_string_lossy().into_owned(),
        source: "alternative-file".to_owned(),
    }))
}

pub(crate) fn legacy_alternative_file_bytes(bytes: &[u8]) -> Result<Option<PathBuf>, String> {
    let document = ProfileDocument::from_bytes(PathBuf::new(), bytes)?;
    Ok(document
        .raw_value("General", "AlternativeFile")
        .map(str::trim)
        .map(|value| value.trim_matches('"'))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from))
}

pub(super) fn import_profile_to(
    source: &Path,
    directory: &Path,
) -> Result<ProfileDocument, String> {
    if !source.is_file()
        || !source
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
    {
        return Err("select an existing INI profile".to_owned());
    }
    let source = fs::canonicalize(source).map_err(|error| error.to_string())?;
    let bytes = read_bounded_file(
        &source,
        mactype_service_contract::MAX_PROFILE_BYTES,
        "imported profile",
    )?;
    ProfileDocument::from_bytes(source.clone(), &bytes)?;
    let stem = source
        .file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| "profile has no file name".to_owned())?;
    validate_entry(&stem, "profile name")?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;

    for suffix in 1..=999 {
        let name = if suffix == 1 {
            format!("{stem}.ini")
        } else {
            format!("{stem} ({suffix}).ini")
        };
        let destination = directory.join(name);
        let mut output = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        };
        if let Err(error) = output.write_all(&bytes).and_then(|()| output.sync_all()) {
            drop(output);
            let _ = fs::remove_file(&destination);
            return Err(error.to_string());
        }
        drop(output);
        return ProfileDocument::open(destination);
    }
    Err("too many profiles have the same name".to_owned())
}

pub(super) fn find_default_profile() -> Result<Option<PathBuf>, String> {
    let Some(root) = installation_root() else {
        return Ok(None);
    };
    find_default_profile_at(&root)
}

pub(super) fn find_default_profile_at(root: &Path) -> Result<Option<PathBuf>, String> {
    let profile_root = root.join("ini");
    let default = profile_root.join("Default.ini");
    if default.is_file() {
        return Ok(Some(default));
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(&profile_root) {
        for (index, entry) in entries.enumerate() {
            if index == MAX_PROFILE_DIRECTORY_ENTRIES {
                return Err(format!(
                    "profile directory contains more than {MAX_PROFILE_DIRECTORY_ENTRIES} entries"
                ));
            }
            let Ok(entry) = entry else {
                continue;
            };
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
            {
                candidates.push(path);
            }
        }
    }
    // Directory enumeration order is filesystem-dependent; pick the
    // case-insensitive alphabetical minimum so the fallback is deterministic.
    let fallback = candidates.into_iter().min_by_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
    });
    if fallback.is_some() {
        return Ok(fallback);
    }
    Ok(root
        .join("MacType.ini")
        .is_file()
        .then(|| root.join("MacType.ini")))
}

pub(super) fn default_profile_payload_from(path: PathBuf) -> Result<(PathBuf, Vec<u8>), String> {
    let bytes = read_bounded_file(
        &path,
        mactype_service_contract::MAX_PROFILE_BYTES,
        "default profile",
    )?;
    ProfileDocument::from_bytes(path.clone(), &bytes)?;
    Ok((path, bytes))
}

pub(crate) fn bundled_default_profile_at(
    installation: &Path,
) -> Result<Option<(PathBuf, Vec<u8>)>, String> {
    let path = installation.join("ini").join("Default.ini");
    if !path.is_file() {
        return Ok(None);
    }
    default_profile_payload_from(path).map(Some)
}

#[cfg(test)]
pub(super) fn install_system_profile_at(
    root: &Path,
    source_profile: &Path,
    profile_bytes: &[u8],
) -> Result<(), String> {
    if profile_bytes.is_empty() || profile_bytes.len() > 4 * 1024 * 1024 {
        return Err("system profile must be between 1 byte and 4 MiB".to_owned());
    }
    let source_name = source_profile
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "system profile has no valid file name".to_owned())?;
    if !source_name.to_ascii_lowercase().ends_with(".ini") {
        return Err("system profile must be an INI file".to_owned());
    }

    let profile_root = root.join("ini");
    fs::create_dir_all(&profile_root).map_err(|error| error.to_string())?;
    let destination = profile_root.join("ControlCenter.ini");
    let temporary = profile_root.join(format!(
        ".ControlCenter.ini.mactype-{}.tmp",
        std::process::id()
    ));
    let mut output = File::create(&temporary).map_err(|error| error.to_string())?;
    output
        .write_all(profile_bytes)
        .map_err(|error| error.to_string())?;
    output.sync_all().map_err(|error| error.to_string())?;
    drop(output);
    let install_result = if destination.exists() {
        let backup = destination.with_extension("ini.bak");
        replace_file(&destination, &temporary, &backup)
    } else {
        fs::rename(&temporary, &destination).map_err(|error| error.to_string())
    };
    if let Err(error) = install_result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }

    let configuration = root.join("MacType.ini");
    let mut document = ProfileDocument::open(&configuration)?;
    document.set_raw_value(
        "General",
        "AlternativeFile",
        Some(r"ini\ControlCenter.ini".to_owned()),
        "system:alternative-file",
    );
    document.set_raw_value(
        "General",
        "ControlCenterSourceProfile",
        Some(source_profile.to_string_lossy().into_owned()),
        "system:source-profile",
    );
    document.save()
}
