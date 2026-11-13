use super::super::*;
use crate::machine_integration::open_service::identity::core_service_capabilities;
use crate::{
    machine_integration::{
        LegacyTrayConflictState, legacy_mactray, status::project_new_service_capabilities,
    },
    service_contract::{
        HealthState, InstallationState, RuntimeState, ServiceBackend,
        ServiceManagementPackageState, SystemServiceStatus,
    },
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CapabilityCase {
    name: String,
    input: CapabilityInput,
    expected: CapabilityExpected,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CapabilityInput {
    runtime: RuntimeState,
    installation: InstallationState,
    configuration_drift: bool,
    backend: ServiceBackend,
    registry_mode_detected: bool,
    legacy_tray_conflict: LegacyTrayConflictState,
    legacy: LegacyCapabilityInput,
    management_package: ServiceManagementPackageState,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LegacyCapabilityInput {
    presence: legacy_mactray::ServicePresence,
    state: legacy_mactray::ServiceRuntimeState,
    migration_available: Option<bool>,
}

#[derive(Debug, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CapabilityExpected {
    can_install: bool,
    can_remove: bool,
    can_start: bool,
    can_stop: bool,
    can_repair: bool,
    can_upgrade: bool,
    system_modes_supported: bool,
    migration_available: bool,
}

impl CapabilityExpected {
    fn from_projection(
        service: &SystemServiceStatus,
        system_modes_supported: bool,
        migration_available: bool,
    ) -> Self {
        Self {
            can_install: service.can_install,
            can_remove: service.can_remove,
            can_start: service.can_start,
            can_stop: service.can_stop,
            can_repair: service.can_repair,
            can_upgrade: service.can_upgrade,
            system_modes_supported,
            migration_available,
        }
    }
}

fn capability_service(input: &CapabilityInput) -> SystemServiceStatus {
    if input.backend == ServiceBackend::None && input.installation == InstallationState::Absent {
        let status = absent_status();
        assert_eq!(status.runtime, input.runtime);
        assert_eq!(status.configuration_drift, input.configuration_drift);
        return status;
    }
    let capabilities =
        core_service_capabilities(input.runtime, input.installation, input.configuration_drift);
    SystemServiceStatus {
        backend: input.backend,
        installation: input.installation,
        runtime: input.runtime,
        health: HealthState::Unknown,
        binary_path: None,
        win32_error: None,
        active_profile_digest: None,
        configuration_drift: input.configuration_drift,
        can_install: false,
        can_remove: input.backend == ServiceBackend::OpenSource && capabilities.can_remove,
        can_start: input.backend == ServiceBackend::OpenSource && capabilities.can_start,
        can_stop: input.backend == ServiceBackend::OpenSource && capabilities.can_stop,
        can_repair: input.backend == ServiceBackend::OpenSource && capabilities.can_repair,
        can_upgrade: input.backend == ServiceBackend::OpenSource && capabilities.can_upgrade,
    }
}

fn legacy_capability(input: &CapabilityInput) -> bool {
    let status = legacy_mactray::LegacyServiceStatus {
        presence: input.legacy.presence,
        state: input.legacy.state,
        binary_path: None,
        win32_error: None,
        trusted_binary_available: false,
        registry_conflict: input.registry_mode_detected,
        can_remove: false,
        can_stop: false,
    };
    let available = legacy_migration_available(&status);
    if let Some(declared) = input.legacy.migration_available {
        assert_eq!(available, declared, "legacy.migrationAvailable input");
    }
    available
}

#[test]
fn shared_service_capability_cases_match_rust_projections() {
    let cases: Vec<CapabilityCase> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../shared/service-capability-cases.json"
    )))
    .unwrap();

    for case in cases {
        let service = capability_service(&case.input);
        let service = project_new_service_capabilities(
            service,
            case.input.registry_mode_detected,
            case.input.legacy_tray_conflict,
        );
        let (service, system_modes_supported) = crate::execution::project_execution_capabilities(
            service,
            case.input.registry_mode_detected,
            case.input.management_package,
        );
        let actual = CapabilityExpected::from_projection(
            &service,
            system_modes_supported,
            legacy_capability(&case.input),
        );
        assert_eq!(actual, case.expected, "{}", case.name);
    }
}
