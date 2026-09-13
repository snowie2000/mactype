use std::thread;
use std::time::{Duration, Instant};

use mactype_service_contract::{effective_health_pipe_name, HealthReport, HealthState};
use mactype_service_platform::NamedPipeClient;

use crate::SetupError;

fn health_server_matches_expected(expected_service_pid: u32, server_pid: u32) -> bool {
    expected_service_pid != 0 && server_pid != 0 && expected_service_pid == server_pid
}

fn health_report_matches_expected_profile(
    report: &HealthReport,
    expected_profile_digest: Option<&str>,
) -> bool {
    match expected_profile_digest {
        Some(expected) => report.is_active_for(expected),
        None => report.validate().is_ok() && report.health == HealthState::Ready,
    }
}

pub(super) fn wait_for_ready_health(
    expected_service_pid: u32,
    expected_profile_digest: Option<&str>,
    timeout: Duration,
) -> Result<(), SetupError> {
    let deadline = Instant::now() + timeout;
    let pipe_name = effective_health_pipe_name();
    while Instant::now() < deadline {
        // The pipe is absent until the service publishes it; every open
        // failure is a reason to keep polling rather than an error.
        if let Ok(client) = NamedPipeClient::open_read(pipe_name) {
            let server_pid = client.server_process_id().map_err(SetupError::Io)?;
            if !health_server_matches_expected(expected_service_pid, server_pid) {
                return Err(SetupError::Runtime(format!(
                    "health pipe server PID {server_pid} does not match SCM service PID {expected_service_pid}"
                )));
            }
            let mut buffer = vec![0u8; 16 * 1024];
            let read = client.read(&mut buffer).unwrap_or(0);
            drop(client);
            if read > 0 {
                buffer.truncate(read);
                if let Ok(report) = serde_json::from_slice::<HealthReport>(&buffer) {
                    if health_report_matches_expected_profile(&report, expected_profile_digest) {
                        return Ok(());
                    }
                    if report.validate().is_ok() && report.health == HealthState::Ready {
                        return Err(SetupError::Runtime(
                            "service Ready health reported a different active profile digest"
                                .to_owned(),
                        ));
                    }
                    if report.health == HealthState::Failed {
                        return Err(SetupError::Runtime(
                            "service reported failed health during start".to_owned(),
                        ));
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(SetupError::Runtime(
        "service did not publish Ready health before timeout".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use mactype_service_contract::HealthReport;

    use super::{health_report_matches_expected_profile, health_server_matches_expected};

    #[test]
    fn ready_health_is_accepted_only_from_the_verified_service_process() {
        assert!(health_server_matches_expected(42, 42));
        assert!(!health_server_matches_expected(0, 42));
        assert!(!health_server_matches_expected(42, 0));
        assert!(!health_server_matches_expected(42, 43));
    }

    #[test]
    fn bootstrap_ready_health_must_match_the_exact_published_profile_digest() {
        let expected = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let wrong = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

        assert!(health_report_matches_expected_profile(
            &HealthReport::ready("0.2.0", Some(expected.to_owned())),
            Some(expected),
        ));
        assert!(!health_report_matches_expected_profile(
            &HealthReport::ready("0.2.0", Some(wrong.to_owned())),
            Some(expected),
        ));
    }
}
