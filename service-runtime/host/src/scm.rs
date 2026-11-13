mod win32_adapter;

#[cfg(feature = "ci-test-adapter")]
mod crash_adapter;
#[cfg(all(test, feature = "ci-test-adapter"))]
mod tests;

use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use mactype_service_contract::{
    HEALTH_PROTOCOL_VERSION, HealthReport, HealthState, InjectionTelemetry, MachinePaths,
    ReadinessReport, StructuredServiceError, effective_health_pipe_name, effective_service_name,
};
use mactype_service_platform::run_service_dispatcher;

use crate::named_pipe::NamedPipeHealthPublisher;
use crate::service_version::service_runtime_version;
use crate::{
    CompositeHealthPublisher, FileHealthPublisher, HealthPublisher, HostError, HostEventLogger,
    HostEventSink, ServiceRuntime, ServiceStatus, StatusReporter, WindowsOpenServiceInitializer,
};
#[cfg(feature = "ci-test-adapter")]
use crash_adapter::spawn_crash_once_adapter;
pub(crate) use win32_adapter::stop_requested;
use win32_adapter::{ServiceControlContext, Win32StatusReporter, Win32StopSignal};

pub fn run_dispatcher() -> io::Result<()> {
    run_service_dispatcher(effective_service_name(), service_main)
}

fn service_main() {
    let _control_context = match ServiceControlContext::register(effective_service_name()) {
        Ok(context) => context,
        Err(_) => return,
    };

    let paths = match crate::known_folders::machine_paths() {
        Ok(paths) => paths,
        Err(error) => {
            let reporter = Win32StatusReporter;
            let _ = reporter.report(ServiceStatus::stopped_with_error(
                error.raw_os_error().unwrap_or(1) as u32,
                0,
            ));
            return;
        }
    };
    let events: Arc<dyn HostEventSink> = Arc::new(HostEventLogger::new(
        paths.service_host_event_log().to_owned(),
    ));
    let persisted = FileHealthPublisher::new(paths.service_root().join("health.json"));
    match catch_unwind(AssertUnwindSafe(|| {
        run_registered_service(paths, &persisted, events)
    })) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => report_terminal_failure(&persisted, &error, 3),
        Err(_) => report_terminal_failure(
            &persisted,
            &StructuredServiceError {
                code: "service-panic".to_owned(),
                message: "the service runtime terminated after an unexpected panic".to_owned(),
                win32_error: None,
            },
            2,
        ),
    }
}

fn run_registered_service(
    paths: MachinePaths,
    persisted: &FileHealthPublisher,
    events: Arc<dyn HostEventSink>,
) -> Result<(), StructuredServiceError> {
    let reporter = Win32StatusReporter;
    let stop = Win32StopSignal;
    #[cfg(feature = "ci-test-adapter")]
    spawn_crash_once_adapter(paths.clone());
    let initializer = WindowsOpenServiceInitializer::new(paths, events.clone());
    let health =
        NamedPipeHealthPublisher::start(effective_health_pipe_name()).map_err(|error| {
            StructuredServiceError {
                code: "health-pipe-start-failed".to_owned(),
                message: format!("the service could not create its fixed health pipe: {error}"),
                win32_error: error.raw_os_error().map(|value| value as u32),
            }
        })?;
    let composite = CompositeHealthPublisher::new(&health, persisted);
    ServiceRuntime::new(service_runtime_version(), events)
        .run(&reporter, &composite, &initializer, &stop)
        .map_err(structured_host_error)
}

fn structured_host_error(error: HostError) -> StructuredServiceError {
    match error {
        HostError::Runtime(error) => error,
        HostError::Io(error) => StructuredServiceError {
            code: "service-host-io-failed".to_owned(),
            message: error.to_string(),
            win32_error: error.raw_os_error().map(|value| value as u32),
        },
    }
}

fn report_terminal_failure(
    persisted: &FileHealthPublisher,
    error: &StructuredServiceError,
    service_specific_code: u32,
) {
    let _ = persisted.publish(&failed_health_report(
        &error.code,
        &error.message,
        error.win32_error,
    ));
    let reporter = Win32StatusReporter;
    let _ = reporter.report(ServiceStatus::stopped_with_error(
        1066,
        service_specific_code,
    ));
}

fn failed_health_report(code: &str, message: &str, win32_error: Option<u32>) -> HealthReport {
    HealthReport {
        protocol_version: HEALTH_PROTOCOL_VERSION,
        service_version: service_runtime_version().to_owned(),
        health: HealthState::Failed,
        active_profile_digest: None,
        readiness: ReadinessReport::initializing(),
        injection: InjectionTelemetry::default(),
        last_error: Some(StructuredServiceError {
            code: code.to_owned(),
            message: message.to_owned(),
            win32_error,
        }),
    }
}
