mod model;

use crate::target_validation::DeferralReason;
use crate::TargetLifecycle;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use mactype_service_contract::{
    InjectionArchitecture, InjectionSuccess, InjectionTelemetry, StructuredServiceError,
};

use crate::{
    BrokerDisposition, BrokerResult, InjectionBroker, InjectionRequest, ProcessIdentity,
    ProcessInspector, ProcessTargetDecision, ProcessTargetValidator, TargetLiveness,
};

pub use model::{
    DeferralPolicy, DeferredTarget, ProcessAttemptRecord, ProcessOutcome, RetryPolicy,
    RetryScheduler, SessionChange, MAX_DEFERRED_TARGETS, MAX_TRACKED_PROCESS_RESULTS,
};

/// The bounded target-result code recorded when an untrustworthy cleanup
/// result was re-checked and the verified target provably no longer existed.
pub const TARGET_VANISHED_RESULT_CODE: &str = "injection-target-vanished";

const DEFERRAL_CAPACITY_EXHAUSTED_CODE: &str = "deferral-capacity-exhausted";

pub struct InjectionOrchestrator<'a> {
    generation_id: String,
    profile_digest: Option<String>,
    target_validator: ProcessTargetValidator<'a>,
    inspector: &'a dyn ProcessInspector,
    broker: &'a dyn InjectionBroker,
    processed: HashMap<(u32, u64), ProcessAttemptRecord>,
    process_order: VecDeque<(u32, u64)>,
    deferred: HashMap<(u32, u64), DeferredTarget>,
    deferred_order: VecDeque<(u32, u64)>,
    deferral_policy: DeferralPolicy,
    retry_policy: RetryPolicy,
    retry_scheduler: Option<&'a dyn RetryScheduler>,
    last_injected_identity: Option<ProcessIdentity>,
    injection_telemetry: InjectionTelemetry,
}

impl<'a> InjectionOrchestrator<'a> {
    pub fn new(
        service_pid: u32,
        generation_id: impl Into<String>,
        inspector: &'a dyn ProcessInspector,
        broker: &'a dyn InjectionBroker,
    ) -> Self {
        Self::build(
            service_pid,
            generation_id,
            inspector,
            broker,
            RetryPolicy::default(),
            None,
        )
    }

    pub fn with_retry_policy(
        service_pid: u32,
        generation_id: impl Into<String>,
        inspector: &'a dyn ProcessInspector,
        broker: &'a dyn InjectionBroker,
        retry_policy: RetryPolicy,
        retry_scheduler: &'a dyn RetryScheduler,
    ) -> Self {
        Self::build(
            service_pid,
            generation_id,
            inspector,
            broker,
            retry_policy,
            Some(retry_scheduler),
        )
    }

    pub fn with_runtime_context(
        service_pid: u32,
        generation_id: impl Into<String>,
        profile_digest: impl Into<String>,
        inspector: &'a dyn ProcessInspector,
        broker: &'a dyn InjectionBroker,
        retry_policy: RetryPolicy,
        retry_scheduler: &'a dyn RetryScheduler,
    ) -> Self {
        let mut orchestrator = Self::with_retry_policy(
            service_pid,
            generation_id,
            inspector,
            broker,
            retry_policy,
            retry_scheduler,
        );
        orchestrator.profile_digest = Some(profile_digest.into());
        orchestrator
    }

    fn build(
        service_pid: u32,
        generation_id: impl Into<String>,
        inspector: &'a dyn ProcessInspector,
        broker: &'a dyn InjectionBroker,
        retry_policy: RetryPolicy,
        retry_scheduler: Option<&'a dyn RetryScheduler>,
    ) -> Self {
        Self {
            generation_id: generation_id.into(),
            profile_digest: None,
            target_validator: ProcessTargetValidator::new(service_pid, inspector),
            inspector,
            broker,
            processed: HashMap::new(),
            process_order: VecDeque::new(),
            deferred: HashMap::new(),
            deferred_order: VecDeque::new(),
            deferral_policy: DeferralPolicy::default(),
            retry_policy,
            retry_scheduler,
            last_injected_identity: None,
            injection_telemetry: InjectionTelemetry::default(),
        }
    }

    pub fn handle_pid(&mut self, pid: u32) -> Result<ProcessOutcome, StructuredServiceError> {
        self.handle_pid_at(pid, Instant::now())
    }

    pub fn handle_pid_at(
        &mut self,
        pid: u32,
        now: Instant,
    ) -> Result<ProcessOutcome, StructuredServiceError> {
        match self.target_validator.validate(pid)? {
            ProcessTargetDecision::Eligible(identity) => {
                if self.contains_identity(&identity) {
                    return Ok(ProcessOutcome::Duplicate);
                }
                self.attempt_injection(identity, 0, now)
            }
            ProcessTargetDecision::Deferred { identity, reason } => {
                if self.contains_identity(&identity) {
                    return Ok(ProcessOutcome::Duplicate);
                }
                let not_before = now + self.deferral_policy.frozen_recheck;
                self.insert_deferred(DeferredTarget {
                    identity,
                    reason,
                    deferrals: 0,
                    not_before,
                });
                Ok(ProcessOutcome::Deferred)
            }
            ProcessTargetDecision::Skipped => {
                crate::event_log::injection_skipped();
                Ok(ProcessOutcome::Skipped)
            }
        }
    }

    pub fn poll_deferred(
        &mut self,
        now: Instant,
    ) -> Result<Option<ProcessOutcome>, StructuredServiceError> {
        let Some(index) = self.deferred_order.iter().position(|key| {
            self.deferred
                .get(key)
                .is_some_and(|target| target.not_before <= now)
        }) else {
            return Ok(None);
        };
        let key = self
            .deferred_order
            .remove(index)
            .expect("the located deferred key exists");
        let Some(target) = self.deferred.remove(&key) else {
            return Ok(None);
        };

        if target.reason == DeferralReason::Frozen {
            match self.inspector.probe_target_lifecycle(&target.identity) {
                TargetLifecycle::Frozen | TargetLifecycle::Unknown => {
                    self.insert_deferred(DeferredTarget {
                        not_before: now + self.deferral_policy.frozen_recheck,
                        ..target
                    });
                    return Ok(Some(ProcessOutcome::Deferred));
                }
                TargetLifecycle::Exiting => {
                    self.record_skip(target.identity, "process-exiting", None);
                    return Ok(Some(ProcessOutcome::Skipped));
                }
                TargetLifecycle::Running => {}
            }
        }

        Ok(Some(self.revalidate_deferred(target, now)?))
    }

    fn revalidate_deferred(
        &mut self,
        target: DeferredTarget,
        now: Instant,
    ) -> Result<ProcessOutcome, StructuredServiceError> {
        match self.target_validator.validate(target.identity.pid)? {
            ProcessTargetDecision::Eligible(identity) if identity == target.identity => {
                self.attempt_injection(identity, target.deferrals, now)
            }
            ProcessTargetDecision::Eligible(_) => {
                self.record_skip(target.identity, TARGET_VANISHED_RESULT_CODE, None);
                Ok(ProcessOutcome::Skipped)
            }
            ProcessTargetDecision::Deferred { identity, reason } if identity == target.identity => {
                self.insert_deferred(DeferredTarget {
                    identity,
                    reason,
                    deferrals: target.deferrals,
                    not_before: now + self.deferral_policy.frozen_recheck,
                });
                Ok(ProcessOutcome::Deferred)
            }
            ProcessTargetDecision::Deferred { .. } => {
                self.record_skip(target.identity, TARGET_VANISHED_RESULT_CODE, None);
                Ok(ProcessOutcome::Skipped)
            }
            ProcessTargetDecision::Skipped => {
                self.record_skip(target.identity, "process-no-longer-eligible", None);
                Ok(ProcessOutcome::Skipped)
            }
        }
    }

    fn attempt_injection(
        &mut self,
        identity: ProcessIdentity,
        deferrals: u8,
        now: Instant,
    ) -> Result<ProcessOutcome, StructuredServiceError> {
        let request = InjectionRequest {
            identity,
            generation_id: self.generation_id.clone(),
        };
        let attempts = self.retry_policy.max_attempts.max(1);
        let mut delay = self.retry_policy.initial_delay;
        for attempt in 1..=attempts {
            let mut result = self.broker.inject(&request);
            if result.disposition == BrokerDisposition::RetryableFailure
                && !safe_to_retry_same_identity(&result.code)
            {
                result.disposition = BrokerDisposition::Rejected;
            }
            match result.disposition {
                BrokerDisposition::LaunchFailed => {
                    return Ok(self.handle_launch_failure(
                        request.identity.clone(),
                        deferrals,
                        now,
                        result,
                        attempt,
                    ));
                }
                BrokerDisposition::TargetFrozen => {
                    self.insert_deferred(DeferredTarget {
                        identity: request.identity.clone(),
                        reason: DeferralReason::Frozen,
                        deferrals,
                        not_before: now + self.deferral_policy.frozen_recheck,
                    });
                    return Ok(ProcessOutcome::Deferred);
                }
                _ => {}
            }
            if let Some(outcome) = terminal_outcome(result.disposition, attempt == attempts) {
                if outcome == ProcessOutcome::Injected {
                    self.last_injected_identity = Some(request.identity.clone());
                    self.record_injection_success(&request.identity);
                }
                let (outcome, result) =
                    self.reclassify_vanished_target(&request.identity, outcome, result);
                self.record_result(request.identity.clone(), outcome, attempt, result);
                return Ok(outcome);
            }

            if let Some(scheduler) = self.retry_scheduler {
                if !scheduler.wait(delay) {
                    self.record_result(
                        request.identity.clone(),
                        ProcessOutcome::Cancelled,
                        attempt,
                        result,
                    );
                    return Ok(ProcessOutcome::Cancelled);
                }
            } else {
                std::thread::sleep(delay);
            }
            delay = delay.saturating_mul(2).min(self.retry_policy.max_delay);
        }
        unreachable!("the bounded attempt loop always returns")
    }

    fn handle_launch_failure(
        &mut self,
        identity: ProcessIdentity,
        deferrals: u8,
        now: Instant,
        result: BrokerResult,
        attempt: u8,
    ) -> ProcessOutcome {
        if self.inspector.probe_target_liveness(&identity) == TargetLiveness::Vanished {
            self.record_skip(identity, TARGET_VANISHED_RESULT_CODE, result.win32_error);
            return ProcessOutcome::Skipped;
        }
        if deferrals >= self.deferral_policy.launch_max_deferrals {
            self.record_result(identity, ProcessOutcome::Rejected, attempt, result);
            return ProcessOutcome::Rejected;
        }
        let multiplier = 1_u32.checked_shl(u32::from(deferrals)).unwrap_or(u32::MAX);
        let delay = self
            .deferral_policy
            .launch_initial_delay
            .saturating_mul(multiplier)
            .min(self.deferral_policy.launch_max_delay);
        self.insert_deferred(DeferredTarget {
            identity,
            reason: DeferralReason::HelperLaunchFailed,
            deferrals: deferrals.saturating_add(1),
            not_before: now + delay,
        });
        ProcessOutcome::Deferred
    }

    fn contains_identity(&self, identity: &ProcessIdentity) -> bool {
        let key = (identity.pid, identity.creation_time);
        self.processed.contains_key(&key) || self.deferred.contains_key(&key)
    }

    fn insert_deferred(&mut self, target: DeferredTarget) {
        let key = (target.identity.pid, target.identity.creation_time);
        self.deferred_order.retain(|candidate| *candidate != key);
        if !self.deferred.contains_key(&key) {
            while self.deferred.len() >= MAX_DEFERRED_TARGETS {
                let Some(oldest) = self.deferred_order.pop_front() else {
                    break;
                };
                if let Some(evicted) = self.deferred.remove(&oldest) {
                    self.record_skip(evicted.identity, DEFERRAL_CAPACITY_EXHAUSTED_CODE, None);
                    break;
                }
            }
        }
        self.deferred.insert(key, target);
        self.deferred_order.push_back(key);
    }

    fn record_skip(&mut self, identity: ProcessIdentity, code: &str, win32_error: Option<u32>) {
        self.record_result(
            identity,
            ProcessOutcome::Skipped,
            0,
            BrokerResult {
                disposition: BrokerDisposition::Skipped,
                code: code.to_owned(),
                win32_error,
            },
        );
    }

    pub fn deferred_targets(&self) -> Vec<DeferredTarget> {
        self.deferred_order
            .iter()
            .filter_map(|key| self.deferred.get(key).cloned())
            .collect()
    }

    pub fn handle_session_change(&mut self, change: SessionChange) {
        if change.is_overflow() {
            self.processed.clear();
            self.process_order.clear();
            self.deferred.clear();
            self.deferred_order.clear();
            return;
        }
        if matches!(change.event_type, 2 | 4 | 6 | 11) {
            self.processed
                .retain(|_, record| record.identity.session_id != change.session_id);
            self.process_order
                .retain(|identity| self.processed.contains_key(identity));
            self.deferred
                .retain(|_, target| target.identity.session_id != change.session_id);
            self.deferred_order
                .retain(|identity| self.deferred.contains_key(identity));
        }
    }

    pub fn last_injected_identity(&self) -> Option<&ProcessIdentity> {
        self.last_injected_identity.as_ref()
    }

    pub fn last_result(&self, pid: u32, creation_time: u64) -> Option<&ProcessAttemptRecord> {
        self.processed.get(&(pid, creation_time))
    }

    pub fn tracked_process_count(&self) -> usize {
        self.processed.len()
    }

    pub fn most_recent_result(&self) -> Option<&ProcessAttemptRecord> {
        self.process_order
            .back()
            .and_then(|identity| self.processed.get(identity))
    }

    pub fn injection_telemetry(&self) -> InjectionTelemetry {
        self.injection_telemetry.clone()
    }

    pub fn generation_health_error(&self) -> Option<StructuredServiceError> {
        let record = self.most_recent_result()?;
        let code = if record.code.ends_with("-cleanup-unknown") {
            "injection-cleanup-unknown"
        } else if matches!(
            record.code.as_str(),
            "helper-response-invalid"
                | "helper-response-too-large"
                | "helper-exit-mismatch"
                | "runtime-generation-mismatch"
        ) {
            "injection-helper-response-integrity-unknown"
        } else {
            return None;
        };
        Some(StructuredServiceError {
            code: code.to_owned(),
            message: format!(
                "target result is not trustworthy: pid={} creation_time={} session_id={} generation={} broker_code={}",
                record.identity.pid,
                record.identity.creation_time,
                record.identity.session_id,
                record.runtime_generation_id,
                record.code
            ),
            win32_error: record.win32_error,
        })
    }

    /// A `*-cleanup-unknown` result only says the helper could not verify the
    /// post-injection state; when the target exits during that verification the
    /// evidence (for example win32 error 299, `ERROR_PARTIAL_COPY`) is a
    /// process vanish, not runtime damage. Re-checking the exact verified
    /// identity turns a proven vanish into a normal target skip that must not
    /// change global service health, while keeping a distinct bounded target
    /// result for telemetry. An alive or undeterminable target keeps the
    /// conservative untrustworthy classification.
    fn reclassify_vanished_target(
        &self,
        identity: &ProcessIdentity,
        outcome: ProcessOutcome,
        result: BrokerResult,
    ) -> (ProcessOutcome, BrokerResult) {
        if !matches!(
            outcome,
            ProcessOutcome::Rejected | ProcessOutcome::RetryExhausted
        ) || !result.code.ends_with("-cleanup-unknown")
        {
            return (outcome, result);
        }
        match self.inspector.probe_target_liveness(identity) {
            TargetLiveness::Vanished => (
                ProcessOutcome::Skipped,
                BrokerResult {
                    code: TARGET_VANISHED_RESULT_CODE.to_owned(),
                    ..result
                },
            ),
            TargetLiveness::Alive | TargetLiveness::Unknown => (outcome, result),
        }
    }

    fn record_injection_success(&mut self, identity: &ProcessIdentity) {
        let Some(profile_digest) = &self.profile_digest else {
            return;
        };
        self.injection_telemetry.record_success(
            match identity.architecture {
                crate::ProcessArchitecture::X86 => InjectionArchitecture::X86,
                crate::ProcessArchitecture::X64 => InjectionArchitecture::X64,
            },
            InjectionSuccess {
                pid: identity.pid,
                creation_time: identity.creation_time,
                session_id: identity.session_id,
                runtime_generation_id: self.generation_id.clone(),
                profile_digest: profile_digest.clone(),
            },
        );
    }

    fn record_result(
        &mut self,
        identity: ProcessIdentity,
        outcome: ProcessOutcome,
        attempts: u8,
        result: BrokerResult,
    ) {
        let key = (identity.pid, identity.creation_time);
        if !self.processed.contains_key(&key) {
            while self.processed.len() >= MAX_TRACKED_PROCESS_RESULTS {
                if let Some(oldest) = self.process_order.pop_front() {
                    self.processed.remove(&oldest);
                }
            }
            self.process_order.push_back(key);
        }
        let record = ProcessAttemptRecord {
            identity,
            runtime_generation_id: self.generation_id.clone(),
            outcome,
            attempts,
            code: result.code,
            win32_error: result.win32_error,
        };
        let process = if matches!(
            outcome,
            ProcessOutcome::Rejected | ProcessOutcome::RetryExhausted
        ) {
            self.inspector
                .process_basename(&record.identity)
                .unwrap_or_else(|| format!("pid-{}", record.identity.pid))
        } else {
            String::new()
        };
        crate::event_log::injection_result(&record, process);
        self.processed.insert(key, record);
    }
}

fn terminal_outcome(disposition: BrokerDisposition, final_attempt: bool) -> Option<ProcessOutcome> {
    match disposition {
        BrokerDisposition::LaunchFailed | BrokerDisposition::TargetFrozen => None,
        BrokerDisposition::Cancelled => Some(ProcessOutcome::Cancelled),
        BrokerDisposition::Injected => Some(ProcessOutcome::Injected),
        BrokerDisposition::Skipped => Some(ProcessOutcome::Skipped),
        BrokerDisposition::Rejected => Some(ProcessOutcome::Rejected),
        BrokerDisposition::RetryableFailure if final_attempt => {
            Some(ProcessOutcome::RetryExhausted)
        }
        BrokerDisposition::RetryableFailure => None,
    }
}

fn safe_to_retry_same_identity(code: &str) -> bool {
    matches!(
        code,
        "session-unavailable"
            | "identity-unavailable"
            | "architecture-unavailable"
            | "module-inventory-unavailable"
    )
}
