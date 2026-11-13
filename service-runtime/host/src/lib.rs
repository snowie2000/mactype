#![forbid(unsafe_code)]

mod active_generation;
mod control;
#[cfg(windows)]
mod etw_observer;
mod event_log;
mod file_health;
mod helper_broker;
mod injection_orchestrator;
mod observer;
mod orchestration_runtime;
mod profile_runtime;
mod protected_path;
mod runtime;
mod runtime_assets;
mod service_version;
mod session_event_queue;
mod startup_safety;
mod status;
mod target_validation;
#[cfg(windows)]
mod windows_helper_launcher;

#[cfg(windows)]
mod known_folders;
#[cfg(windows)]
mod named_pipe;
#[cfg(windows)]
mod scm;
#[cfg(windows)]
mod windows_process;
#[cfg(windows)]
mod windows_runtime;
#[cfg(windows)]
mod windows_startup_safety;
#[cfg(windows)]
mod windows_wmi;

#[cfg(windows)]
pub use etw_observer::{
    ETW_OBSERVER_OVERFLOW_CODE, ETW_OBSERVER_STOPPED_CODE, EtwProcessEventSource,
    PROCESS_START_SESSION,
};
pub use event_log::{HostEvent, HostEventLogger, HostEventSink, LiveObserver};
pub use file_health::{CompositeHealthPublisher, FileHealthPublisher};
pub use helper_broker::{
    FixedHelperBroker, FixedModuleProbe, FixedModuleState, HelperInvocation, HelperLaunchError,
    HelperLaunchStage, HelperLauncher, HelperOutput, PlatformFixedModuleProbe,
};
pub use injection_orchestrator::{
    DeferralPolicy, DeferredTarget, InjectionOrchestrator, MAX_DEFERRED_TARGETS,
    MAX_TRACKED_PROCESS_RESULTS, ProcessAttemptRecord, ProcessOutcome, RetryPolicy, RetryScheduler,
    SessionChange, TARGET_VANISHED_RESULT_CODE,
};
#[cfg(windows)]
pub use named_pipe::{HEALTH_PIPE_SECURITY_SDDL, NamedPipeHealthPublisher};
pub type ProcessOrchestrator<'a> = InjectionOrchestrator<'a>;
pub use observer::{
    BrokerDisposition, BrokerResult, FALLBACK_PROCESS_CREATION_QUERY, InjectionBroker,
    InjectionRequest, InspectedProcess, PROCESS_CREATION_QUERY, ProcessArchitecture,
    ProcessEventSource, ProcessFacts, ProcessIdentity, ProcessInspector, TargetLifecycle,
    TargetLiveness, subscribe_process_creation,
};
pub use orchestration_runtime::{
    ObserverRecoveryPolicy, initialize_process_orchestration,
    initialize_process_orchestration_with_observer_recovery,
};
pub use profile_runtime::{ACTIVE_PROFILE_ABSENT_CODE, ProtectedProfileInitializer};
pub use runtime::{
    HealthPublisher, HostError, InitializedRuntime, RuntimeDriver, RuntimeHealthReporter,
    RuntimeInitializer, ServiceRuntime, StopSignal,
};
pub use runtime_assets::{ProtectedRuntimeAssets, RUNTIME_PROFILE_ABSENT_CODE};
pub use startup_safety::{LegacyServiceRuntimeState, StartupSafetySnapshot};
pub use status::{SERVICE_STOP_WAIT_HINT_MS, ScmState, ServiceStatus, StatusReporter};
pub use target_validation::{
    DeferralReason, ProcessTargetDecision, ProcessTargetValidator, SkipReason, is_relay_root,
};
#[cfg(windows)]
pub use windows_helper_launcher::WindowsHelperLauncher;
#[cfg(windows)]
pub use windows_process::WindowsProcessInspector;
#[cfg(windows)]
pub use windows_runtime::WindowsOpenServiceInitializer;
#[cfg(windows)]
pub use windows_startup_safety::WindowsStartupSafety;
#[cfg(windows)]
pub use windows_wmi::WmiProcessEventSource;

pub fn validate_host_arguments<I, S>(arguments: I) -> Result<(), &'static str>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut arguments = arguments.into_iter();
    match arguments.next() {
        None => Ok(()),
        Some(argument) if argument.as_ref() == "--service" && arguments.next().is_none() => Ok(()),
        Some(_) => Err("the service host accepts only the fixed --service switch"),
    }
}

#[cfg(windows)]
pub fn run_service_process() -> std::io::Result<()> {
    scm::run_dispatcher()
}

#[cfg(not(windows))]
pub fn run_service_process() -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "the service host requires Windows SCM",
    ))
}
pub use control::{ACCEPTED_CONTROL_MASK, ServiceControl};
