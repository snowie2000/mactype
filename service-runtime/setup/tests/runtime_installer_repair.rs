#![cfg(feature = "ci-test-adapter")]

#[path = "support/runtime_installer/active.rs"]
mod active_support;
#[path = "support/runtime_installer/core.rs"]
mod support;

use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs;

use mactype_service_contract::{sha256_digest, MachinePaths};
use mactype_service_setup::{RuntimeInstaller, RuntimeServiceBinding, SetupError};

use active_support::active_version;
use support::{payload, test_paths};
#[test]
fn repair_replaces_a_corrupted_current_runtime_from_the_verified_payload() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    let payload = payload(base.path(), "0.2.0", b"service-v2");
    installer
        .deploy_with_health_check(&payload, |_| Ok(()))
        .unwrap();
    let service = paths
        .runtime_versions()
        .join("0.2.0")
        .join("mactype-service.exe");
    fs::write(&service, b"corrupted").unwrap();

    installer
        .repair_with_health_check(&payload, |_| Ok(()))
        .unwrap();

    assert_eq!(fs::read(service).unwrap(), b"service-v2");
}

#[test]
fn repair_refuses_to_turn_an_outdated_runtime_into_an_implicit_upgrade() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();

    let result = installer.repair_current_with_health_check(
        &payload(base.path(), "0.3.0", b"service-v3"),
        |_| Ok(()),
    );

    assert!(result.is_err());
    assert_eq!(active_version(paths.runtime_pointer()), "0.2.0");
}

#[test]
fn same_version_repair_treats_the_exact_candidate_binding_as_the_previous_binding() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    let payload = payload(base.path(), "0.2.0", b"service-v2");
    installer
        .deploy_with_health_check(&payload, |_| Ok(()))
        .unwrap();

    let repair = installer.repair_current_with_prepare_and_health_check(
        &payload,
        |_| Ok(()),
        |_, _| {
            Err(SetupError::Runtime(
                "repair did not become Ready".to_owned(),
            ))
        },
    );
    assert!(repair.is_err());
    assert!(paths.runtime_activation_journal().is_file());

    let recovered = installer
        .recover_interrupted_activation_with_service_binding(
            |candidate, previous| {
                assert_eq!(candidate, previous);
                Ok(RuntimeServiceBinding::Candidate)
            },
            |_, _| {
                panic!("an identical repair binding must not be reconfigured as its own rollback")
            },
        )
        .unwrap()
        .expect("same-version repair must recover the existing runtime");

    assert_eq!(recovered.version(), "0.2.0");
    assert_eq!(active_version(paths.runtime_pointer()), "0.2.0");
    assert!(!paths.runtime_activation_journal().exists());
}

#[test]
fn two_phase_repair_retry_refuses_pending_rollback_without_running_prepare() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    let payload = payload(base.path(), "0.2.0", b"service-v2");
    installer
        .deploy_with_health_check(&payload, |_| Ok(()))
        .unwrap();
    installer
        .repair_current_with_prepare_and_health_check(
            &payload,
            |_| Ok(()),
            |_, _| {
                Err(SetupError::Runtime(
                    "repair did not become Ready".to_owned(),
                ))
            },
        )
        .unwrap_err();
    let pointer_before = fs::read(paths.runtime_pointer()).unwrap();
    let journal_before = fs::read(paths.runtime_activation_journal()).unwrap();
    let prepare_called = Cell::new(false);

    let error = installer
        .repair_current_with_prepare_and_health_check(
            &payload,
            |_| {
                prepare_called.set(true);
                Ok(())
            },
            |_, _| Ok(()),
        )
        .unwrap_err();

    assert!(error.to_string().contains("pending"));
    assert!(!prepare_called.get());
    assert_eq!(fs::read(paths.runtime_pointer()).unwrap(), pointer_before);
    assert_eq!(
        fs::read(paths.runtime_activation_journal()).unwrap(),
        journal_before
    );
}

#[test]
fn two_phase_repair_refuses_a_pending_repair_journal_without_running_prepare() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    let payload = payload(base.path(), "0.2.0", b"service-v2");
    installer
        .deploy_with_health_check(&payload, |_| Ok(()))
        .unwrap();
    let repair_journal = paths.service_root().join("runtime-repair.json");
    let pending = b"pending repair transaction";
    fs::write(&repair_journal, pending).unwrap();
    let pointer_before = fs::read(paths.runtime_pointer()).unwrap();
    let prepare_called = Cell::new(false);

    let error = installer
        .repair_current_with_prepare_and_health_check(
            &payload,
            |_| {
                prepare_called.set(true);
                Ok(())
            },
            |_, _| Ok(()),
        )
        .unwrap_err();

    assert!(error.to_string().contains("pending runtime repair"));
    assert!(!prepare_called.get());
    assert_eq!(fs::read(paths.runtime_pointer()).unwrap(), pointer_before);
    assert_eq!(fs::read(repair_journal).unwrap(), pending);
}

fn write_runtime_files(root: &std::path::Path, service: &[u8]) {
    fs::create_dir_all(root).unwrap();
    for (name, bytes) in [
        ("mactype-service.exe", service),
        ("mactype-injector32.exe", b"injector-32"),
        ("mactype-injector64.exe", b"injector-64"),
        ("MacType.dll", b"mactype-32"),
        ("MacType64.dll", b"mactype-64"),
    ] {
        fs::write(root.join(name), bytes).unwrap();
    }
}

fn repair_receipt(service: &[u8]) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("mactype-service.exe".to_owned(), sha256_digest(service)),
        (
            "mactype-injector32.exe".to_owned(),
            sha256_digest(b"injector-32"),
        ),
        (
            "mactype-injector64.exe".to_owned(),
            sha256_digest(b"injector-64"),
        ),
        ("MacType.dll".to_owned(), sha256_digest(b"mactype-32")),
        ("MacType64.dll".to_owned(), sha256_digest(b"mactype-64")),
    ])
}

fn write_repair_journal(
    paths: &MachinePaths,
    staging: &str,
    backup: &str,
    phase: &str,
    old_service: &[u8],
    new_service: &[u8],
) {
    fs::write(
        paths.service_root().join("runtime-repair.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema": 2,
            "version": "0.2.0",
            "staging": staging,
            "backup": backup,
            "phase": phase,
            "old_receipt": {"files": repair_receipt(old_service)},
            "new_receipt": {"files": repair_receipt(new_service)},
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn interrupted_repair_after_old_rename_restores_the_old_destination() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-old"), |_| Ok(()))
        .unwrap();
    let destination = paths.runtime_versions().join("0.2.0");
    let staging_name = ".repair-new-0.2.0-test";
    let backup_name = ".repair-old-0.2.0-test";
    fs::rename(&destination, paths.runtime_versions().join(backup_name)).unwrap();
    write_runtime_files(&paths.runtime_versions().join(staging_name), b"service-new");
    write_repair_journal(
        &paths,
        staging_name,
        backup_name,
        "old-moved",
        b"service-old",
        b"service-new",
    );

    installer.recover_interrupted_activation().unwrap();

    assert_eq!(
        fs::read(destination.join("mactype-service.exe")).unwrap(),
        b"service-old"
    );
    assert!(!paths.runtime_versions().join(staging_name).exists());
    assert!(!paths.runtime_versions().join(backup_name).exists());
    assert!(!paths.service_root().join("runtime-repair.json").exists());
}

#[test]
fn interrupted_repair_after_new_rename_keeps_new_and_cleans_old() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-old"), |_| Ok(()))
        .unwrap();
    let destination = paths.runtime_versions().join("0.2.0");
    let staging_name = ".repair-new-0.2.0-test";
    let backup_name = ".repair-old-0.2.0-test";
    fs::rename(&destination, paths.runtime_versions().join(backup_name)).unwrap();
    write_runtime_files(&destination, b"service-new");
    write_repair_journal(
        &paths,
        staging_name,
        backup_name,
        "new-verified",
        b"service-old",
        b"service-new",
    );

    installer.recover_interrupted_activation().unwrap();

    assert_eq!(
        fs::read(destination.join("mactype-service.exe")).unwrap(),
        b"service-new"
    );
    assert!(!paths.runtime_versions().join(backup_name).exists());
    assert!(!paths.service_root().join("runtime-repair.json").exists());
}

#[test]
fn verified_repair_recovers_after_cleanup_was_interrupted_mid_directory() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-old"), |_| Ok(()))
        .unwrap();
    let destination = paths.runtime_versions().join("0.2.0");
    let staging_name = ".repair-new-0.2.0-test";
    let backup_name = ".repair-old-0.2.0-test";
    let backup = paths.runtime_versions().join(backup_name);
    fs::rename(&destination, &backup).unwrap();
    for entry in fs::read_dir(&backup).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() != "MacType64.dll" {
            fs::remove_file(entry.path()).unwrap();
        }
    }
    write_runtime_files(&destination, b"service-new");
    write_repair_journal(
        &paths,
        staging_name,
        backup_name,
        "new-verified",
        b"service-old",
        b"service-new",
    );

    installer.recover_interrupted_activation().unwrap();

    assert_eq!(
        fs::read(destination.join("mactype-service.exe")).unwrap(),
        b"service-new"
    );
    assert!(!backup.exists());
    assert!(!paths.service_root().join("runtime-repair.json").exists());
}

#[test]
fn repair_cleanup_failure_preserves_the_new_active_runtime_and_pending_journal() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-old"), |_| Ok(()))
        .unwrap();
    let destination = paths.runtime_versions().join("0.2.0");
    let staging_name = ".repair-new-0.2.0-test";
    let backup_name = ".repair-old-0.2.0-test";
    let backup = paths.runtime_versions().join(backup_name);
    fs::rename(&destination, &backup).unwrap();
    fs::write(backup.join("unexpected.bin"), b"operator-owned").unwrap();
    write_runtime_files(&destination, b"service-new");
    write_repair_journal(
        &paths,
        staging_name,
        backup_name,
        "new-verified",
        b"service-old",
        b"service-new",
    );

    assert!(installer.recover_interrupted_activation().is_err());
    assert_eq!(
        fs::read(destination.join("mactype-service.exe")).unwrap(),
        b"service-new"
    );
    assert!(backup.join("unexpected.bin").exists());
    assert!(paths.service_root().join("runtime-repair.json").exists());
}

#[test]
fn unverified_repair_destination_is_never_committed_when_rollback_staging_is_missing() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-old"), |_| Ok(()))
        .unwrap();
    let destination = paths.runtime_versions().join("0.2.0");
    let staging_name = ".repair-new-0.2.0-test";
    let backup_name = ".repair-old-0.2.0-test";
    let backup = paths.runtime_versions().join(backup_name);
    fs::rename(&destination, &backup).unwrap();
    write_runtime_files(&destination, b"service-new");
    write_repair_journal(
        &paths,
        staging_name,
        backup_name,
        "new-placed-unverified",
        b"service-old",
        b"service-new",
    );
    fs::write(destination.join("mactype-service.exe"), b"invalid-new").unwrap();

    let recovery = installer.recover_interrupted_activation();

    if recovery.is_ok() {
        assert_eq!(
            fs::read(destination.join("mactype-service.exe")).unwrap(),
            b"service-old"
        );
        assert!(!backup.exists());
        assert!(!paths.service_root().join("runtime-repair.json").exists());
    } else {
        assert_eq!(
            fs::read(backup.join("mactype-service.exe")).unwrap(),
            b"service-old"
        );
        assert!(paths.service_root().join("runtime-repair.json").exists());
    }
}
