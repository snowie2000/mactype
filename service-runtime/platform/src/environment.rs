//! Bounded environment-string expansion.

use std::io;
use std::ptr::null_mut;

use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;

use crate::wide::wide_null;

/// Expands environment-variable references while bounding the result in UTF-16
/// units, including its terminating NUL.
pub fn expand_environment_strings(value: &str, maximum_units: usize) -> io::Result<String> {
    if value.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "environment string contains a NUL",
        ));
    }
    let source = wide_null(value);
    // SAFETY: `source` is NUL-terminated; a null output and zero capacity form
    // the documented size probe.
    let required = unsafe { ExpandEnvironmentStringsW(source.as_ptr(), null_mut(), 0) };
    if required == 0 {
        return Err(io::Error::last_os_error());
    }
    if required as usize > maximum_units {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expanded environment string exceeds the bound",
        ));
    }
    let mut result = vec![0_u16; required as usize];
    // SAFETY: both strings are live and NUL-terminated; `result` is writable for
    // exactly `required` units, the capacity passed to Windows.
    let written = unsafe {
        ExpandEnvironmentStringsW(source.as_ptr(), result.as_mut_ptr(), result.len() as u32)
    };
    if written != required {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expanded environment string changed during the read",
        ));
    }
    result.pop();
    String::from_utf16(&result).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "expanded environment string is not valid UTF-16",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::expand_environment_strings;

    #[test]
    fn system_root_expands_within_the_bound() {
        let expanded = expand_environment_strings(r"%SystemRoot%\x", 32_768).unwrap();
        let bytes = expanded.as_bytes();
        assert!(bytes.len() >= 4);
        assert!(bytes[0].is_ascii_alphabetic() && bytes[1] == b':');
        assert!(expanded.ends_with(r"\x"));
    }
}
