use super::super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
struct FakeOpenServiceSnapshot {
    installed: bool,
    running: bool,
}

struct FakeMigrationBackend {
    operations: Vec<&'static str>,
    backup_valid: bool,
    verification: MigrationVerification,
    fail_at: Option<&'static str>,
    legacy_stopped: bool,
    legacy_removed: bool,
    open_service_installed: bool,
}

impl Default for FakeMigrationBackend {
    fn default() -> Self {
        Self {
            operations: Vec::new(),
            backup_valid: true,
            verification: MigrationVerification::fully_verified(),
            fail_at: None,
            legacy_stopped: false,
            legacy_removed: false,
            open_service_installed: false,
        }
    }
}

impl FakeMigrationBackend {
    fn operation(&mut self, name: &'static str) -> Result<(), String> {
        self.operations.push(name);
        if self.fail_at == Some(name) {
            Err(format!("{name} failed"))
        } else {
            Ok(())
        }
    }
}

impl MigrationBackend for FakeMigrationBackend {
    type OpenServiceSnapshot = FakeOpenServiceSnapshot;

    fn prepare_legacy_backup(&mut self) -> Result<(), String> {
        self.operation("prepare-backup")
    }

    fn legacy_backup_is_valid(&mut self) -> bool {
        self.operations.push("validate-backup");
        self.backup_valid
    }

    fn capture_open_service(&mut self) -> Result<Self::OpenServiceSnapshot, String> {
        self.operation("capture-open-service")?;
        Ok(FakeOpenServiceSnapshot {
            installed: false,
            running: false,
        })
    }

    fn stop_legacy(&mut self) -> Result<(), String> {
        self.operation("stop-legacy")?;
        self.legacy_stopped = true;
        Ok(())
    }

    fn publish_profile(
        &mut self,
        _snapshot: &Self::OpenServiceSnapshot,
        _profile: &[u8],
    ) -> Result<(), String> {
        self.operation("publish-profile")
    }

    fn activate_open_service(
        &mut self,
        _snapshot: &Self::OpenServiceSnapshot,
    ) -> Result<(), String> {
        self.operation("activate-open-service")?;
        self.open_service_installed = true;
        Ok(())
    }

    fn strict_ready(&mut self, _expected_digest: &str) -> Result<bool, String> {
        self.operation("verify-ready")?;
        Ok(self.verification.scm_running_ready && self.verification.active_digest_match)
    }

    fn verify_injection_smoke(&mut self, _expected_digest: &str) -> Result<bool, String> {
        self.operation("verify-injection-smoke")?;
        Ok(self.verification.telemetry_verified)
    }

    fn complete_migration(&mut self, _snapshot: &Self::OpenServiceSnapshot) -> Result<(), String> {
        self.operation("complete-migration")
    }

    fn rollback_open_service(
        &mut self,
        snapshot: &Self::OpenServiceSnapshot,
    ) -> Result<(), String> {
        self.operation("rollback-open-service")?;
        self.open_service_installed = snapshot.installed;
        Ok(())
    }

    fn restore_legacy(&mut self) -> Result<(), String> {
        self.operation("restore-legacy")?;
        self.legacy_stopped = false;
        self.legacy_removed = false;
        Ok(())
    }

    fn removal_verification(
        &mut self,
        _expected_digest: &str,
    ) -> Result<MigrationVerification, String> {
        self.operation("verify-removal")?;
        Ok(self.verification)
    }

    fn remove_legacy(&mut self) -> Result<(), String> {
        self.operation("remove-legacy")?;
        self.legacy_removed = true;
        Ok(())
    }
}

#[test]
fn migration_stops_but_never_removes_the_legacy_service() {
    let mut backend = FakeMigrationBackend::default();

    migrate_from_legacy(&mut backend, b"[General]\r\nGammaMode=0\r\n").unwrap();

    assert_eq!(
        backend.operations,
        [
            "prepare-backup",
            "validate-backup",
            "capture-open-service",
            "stop-legacy",
            "publish-profile",
            "activate-open-service",
            "verify-ready",
            "verify-injection-smoke",
            "complete-migration",
        ]
    );
    assert!(backend.legacy_stopped);
    assert!(!backend.legacy_removed);
}

#[test]
fn migration_failure_rolls_back_open_service_and_restores_legacy_state() {
    let mut backend = FakeMigrationBackend {
        fail_at: Some("publish-profile"),
        ..FakeMigrationBackend::default()
    };

    let error = migrate_from_legacy(&mut backend, b"profile").unwrap_err();

    assert!(error.contains("publish active profile"));
    assert_eq!(
        backend.operations,
        [
            "prepare-backup",
            "validate-backup",
            "capture-open-service",
            "stop-legacy",
            "publish-profile",
            "rollback-open-service",
            "restore-legacy",
        ]
    );
    assert!(!backend.legacy_stopped);
    assert!(!backend.legacy_removed);
}

#[test]
fn legacy_removal_revalidates_backup_health_digest_and_telemetry() {
    let mut backend = FakeMigrationBackend {
        legacy_stopped: true,
        ..FakeMigrationBackend::default()
    };

    remove_legacy_after_verification(&mut backend, b"profile").unwrap();

    assert_eq!(
        backend.operations,
        ["validate-backup", "verify-removal", "remove-legacy"]
    );
    assert!(backend.legacy_removed);
}

#[test]
fn failed_legacy_removal_leaves_the_legacy_service_stopped_without_restarting_it() {
    // Restarting the legacy service on a removal failure would double-inject
    // against the already-live new service, so removal failure must leave the
    // legacy service stopped and disabled rather than restore (restart) it.
    let mut backend = FakeMigrationBackend {
        fail_at: Some("remove-legacy"),
        legacy_stopped: true,
        ..FakeMigrationBackend::default()
    };

    let error = remove_legacy_after_verification(&mut backend, b"profile").unwrap_err();

    assert!(error.contains("remove legacy service"));
    assert!(error.contains("stopped and disabled"));
    assert_eq!(
        backend.operations,
        ["validate-backup", "verify-removal", "remove-legacy"]
    );
    assert!(
        backend.legacy_stopped,
        "the legacy service must stay stopped, never restarted, on removal failure"
    );
    assert!(!backend.legacy_removed);
}

#[test]
fn failed_first_install_removes_the_new_open_service_residue() {
    let mut backend = FakeMigrationBackend {
        fail_at: Some("verify-ready"),
        ..FakeMigrationBackend::default()
    };

    migrate_from_legacy(&mut backend, b"profile").unwrap_err();

    assert!(!backend.open_service_installed);
    assert_eq!(
        &backend.operations[backend.operations.len() - 2..],
        ["rollback-open-service", "restore-legacy"]
    );
}

#[test]
fn every_mutating_migration_stage_failure_runs_both_rollbacks() {
    for stage in [
        "stop-legacy",
        "publish-profile",
        "activate-open-service",
        "verify-ready",
        "verify-injection-smoke",
        "complete-migration",
    ] {
        let mut backend = FakeMigrationBackend {
            fail_at: Some(stage),
            ..FakeMigrationBackend::default()
        };

        let error = migrate_from_legacy(&mut backend, b"profile").unwrap_err();

        assert!(error.contains("failed"), "missing stage error for {stage}");
        assert_eq!(
            &backend.operations[backend.operations.len() - 2..],
            ["rollback-open-service", "restore-legacy"],
            "missing rollback after {stage}"
        );
    }
}

#[test]
fn false_ready_or_profile_digest_rolls_back_migration() {
    for verification in [
        MigrationVerification {
            scm_running_ready: false,
            ..MigrationVerification::fully_verified()
        },
        MigrationVerification {
            active_digest_match: false,
            ..MigrationVerification::fully_verified()
        },
    ] {
        let mut backend = FakeMigrationBackend {
            verification,
            ..FakeMigrationBackend::default()
        };

        let error = migrate_from_legacy(&mut backend, b"profile").unwrap_err();

        assert!(error.contains("strict Ready or profile digest mismatch"));
        assert_eq!(
            &backend.operations[backend.operations.len() - 2..],
            ["rollback-open-service", "restore-legacy"]
        );
    }
}

#[test]
fn x86_only_or_other_partial_telemetry_never_permits_legacy_removal() {
    let mut backend = FakeMigrationBackend {
        verification: MigrationVerification {
            telemetry_verified: false,
            ..MigrationVerification::fully_verified()
        },
        legacy_stopped: true,
        ..FakeMigrationBackend::default()
    };

    let error = remove_legacy_after_verification(&mut backend, b"profile").unwrap_err();

    assert!(error.contains("x86/x64 telemetry"));
    assert_eq!(backend.operations, ["validate-backup", "verify-removal"]);
    assert!(!backend.legacy_removed);
}

#[test]
fn invalid_or_reparse_backed_receipt_blocks_removal_before_health_is_read() {
    let mut backend = FakeMigrationBackend {
        backup_valid: false,
        legacy_stopped: true,
        ..FakeMigrationBackend::default()
    };

    let error = remove_legacy_after_verification(&mut backend, b"profile").unwrap_err();

    assert!(error.contains("backup receipt is invalid"));
    assert_eq!(backend.operations, ["validate-backup"]);
    assert!(!backend.legacy_removed);
}

#[test]
fn migration_without_both_architecture_smoke_rolls_back() {
    let mut backend = FakeMigrationBackend {
        verification: MigrationVerification {
            telemetry_verified: false,
            ..MigrationVerification::fully_verified()
        },
        ..FakeMigrationBackend::default()
    };

    let error = migrate_from_legacy(&mut backend, b"profile").unwrap_err();

    assert!(error.contains("x86/x64 injection smoke"));
    assert_eq!(
        &backend.operations[backend.operations.len() - 3..],
        [
            "verify-injection-smoke",
            "rollback-open-service",
            "restore-legacy"
        ]
    );
}
