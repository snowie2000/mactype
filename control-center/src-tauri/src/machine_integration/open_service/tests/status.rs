use super::super::*;
use crate::machine_integration::legacy_mactray::{
    LegacyServiceStatus, ServicePresence, ServiceRuntimeState,
};
use crate::machine_integration::open_service::identity::{
    core_service_capabilities, core_service_configuration_drift, owned_core_service_identity,
    ObservedCoreServiceConfiguration,
};

#[test]
fn absent_service_never_claims_system_injection() {
    let status = absent_status();
    assert!(!status.system_injection_active(Some("sha256:profile")));
    assert!(status.can_install);
    assert!(!status.can_start);
}

#[test]
fn stale_legacy_migration_requires_a_stopped_service_or_a_restartable_binary() {
    let mut status = LegacyServiceStatus {
        presence: ServicePresence::Owned,
        state: ServiceRuntimeState::Stopped,
        binary_path: None,
        win32_error: None,
        trusted_binary_available: false,
        registry_conflict: false,
        can_remove: true,
        can_stop: false,
    };
    assert!(legacy_migration_available(&status));

    status.state = ServiceRuntimeState::Running;
    assert!(!legacy_migration_available(&status));

    status.trusted_binary_available = true;
    assert!(legacy_migration_available(&status));

    status.state = ServiceRuntimeState::StopPending;
    assert!(!legacy_migration_available(&status));

    status.state = ServiceRuntimeState::Stopped;
    status.registry_conflict = true;
    assert!(!legacy_migration_available(&status));
}

#[test]
fn bundled_manifest_version_drives_outdated_classification() {
    let manifest = br#"{"schema":1,"version":"0.3.0","files":{"MacType.dll":"sha256:0000000000000000000000000000000000000000000000000000000000000000","MacType64.dll":"sha256:0000000000000000000000000000000000000000000000000000000000000000","mactype-injector32.exe":"sha256:0000000000000000000000000000000000000000000000000000000000000000","mactype-injector64.exe":"sha256:0000000000000000000000000000000000000000000000000000000000000000","mactype-service.exe":"sha256:0000000000000000000000000000000000000000000000000000000000000000"}}"#;
    assert_eq!(bundled_runtime_version(manifest).unwrap(), "0.3.0");
    assert!(bundled_runtime_version(br#"{"schema":2,"version":"0.3.0","files":{}}"#).is_err());

    let root = std::path::Path::new(r"C:\Program Files\MacType Control Center\Service");
    let configured = root.join("bin").join("0.2.0").join("mactype-service.exe");
    let pointer = configured.clone();
    let bundled = root.join("bin").join("0.3.0").join("mactype-service.exe");
    assert_eq!(
        classify_owned_installation(&configured, &pointer, &bundled),
        InstallationState::Outdated
    );
    assert_eq!(
        classify_owned_installation(&bundled, &bundled, &bundled),
        InstallationState::Current
    );
}

#[test]
fn status_separates_core_service_identity_from_configuration_drift() {
    let exact = ObservedCoreServiceConfiguration {
        service_type: 0x10,
        start_type: 2,
        error_control: 1,
        account: "LocalSystem",
        display_name: "MacType Control Center Service",
        load_order_group: "",
        tag_id: 0,
        dependencies_empty: true,
        protected_image: true,
    };
    assert!(owned_core_service_identity(&exact));
    assert!(!core_service_configuration_drift(&exact));

    for drift in [
        ObservedCoreServiceConfiguration {
            start_type: 3,
            ..exact
        },
        ObservedCoreServiceConfiguration {
            error_control: 0,
            ..exact
        },
        ObservedCoreServiceConfiguration {
            display_name: "Foreign Display",
            load_order_group: "group",
            tag_id: 1,
            dependencies_empty: false,
            ..exact
        },
    ] {
        assert!(owned_core_service_identity(&drift));
        assert!(core_service_configuration_drift(&drift));
    }

    for foreign in [
        ObservedCoreServiceConfiguration {
            service_type: 0x20,
            ..exact
        },
        ObservedCoreServiceConfiguration {
            account: "LocalService",
            ..exact
        },
        ObservedCoreServiceConfiguration {
            protected_image: false,
            ..exact
        },
    ] {
        assert!(!owned_core_service_identity(&foreign));
    }
}

#[test]
fn service_capability_matrix_treats_drift_as_repairable_owned_state() {
    let flags = |runtime, installation, drift| {
        let capabilities = core_service_capabilities(runtime, installation, drift);
        (
            capabilities.can_remove,
            capabilities.can_start,
            capabilities.can_stop,
            capabilities.can_repair,
            capabilities.can_upgrade,
        )
    };

    assert_eq!(
        flags(RuntimeState::Stopped, InstallationState::Current, false),
        (true, true, false, true, false)
    );
    assert_eq!(
        flags(RuntimeState::Stopped, InstallationState::Current, true),
        (true, false, false, true, false)
    );
    assert_eq!(
        flags(RuntimeState::Running, InstallationState::Current, true),
        (true, false, true, true, false)
    );
    assert_eq!(
        flags(RuntimeState::Stopped, InstallationState::Outdated, true),
        (true, false, false, false, true)
    );
    assert_eq!(
        flags(RuntimeState::Running, InstallationState::Outdated, false),
        (true, false, true, false, true)
    );
    for runtime in [
        RuntimeState::StartPending,
        RuntimeState::StopPending,
        RuntimeState::Paused,
        RuntimeState::Unknown,
    ] {
        assert_eq!(
            flags(runtime, InstallationState::Current, true),
            (false, false, false, false, false)
        );
    }
}

#[test]
fn persisted_health_is_diagnostic_only_and_never_revives_stale_ready() {
    let ready = HealthReport::ready(
        "0.2.0",
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned()),
    );
    let mut failed = ready.clone();
    failed.health = mactype_service_contract::HealthState::Failed;
    failed.active_profile_digest = None;
    failed.readiness = mactype_service_contract::ReadinessReport::initializing();
    failed.last_error = Some(mactype_service_contract::StructuredServiceError {
        code: "service-panic".to_owned(),
        message: "panic boundary".to_owned(),
        win32_error: None,
    });
    assert!(failed.validate().is_ok());
    let mut degraded = failed.clone();
    degraded.health = mactype_service_contract::HealthState::Degraded;
    assert!(degraded.validate().is_ok());
    let terminal = HealthReport {
        protocol_version: mactype_service_contract::HEALTH_PROTOCOL_VERSION,
        service_version: "0.2.0".to_owned(),
        health: mactype_service_contract::HealthState::Unknown,
        active_profile_digest: None,
        readiness: mactype_service_contract::ReadinessReport::not_required(),
        injection: Default::default(),
        last_error: None,
    };
    assert!(terminal.validate().is_ok());

    assert!(select_service_health(RuntimeState::Stopped, 0, None, Some(ready.clone())).is_none());
    assert!(select_service_health(RuntimeState::Stopped, 0, None, Some(terminal)).is_none());
    let stopped_degradation =
        select_service_health(RuntimeState::Stopped, 0, None, Some(degraded)).unwrap();
    assert!(!stopped_degradation.live);
    assert_eq!(
        stopped_degradation.report.health,
        mactype_service_contract::HealthState::Degraded
    );
    let stopped_failure =
        select_service_health(RuntimeState::Stopped, 0, None, Some(failed.clone())).unwrap();
    assert!(!stopped_failure.live);
    assert_eq!(
        stopped_failure.report.health,
        mactype_service_contract::HealthState::Failed
    );
    for transitional in [
        RuntimeState::StartPending,
        RuntimeState::StopPending,
        RuntimeState::Paused,
        RuntimeState::Unknown,
    ] {
        assert!(select_service_health(transitional, 0, None, Some(failed.clone())).is_none());
    }
    assert!(select_service_health(RuntimeState::Running, 42, None, Some(ready)).is_none());
    assert!(
        select_service_health(
            RuntimeState::Running,
            42,
            Some(LiveHealthReport {
                server_pid: 42,
                report: failed,
            }),
            None,
        )
        .unwrap()
        .live
    );
}

#[test]
fn live_ready_is_authoritative_only_when_the_pipe_server_pid_matches_scm() {
    let ready = HealthReport::ready(
        "0.2.0",
        Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned()),
    );

    assert!(select_service_health(
        RuntimeState::Running,
        4242,
        Some(LiveHealthReport {
            server_pid: 7331,
            report: ready.clone(),
        }),
        None,
    )
    .is_none());
    assert!(select_service_health(
        RuntimeState::Running,
        4242,
        Some(LiveHealthReport {
            server_pid: 4242,
            report: ready,
        }),
        None,
    )
    .is_some());
}

#[test]
fn reveal_accepts_only_owned_stable_protected_service_images() {
    let root = std::path::Path::new(r"C:\Program Files\MacType Control Center\Service");
    let binary = root.join("bin").join("0.3.0").join("mactype-service.exe");
    let mut status = absent_status();
    status.backend = ServiceBackend::OpenSource;
    status.installation = InstallationState::Current;
    status.runtime = RuntimeState::Running;
    status.binary_path = Some(format!(r#""{}" --service"#, binary.display()));
    assert_eq!(validated_reveal_binary(root, &status).unwrap(), binary);

    status.runtime = RuntimeState::StartPending;
    assert!(validated_reveal_binary(root, &status).is_err());
    status.runtime = RuntimeState::Running;
    status.backend = ServiceBackend::Foreign;
    assert!(validated_reveal_binary(root, &status).is_err());
    status.backend = ServiceBackend::OpenSource;
    status.binary_path = Some(r#""C:\Users\person\mactype-service.exe" --service"#.to_owned());
    assert!(validated_reveal_binary(root, &status).is_err());
}
