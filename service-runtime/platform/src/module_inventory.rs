//! Whether a process already has a particular DLL loaded.
//!
//! The service asks this before it launches the injection helper: a target the
//! in-process relay has already reached needs no helper at all, and skipping
//! the launch saves a process creation and the several hundred milliseconds it
//! takes to arrive at the same answer.

use std::io;
use std::mem::size_of;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::ProcessStatus::{
    EnumProcessModulesEx, GetModuleFileNameExW, LIST_MODULES_ALL,
};

use crate::process::{Process, ProcessAccess};

/// Modules are counted in the low hundreds even in a heavily instrumented
/// process; this bound keeps one enumeration to a single fixed allocation.
const MAX_ENUMERATED_MODULES: usize = 4_096;
const MAX_MODULE_PATH_UNITS: usize = 32_768;

/// What the module list said about one specific DLL path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModulePresence {
    /// Exactly that path is loaded in the target right now.
    Loaded,
    /// The module list was read in full and that path was not in it.
    Absent,
    /// The question could not be answered: the target could not be opened,
    /// its identity no longer matches, or the list could not be read.
    Unknown,
}

/// Whether the process identified by `pid` and `creation_time` has `module`
/// loaded.
///
/// The creation time is re-checked against the open handle before anything is
/// read, so a PID the kernel has handed to a different process cannot be
/// mistaken for the target. Every failure answers [`ModulePresence::Unknown`],
/// which leaves the caller doing exactly what it did before it asked.
pub fn process_module_presence(pid: u32, creation_time: u64, module: &Path) -> ModulePresence {
    let Some(expected) = module.to_str().and_then(normalized_module_path) else {
        return ModulePresence::Unknown;
    };
    let Ok(process) = Process::open(pid, ProcessAccess::ModuleInventory) else {
        return ModulePresence::Unknown;
    };
    if process.creation_time().ok() != Some(creation_time) {
        return ModulePresence::Unknown;
    }
    let Ok(loaded) = module_paths(&process) else {
        return ModulePresence::Unknown;
    };
    for path in loaded {
        let Some(normalized) = normalized_module_path(&path) else {
            return ModulePresence::Unknown;
        };
        if normalized == expected {
            return ModulePresence::Loaded;
        }
    }
    ModulePresence::Absent
}

/// Every module path loaded in `process`, native and WOW64 alike.
fn module_paths(process: &Process) -> io::Result<Vec<String>> {
    let mut modules = vec![std::ptr::null_mut::<std::ffi::c_void>(); MAX_ENUMERATED_MODULES];
    let capacity = (modules.len() * size_of::<HMODULE>()) as u32;
    let mut needed = 0_u32;
    // SAFETY: the handle is live and carries the query and read rights this
    // call documents; `modules` is writable for exactly `capacity` bytes, and
    // `needed` is a local the call fills with the bytes it would have used.
    if unsafe {
        EnumProcessModulesEx(
            process.handle().as_raw(),
            modules.as_mut_ptr(),
            capacity,
            &mut needed,
            LIST_MODULES_ALL,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if needed > capacity || !(needed as usize).is_multiple_of(size_of::<HMODULE>()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the process module list does not fill whole module handles",
        ));
    }
    modules.truncate(needed as usize / size_of::<HMODULE>());

    let mut buffer = vec![0_u16; MAX_MODULE_PATH_UNITS];
    let mut paths = Vec::with_capacity(modules.len());
    for module in modules {
        // SAFETY: the handle is live, `module` came from the enumeration just
        // above, and `buffer` is writable for the unit count passed in.
        let length = unsafe {
            GetModuleFileNameExW(
                process.handle().as_raw(),
                module,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
            )
        };
        if length == 0 || length as usize >= buffer.len() {
            return Err(io::Error::last_os_error());
        }
        let path = std::ffi::OsString::from_wide(&buffer[..length as usize]);
        match path.into_string() {
            Ok(path) => paths.push(path),
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "a loaded module path is not valid Unicode",
                ));
            }
        }
    }
    Ok(paths)
}

/// Reduces a Windows path to the form two paths can be compared in: the
/// namespace prefix removed, `.` and `..` resolved, separators unified, and
/// the whole thing lowercased, because the file system does not distinguish
/// case. A relative path has no comparable form and answers `None`.
pub(crate) fn normalized_module_path(path: &str) -> Option<String> {
    if path.is_empty() || path.contains('\0') {
        return None;
    }
    let stripped = strip_namespace_prefix(path);
    let (root, rest) = split_root(&stripped)?;
    let mut components: Vec<&str> = Vec::new();
    for component in rest.split(['\\', '/']) {
        match component {
            "" | "." => {}
            ".." => {
                components.pop()?;
            }
            name => components.push(name),
        }
    }
    let mut normalized = root;
    normalized.push_str(&components.join("\\"));
    Some(normalized.to_lowercase())
}

/// Turns the two device-namespace spellings Windows reports module paths in
/// back into the ordinary form, so `\\?\C:\x` and `C:\x` compare equal.
fn strip_namespace_prefix(path: &str) -> String {
    for unc in [r"\\?\UNC\", r"\??\UNC\"] {
        if starts_with_ignore_case(path, unc) {
            return format!(r"\\{}", &path[unc.len()..]);
        }
    }
    for prefix in [r"\\?\", r"\??\"] {
        if starts_with_ignore_case(path, prefix) {
            return path[prefix.len()..].to_owned();
        }
    }
    path.to_owned()
}

/// Splits an absolute path into the part that must stay verbatim (a drive
/// with its separator, or the two leading separators of a UNC name) and the
/// components after it.
fn split_root(path: &str) -> Option<(String, &str)> {
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'\\' && bytes[1] == b'\\' {
        return Some((r"\\".to_owned(), &path[2..]));
    }
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        return Some((format!("{}:\\", bytes[0] as char), &path[3..]));
    }
    None
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix)
}

#[cfg(test)]
mod tests {
    use super::{ModulePresence, normalized_module_path, process_module_presence};

    #[test]
    fn the_two_device_namespace_spellings_reduce_to_the_ordinary_path() {
        let plain = normalized_module_path(r"C:\Program Files\MacType\MacType64.dll").unwrap();

        assert_eq!(
            normalized_module_path(r"\\?\C:\Program Files\MacType\MacType64.dll").unwrap(),
            plain
        );
        assert_eq!(
            normalized_module_path(r"\??\C:\Program Files\MacType\MacType64.dll").unwrap(),
            plain
        );
    }

    #[test]
    fn case_and_redundant_components_do_not_make_two_paths_differ() {
        let expected = normalized_module_path(r"C:\runtime\0.2.0\MacType64.dll").unwrap();

        assert_eq!(
            normalized_module_path(r"c:\RUNTIME\0.2.0\mactype64.DLL").unwrap(),
            expected
        );
        assert_eq!(
            normalized_module_path(r"C:\runtime\.\0.2.0\MacType64.dll").unwrap(),
            expected
        );
        assert_eq!(
            normalized_module_path(r"C:\runtime\0.3.0\..\0.2.0\MacType64.dll").unwrap(),
            expected
        );
        assert_eq!(
            normalized_module_path(r"C:/runtime/0.2.0/MacType64.dll").unwrap(),
            expected
        );
    }

    #[test]
    fn a_unc_path_keeps_both_leading_separators() {
        assert_eq!(
            normalized_module_path(r"\\?\UNC\server\share\MacType.dll").unwrap(),
            r"\\server\share\mactype.dll"
        );
    }

    #[test]
    fn a_path_without_a_root_has_no_comparable_form() {
        assert_eq!(normalized_module_path(""), None);
        assert_eq!(normalized_module_path("MacType64.dll"), None);
        assert_eq!(normalized_module_path(r"runtime\MacType64.dll"), None);
        assert_eq!(normalized_module_path(r"C:\..\..\x.dll"), None);
        assert_eq!(normalized_module_path("C:\\x\0.dll"), None);
    }

    #[test]
    fn this_process_reports_its_own_executable_as_loaded_and_a_stranger_as_absent() {
        let pid = std::process::id();
        let process =
            super::Process::open(pid, super::ProcessAccess::QueryLimited).expect("own process");
        let creation_time = process.creation_time().unwrap();
        let image = process.image_path().unwrap();

        assert_eq!(
            process_module_presence(pid, creation_time, &image),
            ModulePresence::Loaded
        );
        assert_eq!(
            process_module_presence(
                pid,
                creation_time,
                std::path::Path::new(r"C:\nowhere\MacType64.dll")
            ),
            ModulePresence::Absent
        );
    }

    #[test]
    fn a_creation_time_that_no_longer_matches_answers_unknown() {
        let pid = std::process::id();
        let process =
            super::Process::open(pid, super::ProcessAccess::QueryLimited).expect("own process");
        let image = process.image_path().unwrap();

        assert_eq!(
            process_module_presence(pid, process.creation_time().unwrap() + 1, &image),
            ModulePresence::Unknown
        );
    }
}
