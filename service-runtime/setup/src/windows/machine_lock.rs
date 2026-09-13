mod security;

use std::time::Duration;

use mactype_service_platform::{MutexAcquisition, NamedMutex};
use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;

use crate::SetupError;

const SETUP_MUTEX_NAME: &str = "Global\\MacTypeControlCenter.Setup.v1";
const SETUP_MUTEX_TIMEOUT: Duration = Duration::from_secs(30);

/// The held machine setup lock. Dropping it releases the mutex and closes
/// the handle.
#[derive(Debug)]
pub(super) struct MachineSetupLock {
    _mutex: NamedMutex,
    _abandoned: bool,
}

impl MachineSetupLock {
    pub(super) fn acquire() -> Result<Self, SetupError> {
        Self::acquire_with_timeout(SETUP_MUTEX_TIMEOUT)
    }

    #[cfg(test)]
    pub(super) fn acquire_for_test(timeout: Duration) -> Result<Self, SetupError> {
        Self::acquire_with_timeout(timeout)
    }

    #[cfg(test)]
    fn acquire_named_for_test(name: &str, timeout: Duration) -> Result<Self, SetupError> {
        Self::acquire_named_with_timeout(name, timeout)
    }

    fn acquire_with_timeout(timeout: Duration) -> Result<Self, SetupError> {
        Self::acquire_named_with_timeout(SETUP_MUTEX_NAME, timeout)
    }

    fn acquire_named_with_timeout(name: &str, timeout: Duration) -> Result<Self, SetupError> {
        let descriptor = security::machine_setup_lock_descriptor()?;
        let mutex = match NamedMutex::create(name, &descriptor) {
            Ok(mutex) => mutex,
            Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => {
                return Err(foreign_lock_error(
                    "the named object cannot be opened for security verification",
                ));
            }
            Err(error) => return Err(SetupError::Io(error)),
        };
        // A verification failure drops the mutex, which closes the handle.
        security::verify_machine_setup_lock(mutex.handle())?;
        match mutex.acquire(timeout) {
            Ok(MutexAcquisition::Acquired) => Ok(Self {
                _mutex: mutex,
                _abandoned: false,
            }),
            Ok(MutexAcquisition::Abandoned) => Ok(Self {
                _mutex: mutex,
                _abandoned: true,
            }),
            Ok(MutexAcquisition::TimedOut) => Err(SetupError::Runtime(
                "another machine setup operation did not finish before the bounded wait expired"
                    .to_owned(),
            )),
            Err(error) => Err(SetupError::Io(error)),
        }
    }
}

fn foreign_lock_error(detail: &str) -> SetupError {
    SetupError::Runtime(format!("foreign machine setup lock rejected: {detail}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mactype_service_platform::SecurityDescriptor;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_MUTEX_ID: AtomicU64 = AtomicU64::new(0);

    fn unique_mutex_name(label: &str) -> String {
        format!(
            "Local\\MacTypeControlCenter.Setup.test.{}.{}.{}",
            std::process::id(),
            TEST_MUTEX_ID.fetch_add(1, Ordering::Relaxed),
            label
        )
    }

    #[test]
    fn a_permissive_precreated_mutex_is_rejected_as_foreign() {
        let name = unique_mutex_name("foreign");
        let descriptor = SecurityDescriptor::from_sddl("D:P(A;;GA;;;AU)").unwrap();
        let foreign = NamedMutex::create(&name, &descriptor)
            .expect("failed to create the foreign test mutex");

        let error =
            MachineSetupLock::acquire_named_for_test(&name, Duration::from_millis(25)).unwrap_err();
        assert!(
            error.to_string().contains("foreign machine setup lock"),
            "unexpected lock error: {error}"
        );

        drop(foreign);
    }
}
