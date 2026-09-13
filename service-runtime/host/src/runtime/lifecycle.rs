use mactype_service_contract::{
    HealthReport, HealthState, InjectionTelemetry, ReadinessReport, StructuredServiceError,
    HEALTH_PROTOCOL_VERSION,
};

use super::{
    HealthPublisher, HostError, RuntimeHealthReporter, RuntimeInitializer, ServiceRuntime,
    StopSignal,
};
use crate::{
    ServiceStatus, StatusReporter, ACTIVE_PROFILE_ABSENT_CODE, RUNTIME_PROFILE_ABSENT_CODE,
};

const ERROR_SERVICE_SPECIFIC_ERROR: u32 = 1066;

fn is_supported_stopped_state(error: &StructuredServiceError) -> bool {
    matches!(
        error.code.as_str(),
        ACTIVE_PROFILE_ABSENT_CODE | RUNTIME_PROFILE_ABSENT_CODE
    )
}

impl ServiceRuntime<'_> {
    pub fn run(
        &self,
        status: &dyn StatusReporter,
        health: &dyn HealthPublisher,
        initializer: &dyn RuntimeInitializer,
        stop: &dyn StopSignal,
    ) -> Result<(), HostError> {
        if let Err(error) = status.report(ServiceStatus::start_pending(1, 10_000)) {
            return Err(self.report_io_failure(
                status,
                health,
                "start-pending-report-failed",
                error,
            ));
        }
        if let Err(error) = health.publish(&HealthReport {
            protocol_version: HEALTH_PROTOCOL_VERSION,
            service_version: self.service_version.to_owned(),
            health: HealthState::Initializing,
            active_profile_digest: None,
            readiness: ReadinessReport::initializing(),
            injection: InjectionTelemetry::default(),
            last_error: None,
        }) {
            return Err(self.report_io_failure(
                status,
                health,
                "initializing-health-publish-failed",
                error,
            ));
        }

        let mut initialized = match initializer.initialize() {
            Ok(initialized) => initialized,
            Err(error) if is_supported_stopped_state(&error) => {
                return self.report_supported_stop(status, health, &error);
            }
            Err(error) => {
                self.report_failure(status, health, &error);
                return Err(HostError::Runtime(error));
            }
        };

        if let Err(error) = status.report(ServiceStatus::running()) {
            return Err(self.report_io_failure(
                status,
                health,
                "running-status-report-failed",
                error,
            ));
        }
        let ready = HealthReport {
            protocol_version: HEALTH_PROTOCOL_VERSION,
            service_version: self.service_version.to_owned(),
            health: HealthState::Ready,
            active_profile_digest: initialized.active_profile_digest.clone(),
            readiness: initialized.readiness.clone(),
            injection: InjectionTelemetry::default(),
            last_error: None,
        };
        if ready.validate().is_err() {
            let error = StructuredServiceError {
                code: "readiness-incomplete".to_owned(),
                message: "required runtime components are not ready".to_owned(),
                win32_error: None,
            };
            self.report_failure(status, health, &error);
            return Err(HostError::Runtime(error));
        }
        if let Err(publish_error) = health.publish(&ready) {
            let error = StructuredServiceError {
                code: "health-publish-failed".to_owned(),
                message: publish_error.to_string(),
                win32_error: publish_error.raw_os_error().map(|code| code as u32),
            };
            self.report_failure(status, health, &error);
            return Err(HostError::Runtime(error));
        }
        crate::event_log::service_started(self.service_version);

        let runtime_health = RuntimeHealthAdapter {
            publisher: health,
            service_version: self.service_version,
            active_profile_digest: initialized.active_profile_digest.clone(),
        };
        let wait_result = match initialized.driver.as_mut() {
            Some(driver) => driver.run(stop, &runtime_health),
            None => stop.wait(),
        };
        let terminal_error = match wait_result {
            Ok(()) => None,
            Err(error) if stop.stop_requested() => Some(error),
            Err(error) => {
                self.report_failure(status, health, &error);
                return Err(HostError::Runtime(error));
            }
        };

        let _ = health.publish(&HealthReport {
            protocol_version: HEALTH_PROTOCOL_VERSION,
            service_version: self.service_version.to_owned(),
            health: HealthState::Unknown,
            active_profile_digest: None,
            readiness: ReadinessReport::not_required(),
            injection: InjectionTelemetry::default(),
            last_error: terminal_error,
        });
        crate::event_log::service_stopped();
        if let Err(error) = status.report(ServiceStatus::stopped()) {
            return Err(self.report_io_failure(
                status,
                health,
                "stopped-status-report-failed",
                error,
            ));
        }
        Ok(())
    }

    fn report_supported_stop(
        &self,
        status: &dyn StatusReporter,
        health: &dyn HealthPublisher,
        error: &StructuredServiceError,
    ) -> Result<(), HostError> {
        let _ = health.publish(&HealthReport {
            protocol_version: HEALTH_PROTOCOL_VERSION,
            service_version: self.service_version.to_owned(),
            health: HealthState::Unknown,
            active_profile_digest: None,
            readiness: ReadinessReport::not_required(),
            injection: InjectionTelemetry::default(),
            last_error: Some(error.clone()),
        });
        crate::event_log::service_start_skipped(error);
        if let Err(error) = status.report(ServiceStatus::stopped()) {
            return Err(self.report_io_failure(
                status,
                health,
                "stopped-status-report-failed",
                error,
            ));
        }
        Ok(())
    }

    fn report_io_failure(
        &self,
        status: &dyn StatusReporter,
        health: &dyn HealthPublisher,
        code: &str,
        error: std::io::Error,
    ) -> HostError {
        let structured = StructuredServiceError {
            code: code.to_owned(),
            message: error.to_string(),
            win32_error: error.raw_os_error().map(|value| value as u32),
        };
        self.report_failure(status, health, &structured);
        HostError::Io(error)
    }

    fn report_failure(
        &self,
        status: &dyn StatusReporter,
        health: &dyn HealthPublisher,
        error: &StructuredServiceError,
    ) {
        if health
            .publish(&HealthReport {
                protocol_version: HEALTH_PROTOCOL_VERSION,
                service_version: self.service_version.to_owned(),
                health: HealthState::Failed,
                active_profile_digest: None,
                readiness: ReadinessReport::initializing(),
                injection: InjectionTelemetry::default(),
                last_error: Some(error.clone()),
            })
            .is_ok()
        {
            crate::event_log::health_changed(HealthState::Failed, Some(error));
        }
        let _ = status.report(ServiceStatus::stopped_with_error(
            ERROR_SERVICE_SPECIFIC_ERROR,
            1,
        ));
    }
}

struct RuntimeHealthAdapter<'a> {
    publisher: &'a dyn HealthPublisher,
    service_version: &'a str,
    active_profile_digest: Option<String>,
}

impl RuntimeHealthReporter for RuntimeHealthAdapter<'_> {
    fn report(
        &self,
        health: HealthState,
        readiness: ReadinessReport,
        injection: InjectionTelemetry,
        last_error: Option<StructuredServiceError>,
    ) -> Result<(), StructuredServiceError> {
        let report = HealthReport {
            protocol_version: HEALTH_PROTOCOL_VERSION,
            service_version: self.service_version.to_owned(),
            health,
            active_profile_digest: self.active_profile_digest.clone(),
            readiness,
            injection,
            last_error,
        };
        report.validate().map_err(|_| StructuredServiceError {
            code: "runtime-health-invalid".to_owned(),
            message: "the runtime attempted to publish an invalid health report".to_owned(),
            win32_error: None,
        })?;
        self.publisher
            .publish(&report)
            .map_err(|error| StructuredServiceError {
                code: "health-publish-failed".to_owned(),
                message: error.to_string(),
                win32_error: error.raw_os_error().map(|code| code as u32),
            })?;
        crate::event_log::health_changed(report.health, report.last_error.as_ref());
        Ok(())
    }
}
