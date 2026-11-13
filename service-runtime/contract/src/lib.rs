mod appinit;
mod broker;
pub mod event_log;
mod health;
mod manifest;
mod migration_pin;
mod paths;
mod profile;
mod runtime_activation;
mod service_configuration;

pub use broker::{BrokerCommand, BrokerCommandError, parse_broker_command};
pub use health::{
    ArchitectureInjectionTelemetry, ComponentReadiness, HEALTH_PROTOCOL_VERSION,
    HealthContractError, HealthReport, HealthState, InjectionArchitecture, InjectionSuccess,
    InjectionTelemetry, ReadinessReport, StructuredServiceError,
};
pub use manifest::{
    IMMUTABLE_RUNTIME_FILES, MAX_RUNTIME_FILE_BYTES, ManifestError, RUNTIME_MANIFEST_SCHEMA,
    VerifiedRuntimeManifest, runtime_generation_id, sha256_digest, verify_runtime_manifest,
};
pub use migration_pin::{
    MAX_MIGRATION_RUNTIME_PIN_BYTES, MAX_PINNED_RUNTIMES, MIGRATION_RUNTIME_PIN_SCHEMA,
    MigrationPinError, MigrationPinnedRuntime, MigrationRuntimePin,
};
pub use paths::{MachinePathError, MachinePaths};
pub use profile::{
    GenerationId, GenerationPointer, MAX_PROFILE_BYTES, MAX_PROFILE_POINTER_BYTES,
    PROFILE_POINTER_SCHEMA, ProfileCatalog, ProfileError, SourceMetadata,
};
pub use runtime_activation::{
    LEGACY_RUNTIME_ACTIVATION_SCHEMA, MAX_RUNTIME_ACTIVATION_RECEIPT_BYTES,
    MAX_RUNTIME_POINTER_BYTES, ParsedRuntimeActivationReceipt, RUNTIME_ACTIVATION_SCHEMA,
    RUNTIME_POINTER_SCHEMA, RuntimeActivationContractError, RuntimeActivationPhase,
    RuntimeActivationReceipt, RuntimeGenerationPointer, UNCOMMITTED_RUNTIME_ACTIVATION_SCHEMA,
    parse_runtime_activation_receipt, valid_runtime_version_component,
};
pub use service_configuration::{
    FIXED_ACCOUNT, FIXED_ERROR_CONTROL, FIXED_SERVICE_TYPE, FIXED_START_TYPE,
    ObservedServiceConfiguration, SERVICE_DISPLAY_NAME, ServiceConfigurationDrift,
    owned_service_identity, service_configuration_drift, service_image_matches_protected_contract,
    service_image_matches_protected_layout,
};

pub const SERVICE_NAME: &str = "MacTypeControlCenter";
pub const LEGACY_SERVICE_NAME: &str = "MacType";
pub const CI_TEST_SERVICE_NAME: &str = "MacTypeControlCenterTest";
pub const HEALTH_PIPE_NAME: &str = r"\\.\pipe\MacTypeControlCenter.health.v1";
pub const CI_TEST_HEALTH_PIPE_NAME: &str = r"\\.\pipe\MacTypeControlCenterTest.health.v1";

pub const fn effective_service_name() -> &'static str {
    if cfg!(feature = "ci-test-adapter") {
        CI_TEST_SERVICE_NAME
    } else {
        SERVICE_NAME
    }
}

pub const fn effective_health_pipe_name() -> &'static str {
    if cfg!(feature = "ci-test-adapter") {
        CI_TEST_HEALTH_PIPE_NAME
    } else {
        HEALTH_PIPE_NAME
    }
}
pub use appinit::{AppInitValueError, appinit_mactype_conflict};
