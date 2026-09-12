use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use mactype_service_contract::{effective_service_name, HealthReport, HealthState};
use mactype_service_platform::{
    Process, ProcessAccess, ServiceAccess, ServiceConfig, ServiceHandle, ServiceState,
    ServiceStatusSnapshot, WaitOutcome,
};
use windows_sys::Win32::Foundation::{
    ERROR_INVALID_PARAMETER, ERROR_SERVICE_MARKED_FOR_DELETE, WAIT_ABANDONED,
};

use super::configuration::{
    configure_metadata, observed_configuration, quoted_image_path, service_configuration_drift,
    service_configuration_matches_owned_contract, service_identity_matches_owned_contract,
    validate_service_binary,
};
use super::health::wait_for_ready_health;
use super::{ServiceManager, DISPLAY_NAME, HEALTH_TIMEOUT, STATE_TIMEOUT};
use crate::storage::read_bounded_regular_file;
use crate::SetupError;

const MAX_PERSISTED_HEALTH_BYTES: u64 = 16 * 1024;
/// Reconfiguration must also carry `SERVICE_START`, which the restart-on-failure
/// metadata requires; the platform `Reconfigure` rights set includes it.
const RECONFIGURE_ACCESS: ServiceAccess = ServiceAccess::Reconfigure;

fn wait_for_process_exit(process: &Process, deadline: Instant) -> Result<(), SetupError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    // A deadline that already passed polls once; any remaining time waits at
    // least one millisecond so a sub-millisecond remainder cannot degrade into
    // a poll.
    let timeout = if remaining.is_zero() {
        Duration::ZERO
    } else {
        remaining.max(Duration::from_millis(1))
    };
    match process.wait(Some(timeout)) {
        Ok(WaitOutcome::Signaled) => Ok(()),
        Ok(WaitOutcome::TimedOut) => Err(SetupError::Runtime(
            "service process did not exit before the stop timeout".to_owned(),
        )),
        Ok(WaitOutcome::Abandoned) => Err(SetupError::Runtime(format!(
            "service process exit wait returned unknown result {WAIT_ABANDONED}"
        ))),
        Err(error) => Err(SetupError::Io(error)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AbsenceObservation {
    Absent,
    Present,
    DeletePending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AbsencePollAction {
    Complete,
    Retry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessCaptureTarget {
    AlreadyStopped,
    Pid(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessIdentityObservation {
    Stopped,
    Same,
    Changed(u32),
}

fn process_capture_target(
    status: &ServiceStatusSnapshot,
) -> Result<ProcessCaptureTarget, SetupError> {
    if status.state == ServiceState::Stopped {
        return Ok(ProcessCaptureTarget::AlreadyStopped);
    }
    if status.process_id == 0 {
        return Err(SetupError::Runtime(
            "SCM reported a non-stopped service without a process identity".to_owned(),
        ));
    }
    Ok(ProcessCaptureTarget::Pid(status.process_id))
}

fn observe_process_identity(
    captured_pid: u32,
    status: &ServiceStatusSnapshot,
) -> Result<ProcessIdentityObservation, SetupError> {
    match process_capture_target(status)? {
        ProcessCaptureTarget::AlreadyStopped => Ok(ProcessIdentityObservation::Stopped),
        ProcessCaptureTarget::Pid(observed_pid) if observed_pid == captured_pid => {
            Ok(ProcessIdentityObservation::Same)
        }
        ProcessCaptureTarget::Pid(observed_pid) => {
            Ok(ProcessIdentityObservation::Changed(observed_pid))
        }
    }
}

fn stopping_process_is_complete(
    captured_pid: u32,
    status: &ServiceStatusSnapshot,
) -> Result<bool, SetupError> {
    match observe_process_identity(captured_pid, status)? {
        ProcessIdentityObservation::Stopped => Ok(true),
        ProcessIdentityObservation::Same => Ok(false),
        ProcessIdentityObservation::Changed(observed_pid) => Err(SetupError::Runtime(format!(
            "service process identity changed while stopping ({captured_pid} -> {observed_pid})"
        ))),
    }
}

fn capture_service_process(
    service: &ServiceHandle,
    deadline: Instant,
) -> Result<Option<(u32, Process)>, SetupError> {
    loop {
        let status = service.status()?;
        let pid = match process_capture_target(&status)? {
            ProcessCaptureTarget::AlreadyStopped => return Ok(None),
            ProcessCaptureTarget::Pid(pid) => pid,
        };

        match Process::open(pid, ProcessAccess::Synchronize) {
            Ok(process) => match observe_process_identity(pid, &service.status()?)? {
                ProcessIdentityObservation::Stopped | ProcessIdentityObservation::Same => {
                    return Ok(Some((pid, process)))
                }
                ProcessIdentityObservation::Changed(_) => {
                    wait_before_process_capture_retry(deadline)?
                }
            },
            Err(error) if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) => {
                match observe_process_identity(pid, &service.status()?)? {
                    ProcessIdentityObservation::Stopped => return Ok(None),
                    ProcessIdentityObservation::Same | ProcessIdentityObservation::Changed(_) => {
                        wait_before_process_capture_retry(deadline)?
                    }
                }
            }
            Err(error) => {
                return Err(SetupError::Runtime(format!(
                    "could not capture the service process identity before stopping: {error}"
                )))
            }
        }
    }
}

fn wait_before_process_capture_retry(deadline: Instant) -> Result<(), SetupError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(SetupError::Runtime(
            "service process identity could not be captured before the stop timeout".to_owned(),
        ));
    }
    thread::sleep(remaining.min(Duration::from_millis(10)));
    Ok(())
}

fn wait_for_stopped_process(
    service: &ServiceHandle,
    captured_pid: u32,
    deadline: Instant,
) -> Result<(), SetupError> {
    loop {
        let status = service.status()?;
        if stopping_process_is_complete(captured_pid, &status)? {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(SetupError::Runtime(
                "service did not reach the stopped state before timeout".to_owned(),
            ));
        }
        thread::sleep(remaining.min(Duration::from_millis(100)));
    }
}

fn absence_poll_action(
    observation: AbsenceObservation,
    timed_out: bool,
) -> Result<AbsencePollAction, SetupError> {
    if observation == AbsenceObservation::Absent {
        return Ok(AbsencePollAction::Complete);
    }
    if timed_out {
        return Err(SetupError::Runtime(
            "service name remained delete-pending or occupied until removal timeout".to_owned(),
        ));
    }
    Ok(AbsencePollAction::Retry)
}

impl ServiceManager {
    pub fn install(&self, service_binary: &Path) -> Result<(), SetupError> {
        validate_service_binary(&self.protected_root, service_binary)?;
        if self.open_service(ServiceAccess::QueryStatus)?.is_some() {
            return Err(SetupError::Runtime(
                "the fixed service name already exists; refusing to replace it".to_owned(),
            ));
        }

        let image_path = quoted_image_path(service_binary)?;
        let service = self.manager.create_own_process_auto_start(
            effective_service_name(),
            DISPLAY_NAME,
            &image_path,
        )?;
        if let Err(error) = configure_metadata(&service) {
            // The half-configured record is removed on a best-effort basis;
            // the metadata failure is what the caller sees.
            let _ = service.delete();
            return Err(error);
        }
        Ok(())
    }

    pub fn reconfigure(&self, service_binary: &Path) -> Result<(), SetupError> {
        validate_service_binary(&self.protected_root, service_binary)?;
        let service = self
            .open_service(RECONFIGURE_ACCESS)?
            .ok_or_else(|| SetupError::Runtime("the open service is not installed".to_owned()))?;
        self.ensure_owned(&service)?;
        let before = service.config()?;
        let repaired_fields = service_configuration_drift(&observed_configuration(&before));
        let image_path = quoted_image_path(service_binary)?;
        service.set_image_and_display_name(&image_path, DISPLAY_NAME)?;
        service.restore_owned_mutable_configuration()?;
        configure_metadata(&service).map_err(|error| {
            error.at_machine_path("configure service recovery metadata", service_binary)
        })?;
        let after = service.config()?;
        let observed_after = observed_configuration(&after);
        if !service_identity_matches_owned_contract(&self.protected_root, &observed_after) {
            return Err(SetupError::Runtime(
                "service reconfiguration left an identity mismatch".to_owned(),
            ));
        }
        if !service_configuration_matches_owned_contract(&self.protected_root, &observed_after) {
            return Err(SetupError::Runtime(format!(
                "service reconfiguration left configuration drift: {}",
                service_configuration_drift(&observed_after).join(",")
            )));
        }
        if !repaired_fields.is_empty() {
            crate::event_log::service_configuration_repaired(&repaired_fields);
        }
        Ok(())
    }

    pub fn start_and_wait_ready(&self) -> Result<(), SetupError> {
        self.start_and_wait_ready_for(None)
    }

    pub fn start_and_wait_ready_for_profile(
        &self,
        expected_profile_digest: &str,
    ) -> Result<(), SetupError> {
        self.start_and_wait_ready_for(Some(expected_profile_digest))
    }

    fn start_and_wait_ready_for(
        &self,
        expected_profile_digest: Option<&str>,
    ) -> Result<(), SetupError> {
        let service = self
            .open_service(ServiceAccess::Start)?
            .ok_or_else(|| SetupError::Runtime("the open service is not installed".to_owned()))?;
        self.ensure_owned(&service)?;
        // An already running service is not an error; the readiness wait
        // below decides whether it is healthy.
        service.start()?;
        let health_path = self.protected_root.join("health.json");
        let status = wait_for_state(
            &service,
            ServiceState::Running,
            STATE_TIMEOUT,
            Some(&health_path),
        )?;
        if status.process_id == 0 {
            return Err(SetupError::Runtime(
                "SCM reported a running service without a process identity".to_owned(),
            ));
        }
        wait_for_ready_health(status.process_id, expected_profile_digest, HEALTH_TIMEOUT)
    }

    pub fn is_running(&self) -> Result<bool, SetupError> {
        let service = self
            .open_service(ServiceAccess::QueryStatusAndConfig)?
            .ok_or_else(|| SetupError::Runtime("the open service is not installed".to_owned()))?;
        self.ensure_owned(&service)?;
        match service.status()?.state {
            ServiceState::Running => Ok(true),
            ServiceState::Stopped => Ok(false),
            _ => Err(SetupError::Runtime(
                "the open service is transitioning and cannot be repaired".to_owned(),
            )),
        }
    }

    pub fn stop(&self) -> Result<(), SetupError> {
        let Some(service) = self.open_service(ServiceAccess::Stop)? else {
            return Ok(());
        };
        self.ensure_owned(&service)?;
        let deadline = Instant::now() + STATE_TIMEOUT;
        let Some((captured_pid, process)) = capture_service_process(&service, deadline)? else {
            return Ok(());
        };
        // A service that is no longer active has nothing left to stop; the
        // identity checks below confirm the captured process is gone.
        service.stop()?;
        wait_for_stopped_process(&service, captured_pid, deadline)?;
        wait_for_process_exit(&process, deadline)
    }

    pub fn remove(&self) -> Result<(), SetupError> {
        let expected = {
            let Some(service) = self.open_service(ServiceAccess::QueryStatusAndConfig)? else {
                return Ok(());
            };
            self.ensure_owned(&service)?;
            service.config()?
        };
        self.stop()?;
        let Some(service) = self.open_service(ServiceAccess::Delete)? else {
            return Ok(());
        };
        ensure_captured_configuration_unchanged(&expected, &service.config()?)?;
        service.delete()?;
        drop(service);
        self.wait_until_absent(STATE_TIMEOUT)
    }

    /// Ownership and configuration are verified before `DeleteService` is
    /// issued; once the delete succeeded the SCM record is delete-pending and
    /// an external open handle can keep it observable with unstable
    /// `QueryServiceConfigW` output, so this poll checks absence only instead
    /// of re-comparing the captured configuration.
    fn wait_until_absent(&self, timeout: Duration) -> Result<(), SetupError> {
        let deadline = Instant::now() + timeout;
        loop {
            let observation = match self.open_service(ServiceAccess::QueryStatus) {
                Ok(None) => AbsenceObservation::Absent,
                Ok(Some(_)) => AbsenceObservation::Present,
                Err(error)
                    if error.raw_os_error() == Some(ERROR_SERVICE_MARKED_FOR_DELETE as i32) =>
                {
                    AbsenceObservation::DeletePending
                }
                Err(error) => return Err(SetupError::Io(error)),
            };
            match absence_poll_action(observation, Instant::now() >= deadline)? {
                AbsencePollAction::Complete => return Ok(()),
                AbsencePollAction::Retry => thread::sleep(Duration::from_millis(100)),
            }
        }
    }
}

fn ensure_captured_configuration_unchanged(
    expected: &ServiceConfig,
    actual: &ServiceConfig,
) -> Result<(), SetupError> {
    match describe_configuration_difference(expected, actual) {
        None => Ok(()),
        Some(difference) => Err(SetupError::Runtime(format!(
            "the fixed service configuration changed after ownership was verified: {difference}"
        ))),
    }
}

fn describe_configuration_difference(
    expected: &ServiceConfig,
    actual: &ServiceConfig,
) -> Option<String> {
    let mut differences = Vec::new();
    let mut compare = |field: &str, expected: String, actual: String| {
        if expected != actual {
            differences.push(format!("{field} changed ({expected} -> {actual})"));
        }
    };
    compare(
        "service_type",
        expected.service_type.to_string(),
        actual.service_type.to_string(),
    );
    compare(
        "start_type",
        expected.start_type.to_string(),
        actual.start_type.to_string(),
    );
    compare(
        "error_control",
        expected.error_control.to_string(),
        actual.error_control.to_string(),
    );
    compare(
        "image_path",
        format!("[{}]", expected.image_path),
        format!("[{}]", actual.image_path),
    );
    compare(
        "account",
        format!("[{}]", expected.account),
        format!("[{}]", actual.account),
    );
    compare(
        "display_name",
        format!("[{}]", expected.display_name),
        format!("[{}]", actual.display_name),
    );
    compare(
        "load_order_group",
        format!("[{}]", expected.load_order_group),
        format!("[{}]", actual.load_order_group),
    );
    compare(
        "tag_id",
        expected.tag_id.to_string(),
        actual.tag_id.to_string(),
    );
    compare(
        "dependencies",
        format!("{:?}", expected.dependencies),
        format!("{:?}", actual.dependencies),
    );
    if differences.is_empty() {
        None
    } else {
        Some(differences.join(", "))
    }
}

fn wait_for_state(
    service: &ServiceHandle,
    expected: ServiceState,
    timeout: Duration,
    failure_health_path: Option<&Path>,
) -> Result<ServiceStatusSnapshot, SetupError> {
    let deadline = Instant::now() + timeout;
    loop {
        let status = service.status()?;
        if status.state == expected {
            return Ok(status);
        }
        if status.state == ServiceState::Stopped && expected != ServiceState::Stopped {
            return Err(stopped_before_expected_state_error(
                &status,
                expected,
                failure_health_path,
            ));
        }
        if Instant::now() >= deadline {
            return Err(SetupError::Runtime(format!(
                "service did not reach state {} before timeout",
                expected.as_raw()
            )));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn stopped_before_expected_state_error(
    status: &ServiceStatusSnapshot,
    expected: ServiceState,
    failure_health_path: Option<&Path>,
) -> SetupError {
    let mut message = format!(
        "service stopped before reaching state {} (win32={}, service={})",
        expected.as_raw(),
        status.win32_exit_code,
        status.service_specific_exit_code
    );
    if let Some(diagnostic) = failure_health_path.and_then(persisted_failure_diagnostic) {
        message.push_str("; persisted health failure: ");
        message.push_str(&diagnostic);
    }
    SetupError::Runtime(message)
}

fn persisted_failure_diagnostic(path: &Path) -> Option<String> {
    let bytes = read_bounded_regular_file(
        path,
        MAX_PERSISTED_HEALTH_BYTES,
        "persisted service health diagnostic",
    )
    .ok()?;
    let report: HealthReport = serde_json::from_slice(&bytes).ok()?;
    report.validate().ok()?;
    if report.health != HealthState::Failed {
        return None;
    }
    let error = report.last_error?;
    Some(match error.win32_error {
        Some(code) => format!("{}: {} (win32={code})", error.code, error.message),
        None => format!("{}: {}", error.code, error.message),
    })
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::time::{Duration, Instant};

    use mactype_service_contract::{
        HealthReport, HealthState, InjectionTelemetry, ReadinessReport, StructuredServiceError,
        HEALTH_PROTOCOL_VERSION,
    };
    use mactype_service_platform::{
        Process, ProcessAccess, ServiceConfig, ServiceState, ServiceStatusSnapshot,
    };
    use windows_sys::Win32::System::Services::{
        SERVICE_AUTO_START, SERVICE_DEMAND_START, SERVICE_ERROR_NORMAL, SERVICE_START,
        SERVICE_WIN32_OWN_PROCESS,
    };

    use super::{
        absence_poll_action, ensure_captured_configuration_unchanged, observe_process_identity,
        process_capture_target, stopped_before_expected_state_error, stopping_process_is_complete,
        wait_for_process_exit, AbsenceObservation, AbsencePollAction, ProcessIdentityObservation,
        RECONFIGURE_ACCESS,
    };

    const PROCESS_EXIT_CHILD_ENV: &str = "MACTYPE_SETUP_PROCESS_EXIT_CHILD";

    fn owned_configuration(image_path: &str) -> ServiceConfig {
        ServiceConfig {
            service_type: SERVICE_WIN32_OWN_PROCESS,
            start_type: SERVICE_AUTO_START,
            error_control: SERVICE_ERROR_NORMAL,
            image_path: image_path.to_owned(),
            account: "LocalSystem".to_owned(),
            display_name: "MacType Control Center Service".to_owned(),
            load_order_group: String::new(),
            tag_id: 0,
            dependencies: Vec::new(),
        }
    }

    fn snapshot(state: ServiceState, process_id: u32) -> ServiceStatusSnapshot {
        ServiceStatusSnapshot {
            state,
            process_id,
            win32_exit_code: 0,
            service_specific_exit_code: 0,
        }
    }

    #[test]
    fn removal_uses_the_captured_scm_identity_after_the_service_stops() {
        let captured = owned_configuration(
            r#""C:\Program Files\MacType Control Center\Service\bin\0.2.0\mactype-service.exe" --service"#,
        );
        let unchanged = captured.clone();
        assert!(ensure_captured_configuration_unchanged(&captured, &unchanged).is_ok());

        let mut changed = unchanged;
        changed.image_path = r#""C:\foreign\mactype-service.exe" --service"#.to_owned();
        let error = ensure_captured_configuration_unchanged(&captured, &changed).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("image_path changed"));
        assert!(message.contains(r#"bin\0.2.0\mactype-service.exe"#));
        assert!(message.contains(r#"C:\foreign\mactype-service.exe"#));
    }

    #[test]
    fn configuration_tamper_diagnostic_names_every_changed_field_with_old_and_new() {
        let captured = owned_configuration(
            r#""C:\Program Files\MacType Control Center\Service\bin\0.2.0\mactype-service.exe" --service"#,
        );
        let mut changed = captured.clone();
        changed.display_name = "Foreign Service".to_owned();
        changed.start_type = SERVICE_DEMAND_START;

        let error = ensure_captured_configuration_unchanged(&captured, &changed).unwrap_err();
        let message = error.to_string();

        assert!(message
            .contains("the fixed service configuration changed after ownership was verified"));
        assert!(message.contains(&format!(
            "start_type changed ({SERVICE_AUTO_START} -> {SERVICE_DEMAND_START})"
        )));
        assert!(message.contains(
            "display_name changed ([MacType Control Center Service] -> [Foreign Service])"
        ));
        assert!(!message.contains("image_path changed"));
    }

    #[test]
    fn process_exit_wait_child() {
        if std::env::var_os(PROCESS_EXIT_CHILD_ENV).is_some() {
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    #[test]
    fn process_exit_wait_tracks_the_captured_process_until_it_signals() {
        let started = Instant::now();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "windows::scm::lifecycle::tests::process_exit_wait_child",
            ])
            .env(PROCESS_EXIT_CHILD_ENV, "1")
            .spawn()
            .unwrap();
        let process = Process::open(child.id(), ProcessAccess::Synchronize).unwrap();

        wait_for_process_exit(&process, Instant::now() + Duration::from_secs(5)).unwrap();

        assert!(child.wait().unwrap().success());
        assert!(
            started.elapsed() >= Duration::from_millis(100),
            "the wait returned before the captured child exited"
        );
    }

    #[test]
    fn non_stopped_service_without_a_pid_is_rejected_as_unknown_identity() {
        let status = snapshot(ServiceState::Running, 0);

        let error = process_capture_target(&status).unwrap_err();

        assert!(error.to_string().contains("without a process identity"));
    }

    #[test]
    fn process_that_exits_before_capture_is_safe_only_after_scm_reports_stopped() {
        let stopped = snapshot(ServiceState::Stopped, 0);
        let still_running = snapshot(ServiceState::Running, 42);

        assert_eq!(
            observe_process_identity(42, &stopped).unwrap(),
            ProcessIdentityObservation::Stopped
        );
        assert_eq!(
            observe_process_identity(42, &still_running).unwrap(),
            ProcessIdentityObservation::Same
        );
    }

    #[test]
    fn captured_process_identity_is_discarded_if_scm_changes_pid() {
        let same_process = snapshot(ServiceState::Running, 41);
        let replaced_process = snapshot(ServiceState::Running, 42);
        let stopped = snapshot(ServiceState::Stopped, 0);

        assert_eq!(
            observe_process_identity(41, &same_process).unwrap(),
            ProcessIdentityObservation::Same
        );
        assert_eq!(
            observe_process_identity(41, &replaced_process).unwrap(),
            ProcessIdentityObservation::Changed(42)
        );
        assert_eq!(
            observe_process_identity(41, &stopped).unwrap(),
            ProcessIdentityObservation::Stopped
        );
    }

    #[test]
    fn service_process_identity_change_after_stop_request_fails_closed() {
        let replacement = snapshot(ServiceState::Running, 42);

        let error = stopping_process_is_complete(41, &replacement).unwrap_err();

        assert!(error
            .to_string()
            .contains("process identity changed while stopping"));
    }

    #[test]
    fn service_reconfiguration_can_apply_restart_recovery_metadata() {
        assert_ne!(
            RECONFIGURE_ACCESS.rights() & SERVICE_START,
            0,
            "SC_ACTION_RESTART metadata requires a service handle with SERVICE_START"
        );
    }

    #[test]
    fn service_removal_waits_through_delete_pending_and_times_out_explicitly() {
        assert_eq!(
            absence_poll_action(AbsenceObservation::DeletePending, false).unwrap(),
            AbsencePollAction::Retry
        );
        assert_eq!(
            absence_poll_action(AbsenceObservation::Present, false).unwrap(),
            AbsencePollAction::Retry
        );
        assert_eq!(
            absence_poll_action(AbsenceObservation::Absent, false).unwrap(),
            AbsencePollAction::Complete
        );
        let error = absence_poll_action(AbsenceObservation::DeletePending, true).unwrap_err();
        assert!(error
            .to_string()
            .contains("service name remained delete-pending"));
    }

    #[test]
    fn stopped_service_diagnostic_includes_the_bounded_persisted_failure() {
        let directory = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let health_path = directory.path().join("health.json");
        let failure = HealthReport {
            protocol_version: HEALTH_PROTOCOL_VERSION,
            service_version: "0.2.0".to_owned(),
            health: HealthState::Failed,
            active_profile_digest: None,
            readiness: ReadinessReport::initializing(),
            injection: InjectionTelemetry::default(),
            last_error: Some(StructuredServiceError {
                code: "activation-recovery-required".to_owned(),
                message: "the activation receipt did not own the candidate".to_owned(),
                win32_error: None,
            }),
        };
        std::fs::write(&health_path, serde_json::to_vec(&failure).unwrap()).unwrap();
        let status = ServiceStatusSnapshot {
            state: ServiceState::Stopped,
            process_id: 0,
            win32_exit_code: 1066,
            service_specific_exit_code: 1,
        };

        let error =
            stopped_before_expected_state_error(&status, ServiceState::Running, Some(&health_path));
        let message = error.to_string();

        assert!(message.contains("win32=1066, service=1"));
        assert!(message.contains("activation-recovery-required"));
        assert!(message.contains("activation receipt did not own the candidate"));
    }
}
