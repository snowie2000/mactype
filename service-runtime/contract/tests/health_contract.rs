use mactype_service_contract::{
    ComponentReadiness, HealthReport, HealthState, InjectionArchitecture, InjectionSuccess,
    ReadinessReport, StructuredServiceError, HEALTH_PIPE_NAME, HEALTH_PROTOCOL_VERSION,
};

const PROFILE: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn ready_health_is_versioned_and_distinct_from_scm_running() {
    let report = HealthReport::ready("0.2.0", Some(PROFILE.to_owned()));

    assert_eq!(HEALTH_PIPE_NAME, r"\\.\pipe\MacTypeControlCenter.health.v1");
    assert_eq!(report.protocol_version, HEALTH_PROTOCOL_VERSION);
    assert_eq!(report.health, HealthState::Ready);
    assert_eq!(report.readiness.observer, ComponentReadiness::Ready);
    assert!(report.is_active_for(PROFILE));
    assert!(!report.is_active_for("sha256:different"));

    let encoded = serde_json::to_vec(&report).unwrap();
    let decoded: HealthReport = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, report);
    assert!(decoded.validate().is_ok());
}

#[test]
fn terminal_unknown_health_accepts_not_required_readiness_without_runtime_state() {
    let report = HealthReport {
        protocol_version: HEALTH_PROTOCOL_VERSION,
        service_version: "0.2.0".to_owned(),
        health: HealthState::Unknown,
        active_profile_digest: None,
        readiness: ReadinessReport::not_required(),
        injection: Default::default(),
        last_error: None,
    };

    assert_eq!(report.readiness.profile, ComponentReadiness::NotRequired);
    assert_eq!(report.readiness.observer, ComponentReadiness::NotRequired);
    assert_eq!(report.readiness.injector32, ComponentReadiness::NotRequired);
    assert_eq!(report.readiness.injector64, ComponentReadiness::NotRequired);
    assert!(report.validate().is_ok());
}

#[test]
fn running_without_ready_health_cannot_be_reported_as_active() {
    let report = HealthReport {
        protocol_version: HEALTH_PROTOCOL_VERSION,
        service_version: "0.2.0".to_owned(),
        health: HealthState::Initializing,
        active_profile_digest: Some(PROFILE.to_owned()),
        readiness: ReadinessReport::initializing(),
        injection: Default::default(),
        last_error: None,
    };
    assert!(!report.is_active_for(PROFILE));

    let failed = HealthReport {
        protocol_version: HEALTH_PROTOCOL_VERSION,
        service_version: "0.2.0".to_owned(),
        health: HealthState::Failed,
        active_profile_digest: None,
        readiness: ReadinessReport {
            profile: ComponentReadiness::Failed,
            observer: ComponentReadiness::Initializing,
            injector32: ComponentReadiness::NotRequired,
            injector64: ComponentReadiness::NotRequired,
        },
        injection: Default::default(),
        last_error: Some(StructuredServiceError {
            code: "profile-invalid".to_owned(),
            message: "active profile failed validation".to_owned(),
            win32_error: None,
        }),
    };
    assert!(!failed.is_active_for("sha256:abc"));
}

#[test]
fn ready_health_requires_every_required_component_to_be_ready() {
    let mut report = HealthReport::ready("0.2.0", Some(PROFILE.to_owned()));
    report.readiness.observer = ComponentReadiness::Initializing;
    assert!(report.validate().is_err());
    assert!(!report.is_active_for(PROFILE));
}

#[test]
fn open_service_ready_cannot_bypass_observer_or_helpers_as_not_required() {
    for component in ["profile", "observer", "injector32", "injector64"] {
        let mut report = HealthReport::ready("0.2.0", Some(PROFILE.to_owned()));
        match component {
            "profile" => report.readiness.profile = ComponentReadiness::NotRequired,
            "observer" => report.readiness.observer = ComponentReadiness::NotRequired,
            "injector32" => report.readiness.injector32 = ComponentReadiness::NotRequired,
            "injector64" => report.readiness.injector64 = ComponentReadiness::NotRequired,
            _ => unreachable!(),
        }

        assert!(report.validate().is_err(), "{component} bypassed readiness");
        assert!(!report.is_active_for(PROFILE));
    }
}

#[test]
fn unsupported_health_protocol_is_rejected() {
    let mut report = HealthReport::ready("0.2.0", Some(PROFILE.to_owned()));
    report.protocol_version += 1;
    assert!(report.validate().is_err());
}

#[test]
fn migration_verification_requires_both_architectures_for_the_current_runtime_and_profile() {
    let generation = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let profile = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let mut report = HealthReport::ready("0.2.0", Some(profile.to_owned()));
    assert!(!report.verified_for_migration(generation, profile));

    report.injection.record_success(
        InjectionArchitecture::X86,
        InjectionSuccess {
            pid: 42,
            creation_time: 100,
            session_id: 2,
            runtime_generation_id: generation.to_owned(),
            profile_digest: profile.to_owned(),
        },
    );
    assert!(!report.verified_for_migration(generation, profile));

    report.injection.record_success(
        InjectionArchitecture::X64,
        InjectionSuccess {
            pid: 43,
            creation_time: 101,
            session_id: 2,
            runtime_generation_id: generation.to_owned(),
            profile_digest: profile.to_owned(),
        },
    );
    assert!(report.verified_for_migration(generation, profile));
    assert!(!report.verified_for_migration(
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        profile
    ));
    assert!(!report.verified_for_migration(
        generation,
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    ));

    let encoded = serde_json::to_vec(&report).unwrap();
    let decoded: HealthReport = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, report);
}

#[test]
fn pre_telemetry_health_reports_decode_with_empty_counters() {
    let encoded = br#"{
        "protocolVersion":1,
        "serviceVersion":"0.2.0",
        "health":"ready",
        "activeProfileDigest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "readiness":{"profile":"ready","observer":"ready","injector32":"ready","injector64":"ready"},
        "lastError":null
    }"#;

    let report: HealthReport = serde_json::from_slice(encoded).unwrap();
    assert_eq!(report.injection.x86.success_count, 0);
    assert_eq!(report.injection.x64.success_count, 0);
}

#[test]
fn external_health_input_obeys_state_digest_and_error_bounds() {
    let profile = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let mut report = HealthReport::ready("0.2.0", Some(profile.to_owned()));

    report.active_profile_digest = Some("sha256:abc".to_owned());
    assert!(report.validate().is_err());
    report.active_profile_digest = Some(profile.to_owned());
    report.last_error = Some(StructuredServiceError {
        code: "unexpected".to_owned(),
        message: "Ready cannot carry an error".to_owned(),
        win32_error: None,
    });
    assert!(report.validate().is_err());

    report.health = HealthState::Degraded;
    report.last_error = None;
    assert!(report.validate().is_err());
    report.health = HealthState::Failed;
    assert!(report.validate().is_err());

    report.health = HealthState::Initializing;
    report.active_profile_digest = Some(profile.to_owned());
    assert!(report.validate().is_err());

    report.active_profile_digest = None;
    report.last_error = Some(StructuredServiceError {
        code: "x".repeat(129),
        message: "bounded".to_owned(),
        win32_error: None,
    });
    assert!(report.validate().is_err());
    report.last_error = Some(StructuredServiceError {
        code: "bounded".to_owned(),
        message: "x".repeat(1025),
        win32_error: None,
    });
    assert!(report.validate().is_err());
}

#[test]
fn ready_health_requires_a_canonical_active_profile() {
    let report = HealthReport::ready("0.2.0", None);
    assert!(report.validate().is_err());
    assert!(!report.is_active_for(PROFILE));
}
