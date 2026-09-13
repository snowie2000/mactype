#![cfg(feature = "ci-test-adapter")]

#[path = "support/runtime_installer/pin.rs"]
mod pin_support;
#[path = "support/runtime_installer/core.rs"]
mod support;

use std::fs;

use mactype_service_contract::{SourceMetadata, MAX_PINNED_RUNTIMES, MAX_RUNTIME_FILE_BYTES};
use mactype_service_setup::{ProfileStore, RuntimeInstaller};

use pin_support::pin_runtime_generation;
use support::{payload, test_paths};
#[test]
fn active_profile_pointer_is_bounded_before_runtime_materialization() {
    let (base, paths) = test_paths();
    let profile = ProfileStore::new(paths.clone())
        .publish_and_activate(
            b"[General]\r\nGammaValue=1.25\r\n",
            SourceMetadata {
                display_name: "bounded pointer".to_owned(),
            },
        )
        .unwrap();
    let mut pointer = serde_json::to_vec(&serde_json::json!({
        "schema": 1,
        "generation": profile.as_str(),
    }))
    .unwrap();
    pointer.resize(64 * 1024 + 1, b' ');
    fs::write(paths.active_profile(), pointer).unwrap();

    let error = RuntimeInstaller::new(paths)
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service"), |_| Ok(()))
        .unwrap_err();

    assert!(error
        .to_string()
        .contains("active profile pointer is not a bounded regular file"));
}

#[test]
fn fixed_payload_rejects_entry_count_before_loading_an_unexpected_file() {
    let (base, paths) = test_paths();
    let fixed = payload(base.path(), "0.2.0", b"service");
    let payload_root = base.path().join("payload-0.2.0").join("files");
    fs::write(payload_root.join("unsigned.bin"), b"unsigned").unwrap();

    let error = RuntimeInstaller::new(paths)
        .deploy_with_health_check(&fixed, |_| Ok(()))
        .unwrap_err();

    assert!(error
        .to_string()
        .contains("payload entry count exceeds the fixed limit"));
}

#[test]
fn existing_runtime_file_is_bounded_before_hash_verification() {
    let (base, paths) = test_paths();
    let fixed = payload(base.path(), "0.2.0", b"service");
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&fixed, |_| Ok(()))
        .unwrap();
    let service = paths
        .runtime_versions()
        .join("0.2.0")
        .join("mactype-service.exe");
    fs::OpenOptions::new()
        .write(true)
        .open(service)
        .unwrap()
        .set_len(MAX_RUNTIME_FILE_BYTES as u64 + 1)
        .unwrap();

    let error = installer
        .deploy_with_health_check(&fixed, |_| Ok(()))
        .unwrap_err();

    assert!(error
        .to_string()
        .contains("installed runtime file is not a bounded regular file"));
}

#[test]
fn runtime_receipt_verification_rejects_entry_overflow_at_the_fixed_boundary() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service"), |_| Ok(()))
        .unwrap();
    let runtime = paths.runtime_versions().join("0.2.0");
    fs::write(runtime.join("first-unsigned.bin"), b"unsigned").unwrap();
    fs::write(runtime.join("second-unsigned.bin"), b"unsigned").unwrap();

    let error = installer.inspect_current_stable().unwrap_err();

    assert!(error
        .to_string()
        .contains("runtime generation entry count exceeds the fixed limit"));
}

#[test]
fn migration_pinned_runtime_rejects_entry_overflow_at_the_fixed_boundary() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service"), |_| Ok(()))
        .unwrap();
    pin_runtime_generation(&paths, "00112233445566778899aabbccddeeff", "0.2.0");
    let runtime = paths.runtime_versions().join("0.2.0");
    fs::write(runtime.join("first-unsigned.bin"), b"unsigned").unwrap();
    fs::write(runtime.join("second-unsigned.bin"), b"unsigned").unwrap();

    let error = installer
        .restore_pinned_current_with_health_check(|_| Ok(()))
        .unwrap_err();

    assert!(error
        .to_string()
        .contains("pinned runtime entry count exceeds the fixed limit"));
}

#[test]
fn uninstall_rejects_runtime_generation_count_overflow_before_verification() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service"), |_| Ok(()))
        .unwrap();
    for index in 0..=MAX_PINNED_RUNTIMES + 1 {
        fs::create_dir(paths.runtime_versions().join(format!("9.0.{index}"))).unwrap();
    }

    let error = installer.remove_receipted_installation().unwrap_err();

    assert!(error
        .to_string()
        .contains("runtime generation count exceeds the fixed limit"));
}

#[test]
fn uninstall_rejects_runtime_receipt_count_overflow_before_set_comparison() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service"), |_| Ok(()))
        .unwrap();
    let receipts = paths.service_root().join("runtime-receipts");
    for index in 0..=MAX_PINNED_RUNTIMES + 1 {
        fs::write(receipts.join(format!("9.0.{index}.json")), b"{}").unwrap();
    }

    let error = installer.remove_receipted_installation().unwrap_err();

    assert!(error
        .to_string()
        .contains("runtime receipt count exceeds the fixed limit"));
}
