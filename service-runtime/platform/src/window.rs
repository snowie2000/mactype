//! Top-level window inventory and asynchronous messages.

use std::io;

use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    RegisterWindowMessageW, SendNotifyMessageW, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
};

use crate::wide::wide_null;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct WindowHandle(isize);

impl WindowHandle {
    pub fn raw(self) -> isize {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TopLevelWindow {
    pub window: WindowHandle,
    pub process_id: u32,
    pub visible: bool,
    pub tool_window: bool,
    pub title: Option<String>,
}

struct EnumerationContext {
    windows: Vec<TopLevelWindow>,
    maximum_title_units: usize,
}

unsafe extern "system" fn collect_window(window: HWND, parameter: LPARAM) -> i32 {
    // SAFETY: `top_level_windows` passes a live, exclusively borrowed context;
    // EnumWindows invokes callbacks synchronously and this is its only writer.
    let context = unsafe { &mut *(parameter as *mut EnumerationContext) };
    let mut process_id = 0;
    // SAFETY: `window` comes from EnumWindows and `process_id` is a local output.
    unsafe { GetWindowThreadProcessId(window, &mut process_id) };
    // SAFETY: `window` comes from EnumWindows; neither query dereferences caller
    // memory and both are valid for any top-level window handle.
    let visible = unsafe { IsWindowVisible(window) } != 0;
    // SAFETY: `window` comes from EnumWindows and GWL_EXSTYLE is a valid index.
    let tool_window = unsafe { GetWindowLongW(window, GWL_EXSTYLE) } as u32 & WS_EX_TOOLWINDOW != 0;
    let title = if context.maximum_title_units == 0 {
        None
    } else {
        let capacity = context.maximum_title_units.min(i32::MAX as usize);
        let mut buffer = vec![0_u16; capacity];
        // SAFETY: `window` comes from EnumWindows and `buffer` is writable for
        // the capacity passed to GetWindowTextW.
        let length = unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), capacity as i32) };
        (length > 0).then(|| String::from_utf16_lossy(&buffer[..length as usize]))
    };
    context.windows.push(TopLevelWindow {
        window: WindowHandle(window as isize),
        process_id,
        visible,
        tool_window,
        title,
    });
    1
}

/// Enumerates top-level windows in the Z order supplied by `EnumWindows`.
pub fn top_level_windows(maximum_title_units: usize) -> io::Result<Vec<TopLevelWindow>> {
    if maximum_title_units > i32::MAX as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "window title bound exceeds the Win32 capacity type",
        ));
    }
    let mut context = EnumerationContext {
        windows: Vec::new(),
        maximum_title_units,
    };
    // SAFETY: `context` outlives synchronous enumeration and the callback is its
    // only writer until EnumWindows returns.
    if unsafe {
        EnumWindows(
            Some(collect_window),
            (&mut context as *mut EnumerationContext) as LPARAM,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(context.windows)
}

pub fn register_window_message(name: &str) -> io::Result<u32> {
    if name.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "window message name contains a NUL",
        ));
    }
    let name = wide_null(name);
    // SAFETY: `name` is NUL-terminated and remains live for the call.
    let message = unsafe { RegisterWindowMessageW(name.as_ptr()) };
    if message == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(message)
}

pub fn send_notify_message(window: WindowHandle, message: u32) -> io::Result<()> {
    // SAFETY: WindowHandle stores an HWND obtained from the window subsystem;
    // this asynchronous message carries no caller-owned pointer arguments.
    if unsafe { SendNotifyMessageW(window.0 as HWND, message, 0, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{register_window_message, top_level_windows};

    #[test]
    fn top_level_windows_enumerate_with_nonzero_handles() {
        // A hosted runner may have an empty desktop, so the list may be empty;
        // what must hold is that every entry names a real window.
        let windows = top_level_windows(512).unwrap();
        assert!(windows.iter().all(|entry| entry.window.raw() != 0));
    }

    #[test]
    fn registered_message_is_in_the_system_defined_range() {
        let message = register_window_message("MacType_Exit_Notify").unwrap();
        assert!((0xC000..=0xFFFF).contains(&message));
    }
}
