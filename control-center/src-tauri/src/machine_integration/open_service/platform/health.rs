use super::super::{read_bounded_regular_file, HealthReport, LiveHealthReport};
use mactype_service_contract::effective_health_pipe_name;
use std::{fs, io::Read, path::Path, time::Duration};

const MAX_HEALTH_BYTES: u64 = 16 * 1024;
const LIVE_HEALTH_ATTEMPTS: usize = 8;
const LIVE_HEALTH_POLL: Duration = Duration::from_millis(25);

pub(super) fn read_health() -> Result<LiveHealthReport, String> {
    read_health_with_retry(LIVE_HEALTH_ATTEMPTS, read_health_once, || {
        std::thread::sleep(LIVE_HEALTH_POLL)
    })
}

fn read_health_once() -> Result<LiveHealthReport, String> {
    let mut file =
        fs::File::open(effective_health_pipe_name()).map_err(|error| error.to_string())?;
    let server_pid = mactype_service_platform::named_pipe_server_process_id(&file)
        .map_err(|_| "service health pipe server PID is unavailable".to_owned())?;
    if server_pid == 0 {
        return Err("service health pipe server PID is unavailable".to_owned());
    }
    let report = read_health_message(&mut file)?;
    Ok(LiveHealthReport { server_pid, report })
}

fn read_health_with_retry(
    maximum_attempts: usize,
    mut observe: impl FnMut() -> Result<LiveHealthReport, String>,
    mut wait: impl FnMut(),
) -> Result<LiveHealthReport, String> {
    if maximum_attempts == 0 {
        return Err("service health query has no retry budget".to_owned());
    }
    let mut last_error = None;
    for attempt in 0..maximum_attempts {
        match observe() {
            Ok(report) => return Ok(report),
            Err(error) => last_error = Some(error),
        }
        if attempt + 1 < maximum_attempts {
            wait();
        }
    }
    Err(last_error.expect("at least one live health observation"))
}

fn read_health_message(reader: &mut impl Read) -> Result<HealthReport, String> {
    let mut bytes = vec![0; MAX_HEALTH_BYTES as usize + 1];
    let read = reader.read(&mut bytes).map_err(|error| error.to_string())?;
    if read == 0 || read > MAX_HEALTH_BYTES as usize {
        return Err("service health response is empty or too large".to_owned());
    }
    bytes.truncate(read);
    let report: HealthReport = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    report.validate().map_err(|error| error.to_string())?;
    Ok(report)
}

pub(in crate::machine_integration::open_service) fn read_health_for_scm_process(
    process_id: u32,
) -> Result<HealthReport, String> {
    let live = read_health()?;
    if process_id == 0 || live.server_pid != process_id {
        return Err(format!(
            "service health pipe PID {} does not match SCM PID {process_id}",
            live.server_pid
        ));
    }
    Ok(live.report)
}

pub(super) fn read_persisted_health(service_root: &Path) -> Result<HealthReport, String> {
    let bytes = read_bounded_regular_file(
        &service_root.join("health.json"),
        MAX_HEALTH_BYTES,
        "persisted service health snapshot",
    )?;
    let report: HealthReport = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    report.validate().map_err(|error| error.to_string())?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    struct OneMessageThenBrokenPipe {
        message: Option<Vec<u8>>,
        reads: usize,
    }

    impl Read for OneMessageThenBrokenPipe {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            if let Some(message) = self.message.take() {
                buffer[..message.len()].copy_from_slice(&message);
                return Ok(message.len());
            }
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the message-mode server disconnected after one response",
            ))
        }
    }

    #[test]
    fn live_health_reads_exactly_one_bounded_pipe_message() {
        let report = HealthReport::ready(
            "0.2.0",
            Some(
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_owned(),
            ),
        );
        let mut reader = OneMessageThenBrokenPipe {
            message: Some(serde_json::to_vec(&report).unwrap()),
            reads: 0,
        };

        let decoded = read_health_message(&mut reader).unwrap();

        assert_eq!(decoded, report);
        assert_eq!(reader.reads, 1);
    }

    #[test]
    fn live_health_retries_transport_failures_but_returns_a_degraded_report_immediately() {
        let ready = HealthReport::ready(
            "0.2.0",
            Some(
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_owned(),
            ),
        );
        let mut attempts = 0;
        let recovered = read_health_with_retry(
            3,
            || {
                attempts += 1;
                if attempts < 3 {
                    Err("all pipe instances are busy".to_owned())
                } else {
                    Ok(LiveHealthReport {
                        server_pid: 42,
                        report: ready.clone(),
                    })
                }
            },
            || {},
        )
        .unwrap();
        assert_eq!(attempts, 3);
        assert_eq!(
            recovered.report.health,
            mactype_service_contract::HealthState::Ready
        );

        let mut degraded = ready;
        degraded.health = mactype_service_contract::HealthState::Degraded;
        degraded.last_error = Some(mactype_service_contract::StructuredServiceError {
            code: "conflicting-mactype-module-loaded".to_owned(),
            message: "target already contains another MacType module".to_owned(),
            win32_error: None,
        });
        let mut degraded_attempts = 0;
        let observed = read_health_with_retry(
            3,
            || {
                degraded_attempts += 1;
                Ok(LiveHealthReport {
                    server_pid: 42,
                    report: degraded.clone(),
                })
            },
            || {},
        )
        .unwrap();
        assert_eq!(degraded_attempts, 1);
        assert_eq!(
            observed.report.health,
            mactype_service_contract::HealthState::Degraded
        );
    }
}
