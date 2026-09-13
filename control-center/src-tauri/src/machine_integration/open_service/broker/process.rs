use mactype_service_platform::{Process, WaitOutcome};
use std::{io, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::machine_integration::open_service) enum BrokerTermination {
    AlreadyExited,
    Terminated,
}

pub(in crate::machine_integration::open_service) trait BrokerProcessControl {
    fn wait(&mut self, process: &Process, timeout: Duration) -> io::Result<WaitOutcome>;
    fn terminate(&mut self, process: &Process, exit_code: u32) -> io::Result<()>;
}

struct WindowsBrokerProcessControl;

impl BrokerProcessControl for WindowsBrokerProcessControl {
    fn wait(&mut self, process: &Process, timeout: Duration) -> io::Result<WaitOutcome> {
        process.wait(Some(timeout))
    }

    fn terminate(&mut self, process: &Process, exit_code: u32) -> io::Result<()> {
        process.terminate(exit_code)
    }
}

pub(in crate::machine_integration::open_service) fn terminate_broker_process_with(
    process: &Process,
    control: &mut impl BrokerProcessControl,
) -> Result<BrokerTermination, String> {
    let initial = control.wait(process, Duration::ZERO).map_err(|error| {
        format!(
            "elevated broker cleanup is unknown: initial process wait failed with {}",
            error.raw_os_error().unwrap_or(0)
        )
    })?;
    if initial == WaitOutcome::Signaled {
        return Ok(BrokerTermination::AlreadyExited);
    }

    if let Err(error) = control.terminate(process, 21) {
        let after_failure = control.wait(process, Duration::ZERO);
        if matches!(after_failure, Ok(WaitOutcome::Signaled)) {
            return Ok(BrokerTermination::AlreadyExited);
        }
        let state = match after_failure {
            Ok(WaitOutcome::TimedOut) => "timed out".to_owned(),
            Ok(WaitOutcome::Signaled) => unreachable!(),
            Ok(WaitOutcome::Abandoned) => "abandoned".to_owned(),
            Err(error) => error.raw_os_error().unwrap_or(0).to_string(),
        };
        return Err(format!(
            "elevated broker cleanup is unknown: TerminateProcess failed with {} and the process state is {state}",
            error.raw_os_error().unwrap_or(0)
        ));
    }

    match control.wait(process, Duration::from_millis(5_000)) {
        Ok(WaitOutcome::Signaled) => Ok(BrokerTermination::Terminated),
        Ok(WaitOutcome::TimedOut) => Err(
            "elevated broker cleanup is unknown: termination was not confirmed within 5000 ms"
                .to_owned(),
        ),
        Ok(WaitOutcome::Abandoned) => Err(
            "elevated broker cleanup is unknown: termination confirmation returned abandoned"
                .to_owned(),
        ),
        Err(error) => Err(format!(
            "elevated broker cleanup is unknown: termination confirmation failed with {}",
            error.raw_os_error().unwrap_or(0)
        )),
    }
}

pub(super) fn terminate_broker_process(process: &Process) -> Result<BrokerTermination, String> {
    terminate_broker_process_with(process, &mut WindowsBrokerProcessControl)
}

pub(in crate::machine_integration::open_service) fn combine_broker_cleanup_error(
    operation_error: &str,
    cleanup: Result<BrokerTermination, String>,
) -> String {
    match cleanup {
        Ok(_) => operation_error.to_owned(),
        Err(cleanup_error) => format!("{operation_error}; {cleanup_error}"),
    }
}
