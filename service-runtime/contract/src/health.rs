use std::fmt;

use serde::{Deserialize, Serialize};

pub const HEALTH_PROTOCOL_VERSION: u16 = 1;
const MAX_ERROR_CODE_BYTES: usize = 128;
const MAX_ERROR_MESSAGE_BYTES: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    Unknown,
    Initializing,
    Ready,
    Degraded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentReadiness {
    NotRequired,
    Initializing,
    Ready,
    Failed,
}

impl ComponentReadiness {
    fn satisfies_ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReadinessReport {
    pub profile: ComponentReadiness,
    pub observer: ComponentReadiness,
    pub injector32: ComponentReadiness,
    pub injector64: ComponentReadiness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectionArchitecture {
    X86,
    X64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InjectionSuccess {
    pub pid: u32,
    pub creation_time: u64,
    pub session_id: u32,
    pub runtime_generation_id: String,
    pub profile_digest: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ArchitectureInjectionTelemetry {
    pub success_count: u64,
    pub last_success: Option<InjectionSuccess>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InjectionTelemetry {
    pub x86: ArchitectureInjectionTelemetry,
    pub x64: ArchitectureInjectionTelemetry,
}

impl InjectionTelemetry {
    pub fn record_success(
        &mut self,
        architecture: InjectionArchitecture,
        success: InjectionSuccess,
    ) {
        let telemetry = match architecture {
            InjectionArchitecture::X86 => &mut self.x86,
            InjectionArchitecture::X64 => &mut self.x64,
        };
        telemetry.success_count = telemetry.success_count.saturating_add(1);
        telemetry.last_success = Some(success);
    }

    pub fn verified_for_migration(&self, generation_id: &str, profile_digest: &str) -> bool {
        [&self.x86, &self.x64].into_iter().all(|telemetry| {
            telemetry.success_count > 0
                && telemetry.last_success.as_ref().is_some_and(|success| {
                    success.runtime_generation_id == generation_id
                        && success.profile_digest == profile_digest
                })
        })
    }

    fn validate(&self) -> bool {
        [&self.x86, &self.x64]
            .into_iter()
            .all(ArchitectureInjectionTelemetry::validate)
    }
}

impl ArchitectureInjectionTelemetry {
    fn validate(&self) -> bool {
        match (self.success_count, &self.last_success) {
            (0, None) => true,
            (0, Some(_)) | (_, None) => false,
            (_, Some(success)) => {
                success.pid != 0
                    && success.creation_time != 0
                    && success.session_id != 0
                    && canonical_generation_id(&success.runtime_generation_id)
                    && canonical_profile_digest(&success.profile_digest)
            }
        }
    }
}

impl ReadinessReport {
    pub const fn not_required() -> Self {
        Self {
            profile: ComponentReadiness::NotRequired,
            observer: ComponentReadiness::NotRequired,
            injector32: ComponentReadiness::NotRequired,
            injector64: ComponentReadiness::NotRequired,
        }
    }

    pub const fn initializing() -> Self {
        Self {
            profile: ComponentReadiness::Initializing,
            observer: ComponentReadiness::Initializing,
            injector32: ComponentReadiness::Initializing,
            injector64: ComponentReadiness::Initializing,
        }
    }

    pub const fn ready() -> Self {
        Self {
            profile: ComponentReadiness::Ready,
            observer: ComponentReadiness::Ready,
            injector32: ComponentReadiness::Ready,
            injector64: ComponentReadiness::Ready,
        }
    }

    pub fn all_required_ready(&self) -> bool {
        self.profile.satisfies_ready()
            && self.observer.satisfies_ready()
            && self.injector32.satisfies_ready()
            && self.injector64.satisfies_ready()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuredServiceError {
    pub code: String,
    pub message: String,
    pub win32_error: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HealthReport {
    pub protocol_version: u16,
    pub service_version: String,
    pub health: HealthState,
    pub active_profile_digest: Option<String>,
    pub readiness: ReadinessReport,
    #[serde(default)]
    pub injection: InjectionTelemetry,
    pub last_error: Option<StructuredServiceError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealthContractError;

impl fmt::Display for HealthContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("unsupported or malformed service health report")
    }
}

impl std::error::Error for HealthContractError {}

impl HealthReport {
    pub fn ready(service_version: impl Into<String>, digest: Option<String>) -> Self {
        Self {
            protocol_version: HEALTH_PROTOCOL_VERSION,
            service_version: service_version.into(),
            health: HealthState::Ready,
            active_profile_digest: digest,
            readiness: ReadinessReport::ready(),
            injection: InjectionTelemetry::default(),
            last_error: None,
        }
    }

    pub fn validate(&self) -> Result<(), HealthContractError> {
        let state_is_consistent = match self.health {
            HealthState::Ready => self.last_error.is_none() && self.active_profile_digest.is_some(),
            HealthState::Degraded | HealthState::Failed => self.last_error.is_some(),
            HealthState::Initializing => self.active_profile_digest.is_none(),
            HealthState::Unknown => true,
        };
        if self.protocol_version != HEALTH_PROTOCOL_VERSION
            || self.service_version.is_empty()
            || self.service_version.len() > 64
            || (self.health == HealthState::Ready && !self.readiness.all_required_ready())
            || self
                .active_profile_digest
                .as_deref()
                .is_some_and(|digest| !canonical_profile_digest(digest))
            || !state_is_consistent
            || !self.injection.validate()
            || self.last_error.as_ref().is_some_and(|error| {
                error.code.is_empty()
                    || error.code.len() > MAX_ERROR_CODE_BYTES
                    || error.message.is_empty()
                    || error.message.len() > MAX_ERROR_MESSAGE_BYTES
            })
        {
            return Err(HealthContractError);
        }
        Ok(())
    }

    pub fn is_active_for(&self, expected_digest: &str) -> bool {
        self.validate().is_ok()
            && self.health == HealthState::Ready
            && self.readiness.all_required_ready()
            && self.active_profile_digest.as_deref() == Some(expected_digest)
    }

    pub fn verified_for_migration(
        &self,
        runtime_generation_id: &str,
        profile_digest: &str,
    ) -> bool {
        self.is_active_for(profile_digest)
            && self
                .injection
                .verified_for_migration(runtime_generation_id, profile_digest)
    }
}

fn canonical_generation_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn canonical_profile_digest(value: &str) -> bool {
    value.len() == 71 && value.starts_with("sha256:") && canonical_generation_id(&value[7..])
}
