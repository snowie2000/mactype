mod lifecycle;

use std::fmt;
use std::io;

use mactype_service_contract::{ReadinessReport, StructuredServiceError};

pub trait HealthPublisher: Send + Sync {
    fn publish(&self, report: &mactype_service_contract::HealthReport) -> io::Result<()>;
}

pub trait RuntimeInitializer: Send + Sync {
    fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError>;
}

pub trait StopSignal: Send + Sync {
    fn wait(&self) -> Result<(), StructuredServiceError>;

    fn wait_timeout(&self, _timeout: std::time::Duration) -> Result<bool, StructuredServiceError> {
        self.wait().map(|()| true)
    }

    fn stop_requested(&self) -> bool {
        false
    }

    fn take_session_change(&self) -> Option<crate::SessionChange> {
        None
    }
}

pub struct InitializedRuntime {
    pub active_profile_digest: Option<String>,
    pub readiness: ReadinessReport,
    driver: Option<Box<dyn RuntimeDriver>>,
}

impl InitializedRuntime {
    pub fn ready(active_profile_digest: Option<String>, readiness: ReadinessReport) -> Self {
        Self {
            active_profile_digest,
            readiness,
            driver: None,
        }
    }

    pub fn driven(
        active_profile_digest: Option<String>,
        readiness: ReadinessReport,
        driver: Box<dyn RuntimeDriver>,
    ) -> Self {
        Self {
            active_profile_digest,
            readiness,
            driver: Some(driver),
        }
    }
}

pub trait RuntimeHealthReporter {
    fn report(
        &self,
        health: mactype_service_contract::HealthState,
        readiness: ReadinessReport,
        injection: mactype_service_contract::InjectionTelemetry,
        last_error: Option<StructuredServiceError>,
    ) -> Result<(), StructuredServiceError>;
}

pub trait RuntimeDriver {
    fn run(
        &mut self,
        stop: &dyn StopSignal,
        health: &dyn RuntimeHealthReporter,
    ) -> Result<(), StructuredServiceError>;
}

#[derive(Debug)]
pub enum HostError {
    Io(io::Error),
    Runtime(StructuredServiceError),
}

impl fmt::Display for HostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "service host I/O failed: {error}"),
            Self::Runtime(error) => write!(
                formatter,
                "service runtime failed at {}: {}",
                error.code, error.message
            ),
        }
    }
}

impl std::error::Error for HostError {}

impl From<io::Error> for HostError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub struct ServiceRuntime<'a> {
    service_version: &'a str,
}

impl<'a> ServiceRuntime<'a> {
    pub const fn new(service_version: &'a str) -> Self {
        Self { service_version }
    }
}
