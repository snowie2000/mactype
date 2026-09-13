//! Job objects that bound a helper process's lifetime.

use std::io;
use std::mem::size_of;
use std::ptr::{null, null_mut};

use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

use crate::handle::OwnedHandle;
use crate::process::Process;

/// The limits a job carries, as read back from the kernel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobLimits {
    pub active_process_limit: u32,
    /// Whether `active_process_limit` is in force
    /// (`JOB_OBJECT_LIMIT_ACTIVE_PROCESS`); without the flag the count is
    /// ignored by the kernel.
    pub active_process_limit_enabled: bool,
    pub kill_on_close: bool,
}

/// An anonymous job that admits exactly one process and kills it when the
/// last handle to the job closes.
#[derive(Debug)]
pub struct JobObject(OwnedHandle);

impl JobObject {
    /// Creates the job and applies the single-process, kill-on-close limits
    /// before it is handed out, so a job can never exist without them.
    pub fn single_process_kill_on_close() -> io::Result<Self> {
        // SAFETY: both arguments are null (no security attributes, no name).
        let handle = OwnedHandle::from_creation(unsafe { CreateJobObjectW(null(), null()) })?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_ACTIVE_PROCESS | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        limits.BasicLimitInformation.ActiveProcessLimit = 1;
        // SAFETY: the handle is live; the buffer pointer and length describe
        // exactly the local limit structure.
        if unsafe {
            SetInformationJobObject(
                handle.as_raw(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(handle))
    }

    /// Creates a job that kills every member when its last handle closes.
    pub fn kill_on_close() -> io::Result<Self> {
        // SAFETY: both arguments are null (no security attributes, no name).
        let handle = OwnedHandle::from_creation(unsafe { CreateJobObjectW(null(), null()) })?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: the handle is live; the buffer pointer and length describe
        // exactly the local limit structure.
        if unsafe {
            SetInformationJobObject(
                handle.as_raw(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(handle))
    }

    /// Assigns `process` to the job.
    pub fn assign(&self, process: &Process) -> io::Result<()> {
        // SAFETY: both handles are live and owned by this process.
        if unsafe { AssignProcessToJobObject(self.0.as_raw(), process.handle().as_raw()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Assigns the calling process and keeps the job handle alive for the rest
    /// of its lifetime. Forced termination then closes the handle and kills
    /// every descendant; a job containing this process cannot be released early
    /// in any case.
    pub fn assign_current_process(self) -> io::Result<()> {
        // SAFETY: the job handle is live; GetCurrentProcess returns a valid
        // pseudo-handle that does not need to be closed.
        if unsafe { AssignProcessToJobObject(self.0.as_raw(), GetCurrentProcess()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        std::mem::forget(self);
        Ok(())
    }

    /// Terminates every process in the job with `exit_code`.
    pub fn terminate(&self, exit_code: u32) -> io::Result<()> {
        // SAFETY: the handle is live; the call takes no pointers.
        if unsafe { TerminateJobObject(self.0.as_raw(), exit_code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Reads the limits back from the kernel.
    pub fn limits(&self) -> io::Result<JobLimits> {
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        // SAFETY: the handle is live; the buffer pointer and length describe
        // exactly the local limit structure and the returned-length pointer is
        // null, which the API permits.
        if unsafe {
            QueryInformationJobObject(
                self.0.as_raw(),
                JobObjectExtendedLimitInformation,
                (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let flags = limits.BasicLimitInformation.LimitFlags;
        Ok(JobLimits {
            active_process_limit: limits.BasicLimitInformation.ActiveProcessLimit,
            active_process_limit_enabled: flags & JOB_OBJECT_LIMIT_ACTIVE_PROCESS != 0,
            kill_on_close: flags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE != 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::JobObject;

    #[test]
    fn a_job_always_carries_the_single_process_kill_on_close_limits() {
        let job = JobObject::single_process_kill_on_close().unwrap();
        let limits = job.limits().unwrap();
        assert_eq!(limits.active_process_limit, 1);
        assert!(limits.active_process_limit_enabled);
        assert!(limits.kill_on_close);
    }

    #[test]
    fn a_kill_on_close_job_has_no_active_process_limit() {
        let job = JobObject::kill_on_close().unwrap();
        let limits = job.limits().unwrap();
        assert!(!limits.active_process_limit_enabled);
        assert!(limits.kill_on_close);
    }
}
