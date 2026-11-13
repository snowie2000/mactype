#[path = "support/event_sink.rs"]
mod event_sink_support;

#[path = "support/inspector.rs"]
mod inspector_support;
#[path = "support/recorder.rs"]
mod recorder_support;

use event_sink_support::discard_events;
use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mactype_service_contract::{
    ComponentReadiness, HealthReport, HealthState, StructuredServiceError,
};
use mactype_service_host::{
    BrokerDisposition, BrokerResult, HealthPublisher, InitializedRuntime, InjectionBroker,
    InjectionRequest, InspectedProcess, ObserverRecoveryPolicy, ProcessArchitecture,
    ProcessEventSource, ProcessFacts, ProcessIdentity, ProcessInspector, RuntimeInitializer,
    ServiceRuntime, SessionChange, StopSignal, TargetLiveness, initialize_process_orchestration,
    initialize_process_orchestration_with_observer_recovery,
};

use inspector_support::{InspectorResponse, ScriptedInspector};
use recorder_support::Recorder;

const PROFILE_DIGEST: &str =
    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const RUNTIME_GENERATION: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn inspected(identity: ProcessIdentity) -> InspectedProcess {
    InspectedProcess {
        identity,
        facts: ProcessFacts {
            critical_or_unknown: false,
            prohibits_dynamic_code: false,
            restricts_binary_signature: false,
            image_name: Some("target.exe".to_owned()),
        },
    }
}

fn fixed_inspector(pids: impl IntoIterator<Item = u32>) -> ScriptedInspector {
    ScriptedInspector::new(pids.into_iter().map(|pid| {
        (
            pid,
            InspectorResponse::inspected(
                inspected(ProcessIdentity {
                    pid,
                    creation_time: 100,
                    session_id: 2,
                    architecture: ProcessArchitecture::X64,
                    protected: false,
                }),
                mactype_service_host::TargetLifecycle::Running,
            ),
        )
    }))
}

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

fn observer_error(code: &str) -> StructuredServiceError {
    StructuredServiceError {
        code: code.to_owned(),
        message: "the process observer failed".to_owned(),
        win32_error: Some(0x8004_1032),
    }
}

#[derive(Clone)]
struct ScriptedSourceHandle {
    state: Arc<Mutex<ScriptedSourceState>>,
}

struct ScriptedSourceState {
    subscribe_results: VecDeque<Result<(), StructuredServiceError>>,
    snapshots: VecDeque<Result<Vec<u32>, StructuredServiceError>>,
    next_results: VecDeque<Result<Option<u32>, StructuredServiceError>>,
    subscribe_calls: usize,
}

struct ScriptedSource {
    handle: ScriptedSourceHandle,
}

impl ProcessEventSource for ScriptedSource {
    fn subscribe(&mut self, _query: &str) -> Result<(), StructuredServiceError> {
        let mut state = self.handle.state.lock().unwrap();
        state.subscribe_calls += 1;
        state.subscribe_results.pop_front().unwrap_or(Ok(()))
    }

    fn snapshot_pids(&mut self) -> Result<Vec<u32>, StructuredServiceError> {
        self.handle
            .state
            .lock()
            .unwrap()
            .snapshots
            .pop_front()
            .unwrap_or(Ok(Vec::new()))
    }

    fn next_pid(&mut self, _timeout: Duration) -> Result<Option<u32>, StructuredServiceError> {
        self.handle
            .state
            .lock()
            .unwrap()
            .next_results
            .pop_front()
            .unwrap_or(Ok(None))
    }
}

fn scripted_source(
    subscribe_results: impl IntoIterator<Item = Result<(), StructuredServiceError>>,
    snapshots: impl IntoIterator<Item = Result<Vec<u32>, StructuredServiceError>>,
    next_results: impl IntoIterator<Item = Result<Option<u32>, StructuredServiceError>>,
) -> (ScriptedSource, ScriptedSourceHandle) {
    let handle = ScriptedSourceHandle {
        state: Arc::new(Mutex::new(ScriptedSourceState {
            subscribe_results: subscribe_results.into_iter().collect(),
            snapshots: snapshots.into_iter().collect(),
            next_results: next_results.into_iter().collect(),
            subscribe_calls: 0,
        })),
    };
    (
        ScriptedSource {
            handle: handle.clone(),
        },
        handle,
    )
}

fn recovery_policy() -> ObserverRecoveryPolicy {
    ObserverRecoveryPolicy {
        initial_delay: Duration::from_millis(1),
        max_delay: Duration::from_millis(2),
        max_attempts: 3,
    }
}

fn initialize_with_recovery_source(
    source: ScriptedSource,
    requests: Arc<Mutex<Vec<InjectionRequest>>>,
) -> Result<InitializedRuntime, StructuredServiceError> {
    initialize_process_orchestration_with_observer_recovery(
        Some(PROFILE_DIGEST.to_owned()),
        recovery_policy(),
        900,
        RUNTIME_GENERATION,
        Box::new(source),
        Box::new(fixed_inspector([42, 43, 44])),
        Box::new(SharedBroker { requests }),
        discard_events(),
    )
}

struct RecoveryInitializer {
    source: Mutex<Option<ScriptedSource>>,
    requests: Arc<Mutex<Vec<InjectionRequest>>>,
}

impl RuntimeInitializer for RecoveryInitializer {
    fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError> {
        initialize_with_recovery_source(
            self.source.lock().unwrap().take().unwrap(),
            self.requests.clone(),
        )
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
            Box::new(fixed_inspector([42, 40, 41])),
            Box::new(SharedBroker {
                requests: self.requests.clone(),
            }),
            discard_events(),
        )
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

struct StopAfterRecoveryWork {
    polls: AtomicUsize,
}

impl StopSignal for StopAfterRecoveryWork {
    fn wait(&self) -> Result<(), StructuredServiceError> {
        Ok(())
    }

    fn wait_timeout(&self, _timeout: Duration) -> Result<bool, StructuredServiceError> {
        Ok(self.polls.fetch_add(1, Ordering::AcqRel) >= 5)
    }
}

struct NeverStop;

impl StopSignal for NeverStop {
    fn wait(&self) -> Result<(), StructuredServiceError> {
        Ok(())
    }

    fn wait_timeout(&self, _timeout: Duration) -> Result<bool, StructuredServiceError> {
        Ok(false)
    }
}

struct StopDuringRecoveryWait;

impl StopSignal for StopDuringRecoveryWait {
    fn wait(&self) -> Result<(), StructuredServiceError> {
        Ok(())
    }

    fn wait_timeout(&self, timeout: Duration) -> Result<bool, StructuredServiceError> {
        Ok(!timeout.is_zero())
    }
}

#[test]
fn observer_failure_resubscribes_and_reconciles_the_missed_snapshot() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorder = Recorder::default();
    let (source, source_handle) = scripted_source(
        [Ok(()), Ok(())],
        [Ok(Vec::new()), Ok(vec![43, 44])],
        [Ok(Some(42)), Err(observer_error("observer-wait-failed"))],
    );

    ServiceRuntime::new("0.2.0", discard_events())
        .run(
            &recorder,
            &recorder,
            &RecoveryInitializer {
                source: Mutex::new(Some(source)),
                requests: requests.clone(),
            },
            &StopAfterRecoveryWork {
                polls: AtomicUsize::new(0),
            },
        )
        .unwrap();

    assert_eq!(source_handle.state.lock().unwrap().subscribe_calls, 2);
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.identity.pid)
            .collect::<Vec<_>>(),
        [42, 43, 44]
    );
    let reports = recorder.reports();
    let degraded_index = reports
        .iter()
        .position(|report| {
            report.health == HealthState::Degraded
                && report.readiness.observer == ComponentReadiness::Failed
        })
        .unwrap();
    assert!(
        reports[degraded_index + 1..]
            .iter()
            .any(|report| report.health == HealthState::Ready)
    );
}

#[test]
fn observer_recovery_gives_up_after_the_configured_attempts() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorder = Recorder::default();
    let (source, source_handle) = scripted_source(
        [
            Ok(()),
            Err(observer_error("observer-resubscribe-1")),
            Err(observer_error("observer-resubscribe-2")),
            Err(observer_error("observer-resubscribe-3")),
        ],
        [Ok(Vec::new())],
        [Err(observer_error("observer-wait-failed"))],
    );

    let error = ServiceRuntime::new("0.2.0", discard_events())
        .run(
            &recorder,
            &recorder,
            &RecoveryInitializer {
                source: Mutex::new(Some(source)),
                requests,
            },
            &NeverStop,
        )
        .expect_err("exhausted observer recovery must fail the service");

    assert!(error.to_string().contains("observer-resubscribe-3"));
    assert_eq!(source_handle.state.lock().unwrap().subscribe_calls, 4);
    let reports = recorder.reports();
    let terminal_runtime_health = reports
        .iter()
        .rev()
        .find(|report| report.active_profile_digest.is_some())
        .unwrap();
    assert_eq!(terminal_runtime_health.health, HealthState::Failed);
    assert_eq!(
        terminal_runtime_health.readiness.observer,
        ComponentReadiness::Failed
    );
}

#[test]
fn observer_recovery_stops_when_the_service_stops() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorder = Recorder::default();
    let (source, source_handle) = scripted_source(
        [Ok(())],
        [Ok(Vec::new())],
        [Err(observer_error("observer-wait-failed"))],
    );

    ServiceRuntime::new("0.2.0", discard_events())
        .run(
            &recorder,
            &recorder,
            &RecoveryInitializer {
                source: Mutex::new(Some(source)),
                requests,
            },
            &StopDuringRecoveryWait,
        )
        .unwrap();

    assert_eq!(source_handle.state.lock().unwrap().subscribe_calls, 1);
}

#[test]
fn ready_driver_consumes_process_events_until_stop() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorder = Recorder::default();

    ServiceRuntime::new("0.2.0", discard_events())
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
    let reports = recorder.reports();
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
            Box::new(fixed_inspector([42])),
            Box::new(TerminalBroker {
                requests: self.requests.clone(),
            }),
            discard_events(),
        )
    }
}

#[test]
fn terminal_target_failure_keeps_global_ready_while_the_result_stays_process_local() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorder = Recorder::default();

    ServiceRuntime::new("0.2.0", discard_events())
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
    let reports = recorder.reports();
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
            Box::new(fixed_inspector([42, 43])),
            Box::new(RecoveringBroker {
                attempts: AtomicUsize::new(0),
                requests: self.requests.clone(),
                first_code: self.first_code,
                first_win32_error: self.first_win32_error,
            }),
            discard_events(),
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

    ServiceRuntime::new("0.2.0", discard_events())
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

    let reports = recorder.reports();
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
    fn inspect(&self, pid: u32) -> Result<InspectedProcess, StructuredServiceError> {
        Ok(inspected(ProcessIdentity {
            pid,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
        }))
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
            discard_events(),
        )
    }
}

#[test]
fn cleanup_unknown_for_a_vanished_target_never_degrades_global_health() {
    let recorder = Recorder::default();
    let requests = Arc::new(Mutex::new(Vec::new()));

    ServiceRuntime::new("0.2.0", discard_events())
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

    let reports = recorder.reports();
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

    ServiceRuntime::new("0.2.0", discard_events())
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

    let reports = recorder.reports();
    assert!(
        reports
            .iter()
            .all(|report| report.health != HealthState::Degraded)
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
fn invalid_helper_response_degrades_its_generation_then_next_success_recovers_ready() {
    let recorder = Recorder::default();
    let requests = Arc::new(Mutex::new(Vec::new()));

    ServiceRuntime::new("0.2.0", discard_events())
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

    let reports = recorder.reports();
    let degraded = reports
        .iter()
        .find(|report| report.health == HealthState::Degraded)
        .expect("invalid helper response must be observable before recovery");
    let error = degraded.last_error.as_ref().unwrap();
    assert_eq!(error.code, "injection-helper-response-integrity-unknown");
    assert!(error.message.contains("pid=42"));
    assert!(
        error
            .message
            .contains(&format!("generation={RUNTIME_GENERATION}"))
    );
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
    fn inspect(&self, pid: u32) -> Result<InspectedProcess, StructuredServiceError> {
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
        Ok(inspected(ProcessIdentity {
            pid,
            creation_time: u64::from(pid),
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
        }))
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
            discard_events(),
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

    ServiceRuntime::new("0.2.0", discard_events())
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
    let reports = recorder.reports();
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
            Box::new(fixed_inspector(
                (0..self.process_count).map(|index| 1000 + index as u32),
            )),
            Box::new(SharedBroker {
                requests: self.requests.clone(),
            }),
            discard_events(),
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

    ServiceRuntime::new("0.2.0", discard_events())
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

    let error = ServiceRuntime::new("0.2.0", discard_events())
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

    ServiceRuntime::new("0.2.0", discard_events())
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
