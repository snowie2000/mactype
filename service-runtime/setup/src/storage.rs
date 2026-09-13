use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use mactype_service_contract::ProfileError;

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub enum SetupError {
    Io(io::Error),
    MachineOperation {
        operation: &'static str,
        path: PathBuf,
        source: Box<SetupError>,
    },
    InvalidProfile(ProfileError),
    InvalidMetadata,
    InvalidPointer,
    TamperedGeneration,
    ReparsePoint(PathBuf),
    NoPreviousGeneration,
    CleanupUnknown(String),
    Manifest(String),
    Runtime(String),
}

impl fmt::Display for SetupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "machine storage I/O failed: {error}"),
            Self::MachineOperation {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "machine operation '{operation}' failed at {}: {source}",
                path.display()
            ),
            Self::InvalidProfile(error) => write!(formatter, "profile validation failed: {error}"),
            Self::InvalidMetadata => formatter.write_str("source metadata is invalid"),
            Self::InvalidPointer => formatter.write_str("generation pointer is invalid"),
            Self::TamperedGeneration => {
                formatter.write_str("profile generation hash does not match")
            }
            Self::ReparsePoint(path) => {
                write!(
                    formatter,
                    "reparse points are forbidden in machine storage: {}",
                    path.display()
                )
            }
            Self::NoPreviousGeneration => {
                formatter.write_str("no previous generation is available")
            }
            Self::CleanupUnknown(message) => {
                write!(formatter, "machine cleanup state is unknown: {message}")
            }
            Self::Manifest(message) => write!(formatter, "runtime manifest is invalid: {message}"),
            Self::Runtime(message) => {
                write!(formatter, "machine runtime operation failed: {message}")
            }
        }
    }
}

impl std::error::Error for SetupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::MachineOperation { source, .. } => Some(source.as_ref()),
            Self::InvalidProfile(error) => Some(error),
            _ => None,
        }
    }
}

impl SetupError {
    pub(crate) fn at_machine_path(self, operation: &'static str, path: &Path) -> Self {
        Self::MachineOperation {
            operation,
            path: path.to_owned(),
            source: Box::new(self),
        }
    }
}

impl From<io::Error> for SetupError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ProfileError> for SetupError {
    fn from(value: ProfileError) -> Self {
        Self::InvalidProfile(value)
    }
}

impl From<serde_json::Error> for SetupError {
    fn from(value: serde_json::Error) -> Self {
        Self::Manifest(value.to_string())
    }
}

pub(crate) fn create_protected_directory(path: &Path) -> Result<(), SetupError> {
    reject_reparse_ancestors(path)?;
    fs::create_dir_all(path)?;
    reject_reparse_ancestors(path)
}

pub(crate) fn reject_reparse_ancestors(path: &Path) -> Result<(), SetupError> {
    for ancestor in path.ancestors().filter(|candidate| candidate.exists()) {
        if is_reparse_point(ancestor)? {
            return Err(SetupError::ReparsePoint(ancestor.to_owned()));
        }
    }
    Ok(())
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), SetupError> {
    let parent = path
        .parent()
        .ok_or_else(|| SetupError::Runtime("destination has no parent".to_owned()))?;
    create_protected_directory(parent)?;
    reject_reparse_ancestors(path)?;

    let temporary = parent.join(format!(
        ".{}.new-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("pointer"),
        temporary_nonce()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        replace_file(&temporary, path)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

pub(crate) fn read_bounded_regular_file(
    path: &Path,
    maximum_bytes: u64,
    description: &str,
) -> Result<Vec<u8>, SetupError> {
    reject_reparse_ancestors(path)?;
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum_bytes {
        return Err(SetupError::Runtime(format!(
            "{description} is not a bounded regular file"
        )));
    }
    let file = OpenOptions::new().read(true).open(path)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(maximum_bytes + 1).read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() as u64 > maximum_bytes {
        return Err(SetupError::Runtime(format!(
            "{description} is not a bounded regular file"
        )));
    }
    Ok(bytes)
}

pub(crate) fn read_bounded_directory(
    path: &Path,
    maximum_entries: usize,
    description: &str,
) -> Result<Vec<fs::DirEntry>, SetupError> {
    reject_reparse_ancestors(path)?;
    if !fs::metadata(path)?.is_dir() {
        return Err(SetupError::Runtime(format!(
            "{description} is not a regular directory"
        )));
    }
    let mut entries = Vec::with_capacity(maximum_entries);
    for entry in fs::read_dir(path)? {
        if entries.len() == maximum_entries {
            return Err(SetupError::Runtime(format!(
                "{description} exceeds the fixed limit"
            )));
        }
        entries.push(entry?);
    }
    Ok(entries)
}

pub(crate) fn temporary_nonce() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{timestamp}-{sequence}", std::process::id())
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), SetupError> {
    mactype_service_platform::replace_file(source, destination).map_err(SetupError::Io)
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), SetupError> {
    fs::rename(source, destination)?;
    Ok(())
}

#[cfg(windows)]
fn is_reparse_point(path: &Path) -> Result<bool, SetupError> {
    mactype_service_platform::is_reparse_point(path).map_err(SetupError::Io)
}

#[cfg(not(windows))]
fn is_reparse_point(path: &Path) -> Result<bool, SetupError> {
    Ok(fs::symlink_metadata(path)?.file_type().is_symlink())
}
