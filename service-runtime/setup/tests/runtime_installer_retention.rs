#![cfg(feature = "ci-test-adapter")]

#[path = "support/runtime_installer/active.rs"]
mod active_support;
#[path = "support/runtime_installer/pin.rs"]
mod pin_support;
#[path = "support/runtime_installer/core.rs"]
mod support;

use std::fs;

use mactype_service_setup::RuntimeInstaller;

use active_support::active_version;
use pin_support::pin_runtime_generation;
use support::{payload, test_paths};
#[test]
fn successful_activation_retains_only_current_and_previous_verified_generations() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    for (version, bytes) in [
        ("0.1.0", b"service-v1".as_slice()),
        ("0.2.0", b"service-v2".as_slice()),
        ("0.3.0", b"service-v3".as_slice()),
    ] {
        installer
            .deploy_with_health_check(&payload(base.path(), version, bytes), |_| Ok(()))
            .unwrap();
    }

    assert!(!paths.runtime_versions().join("0.1.0").exists());
    assert!(paths.runtime_versions().join("0.2.0").is_dir());
    assert!(paths.runtime_versions().join("0.3.0").is_dir());
    let previous: serde_json::Value = serde_json::from_slice(
        &fs::read(paths.service_root().join("previous-runtime.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(previous["version"], "0.2.0");
}

#[test]
fn migration_pin_survives_upgrade_retention_and_restores_the_old_runtime() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.1.0", b"service-v1"), |_| Ok(()))
        .unwrap();
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();
    pin_runtime_generation(&paths, "00112233445566778899aabbccddeeff", "0.1.0");

    installer
        .deploy_with_health_check(&payload(base.path(), "0.3.0", b"service-v3"), |_| Ok(()))
        .unwrap();

    assert!(paths.runtime_versions().join("0.1.0").is_dir());
    fs::write(
        paths.runtime_pointer(),
        br#"{"schema":1,"version":"0.1.0"}"#,
    )
    .unwrap();
    let restored = installer
        .restore_pinned_current_with_health_check(|service_binary| {
            assert_eq!(
                service_binary,
                paths
                    .runtime_versions()
                    .join("0.1.0")
                    .join("mactype-service.exe")
            );
            Ok(())
        })
        .unwrap();
    assert_eq!(restored.version(), "0.1.0");
    assert_eq!(active_version(paths.runtime_pointer()), "0.1.0");
}

#[test]
fn tampered_migration_pin_generation_defers_cleanup_and_cannot_be_restored() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.1.0", b"service-v1"), |_| Ok(()))
        .unwrap();
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();
    pin_runtime_generation(&paths, "ffeeddccbbaa99887766554433221100", "0.1.0");
    fs::write(
        paths
            .runtime_versions()
            .join("0.1.0")
            .join("mactype-service.exe"),
        b"tampered-after-pin",
    )
    .unwrap();

    installer
        .deploy_with_health_check(&payload(base.path(), "0.3.0", b"service-v3"), |_| Ok(()))
        .unwrap();

    assert!(paths.runtime_versions().join("0.1.0").is_dir());
    fs::write(
        paths.runtime_pointer(),
        br#"{"schema":1,"version":"0.1.0"}"#,
    )
    .unwrap();
    assert!(installer
        .restore_pinned_current_with_health_check(|_| Ok(()))
        .is_err());
}

#[test]
fn retention_preserves_an_old_generation_with_an_unexpected_entry() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.1.0", b"service-v1"), |_| Ok(()))
        .unwrap();
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();
    fs::write(
        paths.runtime_versions().join("0.1.0").join("operator.bin"),
        b"preserve me",
    )
    .unwrap();

    installer
        .deploy_with_health_check(&payload(base.path(), "0.3.0", b"service-v3"), |_| Ok(()))
        .unwrap();

    assert!(paths
        .runtime_versions()
        .join("0.1.0")
        .join("operator.bin")
        .exists());
}
