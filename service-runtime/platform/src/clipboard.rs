//! Publishing Unicode text to the Windows clipboard.

use std::io;
use std::ptr::{copy_nonoverlapping, null_mut};

use windows_sys::Win32::Foundation::{
    GetLastError, GlobalFree, SetLastError, ERROR_SUCCESS, HGLOBAL,
};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

struct ClipboardGuard;

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: this guard exists only after OpenClipboard succeeds and closes
        // that thread's one open clipboard exactly once.
        unsafe { CloseClipboard() };
    }
}

struct GlobalAllocation(Option<HGLOBAL>);

impl GlobalAllocation {
    fn handle(&self) -> HGLOBAL {
        self.0.expect("global allocation is still owned")
    }

    fn transfer(&mut self) {
        self.0 = None;
    }
}

impl Drop for GlobalAllocation {
    fn drop(&mut self) {
        if let Some(handle) = self.0 {
            // SAFETY: the guard still owns this movable global allocation because
            // SetClipboardData has not accepted it; this is its only free.
            unsafe { GlobalFree(handle) };
        }
    }
}

/// Replaces the clipboard contents with NUL-terminated UTF-16 text.
pub fn set_clipboard_unicode_text(text: &str) -> io::Result<()> {
    // SAFETY: a null owner is permitted and no caller-owned pointers are passed.
    if unsafe { OpenClipboard(null_mut()) } == 0 {
        return Err(step_error("open", io::Error::last_os_error()));
    }
    let _clipboard = ClipboardGuard;
    // SAFETY: this thread owns the open clipboard for the guard's lifetime.
    if unsafe { EmptyClipboard() } == 0 {
        return Err(step_error("clear", io::Error::last_os_error()));
    }

    let mut units = text.encode_utf16().collect::<Vec<_>>();
    units.push(0);
    let byte_length = units.len().checked_mul(size_of::<u16>()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "allocate clipboard text failed: text length overflow",
        )
    })?;
    // SAFETY: `byte_length` is the checked byte size of the UTF-16 allocation.
    let allocation = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_length) };
    if allocation.is_null() {
        return Err(step_error("allocate", io::Error::last_os_error()));
    }
    let mut allocation = GlobalAllocation(Some(allocation));
    // SAFETY: the guard owns a live global allocation and keeps it alive and
    // locked until the UTF-16 copy has completed.
    let destination = unsafe { GlobalLock(allocation.handle()) }.cast::<u16>();
    if destination.is_null() {
        return Err(step_error("lock", io::Error::last_os_error()));
    }
    // SAFETY: `destination` points to `byte_length` writable bytes and `units`
    // contains exactly that many initialized UTF-16 bytes without overlap.
    unsafe { copy_nonoverlapping(units.as_ptr(), destination, units.len()) };
    // SAFETY: clearing last-error distinguishes a successful final unlock from
    // an actual GlobalUnlock failure when its return value is zero.
    unsafe { SetLastError(ERROR_SUCCESS) };
    // SAFETY: the allocation has one matching successful GlobalLock above.
    let unlocked = unsafe { GlobalUnlock(allocation.handle()) };
    if unlocked == 0 {
        // SAFETY: GetLastError reads thread-local state set by GlobalUnlock.
        let code = unsafe { GetLastError() };
        if code != ERROR_SUCCESS {
            return Err(step_error(
                "publish",
                io::Error::from_raw_os_error(code as i32),
            ));
        }
    }
    // SAFETY: the clipboard is open and empty, and `allocation` is an unlocked
    // GMEM_MOVEABLE block containing NUL-terminated UTF-16 text. Ownership moves
    // to the clipboard only when SetClipboardData succeeds.
    if unsafe { SetClipboardData(u32::from(CF_UNICODETEXT), allocation.handle()) }.is_null() {
        return Err(step_error("publish", io::Error::last_os_error()));
    }
    allocation.transfer();
    Ok(())
}

#[derive(Debug)]
struct ClipboardStepError {
    step: &'static str,
    source: io::Error,
}

impl std::fmt::Display for ClipboardStepError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} clipboard text failed: {}",
            self.step, self.source
        )
    }
}

impl std::error::Error for ClipboardStepError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

fn step_error(step: &'static str, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        ClipboardStepError {
            step,
            source: error,
        },
    )
}

#[cfg(test)]
fn clipboard_error_code(error: &io::Error) -> Option<i32> {
    error.raw_os_error().or_else(|| {
        error
            .get_ref()?
            .downcast_ref::<ClipboardStepError>()?
            .source
            .raw_os_error()
    })
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::ptr::null_mut;
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ACCESS_DENIED};
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

    use super::{clipboard_error_code, set_clipboard_unicode_text};
    use crate::wide::bounded_string;

    #[test]
    fn unicode_text_round_trips_through_the_clipboard() {
        std::thread::spawn(|| {
            let expected = "MacType clipboard test 한글";
            let mut last_error = None;
            for attempt in 0..5 {
                match set_clipboard_unicode_text(expected) {
                    Ok(()) => match read_clipboard_text() {
                        Ok(actual) => {
                            assert_eq!(actual, expected);
                            return;
                        }
                        Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {
                            last_error = Some(error);
                            if attempt < 4 {
                                std::thread::sleep(Duration::from_millis(50));
                            }
                        }
                        Err(error) => panic!("reading clipboard text failed: {error}"),
                    },
                    Err(error)
                        if clipboard_error_code(&error) == Some(ERROR_ACCESS_DENIED as i32) =>
                    {
                        last_error = Some(error);
                        if attempt < 4 {
                            std::thread::sleep(Duration::from_millis(50));
                        }
                    }
                    Err(error) => panic!("setting clipboard text failed: {error}"),
                }
            }
            panic!("setting clipboard text failed: {}", last_error.unwrap());
        })
        .join()
        .unwrap();
    }

    fn read_clipboard_text() -> io::Result<String> {
        // SAFETY: a null owner is permitted and no pointers are retained.
        if unsafe { OpenClipboard(null_mut()) } == 0 {
            // SAFETY: GetLastError immediately preserves OpenClipboard's failure code.
            let code = unsafe { GetLastError() };
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        struct Close;
        impl Drop for Close {
            fn drop(&mut self) {
                // SAFETY: the helper creates this guard only after opening the clipboard.
                unsafe { CloseClipboard() };
            }
        }
        let _close = Close;
        // SAFETY: the clipboard is open and the returned handle remains owned by it.
        let handle = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT)) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the clipboard owns a live global allocation while it remains open.
        let pointer = unsafe { GlobalLock(handle) }.cast::<u16>();
        if pointer.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: CF_UNICODETEXT supplies a NUL-terminated UTF-16 string; the test
        // imposes a conservative maximum before decoding it.
        let text = unsafe { bounded_string(pointer, 32_768) }
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "clipboard text is invalid"));
        // SAFETY: this matches the successful GlobalLock above.
        unsafe { GlobalUnlock(handle) };
        text
    }
}
