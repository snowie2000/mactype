use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use mactype_service_contract::{
    ComponentReadiness, HealthReport, HealthState, ReadinessReport, StructuredServiceError,
};
use mactype_service_host::{
    CompositeHealthPublisher, FileHealthPublisher, HealthPublisher, InitializedRuntime,
    RuntimeDriver, RuntimeHealthReporter, RuntimeInitializer, ScmState, ServiceRuntime,
    ServiceStatus, StatusReporter, StopSignal, ACTIVE_PROFILE_ABSENT_CODE,
    RUNTIME_PROFILE_ABSENT_CODE,
};

const PROFILE: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[derive(Default)]
struct RecorderState {
    events: Vec<String>,
    reports: Vec<HealthReport>,
    statuses: Vec<ServiceStatus>,
}

#[derive(Default)]
struct Recorder {
    state: Mutex<RecorderState>,
}

impl Recorder {
    fn record(&self, event: String, report: Option<&HealthReport>, status: Option<ServiceStatus>) {
        let mut state = self.state.lock().unwrap();
        state.events.push(event);
        if let Some(report) = report {
            state.reports.push(report.clone());
        }
        if let Some(status) = status {
            state.statuses.push(status);
        }
    }
}

impl StatusReporter for Recorder {
    fn report(&self, status: ServiceStatus) -> io::Result<()> {
        self.record(format!("scm:{:?}", status.state), None, Some(status));
        Ok(())
    }
}

impl HealthPublisher for Recorder {
    fn publish(&self, report: &HealthReport) -> io::Result<()> {
        self.record(format!("health:{:?}", report.health), Some(report), None);
        Ok(())
    }
}

struct ReadyInitializer;

impl RuntimeInitializer for ReadyInitializer {
    fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError> {
        Ok(InitializedRuntime::ready(
            Some(PROFILE.to_owned()),
            ReadinessReport::ready(),
        ))
    }
}

struct ImmediateStop;

impl StopSignal for ImmediateStop {
    fn wait(&self) -> Result<(), StructuredServiceError> {
        Ok(())
    }
}

#[test]
fn driverless_graceful_stop_publishes_terminal_unknown_before_scm_stopped() {
    let recorder = Recorder::default();
    ServiceRuntime::new("0.2.0")
        .run(&recorder, &recorder, &ReadyInitializer, &ImmediateStop)
        .unwrap();

    let state = recorder.state.lock().unwrap();
    let terminal_health = state
        .events
        .iter()
        .position(|event| event == "health:Unknown")
        .unwrap();
    let stopped = state
        .events
        .iter()
        .position(|event| event == "scm:Stopped")
        .unwrap();
    assert_eq!(state.events.len(), 6);
    assert_eq!(state.events[0], "scm:StartPending");
    assert_eq!(state.events[1], "health:Initializing");
    assert_eq!(state.events[2], "scm:Running");
    assert_eq!(state.events[3], "health:Ready");
    assert_eq!(state.events[4], "health:Unknown");
    assert_eq!(state.events[5], "scm:Stopped");
    assert!(terminal_health < stopped);
    let terminal = state.reports.last().unwrap();
    assert_eq!(terminal.health, HealthState::Unknown);
    assert_eq!(terminal.readiness, ReadinessReport::not_required());
    assert_eq!(terminal.active_profile_digest, None);
    assert_eq!(terminal.last_error, None);
    assert_eq!(
        terminal.injection,
        mactype_service_contract::InjectionTelemetry::default()
    );
    assert!(terminal.validate().is_ok());
}

struct TerminalPublishFailureRecorder {
    events: Mutex<Vec<String>>,
}

impl StatusReporter for TerminalPublishFailureRecorder {
    fn report(&self, status: ServiceStatus) -> io::Result<()> {
        self.events
            .lock()
            .unwrap()
            .push(format!("scm:{:?}", status.state));
        Ok(())
    }
}

impl HealthPublisher for TerminalPublishFailureRecorder {
    fn publish(&self, report: &HealthReport) -> io::Result<()> {
        self.events
            .lock()
            .unwrap()
            .push(format!("health:{:?}", report.health));
        if report.health == HealthState::Unknown {
            Err(io::Error::other("injected terminal publish fault"))
        } else {
            Ok(())
        }
    }
}

#[test]
fn terminal_health_publish_failure_does_not_fail_graceful_stop() {
    let recorder = TerminalPublishFailureRecorder {
        events: Mutex::new(Vec::new()),
    };

    ServiceRuntime::new("0.2.0")
        .run(&recorder, &recorder, &ReadyInitializer, &ImmediateStop)
        .unwrap();

    let events = recorder.events.lock().unwrap();
    let terminal = events
        .iter()
        .position(|event| event == "health:Unknown")
        .unwrap();
    let stopped = events
        .iter()
        .position(|event| event == "scm:Stopped")
        .unwrap();
    assert!(terminal < stopped);
    assert!(!events.contains(&"health:Failed".to_owned()));
}

struct ErrorInitializer {
    code: &'static str,
}

impl RuntimeInitializer for ErrorInitializer {
    fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError> {
        Err(StructuredServiceError {
            code: self.code.to_owned(),
            message: "runtime initialization failed".to_owned(),
            win32_error: None,
        })
    }
}

#[test]
fn absent_profile_states_end_the_start_with_a_clean_stop() {
    for code in [ACTIVE_PROFILE_ABSENT_CODE, RUNTIME_PROFILE_ABSENT_CODE] {
        let recorder = Recorder::default();

        ServiceRuntime::new("0.2.0")
            .run(
                &recorder,
                &recorder,
                &ErrorInitializer { code },
                &ImmediateStop,
            )
            .unwrap();

        let state = recorder.state.lock().unwrap();
        assert_eq!(
            state.events,
            [
                "scm:StartPending",
                "health:Initializing",
                "health:Unknown",
                "scm:Stopped",
            ],
            "{code}"
        );
        assert_eq!(
            state.statuses,
            [
                ServiceStatus::start_pending(1, 10_000),
                ServiceStatus::stopped()
            ],
            "{code}"
        );
        assert_eq!(
            state
                .reports
                .iter()
                .map(|report| report.health)
                .collect::<Vec<_>>(),
            [HealthState::Initializing, HealthState::Unknown],
            "{code}"
        );
        let terminal = state.reports.last().unwrap();
        assert_eq!(terminal.health, HealthState::Unknown, "{code}");
        assert_eq!(terminal.active_profile_digest, None, "{code}");
        assert_eq!(
            terminal.readiness,
            ReadinessReport::not_required(),
            "{code}"
        );
        assert_eq!(
            terminal.injection,
            mactype_service_contract::InjectionTelemetry::default(),
            "{code}"
        );
        assert_eq!(
            terminal
                .last_error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some(code)
        );
        assert!(terminal.validate().is_ok(), "{code}");
        assert!(!state.events.contains(&"scm:Running".to_owned()), "{code}");
        assert!(!state.events.contains(&"health:Ready".to_owned()), "{code}");
        assert!(
            !state.events.contains(&"health:Failed".to_owned()),
            "{code}"
        );
    }
}

#[test]
fn invalid_runtime_file_set_still_fails_the_start() {
    let recorder = Recorder::default();

    assert!(ServiceRuntime::new("0.2.0")
        .run(
            &recorder,
            &recorder,
            &ErrorInitializer {
                code: "runtime-file-set-invalid",
            },
            &ImmediateStop,
        )
        .is_err());

    let state = recorder.state.lock().unwrap();
    assert_eq!(
        state.events,
        [
            "scm:StartPending",
            "health:Initializing",
            "health:Failed",
            "scm:Stopped",
        ]
    );
    assert_eq!(
        state.statuses,
        [
            ServiceStatus::start_pending(1, 10_000),
            ServiceStatus::stopped_with_error(1066, 1),
        ]
    );
    assert_eq!(
        state
            .reports
            .iter()
            .map(|report| report.health)
            .collect::<Vec<_>>(),
        [HealthState::Initializing, HealthState::Failed]
    );
    assert_eq!(state.reports.last().unwrap().health, HealthState::Failed);
    assert_eq!(
        state
            .reports
            .last()
            .unwrap()
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("runtime-file-set-invalid")
    );
    assert_eq!(
        state.statuses.last(),
        Some(&ServiceStatus::stopped_with_error(1066, 1))
    );
    assert!(!state.events.contains(&"scm:Running".to_owned()));
    assert!(!state.events.contains(&"health:Ready".to_owned()));
}

#[test]
fn initialization_failure_never_reports_running_or_ready() {
    let recorder = Recorder::default();
    assert!(ServiceRuntime::new("0.2.0")
        .run(
            &recorder,
            &recorder,
            &ErrorInitializer {
                code: "profile-invalid",
            },
            &ImmediateStop,
        )
        .is_err());
    let state = recorder.state.lock().unwrap();
    assert!(state.events.contains(&"health:Failed".to_owned()));
    assert!(state.events.contains(&"scm:Stopped".to_owned()));
    assert!(!state.events.contains(&"scm:Running".to_owned()));
    assert!(!state.events.contains(&"health:Ready".to_owned()));
}

#[derive(Clone, Copy)]
enum LifecycleFault {
    StartPendingStatus,
    InitializingHealth,
    RunningStatus,
    FinalStoppedStatus,
}

struct FaultRecorder {
    fault: LifecycleFault,
    failed_once: AtomicBool,
    events: Mutex<Vec<String>>,
}

impl FaultRecorder {
    fn new(fault: LifecycleFault) -> Self {
        Self {
            fault,
            failed_once: AtomicBool::new(false),
            events: Mutex::new(Vec::new()),
        }
    }

    fn fail_once(&self, matches: bool) -> io::Result<()> {
        if matches && !self.failed_once.swap(true, Ordering::AcqRel) {
            return Err(io::Error::other("injected lifecycle reporter fault"));
        }
        Ok(())
    }
}

impl StatusReporter for FaultRecorder {
    fn report(&self, status: ServiceStatus) -> io::Result<()> {
        self.events
            .lock()
            .unwrap()
            .push(format!("scm:{:?}:{}", status.state, status.win32_exit_code));
        self.fail_once(match self.fault {
            LifecycleFault::StartPendingStatus => status.state == ScmState::StartPending,
            LifecycleFault::RunningStatus => status.state == ScmState::Running,
            LifecycleFault::FinalStoppedStatus => {
                status.state == ScmState::Stopped && status.win32_exit_code == 0
            }
            LifecycleFault::InitializingHealth => false,
        })
    }
}

impl HealthPublisher for FaultRecorder {
    fn publish(&self, report: &HealthReport) -> io::Result<()> {
        self.events
            .lock()
            .unwrap()
            .push(format!("health:{:?}", report.health));
        self.fail_once(
            matches!(self.fault, LifecycleFault::InitializingHealth)
                && report.health == HealthState::Initializing,
        )
    }
}

#[test]
fn every_lifecycle_reporter_fault_attempts_failed_health_and_stopped_with_error() {
    for fault in [
        LifecycleFault::StartPendingStatus,
        LifecycleFault::InitializingHealth,
        LifecycleFault::RunningStatus,
        LifecycleFault::FinalStoppedStatus,
    ] {
        let recorder = FaultRecorder::new(fault);

        assert!(ServiceRuntime::new("0.2.0")
            .run(&recorder, &recorder, &ReadyInitializer, &ImmediateStop)
            .is_err());

        let events = recorder.events.lock().unwrap();
        assert!(events.contains(&"health:Failed".to_owned()), "{events:?}");
        assert!(
            events.contains(&"scm:Stopped:1066".to_owned()),
            "{events:?}"
        );
    }
}

#[test]
fn incomplete_required_readiness_is_reported_as_failed_before_exit() {
    struct IncompleteInitializer;
    impl RuntimeInitializer for IncompleteInitializer {
        fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError> {
            Ok(InitializedRuntime::ready(
                Some(PROFILE.to_owned()),
                ReadinessReport {
                    profile: ComponentReadiness::Ready,
                    observer: ComponentReadiness::Initializing,
                    injector32: ComponentReadiness::NotRequired,
                    injector64: ComponentReadiness::NotRequired,
                },
            ))
        }
    }

    let recorder = Recorder::default();
    assert!(ServiceRuntime::new("0.2.0")
        .run(&recorder, &recorder, &IncompleteInitializer, &ImmediateStop)
        .is_err());
    let state = recorder.state.lock().unwrap();
    assert!(state.events.contains(&"health:Failed".to_owned()));
    assert!(state.events.contains(&"scm:Stopped".to_owned()));
    assert!(!state.events.contains(&"health:Ready".to_owned()));
}

#[test]
fn file_health_adapter_writes_a_versioned_machine_readable_report() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("health.json");
    let publisher = FileHealthPublisher::new(path.clone());
    let report = HealthReport::ready("0.2.0", Some(PROFILE.to_owned()));

    publisher.publish(&report).unwrap();
    let decoded: HealthReport = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(decoded, report);
    assert_eq!(decoded.health, HealthState::Ready);
    assert_eq!(FileHealthPublisher::read(&path).unwrap(), report);

    std::fs::write(&path, vec![b'x'; 16 * 1024 + 1]).unwrap();
    assert!(FileHealthPublisher::read(&path).is_err());
}

#[test]
fn composite_health_publisher_updates_pipe_and_persisted_snapshot_together() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("health.json");
    let file = FileHealthPublisher::new(path.clone());
    let recorder = Recorder::default();
    let composite = CompositeHealthPublisher::new(&recorder, &file);
    let report = HealthReport::ready("0.2.0", Some(PROFILE.to_owned()));

    composite.publish(&report).unwrap();

    assert_eq!(FileHealthPublisher::read(&path).unwrap(), report);
    assert!(recorder
        .state
        .lock()
        .unwrap()
        .events
        .contains(&"health:Ready".to_owned()));
}

#[test]
fn persisted_health_cleans_only_bounded_owned_staging_residue() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("health.json");
    let stale = directory.path().join(".health.json.new-123-456-1");
    let unrelated = directory.path().join(".health.json.new-operator-note");
    std::fs::write(&stale, b"partial").unwrap();
    std::fs::write(&unrelated, b"preserve").unwrap();

    FileHealthPublisher::new(path)
        .publish(&HealthReport::ready("0.2.0", Some(PROFILE.to_owned())))
        .unwrap();

    assert!(!stale.exists());
    assert!(unrelated.exists());
}

#[test]
fn persisted_health_refuses_oversized_service_root_without_cleanup_or_replacement() {
    const OVERSIZED_SERVICE_ROOT_ENTRY_COUNT: usize = 4097;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("health.json");
    let stale = directory.path().join(".health.json.new-123-456-1");
    std::fs::write(&path, b"preserve-health").unwrap();
    std::fs::write(&stale, b"preserve-staging").unwrap();
    for index in 0..(OVERSIZED_SERVICE_ROOT_ENTRY_COUNT - 2) {
        std::fs::write(directory.path().join(format!("unrelated-{index}")), []).unwrap();
    }

    let error = FileHealthPublisher::new(path.clone())
        .publish(&HealthReport::ready("0.2.0", Some(PROFILE.to_owned())))
        .expect_err("oversized service root must fail closed");

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("entry count"), "{error}");
    assert_eq!(std::fs::read(path).unwrap(), b"preserve-health");
    assert_eq!(std::fs::read(stale).unwrap(), b"preserve-staging");
    assert_eq!(
        std::fs::read_dir(directory.path()).unwrap().count(),
        OVERSIZED_SERVICE_ROOT_ENTRY_COUNT
    );
}

#[test]
fn stop_statuses_are_nonzero_checkpoint_only_while_pending() {
    let recorder = Recorder::default();
    ServiceRuntime::new("0.2.0")
        .run(&recorder, &recorder, &ReadyInitializer, &ImmediateStop)
        .unwrap();

    let start = ServiceStatus::start_pending(1, 10_000);
    let stop = ServiceStatus::stop_pending(2, 5_000);
    assert_eq!(start.state, ScmState::StartPending);
    assert_eq!(start.checkpoint, 1);
    assert_eq!(stop.checkpoint, 2);
    assert_eq!(ServiceStatus::running().checkpoint, 0);
    assert_eq!(ServiceStatus::stopped().checkpoint, 0);
}

struct FailingDriver {
    wait_for_stop: bool,
}

impl RuntimeDriver for FailingDriver {
    fn run(
        &mut self,
        stop: &dyn StopSignal,
        _health: &dyn RuntimeHealthReporter,
    ) -> Result<(), StructuredServiceError> {
        if self.wait_for_stop {
            stop.wait()?;
        }
        Err(StructuredServiceError {
            code: "driver-wind-down-failed".to_owned(),
            message: "the runtime driver failed while winding down".to_owned(),
            win32_error: Some(31),
        })
    }
}

struct FailingDriverInitializer {
    wait_for_stop: bool,
}

impl RuntimeInitializer for FailingDriverInitializer {
    fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError> {
        Ok(InitializedRuntime::driven(
            Some(PROFILE.to_owned()),
            ReadinessReport::ready(),
            Box::new(FailingDriver {
                wait_for_stop: self.wait_for_stop,
            }),
        ))
    }
}

#[derive(Default)]
struct RequestedStop {
    requested: AtomicBool,
}

impl StopSignal for RequestedStop {
    fn wait(&self) -> Result<(), StructuredServiceError> {
        self.requested.store(true, Ordering::Release);
        Ok(())
    }

    fn stop_requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }
}

#[test]
fn driver_error_after_requested_stop_reports_clean_stop_with_terminal_error() {
    let recorder = Recorder::default();

    ServiceRuntime::new("0.2.0")
        .run(
            &recorder,
            &recorder,
            &FailingDriverInitializer {
                wait_for_stop: true,
            },
            &RequestedStop::default(),
        )
        .unwrap();

    let state = recorder.state.lock().unwrap();
    assert_eq!(state.statuses.last(), Some(&ServiceStatus::stopped()));
    assert_eq!(state.statuses.last().unwrap().win32_exit_code, 0);
    assert_eq!(state.statuses.last().unwrap().service_specific_exit_code, 0);
    let terminal = state.reports.last().unwrap();
    assert_eq!(terminal.health, HealthState::Unknown);
    assert_eq!(
        terminal.last_error,
        Some(StructuredServiceError {
            code: "driver-wind-down-failed".to_owned(),
            message: "the runtime driver failed while winding down".to_owned(),
            win32_error: Some(31),
        })
    );
    assert!(terminal.validate().is_ok());
}

#[test]
fn driver_error_without_requested_stop_reports_failure() {
    let recorder = Recorder::default();

    assert!(ServiceRuntime::new("0.2.0")
        .run(
            &recorder,
            &recorder,
            &FailingDriverInitializer {
                wait_for_stop: false,
            },
            &ImmediateStop,
        )
        .is_err());

    let state = recorder.state.lock().unwrap();
    let failure = state.reports.last().unwrap();
    assert_eq!(failure.health, HealthState::Failed);
    assert_eq!(
        failure.last_error.as_ref().unwrap().code,
        "driver-wind-down-failed"
    );
    assert_eq!(
        state.statuses.last(),
        Some(&ServiceStatus::stopped_with_error(1066, 1))
    );
}

struct RecordingDriver {
    ran: Arc<AtomicBool>,
}

impl RuntimeDriver for RecordingDriver {
    fn run(
        &mut self,
        stop: &dyn StopSignal,
        health: &dyn RuntimeHealthReporter,
    ) -> Result<(), StructuredServiceError> {
        self.ran.store(true, Ordering::Release);
        health.report(
            HealthState::Degraded,
            ReadinessReport::ready(),
            mactype_service_contract::InjectionTelemetry::default(),
            Some(StructuredServiceError {
                code: "transient-injection-miss".to_owned(),
                message: "a transient injection attempt failed".to_owned(),
                win32_error: None,
            }),
        )?;
        stop.wait()
    }
}

#[test]
fn driven_graceful_stop_replaces_degraded_with_terminal_unknown_before_scm_stopped() {
    struct DrivenInitializer {
        ran: Arc<AtomicBool>,
    }
    impl RuntimeInitializer for DrivenInitializer {
        fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError> {
            Ok(InitializedRuntime::driven(
                Some(PROFILE.to_owned()),
                ReadinessReport::ready(),
                Box::new(RecordingDriver {
                    ran: self.ran.clone(),
                }),
            ))
        }
    }

    let ran = Arc::new(AtomicBool::new(false));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("health.json");
    let persisted = FileHealthPublisher::new(path.clone());
    let live = Recorder::default();
    let status = Recorder::default();
    let composite = CompositeHealthPublisher::new(&live, &persisted);
    ServiceRuntime::new("0.2.0")
        .run(
            &status,
            &composite,
            &DrivenInitializer { ran: ran.clone() },
            &ImmediateStop,
        )
        .unwrap();

    assert!(ran.load(Ordering::Acquire));
    let live_state = live.state.lock().unwrap();
    let status_state = status.state.lock().unwrap();
    assert!(live_state
        .reports
        .iter()
        .any(|report| report.health == HealthState::Degraded));
    let terminal = live_state.reports.last().unwrap();
    assert_eq!(terminal.health, HealthState::Unknown);
    assert_eq!(terminal.readiness, ReadinessReport::not_required());
    assert_eq!(terminal.active_profile_digest, None);
    assert_eq!(terminal.last_error, None);
    assert_eq!(
        terminal.injection,
        mactype_service_contract::InjectionTelemetry::default()
    );
    assert!(terminal.validate().is_ok());
    assert_eq!(live_state.events.last().unwrap(), "health:Unknown");
    assert_eq!(status_state.events.last().unwrap(), "scm:Stopped");
    assert_eq!(FileHealthPublisher::read(&path).unwrap(), terminal.clone());
}
