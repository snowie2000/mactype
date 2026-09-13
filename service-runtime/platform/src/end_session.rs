use std::{io, sync::OnceLock};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{WM_ENDSESSION, WM_NCDESTROY},
    },
};

const END_SESSION_SUBCLASS_ID: usize = 0x4d54_4548;
static BEFORE_EXIT: OnceLock<fn()> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndSessionAction {
    Exit,
    PassThrough,
}

pub fn end_session_action(message: u32, wparam: usize) -> EndSessionAction {
    if message == WM_ENDSESSION && wparam != 0 {
        EndSessionAction::Exit
    } else {
        EndSessionAction::PassThrough
    }
}

/// Installs the exit hook on a window owned by the calling thread.
///
/// The subclassing helpers do not support cross-thread installation.
pub fn install_end_session_exit_hook(hwnd: isize, before_exit: fn()) -> io::Result<()> {
    register_callback(&BEFORE_EXIT, before_exit)?;
    // SAFETY: The caller supplies a live HWND owned by the current thread. The callback and
    // fixed ID remain valid for the process lifetime, and no reference data is dereferenced.
    let installed = unsafe {
        SetWindowSubclass(
            hwnd as HWND,
            Some(end_session_subclass_proc),
            END_SESSION_SUBCLASS_ID,
            0,
        )
    };
    if installed == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn register_callback(registration: &OnceLock<fn()>, callback: fn()) -> io::Result<()> {
    match registration.get() {
        Some(registered) if *registered as *const () == callback as *const () => Ok(()),
        Some(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "an end-session exit callback is already registered",
        )),
        None => match registration.set(callback) {
            Ok(()) => Ok(()),
            Err(callback) => register_callback(registration, callback),
        },
    }
}

unsafe extern "system" fn end_session_subclass_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    _reference_data: usize,
) -> LRESULT {
    if end_session_action(message, wparam) == EndSessionAction::Exit {
        if let Some(before_exit) = BEFORE_EXIT.get() {
            before_exit();
        }
        std::process::exit(0);
    }

    if message == WM_NCDESTROY {
        // SAFETY: Windows invoked this callback for `hwnd`, and the callback/ID pair exactly
        // matches the pair installed by `install_end_session_exit_hook` on the same thread.
        unsafe {
            RemoveWindowSubclass(
                hwnd,
                Some(end_session_subclass_proc),
                END_SESSION_SUBCLASS_ID,
            )
        };
    }

    // SAFETY: Windows invoked this function as a subclass callback with the supplied message
    // arguments. Delegation keeps the remaining subclass chain and original procedure intact.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use windows_sys::Win32::UI::WindowsAndMessaging::WM_QUERYENDSESSION;

    static CALLBACK_RESULT: AtomicUsize = AtomicUsize::new(0);

    fn callback_one() {
        CALLBACK_RESULT.store(1, Ordering::Relaxed);
    }

    fn callback_two() {
        CALLBACK_RESULT.store(2, Ordering::Relaxed);
    }

    #[test]
    fn confirmed_end_session_exits() {
        assert_eq!(end_session_action(WM_ENDSESSION, 1), EndSessionAction::Exit);
    }

    #[test]
    fn cancelled_end_session_passes_through() {
        assert_eq!(
            end_session_action(WM_ENDSESSION, 0),
            EndSessionAction::PassThrough
        );
    }

    #[test]
    fn query_end_session_passes_through() {
        assert_eq!(
            end_session_action(WM_QUERYENDSESSION, 1),
            EndSessionAction::PassThrough
        );
    }

    #[test]
    fn unrelated_message_passes_through() {
        assert_eq!(end_session_action(0, 1), EndSessionAction::PassThrough);
    }

    #[test]
    fn registering_the_same_callback_is_idempotent() {
        let registration = OnceLock::new();
        assert!(register_callback(&registration, callback_one).is_ok());
        assert!(register_callback(&registration, callback_one).is_ok());
    }

    #[test]
    fn registering_a_different_callback_is_rejected() {
        let registration = OnceLock::new();
        register_callback(&registration, callback_one).unwrap();
        let error = register_callback(&registration, callback_two).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    }
}
