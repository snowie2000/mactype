use std::env;

#[cfg(windows)]
const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const VALUE_NAME: &str = "MacTypeControlCenter";
#[cfg(windows)]
const MAX_VALUE_BYTES: usize = 64 * 1024;

#[cfg(windows)]
fn read_registry_string() -> Option<String> {
    use mactype_service_platform::{RegistryKey, RegistryRoot, RegistryView};
    use windows_sys::Win32::System::Registry::REG_SZ;

    let key = RegistryKey::open(
        RegistryRoot::CurrentUser,
        RUN_SUBKEY,
        RegistryView::Native64,
    )
    .ok()??;
    let value = key.read_raw(VALUE_NAME, MAX_VALUE_BYTES).ok()??;
    if value.kind != REG_SZ || value.bytes.len() < 2 {
        return None;
    }
    let mut units = value
        .bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    units.truncate(
        units
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(units.len()),
    );
    Some(String::from_utf16_lossy(&units))
}

#[cfg(windows)]
pub(super) fn autostart_value() -> Option<String> {
    read_registry_string()
}

#[cfg(not(windows))]
pub(super) fn autostart_value() -> Option<String> {
    None
}

#[cfg(windows)]
pub(super) fn set_autostart(enabled: bool) -> Result<bool, String> {
    use mactype_service_platform::{DeleteValueOutcome, RegistryKey, RegistryRoot, RegistryView};

    let key = RegistryKey::open_writable(
        RegistryRoot::CurrentUser,
        RUN_SUBKEY,
        RegistryView::Native64,
    )
    .map_err(format_registry_error)?;
    if enabled {
        let key = key.ok_or_else(|| {
            "autostart registry update failed because the Run key is absent".to_owned()
        })?;
        let executable = env::current_exe().map_err(|error| error.to_string())?;
        let command = format!("\"{}\" --tray", executable.display());
        key.set_string(VALUE_NAME, &command)
            .map_err(format_registry_error)?;
    } else if let Some(key) = key {
        match key
            .delete_value(VALUE_NAME)
            .map_err(format_registry_error)?
        {
            DeleteValueOutcome::Deleted | DeleteValueOutcome::Absent => {}
        }
    }
    Ok(autostart_value().is_some())
}

#[cfg(windows)]
fn format_registry_error(error: std::io::Error) -> String {
    match error.raw_os_error() {
        Some(win32) => format!("autostart registry update failed with Windows error {win32}"),
        None => format!("autostart registry update failed: {error}"),
    }
}

#[cfg(not(windows))]
pub(super) fn set_autostart(_enabled: bool) -> Result<bool, String> {
    Err("autostart is supported only on Windows".to_owned())
}
