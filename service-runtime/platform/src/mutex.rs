//! Named mutexes used as machine-wide setup locks.

use std::io;
use std::ptr::null;
use std::time::Duration;

use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex};

use crate::handle::{OwnedHandle, WaitOutcome};
use crate::security::SecurityDescriptor;
use crate::wide::wide_null;

/// How a mutex wait ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutexAcquisition {
    Acquired,
    /// Acquired after the previous owner exited without releasing it.
    Abandoned,
    TimedOut,
}

/// A named mutex created (or opened, when it already exists) with a caller
/// supplied security descriptor. Ownership is released on drop whether or not
/// the wait succeeded, which is harmless for a mutex this thread never owned.
#[derive(Debug)]
pub struct NamedMutex(OwnedHandle);

impl NamedMutex {
    /// Creates or opens `name`. The error carries the Win32 code, so a caller
    /// can distinguish `ERROR_ACCESS_DENIED` (an existing object that this
    /// process may not open) from other failures.
    pub fn create(name: &str, descriptor: &SecurityDescriptor) -> io::Result<Self> {
        let name = wide_null(name);
        let attributes = descriptor.attributes(false);
        // SAFETY: the attribute block and the descriptor it points at outlive
        // the call, and `name` is NUL-terminated.
        let handle = unsafe { CreateMutexW(attributes.as_ptr(), 0, name.as_ptr()) };
        Ok(Self(OwnedHandle::from_creation(handle)?))
    }

    /// Creates or opens `name` with the process token's default security.
    pub fn create_with_default_security(name: &str) -> io::Result<Self> {
        let name = wide_null(name);
        // SAFETY: the null security attributes select default security and
        // `name` is NUL-terminated and outlives the call.
        let handle = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
        Ok(Self(OwnedHandle::from_creation(handle)?))
    }

    /// The owned handle, for security verification of the created object.
    pub fn handle(&self) -> &OwnedHandle {
        &self.0
    }

    /// Waits up to `timeout` for ownership.
    pub fn acquire(&self, timeout: Duration) -> io::Result<MutexAcquisition> {
        Ok(match self.0.wait(Some(timeout))? {
            WaitOutcome::Signaled => MutexAcquisition::Acquired,
            WaitOutcome::Abandoned => MutexAcquisition::Abandoned,
            WaitOutcome::TimedOut => MutexAcquisition::TimedOut,
        })
    }
}

impl Drop for NamedMutex {
    fn drop(&mut self) {
        // SAFETY: the handle is live. Releasing a mutex this thread does not
        // own fails with ERROR_NOT_OWNER and changes nothing, which is the
        // intended no-op for a lock that was never acquired.
        unsafe { ReleaseMutex(self.0.as_raw()) };
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{MutexAcquisition, NamedMutex};
    use crate::security::SecurityDescriptor;

    #[test]
    fn a_held_mutex_times_out_for_a_second_opener_and_frees_on_drop() {
        let name = format!("Local\\mactype-platform-mutex-{}", std::process::id());
        let descriptor =
            SecurityDescriptor::from_sddl("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;AU)").unwrap();
        let first = NamedMutex::create(&name, &descriptor).unwrap();
        assert_eq!(
            first.acquire(Duration::from_millis(50)).unwrap(),
            MutexAcquisition::Acquired
        );

        let second = NamedMutex::create(&name, &descriptor).unwrap();
        let waiter = std::thread::spawn(move || second.acquire(Duration::from_millis(50)).unwrap());
        assert_eq!(waiter.join().unwrap(), MutexAcquisition::TimedOut);

        drop(first);
        let third = NamedMutex::create(&name, &descriptor).unwrap();
        let waiter = std::thread::spawn(move || third.acquire(Duration::from_millis(500)).unwrap());
        assert_eq!(waiter.join().unwrap(), MutexAcquisition::Acquired);
    }
}
