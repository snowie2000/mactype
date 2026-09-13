//! Reading `.lnk` shortcuts through the shell.

use std::path::Path;

use windows::core::{IUnknown, Interface, PCWSTR};
use windows::Win32::System::Com::{
    CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink, SLGP_RAWPATH};

use crate::wide::{nul_terminated, wide_path};

const MAX_SHORTCUT_TEXT_UNITS: usize = 32_768;

/// The raw target path and argument string of a shortcut.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShortcutTarget {
    pub path: String,
    pub arguments: String,
}

/// Loads the shortcut at `path` and returns its raw target and arguments.
/// The caller must hold a COM apartment on this thread. Any COM failure or an
/// unterminated or non-UTF-16 field yields `None`: the shortcut is then
/// unreadable, which callers treat as unknown rather than clear.
pub fn read_shortcut(path: &Path) -> Option<ShortcutTarget> {
    // SAFETY: the class and interface identifiers are the shell's own
    // constants; no outer unknown is passed.
    let link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None::<&IUnknown>, CLSCTX_INPROC_SERVER) }.ok()?;
    let persist: IPersistFile = link.cast().ok()?;
    let wide = wide_path(path);
    // SAFETY: `wide` is NUL-terminated and outlives the call.
    unsafe { persist.Load(PCWSTR(wide.as_ptr()), STGM_READ) }.ok()?;

    let mut target = vec![0_u16; MAX_SHORTCUT_TEXT_UNITS];
    // SAFETY: `target` is a writable buffer whose length the binding passes
    // to the shell; no find-data structure is requested.
    unsafe { link.GetPath(&mut target, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32) }.ok()?;
    let path = nul_terminated(&target)?;

    let mut arguments = vec![0_u16; MAX_SHORTCUT_TEXT_UNITS];
    // SAFETY: `arguments` is a writable buffer whose length the binding
    // passes to the shell.
    unsafe { link.GetArguments(&mut arguments) }.ok()?;
    let arguments = nul_terminated(&arguments)?;
    Some(ShortcutTarget { path, arguments })
}

#[cfg(test)]
mod tests {
    use super::read_shortcut;
    use crate::com::{ComApartment, ComThreading};

    #[test]
    fn a_missing_shortcut_is_unreadable_rather_than_empty() {
        std::thread::spawn(|| {
            let _apartment = ComApartment::initialize(ComThreading::Apartment).unwrap();
            let directory = tempfile::tempdir().unwrap();
            assert!(read_shortcut(&directory.path().join("missing.lnk")).is_none());
        })
        .join()
        .unwrap();
    }
}
