#[cfg(test)]
mod tests;

use std::ffi::OsString;
use std::io;
#[cfg(test)]
use std::sync::Mutex;
use std::time::{Duration, Instant};

use mactype_service_platform::{
    anonymous_pipe, null_device, read_bounded, JobObject, OwnedHandle, Process, ProcessAccess,
    ProcessLaunch, StandardHandles, SuspendedChild, WaitOutcome,
};
use windows_sys::Win32::Foundation::{ERROR_CANCELLED, STILL_ACTIVE};

use crate::{HelperInvocation, HelperLaunchError, HelperLauncher, HelperOutput};

const TERMINATION_CONFIRMATION: Duration = Duration::from_millis(250);
const WAIT_SLICE: Duration = Duration::from_millis(10);
const MAX_HELPER_OUTPUT_BYTES: usize = 1024;

/// The launcher's most recent child, held open with SYNCHRONIZE while it is
/// still suspended so tests wait on that exact process object. A PID alone
/// would not do: once the child's object is gone the kernel can hand the same
/// PID to a process another test just spawned.
#[cfg(test)]
static LAST_TEST_CHILD: Mutex<Option<Process>> = Mutex::new(None);

pub struct WindowsHelperLauncher {
    stop_requested: fn() -> bool,
}

impl WindowsHelperLauncher {
    pub const fn new(stop_requested: fn() -> bool) -> Self {
        Self { stop_requested }
    }

    fn launch_process<F>(
        &self,
        invocation: &HelperInvocation,
        arguments_for_handle: F,
    ) -> Result<HelperOutput, HelperLaunchError>
    where
        F: FnOnce(usize) -> Vec<OsString>,
    {
        if (self.stop_requested)() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "service stop was requested before helper launch",
            )
            .into());
        }
        let deadline = Instant::now() + invocation.timeout;
        let job = JobObject::single_process_kill_on_close()?;
        let target = Process::open(invocation.target.pid, ProcessAccess::InjectionTarget)?;
        if target.creation_time()? != invocation.target.creation_time {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "target process creation time changed before helper launch",
            )
            .into());
        }

        let (output_read, output_write) = anonymous_pipe()?;
        let null_device = null_device()?;
        let arguments = arguments_for_handle(target.handle().raw_value());
        if (self.stop_requested)() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "service stop was requested before helper process creation",
            )
            .into());
        }

        let child = SuspendedChild::create(&ProcessLaunch {
            executable: &invocation.executable,
            arguments: &arguments,
            inherit: &[target.handle(), &output_write, &null_device],
            standard: StandardHandles {
                input: &null_device,
                output: &output_write,
                error: &null_device,
            },
        })?;
        #[cfg(test)]
        {
            let observer = Process::open(child.pid(), ProcessAccess::Synchronize).ok();
            *LAST_TEST_CHILD
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = observer;
        }
        if let Err(error) = job.assign(child.process()) {
            terminate_unassigned_child(&child.abandon(), deadline)?;
            return Err(error.into());
        }
        if (self.stop_requested)() {
            let child = child.abandon();
            job.terminate(ERROR_CANCELLED)?;
            confirm_terminated(&child, deadline)?;
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "service stop was requested before helper resume",
            )
            .into());
        }
        if let Err(error) = child.start() {
            let child = child.abandon();
            job.terminate(error.raw_os_error().unwrap_or(1) as u32)?;
            confirm_terminated(&child, deadline)?;
            return Err(error.into());
        }
        let child_process = child.into_process();
        drop(output_write);
        drop(null_device);
        drop(target);

        wait_for_helper(
            self.stop_requested,
            &job,
            &child_process,
            output_read,
            deadline,
            invocation.timeout,
        )
    }
}

impl HelperLauncher for WindowsHelperLauncher {
    fn launch(&self, invocation: &HelperInvocation) -> Result<HelperOutput, HelperLaunchError> {
        self.launch_process(invocation, |handle| {
            invocation.arguments_for_process_handle(handle)
        })
    }
}

fn wait_for_helper(
    stop_requested: fn() -> bool,
    job: &JobObject,
    child_process: &Process,
    output_read: OwnedHandle,
    deadline: Instant,
    timeout: Duration,
) -> Result<HelperOutput, HelperLaunchError> {
    let execution_deadline = deadline
        .checked_sub(timeout.min(TERMINATION_CONFIRMATION))
        .unwrap_or(deadline);
    loop {
        if stop_requested() {
            let cleanup_deadline = deadline.min(Instant::now() + TERMINATION_CONFIRMATION);
            job.terminate(ERROR_CANCELLED)
                .map_err(HelperLaunchError::after_resume)?;
            confirm_terminated(child_process, cleanup_deadline)
                .map_err(HelperLaunchError::after_resume)?;
            return Err(HelperLaunchError::after_resume(io::Error::new(
                io::ErrorKind::Interrupted,
                "service stop terminated an in-flight helper with unknown target cleanup",
            )));
        }
        let remaining = execution_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            job.terminate(1460)
                .map_err(HelperLaunchError::after_resume)?;
            confirm_terminated(child_process, deadline).map_err(HelperLaunchError::after_resume)?;
            return Err(HelperLaunchError::after_resume(io::Error::new(
                io::ErrorKind::TimedOut,
                "helper exceeded its absolute launch timeout",
            )));
        }
        match child_process.wait(Some(wait_slice(remaining))) {
            Ok(WaitOutcome::Signaled) => break,
            Ok(WaitOutcome::TimedOut) => continue,
            Ok(WaitOutcome::Abandoned) => {
                return Err(HelperLaunchError::after_resume(io::Error::other(
                    "unexpected helper wait result",
                )))
            }
            Err(error) => return Err(HelperLaunchError::after_resume(error)),
        }
    }
    // A helper that exits with STILL_ACTIVE as its code looks like a running
    // process to the OS; keep the raw value so the broker rejects it as an
    // exit mismatch, exactly as the raw exit-code query did.
    let exit_code = child_process
        .exit_code()
        .map_err(HelperLaunchError::after_resume)?
        .unwrap_or(STILL_ACTIVE as u32);
    let stdout = read_bounded(&output_read, MAX_HELPER_OUTPUT_BYTES)
        .map_err(HelperLaunchError::after_resume)?;
    Ok(HelperOutput {
        exit_code: exit_code as i32,
        stdout,
    })
}

/// The next poll slice: at most ten milliseconds, at least one, in the whole
/// milliseconds the kernel wait counts in.
fn wait_slice(remaining: Duration) -> Duration {
    Duration::from_millis((remaining.min(WAIT_SLICE).as_millis() as u64).max(1))
}

fn terminate_unassigned_child(child: &Process, deadline: Instant) -> io::Result<()> {
    child.terminate(1)?;
    confirm_terminated(child, deadline)
}

fn confirm_terminated(child: &Process, deadline: Instant) -> io::Result<()> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "helper process termination could not be confirmed within the absolute bound",
            ));
        }
        match child.wait(Some(wait_slice(remaining))) {
            Ok(WaitOutcome::Signaled) => return Ok(()),
            Ok(WaitOutcome::TimedOut) => continue,
            Ok(WaitOutcome::Abandoned) => {
                return Err(io::Error::other("unexpected helper cleanup wait result"))
            }
            Err(error) => return Err(error),
        }
    }
}
