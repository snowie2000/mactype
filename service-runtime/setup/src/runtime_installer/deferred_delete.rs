use std::fs;
use std::path::Path;

use crate::SetupError;

pub(super) fn remove_file_or_defer(path: &Path, operation: &'static str) -> Result<(), SetupError> {
    remove_or_defer(path, false).map_err(|error| error.at_machine_path(operation, path))
}

pub(super) fn remove_directory_or_defer(
    path: &Path,
    operation: &'static str,
) -> Result<(), SetupError> {
    remove_or_defer(path, true).map_err(|error| error.at_machine_path(operation, path))
}

pub(super) fn defer_directory(path: &Path, operation: &'static str) -> Result<(), SetupError> {
    defer_delete_after_reboot(path)
        .map_err(SetupError::Io)
        .map_err(|error| error.at_machine_path(operation, path))
}

fn remove_or_defer(path: &Path, directory: bool) -> Result<(), SetupError> {
    let removal = if directory {
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    };
    match removal {
        Ok(()) => Ok(()),
        Err(removal_error) => defer_delete_after_reboot(path).map_err(|defer_error| {
            SetupError::CleanupUnknown(format!(
                "verified runtime removal failed ({removal_error}) and could not be deferred until reboot ({defer_error})"
            ))
        }),
    }
}

#[cfg(windows)]
fn defer_delete_after_reboot(path: &Path) -> Result<(), std::io::Error> {
    mactype_service_platform::delay_delete_until_reboot(path)
}

#[cfg(not(windows))]
fn defer_delete_after_reboot(_path: &Path) -> Result<(), std::io::Error> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "deferred deletion is available only on Windows",
    ))
}
