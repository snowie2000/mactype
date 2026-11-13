use super::super::{
    StartupReceiptRestorer, SystemServiceAction,
    action_failure::{ActionBlocker, ActionFailure, InstallationPreflightKind},
    finish_action_with_startup_receipts,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    RestoreMachine,
    RestoreUser,
}

struct RecordingRestorer {
    events: Vec<Event>,
    machine_error: Option<String>,
    user_error: Option<String>,
}

impl StartupReceiptRestorer for RecordingRestorer {
    fn restore_local_machine(&mut self) -> Result<(), ActionFailure> {
        self.events.push(Event::RestoreMachine);
        self.machine_error
            .take()
            .map_or(Ok(()), |error| Err(ActionFailure::from(error)))
    }

    fn restore_current_user(&mut self) -> Result<(), ActionFailure> {
        self.events.push(Event::RestoreUser);
        self.user_error
            .take()
            .map_or(Ok(()), |error| Err(ActionFailure::from(error)))
    }
}

fn restorer() -> RecordingRestorer {
    RecordingRestorer {
        events: Vec::new(),
        machine_error: None,
        user_error: None,
    }
}

#[test]
fn failed_initial_install_restores_machine_then_user_startup_receipts() {
    let mut backend = restorer();
    let error = finish_action_with_startup_receipts(
        &mut backend,
        SystemServiceAction::Install,
        Err(ActionFailure::internal("install failed")),
    )
    .unwrap_err();

    assert!(error.contains("install failed"), "{error}");
    assert_eq!(backend.events, [Event::RestoreMachine, Event::RestoreUser]);
}

#[test]
fn failed_installation_preflight_does_not_run_startup_restoration() {
    let mut backend = restorer();
    let blocker = ActionBlocker::InstallationPreflight {
        kind: InstallationPreflightKind::Required,
        diagnostics: Box::new(crate::diagnostics::InstallationPreflightDiagnostics {
            expected_installed_control_center: None,
            current_executable: None,
            expected_executable_exists: None,
            installed_control_center: "not-checked".to_owned(),
            current_bundle: "not-checked".to_owned(),
            selected_service_package: "none".to_owned(),
            setup_broker: "not-checked".to_owned(),
            runtime_manifest: "not-checked".to_owned(),
            runtime_payload: "not-checked".to_owned(),
            elevation_attempted: false,
            elevated_revalidation: "not-attempted".to_owned(),
            machine_state_changed: false,
            rollback_required: false,
        }),
    };
    let error = finish_action_with_startup_receipts(
        &mut backend,
        SystemServiceAction::Install,
        Err(ActionFailure::blocked_with_detail(
            blocker,
            "control-center-installation-required: run the complete installer first".to_owned(),
        )),
    )
    .unwrap_err();

    assert!(error.starts_with("control-center-installation-required:"));
    assert!(backend.events.is_empty());
}

#[test]
fn failed_legacy_migration_restores_both_startup_jurisdictions() {
    let mut backend = restorer();
    assert!(
        finish_action_with_startup_receipts(
            &mut backend,
            SystemServiceAction::MigrateFromLegacy,
            Err(ActionFailure::internal("migration failed")),
        )
        .is_err()
    );
    assert_eq!(backend.events, [Event::RestoreMachine, Event::RestoreUser]);
}

#[test]
fn internal_profile_rollback_does_not_touch_legacy_startup_receipts() {
    let mut backend = restorer();
    finish_action_with_startup_receipts(&mut backend, SystemServiceAction::Rollback, Ok(()))
        .unwrap();
    assert!(backend.events.is_empty());
}

#[test]
fn restoration_attempts_every_scope_and_preserves_all_errors() {
    let mut backend = RecordingRestorer {
        events: Vec::new(),
        machine_error: Some("machine restore failed".to_owned()),
        user_error: Some("user restore failed".to_owned()),
    };
    let error = finish_action_with_startup_receipts(
        &mut backend,
        SystemServiceAction::Install,
        Err(ActionFailure::internal("install failed")),
    )
    .unwrap_err();

    for expected in [
        "install failed",
        "machine restore failed",
        "user restore failed",
    ] {
        assert!(error.contains(expected), "missing {expected}: {error}");
    }
    assert_eq!(backend.events, [Event::RestoreMachine, Event::RestoreUser]);
}

#[test]
fn unrelated_action_results_do_not_touch_legacy_startup_receipts() {
    let mut backend = restorer();
    assert!(
        finish_action_with_startup_receipts(
            &mut backend,
            SystemServiceAction::Upgrade,
            Err(ActionFailure::internal("upgrade failed")),
        )
        .is_err()
    );
    assert!(backend.events.is_empty());
}
