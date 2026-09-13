use mactype_service_setup::{
    parse_setup_command, protected_installer_broker_layout, run_install_bootstrap_with,
    run_uninstall_owned_with, BootstrapBlocker, BootstrapMode, BootstrapOutcome,
    BootstrapPreflight, ConflictObservation, InstallBootstrapBackend, OpenServiceObservation,
    ProtectedProfileObservation, ProtectedRuntimeObservation, SetupCommand, SetupError,
    UninstallBackend, UninstallOutcome,
};
use std::path::Path;

struct FakeBackend {
    preflight: BootstrapPreflight,
    applied: Vec<BootstrapMode>,
    forced_digest: Option<String>,
}

impl FakeBackend {
    fn safe_fresh() -> Self {
        Self {
            preflight: BootstrapPreflight {
                open_service: OpenServiceObservation::Absent,
                protected_profile: ProtectedProfileObservation::Absent,
                protected_runtime: ProtectedRuntimeObservation::Absent,
                legacy_service: ConflictObservation::Clear,
                legacy_tray: ConflictObservation::Clear,
                appinit: ConflictObservation::Clear,
            },
            applied: Vec::new(),
            forced_digest: None,
        }
    }
}

impl InstallBootstrapBackend for FakeBackend {
    fn inspect(&mut self) -> BootstrapPreflight {
        self.preflight.clone()
    }

    fn apply_atomically(&mut self, mode: &BootstrapMode) -> Result<String, SetupError> {
        self.applied.push(mode.clone());
        Ok(self.forced_digest.clone().unwrap_or_else(|| match mode {
            BootstrapMode::FreshBundledDefault => {
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned()
            }
            BootstrapMode::PreserveExistingProfile { generation } => {
                format!("sha256:{generation}")
            }
        }))
    }
}

#[test]
fn safe_fresh_install_publishes_the_fixed_default_and_reaches_ready() {
    let mut backend = FakeBackend::safe_fresh();

    let outcome = run_install_bootstrap_with(&mut backend).unwrap();

    assert_eq!(backend.applied, [BootstrapMode::FreshBundledDefault]);
    assert_eq!(
        outcome,
        BootstrapOutcome::Applied {
            active_profile_digest:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            preserved_existing_profile: false,
        }
    );
}

#[test]
fn upgrade_preserves_the_exact_protected_active_profile() {
    let generation = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let expected_digest = format!("sha256:{generation}");
    let mut backend = FakeBackend {
        preflight: BootstrapPreflight {
            open_service: OpenServiceObservation::OwnedRunning,
            protected_profile: ProtectedProfileObservation::Active(generation.to_owned()),
            protected_runtime: ProtectedRuntimeObservation::Active,
            legacy_service: ConflictObservation::Clear,
            legacy_tray: ConflictObservation::Clear,
            appinit: ConflictObservation::Clear,
        },
        applied: Vec::new(),
        forced_digest: None,
    };

    let outcome = run_install_bootstrap_with(&mut backend).unwrap();

    assert_eq!(
        backend.applied,
        [BootstrapMode::PreserveExistingProfile {
            generation: generation.to_owned(),
        }]
    );
    assert_eq!(
        outcome,
        BootstrapOutcome::Applied {
            active_profile_digest: expected_digest,
            preserved_existing_profile: true,
        }
    );
}

#[test]
fn preserved_profile_bootstrap_rejects_a_mismatched_ready_digest() {
    let generation = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let mut backend = FakeBackend {
        preflight: BootstrapPreflight {
            open_service: OpenServiceObservation::OwnedStopped,
            protected_profile: ProtectedProfileObservation::Active(generation.to_owned()),
            protected_runtime: ProtectedRuntimeObservation::Active,
            legacy_service: ConflictObservation::Clear,
            legacy_tray: ConflictObservation::Clear,
            appinit: ConflictObservation::Clear,
        },
        applied: Vec::new(),
        forced_digest: Some(
            "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_owned(),
        ),
    };

    let error = run_install_bootstrap_with(&mut backend).unwrap_err();

    assert!(error.to_string().contains("Ready profile digest mismatch"));
}

#[test]
fn detected_legacy_service_is_a_non_mutating_blocked_skip() {
    let mut backend = FakeBackend::safe_fresh();
    backend.preflight.legacy_service = ConflictObservation::Detected;

    let outcome = run_install_bootstrap_with(&mut backend).unwrap();

    assert!(backend.applied.is_empty());
    assert_eq!(
        outcome,
        BootstrapOutcome::SkippedBlocked {
            reason: BootstrapBlocker::LegacyService,
        }
    );
}

#[test]
fn detected_legacy_tray_mode_is_a_non_mutating_blocked_install() {
    let mut backend = FakeBackend::safe_fresh();
    backend.preflight.legacy_tray = ConflictObservation::Detected;

    let outcome = run_install_bootstrap_with(&mut backend).unwrap();

    assert!(backend.applied.is_empty());
    assert_eq!(
        outcome,
        BootstrapOutcome::SkippedBlocked {
            reason: BootstrapBlocker::LegacyTrayMode,
        }
    );
}

#[test]
fn detected_appinit_conflict_is_a_non_mutating_blocked_skip() {
    let mut backend = FakeBackend::safe_fresh();
    backend.preflight.appinit = ConflictObservation::Detected;

    let outcome = run_install_bootstrap_with(&mut backend).unwrap();

    assert!(backend.applied.is_empty());
    assert_eq!(
        outcome,
        BootstrapOutcome::SkippedBlocked {
            reason: BootstrapBlocker::AppInit,
        }
    );
}

#[test]
fn foreign_open_service_is_a_non_mutating_blocked_skip() {
    let mut backend = FakeBackend::safe_fresh();
    backend.preflight.open_service = OpenServiceObservation::Foreign;

    let outcome = run_install_bootstrap_with(&mut backend).unwrap();

    assert!(backend.applied.is_empty());
    assert_eq!(
        outcome,
        BootstrapOutcome::SkippedBlocked {
            reason: BootstrapBlocker::ForeignOpenService,
        }
    );
}

#[test]
fn every_unknown_preflight_observation_fails_closed_without_mutation() {
    let unknown_cases = [
        BootstrapPreflight {
            open_service: OpenServiceObservation::Unknown,
            ..FakeBackend::safe_fresh().preflight
        },
        BootstrapPreflight {
            protected_profile: ProtectedProfileObservation::Unknown,
            ..FakeBackend::safe_fresh().preflight
        },
        BootstrapPreflight {
            protected_runtime: ProtectedRuntimeObservation::Unknown,
            ..FakeBackend::safe_fresh().preflight
        },
        BootstrapPreflight {
            legacy_service: ConflictObservation::Unknown,
            ..FakeBackend::safe_fresh().preflight
        },
        BootstrapPreflight {
            legacy_tray: ConflictObservation::Unknown,
            ..FakeBackend::safe_fresh().preflight
        },
        BootstrapPreflight {
            appinit: ConflictObservation::Unknown,
            ..FakeBackend::safe_fresh().preflight
        },
    ];

    for preflight in unknown_cases {
        let mut backend = FakeBackend {
            preflight,
            applied: Vec::new(),
            forced_digest: None,
        };
        let outcome = run_install_bootstrap_with(&mut backend).unwrap();
        assert!(backend.applied.is_empty());
        assert_eq!(
            outcome,
            BootstrapOutcome::SkippedBlocked {
                reason: BootstrapBlocker::UnknownMachineState,
            }
        );
    }
}

#[test]
fn inconsistent_owned_service_state_fails_closed_without_mutation() {
    let inconsistent_cases = [
        BootstrapPreflight {
            open_service: OpenServiceObservation::OwnedStopped,
            protected_runtime: ProtectedRuntimeObservation::Absent,
            ..FakeBackend::safe_fresh().preflight
        },
        BootstrapPreflight {
            open_service: OpenServiceObservation::OwnedRunning,
            protected_profile: ProtectedProfileObservation::Absent,
            protected_runtime: ProtectedRuntimeObservation::Active,
            ..FakeBackend::safe_fresh().preflight
        },
    ];

    for preflight in inconsistent_cases {
        let mut backend = FakeBackend {
            preflight,
            applied: Vec::new(),
            forced_digest: None,
        };
        let outcome = run_install_bootstrap_with(&mut backend).unwrap();
        assert!(backend.applied.is_empty());
        assert_eq!(
            outcome,
            BootstrapOutcome::SkippedBlocked {
                reason: BootstrapBlocker::InconsistentOwnedState,
            }
        );
    }
}

#[test]
fn elevated_bootstrap_rejects_a_broker_below_local_app_data() {
    let program_files = Path::new(r"C:\Program Files");

    assert!(!protected_installer_broker_layout(
        program_files,
        Path::new(
            r"C:\Users\person\AppData\Local\Programs\MacType Control Center\service-runtime\mactype-service-setup.exe"
        ),
    ));
    assert!(protected_installer_broker_layout(
        program_files,
        Path::new(
            r"C:\Program Files\MacType Control Center\service-runtime\mactype-service-setup.exe"
        ),
    ));
}

struct FakeUninstallBackend {
    service: OpenServiceObservation,
    remove_calls: usize,
    remove_fails: bool,
    runtime_present: bool,
}

impl UninstallBackend for FakeUninstallBackend {
    fn inspect_open_service(&mut self) -> OpenServiceObservation {
        self.service
    }

    fn remove_owned_installation(
        &mut self,
        observed_service: OpenServiceObservation,
    ) -> Result<bool, SetupError> {
        assert_eq!(observed_service, self.service);
        self.remove_calls += 1;
        if self.remove_fails {
            Err(SetupError::Runtime("simulated removal failure".to_owned()))
        } else {
            Ok(self.runtime_present)
        }
    }
}

#[test]
fn uninstall_leaves_a_foreign_fixed_name_service_unchanged() {
    let mut backend = FakeUninstallBackend {
        service: OpenServiceObservation::Foreign,
        remove_calls: 0,
        remove_fails: false,
        runtime_present: true,
    };

    let outcome = run_uninstall_owned_with(&mut backend).unwrap();

    assert_eq!(backend.remove_calls, 0);
    assert_eq!(
        outcome,
        UninstallOutcome::SkippedBlocked {
            reason: BootstrapBlocker::ForeignOpenService,
        }
    );
}

#[test]
fn uninstall_fails_closed_when_service_identity_cannot_be_observed() {
    let mut backend = FakeUninstallBackend {
        service: OpenServiceObservation::Unknown,
        remove_calls: 0,
        remove_fails: false,
        runtime_present: true,
    };

    let outcome = run_uninstall_owned_with(&mut backend).unwrap();

    assert_eq!(backend.remove_calls, 0);
    assert_eq!(
        outcome,
        UninstallOutcome::SkippedBlocked {
            reason: BootstrapBlocker::UnknownMachineState,
        }
    );
}

#[test]
fn uninstall_stops_and_removes_only_an_owned_open_service() {
    for service in [
        OpenServiceObservation::OwnedStopped,
        OpenServiceObservation::OwnedRunning,
    ] {
        let mut backend = FakeUninstallBackend {
            service,
            remove_calls: 0,
            remove_fails: false,
            runtime_present: true,
        };

        let outcome = run_uninstall_owned_with(&mut backend).unwrap();

        assert_eq!(backend.remove_calls, 1);
        assert_eq!(outcome, UninstallOutcome::Removed);
    }
}

#[test]
fn uninstall_does_not_report_success_when_owned_service_removal_fails() {
    let mut backend = FakeUninstallBackend {
        service: OpenServiceObservation::OwnedRunning,
        remove_calls: 0,
        remove_fails: true,
        runtime_present: true,
    };

    let error = run_uninstall_owned_with(&mut backend).unwrap_err();

    assert_eq!(backend.remove_calls, 1);
    assert!(error.to_string().contains("simulated removal failure"));
    assert!(error
        .to_string()
        .contains("owned installation removal failed"));
}

#[test]
fn uninstall_is_idempotent_when_the_open_service_is_already_absent() {
    let mut backend = FakeUninstallBackend {
        service: OpenServiceObservation::Absent,
        remove_calls: 0,
        remove_fails: false,
        runtime_present: false,
    };

    let outcome = run_uninstall_owned_with(&mut backend).unwrap();

    assert_eq!(backend.remove_calls, 1);
    assert_eq!(outcome, UninstallOutcome::AlreadyAbsent);
}

#[test]
fn uninstall_cleans_a_verified_runtime_orphan_after_the_service_is_already_absent() {
    let mut backend = FakeUninstallBackend {
        service: OpenServiceObservation::Absent,
        remove_calls: 0,
        remove_fails: false,
        runtime_present: true,
    };

    let outcome = run_uninstall_owned_with(&mut backend).unwrap();

    assert_eq!(backend.remove_calls, 1);
    assert_eq!(outcome, UninstallOutcome::Removed);
}

#[test]
fn installer_bootstrap_cli_accepts_only_the_fixed_argument_free_verb() {
    assert_eq!(
        parse_setup_command(["bootstrap-install"]).unwrap(),
        SetupCommand::BootstrapInstall
    );
    assert!(parse_setup_command(["bootstrap-install", r"C:\payload"]).is_err());
    assert!(parse_setup_command(["bootstrap-install=other-service"]).is_err());
}

#[test]
fn installer_uninstall_cli_accepts_only_the_fixed_argument_free_verb() {
    assert_eq!(
        parse_setup_command(["uninstall-owned"]).unwrap(),
        SetupCommand::UninstallOwned
    );
    assert!(parse_setup_command(["uninstall-owned", "MacTypeControlCenter"]).is_err());
    assert!(parse_setup_command(["uninstall-owned=C:\\other.exe"]).is_err());
}

#[test]
fn setup_cli_stops_consuming_arguments_as_soon_as_the_fixed_contract_is_exceeded() {
    struct BoundedArguments {
        yielded: usize,
    }

    impl Iterator for BoundedArguments {
        type Item = &'static str;

        fn next(&mut self) -> Option<Self::Item> {
            self.yielded += 1;
            match self.yielded {
                1 => Some("bootstrap-install"),
                2 => Some("unexpected"),
                3 => Some("also-unexpected"),
                _ => panic!("the parser consumed arguments after the fixed CLI was exceeded"),
            }
        }
    }

    assert!(parse_setup_command(BoundedArguments { yielded: 0 }).is_err());
}
