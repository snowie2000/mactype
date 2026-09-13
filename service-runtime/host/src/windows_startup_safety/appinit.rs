use std::io;

use mactype_service_contract::{appinit_mactype_conflict, StructuredServiceError};
use mactype_service_platform::{RegistryKey, RegistryRoot, RegistryView};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};

const APPINIT_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Windows";
const APPINIT_ENABLED_VALUE: &str = "LoadAppInit_DLLs";
const APPINIT_DLLS_VALUE: &str = "AppInit_DLLs";

pub(super) fn mactype_enabled(view: RegistryView) -> Result<bool, StructuredServiceError> {
    let key = RegistryKey::open(RegistryRoot::LocalMachine, APPINIT_KEY, view)
        .map_err(|error| {
            service_error(
                "appinit-inspection-failed",
                "an AppInit registry view could not be opened",
                registry_status(&error),
            )
        })?
        .ok_or_else(|| {
            // The Windows key always exists; its absence is reported the way
            // the raw open reported it.
            service_error(
                "appinit-inspection-failed",
                "an AppInit registry view could not be opened",
                Some(ERROR_FILE_NOT_FOUND as i32),
            )
        })?;
    if !read_enabled(&key)? {
        return Ok(false);
    }
    let value = read_dlls(&key)?;
    appinit_mactype_conflict(true, value.as_deref()).map_err(|_| {
        service_error(
            "appinit-inspection-failed",
            "the enabled AppInit_DLLs value is malformed",
            None,
        )
    })
}

fn read_enabled(key: &RegistryKey) -> Result<bool, StructuredServiceError> {
    let value = key.read_dword(APPINIT_ENABLED_VALUE).map_err(|error| {
        service_error(
            "appinit-inspection-failed",
            "the AppInit enable flag could not be read as a DWORD",
            registry_status(&error),
        )
    })?;
    Ok(value.is_some_and(|value| value != 0))
}

fn read_dlls(key: &RegistryKey) -> Result<Option<Vec<u16>>, StructuredServiceError> {
    key.read_string_units(APPINIT_DLLS_VALUE).map_err(|error| {
        service_error(
            "appinit-inspection-failed",
            "the enabled AppInit_DLLs value type or size is invalid",
            registry_status(&error),
        )
    })
}

/// The registry status behind a platform error: the Win32 code it carries,
/// or `ERROR_SUCCESS` when the query itself succeeded and the value merely
/// had the wrong type or size, which is the status the raw query reported.
fn registry_status(error: &io::Error) -> Option<i32> {
    Some(error.raw_os_error().unwrap_or(ERROR_SUCCESS as i32))
}

fn service_error(code: &str, message: &str, win32_error: Option<i32>) -> StructuredServiceError {
    StructuredServiceError {
        code: code.to_owned(),
        message: message.to_owned(),
        win32_error: win32_error.map(|code| code as u32),
    }
}
