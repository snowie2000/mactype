//! Owned kernel handles.

use std::ffi::c_void;
use std::io;
use std::ptr::NonNull;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_ABANDONED, WAIT_FAILED, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Threading::WaitForSingleObject;

/// Result of waiting on a kernel object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitOutcome {
    Signaled,
    TimedOut,
    /// A mutex whose previous owner exited without releasing it. The wait
    /// still acquired ownership.
    Abandoned,
}

/// A kernel handle that is closed exactly once, when the owner is dropped.
///
/// The only way to build one is from a Win32 call that just returned it, so a
/// live `OwnedHandle` is always a valid, open handle that this process owns.
#[derive(Debug)]
pub struct OwnedHandle(NonNull<c_void>);

// SAFETY: a kernel handle is a process-wide token; the kernel serializes every
// operation on it, so moving or sharing the owner across threads is sound.
unsafe impl Send for OwnedHandle {}
// SAFETY: see the `Send` justification; `&OwnedHandle` only ever issues calls
// the kernel already serializes.
unsafe impl Sync for OwnedHandle {}

impl OwnedHandle {
    /// Takes ownership of `handle`, which a Win32 creation or open call just
    /// returned. A null or `INVALID_HANDLE_VALUE` result is turned into the
    /// call's `GetLastError` value, read before anything else can clobber it.
    pub(crate) fn from_creation(handle: HANDLE) -> io::Result<Self> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(NonNull::new(handle).expect("checked non-null handle")))
    }

    pub(crate) fn as_raw(&self) -> HANDLE {
        self.0.as_ptr()
    }

    /// The numeric handle value, for protocols that pass an inherited handle
    /// to a child process on its command line. The number is an opaque token
    /// to the receiver; nothing can be derived from it in this process.
    pub fn raw_value(&self) -> usize {
        self.0.as_ptr() as usize
    }

    /// Waits for the object to become signaled. `None` waits forever.
    pub fn wait(&self, timeout: Option<Duration>) -> io::Result<WaitOutcome> {
        let milliseconds = timeout.map_or(u32::MAX, clamp_timeout);
        // SAFETY: `self.0` is a live handle owned by this process; the wait
        // takes no pointers.
        match unsafe { WaitForSingleObject(self.as_raw(), milliseconds) } {
            WAIT_OBJECT_0 => Ok(WaitOutcome::Signaled),
            WAIT_TIMEOUT => Ok(WaitOutcome::TimedOut),
            WAIT_ABANDONED => Ok(WaitOutcome::Abandoned),
            WAIT_FAILED => Err(io::Error::last_os_error()),
            other => Err(io::Error::other(format!(
                "unexpected wait result {other:#x}"
            ))),
        }
    }

    /// Whether the object is signaled right now.
    pub fn is_signaled(&self) -> bool {
        matches!(
            self.wait(Some(Duration::ZERO)),
            Ok(WaitOutcome::Signaled | WaitOutcome::Abandoned)
        )
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` was returned by a creation call and has not been
        // closed since; this is the single close for it.
        unsafe { CloseHandle(self.as_raw()) };
    }
}

/// Converts a wait duration into the millisecond count Win32 expects, keeping
/// `u32::MAX` (INFINITE) reserved for the explicit "wait forever" case.
pub(crate) fn clamp_timeout(timeout: Duration) -> u32 {
    timeout.as_millis().min(u128::from(u32::MAX - 1)) as u32
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::clamp_timeout;

    #[test]
    fn timeouts_never_collapse_into_infinite() {
        assert_eq!(clamp_timeout(Duration::ZERO), 0);
        assert_eq!(clamp_timeout(Duration::from_millis(250)), 250);
        assert_eq!(clamp_timeout(Duration::from_secs(u64::MAX)), u32::MAX - 1);
    }
}
