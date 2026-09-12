use super::*;
use crate::service_contract::{
    HealthState, InstallationState, RuntimeState, ServiceBackend, SystemServiceStatus,
};
use std::collections::VecDeque;

#[derive(Default)]
struct FakeMachineBackend {
    calls: Vec<&'static str>,
    status: Option<SystemServiceStatus>,
    legacy_tray: Option<LegacyTrayStatus>,
    executed: Option<(MachineAction, Vec<u8>)>,
    appinit_conflict: bool,
    appinit_error: Option<String>,
    legacy_service_blocks_activation: bool,
    legacy_service_observation_error: Option<String>,
}

impl MachineBackend for FakeMachineBackend {
    fn new_service_status(&mut self) -> SystemServiceStatus {
        self.calls.push("status");
        self.status.clone().expect("test status")
    }

    fn execute(&mut self, _action: MachineAction, _profile: Option<&[u8]>) -> Result<(), String> {
        self.calls.push("execute");
        self.executed = Some((_action, _profile.unwrap_or_default().to_vec()));
        Ok(())
    }

    fn appinit_conflict(&mut self) -> Result<bool, String> {
        self.calls.push("appinit");
        match self.appinit_error.take() {
            Some(error) => Err(error),
            None => Ok(self.appinit_conflict),
        }
    }

    fn legacy_service_blocks_activation(&mut self) -> Result<bool, String> {
        self.calls.push("legacy-service");
        match self.legacy_service_observation_error.take() {
            Some(error) => Err(error),
            None => Ok(self.legacy_service_blocks_activation),
        }
    }

    fn legacy_tray_status(&mut self) -> LegacyTrayStatus {
        self.calls.push("legacy-tray");
        self.legacy_tray
            .clone()
            .unwrap_or_else(LegacyTrayStatus::clear)
    }
}

fn ready_auto_service() -> SystemServiceStatus {
    SystemServiceStatus {
        backend: ServiceBackend::OpenSource,
        installation: InstallationState::Current,
        runtime: RuntimeState::Running,
        health: HealthState::Ready,
        binary_path: Some("fixed-service.exe".to_owned()),
        win32_error: None,
        active_profile_digest: Some("sha256:active".to_owned()),
        configuration_drift: false,
        can_install: false,
        can_remove: true,
        can_start: false,
        can_stop: true,
        can_repair: true,
        can_upgrade: false,
    }
}

#[test]
fn trusted_current_session_legacy_tray_blocks_a_machine_change_before_dispatch() {
    let mut backend = FakeMachineBackend {
        status: Some(ready_auto_service()),
        legacy_tray: Some(LegacyTrayStatus::from_states(
            LegacyTrayProcessState::TrustedCurrentSession {
                pid: 4242,
                creation_time: 101,
                path: std::path::PathBuf::from(r"C:\Program Files\MacType\MacTray.exe"),
            },
            LegacyTrayStartupState::Absent,
        )),
        ..Default::default()
    };

    let error = execute_machine_action_with(&mut backend, MachineAction::Repair, None).unwrap_err();

    assert!(error.contains("legacy MacTray tray mode"), "{error}");
    assert!(backend.executed.is_none());
}

#[test]
fn login_tray_observes_only_and_never_mutates_the_machine() {
    let mut backend = FakeMachineBackend {
        status: Some(ready_auto_service()),
        ..Default::default()
    };

    assert_eq!(
        tray_login_with(&mut backend, false, false, Some("sha256:active")),
        TrayLoginState::UsingRunningNewService
    );
    assert_eq!(backend.calls, ["status"]);

    backend.calls.clear();
    assert_eq!(
        tray_login_with(&mut backend, true, false, Some("sha256:active")),
        TrayLoginState::Paused
    );
    assert_eq!(backend.calls, ["status"]);
}

#[test]
fn explicit_tray_apply_is_the_only_tray_path_that_publishes_profile_bytes() {
    let profile = b"[General]\r\nGammaValue=1.3\r\n";
    let mut backend = FakeMachineBackend {
        status: Some(ready_auto_service()),
        ..Default::default()
    };

    tray_apply_with(&mut backend, false, profile).unwrap();

    assert_eq!(
        backend.calls,
        [
            "legacy-tray",
            "appinit",
            "status",
            "legacy-service",
            "execute"
        ]
    );
    assert_eq!(
        backend.executed,
        Some((MachineAction::PublishProfile, profile.to_vec()))
    );
    assert!(tray_apply_with(&mut backend, true, profile)
        .unwrap_err()
        .contains("paused"));
}

#[test]
fn unsafe_machine_state_is_rejected_before_dispatch_without_retry() {
    let profile = b"[General]\r\nGammaValue=1.3\r\n";
    let mut foreign = FakeMachineBackend {
        status: Some(SystemServiceStatus {
            backend: ServiceBackend::Foreign,
            installation: InstallationState::Invalid,
            runtime: RuntimeState::Unknown,
            health: HealthState::Unknown,
            binary_path: Some("foreign.exe".to_owned()),
            win32_error: None,
            active_profile_digest: None,
            configuration_drift: false,
            can_install: false,
            can_remove: false,
            can_start: false,
            can_stop: false,
            can_repair: false,
            can_upgrade: false,
        }),
        ..Default::default()
    };
    let error =
        execute_machine_action_with(&mut foreign, MachineAction::PublishProfile, Some(profile))
            .unwrap_err();
    assert!(
        error.contains("foreign") || error.contains("unsafe"),
        "{error}"
    );
    assert_eq!(foreign.calls, ["legacy-tray", "appinit", "status"]);

    let mut appinit = FakeMachineBackend {
        status: Some(ready_auto_service()),
        appinit_conflict: true,
        ..Default::default()
    };
    execute_machine_action_with(&mut appinit, MachineAction::Stop, None).unwrap();
    assert_eq!(appinit.calls, ["status", "execute"]);

    foreign.calls.clear();
    execute_machine_action_with(&mut foreign, MachineAction::Rollback, None).unwrap();
    assert_eq!(foreign.calls, ["status", "execute"]);
}

#[test]
fn native_actions_require_the_matching_backend_capability_before_dispatch() {
    let profile = b"[General]\r\nGammaValue=1.3\r\n";
    for action in [
        MachineAction::Install,
        MachineAction::Upgrade,
        MachineAction::Repair,
        MachineAction::Remove,
        MachineAction::Start,
        MachineAction::Stop,
    ] {
        let mut status = ready_auto_service();
        status.can_install = false;
        status.can_upgrade = false;
        status.can_repair = false;
        status.can_remove = false;
        status.can_start = false;
        status.can_stop = false;

        let mut denied = FakeMachineBackend {
            status: Some(status.clone()),
            appinit_conflict: action == MachineAction::Stop,
            ..Default::default()
        };
        let payload = (action == MachineAction::Start).then_some(profile.as_slice());
        let error = execute_machine_action_with(&mut denied, action, payload).unwrap_err();
        assert!(error.contains("authorize"), "{action:?}: {error}");
        assert!(denied.executed.is_none(), "{action:?} reached the broker");

        match action {
            MachineAction::Install => status.can_install = true,
            MachineAction::Upgrade => status.can_upgrade = true,
            MachineAction::Repair => status.can_repair = true,
            MachineAction::Remove => status.can_remove = true,
            MachineAction::Start => status.can_start = true,
            MachineAction::Stop => status.can_stop = true,
            _ => unreachable!(),
        }
        let mut allowed = FakeMachineBackend {
            status: Some(status),
            appinit_conflict: action == MachineAction::Stop,
            ..Default::default()
        };
        execute_machine_action_with(&mut allowed, action, payload).unwrap();
        let expected = if action == MachineAction::Start {
            (MachineAction::PublishProfile, profile.to_vec())
        } else {
            (action, Vec::new())
        };
        assert_eq!(allowed.executed, Some(expected));
    }
}

#[test]
fn start_with_an_explicit_profile_uses_the_profile_publication_transaction() {
    let profile = b"[General]\r\nGammaValue=1.3\r\n";
    let mut status = ready_auto_service();
    status.runtime = RuntimeState::Stopped;
    status.health = HealthState::Unknown;
    status.active_profile_digest = None;
    status.can_start = true;
    let mut backend = FakeMachineBackend {
        status: Some(status),
        ..Default::default()
    };

    execute_machine_action_with(&mut backend, MachineAction::Start, Some(profile)).unwrap();

    assert_eq!(
        backend.executed,
        Some((MachineAction::PublishProfile, profile.to_vec()))
    );
}

#[test]
fn start_without_an_explicit_profile_is_rejected_before_machine_observation() {
    let mut status = ready_auto_service();
    status.runtime = RuntimeState::Stopped;
    status.can_start = true;
    let mut backend = FakeMachineBackend {
        status: Some(status),
        ..Default::default()
    };

    let error = execute_machine_action_with(&mut backend, MachineAction::Start, None).unwrap_err();

    assert!(error.contains("invalid profile payload"), "{error}");
    assert!(backend.calls.is_empty());
    assert!(backend.executed.is_none());
}

#[test]
fn a_contending_legacy_service_blocks_generic_activation_but_not_reduction() {
    let profile = b"[General]\r\nGammaValue=1.3\r\n";
    // Install / Start / apply-profile must refuse while a legacy MacType service
    // is installed; retirement goes through Migrate, which stops it first.
    for action in [
        MachineAction::Install,
        MachineAction::Start,
        MachineAction::PublishProfile,
    ] {
        let mut status = ready_auto_service();
        status.installation = InstallationState::Current;
        status.runtime = RuntimeState::Stopped;
        status.can_install = true;
        status.can_start = true;
        let payload: Option<&[u8]> =
            matches!(action, MachineAction::Start | MachineAction::PublishProfile)
                .then_some(profile.as_slice());
        let mut backend = FakeMachineBackend {
            status: Some(status),
            legacy_service_blocks_activation: true,
            ..Default::default()
        };
        let error = execute_machine_action_with(&mut backend, action, payload).unwrap_err();
        assert!(
            error.contains("legacy MacType service"),
            "{action:?}: {error}"
        );
        assert!(backend.executed.is_none(), "{action:?} reached the broker");
    }

    // An inaccessible legacy service is fail-closed: the error propagates.
    let mut inaccessible = FakeMachineBackend {
        status: Some(ready_auto_service()),
        legacy_service_observation_error: Some("legacy service inaccessible".to_owned()),
        ..Default::default()
    };
    let mut start_status = ready_auto_service();
    start_status.runtime = RuntimeState::Stopped;
    start_status.can_start = true;
    inaccessible.status = Some(start_status);
    let error = execute_machine_action_with(&mut inaccessible, MachineAction::Start, Some(profile))
        .unwrap_err();
    assert!(error.contains("inaccessible"), "{error}");
    assert!(inaccessible.executed.is_none());

    // Reductive actions (Stop) are never blocked by a present legacy service and
    // never even consult it.
    let mut stop = FakeMachineBackend {
        status: Some(ready_auto_service()),
        legacy_service_blocks_activation: true,
        ..Default::default()
    };
    execute_machine_action_with(&mut stop, MachineAction::Stop, None).unwrap();
    assert_eq!(stop.executed, Some((MachineAction::Stop, Vec::new())));
    assert!(!stop.calls.contains(&"legacy-service"));
}

#[test]
fn appinit_conflict_never_turns_an_unrelated_native_capability_into_stop() {
    let profile = b"[General]\r\nGammaValue=1.3\r\n";
    let mut status = ready_auto_service();
    status.can_stop = false;
    status.can_start = true;
    let mut backend = FakeMachineBackend {
        status: Some(status),
        appinit_conflict: true,
        ..Default::default()
    };

    let error =
        execute_machine_action_with(&mut backend, MachineAction::Start, Some(profile)).unwrap_err();

    assert!(error.contains("AppInit"), "{error}");
    assert!(backend.executed.is_none());
}

#[test]
fn verified_stop_does_not_depend_on_reading_appinit_registry_state() {
    let profile = b"[General]\r\nGammaValue=1.3\r\n";
    let mut stop = FakeMachineBackend {
        status: Some(ready_auto_service()),
        appinit_error: Some("registry unavailable".to_owned()),
        ..Default::default()
    };

    execute_machine_action_with(&mut stop, MachineAction::Stop, None).unwrap();

    assert_eq!(stop.calls, ["status", "execute"]);

    let mut start_status = ready_auto_service();
    start_status.can_start = true;
    let mut start = FakeMachineBackend {
        status: Some(start_status),
        appinit_error: Some("registry unavailable".to_owned()),
        ..Default::default()
    };

    let error =
        execute_machine_action_with(&mut start, MachineAction::Start, Some(profile)).unwrap_err();

    assert!(error.contains("registry unavailable"), "{error}");
    assert_eq!(start.calls, ["legacy-tray", "appinit"]);
    assert!(start.executed.is_none());
}

#[test]
fn appinit_status_projects_only_the_verified_stop_capability() {
    let projected = project_new_service_capabilities(
        ready_auto_service(),
        true,
        LegacyTrayConflictState::Clear,
    );

    assert!(!projected.can_install);
    assert!(!projected.can_remove);
    assert!(!projected.can_start);
    assert!(projected.can_stop);
    assert!(!projected.can_repair);
    assert!(!projected.can_upgrade);

    let mut foreign = ready_auto_service();
    foreign.backend = ServiceBackend::Foreign;
    let projected = project_new_service_capabilities(foreign, true, LegacyTrayConflictState::Clear);
    assert!(!projected.can_stop);

    let mut not_authorized = ready_auto_service();
    not_authorized.can_stop = false;
    let projected =
        project_new_service_capabilities(not_authorized, true, LegacyTrayConflictState::Clear);
    assert!(!projected.can_stop);
}

#[test]
fn registry_conflict_never_claims_verified_system_injection() {
    let service = ready_auto_service();

    assert!(project_system_injection_active(
        &service,
        false,
        LegacyTrayConflictState::Clear,
        Some("sha256:active")
    ));
    assert!(!project_system_injection_active(
        &service,
        true,
        LegacyTrayConflictState::Clear,
        Some("sha256:active")
    ));
    assert!(
        project_new_service_capabilities(service, true, LegacyTrayConflictState::Clear,).can_stop
    );
}

#[test]
fn legacy_tray_detected_or_unknown_projects_only_verified_stop_and_never_active() {
    for conflict in [
        LegacyTrayConflictState::Detected,
        LegacyTrayConflictState::Unknown,
    ] {
        let service = ready_auto_service();
        let projected = project_new_service_capabilities(service.clone(), false, conflict);

        assert!(!projected.can_install);
        assert!(!projected.can_remove);
        assert!(!projected.can_start);
        assert!(projected.can_stop);
        assert!(!projected.can_repair);
        assert!(!projected.can_upgrade);
        assert!(!project_system_injection_active(
            &service,
            false,
            conflict,
            Some("sha256:active"),
        ));
    }
}

#[test]
fn appinit_blocks_only_enabled_mactype_dlls_and_fails_closed() {
    let mactype = "C:\\Program Files\\MacType\\MacType64.dll\0"
        .encode_utf16()
        .collect::<Vec<_>>();
    let other = "C:\\Other\\OtherHook.dll\0"
        .encode_utf16()
        .collect::<Vec<_>>();
    assert!(appinit_view_conflict(Ok(true), Ok(Some(mactype))).unwrap());
    assert!(!appinit_view_conflict(Ok(false), Err(())).unwrap());
    assert!(!appinit_view_conflict(Ok(true), Ok(Some(other))).unwrap());
    assert!(appinit_view_conflict(Err(()), Ok(None)).is_err());
    assert!(appinit_view_conflict(Ok(true), Ok(Some(vec![b'M' as u16]))).is_err());
}

#[test]
fn publish_profile_orders_running_stopped_and_absent_service_activation() {
    struct PublishBackend {
        states: VecDeque<SystemServiceStatus>,
        actions: Vec<MachineAction>,
    }

    impl MachineBackend for PublishBackend {
        fn new_service_status(&mut self) -> SystemServiceStatus {
            self.states.pop_front().expect("test service state")
        }

        fn legacy_tray_status(&mut self) -> LegacyTrayStatus {
            LegacyTrayStatus::clear()
        }

        fn appinit_conflict(&mut self) -> Result<bool, String> {
            Ok(false)
        }

        fn legacy_service_blocks_activation(&mut self) -> Result<bool, String> {
            Ok(false)
        }

        fn execute(
            &mut self,
            action: MachineAction,
            _profile: Option<&[u8]>,
        ) -> Result<(), String> {
            self.actions.push(action);
            Ok(())
        }
    }

    let profile = b"[General]\r\nGammaValue=1.3\r\n";
    let digest = mactype_service_contract::GenerationId::from_profile_bytes(profile)
        .as_str()
        .to_owned();
    let ready = |installation| SystemServiceStatus {
        installation,
        active_profile_digest: Some(digest.clone()),
        ..ready_auto_service()
    };
    let before = |installation, runtime| SystemServiceStatus {
        installation,
        runtime,
        health: HealthState::Unknown,
        active_profile_digest: None,
        ..ready_auto_service()
    };

    for (initial, expected) in [
        (
            before(InstallationState::Current, RuntimeState::Running),
            vec![
                MachineAction::Stop,
                MachineAction::PublishProfile,
                MachineAction::Start,
            ],
        ),
        (
            before(InstallationState::Current, RuntimeState::Stopped),
            vec![MachineAction::PublishProfile, MachineAction::Start],
        ),
        (
            before(InstallationState::Absent, RuntimeState::Stopped),
            vec![
                MachineAction::PublishProfile,
                MachineAction::Install,
                MachineAction::Start,
            ],
        ),
        (
            before(InstallationState::Outdated, RuntimeState::Stopped),
            vec![
                MachineAction::PublishProfile,
                MachineAction::Upgrade,
                MachineAction::Start,
            ],
        ),
    ] {
        let mut backend = PublishBackend {
            states: VecDeque::from([initial, ready(InstallationState::Current)]),
            actions: Vec::new(),
        };
        publish_profile_transaction_with(&mut backend, profile).unwrap();
        assert_eq!(backend.actions, expected);
    }

    let initializing = SystemServiceStatus {
        installation: InstallationState::Current,
        runtime: RuntimeState::Running,
        health: HealthState::Initializing,
        active_profile_digest: Some(digest.clone()),
        ..ready_auto_service()
    };
    let mut backend = PublishBackend {
        states: VecDeque::from([
            SystemServiceStatus {
                active_profile_digest: Some(digest.clone()),
                ..before(InstallationState::Current, RuntimeState::Running)
            },
            initializing,
            ready(InstallationState::Current),
        ]),
        actions: Vec::new(),
    };

    publish_profile_transaction_with(&mut backend, profile).unwrap();
    assert_eq!(
        backend.actions,
        [
            MachineAction::Stop,
            MachineAction::PublishProfile,
            MachineAction::Start,
        ]
    );
}

struct FailingPublishBackend {
    states: VecDeque<SystemServiceStatus>,
    results: VecDeque<Result<(), String>>,
    actions: Vec<MachineAction>,
}

impl MachineBackend for FailingPublishBackend {
    fn new_service_status(&mut self) -> SystemServiceStatus {
        self.states.pop_front().expect("test service state")
    }

    fn legacy_tray_status(&mut self) -> LegacyTrayStatus {
        LegacyTrayStatus::clear()
    }

    fn appinit_conflict(&mut self) -> Result<bool, String> {
        Ok(false)
    }

    fn legacy_service_blocks_activation(&mut self) -> Result<bool, String> {
        Ok(false)
    }

    fn execute(&mut self, action: MachineAction, _profile: Option<&[u8]>) -> Result<(), String> {
        self.actions.push(action);
        self.results.pop_front().unwrap_or(Ok(()))
    }
}

#[test]
fn publish_failure_reports_a_failed_restart_as_cleanup_unknown() {
    let mut backend = FailingPublishBackend {
        states: VecDeque::from([ready_auto_service()]),
        results: VecDeque::from([
            Ok(()),
            Err("publish failed".to_owned()),
            Err("restart failed".to_owned()),
        ]),
        actions: Vec::new(),
    };

    let error = publish_profile_transaction_with(&mut backend, b"[General]\r\n").unwrap_err();

    assert!(error.contains("publish failed"), "{error}");
    assert!(error.contains("restart failed"), "{error}");
    assert!(error.contains("cleanup is unknown"), "{error}");
    assert_eq!(
        backend.actions,
        [
            MachineAction::Stop,
            MachineAction::PublishProfile,
            MachineAction::Start,
        ]
    );
}

#[test]
fn activation_failure_reports_every_failed_rollback_step() {
    let mut backend = FailingPublishBackend {
        states: VecDeque::from([ready_auto_service()]),
        results: VecDeque::from([
            Ok(()),
            Ok(()),
            Err("activation failed".to_owned()),
            Err("cleanup stop failed".to_owned()),
            Err("profile rollback failed".to_owned()),
            Err("prior restart failed".to_owned()),
        ]),
        actions: Vec::new(),
    };

    let error = publish_profile_transaction_with(&mut backend, b"[General]\r\n").unwrap_err();

    for expected in [
        "activation failed",
        "cleanup stop failed",
        "profile rollback failed",
        "prior restart failed",
        "cleanup is unknown",
    ] {
        assert!(error.contains(expected), "missing {expected}: {error}");
    }
    assert_eq!(
        backend.actions,
        [
            MachineAction::Stop,
            MachineAction::PublishProfile,
            MachineAction::Start,
            MachineAction::Stop,
            MachineAction::Rollback,
            MachineAction::Start,
        ]
    );
}

#[test]
fn activation_failure_preserves_a_profile_that_was_already_active() {
    let profile = b"[General]\r\nGammaValue=1.3\r\n";
    let digest = mactype_service_contract::GenerationId::from_profile_bytes(profile)
        .as_str()
        .to_owned();
    let mut backend = FailingPublishBackend {
        states: VecDeque::from([SystemServiceStatus {
            active_profile_digest: Some(digest),
            ..ready_auto_service()
        }]),
        results: VecDeque::from([
            Ok(()),
            Ok(()),
            Err("activation failed".to_owned()),
            Ok(()),
            Ok(()),
        ]),
        actions: Vec::new(),
    };

    let error = publish_profile_transaction_with(&mut backend, profile).unwrap_err();

    assert!(error.contains("activation failed"), "{error}");
    assert_eq!(
        backend.actions,
        [
            MachineAction::Stop,
            MachineAction::PublishProfile,
            MachineAction::Start,
            MachineAction::Stop,
            MachineAction::Start,
        ]
    );
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartupCoordinationEvent {
    Observe,
    DisableCurrentUser,
    DisableLocalMachine,
    RestoreLocalMachine,
    RestoreCurrentUser,
}

struct RecordingStartupCoordinator {
    statuses: std::collections::VecDeque<LegacyTrayStatus>,
    events: Vec<StartupCoordinationEvent>,
    machine_disable_error: Option<String>,
}

impl LegacyTrayStartupCoordinator for RecordingStartupCoordinator {
    fn observe_status(&mut self) -> LegacyTrayStatus {
        self.events.push(StartupCoordinationEvent::Observe);
        self.statuses
            .pop_front()
            .expect("the startup coordinator observed too many times")
    }

    fn disable_current_user(&mut self) -> Result<(), String> {
        self.events
            .push(StartupCoordinationEvent::DisableCurrentUser);
        Ok(())
    }

    fn disable_local_machine(&mut self) -> Result<(), String> {
        self.events
            .push(StartupCoordinationEvent::DisableLocalMachine);
        self.machine_disable_error.take().map_or(Ok(()), Err)
    }

    fn restore_local_machine(&mut self) -> Result<(), String> {
        self.events
            .push(StartupCoordinationEvent::RestoreLocalMachine);
        Ok(())
    }

    fn restore_current_user(&mut self) -> Result<(), String> {
        self.events
            .push(StartupCoordinationEvent::RestoreCurrentUser);
        Ok(())
    }
}

fn detected_startup_status() -> LegacyTrayStatus {
    LegacyTrayStatus::from_states(
        LegacyTrayProcessState::Absent,
        LegacyTrayStartupState::Detected {
            entries: vec![crate::machine_integration::legacy_mactray::LegacyTrayStartupEntry {
                source_kind:
                    crate::machine_integration::legacy_mactray::LegacyTrayStartupSource::CurrentUserRun64,
                display_name: "MacTray".to_owned(),
                target_path: std::path::PathBuf::from(
                    r"C:\Program Files\MacType\MacTray.exe",
                ),
            }],
        },
    )
}

#[test]
fn consented_startup_disable_rechecks_the_whole_state_after_both_jurisdictions() {
    let mut backend = RecordingStartupCoordinator {
        statuses: [detected_startup_status(), LegacyTrayStatus::clear()].into(),
        events: Vec::new(),
        machine_disable_error: None,
    };

    disable_legacy_tray_startup_with(&mut backend).unwrap();
    assert_eq!(
        backend.events,
        [
            StartupCoordinationEvent::Observe,
            StartupCoordinationEvent::DisableCurrentUser,
            StartupCoordinationEvent::DisableLocalMachine,
            StartupCoordinationEvent::Observe,
        ]
    );
}

#[test]
fn machine_startup_disable_failure_restores_the_current_user_receipt() {
    let mut backend = RecordingStartupCoordinator {
        statuses: [detected_startup_status()].into(),
        events: Vec::new(),
        machine_disable_error: Some("simulated machine failure".to_owned()),
    };

    assert!(disable_legacy_tray_startup_with(&mut backend).is_err());
    assert_eq!(
        backend.events,
        [
            StartupCoordinationEvent::Observe,
            StartupCoordinationEvent::DisableCurrentUser,
            StartupCoordinationEvent::DisableLocalMachine,
            StartupCoordinationEvent::RestoreCurrentUser,
        ]
    );
}

#[test]
fn failed_final_recheck_restores_machine_then_user_autostart() {
    let unknown = LegacyTrayStatus::from_states(
        LegacyTrayProcessState::Absent,
        LegacyTrayStartupState::Unknown {
            error: mactype_service_contract::StructuredServiceError {
                code: "query-failed".to_owned(),
                message: "simulated".to_owned(),
                win32_error: Some(5),
            },
        },
    );
    let mut backend = RecordingStartupCoordinator {
        statuses: [detected_startup_status(), unknown].into(),
        events: Vec::new(),
        machine_disable_error: None,
    };

    assert!(disable_legacy_tray_startup_with(&mut backend).is_err());
    assert_eq!(
        backend.events,
        [
            StartupCoordinationEvent::Observe,
            StartupCoordinationEvent::DisableCurrentUser,
            StartupCoordinationEvent::DisableLocalMachine,
            StartupCoordinationEvent::Observe,
            StartupCoordinationEvent::RestoreLocalMachine,
            StartupCoordinationEvent::RestoreCurrentUser,
        ]
    );
}

#[test]
fn startup_disable_never_mutates_when_a_tray_process_is_running() {
    let mut backend = RecordingStartupCoordinator {
        statuses: [LegacyTrayStatus::from_states(
            LegacyTrayProcessState::TrustedCurrentSession {
                pid: 42,
                creation_time: 99,
                path: std::path::PathBuf::from(r"C:\Program Files\MacType\MacTray.exe"),
            },
            LegacyTrayStartupState::Absent,
        )]
        .into(),
        events: Vec::new(),
        machine_disable_error: None,
    };

    assert!(disable_legacy_tray_startup_with(&mut backend).is_err());
    assert_eq!(backend.events, [StartupCoordinationEvent::Observe]);
}
