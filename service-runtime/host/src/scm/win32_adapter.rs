use std::io;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use mactype_service_contract::StructuredServiceError;
use mactype_service_platform::{
    register_control_handler, report_status, ManualResetEvent, RawServiceStatus,
    ServiceStatusHandle, WaitOutcome,
};
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Services::{
    SERVICE_RUNNING, SERVICE_START_PENDING, SERVICE_STOPPED, SERVICE_STOP_PENDING,
};

use crate::session_event_queue::SessionEventQueue;
use crate::{
    ScmState, ServiceControl, ServiceStatus, SessionChange, StatusReporter, StopSignal,
    ACCEPTED_CONTROL_MASK, SERVICE_STOP_WAIT_HINT_MS,
};

/// The stop event, shared between the SCM control handler that sets it and
/// the service thread that waits on it. `None` outside a registered service
/// main, so every wait fails with `stop-event-unavailable` instead of
/// blocking.
static STOP_EVENT: RwLock<Option<Arc<ManualResetEvent>>> = RwLock::new(None);
static SESSION_CHANGES: SessionEventQueue = SessionEventQueue::new();

pub(super) struct ServiceControlContext {
    _status_handle: ServiceStatusHandle,
}

impl ServiceControlContext {
    pub(super) fn register(service_name: &str) -> io::Result<Self> {
        let status_handle = register_control_handler(service_name, control_handler)?;
        let stop_event = match ManualResetEvent::new() {
            Ok(event) => event,
            Err(error) => {
                let reporter = Win32StatusReporter;
                let _ = reporter.report(ServiceStatus::stopped_with_error(
                    error.raw_os_error().unwrap_or(1) as u32,
                    0,
                ));
                // Dropping the handle forgets the registration, as the raw
                // adapter did by clearing its status handle.
                drop(status_handle);
                return Err(error);
            }
        };
        *STOP_EVENT.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(stop_event));
        Ok(Self {
            _status_handle: status_handle,
        })
    }
}

impl Drop for ServiceControlContext {
    fn drop(&mut self) {
        *STOP_EVENT.write().unwrap_or_else(PoisonError::into_inner) = None;
    }
}

fn control_handler(control: u32, event_type: u32, session_id: Option<u32>) -> u32 {
    match ServiceControl::from_raw(control, event_type) {
        Some(ServiceControl::Stop | ServiceControl::Shutdown) => {
            let reporter = Win32StatusReporter;
            let _ = reporter.report(ServiceStatus::stop_pending(1, SERVICE_STOP_WAIT_HINT_MS));
            if let Some(event) = registered_stop_event() {
                let _ = event.set();
            }
        }
        Some(ServiceControl::SessionChange { .. }) => {
            if let Some(session_id) = session_id {
                SESSION_CHANGES.push(event_type, session_id);
            }
        }
        None => {}
    }
    ERROR_SUCCESS
}

pub(super) struct Win32StatusReporter;

impl StatusReporter for Win32StatusReporter {
    fn report(&self, status: ServiceStatus) -> io::Result<()> {
        let current_state = match status.state {
            ScmState::StartPending => SERVICE_START_PENDING,
            ScmState::Running => SERVICE_RUNNING,
            ScmState::StopPending => SERVICE_STOP_PENDING,
            ScmState::Stopped => SERVICE_STOPPED,
        };
        report_status(RawServiceStatus {
            current_state,
            controls_accepted: if status.state == ScmState::Running {
                ACCEPTED_CONTROL_MASK
            } else {
                0
            },
            win32_exit_code: status.win32_exit_code,
            service_specific_exit_code: status.service_specific_exit_code,
            checkpoint: status.checkpoint,
            wait_hint_ms: status.wait_hint_ms,
        })
    }
}

pub(super) struct Win32StopSignal;

impl StopSignal for Win32StopSignal {
    fn wait(&self) -> Result<(), StructuredServiceError> {
        match stop_event()?.wait(None) {
            Ok(WaitOutcome::Signaled) => Ok(()),
            Ok(WaitOutcome::TimedOut | WaitOutcome::Abandoned) => Err(stop_wait_error(None)),
            Err(error) => Err(stop_wait_error(error.raw_os_error())),
        }
    }

    fn wait_timeout(&self, timeout: Duration) -> Result<bool, StructuredServiceError> {
        match stop_event()?.wait(Some(timeout)) {
            Ok(WaitOutcome::Signaled) => Ok(true),
            Ok(WaitOutcome::TimedOut) => Ok(false),
            Ok(WaitOutcome::Abandoned) => Err(stop_wait_error(None)),
            Err(error) => Err(stop_wait_error(error.raw_os_error())),
        }
    }

    fn stop_requested(&self) -> bool {
        matches!(self.wait_timeout(Duration::ZERO), Ok(true))
    }

    fn take_session_change(&self) -> Option<SessionChange> {
        SESSION_CHANGES.pop()
    }
}

/// A clone of the registered stop event, so no caller holds the lock across
/// its wait.
fn registered_stop_event() -> Option<Arc<ManualResetEvent>> {
    STOP_EVENT
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

fn stop_event() -> Result<Arc<ManualResetEvent>, StructuredServiceError> {
    registered_stop_event().ok_or_else(|| StructuredServiceError {
        code: "stop-event-unavailable".to_owned(),
        message: "service stop event was not initialized".to_owned(),
        win32_error: None,
    })
}

fn stop_wait_error(win32_error: Option<i32>) -> StructuredServiceError {
    StructuredServiceError {
        code: "stop-wait-failed".to_owned(),
        message: "waiting for the service stop event failed".to_owned(),
        win32_error: win32_error.map(|code| code as u32),
    }
}

pub(crate) fn stop_requested() -> bool {
    registered_stop_event().is_some_and(|event| event.is_signaled())
}
