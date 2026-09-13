use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use mactype_service_contract::HealthReport;
#[cfg(windows)]
use mactype_service_platform::replace_file;

use crate::protected_path::{has_reparse_ancestor, read_bounded_contents};
use crate::HealthPublisher;

const MAX_HEALTH_SNAPSHOT_BYTES: u64 = 16 * 1024;
const MAX_SERVICE_ROOT_ENTRIES: usize = 4096;
static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub struct FileHealthPublisher {
    path: PathBuf,
}

impl FileHealthPublisher {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn read(path: &Path) -> io::Result<HealthReport> {
        reject_reparse_ancestors(path)?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
            options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let file = options.open(path)?;
        let metadata = file.metadata()?;
        if metadata_is_reparse(&metadata)
            || !metadata.is_file()
            || metadata.len() == 0
            || metadata.len() > MAX_HEALTH_SNAPSHOT_BYTES
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "persisted health snapshot is not a bounded regular file",
            ));
        }
        let bytes = read_bounded_contents(file, MAX_HEALTH_SNAPSHOT_BYTES)?;
        let report: HealthReport = serde_json::from_slice(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        report
            .validate()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        Ok(report)
    }
}

#[cfg(windows)]
fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse(_metadata: &fs::Metadata) -> bool {
    false
}

impl HealthPublisher for FileHealthPublisher {
    fn publish(&self, report: &HealthReport) -> io::Result<()> {
        report
            .validate()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut bytes = serde_json::to_vec(report)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_HEALTH_SNAPSHOT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "health snapshot exceeds its fixed bound",
            ));
        }
        let parent = self.path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "health path has no parent")
        })?;
        reject_reparse_ancestors(parent)?;
        fs::create_dir_all(parent)?;
        reject_reparse_ancestors(&self.path)?;
        cleanup_owned_staging(parent)?;

        let temporary = parent.join(format!(".health.json.new-{}", temporary_nonce()));
        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            replace_file(&temporary, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

fn cleanup_owned_staging(parent: &Path) -> io::Result<()> {
    let mut entries = Vec::with_capacity(MAX_SERVICE_ROOT_ENTRIES);
    for entry in fs::read_dir(parent)? {
        if entries.len() == MAX_SERVICE_ROOT_ENTRIES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "health staging cleanup is unknown: service root entry count exceeds its fixed limit",
            ));
        }
        entries.push(entry?);
    }

    let mut removable = Vec::new();
    for entry in entries {
        let name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(_) => continue,
        };
        let Some(suffix) = name.strip_prefix(".health.json.new-") else {
            continue;
        };
        let parts = suffix.split('-').collect::<Vec<_>>();
        if parts.len() != 3
            || parts
                .iter()
                .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
        {
            continue;
        }
        let path = entry.path();
        reject_reparse_ancestors(&path)?;
        let metadata = entry.metadata()?;
        if !metadata.is_file() || metadata.len() > MAX_HEALTH_SNAPSHOT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "owned health staging residue is not a bounded regular file",
            ));
        }
        removable.push(path);
    }
    for path in removable {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub struct CompositeHealthPublisher<'a> {
    live: &'a dyn HealthPublisher,
    persisted: &'a dyn HealthPublisher,
}

impl<'a> CompositeHealthPublisher<'a> {
    pub const fn new(live: &'a dyn HealthPublisher, persisted: &'a dyn HealthPublisher) -> Self {
        Self { live, persisted }
    }
}

impl HealthPublisher for CompositeHealthPublisher<'_> {
    fn publish(&self, report: &HealthReport) -> io::Result<()> {
        self.persisted.publish(report)?;
        self.live.publish(report)
    }
}

fn temporary_nonce() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{timestamp}-{sequence}", std::process::id())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

fn reject_reparse_ancestors(path: &Path) -> io::Result<()> {
    if has_reparse_ancestor(path)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "health snapshot path contains a reparse point",
        ));
    }
    Ok(())
}
