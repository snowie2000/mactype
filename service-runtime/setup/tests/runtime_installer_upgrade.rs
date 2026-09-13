#![cfg(feature = "ci-test-adapter")]

#[path = "support/runtime_installer/active.rs"]
mod active_support;
#[path = "support/runtime_installer/core.rs"]
mod support;

use std::cell::Cell;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};

use mactype_service_contract::{
    parse_runtime_activation_receipt, RuntimeActivationPhase, SourceMetadata,
};
use mactype_service_setup::{
    FixedPayload, ProfileStore, RuntimeInstaller, RuntimeServiceBinding, SetupError,
};

use active_support::active_version;
use support::{payload, test_paths};
#[test]
fn failed_upgrade_restores_a_runtime_whose_adjacent_profile_matches_active_bytes() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    let profile_bytes = b"[General]\r\nGammaValue=1.4\r\n";
    ProfileStore::new(paths.clone())
        .publish_and_activate(
            profile_bytes,
            SourceMetadata {
                display_name: "upgrade rollback".to_owned(),
            },
        )
        .unwrap();
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();

    let result = installer
        .deploy_with_health_check(&payload(base.path(), "0.3.0", b"service-v3"), |_| {
            Err(SetupError::Runtime("health never became ready".to_owned()))
        });

    assert!(result.is_err());
    assert_eq!(active_version(paths.runtime_pointer()), "0.2.0");
    assert_eq!(
        fs::read(paths.runtime_versions().join("0.2.0").join("MacType.ini")).unwrap(),
        profile_bytes
    );
    assert!(!paths.runtime_activation_journal().exists());
}

#[test]
fn interrupted_upgrade_before_durable_commit_recovers_the_previous_runtime_and_profile() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    let profile_bytes = b"[General]\r\nGammaValue=1.4\r\n";
    ProfileStore::new(paths.clone())
        .publish_and_activate(
            profile_bytes,
            SourceMetadata {
                display_name: "power interruption".to_owned(),
            },
        )
        .unwrap();
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();

    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        let _ = installer.deploy_with_prepare_and_health_check(
            &payload(base.path(), "0.3.0", b"service-v3"),
            |_| -> Result<(), SetupError> {
                panic!("simulated power interruption after runtime pointer activation")
            },
            |_, _| Ok(()),
        );
    }));
    assert!(interrupted.is_err());
    assert_eq!(active_version(paths.runtime_pointer()), "0.3.0");

    let recovered = installer
        .recover_interrupted_activation()
        .unwrap()
        .expect("interrupted upgrade must recover a previous runtime");

    assert_eq!(recovered.version(), "0.2.0");
    assert_eq!(active_version(paths.runtime_pointer()), "0.2.0");
    assert_eq!(
        fs::read(paths.runtime_versions().join("0.2.0").join("MacType.ini")).unwrap(),
        profile_bytes
    );
    assert!(!paths.runtime_activation_journal().exists());
}

#[test]
fn interrupted_prepare_rolls_scm_and_pointer_back_to_the_previous_runtime() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    let profile_bytes = b"[General]\r\nGammaValue=1.4\r\n";
    ProfileStore::new(paths.clone())
        .publish_and_activate(
            profile_bytes,
            SourceMetadata {
                display_name: "prepared SCM candidate".to_owned(),
            },
        )
        .unwrap();
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();

    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        let _ = installer.deploy_with_prepare_and_health_check(
            &payload(base.path(), "0.3.0", b"service-v3"),
            |_| -> Result<(), SetupError> {
                panic!("simulated power interruption after SCM candidate preparation")
            },
            |_, _| Ok(()),
        );
    }));
    assert!(interrupted.is_err());

    let scm_binding = Cell::new(RuntimeServiceBinding::Candidate);
    let recovered = installer
        .recover_interrupted_activation_with_service_binding(
            |candidate, previous| {
                assert_eq!(
                    candidate,
                    Some(
                        paths
                            .runtime_versions()
                            .join("0.3.0")
                            .join("mactype-service.exe")
                            .as_path()
                    )
                );
                assert_eq!(
                    previous,
                    Some(
                        paths
                            .runtime_versions()
                            .join("0.2.0")
                            .join("mactype-service.exe")
                            .as_path()
                    )
                );
                Ok(scm_binding.get())
            },
            |candidate, previous| {
                assert!(candidate.ends_with("0.3.0\\mactype-service.exe"));
                assert!(previous
                    .expect("upgrade rollback requires the previous service image")
                    .ends_with("0.2.0\\mactype-service.exe"));
                scm_binding.set(RuntimeServiceBinding::Previous);
                Ok(())
            },
        )
        .unwrap()
        .expect("prepared SCM candidate must roll back to the previous runtime");

    assert_eq!(recovered.version(), "0.2.0");
    assert_eq!(active_version(paths.runtime_pointer()), "0.2.0");
    assert_eq!(
        fs::read(paths.runtime_versions().join("0.2.0").join("MacType.ini")).unwrap(),
        profile_bytes
    );
    assert!(!paths.runtime_activation_journal().exists());
}

#[test]
fn interrupted_upgrade_after_durable_commit_recovers_the_candidate_runtime_and_profile() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    let profile_bytes = b"[General]\r\nGammaValue=1.4\r\n";
    ProfileStore::new(paths.clone())
        .publish_and_activate(
            profile_bytes,
            SourceMetadata {
                display_name: "post-commit interruption".to_owned(),
            },
        )
        .unwrap();
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();

    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        let _ = installer.deploy_with_prepare_and_health_check(
            &payload(base.path(), "0.3.0", b"service-v3"),
            |_| Ok(()),
            |_, _| -> Result<(), SetupError> {
                panic!("simulated power interruption after durable runtime commit")
            },
        );
    }));
    assert!(interrupted.is_err());
    assert_eq!(active_version(paths.runtime_pointer()), "0.3.0");

    let recovered = installer
        .recover_interrupted_activation_with_service_binding(
            |_, _| Ok(RuntimeServiceBinding::Candidate),
            |_, _| panic!("committed recovery must never roll back the SCM image"),
        )
        .unwrap()
        .expect("committed upgrade must recover the candidate runtime");

    assert_eq!(recovered.version(), "0.3.0");
    assert_eq!(active_version(paths.runtime_pointer()), "0.3.0");
    assert_eq!(
        fs::read(paths.runtime_versions().join("0.3.0").join("MacType.ini")).unwrap(),
        profile_bytes
    );
    assert!(!paths.runtime_activation_journal().exists());
}

#[test]
fn invalid_manifest_never_changes_the_active_runtime() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    let first = payload(base.path(), "0.2.0", b"service-v2");
    installer
        .deploy_with_health_check(&first, |_| Ok(()))
        .unwrap();

    let invalid_root = base.path().join("payload-invalid");
    fs::create_dir_all(invalid_root.join("files")).unwrap();
    fs::write(
        invalid_root.join("files").join("mactype-service.exe"),
        b"tampered",
    )
    .unwrap();
    fs::write(
        invalid_root.join("manifest.json"),
        br#"{"schema":1,"version":"0.3.0","files":{"mactype-service.exe":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}"#,
    )
    .unwrap();
    let invalid = FixedPayload::from_test_root(invalid_root).unwrap();
    assert!(installer
        .deploy_with_health_check(&invalid, |_| Ok(()))
        .is_err());
    assert_eq!(active_version(paths.runtime_pointer()), "0.2.0");
}

#[test]
fn two_phase_health_failure_keeps_rollback_receipt_until_service_binding_is_restored() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();

    let result = installer.deploy_with_prepare_and_health_check(
        &payload(base.path(), "0.3.0", b"service-v3"),
        |_| Ok(()),
        |_, _| {
            Err(SetupError::Runtime(
                "candidate did not become Ready".to_owned(),
            ))
        },
    );

    assert!(result.is_err());
    assert_eq!(active_version(paths.runtime_pointer()), "0.3.0");
    let receipt =
        parse_runtime_activation_receipt(&fs::read(paths.runtime_activation_journal()).unwrap())
            .unwrap();
    assert_eq!(
        receipt.phase(),
        Some(RuntimeActivationPhase::RollbackRequired)
    );

    let binding = Cell::new(RuntimeServiceBinding::Candidate);
    let recovered = installer
        .recover_interrupted_activation_with_service_binding(
            |_, _| Ok(binding.get()),
            |_, previous| {
                assert!(previous.is_some());
                binding.set(RuntimeServiceBinding::Previous);
                Ok(())
            },
        )
        .unwrap()
        .expect("exact external rollback must restore the previous runtime");

    assert_eq!(recovered.version(), "0.2.0");
    assert_eq!(active_version(paths.runtime_pointer()), "0.2.0");
    assert!(!paths.runtime_activation_journal().exists());
}

#[test]
fn two_phase_retry_refuses_pending_rollback_without_running_prepare() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();
    let candidate_payload = payload(base.path(), "0.3.0", b"service-v3");
    installer
        .deploy_with_prepare_and_health_check(
            &candidate_payload,
            |_| Ok(()),
            |_, _| {
                Err(SetupError::Runtime(
                    "candidate did not become Ready".to_owned(),
                ))
            },
        )
        .unwrap_err();
    let pointer_before = fs::read(paths.runtime_pointer()).unwrap();
    let journal_before = fs::read(paths.runtime_activation_journal()).unwrap();
    let prepare_called = Cell::new(false);

    let error = installer
        .deploy_with_prepare_and_health_check(
            &candidate_payload,
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
