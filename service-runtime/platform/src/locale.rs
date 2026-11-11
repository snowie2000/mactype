//! The machine's ANSI code page.

use std::io;

use windows_sys::Win32::Globalization::{
    GetLocaleInfoEx, LOCALE_IDEFAULTANSICODEPAGE, LOCALE_NAME_SYSTEM_DEFAULT, LOCALE_RETURN_NUMBER,
};

/// The ANSI code page of the system locale: the code page a process without
/// an `activeCodePage` manifest entry decodes narrow strings with. It is read
/// from the locale rather than from `GetACP`, which answers with UTF-8 in a
/// process whose own manifest selects it.
pub fn system_ansi_code_page() -> io::Result<u32> {
    let mut value = 0_u32;
    // SAFETY: with LOCALE_RETURN_NUMBER the output is one DWORD, passed as a
    // buffer of two UTF-16 units that `value` owns for the call's duration.
    let written = unsafe {
        GetLocaleInfoEx(
            LOCALE_NAME_SYSTEM_DEFAULT,
            LOCALE_IDEFAULTANSICODEPAGE | LOCALE_RETURN_NUMBER,
            (&raw mut value).cast::<u16>(),
            (size_of::<u32>() / size_of::<u16>()) as i32,
        )
    };
    if written == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::system_ansi_code_page;

    #[test]
    fn the_system_locale_names_a_code_page() {
        let code_page = system_ansi_code_page().unwrap();
        assert!(code_page != 0 && code_page != 65_001, "{code_page}");
    }
}
