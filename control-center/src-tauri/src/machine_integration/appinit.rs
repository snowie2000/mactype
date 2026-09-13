pub(super) fn appinit_view_conflict(
    enabled: Result<bool, ()>,
    value: Result<Option<Vec<u16>>, ()>,
) -> Result<bool, ()> {
    let enabled = enabled?;
    if !enabled {
        return Ok(false);
    }
    let value = value?;
    mactype_service_contract::appinit_mactype_conflict(true, value.as_deref()).map_err(|_| ())
}

#[cfg(windows)]
const APPINIT_SUBKEY: &str = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Windows";

#[cfg(windows)]
fn read_appinit_enabled(view: mactype_service_platform::RegistryView) -> Result<bool, ()> {
    use mactype_service_platform::{RegistryKey, RegistryRoot};

    let Some(key) =
        RegistryKey::open(RegistryRoot::LocalMachine, APPINIT_SUBKEY, view).map_err(|_| ())?
    else {
        return Ok(false);
    };
    Ok(key
        .read_dword("LoadAppInit_DLLs")
        .map_err(|_| ())?
        .unwrap_or(0)
        != 0)
}

#[cfg(windows)]
fn read_appinit_dlls(view: mactype_service_platform::RegistryView) -> Result<Option<Vec<u16>>, ()> {
    use mactype_service_platform::{RegistryKey, RegistryRoot};

    let Some(key) =
        RegistryKey::open(RegistryRoot::LocalMachine, APPINIT_SUBKEY, view).map_err(|_| ())?
    else {
        return Ok(None);
    };
    // RegGetValueW appended the terminator a stored REG_SZ may lack; the
    // contract parser still requires it.
    Ok(key
        .read_string_units("AppInit_DLLs")
        .map_err(|_| ())?
        .map(|mut units| {
            if units.last() != Some(&0) {
                units.push(0);
            }
            units
        }))
}

#[cfg(windows)]
pub(super) fn appinit_conflict() -> Result<bool, String> {
    use mactype_service_platform::RegistryView;

    [RegistryView::Native64, RegistryView::Native32]
        .into_iter()
        .try_fold(false, |conflict, view| {
            appinit_view_conflict(read_appinit_enabled(view), read_appinit_dlls(view))
                .map(|current| conflict || current)
                .map_err(|_| "AppInit registry state could not be verified".to_owned())
        })
}

pub(crate) fn registry_conflict_detected() -> bool {
    appinit_conflict().unwrap_or(true)
}

#[cfg(not(windows))]
pub(super) fn appinit_conflict() -> Result<bool, String> {
    Ok(false)
}
