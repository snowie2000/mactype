use super::model::*;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

mod file_io;
mod system;

pub(super) use file_io::*;
pub(super) use system::*;

pub(super) fn generation_name() -> Result<String, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "migration-{}-{}",
        now.as_nanos(),
        std::process::id()
    ))
}

pub(super) fn valid_generation_name(value: &str) -> bool {
    value.starts_with("migration-")
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

pub(super) fn backup_source(
    generation_root: &Path,
    source: &Path,
    role: BackupRole,
    bytes: &[u8],
) -> Result<BackupFileReceipt, String> {
    let backup_file = match role {
        BackupRole::Configuration | BackupRole::ConfigurationAndActiveProfile => {
            CONFIGURATION_BACKUP
        }
        BackupRole::ActiveProfile => ACTIVE_PROFILE_BACKUP,
    };
    atomic_write(&generation_root.join(backup_file), bytes)?;
    Ok(BackupFileReceipt {
        role,
        original_path: source.to_string_lossy().into_owned(),
        backup_file: backup_file.to_owned(),
        byte_length: bytes.len() as u64,
        sha256: hex_sha256(bytes),
    })
}

pub(super) fn profile_backup_receipt(
    generation_root: &Path,
    source: &Path,
    role: BackupRole,
    bytes: Option<&[u8]>,
) -> Result<ProfileBackupReceipt, String> {
    match bytes {
        Some(bytes) => {
            backup_source(generation_root, source, role, bytes).map(ProfileBackupReceipt::Present)
        }
        None => Ok(ProfileBackupReceipt::Absent {
            role,
            original_path: source.to_string_lossy().into_owned(),
        }),
    }
}
