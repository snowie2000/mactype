#[path = "support/event_sink.rs"]
mod event_sink_support;

#[path = "support/broker.rs"]
mod broker_support;
#[path = "support/inspector.rs"]
mod inspector_support;

use event_sink_support::{RecordingEventSink, discard_events};
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use mactype_service_contract::StructuredServiceError;
use mactype_service_contract::event_log::read_events;
use mactype_service_host::{
    BrokerDisposition, BrokerResult, HostEvent, HostEventSink, InjectionBroker, InjectionRequest,
    InspectedProcess, MAX_TRACKED_PROCESS_RESULTS, PROCESS_CREATION_QUERY, ProcessArchitecture,
    ProcessEventSource, ProcessFacts, ProcessIdentity, ProcessInspector, ProcessOrchestrator,
    ProcessOutcome, RetryPolicy, RetryScheduler, SessionChange, TARGET_VANISHED_RESULT_CODE,
    TargetLiveness, initialize_process_orchestration, subscribe_process_creation,
};

use broker_support::ScriptedBroker;
use inspector_support::{InspectorResponse, ScriptedInspector};

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

fn fixed_inspector(identity: ProcessIdentity, inspections: usize) -> ScriptedInspector {
    let pid = identity.pid;
    ScriptedInspector::new((0..inspections).map(move |_| {
        (
            pid,
            InspectorResponse::inspected(
                inspected(identity.clone()),
                mactype_service_host::TargetLifecycle::Running,
            ),
        )
    }))
}

#[derive(Default)]
struct RecordingEventSource {
    query: Option<String>,
}

impl ProcessEventSource for RecordingEventSource {
    fn subscribe(&mut self, query: &str) -> Result<(), StructuredServiceError> {
        self.query = Some(query.to_owned());
        Ok(())
    }

    fn next_pid(&mut self, _timeout: Duration) -> Result<Option<u32>, StructuredServiceError> {
        Ok(None)
    }
}

#[test]
fn observer_subscribes_to_immediate_process_start_events() {
    let mut source = RecordingEventSource::default();

    subscribe_process_creation(&mut source).unwrap();

    assert_eq!(
        PROCESS_CREATION_QUERY,
        "SELECT ProcessID FROM Win32_ProcessStartTrace"
    );
    assert_eq!(source.query.as_deref(), Some(PROCESS_CREATION_QUERY));
}

#[derive(Default)]
struct RecordingBroker {
    requests: Mutex<Vec<InjectionRequest>>,
}

impl InjectionBroker for RecordingBroker {
    fn inject(&self, request: &InjectionRequest) -> BrokerResult {
        self.requests.lock().unwrap().push(request.clone());
        BrokerResult {
            disposition: BrokerDisposition::Injected,
            code: "module-loaded".to_owned(),
            win32_error: None,
        }
    }
}

#[test]
fn process_identity_is_requeried_before_the_fixed_broker_request() {
    let identity = ProcessIdentity {
        pid: 42,
        creation_time: 133_967_890_123_456_789,
        session_id: 2,
        architecture: ProcessArchitecture::X64,
        protected: false,
    };
    let inspector = fixed_inspector(identity.clone(), 1);
    let broker = RecordingBroker::default();
    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        discard_events(),
    );

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Injected
    );
    assert_eq!(
        *broker.requests.lock().unwrap(),
        [InjectionRequest {
            identity,
            generation_id: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_owned(),
        }]
    );
}

#[test]
fn session_zero_service_self_and_protected_targets_are_skipped() {
    for identity in [
        ProcessIdentity {
            pid: 900,
            creation_time: 1,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
        },
        ProcessIdentity {
            pid: 42,
            creation_time: 2,
            session_id: 0,
            architecture: ProcessArchitecture::X64,
            protected: false,
        },
        ProcessIdentity {
            pid: 43,
            creation_time: 3,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: true,
        },
    ] {
        let inspector = fixed_inspector(identity.clone(), 1);
        let broker = RecordingBroker::default();
        let mut orchestrator = ProcessOrchestrator::new(
            900,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            &inspector,
            &broker,
            discard_events(),
        );

        assert_eq!(
            orchestrator.handle_pid(identity.pid).unwrap(),
            ProcessOutcome::Skipped
        );
        assert!(broker.requests.lock().unwrap().is_empty());
    }
}

struct FailingInspector {
    error: StructuredServiceError,
}

impl ProcessInspector for FailingInspector {
    fn inspect(&self, _pid: u32) -> Result<InspectedProcess, StructuredServiceError> {
        Err(self.error.clone())
    }
}

struct MismatchedInspector;

impl ProcessInspector for MismatchedInspector {
    fn inspect(&self, _pid: u32) -> Result<InspectedProcess, StructuredServiceError> {
        Ok(inspected(ProcessIdentity {
            pid: 77,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
        }))
    }
}

#[test]
fn unknown_inspector_failures_and_identity_mismatch_remain_errors_without_helper_retry() {
    let broker = RecordingBroker::default();
    let inspector = FailingInspector {
        error: StructuredServiceError {
            code: "inspector-infrastructure-failed".to_owned(),
            message: "the inspector adapter failed".to_owned(),
            win32_error: Some(1722),
        },
    };
    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        discard_events(),
    );

    let error = orchestrator.handle_pid(42).unwrap_err();
    assert_eq!(error.code, "inspector-infrastructure-failed");
    assert!(broker.requests.lock().unwrap().is_empty());

    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &MismatchedInspector,
        &broker,
        discard_events(),
    );
    let error = orchestrator.handle_pid(42).unwrap_err();
    assert_eq!(error.code, "process-identity-mismatch");
    assert!(broker.requests.lock().unwrap().is_empty());
}

struct SequenceInspector {
    identities: Mutex<VecDeque<ProcessIdentity>>,
}

impl ProcessInspector for SequenceInspector {
    fn inspect(&self, pid: u32) -> Result<InspectedProcess, StructuredServiceError> {
        let identity = self.identities.lock().unwrap().pop_front().unwrap();
        assert_eq!(identity.pid, pid);
        Ok(inspected(identity))
    }
}

#[test]
fn duplicate_identity_is_suppressed_but_a_reused_pid_with_new_creation_time_is_processed() {
    let identity = ProcessIdentity {
        pid: 42,
        creation_time: 100,
        session_id: 2,
        architecture: ProcessArchitecture::X64,
        protected: false,
    };
    let reused = ProcessIdentity {
        creation_time: 101,
        ..identity.clone()
    };
    let inspector = SequenceInspector {
        identities: Mutex::new(VecDeque::from([identity.clone(), identity, reused])),
    };
    let broker = RecordingBroker::default();
    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        discard_events(),
    );

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Injected
    );
    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Duplicate
    );
    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Injected
    );
    assert_eq!(broker.requests.lock().unwrap().len(), 2);
}

#[derive(Default)]
struct RecordingScheduler {
    waits: Mutex<Vec<Duration>>,
}

impl RetryScheduler for RecordingScheduler {
    fn wait(&self, delay: Duration) -> bool {
        self.waits.lock().unwrap().push(delay);
        true
    }
}

#[test]
fn retryable_helper_failures_use_bounded_exponential_backoff() {
    let identity = ProcessIdentity {
        pid: 42,
        creation_time: 100,
        session_id: 2,
        architecture: ProcessArchitecture::X64,
        protected: false,
    };
    let inspector = fixed_inspector(identity, 1);
    let broker = ScriptedBroker::new([
        BrokerResult {
            disposition: BrokerDisposition::RetryableFailure,
            code: "session-unavailable".to_owned(),
            win32_error: Some(5),
        },
        BrokerResult {
            disposition: BrokerDisposition::RetryableFailure,
            code: "architecture-unavailable".to_owned(),
            win32_error: Some(5),
        },
        BrokerResult {
            disposition: BrokerDisposition::Injected,
            code: "module-loaded".to_owned(),
            win32_error: None,
        },
    ]);
    let scheduler = RecordingScheduler::default();
    let mut orchestrator = ProcessOrchestrator::with_retry_policy(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        RetryPolicy {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(25),
        },
        &scheduler,
        discard_events(),
    );

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Injected
    );
    assert_eq!(broker.request_count(), 3);
    assert_eq!(
        *scheduler.waits.lock().unwrap(),
        [Duration::from_millis(10), Duration::from_millis(20)]
    );
}

#[test]
fn each_explicitly_safe_transient_code_retries_the_same_process_identity() {
    for code in [
        "session-unavailable",
        "identity-unavailable",
        "architecture-unavailable",
        "module-inventory-unavailable",
    ] {
        let inspector = fixed_inspector(
            ProcessIdentity {
                pid: 42,
                creation_time: 100,
                session_id: 2,
                architecture: ProcessArchitecture::X64,
                protected: false,
            },
            1,
        );
        let broker = ScriptedBroker::new([
            BrokerResult {
                disposition: BrokerDisposition::RetryableFailure,
                code: code.to_owned(),
                win32_error: Some(5),
            },
            BrokerResult {
                disposition: BrokerDisposition::Injected,
                code: "module-loaded".to_owned(),
                win32_error: None,
            },
        ]);
        let scheduler = RecordingScheduler::default();
        let mut orchestrator = ProcessOrchestrator::with_retry_policy(
            900,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            &inspector,
            &broker,
            RetryPolicy::default(),
            &scheduler,
            discard_events(),
        );

        assert_eq!(
            orchestrator.handle_pid(42).unwrap(),
            ProcessOutcome::Injected,
            "{code} must be the bounded safe-retry allowlist"
        );
        assert_eq!(broker.request_count(), 2);
        assert_eq!(scheduler.waits.lock().unwrap().len(), 1);
    }
}

#[test]
fn only_explicitly_safe_transient_codes_can_retry_the_same_process_identity() {
    for code in [
        "remote-load-timeout",
        "remote-load-cleanup-unknown",
        "remote-wait-cleanup-unknown",
        "remote-memory-cleanup-unknown",
        "post-injection-state-cleanup-unknown",
        "helper-absolute-timeout-cleanup-unknown",
        "helper-launch-failed-cleanup-unknown",
        "helper-response-invalid",
        "helper-exit-mismatch",
        "protected-or-inaccessible",
        "fixed-module-missing",
        "remote-allocation-failed",
        "remote-write-failed",
        "loader-address-unavailable",
        "remote-thread-failed",
    ] {
        let inspector = fixed_inspector(
            ProcessIdentity {
                pid: 42,
                creation_time: 100,
                session_id: 2,
                architecture: ProcessArchitecture::X64,
                protected: false,
            },
            1,
        );
        let broker = ScriptedBroker::new([BrokerResult {
            disposition: BrokerDisposition::RetryableFailure,
            code: code.to_owned(),
            win32_error: Some(5),
        }]);
        let scheduler = RecordingScheduler::default();
        let mut orchestrator = ProcessOrchestrator::with_retry_policy(
            900,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            &inspector,
            &broker,
            RetryPolicy::default(),
            &scheduler,
            discard_events(),
        );

        assert_eq!(
            orchestrator.handle_pid(42).unwrap(),
            ProcessOutcome::Rejected
        );
        assert_eq!(broker.request_count(), 1);
        assert!(scheduler.waits.lock().unwrap().is_empty());
        let result = orchestrator.last_result(42, 100).unwrap();
        assert_eq!(result.attempts, 1);
        assert_eq!(result.code, code);
    }
}

struct CancellingScheduler;

impl RetryScheduler for CancellingScheduler {
    fn wait(&self, _delay: Duration) -> bool {
        false
    }
}

#[test]
fn stop_or_shutdown_cancels_retry_without_another_helper_launch() {
    let inspector = fixed_inspector(
        ProcessIdentity {
            pid: 42,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
        },
        1,
    );
    let broker = ScriptedBroker::new([BrokerResult {
        disposition: BrokerDisposition::RetryableFailure,
        code: "session-unavailable".to_owned(),
        win32_error: None,
    }]);
    let mut orchestrator = ProcessOrchestrator::with_retry_policy(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        RetryPolicy::default(),
        &CancellingScheduler,
        discard_events(),
    );

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Cancelled
    );
    assert_eq!(broker.request_count(), 1);
}

#[test]
fn exhausted_retry_records_the_last_bounded_process_result() {
    let identity = ProcessIdentity {
        pid: 42,
        creation_time: 100,
        session_id: 2,
        architecture: ProcessArchitecture::X64,
        protected: false,
    };
    let inspector = fixed_inspector(identity.clone(), 1);
    let broker = ScriptedBroker::new([
        BrokerResult {
            disposition: BrokerDisposition::RetryableFailure,
            code: "identity-unavailable".to_owned(),
            win32_error: Some(87),
        },
        BrokerResult {
            disposition: BrokerDisposition::RetryableFailure,
            code: "module-inventory-unavailable".to_owned(),
            win32_error: Some(1460),
        },
    ]);
    let scheduler = RecordingScheduler::default();
    let mut orchestrator = ProcessOrchestrator::with_retry_policy(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        RetryPolicy {
            max_attempts: 2,
            initial_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        },
        &scheduler,
        discard_events(),
    );

    assert_eq!(
        orchestrator.handle_pid(identity.pid).unwrap(),
        ProcessOutcome::RetryExhausted
    );
    let result = orchestrator
        .last_result(identity.pid, identity.creation_time)
        .expect("the final bounded attempt must remain observable");
    assert_eq!(result.identity, identity);
    assert_eq!(
        result.runtime_generation_id,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    );
    assert_eq!(result.outcome, ProcessOutcome::RetryExhausted);
    assert_eq!(result.attempts, 2);
    assert_eq!(result.code, "module-inventory-unavailable");
    assert_eq!(result.win32_error, Some(1460));
}

struct IdentityFromPidInspector;

impl ProcessInspector for IdentityFromPidInspector {
    fn inspect(&self, pid: u32) -> Result<InspectedProcess, StructuredServiceError> {
        Ok(inspected(ProcessIdentity {
            pid,
            creation_time: u64::from(pid),
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
        }))
    }
}

#[test]
fn process_result_memory_is_bounded_and_evicts_the_oldest_identity() {
    let broker = RecordingBroker::default();
    let mut orchestrator = ProcessOrchestrator::new(
        u32::MAX,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &IdentityFromPidInspector,
        &broker,
        discard_events(),
    );

    for pid in 1..=(MAX_TRACKED_PROCESS_RESULTS as u32 + 1) {
        assert_eq!(
            orchestrator.handle_pid(pid).unwrap(),
            ProcessOutcome::Injected
        );
    }

    assert_eq!(
        orchestrator.tracked_process_count(),
        MAX_TRACKED_PROCESS_RESULTS
    );
    assert!(orchestrator.last_result(1, 1).is_none());
    assert!(
        orchestrator
            .last_result(
                MAX_TRACKED_PROCESS_RESULTS as u32 + 1,
                MAX_TRACKED_PROCESS_RESULTS as u64 + 1,
            )
            .is_some()
    );
}

#[test]
fn wts_logoff_clears_dedupe_state_for_that_session() {
    let identity = ProcessIdentity {
        pid: 42,
        creation_time: 100,
        session_id: 2,
        architecture: ProcessArchitecture::X64,
        protected: false,
    };
    let inspector = fixed_inspector(identity, 3);
    let broker = RecordingBroker::default();
    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        discard_events(),
    );
    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Injected
    );
    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Duplicate
    );

    orchestrator.handle_session_change(SessionChange {
        event_type: 6,
        session_id: 2,
    });

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Injected
    );
    assert_eq!(broker.requests.lock().unwrap().len(), 2);
}

#[test]
fn session_queue_overflow_clears_all_dedupe_state() {
    let identity = ProcessIdentity {
        pid: 42,
        creation_time: 100,
        session_id: 2,
        architecture: ProcessArchitecture::X64,
        protected: false,
    };
    let inspector = fixed_inspector(identity, 3);
    let broker = RecordingBroker::default();
    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        discard_events(),
    );
    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Injected
    );
    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Duplicate
    );

    orchestrator.handle_session_change(SessionChange::overflow());

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Injected
    );
}

struct ServiceStopBroker;

impl InjectionBroker for ServiceStopBroker {
    fn inject(&self, _request: &InjectionRequest) -> BrokerResult {
        BrokerResult {
            disposition: BrokerDisposition::Cancelled,
            code: "helper-cancelled-service-stop".to_owned(),
            win32_error: None,
        }
    }
}

struct LateSuccessBroker;

impl InjectionBroker for LateSuccessBroker {
    fn inject(&self, _request: &InjectionRequest) -> BrokerResult {
        BrokerResult {
            disposition: BrokerDisposition::Injected,
            code: "module-loaded-late".to_owned(),
            win32_error: None,
        }
    }
}

#[test]
fn verified_late_success_records_generation_bound_telemetry() {
    let inspector = fixed_inspector(
        ProcessIdentity {
            pid: 42,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
        },
        1,
    );
    let scheduler = RecordingScheduler::default();
    let mut orchestrator = ProcessOrchestrator::with_runtime_context(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        &inspector,
        &LateSuccessBroker,
        RetryPolicy::default(),
        &scheduler,
        discard_events(),
    );

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Injected
    );
    let telemetry = orchestrator.injection_telemetry();
    assert_eq!(telemetry.x64.success_count, 1);
    assert_eq!(telemetry.x86.success_count, 0);
    let success = telemetry.x64.last_success.unwrap();
    assert_eq!(success.pid, 42);
    assert_eq!(success.creation_time, 100);
    assert_eq!(success.session_id, 2);
    assert_eq!(
        success.runtime_generation_id,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    );
}

#[test]
fn service_stop_cancellation_is_not_retried_or_classified_as_degraded() {
    let inspector = fixed_inspector(
        ProcessIdentity {
            pid: 42,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
        },
        1,
    );
    let broker = ServiceStopBroker;
    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        discard_events(),
    );

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Cancelled
    );
    let record = orchestrator.last_result(42, 100).unwrap();
    assert_eq!(record.outcome, ProcessOutcome::Cancelled);
    assert_eq!(record.attempts, 1);
}

#[test]
fn post_resume_service_stop_is_terminal_and_degrades_its_generation() {
    let inspector = fixed_inspector(
        ProcessIdentity {
            pid: 42,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
        },
        1,
    );
    let broker = ScriptedBroker::new([BrokerResult {
        disposition: BrokerDisposition::Rejected,
        code: "helper-service-stop-cleanup-unknown".to_owned(),
        win32_error: Some(1223),
    }]);
    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        discard_events(),
    );

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Rejected
    );
    assert_eq!(broker.request_count(), 1);
    let record = orchestrator.last_result(42, 100).unwrap();
    assert_eq!(record.attempts, 1);
    assert_eq!(record.outcome, ProcessOutcome::Rejected);
    let health_error = orchestrator.generation_health_error().unwrap();
    assert_eq!(health_error.code, "injection-cleanup-unknown");
    assert_eq!(health_error.win32_error, Some(1223));
}

struct ProbingInspector {
    identity: ProcessIdentity,
    liveness: TargetLiveness,
    probes: Mutex<Vec<ProcessIdentity>>,
}

impl ProbingInspector {
    fn new(identity: ProcessIdentity, liveness: TargetLiveness) -> Self {
        Self {
            identity,
            liveness,
            probes: Mutex::new(Vec::new()),
        }
    }
}

impl ProcessInspector for ProbingInspector {
    fn inspect(&self, pid: u32) -> Result<InspectedProcess, StructuredServiceError> {
        assert_eq!(pid, self.identity.pid);
        Ok(inspected(self.identity.clone()))
    }

    fn probe_target_liveness(&self, identity: &ProcessIdentity) -> TargetLiveness {
        self.probes.lock().unwrap().push(identity.clone());
        self.liveness
    }
}

fn cleanup_unknown_broker() -> ScriptedBroker {
    ScriptedBroker::new([BrokerResult {
        disposition: BrokerDisposition::Rejected,
        code: "post-injection-state-cleanup-unknown".to_owned(),
        win32_error: Some(299),
    }])
}

#[test]
fn cleanup_unknown_for_a_vanished_target_is_a_trusted_skip_with_a_bounded_result() {
    let identity = ProcessIdentity {
        pid: 42,
        creation_time: 100,
        session_id: 2,
        architecture: ProcessArchitecture::X64,
        protected: false,
    };
    let inspector = ProbingInspector::new(identity.clone(), TargetLiveness::Vanished);
    let broker = cleanup_unknown_broker();
    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        discard_events(),
    );

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Skipped
    );
    assert_eq!(
        *inspector.probes.lock().unwrap(),
        std::slice::from_ref(&identity)
    );
    let record = orchestrator.last_result(42, 100).unwrap();
    assert_eq!(record.outcome, ProcessOutcome::Skipped);
    assert_eq!(record.code, TARGET_VANISHED_RESULT_CODE);
    assert_eq!(record.win32_error, Some(299));
    assert!(
        orchestrator.generation_health_error().is_none(),
        "a proven target vanish must not change global service health"
    );
    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Duplicate
    );
    assert_eq!(broker.request_count(), 1);
}

#[test]
fn cleanup_unknown_for_a_target_still_alive_keeps_the_degraded_classification() {
    let identity = ProcessIdentity {
        pid: 42,
        creation_time: 100,
        session_id: 2,
        architecture: ProcessArchitecture::X64,
        protected: false,
    };
    let inspector = ProbingInspector::new(identity, TargetLiveness::Alive);
    let broker = cleanup_unknown_broker();
    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        discard_events(),
    );

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Rejected
    );
    assert_eq!(inspector.probes.lock().unwrap().len(), 1);
    let record = orchestrator.last_result(42, 100).unwrap();
    assert_eq!(record.outcome, ProcessOutcome::Rejected);
    assert_eq!(record.code, "post-injection-state-cleanup-unknown");
    let health_error = orchestrator.generation_health_error().unwrap();
    assert_eq!(health_error.code, "injection-cleanup-unknown");
    assert_eq!(health_error.win32_error, Some(299));
}

#[test]
fn undeterminable_liveness_after_cleanup_unknown_keeps_the_degraded_classification() {
    let identity = ProcessIdentity {
        pid: 42,
        creation_time: 100,
        session_id: 2,
        architecture: ProcessArchitecture::X64,
        protected: false,
    };
    let inspector = ProbingInspector::new(identity, TargetLiveness::Unknown);
    let broker = cleanup_unknown_broker();
    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        discard_events(),
    );

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Rejected
    );
    let health_error = orchestrator.generation_health_error().unwrap();
    assert_eq!(health_error.code, "injection-cleanup-unknown");
}

#[test]
fn terminal_results_without_cleanup_unknown_never_probe_target_liveness() {
    let identity = ProcessIdentity {
        pid: 42,
        creation_time: 100,
        session_id: 2,
        architecture: ProcessArchitecture::X64,
        protected: false,
    };
    let inspector = ProbingInspector::new(identity, TargetLiveness::Vanished);
    let broker = ScriptedBroker::new([BrokerResult {
        disposition: BrokerDisposition::Rejected,
        code: "module-load-failed".to_owned(),
        win32_error: None,
    }]);
    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        discard_events(),
    );

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Rejected
    );
    assert!(inspector.probes.lock().unwrap().is_empty());
    let record = orchestrator.last_result(42, 100).unwrap();
    assert_eq!(record.code, "module-load-failed");
}

#[test]
fn conflicting_mactype_module_is_terminal_deduplicated_and_process_local() {
    let inspector = fixed_inspector(
        ProcessIdentity {
            pid: 42,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
        },
        2,
    );
    let broker = ScriptedBroker::new([BrokerResult {
        disposition: BrokerDisposition::Rejected,
        code: "conflicting-mactype-module-loaded".to_owned(),
        win32_error: None,
    }]);
    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        discard_events(),
    );

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Rejected
    );
    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Duplicate
    );
    assert_eq!(broker.request_count(), 1);
    let record = orchestrator.last_result(42, 100).unwrap();
    assert_eq!(record.attempts, 1);
    assert_eq!(record.code, "conflicting-mactype-module-loaded");
    assert!(orchestrator.generation_health_error().is_none());
}

struct SharedEventSource {
    query: Arc<Mutex<Option<String>>>,
}

impl ProcessEventSource for SharedEventSource {
    fn subscribe(&mut self, query: &str) -> Result<(), StructuredServiceError> {
        *self.query.lock().unwrap() = Some(query.to_owned());
        Ok(())
    }

    fn next_pid(&mut self, _timeout: Duration) -> Result<Option<u32>, StructuredServiceError> {
        Ok(None)
    }
}

#[derive(Default)]
struct ReadyBroker {
    checked: Arc<Mutex<Vec<ProcessArchitecture>>>,
}

impl InjectionBroker for ReadyBroker {
    fn verify_ready(
        &self,
        architecture: ProcessArchitecture,
    ) -> Result<(), StructuredServiceError> {
        self.checked.lock().unwrap().push(architecture);
        Ok(())
    }

    fn inject(&self, _request: &InjectionRequest) -> BrokerResult {
        unreachable!("this test verifies initialization only")
    }
}

#[test]
fn runtime_is_ready_only_after_exact_subscription_and_both_helpers_are_verified() {
    let query = Arc::new(Mutex::new(None));
    let broker = ReadyBroker::default();
    let checked = broker.checked.clone();
    let runtime = initialize_process_orchestration(
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned()),
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        Box::new(SharedEventSource {
            query: query.clone(),
        }),
        Box::new(fixed_inspector(
            ProcessIdentity {
                pid: 42,
                creation_time: 100,
                session_id: 2,
                architecture: ProcessArchitecture::X64,
                protected: false,
            },
            0,
        )),
        Box::new(broker),
        discard_events(),
    )
    .unwrap();

    assert_eq!(
        runtime.readiness,
        mactype_service_contract::ReadinessReport::ready()
    );
    assert_eq!(
        query.lock().unwrap().as_deref(),
        Some(PROCESS_CREATION_QUERY)
    );
    assert_eq!(
        *checked.lock().unwrap(),
        [ProcessArchitecture::X86, ProcessArchitecture::X64]
    );
}

#[test]
fn repeated_rejected_target_results_emit_one_throttled_injection_failed_event() {
    let directory = tempfile::tempdir().unwrap();
    let sink = RecordingEventSink::new(directory.path().join("host.log"), Instant::now());
    let identities = [
        ProcessIdentity {
            pid: 42,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
        },
        ProcessIdentity {
            pid: 43,
            creation_time: 101,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
        },
    ];
    let inspector = ScriptedInspector::new(identities.iter().cloned().map(|identity| {
        (
            identity.pid,
            InspectorResponse::inspected(
                inspected(identity),
                mactype_service_host::TargetLifecycle::Running,
            )
            .with_basename("game.exe"),
        )
    }));
    let broker = ScriptedBroker::new((0..2).map(|_| BrokerResult {
        disposition: BrokerDisposition::Rejected,
        code: "module-load-failed".to_owned(),
        win32_error: Some(5),
    }));
    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        sink.clone(),
    );

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Rejected
    );
    assert_eq!(
        orchestrator.handle_pid(43).unwrap(),
        ProcessOutcome::Rejected
    );

    let events = read_events(&[sink.path().to_owned()], 20);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.code == "injection-failed")
            .count(),
        1
    );
}

#[test]
fn successful_injection_emits_its_success_event_once() {
    let directory = tempfile::tempdir().unwrap();
    let sink = RecordingEventSink::new(directory.path().join("host.log"), Instant::now());
    let identity = ProcessIdentity {
        pid: 42,
        creation_time: 100,
        session_id: 2,
        architecture: ProcessArchitecture::X64,
        protected: false,
    };
    let inspector = fixed_inspector(identity, 1);
    let broker = ScriptedBroker::new([BrokerResult {
        disposition: BrokerDisposition::Injected,
        code: "module-loaded".to_owned(),
        win32_error: None,
    }]);
    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        sink.clone(),
    );

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Injected
    );

    assert_eq!(
        sink.events()
            .iter()
            .filter(|event| matches!(
                event,
                HostEvent::InjectionResult { record, .. }
                    if record.outcome == ProcessOutcome::Injected
            ))
            .count(),
        1
    );
}

#[test]
fn injection_summary_flushes_each_elapsed_window_and_resets_its_counts() {
    let directory = tempfile::tempdir().unwrap();
    let sink = RecordingEventSink::new(directory.path().join("host.log"), Instant::now());
    let identities = [
        ProcessIdentity {
            pid: 42,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
        },
        ProcessIdentity {
            pid: 43,
            creation_time: 101,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
        },
    ];
    let inspector = ScriptedInspector::new(identities.iter().cloned().map(|identity| {
        (
            identity.pid,
            InspectorResponse::inspected(
                inspected(identity),
                mactype_service_host::TargetLifecycle::Running,
            ),
        )
    }));
    let broker = ScriptedBroker::new((0..2).map(|_| BrokerResult {
        disposition: BrokerDisposition::Injected,
        code: "module-loaded".to_owned(),
        win32_error: None,
    }));
    let mut orchestrator = ProcessOrchestrator::new(
        900,
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &inspector,
        &broker,
        sink.clone(),
    );

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Injected
    );
    sink.advance(Duration::from_secs(60));
    assert_eq!(
        orchestrator.handle_pid(43).unwrap(),
        ProcessOutcome::Injected
    );
    sink.advance(Duration::from_secs(60));
    HostEventSink::record(sink.as_ref(), HostEvent::FlushInjectionSummary);

    let events = read_events(&[sink.path().to_owned()], 20);
    let summaries = events
        .iter()
        .filter(|event| event.code == "injection-summary")
        .collect::<Vec<_>>();
    assert_eq!(summaries.len(), 2);
    assert_eq!(
        summaries[0].params.get("injected").map(String::as_str),
        Some("1")
    );
    assert_eq!(
        summaries[1].params.get("injected").map(String::as_str),
        Some("1")
    );
    assert!(
        summaries
            .iter()
            .all(|summary| summary.params.get("failed").map(String::as_str) == Some("0"))
    );
}
