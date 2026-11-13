//! Manual-reset events.

use std::io;
use std::ptr::null;
use std::time::Duration;

use windows_sys::Win32::System::Threading::{CreateEventW, SetEvent};

use crate::handle::{OwnedHandle, WaitOutcome};

/// An anonymous manual-reset event that starts unsignaled.
#[derive(Debug)]
pub struct ManualResetEvent(OwnedHandle);

impl ManualResetEvent {
    pub fn new() -> io::Result<Self> {
        // SAFETY: no security attributes and no name are passed; the integer
        // flags request a manual-reset event that starts unsignaled.
        let handle = unsafe { CreateEventW(null(), 1, 0, null()) };
        Ok(Self(OwnedHandle::from_creation(handle)?))
    }

    /// Signals the event; every current and future wait completes.
    pub fn set(&self) -> io::Result<()> {
        // SAFETY: the handle is live; the call takes no pointers.
        if unsafe { SetEvent(self.0.as_raw()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// The owned handle, for callers that hand the event to a wait that
    /// takes a raw handle value.
    pub fn handle(&self) -> &OwnedHandle {
        &self.0
    }

    pub fn wait(&self, timeout: Option<Duration>) -> io::Result<WaitOutcome> {
        self.0.wait(timeout)
    }

    pub fn is_signaled(&self) -> bool {
        self.0.is_signaled()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::ManualResetEvent;
    use crate::handle::WaitOutcome;

    #[test]
    fn an_event_stays_signaled_once_set() {
        let event = ManualResetEvent::new().unwrap();
        assert!(!event.is_signaled());
        assert_eq!(
            event.wait(Some(Duration::ZERO)).unwrap(),
            WaitOutcome::TimedOut
        );
        event.set().unwrap();
        assert!(event.is_signaled());
        assert_eq!(
            event.wait(Some(Duration::ZERO)).unwrap(),
            WaitOutcome::Signaled
        );
        assert!(event.is_signaled());
    }
}
