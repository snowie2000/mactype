#![cfg(feature = "ci-test-adapter")]

#[path = "support/runtime_installer/active.rs"]
mod active_support;
#[path = "support/runtime_installer/core.rs"]
mod support;

use std::cell::Cell;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};

use mactype_service_contract::{
    parse_runtime_activation_receipt, RuntimeActivationPhase, RuntimeActivationReceipt,
    RuntimeGenerationPointer,
};
use mactype_service_setup::{RuntimeInstaller, RuntimeServiceBinding, SetupError};

use active_support::active_version;
use support::{payload, test_paths};
#[test]
fn failed_health_check_restores_the_previous_runtime_pointer() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    let first = payload(base.path(), "0.2.0", b"service-v2");
    installer
        .deploy_with_health_check(&first, |_| Ok(()))
        .unwrap();

    let second = payload(base.path(), "0.3.0", b"service-v3");
    let result = installer.deploy_with_health_check(&second, |_| {
        Err(SetupError::Runtime("health never became ready".to_owned()))
    });
    let message = result.unwrap_err().to_string();
    assert!(message.contains("run runtime activation health check"));
    assert!(message.contains("mactype-service.exe"));
    assert!(message.contains("health never became ready"));
    assert_eq!(active_version(paths.runtime_pointer()), "0.2.0");
}

#[test]
fn interrupted_after_durable_commit_finalizes_the_exact_owned_candidate() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        let _ = installer
            .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| {
                panic!("simulated interruption after first runtime pointer activation")
            });
    }));
    assert!(interrupted.is_err());
    assert!(paths.runtime_pointer().is_file());
    assert!(paths.runtime_activation_journal().is_file());

    let recovered = installer
        .recover_interrupted_activation()
        .unwrap()
        .expect("a durably committed candidate must remain active");

    assert_eq!(recovered.version(), "0.2.0");
    assert_eq!(active_version(paths.runtime_pointer()), "0.2.0");
    assert!(!paths.runtime_activation_journal().exists());
}

#[test]
fn interrupted_uncommitted_candidate_rolls_back_to_the_previous_runtime() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();
    let previous = RuntimeGenerationPointer::new("0.2.0").unwrap();
    let candidate = RuntimeGenerationPointer::new("0.3.0").unwrap();
    fs::write(paths.runtime_pointer(), candidate.to_bytes().unwrap()).unwrap();
    fs::write(
        paths.runtime_activation_journal(),
        RuntimeActivationReceipt::candidate(Some(previous), candidate)
            .to_bytes()
            .unwrap(),
    )
    .unwrap();

    let recovered = installer
        .recover_interrupted_activation()
        .unwrap()
        .expect("the previous runtime must be restored");

    assert_eq!(recovered.version(), "0.2.0");
    assert_eq!(active_version(paths.runtime_pointer()), "0.2.0");
    assert!(!paths.runtime_activation_journal().exists());
}

#[test]
fn interrupted_first_activation_preserves_a_later_foreign_pointer_and_journal() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        let _ = installer
            .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| {
                panic!("simulated interruption after first runtime pointer activation")
            });
    }));
    assert!(interrupted.is_err());
    let foreign = br#"{"schema":1,"version":"9.9.9"}"#;
    fs::write(paths.runtime_pointer(), foreign).unwrap();

    let error = installer.recover_interrupted_activation().unwrap_err();

    assert!(matches!(error, SetupError::CleanupUnknown(_)));
    assert_eq!(fs::read(paths.runtime_pointer()).unwrap(), foreign);
    assert!(paths.runtime_activation_journal().is_file());
}

#[test]
fn interrupted_first_activation_preserves_a_later_non_regular_pointer_and_journal() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        let _ = installer
            .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| {
                panic!("simulated interruption after first runtime pointer activation")
            });
    }));
    assert!(interrupted.is_err());
    fs::remove_file(paths.runtime_pointer()).unwrap();
    fs::create_dir(paths.runtime_pointer()).unwrap();

    let error = installer.recover_interrupted_activation().unwrap_err();

    assert!(matches!(error, SetupError::CleanupUnknown(_)));
    assert!(paths.runtime_pointer().is_dir());
    assert!(paths.runtime_activation_journal().is_file());
}

#[test]
fn committed_recovery_verifies_a_previous_pointer_before_finishing_rollback() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();
    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        let _ = installer
            .deploy_with_health_check(&payload(base.path(), "0.3.0", b"service-v3"), |_| {
                panic!("simulated interruption after upgrade commit")
            });
    }));
    assert!(interrupted.is_err());
    fs::write(
        paths.runtime_pointer(),
        RuntimeGenerationPointer::new("0.2.0")
            .unwrap()
            .to_bytes()
            .unwrap(),
    )
    .unwrap();
    fs::write(
        paths
            .runtime_versions()
            .join("0.2.0")
            .join("mactype-service.exe"),
        b"tampered-previous",
    )
    .unwrap();

    let error = installer.recover_interrupted_activation().unwrap_err();

    assert!(matches!(error, SetupError::CleanupUnknown(_)));
    assert_eq!(active_version(paths.runtime_pointer()), "0.2.0");
    assert!(paths.runtime_activation_journal().is_file());
}

#[test]
fn activation_failure_reports_operation_and_pointer_rollback_failure_and_keeps_journal() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    let foreign = br#"{"schema":1,"version":"9.9.9"}"#.to_vec();
    let result =
        installer.deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| {
            fs::write(paths.runtime_pointer(), &foreign).unwrap();
            Err(SetupError::Runtime("primary health failure".to_owned()))
        });

    let message = result.unwrap_err().to_string();

    assert!(message.contains("primary health failure"));
    assert!(message.contains("rollback"));
    assert_eq!(fs::read(paths.runtime_pointer()).unwrap(), foreign);
    assert!(paths.runtime_activation_journal().is_file());
    let receipt =
        parse_runtime_activation_receipt(&fs::read(paths.runtime_activation_journal()).unwrap())
            .unwrap();
    assert_eq!(
        receipt.phase(),
        Some(RuntimeActivationPhase::RollbackRequired)
    );

    let recovery = installer.recover_interrupted_activation().unwrap_err();
    assert!(matches!(recovery, SetupError::CleanupUnknown(_)));
    assert_eq!(fs::read(paths.runtime_pointer()).unwrap(), foreign);
    assert!(paths.runtime_activation_journal().is_file());
}

#[test]
fn committed_recovery_rejects_previous_and_absent_service_bindings() {
    for binding in [
        RuntimeServiceBinding::Previous,
        RuntimeServiceBinding::Absent,
    ] {
        let (base, paths) = test_paths();
        let installer = RuntimeInstaller::new(paths.clone());
        installer
            .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
            .unwrap();
        let interrupted = catch_unwind(AssertUnwindSafe(|| {
            let _ = installer
                .deploy_with_health_check(&payload(base.path(), "0.3.0", b"service-v3"), |_| {
                    panic!("interrupted after durable commit")
                });
        }));
        assert!(interrupted.is_err());

        let error = installer
            .recover_interrupted_activation_with_service_binding(
                |_, _| Ok(binding),
                |_, _| panic!("committed recovery must not invoke rollback"),
            )
            .unwrap_err();

        assert!(matches!(error, SetupError::CleanupUnknown(_)));
        assert_eq!(active_version(paths.runtime_pointer()), "0.3.0");
        assert!(paths.runtime_activation_journal().is_file());
    }
}

#[test]
fn candidate_recovery_rejects_unknown_and_foreign_service_bindings() {
    for failure in ["unknown service state", "foreign service configuration"] {
        let (base, paths) = test_paths();
        let installer = RuntimeInstaller::new(paths.clone());
        installer
            .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
            .unwrap();
        let interrupted = catch_unwind(AssertUnwindSafe(|| {
            let _ = installer.deploy_with_prepare_and_health_check(
                &payload(base.path(), "0.3.0", b"service-v3"),
                |_| -> Result<(), SetupError> { panic!("interrupted during prepare") },
                |_, _| Ok(()),
            );
        }));
        assert!(interrupted.is_err());

        let error = installer
            .recover_interrupted_activation_with_service_binding(
                |_, _| Err(SetupError::Runtime(failure.to_owned())),
                |_, _| panic!("unclassified service binding must not be mutated"),
            )
            .unwrap_err();

        assert!(matches!(error, SetupError::CleanupUnknown(_)));
        assert_eq!(active_version(paths.runtime_pointer()), "0.3.0");
        assert!(paths.runtime_activation_journal().is_file());
    }
}

#[test]
fn rollback_required_callback_failure_preserves_pointer_and_journal() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();
    let previous = RuntimeGenerationPointer::new("0.2.0").unwrap();
    let candidate = RuntimeGenerationPointer::new("0.3.0").unwrap();
    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        let _ = installer.deploy_with_prepare_and_health_check(
            &payload(base.path(), "0.3.0", b"service-v3"),
            |_| -> Result<(), SetupError> { panic!("interrupted during prepare") },
            |_, _| Ok(()),
        );
    }));
    assert!(interrupted.is_err());
    fs::write(
        paths.runtime_activation_journal(),
        RuntimeActivationReceipt::candidate(Some(previous), candidate)
            .with_phase(RuntimeActivationPhase::RollbackRequired)
            .to_bytes()
            .unwrap(),
    )
    .unwrap();

    let error = installer
        .recover_interrupted_activation_with_service_binding(
            |_, _| Ok(RuntimeServiceBinding::Candidate),
            |_, _| Err(SetupError::Runtime("SCM rollback failed".to_owned())),
        )
        .unwrap_err();

    assert!(matches!(error, SetupError::CleanupUnknown(_)));
    assert_eq!(active_version(paths.runtime_pointer()), "0.3.0");
    assert!(paths.runtime_activation_journal().is_file());
    let receipt =
        parse_runtime_activation_receipt(&fs::read(paths.runtime_activation_journal()).unwrap())
            .unwrap();
    assert_eq!(
        receipt.phase(),
        Some(RuntimeActivationPhase::RollbackRequired)
    );
}

#[test]
fn fresh_candidate_recovery_requires_exact_service_removal_before_pointer_cleanup() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        let _ = installer.deploy_with_prepare_and_health_check(
            &payload(base.path(), "0.2.0", b"service-v2"),
            |_| -> Result<(), SetupError> { panic!("interrupted after fresh service prepare") },
            |_, _| Ok(()),
        );
    }));
    assert!(interrupted.is_err());
    let binding = Cell::new(RuntimeServiceBinding::Candidate);

    let recovered = installer
        .recover_interrupted_activation_with_service_binding(
            |_, previous| {
                assert!(previous.is_none());
                Ok(binding.get())
            },
            |_, previous| {
                assert!(previous.is_none());
                binding.set(RuntimeServiceBinding::Absent);
                Ok(())
            },
        )
        .unwrap();

    assert!(recovered.is_none());
    assert!(!paths.runtime_pointer().exists());
    assert!(!paths.runtime_activation_journal().exists());
}

#[test]
fn schema_one_recovery_derives_the_exact_candidate_from_the_active_pointer() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();
    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        let _ = installer.deploy_with_prepare_and_health_check(
            &payload(base.path(), "0.3.0", b"service-v3"),
            |_| -> Result<(), SetupError> { panic!("interrupted legacy prepare") },
            |_, _| Ok(()),
        );
    }));
    assert!(interrupted.is_err());
    fs::write(
        paths.runtime_activation_journal(),
        br#"{"schema":1,"previous":{"schema":1,"version":"0.2.0"}}"#,
    )
    .unwrap();
    let binding = Cell::new(RuntimeServiceBinding::Candidate);

    let recovered = installer
        .recover_interrupted_activation_with_service_binding(
            |candidate, previous| {
                assert!(candidate
                    .expect("legacy post-switch receipt must derive a candidate")
                    .ends_with("0.3.0\\mactype-service.exe"));
                assert!(previous
                    .expect("legacy upgrade receipt retains its previous pointer")
                    .ends_with("0.2.0\\mactype-service.exe"));
                Ok(binding.get())
            },
            |_, _| {
                binding.set(RuntimeServiceBinding::Previous);
                Ok(())
            },
        )
        .unwrap()
        .expect("legacy activation must recover the previous runtime");

    assert_eq!(recovered.version(), "0.2.0");
    assert_eq!(active_version(paths.runtime_pointer()), "0.2.0");
    assert!(!paths.runtime_activation_journal().exists());
}

#[test]
fn schema_one_before_pointer_switch_accepts_only_the_exact_previous_binding() {
    let (base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    installer
        .deploy_with_health_check(&payload(base.path(), "0.2.0", b"service-v2"), |_| Ok(()))
        .unwrap();
    fs::write(
        paths.runtime_activation_journal(),
        br#"{"schema":1,"previous":{"schema":1,"version":"0.2.0"}}"#,
    )
    .unwrap();

    let recovered = installer
        .recover_interrupted_activation_with_service_binding(
            |candidate, previous| {
                assert!(candidate.is_none());
                assert!(previous
                    .expect("legacy pre-switch upgrade retains the previous binding")
                    .ends_with("0.2.0\\mactype-service.exe"));
                Ok(RuntimeServiceBinding::Previous)
            },
            |_, _| panic!("an exact previous binding needs no mutation"),
        )
        .unwrap()
        .expect("legacy pre-switch upgrade must retain the previous runtime");

    assert_eq!(recovered.version(), "0.2.0");
    assert_eq!(active_version(paths.runtime_pointer()), "0.2.0");
    assert!(!paths.runtime_activation_journal().exists());
}

#[test]
fn schema_one_before_fresh_pointer_switch_accepts_only_an_absent_service() {
    let (_base, paths) = test_paths();
    let installer = RuntimeInstaller::new(paths.clone());
    fs::create_dir_all(paths.service_root()).unwrap();
    fs::write(
        paths.runtime_activation_journal(),
        br#"{"schema":1,"previous":null}"#,
    )
    .unwrap();

    let recovered = installer
        .recover_interrupted_activation_with_service_binding(
            |candidate, previous| {
                assert!(candidate.is_none());
                assert!(previous.is_none());
                Ok(RuntimeServiceBinding::Absent)
            },
            |_, _| panic!("an absent fresh service needs no mutation"),
        )
        .unwrap();

    assert!(recovered.is_none());
    assert!(!paths.runtime_pointer().exists());
    assert!(!paths.runtime_activation_journal().exists());
}
