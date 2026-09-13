use std::fs;
use std::path::Path;

use super::receipt::{
    remove_receipted_runtime_directory, remove_verified_backup_directory,
    runtime_directory_matches, verify_runtime_directory_receipt,
};
use super::RuntimeDirectoryReceipt;
use crate::SetupError;

pub(super) fn recover_prepared_repair(
    destination: &Path,
    staging: &Path,
    backup: &Path,
    old_receipt: &RuntimeDirectoryReceipt,
    new_receipt: &RuntimeDirectoryReceipt,
) -> Result<(), SetupError> {
    if backup.exists() {
        verify_runtime_directory_receipt(backup, old_receipt)?;
        if destination.exists() {
            return Err(SetupError::CleanupUnknown(
                "prepared repair has both destination and backup".to_owned(),
            ));
        }
        fs::rename(backup, destination)?;
    }
    verify_runtime_directory_receipt(destination, old_receipt)?;
    if staging.exists() {
        remove_receipted_runtime_directory(staging, new_receipt)?;
    }
    Ok(())
}

pub(super) fn recover_unverified_repair(
    destination: &Path,
    staging: &Path,
    backup: &Path,
    old_receipt: &RuntimeDirectoryReceipt,
    new_receipt: &RuntimeDirectoryReceipt,
) -> Result<(), SetupError> {
    if !destination.exists() {
        verify_runtime_directory_receipt(backup, old_receipt)?;
        fs::rename(backup, destination)?;
        verify_runtime_directory_receipt(destination, old_receipt)?;
        if staging.exists() {
            remove_receipted_runtime_directory(staging, new_receipt)?;
        }
        return Ok(());
    }

    if runtime_directory_matches(destination, old_receipt)? {
        if backup.exists() {
            return Err(SetupError::CleanupUnknown(
                "unverified repair has duplicate old destination and backup".to_owned(),
            ));
        }
        if staging.exists() {
            remove_receipted_runtime_directory(staging, new_receipt)?;
        }
        return Ok(());
    }
    if !runtime_directory_matches(destination, new_receipt)? {
        return Err(SetupError::CleanupUnknown(
            "unverified repair destination matches neither old nor new receipt".to_owned(),
        ));
    }
    if !backup.exists() || staging.exists() {
        return Err(SetupError::CleanupUnknown(
            "unverified repair cannot safely restore its verified backup".to_owned(),
        ));
    }
    verify_runtime_directory_receipt(backup, old_receipt)?;
    fs::rename(destination, staging)?;
    verify_runtime_directory_receipt(staging, new_receipt)?;
    if let Err(error) = fs::rename(backup, destination) {
        return Err(SetupError::CleanupUnknown(format!(
            "verified repair backup could not be restored after quarantining new payload: {error}"
        )));
    }
    verify_runtime_directory_receipt(destination, old_receipt)?;
    remove_receipted_runtime_directory(staging, new_receipt)
}

pub(super) fn recover_verified_repair(
    destination: &Path,
    staging: &Path,
    backup: &Path,
    old_receipt: &RuntimeDirectoryReceipt,
    new_receipt: &RuntimeDirectoryReceipt,
) -> Result<(), SetupError> {
    if staging.exists() {
        return Err(SetupError::CleanupUnknown(
            "verified repair unexpectedly retained a staging directory".to_owned(),
        ));
    }
    verify_runtime_directory_receipt(destination, new_receipt)?;
    if backup.exists() {
        remove_verified_backup_directory(backup, old_receipt)?;
    }
    Ok(())
}
