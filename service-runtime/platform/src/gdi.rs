//! GDI queries used to inventory installed font families.

use std::io;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::LPARAM;
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, EnumFontFamiliesExW, DEFAULT_CHARSET, HDC, LF_FACESIZE, LOGFONTW,
    TEXTMETRICW,
};

struct OwnedDeviceContext(HDC);

impl Drop for OwnedDeviceContext {
    fn drop(&mut self) {
        // SAFETY: this guard uniquely owns the compatible DC returned by
        // CreateCompatibleDC and deletes it exactly once.
        unsafe { DeleteDC(self.0) };
    }
}

unsafe extern "system" fn collect_font_family(
    logical_font: *const LOGFONTW,
    _metrics: *const TEXTMETRICW,
    _font_type: u32,
    parameter: LPARAM,
) -> i32 {
    // SAFETY: `installed_font_families` passes a live, exclusively borrowed Vec;
    // GDI invokes callbacks synchronously and this is its only writer.
    let families = unsafe { &mut *(parameter as *mut Vec<String>) };
    // SAFETY: GDI supplies a live LOGFONTW pointer for the callback invocation.
    let face_name = unsafe { &(*logical_font).lfFaceName };
    let maximum = LF_FACESIZE as usize;
    let length = face_name
        .iter()
        .take(maximum)
        .position(|unit| *unit == 0)
        .unwrap_or(maximum);
    families.push(String::from_utf16_lossy(&face_name[..length]));
    1
}

/// Returns installed font family names in GDI enumeration order, including duplicates.
pub fn installed_font_families() -> io::Result<Vec<String>> {
    // SAFETY: a null source DC requests a memory DC compatible with the current
    // screen; the returned DC is immediately placed in its owned guard.
    let dc = unsafe { CreateCompatibleDC(null_mut()) };
    if dc.is_null() {
        return Err(io::Error::last_os_error());
    }
    let dc = OwnedDeviceContext(dc);
    let logical_font = LOGFONTW {
        lfCharSet: DEFAULT_CHARSET,
        ..Default::default()
    };
    let mut families = Vec::new();
    // SAFETY: the DC and LOGFONTW remain live through synchronous enumeration;
    // `families` is exclusively borrowed by a callback that always continues.
    let result = unsafe {
        EnumFontFamiliesExW(
            dc.0,
            &logical_font,
            Some(collect_font_family),
            (&mut families as *mut Vec<String>) as LPARAM,
            0,
        )
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(families)
}

#[cfg(test)]
mod tests {
    use super::installed_font_families;

    #[test]
    fn common_windows_font_is_installed() {
        let families = installed_font_families().unwrap();
        assert!(families.iter().any(|family| {
            family.eq_ignore_ascii_case("Arial") || family.eq_ignore_ascii_case("Segoe UI")
        }));
    }
}
