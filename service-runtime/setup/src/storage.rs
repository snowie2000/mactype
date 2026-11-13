use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(windows)]
use std::time::{Duration, Instant};

use mactype_service_contract::ProfileError;

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[cfg(windows)]
const PROFILE_FILE_RETRY_BUDGET: Duration = Duration::from_secs(2);
#[cfg(windows)]
const PROFILE_FILE_INITIAL_BACKOFF: Duration = Duration::from_millis(5);
#[cfg(windows)]
const PROFILE_FILE_MAX_BACKOFF: Duration = Duration::from_millis(100);

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
    RollbackFailed {
        operation: String,
        restoration: String,
    },
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
            Self::RollbackFailed {
                operation,
                restoration,
            } => write!(
                formatter,
                "machine cleanup state is unknown: {operation}; {restoration}"
            ),
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
    atomic_write_with_replace(path, bytes, replace_file)
}

#[cfg(all(test, windows))]
pub(crate) fn atomic_write_until(
    path: &Path,
    bytes: &[u8],
    deadline: Instant,
) -> Result<(), SetupError> {
    atomic_write_with_replace(path, bytes, |source, destination| {
        replace_file_until(source, destination, deadline)
    })
}

fn atomic_write_with_replace(
    path: &Path,
    bytes: &[u8],
    replace: impl FnOnce(&Path, &Path) -> Result<(), SetupError>,
) -> Result<(), SetupError> {
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
        replace(&temporary, path)
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
pub(crate) fn retry_profile_file_operation<T>(
    operation: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    retry_profile_file_operation_until(Instant::now() + PROFILE_FILE_RETRY_BUDGET, operation)
}

#[cfg(not(windows))]
pub(crate) fn retry_profile_file_operation<T>(
    mut operation: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    operation()
}

#[cfg(windows)]
fn retry_profile_file_operation_until<T>(
    deadline: Instant,
    mut operation: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
    use windows_sys::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_LOCK_VIOLATION, ERROR_SHARING_VIOLATION,
    };

    let mut backoff = PROFILE_FILE_INITIAL_BACKOFF;
    loop {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error)
                if matches!(
                    error.raw_os_error(),
                    Some(code)
                        if code == ERROR_ACCESS_DENIED as i32
                            || code == ERROR_SHARING_VIOLATION as i32
                            || code == ERROR_LOCK_VIOLATION as i32
                ) =>
            {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(error);
                }
                if remaining <= backoff {
                    std::thread::sleep(remaining);
                    return Err(error);
                }
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(PROFILE_FILE_MAX_BACKOFF);
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), SetupError> {
    retry_profile_file_operation(|| mactype_service_platform::replace_file(source, destination))
        .map_err(SetupError::Io)
}

#[cfg(all(test, windows))]
fn replace_file_until(
    source: &Path,
    destination: &Path,
    deadline: Instant,
) -> Result<(), SetupError> {
    retry_profile_file_operation_until(deadline, || {
        mactype_service_platform::replace_file(source, destination)
    })
    .map_err(SetupError::Io)
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), SetupError> {
    retry_profile_file_operation(|| fs::rename(source, destination)).map_err(SetupError::Io)
}

#[cfg(windows)]
fn is_reparse_point(path: &Path) -> Result<bool, SetupError> {
    mactype_service_platform::is_reparse_point(path).map_err(SetupError::Io)
}

#[cfg(not(windows))]
fn is_reparse_point(path: &Path) -> Result<bool, SetupError> {
    Ok(fs::symlink_metadata(path)?.file_type().is_symlink())
}

#[cfg(all(test, windows))]
mod tests {
    use std::os::windows::fs::OpenOptionsExt;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::thread::{self, JoinHandle};

    use tempfile::tempdir;
    use windows_sys::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_LOCK_VIOLATION, ERROR_SHARING_VIOLATION,
        GENERIC_READ,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    use super::*;

    fn hold_read_handle(path: PathBuf, duration: Duration) -> JoinHandle<()> {
        let (ready_tx, ready_rx) = mpsc::sync_channel(0);
        let holder = thread::spawn(move || {
            let file = OpenOptions::new()
                .access_mode(GENERIC_READ)
                .share_mode(FILE_SHARE_READ)
                .open(path)
                .expect("failed to open the destination with renderer-compatible sharing");
            ready_tx
                .send(())
                .expect("failed to report the held destination handle");
            thread::sleep(duration);
            drop(file);
        });
        ready_rx
            .recv()
            .expect("destination handle holder stopped before reporting readiness");
        holder
    }

    #[test]
    fn direct_replace_is_denied_while_destination_disallows_delete_sharing() {
        let directory = tempdir().unwrap();
        let destination = directory.path().join("MacType.ini");
        let replacement = directory.path().join("replacement.ini");
        fs::write(&destination, b"old").unwrap();
        fs::write(&replacement, b"new").unwrap();
        let held = OpenOptions::new()
            .access_mode(GENERIC_READ)
            .share_mode(FILE_SHARE_READ)
            .open(&destination)
            .unwrap();

        let error = mactype_service_platform::replace_file(&replacement, &destination).unwrap_err();

        assert_eq!(error.raw_os_error(), Some(ERROR_ACCESS_DENIED as i32));
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        drop(held);
    }

    #[test]
    fn atomic_write_retries_until_destination_reader_releases_its_handle() {
        let directory = tempdir().unwrap();
        let destination = directory.path().join("MacType.ini");
        fs::write(&destination, b"old").unwrap();
        let holder = hold_read_handle(destination.clone(), Duration::from_millis(300));

        atomic_write(&destination, b"new").unwrap();

        holder.join().unwrap();
        assert_eq!(fs::read(destination).unwrap(), b"new");
    }

    #[test]
    fn atomic_write_stops_at_injected_deadline_and_removes_its_temporary_file() {
        let directory = tempdir().unwrap();
        let destination = directory.path().join("MacType.ini");
        fs::write(&destination, b"old").unwrap();
        let held = OpenOptions::new()
            .access_mode(GENERIC_READ)
            .share_mode(FILE_SHARE_READ)
            .open(&destination)
            .unwrap();

        let error = atomic_write_until(
            &destination,
            b"new",
            Instant::now() + Duration::from_millis(100),
        )
        .unwrap_err();

        drop(held);
        let SetupError::Io(error) = error else {
            panic!("atomic write returned an unexpected error: {error}");
        };
        assert_eq!(error.raw_os_error(), Some(ERROR_ACCESS_DENIED as i32));
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        assert!(
            fs::read_dir(directory.path()).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".MacType.ini.new-")
            }),
            "atomic write left its temporary file behind"
        );
    }

    #[test]
    fn transient_windows_errors_retry_the_same_operation() {
        for code in [
            ERROR_ACCESS_DENIED,
            ERROR_SHARING_VIOLATION,
            ERROR_LOCK_VIOLATION,
        ] {
            let mut attempts = 0;
            let result = retry_profile_file_operation(|| {
                attempts += 1;
                if attempts == 1 {
                    Err(io::Error::from_raw_os_error(code as i32))
                } else {
                    Ok(42)
                }
            });
            assert_eq!(result.unwrap(), 42);
            assert_eq!(attempts, 2);
        }
    }

    #[test]
    fn non_transient_error_is_returned_without_retrying() {
        let directory = tempdir().unwrap();
        let missing_replacement = directory.path().join("missing.ini");
        let destination = directory.path().join("MacType.ini");
        fs::write(&destination, b"old").unwrap();
        let mut attempts = 0;

        let result = retry_profile_file_operation(|| {
            attempts += 1;
            mactype_service_platform::replace_file(&missing_replacement, &destination)
        });

        let error = result.unwrap_err();
        assert_eq!(attempts, 1);
        assert_eq!(error.raw_os_error(), Some(ERROR_FILE_NOT_FOUND as i32));
        assert_eq!(fs::read(destination).unwrap(), b"old");
    }
}
