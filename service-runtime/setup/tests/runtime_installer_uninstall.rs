#![cfg(feature = "ci-test-adapter")]

#[path = "support/runtime_installer/core.rs"]
mod support;

use std::fs;

use mactype_service_contract::{HealthReport, SourceMetadata};
use mactype_service_setup::{ProfileStore, RuntimeInstaller};

use support::{payload, test_paths};
#[test]
fn uninstall_removes_only_a_fully_receipted_runtime_and_preserves_program_data_profiles() {
    let (base, paths) = test_paths();
    let profile_bytes = b"[General]\r\nGammaValue=1.4\r\n";
    let generation = ProfileStore::new(paths.clone())
        .publish_and_activate(
            profile_bytes,
            SourceMetadata {
                display_name: "preserved after uninstall".to_owned(),
            },
        )
        .unwrap();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();
    fs::write(
        paths.service_root().join("health.json"),
        serde_json::to_vec(&HealthReport::ready(
            "0.2.0",
            Some(generation.as_str().to_owned()),
        ))
        .unwrap(),
    )
    .unwrap();
    let active_pointer = fs::read(paths.active_profile()).unwrap();

    assert!(installer.remove_receipted_installation().unwrap());

    assert!(!paths.service_root().exists());
    assert_eq!(fs::read(paths.active_profile()).unwrap(), active_pointer);
    assert_eq!(
        fs::read(
            paths
                .profile_generations()
                .join(generation.directory_name())
                .join("profile.ini")
        )
        .unwrap(),
        profile_bytes
    );
}

#[test]
fn uninstall_refuses_an_unsigned_service_root_entry_without_deleting_owned_files() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();
    let foreign = paths.service_root().join("operator-note.txt");
    fs::write(&foreign, b"preserve me").unwrap();

    let error = installer.remove_receipted_installation().unwrap_err();

    assert!(error
        .to_string()
        .contains("unexpected service runtime entry"));
    assert!(foreign.is_file());
    assert!(paths.runtime_pointer().is_file());
    assert!(paths
        .runtime_versions()
        .join("0.2.0")
        .join("mactype-service.exe")
        .is_file());
}

#[test]
fn uninstall_refuses_a_tampered_receipted_generation_without_partial_cleanup() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();
    let service = paths
        .runtime_versions()
        .join("0.2.0")
        .join("mactype-service.exe");
    fs::write(&service, b"tampered").unwrap();

    let error = installer.remove_receipted_installation().unwrap_err();

    assert!(error.to_string().contains("differs from its receipt"));
    assert_eq!(fs::read(service).unwrap(), b"tampered");
    assert!(paths.runtime_pointer().is_file());
}
