use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use mactype_service_contract::StructuredServiceError;
use mactype_service_host::{
    BrokerDisposition, BrokerResult, DeferralReason, InjectionBroker, InjectionRequest,
    ProcessArchitecture, ProcessIdentity, ProcessInspector, ProcessOrchestrator, ProcessOutcome,
    SessionChange, TargetLifecycle, TargetLiveness, MAX_DEFERRED_TARGETS,
    TARGET_VANISHED_RESULT_CODE,
};

fn binding() -> String {
    "a".repeat(64)
}

fn identity(pid: u32) -> ProcessIdentity {
    ProcessIdentity {
        pid,
        creation_time: u64::from(pid) + 100,
        session_id: 2,
        architecture: ProcessArchitecture::X64,
        protected: false,
        critical: false,
    }
}

struct MutableInspector {
    lifecycle: Mutex<TargetLifecycle>,
    liveness: Mutex<TargetLiveness>,
}

impl MutableInspector {
    fn new(lifecycle: TargetLifecycle, liveness: TargetLiveness) -> Self {
        Self {
            lifecycle: Mutex::new(lifecycle),
            liveness: Mutex::new(liveness),
        }
    }

    fn set_lifecycle(&self, lifecycle: TargetLifecycle) {
        *self.lifecycle.lock().unwrap() = lifecycle;
    }
}

impl ProcessInspector for MutableInspector {
    fn inspect(&self, pid: u32) -> Result<ProcessIdentity, StructuredServiceError> {
        Ok(identity(pid))
    }

    fn probe_target_lifecycle(&self, _identity: &ProcessIdentity) -> TargetLifecycle {
        *self.lifecycle.lock().unwrap()
    }

    fn probe_target_liveness(&self, _identity: &ProcessIdentity) -> TargetLiveness {
        *self.liveness.lock().unwrap()
    }
}

struct SequenceBroker {
    results: Mutex<VecDeque<BrokerResult>>,
    requests: Mutex<Vec<InjectionRequest>>,
}

impl SequenceBroker {
    fn new(results: impl IntoIterator<Item = BrokerResult>) -> Self {
        Self {
            results: Mutex::new(results.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

impl InjectionBroker for SequenceBroker {
    fn inject(&self, request: &InjectionRequest) -> BrokerResult {
        self.requests.lock().unwrap().push(request.clone());
        self.results.lock().unwrap().pop_front().unwrap()
    }
}

fn broker_result(disposition: BrokerDisposition, code: &str) -> BrokerResult {
    BrokerResult {
        disposition,
        code: code.to_owned(),
        win32_error: None,
    }
}

#[test]
fn frozen_target_is_deferred_deduplicated_and_injected_after_it_runs() {
    let inspector = MutableInspector::new(TargetLifecycle::Frozen, TargetLiveness::Alive);
    let broker = SequenceBroker::new([broker_result(BrokerDisposition::Injected, "module-loaded")]);
    let mut orchestrator = ProcessOrchestrator::new(900, binding(), &inspector, &broker);
    let start = Instant::now();

    assert_eq!(
        orchestrator.handle_pid_at(42, start).unwrap(),
        ProcessOutcome::Deferred
    );
    assert_eq!(orchestrator.deferred_targets().len(), 1);
    assert_eq!(
        orchestrator.deferred_targets()[0].reason,
        DeferralReason::Frozen
    );
    assert_eq!(broker.request_count(), 0);
    assert_eq!(
        orchestrator.handle_pid_at(42, start).unwrap(),
        ProcessOutcome::Duplicate
    );
    assert_eq!(
        orchestrator
            .poll_deferred(start + Duration::from_secs(1))
            .unwrap(),
        None
    );

    inspector.set_lifecycle(TargetLifecycle::Running);
    assert_eq!(
        orchestrator
            .poll_deferred(start + Duration::from_secs(2))
            .unwrap(),
        Some(ProcessOutcome::Injected)
    );
    assert_eq!(broker.request_count(), 1);
    assert!(orchestrator.deferred_targets().is_empty());
}

#[test]
fn frozen_target_that_exits_becomes_a_quiet_skip() {
    let inspector = MutableInspector::new(TargetLifecycle::Frozen, TargetLiveness::Alive);
    let broker = SequenceBroker::new([]);
    let mut orchestrator = ProcessOrchestrator::new(900, binding(), &inspector, &broker);
    let start = Instant::now();

    assert_eq!(
        orchestrator.handle_pid_at(42, start).unwrap(),
        ProcessOutcome::Deferred
    );
    inspector.set_lifecycle(TargetLifecycle::Exiting);
    assert_eq!(
        orchestrator
            .poll_deferred(start + Duration::from_secs(2))
            .unwrap(),
        Some(ProcessOutcome::Skipped)
    );
    assert!(orchestrator.deferred_targets().is_empty());
    assert_eq!(
        orchestrator.last_result(42, 142).unwrap().code,
        "process-exiting"
    );
    assert_eq!(broker.request_count(), 0);
}

#[test]
fn pre_resume_launch_failure_for_a_vanished_target_is_a_quiet_skip() {
    let inspector = MutableInspector::new(TargetLifecycle::Running, TargetLiveness::Vanished);
    let broker = SequenceBroker::new([broker_result(
        BrokerDisposition::LaunchFailed,
        "helper-launch-failed-before-resume",
    )]);
    let mut orchestrator = ProcessOrchestrator::new(900, binding(), &inspector, &broker);

    assert_eq!(
        orchestrator.handle_pid(42).unwrap(),
        ProcessOutcome::Skipped
    );
    assert_eq!(
        orchestrator.last_result(42, 142).unwrap().code,
        TARGET_VANISHED_RESULT_CODE
    );
    assert!(orchestrator.deferred_targets().is_empty());
}

#[test]
fn helper_launch_deferrals_double_to_the_cap_then_reject() {
    let inspector = MutableInspector::new(TargetLifecycle::Running, TargetLiveness::Alive);
    let launch_failure = || {
        broker_result(
            BrokerDisposition::LaunchFailed,
            "helper-launch-failed-before-resume",
        )
    };
    let broker = SequenceBroker::new((0..6).map(|_| launch_failure()));
    let mut orchestrator = ProcessOrchestrator::new(900, binding(), &inspector, &broker);
    let start = Instant::now();

    assert_eq!(
        orchestrator.handle_pid_at(42, start).unwrap(),
        ProcessOutcome::Deferred
    );
    let expected_due = [2, 6, 14, 30, 62];
    for (index, seconds) in expected_due.into_iter().enumerate() {
        let target = orchestrator.deferred_targets().into_iter().next().unwrap();
        assert_eq!(target.deferrals, index as u8 + 1);
        assert_eq!(target.not_before, start + Duration::from_secs(seconds));
        assert_eq!(
            orchestrator.poll_deferred(target.not_before).unwrap(),
            Some(if index == 4 {
                ProcessOutcome::Rejected
            } else {
                ProcessOutcome::Deferred
            })
        );
    }
    assert_eq!(broker.request_count(), 6);
    assert!(orchestrator.deferred_targets().is_empty());
    let result = orchestrator.last_result(42, 142).unwrap();
    assert_eq!(result.outcome, ProcessOutcome::Rejected);
    assert_eq!(result.outcome, ProcessOutcome::Rejected);
    assert_eq!(result.code, "helper-launch-failed-before-resume");
    assert!(orchestrator.generation_health_error().is_none());
}

#[test]
fn broker_frozen_race_enters_the_frozen_deferral_queue() {
    let inspector = MutableInspector::new(TargetLifecycle::Running, TargetLiveness::Alive);
    let broker = SequenceBroker::new([broker_result(
        BrokerDisposition::TargetFrozen,
        "process-frozen",
    )]);
    let mut orchestrator = ProcessOrchestrator::new(900, binding(), &inspector, &broker);
    let start = Instant::now();

    assert_eq!(
        orchestrator.handle_pid_at(42, start).unwrap(),
        ProcessOutcome::Deferred
    );
    let deferred = orchestrator.deferred_targets().into_iter().next().unwrap();
    assert_eq!(deferred.reason, DeferralReason::Frozen);
    assert_eq!(deferred.deferrals, 0);
    assert_eq!(deferred.not_before, start + Duration::from_secs(2));
}

#[test]
fn deferred_capacity_evicts_and_records_the_oldest_target() {
    let inspector = MutableInspector::new(TargetLifecycle::Frozen, TargetLiveness::Alive);
    let broker = SequenceBroker::new([]);
    let mut orchestrator = ProcessOrchestrator::new(u32::MAX, binding(), &inspector, &broker);
    let start = Instant::now();

    for pid in 1..=(MAX_DEFERRED_TARGETS as u32 + 1) {
        assert_eq!(
            orchestrator.handle_pid_at(pid, start).unwrap(),
            ProcessOutcome::Deferred
        );
    }
    assert_eq!(orchestrator.deferred_targets().len(), MAX_DEFERRED_TARGETS);
    assert_eq!(
        orchestrator.last_result(1, 101).unwrap().code,
        "deferral-capacity-exhausted"
    );
    assert_eq!(orchestrator.deferred_targets()[0].identity.pid, 2);
}

#[test]
fn session_changes_retain_other_sessions_and_overflow_clears_all_deferrals() {
    let inspector = MutableInspector::new(TargetLifecycle::Frozen, TargetLiveness::Alive);
    let broker = SequenceBroker::new([]);
    let mut orchestrator = ProcessOrchestrator::new(900, binding(), &inspector, &broker);
    let start = Instant::now();

    orchestrator.handle_pid_at(42, start).unwrap();
    orchestrator.handle_session_change(SessionChange {
        event_type: 6,
        session_id: 3,
    });
    assert_eq!(orchestrator.deferred_targets().len(), 1);
    orchestrator.handle_session_change(SessionChange {
        event_type: 6,
        session_id: 2,
    });
    assert!(orchestrator.deferred_targets().is_empty());

    orchestrator.handle_pid_at(43, start).unwrap();
    orchestrator.handle_session_change(SessionChange::overflow());
    assert!(orchestrator.deferred_targets().is_empty());
}
