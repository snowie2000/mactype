use mactype_service_setup::{
    BootstrapBlocker, BootstrapOutcome, BootstrapPlan, BootstrapPreflight, BootstrapProfileMode,
    BootstrapStartPolicy, ConflictObservation, InstallBootstrapBackend, OpenServiceObservation,
    ProtectedProfileObservation, ProtectedRuntimeObservation, SetupCommand, SetupError,
    UninstallBackend, UninstallOutcome, parse_setup_command, protected_installer_broker_layout,
    run_install_bootstrap_with, run_uninstall_owned_with,
};
use std::path::Path;

const DEFAULT_DIGEST: &str =
    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const EXISTING_GENERATION: &str =
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

struct FakeBackend {
    preflight: BootstrapPreflight,
    applied: Vec<BootstrapPlan>,
    forced_digest: Option<Option<String>>,
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

    fn owned(service: OpenServiceObservation, profile: ProtectedProfileObservation) -> Self {
        Self {
            preflight: BootstrapPreflight {
                open_service: service,
                protected_profile: profile,
                protected_runtime: ProtectedRuntimeObservation::Active,
                ..Self::safe_fresh().preflight
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

    fn apply_atomically(&mut self, plan: &BootstrapPlan) -> Result<Option<String>, SetupError> {
        self.applied.push(plan.clone());
        Ok(self
            .forced_digest
            .clone()
            .unwrap_or_else(|| match &plan.profile {
                BootstrapProfileMode::PublishBundledDefault => Some(DEFAULT_DIGEST.to_owned()),
                BootstrapProfileMode::PreserveExisting { generation } => {
                    Some(format!("sha256:{generation}"))
                }
                BootstrapProfileMode::LeaveUnpublished => None,
            }))
    }
}

fn plan(profile: BootstrapProfileMode, start_service: bool) -> BootstrapPlan {
    BootstrapPlan {
        profile,
        start_service,
    }
}

fn existing() -> ProtectedProfileObservation {
    ProtectedProfileObservation::Active(EXISTING_GENERATION.to_owned())
}

#[test]
fn safe_fresh_install_publishes_the_fixed_default_and_reaches_ready() {
    let mut backend = FakeBackend::safe_fresh();

    let outcome =
        run_install_bootstrap_with(&mut backend, BootstrapStartPolicy::EnsureRunning).unwrap();

    assert_eq!(
        backend.applied,
        [plan(BootstrapProfileMode::PublishBundledDefault, true)]
    );
    assert_eq!(
        outcome,
        BootstrapOutcome::Applied {
            active_profile_digest: Some(DEFAULT_DIGEST.to_owned()),
            preserved_existing_profile: false,
            service_started: true,
        }
    );
}

#[test]
fn preserving_fresh_install_registers_the_service_without_a_profile_or_a_start() {
    let mut backend = FakeBackend::safe_fresh();

    let outcome =
        run_install_bootstrap_with(&mut backend, BootstrapStartPolicy::PreserveObservedState)
            .unwrap();

    assert_eq!(
        backend.applied,
        [plan(BootstrapProfileMode::LeaveUnpublished, false)]
    );
    assert_eq!(
        outcome,
        BootstrapOutcome::Applied {
            active_profile_digest: None,
            preserved_existing_profile: false,
            service_started: false,
        }
    );
}

#[test]
fn preserving_update_leaves_a_stopped_service_stopped_with_its_profile() {
    let mut backend = FakeBackend::owned(OpenServiceObservation::OwnedStopped, existing());

    let outcome =
        run_install_bootstrap_with(&mut backend, BootstrapStartPolicy::PreserveObservedState)
            .unwrap();

    assert_eq!(
        backend.applied,
        [plan(
            BootstrapProfileMode::PreserveExisting {
                generation: EXISTING_GENERATION.to_owned(),
            },
            false,
        )]
    );
    assert_eq!(
        outcome,
        BootstrapOutcome::Applied {
            active_profile_digest: Some(format!("sha256:{EXISTING_GENERATION}")),
            preserved_existing_profile: true,
            service_started: false,
        }
    );
}

#[test]
fn preserving_update_restarts_a_running_service_and_verifies_ready() {
    let mut backend = FakeBackend::owned(OpenServiceObservation::OwnedRunning, existing());

    let outcome =
        run_install_bootstrap_with(&mut backend, BootstrapStartPolicy::PreserveObservedState)
            .unwrap();

    assert_eq!(
        backend.applied,
        [plan(
            BootstrapProfileMode::PreserveExisting {
                generation: EXISTING_GENERATION.to_owned(),
            },
            true,
        )]
    );
    assert_eq!(
        outcome,
        BootstrapOutcome::Applied {
            active_profile_digest: Some(format!("sha256:{EXISTING_GENERATION}")),
            preserved_existing_profile: true,
            service_started: true,
        }
    );
}

#[test]
fn preserving_update_of_a_never_started_install_keeps_the_profile_unpublished() {
    let mut backend = FakeBackend::owned(
        OpenServiceObservation::OwnedStopped,
        ProtectedProfileObservation::Absent,
    );

    let outcome =
        run_install_bootstrap_with(&mut backend, BootstrapStartPolicy::PreserveObservedState)
            .unwrap();

    assert_eq!(
        backend.applied,
        [plan(BootstrapProfileMode::LeaveUnpublished, false)]
    );
    assert_eq!(
        outcome,
        BootstrapOutcome::Applied {
            active_profile_digest: None,
            preserved_existing_profile: false,
            service_started: false,
        }
    );
}

#[test]
fn ensuring_a_never_started_install_publishes_the_default_and_starts() {
    let mut backend = FakeBackend::owned(
        OpenServiceObservation::OwnedStopped,
        ProtectedProfileObservation::Absent,
    );

    let outcome =
        run_install_bootstrap_with(&mut backend, BootstrapStartPolicy::EnsureRunning).unwrap();

    assert_eq!(
        backend.applied,
        [plan(BootstrapProfileMode::PublishBundledDefault, true)]
    );
    assert_eq!(
        outcome,
        BootstrapOutcome::Applied {
            active_profile_digest: Some(DEFAULT_DIGEST.to_owned()),
            preserved_existing_profile: false,
            service_started: true,
        }
    );
}

#[test]
fn upgrade_preserves_the_exact_protected_active_profile() {
    let mut backend = FakeBackend::owned(OpenServiceObservation::OwnedRunning, existing());

    let outcome =
        run_install_bootstrap_with(&mut backend, BootstrapStartPolicy::EnsureRunning).unwrap();

    assert_eq!(
        backend.applied,
        [plan(
            BootstrapProfileMode::PreserveExisting {
                generation: EXISTING_GENERATION.to_owned(),
            },
            true,
        )]
    );
    assert_eq!(
        outcome,
        BootstrapOutcome::Applied {
            active_profile_digest: Some(format!("sha256:{EXISTING_GENERATION}")),
            preserved_existing_profile: true,
            service_started: true,
        }
    );
}

#[test]
fn preserved_profile_bootstrap_rejects_a_mismatched_ready_digest() {
    for policy in [
        BootstrapStartPolicy::EnsureRunning,
        BootstrapStartPolicy::PreserveObservedState,
    ] {
        let mut backend = FakeBackend::owned(OpenServiceObservation::OwnedStopped, existing());
        backend.forced_digest = Some(Some(
            "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_owned(),
        ));

        let error = run_install_bootstrap_with(&mut backend, policy).unwrap_err();

        assert!(error.to_string().contains("Ready profile digest mismatch"));
    }
}

#[test]
fn an_unpublished_bootstrap_must_not_report_a_digest() {
    let mut backend = FakeBackend::safe_fresh();
    backend.forced_digest = Some(Some(DEFAULT_DIGEST.to_owned()));

    let error =
        run_install_bootstrap_with(&mut backend, BootstrapStartPolicy::PreserveObservedState)
            .unwrap_err();

    assert!(error.to_string().contains("unpublished"), "{error}");
}

#[test]
fn a_published_bootstrap_must_report_a_digest() {
    let mut backend = FakeBackend::safe_fresh();
    backend.forced_digest = Some(None);

    let error =
        run_install_bootstrap_with(&mut backend, BootstrapStartPolicy::EnsureRunning).unwrap_err();

    assert!(error.to_string().contains("no Ready digest"), "{error}");
}

fn assert_blocked_without_mutation(preflight: BootstrapPreflight, reason: BootstrapBlocker) {
    for policy in [
        BootstrapStartPolicy::EnsureRunning,
        BootstrapStartPolicy::PreserveObservedState,
    ] {
        let mut backend = FakeBackend {
            preflight: preflight.clone(),
            applied: Vec::new(),
            forced_digest: None,
        };
        let outcome = run_install_bootstrap_with(&mut backend, policy).unwrap();
        assert!(backend.applied.is_empty());
        assert_eq!(
            outcome,
            BootstrapOutcome::SkippedBlocked {
                reason: reason.clone(),
            }
        );
    }
}

#[test]
fn detected_legacy_service_is_a_non_mutating_blocked_skip() {
    let mut preflight = FakeBackend::safe_fresh().preflight;
    preflight.legacy_service = ConflictObservation::Detected;

    assert_blocked_without_mutation(preflight, BootstrapBlocker::LegacyService);
}

#[test]
fn detected_legacy_tray_mode_is_a_non_mutating_blocked_install() {
    let mut preflight = FakeBackend::safe_fresh().preflight;
    preflight.legacy_tray = ConflictObservation::Detected;

    assert_blocked_without_mutation(preflight, BootstrapBlocker::LegacyTrayMode);
}

#[test]
fn detected_appinit_conflict_is_a_non_mutating_blocked_skip() {
    let mut preflight = FakeBackend::safe_fresh().preflight;
    preflight.appinit = ConflictObservation::Detected;

    assert_blocked_without_mutation(preflight, BootstrapBlocker::AppInit);
}

#[test]
fn foreign_open_service_is_a_non_mutating_blocked_skip() {
    let mut preflight = FakeBackend::safe_fresh().preflight;
    preflight.open_service = OpenServiceObservation::Foreign;

    assert_blocked_without_mutation(preflight, BootstrapBlocker::ForeignOpenService);
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
        assert_blocked_without_mutation(preflight, BootstrapBlocker::UnknownMachineState);
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
        assert_blocked_without_mutation(preflight, BootstrapBlocker::InconsistentOwnedState);
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
    assert!(
        error
            .to_string()
            .contains("owned installation removal failed")
    );
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
fn installer_bootstrap_cli_accepts_only_the_fixed_argument_free_verbs() {
    assert_eq!(
        parse_setup_command(["bootstrap-install"]).unwrap(),
        SetupCommand::BootstrapInstall(BootstrapStartPolicy::EnsureRunning)
    );
    assert_eq!(
        parse_setup_command(["bootstrap-install-preserve-run-state"]).unwrap(),
        SetupCommand::BootstrapInstall(BootstrapStartPolicy::PreserveObservedState)
    );
    assert!(parse_setup_command(["bootstrap-install", r"C:\payload"]).is_err());
    assert!(parse_setup_command(["bootstrap-install=other-service"]).is_err());
    assert!(parse_setup_command(["bootstrap-install-preserve-run-state", "now"]).is_err());
    assert!(parse_setup_command(["bootstrap-install-preserve-run-state=1"]).is_err());
    assert!(parse_setup_command(["bootstrap-install-preserve"]).is_err());
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
