use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mactype_service_contract::{HealthReport, HealthState, StructuredServiceError};
use mactype_service_host::{
    initialize_process_orchestration, BrokerDisposition, BrokerResult, HealthPublisher,
    InitializedRuntime, InjectionBroker, InjectionRequest, ProcessArchitecture, ProcessEventSource,
    ProcessIdentity, ProcessInspector, RuntimeInitializer, ServiceRuntime, ServiceStatus,
    SessionChange, StatusReporter, StopSignal, TargetLiveness,
};

const PROFILE_DIGEST: &str =
    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const RUNTIME_GENERATION: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct QueueSource {
    snapshot: Vec<u32>,
    pids: VecDeque<Option<u32>>,
}

impl ProcessEventSource for QueueSource {
    fn subscribe(&mut self, _query: &str) -> Result<(), StructuredServiceError> {
        Ok(())
    }

    fn snapshot_pids(&mut self) -> Result<Vec<u32>, StructuredServiceError> {
        Ok(self.snapshot.clone())
    }

    fn next_pid(&mut self, _timeout: Duration) -> Result<Option<u32>, StructuredServiceError> {
        Ok(self.pids.pop_front().unwrap_or(None))
    }
}

struct FixedInspector;

impl ProcessInspector for FixedInspector {
    fn inspect(&self, pid: u32) -> Result<ProcessIdentity, StructuredServiceError> {
        Ok(ProcessIdentity {
            pid,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
            critical: false,
        })
    }
}

struct SharedBroker {
    requests: Arc<Mutex<Vec<InjectionRequest>>>,
}

impl InjectionBroker for SharedBroker {
    fn verify_ready(
        &self,
        _architecture: ProcessArchitecture,
    ) -> Result<(), StructuredServiceError> {
        Ok(())
    }

    fn inject(&self, request: &InjectionRequest) -> BrokerResult {
        self.requests.lock().unwrap().push(request.clone());
        BrokerResult {
            disposition: BrokerDisposition::Injected,
            code: "module-loaded".to_owned(),
            win32_error: None,
        }
    }
}

struct TestInitializer {
    requests: Arc<Mutex<Vec<InjectionRequest>>>,
}

impl RuntimeInitializer for TestInitializer {
    fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError> {
        initialize_process_orchestration(
            Some(
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_owned(),
            ),
            900,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            Box::new(QueueSource {
                snapshot: vec![40, 41],
                pids: VecDeque::from([Some(42)]),
            }),
            Box::new(FixedInspector),
            Box::new(SharedBroker {
                requests: self.requests.clone(),
            }),
        )
    }
}

#[derive(Default)]
struct Recorder {
    reports: Mutex<Vec<HealthReport>>,
}

impl StatusReporter for Recorder {
    fn report(&self, _status: ServiceStatus) -> io::Result<()> {
        Ok(())
    }
}

impl HealthPublisher for Recorder {
    fn publish(&self, report: &HealthReport) -> io::Result<()> {
        self.reports.lock().unwrap().push(report.clone());
        Ok(())
    }
}

#[derive(Default)]
struct StopAfterOnePoll {
    polls: AtomicUsize,
}

#[derive(Default)]
struct StopAfterThreePolls {
    polls: AtomicUsize,
}

impl StopSignal for StopAfterThreePolls {
    fn wait(&self) -> Result<(), StructuredServiceError> {
        Ok(())
    }

    fn wait_timeout(&self, _timeout: Duration) -> Result<bool, StructuredServiceError> {
        Ok(self.polls.fetch_add(1, Ordering::AcqRel) > 2)
    }
}

impl StopSignal for StopAfterOnePoll {
    fn wait(&self) -> Result<(), StructuredServiceError> {
        Ok(())
    }

    fn wait_timeout(&self, _timeout: Duration) -> Result<bool, StructuredServiceError> {
        Ok(self.polls.fetch_add(1, Ordering::AcqRel) > 0)
    }

    fn take_session_change(&self) -> Option<SessionChange> {
        None
    }
}

#[test]
fn ready_driver_consumes_process_events_until_stop() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorder = Recorder::default();

    ServiceRuntime::new("0.2.0")
        .run(
            &recorder,
            &recorder,
            &TestInitializer {
                requests: requests.clone(),
            },
            &StopAfterThreePolls::default(),
        )
        .unwrap();

    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.identity.pid)
            .collect::<Vec<_>>(),
        [42, 40, 41]
    );
    let reports = recorder.reports.lock().unwrap();
    let terminal = reports.last().unwrap();
    assert_eq!(terminal.health, HealthState::Unknown);
    assert_eq!(
        terminal.readiness,
        mactype_service_contract::ReadinessReport::not_required()
    );
    assert!(terminal.last_error.is_none());
    assert!(terminal.active_profile_digest.is_none());
    let latest = reports
        .iter()
        .rev()
        .find(|report| report.health == HealthState::Ready)
        .unwrap();
    assert_eq!(latest.injection.x64.success_count, 3);
    assert_eq!(latest.injection.x86.success_count, 0);
    assert_eq!(
        latest
            .injection
            .x64
            .last_success
            .as_ref()
            .unwrap()
            .runtime_generation_id,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    );
}

struct TerminalBroker {
    requests: Arc<Mutex<Vec<InjectionRequest>>>,
}

impl InjectionBroker for TerminalBroker {
    fn verify_ready(
        &self,
        _architecture: ProcessArchitecture,
    ) -> Result<(), StructuredServiceError> {
        Ok(())
    }

    fn inject(&self, request: &InjectionRequest) -> BrokerResult {
        self.requests.lock().unwrap().push(request.clone());
        BrokerResult {
            disposition: BrokerDisposition::RetryableFailure,
            code: "remote-load-timeout".to_owned(),
            win32_error: Some(1460),
        }
    }
}

struct TerminalInitializer {
    requests: Arc<Mutex<Vec<InjectionRequest>>>,
}

impl RuntimeInitializer for TerminalInitializer {
    fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError> {
        initialize_process_orchestration(
            Some(
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_owned(),
            ),
            900,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            Box::new(QueueSource {
                snapshot: Vec::new(),
                pids: VecDeque::from([Some(42)]),
            }),
            Box::new(FixedInspector),
            Box::new(TerminalBroker {
                requests: self.requests.clone(),
            }),
        )
    }
}

#[test]
fn terminal_target_failure_keeps_global_ready_while_the_result_stays_process_local() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorder = Recorder::default();

    ServiceRuntime::new("0.2.0")
        .run(
            &recorder,
            &recorder,
            &TerminalInitializer {
                requests: requests.clone(),
            },
            &StopAfterOnePoll::default(),
        )
        .unwrap();

    assert_eq!(requests.lock().unwrap().len(), 1);
    let reports = recorder.reports.lock().unwrap();
    let latest = reports
        .iter()
        .rev()
        .find(|report| report.health == HealthState::Ready)
        .unwrap();
    assert_eq!(latest.health, HealthState::Ready);
    assert!(latest.last_error.is_none());
}

struct RecoveringBroker {
    attempts: AtomicUsize,
    requests: Arc<Mutex<Vec<InjectionRequest>>>,
    first_code: &'static str,
    first_win32_error: Option<u32>,
}

impl InjectionBroker for RecoveringBroker {
    fn verify_ready(
        &self,
        _architecture: ProcessArchitecture,
    ) -> Result<(), StructuredServiceError> {
        Ok(())
    }

    fn inject(&self, request: &InjectionRequest) -> BrokerResult {
        self.requests.lock().unwrap().push(request.clone());
        if self.attempts.fetch_add(1, Ordering::AcqRel) == 0 {
            BrokerResult {
                disposition: BrokerDisposition::Rejected,
                code: self.first_code.to_owned(),
                win32_error: self.first_win32_error,
            }
        } else {
            BrokerResult {
                disposition: BrokerDisposition::Injected,
                code: "module-loaded".to_owned(),
                win32_error: None,
            }
        }
    }
}

struct RecoveringInitializer {
    requests: Arc<Mutex<Vec<InjectionRequest>>>,
    first_code: &'static str,
    first_win32_error: Option<u32>,
}

impl RuntimeInitializer for RecoveringInitializer {
    fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError> {
        initialize_process_orchestration(
            Some(PROFILE_DIGEST.to_owned()),
            900,
            RUNTIME_GENERATION,
            Box::new(QueueSource {
                snapshot: Vec::new(),
                pids: VecDeque::from([Some(42), Some(43)]),
            }),
            Box::new(FixedInspector),
            Box::new(RecoveringBroker {
                attempts: AtomicUsize::new(0),
                requests: self.requests.clone(),
                first_code: self.first_code,
                first_win32_error: self.first_win32_error,
            }),
        )
    }
}

struct StopAfterTwoProcesses {
    polls: AtomicUsize,
}

impl StopSignal for StopAfterTwoProcesses {
    fn wait(&self) -> Result<(), StructuredServiceError> {
        Ok(())
    }

    fn wait_timeout(&self, _timeout: Duration) -> Result<bool, StructuredServiceError> {
        Ok(self.polls.fetch_add(1, Ordering::AcqRel) >= 2)
    }
}

#[test]
fn cleanup_unknown_degrades_its_generation_then_next_success_recovers_ready() {
    let recorder = Recorder::default();
    let requests = Arc::new(Mutex::new(Vec::new()));

    ServiceRuntime::new("0.2.0")
        .run(
            &recorder,
            &recorder,
            &RecoveringInitializer {
                requests: requests.clone(),
                first_code: "post-injection-state-cleanup-unknown",
                first_win32_error: Some(109),
            },
            &StopAfterTwoProcesses {
                polls: AtomicUsize::new(0),
            },
        )
        .unwrap();

    let reports = recorder.reports.lock().unwrap();
    let degraded = reports
        .iter()
        .find(|report| report.health == HealthState::Degraded)
        .expect("cleanup-unknown must be observable before recovery");
    let error = degraded.last_error.as_ref().unwrap();
    assert_eq!(error.code, "injection-cleanup-unknown");
    for identity in [
        "pid=42",
        "creation_time=100",
        "session_id=2",
        &format!("generation={RUNTIME_GENERATION}"),
    ] {
        assert!(error.message.contains(identity));
    }
    assert_eq!(error.win32_error, Some(109));
    let latest = reports
        .iter()
        .rev()
        .find(|report| report.health == HealthState::Ready)
        .unwrap();
    assert_eq!(latest.health, HealthState::Ready);
    assert!(latest.last_error.is_none());
    assert_eq!(latest.injection.x64.success_count, 1);
    assert_eq!(latest.injection.x64.last_success.as_ref().unwrap().pid, 43);
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.identity.pid)
            .collect::<Vec<_>>(),
        [42, 43]
    );
}

struct VanishedTargetInspector;

impl ProcessInspector for VanishedTargetInspector {
    fn inspect(&self, pid: u32) -> Result<ProcessIdentity, StructuredServiceError> {
        Ok(ProcessIdentity {
            pid,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
            critical: false,
        })
    }

    fn probe_target_liveness(&self, _identity: &ProcessIdentity) -> TargetLiveness {
        TargetLiveness::Vanished
    }
}

struct VanishedTargetInitializer {
    requests: Arc<Mutex<Vec<InjectionRequest>>>,
}

impl RuntimeInitializer for VanishedTargetInitializer {
    fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError> {
        initialize_process_orchestration(
            Some(PROFILE_DIGEST.to_owned()),
            900,
            RUNTIME_GENERATION,
            Box::new(QueueSource {
                snapshot: Vec::new(),
                pids: VecDeque::from([Some(42), Some(43)]),
            }),
            Box::new(VanishedTargetInspector),
            Box::new(RecoveringBroker {
                attempts: AtomicUsize::new(0),
                requests: self.requests.clone(),
                first_code: "post-injection-state-cleanup-unknown",
                first_win32_error: Some(299),
            }),
        )
    }
}

#[test]
fn cleanup_unknown_for_a_vanished_target_never_degrades_global_health() {
    let recorder = Recorder::default();
    let requests = Arc::new(Mutex::new(Vec::new()));

    ServiceRuntime::new("0.2.0")
        .run(
            &recorder,
            &recorder,
            &VanishedTargetInitializer {
                requests: requests.clone(),
            },
            &StopAfterTwoProcesses {
                polls: AtomicUsize::new(0),
            },
        )
        .unwrap();

    let reports = recorder.reports.lock().unwrap();
    assert!(
        reports
            .iter()
            .all(|report| report.health != HealthState::Degraded),
        "a proven target vanish must never latch Degraded health"
    );
    let latest = reports
        .iter()
        .rev()
        .find(|report| report.health == HealthState::Ready)
        .unwrap();
    assert_eq!(latest.health, HealthState::Ready);
    assert!(latest.last_error.is_none());
    assert_eq!(latest.injection.x64.success_count, 1);
    assert_eq!(latest.injection.x64.last_success.as_ref().unwrap().pid, 43);
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.identity.pid)
            .collect::<Vec<_>>(),
        [42, 43]
    );
}

#[test]
fn conflicting_mactype_module_stays_process_local_and_global_ready() {
    let recorder = Recorder::default();
    let requests = Arc::new(Mutex::new(Vec::new()));

    ServiceRuntime::new("0.2.0")
        .run(
            &recorder,
            &recorder,
            &RecoveringInitializer {
                requests: requests.clone(),
                first_code: "conflicting-mactype-module-loaded",
                first_win32_error: None,
            },
            &StopAfterTwoProcesses {
                polls: AtomicUsize::new(0),
            },
        )
        .unwrap();

    let reports = recorder.reports.lock().unwrap();
    assert!(reports
        .iter()
        .all(|report| report.health != HealthState::Degraded));
    let latest = reports
        .iter()
        .rev()
        .find(|report| report.health == HealthState::Ready)
        .unwrap();
    assert_eq!(latest.health, HealthState::Ready);
    assert!(latest.last_error.is_none());
    assert_eq!(latest.injection.x64.success_count, 1);
    assert_eq!(latest.injection.x64.last_success.as_ref().unwrap().pid, 43);
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.identity.pid)
            .collect::<Vec<_>>(),
        [42, 43]
    );
}

#[test]
fn invalid_helper_response_degrades_its_generation_then_next_success_recovers_ready() {
    let recorder = Recorder::default();
    let requests = Arc::new(Mutex::new(Vec::new()));

    ServiceRuntime::new("0.2.0")
        .run(
            &recorder,
            &recorder,
            &RecoveringInitializer {
                requests: requests.clone(),
                first_code: "helper-response-invalid",
                first_win32_error: None,
            },
            &StopAfterTwoProcesses {
                polls: AtomicUsize::new(0),
            },
        )
        .unwrap();

    let reports = recorder.reports.lock().unwrap();
    let degraded = reports
        .iter()
        .find(|report| report.health == HealthState::Degraded)
        .expect("invalid helper response must be observable before recovery");
    let error = degraded.last_error.as_ref().unwrap();
    assert_eq!(error.code, "injection-helper-response-integrity-unknown");
    assert!(error.message.contains("pid=42"));
    assert!(error
        .message
        .contains(&format!("generation={RUNTIME_GENERATION}")));
    let latest = reports
        .iter()
        .rev()
        .find(|report| report.health == HealthState::Ready)
        .unwrap();
    assert_eq!(latest.health, HealthState::Ready);
    assert!(latest.last_error.is_none());
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.identity.pid)
            .collect::<Vec<_>>(),
        [42, 43]
    );
}

struct TargetInspectionRaceInspector;

impl ProcessInspector for TargetInspectionRaceInspector {
    fn inspect(&self, pid: u32) -> Result<ProcessIdentity, StructuredServiceError> {
        let code = match pid {
            10 => Some("process-protected-or-inaccessible"),
            20 => Some("process-creation-time-unavailable"),
            30 => Some("process-session-unavailable"),
            40 => Some("process-architecture-unavailable"),
            50 => Some("process-architecture-unsupported"),
            _ => None,
        };
        if let Some(code) = code {
            return Err(StructuredServiceError {
                code: code.to_owned(),
                message: "the observed target changed during inspection".to_owned(),
                win32_error: Some(5),
            });
        }
        Ok(ProcessIdentity {
            pid,
            creation_time: u64::from(pid),
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
            critical: false,
        })
    }
}

struct TargetInspectionRaceInitializer {
    requests: Arc<Mutex<Vec<InjectionRequest>>>,
}

impl RuntimeInitializer for TargetInspectionRaceInitializer {
    fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError> {
        initialize_process_orchestration(
            Some(PROFILE_DIGEST.to_owned()),
            900,
            RUNTIME_GENERATION,
            Box::new(QueueSource {
                snapshot: Vec::new(),
                pids: VecDeque::from([
                    Some(10),
                    Some(11),
                    Some(20),
                    Some(21),
                    Some(30),
                    Some(31),
                    Some(40),
                    Some(41),
                    Some(50),
                    Some(51),
                ]),
            }),
            Box::new(TargetInspectionRaceInspector),
            Box::new(SharedBroker {
                requests: self.requests.clone(),
            }),
        )
    }
}

struct StopAfterTenProcesses {
    polls: AtomicUsize,
}

impl StopSignal for StopAfterTenProcesses {
    fn wait(&self) -> Result<(), StructuredServiceError> {
        Ok(())
    }

    fn wait_timeout(&self, _timeout: Duration) -> Result<bool, StructuredServiceError> {
        Ok(self.polls.fetch_add(1, Ordering::AcqRel) >= 10)
    }
}

#[test]
fn target_inspection_races_are_skipped_without_degrading_ready_or_blocking_the_next_target() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorder = Recorder::default();

    ServiceRuntime::new("0.2.0")
        .run(
            &recorder,
            &recorder,
            &TargetInspectionRaceInitializer {
                requests: requests.clone(),
            },
            &StopAfterTenProcesses {
                polls: AtomicUsize::new(0),
            },
        )
        .unwrap();

    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.identity.pid)
            .collect::<Vec<_>>(),
        [11, 21, 31, 41, 51]
    );
    let reports = recorder.reports.lock().unwrap();
    assert!(reports.iter().all(
        |report| report.health != HealthState::Degraded && report.health != HealthState::Failed
    ));
    let latest = reports
        .iter()
        .rev()
        .find(|report| report.health == HealthState::Ready)
        .unwrap();
    assert_eq!(latest.health, HealthState::Ready);
    assert!(latest.last_error.is_none());
    assert_eq!(latest.injection.x64.success_count, 5);
}

struct TelemetryInitializer {
    requests: Arc<Mutex<Vec<InjectionRequest>>>,
    process_count: usize,
}

impl RuntimeInitializer for TelemetryInitializer {
    fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError> {
        initialize_process_orchestration(
            Some(PROFILE_DIGEST.to_owned()),
            900,
            RUNTIME_GENERATION,
            Box::new(QueueSource {
                snapshot: Vec::new(),
                pids: (0..self.process_count)
                    .map(|index| Some(1000 + index as u32))
                    .collect(),
            }),
            Box::new(FixedInspector),
            Box::new(SharedBroker {
                requests: self.requests.clone(),
            }),
        )
    }
}

struct StopAfterProcesses {
    polls: AtomicUsize,
    process_count: usize,
}

impl StopSignal for StopAfterProcesses {
    fn wait(&self) -> Result<(), StructuredServiceError> {
        Ok(())
    }

    fn wait_timeout(&self, _timeout: Duration) -> Result<bool, StructuredServiceError> {
        Ok(self.polls.fetch_add(1, Ordering::AcqRel) >= self.process_count)
    }
}

#[derive(Clone, Copy)]
enum TelemetryFailurePattern {
    First(usize),
    Always,
    Intermittent,
}

struct TelemetryHealthPublisher {
    pattern: TelemetryFailurePattern,
    attempts: AtomicUsize,
    successes: AtomicUsize,
}

impl TelemetryHealthPublisher {
    fn new(pattern: TelemetryFailurePattern) -> Self {
        Self {
            pattern,
            attempts: AtomicUsize::new(0),
            successes: AtomicUsize::new(0),
        }
    }
}

impl HealthPublisher for TelemetryHealthPublisher {
    fn publish(&self, report: &HealthReport) -> io::Result<()> {
        if report.health != HealthState::Ready || report.injection.x64.success_count == 0 {
            return Ok(());
        }
        let attempt = self.attempts.fetch_add(1, Ordering::AcqRel) + 1;
        let fail = match self.pattern {
            TelemetryFailurePattern::First(count) => attempt <= count,
            TelemetryFailurePattern::Always => true,
            TelemetryFailurePattern::Intermittent => attempt % 2 == 1,
        };
        if fail {
            Err(io::Error::other("injected telemetry publish fault"))
        } else {
            self.successes.fetch_add(1, Ordering::AcqRel);
            Ok(())
        }
    }
}

#[test]
fn bounded_consecutive_health_report_failures_recover_and_stop_cleanly() {
    let process_count = 6;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let health = TelemetryHealthPublisher::new(TelemetryFailurePattern::First(4));
    let status = Recorder::default();

    ServiceRuntime::new("0.2.0")
        .run(
            &status,
            &health,
            &TelemetryInitializer {
                requests: requests.clone(),
                process_count,
            },
            &StopAfterProcesses {
                polls: AtomicUsize::new(0),
                process_count,
            },
        )
        .unwrap();

    assert_eq!(requests.lock().unwrap().len(), process_count);
    assert_eq!(health.attempts.load(Ordering::Acquire), process_count);
    assert_eq!(health.successes.load(Ordering::Acquire), 2);
}

#[test]
fn excessive_consecutive_health_report_failures_return_the_publish_error() {
    let process_count = 21;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let health = TelemetryHealthPublisher::new(TelemetryFailurePattern::Always);
    let status = Recorder::default();

    let error = ServiceRuntime::new("0.2.0")
        .run(
            &status,
            &health,
            &TelemetryInitializer {
                requests: requests.clone(),
                process_count,
            },
            &StopAfterProcesses {
                polls: AtomicUsize::new(0),
                process_count,
            },
        )
        .expect_err("the twenty-first consecutive telemetry failure must stop the driver");

    assert!(error.to_string().contains("health-publish-failed"));
    assert_eq!(requests.lock().unwrap().len(), process_count);
    assert_eq!(health.attempts.load(Ordering::Acquire), process_count);
    assert_eq!(health.successes.load(Ordering::Acquire), 0);
}

#[test]
fn intermittent_health_report_failures_reset_the_consecutive_failure_count() {
    let process_count = 42;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let health = TelemetryHealthPublisher::new(TelemetryFailurePattern::Intermittent);
    let status = Recorder::default();

    ServiceRuntime::new("0.2.0")
        .run(
            &status,
            &health,
            &TelemetryInitializer {
                requests: requests.clone(),
                process_count,
            },
            &StopAfterProcesses {
                polls: AtomicUsize::new(0),
                process_count,
            },
        )
        .unwrap();

    assert_eq!(requests.lock().unwrap().len(), process_count);
    assert_eq!(health.attempts.load(Ordering::Acquire), process_count);
    assert_eq!(health.successes.load(Ordering::Acquire), process_count / 2);
}
