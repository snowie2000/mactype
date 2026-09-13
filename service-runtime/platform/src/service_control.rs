//! The service control dispatcher and handler trampolines.
//!
//! The SCM calls back into the process through `extern "system"` functions
//! that receive raw pointers. Both trampolines live here: they decode what the
//! SCM passed into plain values and forward to safe function pointers the host
//! registered. Nothing outside this module ever sees the raw callback data.

use std::ffi::c_void;
use std::io;
use std::ptr;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::RemoteDesktop::WTSSESSION_NOTIFICATION;
use windows_sys::Win32::System::Services::{
    RegisterServiceCtrlHandlerExW, SetServiceStatus, StartServiceCtrlDispatcherW,
    SERVICE_CONTROL_SESSIONCHANGE, SERVICE_STATUS, SERVICE_STATUS_HANDLE, SERVICE_TABLE_ENTRYW,
    SERVICE_WIN32_OWN_PROCESS,
};

use crate::wide::wide_null;

/// The host's safe control handler. It receives the control code, the event
/// type, and, for session-change notifications, the session it names.
pub type ServiceControlCallback = fn(control: u32, event_type: u32, session_id: Option<u32>) -> u32;

/// The status fields a service reports, without any Win32 structure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawServiceStatus {
    pub current_state: u32,
    pub controls_accepted: u32,
    pub win32_exit_code: u32,
    pub service_specific_exit_code: u32,
    pub checkpoint: u32,
    pub wait_hint_ms: u32,
}

static SERVICE_MAIN: AtomicUsize = AtomicUsize::new(0);
static CONTROL_CALLBACK: AtomicUsize = AtomicUsize::new(0);
static STATUS_HANDLE: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

/// Connects this process to the SCM as `service_name` and runs `service_main`
/// on the SCM's thread until it returns. Blocks until the service stops.
pub fn run_service_dispatcher(service_name: &str, service_main: fn()) -> io::Result<()> {
    SERVICE_MAIN.store(service_main as usize, Ordering::Release);
    let mut name = wide_null(service_name);
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: name.as_mut_ptr(),
            lpServiceProc: Some(service_main_trampoline),
        },
        SERVICE_TABLE_ENTRYW::default(),
    ];
    // SAFETY: the table is NULL-terminated, its name buffer outlives the
    // blocking call, and the trampoline is a valid `extern "system"` function.
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

unsafe extern "system" fn service_main_trampoline(_argument_count: u32, _arguments: *mut *mut u16) {
    let main = SERVICE_MAIN.load(Ordering::Acquire);
    if main == 0 {
        return;
    }
    // SAFETY: the only writer stored a `fn()` cast to usize in
    // `run_service_dispatcher`, which is the sole way to reach this trampoline.
    let main: fn() = unsafe { std::mem::transmute::<usize, fn()>(main) };
    main();
}

/// The registered status handle. Reports go through it; dropping it forgets
/// the registration (the SCM owns the underlying handle for the process
/// lifetime, so there is nothing to close).
#[derive(Debug)]
pub struct ServiceStatusHandle {
    _private: (),
}

/// Registers `callback` as the control handler for `service_name`. Must be
/// called from the service main.
pub fn register_control_handler(
    service_name: &str,
    callback: ServiceControlCallback,
) -> io::Result<ServiceStatusHandle> {
    CONTROL_CALLBACK.store(callback as usize, Ordering::Release);
    let name = wide_null(service_name);
    // SAFETY: `name` is NUL-terminated; the trampoline is a valid
    // `extern "system"` function; no context pointer is passed.
    let handle = unsafe {
        RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(control_trampoline), ptr::null())
    };
    if handle.is_null() {
        CONTROL_CALLBACK.store(0, Ordering::Release);
        return Err(io::Error::last_os_error());
    }
    STATUS_HANDLE.store(handle, Ordering::Release);
    Ok(ServiceStatusHandle { _private: () })
}

impl ServiceStatusHandle {
    /// Reports `status` to the SCM.
    pub fn report(&self, status: RawServiceStatus) -> io::Result<()> {
        report_status(status)
    }
}

impl Drop for ServiceStatusHandle {
    fn drop(&mut self) {
        STATUS_HANDLE.store(ptr::null_mut(), Ordering::Release);
        CONTROL_CALLBACK.store(0, Ordering::Release);
    }
}

/// Reports `status` through the registered handle, from any thread. Fails
/// with `NotConnected` before registration or after the handle was dropped.
pub fn report_status(status: RawServiceStatus) -> io::Result<()> {
    let handle: SERVICE_STATUS_HANDLE = STATUS_HANDLE.load(Ordering::Acquire);
    if handle.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "SCM status handler is not registered",
        ));
    }
    let native = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: status.current_state,
        dwControlsAccepted: status.controls_accepted,
        dwWin32ExitCode: status.win32_exit_code,
        dwServiceSpecificExitCode: status.service_specific_exit_code,
        dwCheckPoint: status.checkpoint,
        dwWaitHint: status.wait_hint_ms,
    };
    // SAFETY: the handle was returned by the registration call and `native`
    // is a fully initialized local structure.
    if unsafe { SetServiceStatus(handle, &native) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

unsafe extern "system" fn control_trampoline(
    control: u32,
    event_type: u32,
    event_data: *mut c_void,
    _context: *mut c_void,
) -> u32 {
    let callback = CONTROL_CALLBACK.load(Ordering::Acquire);
    if callback == 0 {
        return ERROR_SUCCESS;
    }
    // SAFETY: the only writer stored a `ServiceControlCallback` cast to usize
    // in `register_control_handler`, the sole way to install this trampoline.
    let callback: ServiceControlCallback =
        unsafe { std::mem::transmute::<usize, ServiceControlCallback>(callback) };
    // Other controls (device, power, and time-change events) also pass event
    // data, each with its own structure; only the session-change layout is
    // decoded here, so a handler that accepts those controls never sees a
    // foreign structure misread as a session.
    let session_id = if control != SERVICE_CONTROL_SESSIONCHANGE || event_data.is_null() {
        None
    } else {
        // SAFETY: for SERVICE_CONTROL_SESSIONCHANGE the SCM passes a pointer to
        // a WTSSESSION_NOTIFICATION that stays valid for the duration of this
        // callback; `cbSize` is checked before any field past it is trusted.
        let notification = unsafe { &*(event_data.cast::<WTSSESSION_NOTIFICATION>()) };
        (notification.cbSize as usize >= std::mem::size_of::<WTSSESSION_NOTIFICATION>())
            .then_some(notification.dwSessionId)
    };
    callback(control, event_type, session_id)
}

#[cfg(test)]
mod tests {
    use super::{report_status, RawServiceStatus};

    #[test]
    fn reporting_before_registration_is_a_not_connected_error() {
        let error = report_status(RawServiceStatus {
            current_state: 4,
            controls_accepted: 0,
            win32_exit_code: 0,
            service_specific_exit_code: 0,
            checkpoint: 0,
            wait_hint_ms: 0,
        })
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotConnected);
    }
}
