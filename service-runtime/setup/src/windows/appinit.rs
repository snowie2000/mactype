use mactype_service_contract::appinit_mactype_conflict;
use mactype_service_platform::{RegistryKey, RegistryRoot, RegistryView};

use crate::ConflictObservation;

const APPINIT_KEY: &str = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Windows";
const MAX_APPINIT_BYTES: usize = 64 * 1024;

pub fn observe_conflict() -> ConflictObservation {
    match [RegistryView::Native64, RegistryView::Native32]
        .into_iter()
        .try_fold(false, |conflict, view| {
            observe_view_with(read_enabled(view), || read_dlls(view))
                .map(|current| conflict || current)
        }) {
        Ok(true) => ConflictObservation::Detected,
        Ok(false) => ConflictObservation::Clear,
        Err(()) => ConflictObservation::Unknown,
    }
}

fn observe_view_with<F>(enabled: Result<bool, ()>, read_dlls: F) -> Result<bool, ()>
where
    F: FnOnce() -> Result<Option<Vec<u16>>, ()>,
{
    if !enabled? {
        return Ok(false);
    }

    let value = read_dlls()?;
    appinit_mactype_conflict(true, value.as_deref()).map_err(|_| ())
}

/// `Ok(None)` when the key is absent in this view, which reads as "AppInit is
/// not configured" rather than as an unknown state.
fn open_appinit_key(view: RegistryView) -> Result<Option<RegistryKey>, ()> {
    RegistryKey::open(RegistryRoot::LocalMachine, APPINIT_KEY, view).map_err(|_| ())
}

fn read_enabled(view: RegistryView) -> Result<bool, ()> {
    let Some(key) = open_appinit_key(view)? else {
        return Ok(false);
    };
    match key.read_dword("LoadAppInit_DLLs") {
        Ok(Some(enabled)) => Ok(enabled != 0),
        Ok(None) => Ok(false),
        Err(_) => Err(()),
    }
}

fn read_dlls(view: RegistryView) -> Result<Option<Vec<u16>>, ()> {
    let Some(key) = open_appinit_key(view)? else {
        return Ok(None);
    };
    let mut units = match key.read_string_units("AppInit_DLLs") {
        Ok(Some(units)) => units,
        Ok(None) => return Ok(None),
        Err(_) => return Err(()),
    };
    // The contract parser expects the NUL terminator RegGetValueW used to
    // guarantee; a raw registry string may have been stored without one.
    if units.last() != Some(&0) {
        units.push(0);
    }
    if units.len() * 2 > MAX_APPINIT_BYTES {
        return Err(());
    }
    Ok(Some(units))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::observe_view_with;

    #[test]
    fn disabled_appinit_does_not_read_or_validate_the_dll_list() {
        let read = Cell::new(false);

        let observation = observe_view_with(Ok(false), || {
            read.set(true);
            Err(())
        });

        assert_eq!(observation, Ok(false));
        assert!(!read.get());
    }

    #[test]
    fn enabled_appinit_preserves_dll_read_failures_as_unknown() {
        assert_eq!(observe_view_with(Ok(true), || Err(())), Err(()));
    }
}
