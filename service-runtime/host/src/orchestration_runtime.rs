use std::{collections::VecDeque, sync::Arc, time::Duration};

use mactype_service_contract::{
    ComponentReadiness, HealthState, InjectionTelemetry, ReadinessReport, StructuredServiceError,
};

use crate::{
    HostEvent, HostEventSink, InitializedRuntime, InjectionBroker, ProcessArchitecture,
    ProcessEventSource, ProcessInspector, RetryScheduler, RuntimeDriver, RuntimeHealthReporter,
    StopSignal, subscribe_process_creation,
};

const MAX_TOLERATED_CONSECUTIVE_HEALTH_REPORT_FAILURES: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObserverRecoveryPolicy {
    /// Delay before the first resubscription attempt.
    pub initial_delay: Duration,
    /// Cap for the doubling delay between attempts.
    pub max_delay: Duration,
    /// Attempts before the driver gives up and fails the service.
    pub max_attempts: u32,
}

impl Default for ObserverRecoveryPolicy {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            max_attempts: 12,
        }
    }
}

pub fn initialize_process_orchestration(
    active_profile_digest: Option<String>,
    service_pid: u32,
    generation_id: impl Into<String>,
    source: Box<dyn ProcessEventSource>,
    inspector: Box<dyn ProcessInspector>,
    broker: Box<dyn InjectionBroker>,
    events: Arc<dyn HostEventSink>,
) -> Result<InitializedRuntime, StructuredServiceError> {
    initialize_process_orchestration_with_observer_recovery(
        active_profile_digest,
        ObserverRecoveryPolicy::default(),
        service_pid,
        generation_id,
        source,
        inspector,
        broker,
        events,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn initialize_process_orchestration_with_observer_recovery(
    active_profile_digest: Option<String>,
    observer_recovery: ObserverRecoveryPolicy,
    service_pid: u32,
    generation_id: impl Into<String>,
    mut source: Box<dyn ProcessEventSource>,
    inspector: Box<dyn ProcessInspector>,
    broker: Box<dyn InjectionBroker>,
    events: Arc<dyn HostEventSink>,
) -> Result<InitializedRuntime, StructuredServiceError> {
    let profile_digest = active_profile_digest
        .clone()
        .ok_or_else(|| StructuredServiceError {
            code: "active-profile-unavailable".to_owned(),
            message: "process orchestration requires an active protected profile".to_owned(),
            win32_error: None,
        })?;
    broker.verify_ready(ProcessArchitecture::X86)?;
    broker.verify_ready(ProcessArchitecture::X64)?;
    subscribe_process_creation(source.as_mut())?;
    let snapshot_pids = relay_roots_first(source.snapshot_pids()?, inspector.as_ref());

    Ok(InitializedRuntime::driven(
        active_profile_digest,
        ReadinessReport::ready(),
        Box::new(ProcessOrchestrationDriver {
            service_pid,
            generation_id: generation_id.into(),
            profile_digest,
            observer: ObserverState {
                recovery: observer_recovery,
                snapshot_pids,
                source,
            },
            inspector,
            broker,
            events,
        }),
    ))
}

struct ProcessOrchestrationDriver {
    service_pid: u32,
    generation_id: String,
    profile_digest: String,
    observer: ObserverState,
    inspector: Box<dyn ProcessInspector>,
    broker: Box<dyn InjectionBroker>,
    events: Arc<dyn HostEventSink>,
}

struct ObserverState {
    recovery: ObserverRecoveryPolicy,
    snapshot_pids: VecDeque<u32>,
    source: Box<dyn ProcessEventSource>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObserverRecovery {
    Resubscribed,
    Stopped,
}

/// Orders a backlog so relay roots are drained first, keeping the original
/// relative order within each group. Draining is one helper launch per target,
/// so a shell that sits late in a snapshot of a busy machine stays uninjected
/// for the whole drain, and every program started meanwhile misses the relay.
fn relay_roots_first(pids: Vec<u32>, inspector: &dyn ProcessInspector) -> VecDeque<u32> {
    let (roots, rest): (Vec<u32>, Vec<u32>) = pids.into_iter().partition(|pid| {
        inspector
            .image_name_for_ordering(*pid)
            .is_some_and(|name| crate::is_relay_root(&name))
    });
    roots.into_iter().chain(rest).collect()
}

impl ObserverState {
    /// Takes the subscription again and folds every process that started while
    /// it was down into the backlog, so a recovery misses no injection target.
    /// A relay root found this way goes to the front: the shell may be the very
    /// process whose restart took the subscription down with it.
    fn resubscribe(
        &mut self,
        inspector: &dyn ProcessInspector,
    ) -> Result<(), StructuredServiceError> {
        subscribe_process_creation(self.source.as_mut())?;
        for pid in self.source.snapshot_pids()? {
            if self.snapshot_pids.contains(&pid) {
                continue;
            }
            if inspector
                .image_name_for_ordering(pid)
                .is_some_and(|name| crate::is_relay_root(&name))
            {
                self.snapshot_pids.push_front(pid);
            } else {
                self.snapshot_pids.push_back(pid);
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn recover_observer(
        &mut self,
        error: StructuredServiceError,
        stop: &dyn StopSignal,
        health: &dyn RuntimeHealthReporter,
        scheduler: &StopRetryScheduler<'_>,
        consecutive_health_report_failures: &mut usize,
        injection: InjectionTelemetry,
        inspector: &dyn ProcessInspector,
    ) -> Result<ObserverRecovery, StructuredServiceError> {
        let observer_failed = ReadinessReport {
            observer: ComponentReadiness::Failed,
            ..ReadinessReport::ready()
        };
        report_runtime_health(
            health,
            consecutive_health_report_failures,
            HealthState::Degraded,
            observer_failed.clone(),
            injection.clone(),
            Some(error.clone()),
        )?;

        let mut latest_error = error;
        let mut delay = self.recovery.initial_delay;
        for _attempt in 1..=self.recovery.max_attempts {
            if !scheduler.wait(delay) || stop.stop_requested() {
                return Ok(ObserverRecovery::Stopped);
            }
            match self.resubscribe(inspector) {
                Ok(()) => {
                    report_runtime_health(
                        health,
                        consecutive_health_report_failures,
                        HealthState::Ready,
                        ReadinessReport::ready(),
                        injection,
                        None,
                    )?;
                    return Ok(ObserverRecovery::Resubscribed);
                }
                Err(error) => latest_error = error,
            }
            delay = delay.saturating_mul(2).min(self.recovery.max_delay);
        }

        report_runtime_health(
            health,
            consecutive_health_report_failures,
            HealthState::Failed,
            observer_failed,
            injection,
            Some(latest_error.clone()),
        )?;
        Err(latest_error)
    }
}

impl RuntimeDriver for ProcessOrchestrationDriver {
    fn run(
        &mut self,
        stop: &dyn StopSignal,
        health: &dyn RuntimeHealthReporter,
    ) -> Result<(), StructuredServiceError> {
        let scheduler = StopRetryScheduler(stop);
        let mut orchestrator = crate::InjectionOrchestrator::with_runtime_context(
            self.service_pid,
            &self.generation_id,
            &self.profile_digest,
            self.inspector.as_ref(),
            self.broker.as_ref(),
            crate::RetryPolicy::default(),
            &scheduler,
            self.events.clone(),
        );
        let mut consecutive_health_report_failures = 0;
        loop {
            if stop.wait_timeout(Duration::ZERO)? {
                return Ok(());
            }
            while let Some(change) = stop.take_session_change() {
                orchestrator.handle_session_change(change);
            }
            self.events.record(HostEvent::FlushInjectionSummary);
            let deferred = orchestrator.poll_deferred(std::time::Instant::now());
            let outcome = match deferred {
                Ok(Some(outcome)) if outcome != crate::ProcessOutcome::Deferred => Ok(outcome),
                Err(error) => Err(error),
                _ => {
                    let event_wait = if self.observer.snapshot_pids.is_empty() {
                        Duration::from_millis(250)
                    } else {
                        Duration::ZERO
                    };
                    // A target the live source announced has an age worth
                    // measuring: the process was created moments ago and every
                    // millisecond until the injection is a millisecond it
                    // spends drawing with stock fonts. One drained from a
                    // snapshot may have been running for days, so it carries
                    // no age at all.
                    let (pid, announced) = match self.observer.source.next_pid(event_wait) {
                        Ok(Some(pid)) => (pid, true),
                        Ok(None) => match self.observer.snapshot_pids.pop_front() {
                            Some(pid) => (pid, false),
                            None => continue,
                        },
                        Err(error) => match self.observer.recover_observer(
                            error,
                            stop,
                            health,
                            &scheduler,
                            &mut consecutive_health_report_failures,
                            orchestrator.injection_telemetry(),
                            self.inspector.as_ref(),
                        ) {
                            Ok(ObserverRecovery::Resubscribed) => continue,
                            Ok(ObserverRecovery::Stopped) => return Ok(()),
                            Err(error) => return Err(error),
                        },
                    };
                    let started = std::time::Instant::now();
                    let handling_began = announced.then(current_filetime).flatten();
                    let outcome = orchestrator.handle_pid(pid);
                    let age_millis = handling_began
                        .zip(orchestrator.last_handled_creation_time())
                        .and_then(|(now, created)| filetime_age_millis(now, created));
                    self.events.record(HostEvent::InjectionPipelineSample {
                        millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                        backlog: self.observer.snapshot_pids.len(),
                        age_millis,
                    });
                    outcome
                }
            };
            match outcome {
                Ok(crate::ProcessOutcome::Injected) => {
                    report_runtime_health(
                        health,
                        &mut consecutive_health_report_failures,
                        HealthState::Ready,
                        ReadinessReport::ready(),
                        orchestrator.injection_telemetry(),
                        None,
                    )?;
                }
                Ok(crate::ProcessOutcome::Rejected | crate::ProcessOutcome::RetryExhausted) => {
                    if let Some(error) = orchestrator.generation_health_error() {
                        report_runtime_health(
                            health,
                            &mut consecutive_health_report_failures,
                            HealthState::Degraded,
                            ReadinessReport::ready(),
                            orchestrator.injection_telemetry(),
                            Some(error),
                        )?;
                    }
                }
                Ok(crate::ProcessOutcome::Cancelled) => return Ok(()),
                Ok(_) => {}
                Err(error) => {
                    report_runtime_health(
                        health,
                        &mut consecutive_health_report_failures,
                        HealthState::Degraded,
                        ReadinessReport::ready(),
                        orchestrator.injection_telemetry(),
                        Some(error),
                    )?;
                }
            }
        }
    }
}

/// Ticks between 1601-01-01 and 1970-01-01, the offset between the epoch a
/// `FILETIME` counts from and the one `SystemTime` does.
const UNIX_EPOCH_FILETIME_TICKS: u128 = 116_444_736_000_000_000;

/// The system clock as a `FILETIME`: 100-nanosecond ticks since 1601-01-01
/// UTC, the same scale a process creation time is reported on. `None` if the
/// clock is set before 1970, which a running Windows machine does not produce.
fn current_filetime() -> Option<u64> {
    let since_unix_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    filetime_from_unix_epoch(since_unix_epoch)
}

/// Moves a duration measured from the Unix epoch onto the `FILETIME` scale.
fn filetime_from_unix_epoch(since_unix_epoch: Duration) -> Option<u64> {
    let ticks = since_unix_epoch.as_nanos() / 100 + UNIX_EPOCH_FILETIME_TICKS;
    u64::try_from(ticks).ok()
}

/// How long ago `creation_time` was, in milliseconds. A creation time in the
/// future is the mark of a clock the machine has just adjusted, and no age can
/// be read out of it.
fn filetime_age_millis(now: u64, creation_time: u64) -> Option<u64> {
    now.checked_sub(creation_time).map(|ticks| ticks / 10_000)
}

fn report_runtime_health(
    health: &dyn RuntimeHealthReporter,
    consecutive_failures: &mut usize,
    state: HealthState,
    readiness: ReadinessReport,
    injection: InjectionTelemetry,
    last_error: Option<StructuredServiceError>,
) -> Result<(), StructuredServiceError> {
    match health.report(state, readiness, injection, last_error) {
        Ok(()) => {
            *consecutive_failures = 0;
            Ok(())
        }
        Err(error) => {
            *consecutive_failures += 1;
            if *consecutive_failures > MAX_TOLERATED_CONSECUTIVE_HEALTH_REPORT_FAILURES {
                Err(error)
            } else {
                Ok(())
            }
        }
    }
}

struct StopRetryScheduler<'a>(&'a dyn StopSignal);

impl crate::RetryScheduler for StopRetryScheduler<'_> {
    fn wait(&self, delay: Duration) -> bool {
        matches!(self.0.wait_timeout(delay), Ok(false))
    }
}

#[cfg(test)]
mod tests {
    use super::{filetime_age_millis, filetime_from_unix_epoch, relay_roots_first};
    use crate::{InspectedProcess, ProcessInspector};
    use mactype_service_contract::StructuredServiceError;
    use std::time::Duration;

    /// Names PIDs by a fixed convention so ordering can be asserted without a
    /// live process: 100 and 200 are shells, everything else is not.
    struct NamedPids;

    impl ProcessInspector for NamedPids {
        fn inspect(&self, _pid: u32) -> Result<InspectedProcess, StructuredServiceError> {
            unreachable!("ordering never inspects an identity")
        }

        fn image_name_for_ordering(&self, pid: u32) -> Option<String> {
            match pid {
                100 | 200 => Some("explorer.exe".to_owned()),
                900 => None,
                other => Some(format!("program-{other}.exe")),
            }
        }
    }

    #[test]
    fn a_shell_buried_in_the_backlog_is_drained_before_everything_else() {
        let ordered = relay_roots_first(vec![5, 6, 100, 7, 200, 900], &NamedPids);

        assert_eq!(
            ordered.into_iter().collect::<Vec<u32>>(),
            vec![100, 200, 5, 6, 7, 900],
            "shells first, and both groups keep the order the snapshot gave them"
        );
    }

    #[test]
    fn a_backlog_without_a_shell_is_left_exactly_as_it_arrived() {
        let ordered = relay_roots_first(vec![9, 8, 7], &NamedPids);

        assert_eq!(ordered.into_iter().collect::<Vec<u32>>(), vec![9, 8, 7]);
    }

    #[test]
    fn an_age_counts_whole_milliseconds_of_filetime_ticks() {
        let created = 133_000_000_000_000_000_u64;

        assert_eq!(filetime_age_millis(created, created), Some(0));
        assert_eq!(filetime_age_millis(created + 9_999, created), Some(0));
        assert_eq!(filetime_age_millis(created + 10_000, created), Some(1));
        assert_eq!(
            filetime_age_millis(created + 12_345_678, created),
            Some(1_234)
        );
        assert_eq!(
            filetime_age_millis(created + 20_000_000, created),
            Some(2_000)
        );
    }

    #[test]
    fn a_creation_time_in_the_future_yields_no_age_rather_than_a_wrapped_one() {
        let created = 133_000_000_000_000_000_u64;

        assert_eq!(filetime_age_millis(created - 1, created), None);
        assert_eq!(filetime_age_millis(0, created), None);
    }

    #[test]
    fn the_clock_conversion_lands_on_the_epoch_a_creation_time_is_counted_from() {
        // The Unix epoch itself, and 2020-01-01T00:00:00Z, as FILETIMEs.
        assert_eq!(
            filetime_from_unix_epoch(Duration::ZERO),
            Some(116_444_736_000_000_000)
        );
        assert_eq!(
            filetime_from_unix_epoch(Duration::from_secs(1_577_836_800)),
            Some(132_223_104_000_000_000)
        );
        assert_eq!(
            filetime_from_unix_epoch(Duration::from_nanos(150)),
            Some(116_444_736_000_000_001)
        );
    }
}
