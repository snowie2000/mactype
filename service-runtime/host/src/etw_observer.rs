use std::io;
use std::time::Duration;

use mactype_service_contract::StructuredServiceError;
use mactype_service_platform::{ProcessStartEvent, ProcessStartTrace, running_process_ids};

use crate::ProcessEventSource;

/// The name of the service's real-time trace session. It is stable so that a
/// session left behind by a service that died without stopping it can be
/// recognised and taken back at the next start.
pub const PROCESS_START_SESSION: &str = "MacType-ProcessStart";

/// The observer dropped process identifiers because the driver fell behind.
/// The driver's recovery path resubscribes and folds a fresh snapshot into its
/// backlog, so the targets lost here are reached from the snapshot instead.
pub const ETW_OBSERVER_OVERFLOW_CODE: &str = "etw-observer-overflow";

/// The trace consumer stopped on its own, which leaves the subscription dead
/// rather than merely behind. It takes the same recovery path.
pub const ETW_OBSERVER_STOPPED_CODE: &str = "etw-observer-stopped";

/// Process creation read straight from `Microsoft-Windows-Kernel-Process`.
///
/// WMI hears about a new process through a shared real-time session whose
/// buffers flush about once a second; a private session with a millisecond
/// flush timer hears about it in tens of milliseconds, which is the difference
/// between a program that starts with our fonts and one that repaints into
/// them.
pub struct EtwProcessEventSource {
    session_name: String,
    session: Option<ProcessStartTrace>,
}

impl EtwProcessEventSource {
    /// Starts the trace session at once, so a caller that must fall back to
    /// another source learns here whether this token can open one at all.
    /// Opening a real-time session needs an elevated or LocalSystem token.
    pub fn start(session_name: impl Into<String>) -> Result<Self, StructuredServiceError> {
        let session_name = session_name.into();
        let session = open_session(&session_name)?;
        Ok(Self {
            session_name,
            session: Some(session),
        })
    }
}

impl ProcessEventSource for EtwProcessEventSource {
    /// The WQL text is the WMI source's business; this source subscribes by
    /// provider GUID and keyword. A session that is already live and has lost
    /// nothing is kept, so the subscription the constructor opened is not torn
    /// down and rebuilt a moment later for nothing.
    fn subscribe(&mut self, _query: &str) -> Result<(), StructuredServiceError> {
        if self
            .session
            .as_ref()
            .is_some_and(|session| !session.dropped_process_ids())
        {
            return Ok(());
        }
        // Dropping first stops the old session, which is what clears the
        // overflow flag: the replacement starts with no hole in it.
        self.session = None;
        self.session = Some(open_session(&self.session_name)?);
        Ok(())
    }

    fn snapshot_pids(&mut self) -> Result<Vec<u32>, StructuredServiceError> {
        running_process_ids().map_err(|error| {
            os_error(
                "etw-snapshot-failed",
                "the running process list could not be taken",
                &error,
            )
        })
    }

    fn next_pid(&mut self, timeout: Duration) -> Result<Option<u32>, StructuredServiceError> {
        let session = self.session.as_ref().ok_or_else(|| {
            service_error(
                "etw-not-subscribed",
                "the ETW process observer was not subscribed",
                None,
            )
        })?;
        if session.dropped_process_ids() {
            return Err(service_error(
                ETW_OBSERVER_OVERFLOW_CODE,
                "the ETW process observer dropped process identifiers while the driver was busy",
                None,
            ));
        }
        match session.next_process_id(timeout) {
            ProcessStartEvent::Started(pid) => Ok(Some(pid)),
            ProcessStartEvent::Idle => Ok(None),
            ProcessStartEvent::Ended => Err(service_error(
                ETW_OBSERVER_STOPPED_CODE,
                "the ETW process observer stopped delivering events",
                None,
            )),
        }
    }
}

fn open_session(session_name: &str) -> Result<ProcessStartTrace, StructuredServiceError> {
    ProcessStartTrace::start(session_name).map_err(|error| {
        os_error(
            "etw-session-unavailable",
            "a real-time process trace session could not be started",
            &error,
        )
    })
}

fn os_error(code: &str, message: &str, error: &io::Error) -> StructuredServiceError {
    service_error(code, message, error.raw_os_error().map(|code| code as u32))
}

fn service_error(code: &str, message: &str, win32_error: Option<u32>) -> StructuredServiceError {
    StructuredServiceError {
        code: code.to_owned(),
        message: message.to_owned(),
        win32_error,
    }
}
