use super::super::model::*;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn validate_legacy_path_chain(root: &Path, candidate: &Path) -> Result<(), String> {
    mactype_service_platform::validate_path_chain(candidate, Some(root)).map_err(
        |error| match error {
            mactype_service_platform::PathChainError::ReparsePoint(path) => {
                format!("legacy migration refuses reparse point {}", path.display())
            }
            mactype_service_platform::PathChainError::UnsafeComponent(_) => {
                "legacy migration path contains an unsafe component".to_owned()
            }
            mactype_service_platform::PathChainError::EscapedRoot { .. } => {
                "legacy migration path escaped its trusted root".to_owned()
            }
            mactype_service_platform::PathChainError::Io { source, .. } => source.to_string(),
        },
    )
}

pub(super) fn entry_is_reparse(path: &Path) -> Result<bool, String> {
    match mactype_service_platform::validate_path_chain(path, Some(path)) {
        Ok(()) => Ok(false),
        Err(mactype_service_platform::PathChainError::ReparsePoint(_)) => Ok(true),
        Err(mactype_service_platform::PathChainError::Io { source, .. })
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            Ok(false)
        }
        Err(mactype_service_platform::PathChainError::Io { source, .. }) => Err(source.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

pub(in crate::machine_integration::legacy_migration) fn ensure_absent_restore_target_with(
    path: &Path,
    exists: impl FnOnce(&Path) -> Result<bool, String>,
) -> Result<(), String> {
    if exists(path)? {
        Err(format!(
            "legacy profile cleanup is unknown because {} was recorded absent but now exists; refusing to delete it",
            path.display()
        ))
    } else {
        Ok(())
    }
}

pub(in crate::machine_integration::legacy_migration) fn ensure_absent_restore_target(
    path: &Path,
) -> Result<(), String> {
    ensure_absent_restore_target_with(path, |candidate| match fs::symlink_metadata(candidate) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    })
}

pub(in crate::machine_integration::legacy_migration) fn validate_existing_path(
    root: &Path,
    candidate: &Path,
) -> Result<(), String> {
    validate_legacy_path_chain(root, candidate)
}

#[derive(Clone, Copy, Debug)]
pub(in crate::machine_integration::legacy_migration) struct OpenedFileMetadata {
    pub(in crate::machine_integration::legacy_migration) is_regular_file: bool,
    pub(in crate::machine_integration::legacy_migration) is_reparse_point: bool,
    pub(in crate::machine_integration::legacy_migration) byte_length: u64,
}

pub(in crate::machine_integration::legacy_migration) fn read_opened_bounded_with<R: Read>(
    path: &Path,
    maximum: u64,
    open: impl FnOnce(&Path) -> Result<(R, OpenedFileMetadata), String>,
) -> Result<Vec<u8>, String> {
    let (file, metadata) = open(path)?;
    if metadata.is_reparse_point {
        return Err(format!(
            "legacy migration refuses reparse point {}",
            path.display()
        ));
    }
    if !metadata.is_regular_file {
        return Err(format!(
            "{} is not a regular migration file",
            path.display()
        ));
    }
    if metadata.byte_length > maximum {
        return Err(format!(
            "{} exceeds the {} byte migration limit",
            path.display(),
            maximum
        ));
    }
    let mut bytes = Vec::new();
    file.take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > maximum || bytes.len() as u64 != metadata.byte_length {
        return Err(format!(
            "{} changed size while its migration handle was being read",
            path.display()
        ));
    }
    Ok(bytes)
}

fn open_migration_file(path: &Path) -> Result<(File, OpenedFileMetadata), String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        options
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(FILE_SHARE_READ);
        let file = options.open(path).map_err(|error| error.to_string())?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        Ok((
            file,
            OpenedFileMetadata {
                is_regular_file: metadata.file_type().is_file(),
                is_reparse_point: metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0,
                byte_length: metadata.len(),
            },
        ))
    }
    #[cfg(not(windows))]
    {
        let file = options.open(path).map_err(|error| error.to_string())?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        Ok((
            file,
            OpenedFileMetadata {
                is_regular_file: metadata.file_type().is_file(),
                is_reparse_point: metadata.file_type().is_symlink(),
                byte_length: metadata.len(),
            },
        ))
    }
}

pub(in crate::machine_integration::legacy_migration) fn read_bounded_under_with<R: Read>(
    trusted_root: &Path,
    path: &Path,
    maximum: u64,
    open: impl FnOnce(&Path) -> Result<(R, OpenedFileMetadata), String>,
) -> Result<Vec<u8>, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "legacy migration file has no parent directory".to_owned())?;
    validate_legacy_path_chain(trusted_root, parent)?;
    read_opened_bounded_with(path, maximum, open)
}

pub(in crate::machine_integration::legacy_migration) fn read_bounded_under(
    trusted_root: &Path,
    path: &Path,
    maximum: u64,
) -> Result<Vec<u8>, String> {
    read_bounded_under_with(trusted_root, path, maximum, open_migration_file)
}

pub(in crate::machine_integration::legacy_migration) fn read_json_bounded_under<
    T: DeserializeOwned,
>(
    trusted_root: &Path,
    path: &Path,
) -> Result<T, String> {
    let bytes = read_bounded_under(trusted_root, path, MAX_RECEIPT_BYTES)?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

pub(in crate::machine_integration::legacy_migration) fn read_regular_bounded_under(
    trusted_root: &Path,
    path: &Path,
    maximum: u64,
) -> Result<Vec<u8>, String> {
    read_bounded_under(trusted_root, path, maximum)
}

pub(in crate::machine_integration::legacy_migration) fn read_optional_regular_bounded_under(
    trusted_root: &Path,
    path: &Path,
    maximum: u64,
) -> Result<Option<Vec<u8>>, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "legacy migration file has no parent directory".to_owned())?;
    validate_legacy_path_chain(trusted_root, parent)?;
    match fs::symlink_metadata(path) {
        Ok(_) => read_regular_bounded_under(trusted_root, path, maximum).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn temporary_sibling(path: &Path) -> Result<PathBuf, String> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "migration target has no valid file name".to_owned())?;
    Ok(path.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    )))
}

fn validate_atomic_target(parent: &Path, path: &Path) -> Result<bool, String> {
    if entry_is_reparse(parent)? {
        return Err("legacy migration refuses an atomic write through a reparse point".to_owned());
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if entry_is_reparse(path)? || !metadata.file_type().is_file() {
                Err("legacy migration refuses an unsafe atomic write target".to_owned())
            } else {
                Ok(true)
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(windows)]
fn replace_existing(destination: &Path, replacement: &Path) -> Result<(), String> {
    mactype_service_platform::replace_file_preserving_attributes(destination, replacement, None)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn replace_existing(destination: &Path, replacement: &Path) -> Result<(), String> {
    fs::rename(replacement, destination).map_err(|error| error.to_string())
}

pub(in crate::machine_integration::legacy_migration) fn atomic_write(
    path: &Path,
    bytes: &[u8],
) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "migration target has no parent".to_owned())?;
    validate_atomic_target(parent, path)?;
    let temporary = temporary_sibling(path)?;
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    if let Err(error) = output.write_all(bytes).and_then(|()| output.sync_all()) {
        drop(output);
        let _ = fs::remove_file(&temporary);
        return Err(error.to_string());
    }
    drop(output);
    let destination_exists = match validate_atomic_target(parent, path) {
        Ok(exists) => exists,
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
    };
    let result = if destination_exists {
        replace_existing(path, &temporary)
    } else {
        fs::rename(&temporary, path).map_err(|error| error.to_string())
    };
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(in crate::machine_integration::legacy_migration) fn atomic_json(
    path: &Path,
    value: &impl Serialize,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err("legacy migration receipt exceeded its size limit".to_owned());
    }
    atomic_write(path, &bytes)
}
