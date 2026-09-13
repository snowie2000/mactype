//! Known-folder lookups.

use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr::null_mut;

use windows_sys::core::GUID;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows_sys::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_ProgramData, FOLDERID_ProgramFiles, FOLDERID_Startup,
    FOLDERID_Windows, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
};

use crate::wide::bounded_units;

const MAX_FOLDER_PATH_UNITS: usize = 32_768;

/// The folders the service resolves. Only these fixed values exist, so a
/// caller cannot ask the shell for an arbitrary folder identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnownFolder {
    ProgramFiles,
    ProgramData,
    LocalAppData,
    Windows,
    /// The current user's Startup folder; needs a COM apartment on the thread.
    Startup,
}

impl KnownFolder {
    fn identifier(self) -> &'static GUID {
        match self {
            Self::ProgramFiles => &FOLDERID_ProgramFiles,
            Self::ProgramData => &FOLDERID_ProgramData,
            Self::LocalAppData => &FOLDERID_LocalAppData,
            Self::Windows => &FOLDERID_Windows,
            Self::Startup => &FOLDERID_Startup,
        }
    }
}

/// Resolves `folder` to its current path.
pub fn known_folder_path(folder: KnownFolder) -> io::Result<PathBuf> {
    let mut raw = null_mut();
    // SAFETY: the identifier is a static GUID, the token is null, and `raw`
    // is a live out pointer that receives a CoTaskMemAlloc string on success.
    let result = unsafe {
        SHGetKnownFolderPath(
            folder.identifier(),
            KF_FLAG_DEFAULT as u32,
            null_mut(),
            &mut raw,
        )
    };
    if result < 0 || raw.is_null() {
        return Err(io::Error::from_raw_os_error(result));
    }
    // SAFETY: the shell returned a NUL-terminated string in `raw`; it stays
    // valid until the CoTaskMemFree below.
    let path = unsafe { bounded_units(raw, MAX_FOLDER_PATH_UNITS) }
        .map(|units| PathBuf::from(std::ffi::OsString::from_wide(units)));
    // SAFETY: `raw` was allocated by the shell for this call and is freed once.
    unsafe { CoTaskMemFree(raw as *const c_void) };
    path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "the shell returned an unterminated known folder path",
        )
    })
}

/// Returns the system directory path reported by Windows.
pub fn system_directory() -> io::Result<PathBuf> {
    let mut stack = [0_u16; 260];
    // SAFETY: `stack` is writable for the capacity passed alongside it.
    let length = unsafe { GetSystemDirectoryW(stack.as_mut_ptr(), stack.len() as u32) };
    if length == 0 {
        return Err(io::Error::last_os_error());
    }
    if (length as usize) < stack.len() {
        return Ok(PathBuf::from(std::ffi::OsString::from_wide(
            &stack[..length as usize],
        )));
    }

    let capacity = (length as usize).checked_add(1).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "system directory is too long")
    })?;
    let mut buffer = vec![0_u16; capacity];
    // SAFETY: `buffer` is writable for the capacity passed alongside it.
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
    if length == 0 {
        return Err(io::Error::last_os_error());
    }
    if length as usize >= buffer.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "system directory changed while it was being read",
        ));
    }
    Ok(PathBuf::from(std::ffi::OsString::from_wide(
        &buffer[..length as usize],
    )))
}

#[cfg(test)]
mod tests {
    use super::{known_folder_path, system_directory, KnownFolder};

    #[test]
    fn machine_folders_resolve_to_absolute_directories() {
        for folder in [
            KnownFolder::ProgramFiles,
            KnownFolder::ProgramData,
            KnownFolder::Windows,
        ] {
            let path = known_folder_path(folder).unwrap();
            assert!(path.is_absolute(), "{path:?}");
            assert!(path.is_dir(), "{path:?}");
        }
    }

    #[test]
    fn local_app_data_and_system_directory_resolve() {
        assert!(known_folder_path(KnownFolder::LocalAppData)
            .unwrap()
            .is_dir());
        let system = system_directory().unwrap();
        assert!(system
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("System32")));
    }
}
