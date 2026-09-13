use std::{collections::VecDeque, time::Duration};

use mactype_service_contract::{
    ComponentReadiness, HealthState, InjectionTelemetry, ReadinessReport, StructuredServiceError,
};

use crate::{
    subscribe_process_creation, InitializedRuntime, InjectionBroker, ProcessArchitecture,
    ProcessEventSource, ProcessInspector, RuntimeDriver, RuntimeHealthReporter, StopSignal,
};

const MAX_TOLERATED_CONSECUTIVE_HEALTH_REPORT_FAILURES: usize = 20;

pub fn initialize_process_orchestration(
    active_profile_digest: Option<String>,
    service_pid: u32,
    generation_id: impl Into<String>,
    mut source: Box<dyn ProcessEventSource>,
    inspector: Box<dyn ProcessInspector>,
    broker: Box<dyn InjectionBroker>,
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
    let snapshot_pids = source.snapshot_pids()?.into();

    Ok(InitializedRuntime::driven(
        active_profile_digest,
        ReadinessReport::ready(),
        Box::new(ProcessOrchestrationDriver {
            service_pid,
            generation_id: generation_id.into(),
            profile_digest,
            snapshot_pids,
            source,
            inspector,
            broker,
        }),
    ))
}

struct ProcessOrchestrationDriver {
    service_pid: u32,
    generation_id: String,
    profile_digest: String,
    snapshot_pids: VecDeque<u32>,
    source: Box<dyn ProcessEventSource>,
    inspector: Box<dyn ProcessInspector>,
    broker: Box<dyn InjectionBroker>,
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
        );
        let mut consecutive_health_report_failures = 0;
        loop {
            if stop.wait_timeout(Duration::ZERO)? {
                return Ok(());
            }
            while let Some(change) = stop.take_session_change() {
                orchestrator.handle_session_change(change);
            }
            crate::event_log::flush_elapsed_injection_summary();
            let deferred = orchestrator.poll_deferred(std::time::Instant::now());
            let outcome = match deferred {
                Ok(Some(outcome)) if outcome != crate::ProcessOutcome::Deferred => Ok(outcome),
                Err(error) => Err(error),
                _ => {
                    let event_wait = if self.snapshot_pids.is_empty() {
                        Duration::from_millis(250)
                    } else {
                        Duration::ZERO
                    };
                    let pid = match self.source.next_pid(event_wait) {
                        Ok(Some(pid)) => pid,
                        Ok(None) => match self.snapshot_pids.pop_front() {
                            Some(pid) => pid,
                            None => continue,
                        },
                        Err(error) => {
                            let _ = report_runtime_health(
                                health,
                                &mut consecutive_health_report_failures,
                                HealthState::Failed,
                                ReadinessReport {
                                    observer: ComponentReadiness::Failed,
                                    ..ReadinessReport::ready()
                                },
                                orchestrator.injection_telemetry(),
                                Some(error.clone()),
                            );
                            return Err(error);
                        }
                    };
                    orchestrator.handle_pid(pid)
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
