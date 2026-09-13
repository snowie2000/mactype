use super::*;

fn official_configuration(path: &Path) -> ServiceConfiguration {
    owned_service_configuration(path)
}

fn complete_migration_snapshot(path: &Path) -> LegacyScmSnapshot {
    LegacyScmSnapshot {
        presence: ServicePresence::Owned,
        state: ServiceRuntimeState::Running,
        configuration: owned_service_configuration(path),
        extended: ServiceExtendedConfiguration {
            description: Some("legacy renderer".to_owned()),
            failure_actions: FailureActionsConfiguration {
                reset_period_seconds: 30,
                reboot_message: None,
                command: None,
                actions: vec![FailureAction {
                    action_type: 1,
                    delay_ms: 1_000,
                }],
            },
            failure_actions_on_non_crash: true,
            delayed_auto_start: false,
            service_sid_type: 0,
            required_privileges: vec!["SeChangeNotifyPrivilege".to_owned()],
            preshutdown_timeout_ms: 180_000,
            triggers: ServiceTriggerConfiguration::None,
            security_descriptor: SecurityDescriptorSnapshot {
                self_relative: {
                    let mut descriptor = vec![0u8; 20];
                    descriptor[0] = 1;
                    descriptor[2] = 4;
                    descriptor[3] = 128;
                    descriptor
                },
            },
        },
    }
}

#[test]
fn direct_scm_registration_matches_the_owned_service_policy() {
    let path = Path::new(r"C:\Program Files\MacType\MacTray.exe");
    let configuration = owned_service_configuration(path);
    assert_eq!(
        classify_configuration(&configuration, path),
        ServicePresence::Owned
    );
    assert_eq!(
        configuration.binary_path,
        r#""C:\Program Files\MacType\MacTray.exe" -service"#
    );
    assert_eq!(configuration.dependencies, ["winmgmt"]);
    assert_eq!(configuration.account, "LocalSystem");
    assert_eq!(configuration.display_name, "MacType");
    assert_eq!(configuration.load_order_group, None);
    assert_eq!(configuration.tag_id, 0);
}

#[test]
fn a_disabled_start_type_is_still_the_owned_service_but_other_start_types_are_foreign() {
    let path = Path::new(r"C:\Program Files\MacType\MacTray.exe");

    // The migration parks the owned service disabled between the stop and the
    // funeral; it must still classify as owned so removal and rollback work.
    let mut disabled = owned_service_configuration(path);
    disabled.start_type = 4;
    assert_eq!(
        classify_configuration(&disabled, path),
        ServicePresence::Owned
    );

    // Any other start type (manual, boot, system) is not a shape we ever set.
    for start_type in [0u32, 1, 3] {
        let mut other = owned_service_configuration(path);
        other.start_type = start_type;
        assert_eq!(
            classify_configuration(&other, path),
            ServicePresence::Foreign,
            "start type {start_type} must not classify as owned"
        );
    }
}

#[test]
fn only_a_stopped_owned_disabled_legacy_service_is_retired_for_new_activation() {
    assert!(retired_owned_service(
        ServicePresence::Owned,
        ServiceRuntimeState::Stopped,
        4,
    ));
    assert!(retired_owned_service(
        ServicePresence::CompatibleUnquoted,
        ServiceRuntimeState::Stopped,
        4,
    ));
    assert!(!retired_owned_service(
        ServicePresence::Owned,
        ServiceRuntimeState::Running,
        4,
    ));
    assert!(!retired_owned_service(
        ServicePresence::Owned,
        ServiceRuntimeState::Stopped,
        2,
    ));
    assert!(!retired_owned_service(
        ServicePresence::Foreign,
        ServiceRuntimeState::Stopped,
        4,
    ));
}

#[test]
fn load_order_group_and_tag_are_part_of_strict_service_ownership() {
    let path = Path::new(r"C:\Program Files\MacType\MacTray.exe");

    let mut grouped = official_configuration(path);
    grouped.load_order_group = Some("NetworkProvider".to_owned());
    assert_eq!(
        classify_configuration(&grouped, path),
        ServicePresence::Foreign
    );

    let mut tagged = official_configuration(path);
    tagged.tag_id = 7;
    assert_eq!(
        classify_configuration(&tagged, path),
        ServicePresence::Foreign
    );
}

#[test]
fn migration_snapshot_contract_preserves_the_scm_display_name() {
    let path = Path::new(r"C:\Program Files\MacType\MacTray.exe");
    let mut configuration = owned_service_configuration(path);
    configuration.display_name = "MacType legacy renderer".to_owned();
    let serialized = serde_json::to_vec(&configuration).unwrap();
    let restored: ServiceConfiguration = serde_json::from_slice(&serialized).unwrap();

    assert_eq!(restored.display_name, "MacType legacy renderer");
}

#[test]
fn migration_snapshot_owns_config2_and_security_descriptor_data() {
    let path = Path::new(r"C:\Program Files\MacType\MacTray.exe");
    let snapshot = LegacyScmSnapshot {
        presence: ServicePresence::Owned,
        state: ServiceRuntimeState::Running,
        configuration: owned_service_configuration(path),
        extended: ServiceExtendedConfiguration {
            description: Some("legacy renderer".to_owned()),
            failure_actions: FailureActionsConfiguration {
                reset_period_seconds: 30,
                reboot_message: None,
                command: None,
                actions: vec![FailureAction {
                    action_type: 1,
                    delay_ms: 1_000,
                }],
            },
            failure_actions_on_non_crash: true,
            delayed_auto_start: false,
            service_sid_type: 0,
            required_privileges: vec!["SeChangeNotifyPrivilege".to_owned()],
            preshutdown_timeout_ms: 180_000,
            triggers: ServiceTriggerConfiguration::None,
            security_descriptor: SecurityDescriptorSnapshot {
                self_relative: {
                    let mut descriptor = vec![0u8; 20];
                    descriptor[0] = 1;
                    descriptor[2] = 4;
                    descriptor[3] = 128;
                    descriptor
                },
            },
        },
    };

    let serialized = serde_json::to_vec(&snapshot).unwrap();
    let restored: LegacyScmSnapshot = serde_json::from_slice(&serialized).unwrap();

    assert_eq!(restored, snapshot);
    #[cfg(windows)]
    {
        validate_migration_snapshot(&restored).unwrap();
        let mut invalid = restored.clone();
        invalid
            .extended
            .security_descriptor
            .self_relative
            .truncate(4);
        assert!(validate_migration_snapshot(&invalid).is_err());
    }
}

#[test]
fn rollback_verification_compares_core_config2_and_security_exactly() {
    let expected = complete_migration_snapshot(Path::new(r"C:\Program Files\MacType\MacTray.exe"));

    verify_restored_configuration(&expected, &expected.configuration, &expected.extended).unwrap();

    let mut wrong_core = expected.configuration.clone();
    wrong_core.tag_id = 9;
    assert!(verify_restored_configuration(&expected, &wrong_core, &expected.extended).is_err());

    let mut wrong_config2 = expected.extended.clone();
    wrong_config2.preshutdown_timeout_ms += 1;
    assert!(
        verify_restored_configuration(&expected, &expected.configuration, &wrong_config2).is_err()
    );

    let mut wrong_security = expected.extended.clone();
    wrong_security.security_descriptor.self_relative[4] ^= 1;
    assert!(
        verify_restored_configuration(&expected, &expected.configuration, &wrong_security).is_err()
    );
}

#[test]
fn migration_accepts_no_service_triggers_and_rejects_custom_trigger_state() {
    assert_eq!(
        snapshot_trigger_configuration(0, false, false).unwrap(),
        ServiceTriggerConfiguration::None
    );
    assert!(snapshot_trigger_configuration(1, true, false).is_err());
    assert!(snapshot_trigger_configuration(0, true, false).is_err());
    assert!(snapshot_trigger_configuration(0, false, true).is_err());
}

#[test]
fn migration_accepts_only_exact_running_or_stopped_runtime_states() {
    for state in [ServiceRuntimeState::Running, ServiceRuntimeState::Stopped] {
        require_stable_migration_state(state).unwrap();
    }

    for state in [
        ServiceRuntimeState::StartPending,
        ServiceRuntimeState::StopPending,
        ServiceRuntimeState::ContinuePending,
        ServiceRuntimeState::PausePending,
        ServiceRuntimeState::Paused,
        ServiceRuntimeState::Unknown,
    ] {
        assert!(require_stable_migration_state(state).is_err());
    }
}

struct RecordingServiceRestore {
    steps: Vec<ServiceRestoreStep>,
}

impl ServiceConfigurationRestorer for RecordingServiceRestore {
    fn restore(&mut self, step: ServiceRestoreStep) -> Result<(), String> {
        self.steps.push(step);
        if matches!(
            step,
            ServiceRestoreStep::FailureActions | ServiceRestoreStep::SecurityDescriptor
        ) {
            Err("simulated restore failure".to_owned())
        } else {
            Ok(())
        }
    }
}

#[test]
fn rollback_restores_core_then_config2_then_security_and_aggregates_failures() {
    let mut backend = RecordingServiceRestore { steps: Vec::new() };

    let error = perform_service_configuration_restore(&mut backend).unwrap_err();

    assert_eq!(backend.steps, SERVICE_RESTORE_ORDER);
    assert!(error.contains("failure-actions"));
    assert!(error.contains("security-descriptor"));
}

#[test]
fn only_the_verified_mactray_service_is_owned() {
    let path = Path::new(r"C:\Program Files\MacType\MacTray.exe");
    assert_eq!(
        classify_configuration(&official_configuration(path), path),
        ServicePresence::Owned
    );
    let mut foreign = official_configuration(path);
    foreign.binary_path = r"C:\Temp\MacTray.exe -service".to_owned();
    assert_eq!(
        classify_configuration(&foreign, path),
        ServicePresence::Foreign
    );
}

#[test]
fn compatible_unquoted_official_service_is_a_warning_not_foreign() {
    let path = Path::new(r"C:\Program Files\MacType\MacTray.exe");
    let mut configuration = official_configuration(path);
    configuration.binary_path = format!("{} -service", path.display());
    assert_eq!(
        classify_configuration(&configuration, path),
        ServicePresence::CompatibleUnquoted
    );
    let status = with_capabilities(
        ServicePresence::CompatibleUnquoted,
        ServiceRuntimeState::Stopped,
        Some(configuration.binary_path),
        None,
        true,
        false,
    );
    assert!(status.can_remove);
    assert!(!status.can_stop);
}

#[test]
fn canonicalized_windows_path_matches_the_service_manager_image_path() {
    let canonical = Path::new(r"\\?\C:\Program Files\MacType\MacTray.exe");
    let mut configuration =
        official_configuration(Path::new(r"C:\Program Files\MacType\MacTray.exe"));

    assert_eq!(
        classify_configuration(&configuration, canonical),
        ServicePresence::Owned
    );

    configuration.binary_path = r"C:\Program Files\MacType\MacTray.exe -service".to_owned();
    let presence = classify_configuration(&configuration, canonical);
    assert_eq!(presence, ServicePresence::CompatibleUnquoted);

    let status = with_capabilities(
        presence,
        ServiceRuntimeState::Running,
        Some(configuration.binary_path),
        None,
        true,
        false,
    );
    assert!(!status.can_remove);
    assert!(status.can_stop);
}

#[test]
fn registry_conflict_blocks_legacy_removal_and_stop() {
    let stopped = with_capabilities(
        ServicePresence::Owned,
        ServiceRuntimeState::Stopped,
        None,
        None,
        true,
        true,
    );
    assert!(!stopped.can_remove);
    assert!(!stopped.can_stop);
}

#[test]
fn missing_legacy_binary_allows_stopped_disposal_but_not_running_mutation() {
    let stopped = with_capabilities(
        ServicePresence::Owned,
        ServiceRuntimeState::Stopped,
        Some(r#""C:\Program Files\MacType\MacTray.exe" -service"#.to_owned()),
        None,
        false,
        false,
    );
    assert!(stopped.can_remove);
    assert!(!stopped.can_stop);

    let running = with_capabilities(
        ServicePresence::Owned,
        ServiceRuntimeState::Running,
        Some(r#""C:\Program Files\MacType\MacTray.exe" -service"#.to_owned()),
        None,
        false,
        false,
    );
    assert!(!running.can_remove);
    assert!(!running.can_stop);
}

#[test]
fn scm_ownership_uses_the_fixed_expected_path_not_binary_availability() {
    let expected = Path::new(r"C:\Program Files\MacType\MacTray.exe");
    let configuration = official_configuration(expected);

    let stopped = status_from_configuration(
        &configuration,
        ServiceRuntimeState::Stopped,
        Some(expected),
        false,
        false,
    );

    assert_eq!(stopped.presence, ServicePresence::Owned);
    assert!(!stopped.trusted_binary_available);
    assert!(stopped.can_remove);

    let no_program_files_identity = status_from_configuration(
        &configuration,
        ServiceRuntimeState::Stopped,
        None,
        false,
        false,
    );
    assert_eq!(no_program_files_identity.presence, ServicePresence::Foreign);
    assert!(!no_program_files_identity.can_remove);
}

fn assert_no_mutation(status: &LegacyServiceStatus) {
    assert!(!status.can_remove);
    assert!(!status.can_stop);
}

#[test]
fn unsafe_service_states_never_expose_mutation_capabilities() {
    for presence in [
        ServicePresence::Foreign,
        ServicePresence::DeletePending,
        ServicePresence::Inaccessible,
    ] {
        for state in [
            ServiceRuntimeState::Stopped,
            ServiceRuntimeState::Running,
            ServiceRuntimeState::Unknown,
        ] {
            let status = with_capabilities(presence, state, None, None, true, false);
            assert_no_mutation(&status);
        }
    }
}

#[test]
fn registry_conflict_blocks_every_mutation() {
    for presence in [
        ServicePresence::Absent,
        ServicePresence::Owned,
        ServicePresence::CompatibleUnquoted,
    ] {
        for state in [ServiceRuntimeState::Stopped, ServiceRuntimeState::Running] {
            let status = with_capabilities(presence, state, None, None, true, true);
            assert_no_mutation(&status);
        }
    }
}

#[test]
fn service_metadata_must_match_the_official_configuration() {
    let path = Path::new(r"C:\Program Files\MacType\MacTray.exe");
    let mut configurations = Vec::new();

    let mut wrong_type = official_configuration(path);
    wrong_type.service_type = 0x20;
    configurations.push(wrong_type);

    let mut manual_start = official_configuration(path);
    manual_start.start_type = 3;
    configurations.push(manual_start);

    let mut wrong_error_control = official_configuration(path);
    wrong_error_control.error_control = 0;
    configurations.push(wrong_error_control);

    let mut wrong_account = official_configuration(path);
    wrong_account.account = "LocalService".to_owned();
    configurations.push(wrong_account);

    let mut missing_dependency = official_configuration(path);
    missing_dependency.dependencies.clear();
    configurations.push(missing_dependency);

    let mut extra_argument = official_configuration(path);
    extra_argument.binary_path.push_str(" unexpected");
    configurations.push(extra_argument);

    for configuration in configurations {
        assert_eq!(
            classify_configuration(&configuration, path),
            ServicePresence::Foreign
        );
    }
}

#[test]
fn ownership_requires_exact_display_name_and_exactly_one_winmgmt_dependency() {
    let path = Path::new(r"C:\Program Files\MacType\MacTray.exe");

    let mut wrong_display = official_configuration(path);
    wrong_display.display_name = "MacType Service".to_owned();
    assert_eq!(
        classify_configuration(&wrong_display, path),
        ServicePresence::Foreign
    );

    let mut extra_dependency = official_configuration(path);
    extra_dependency.dependencies.push("RpcSs".to_owned());
    assert_eq!(
        classify_configuration(&extra_dependency, path),
        ServicePresence::Foreign
    );

    let mut duplicate_dependency = official_configuration(path);
    duplicate_dependency.dependencies.push("WINMGMT".to_owned());
    assert_eq!(
        classify_configuration(&duplicate_dependency, path),
        ServicePresence::Foreign
    );
}

#[test]
fn trusted_binary_must_resolve_to_the_exact_program_files_layout() {
    let root = Path::new("program-files");
    assert!(is_trusted_mactray_layout(
        root,
        &root.join("MacType").join("MacTray.exe")
    ));
    assert!(is_trusted_mactray_layout(
        root,
        &root.join("mactype").join("MACTRAY.EXE")
    ));
    assert!(!is_trusted_mactray_layout(
        root,
        &root.join("MacType-old").join("MacTray.exe")
    ));
    assert!(!is_trusted_mactray_layout(
        root,
        &root.join("MacType").join("bin").join("MacTray.exe")
    ));
    assert!(!is_trusted_mactray_layout(
        root,
        &Path::new("other-root").join("MacType").join("MacTray.exe")
    ));
}

#[test]
fn unknown_tray_process_state_fails_closed_without_offering_exit() {
    let status = LegacyTrayStatus::from_states(
        LegacyTrayProcessState::Unknown {
            error: mactype_service_contract::StructuredServiceError {
                code: "legacy-tray-process-query-failed".to_owned(),
                message: "process inventory unavailable".to_owned(),
                win32_error: Some(5),
            },
        },
        LegacyTrayStartupState::Absent,
    );

    assert_eq!(status.conflict, LegacyTrayConflictState::Unknown);
    assert!(status.blocks_machine_change());
    assert!(!status.can_request_exit);
}

#[test]
fn other_session_and_untrusted_mactray_processes_block_without_offering_exit() {
    let processes = [
        LegacyTrayProcessState::TrustedOtherSession {
            session_id: 7,
            path: PathBuf::from(r"C:\Program Files\MacType\MacTray.exe"),
        },
        LegacyTrayProcessState::UntrustedSameName {
            session_id: Some(1),
            path: Some(PathBuf::from(r"C:\Temp\MacTray.exe")),
        },
    ];

    for process in processes {
        let status = LegacyTrayStatus::from_states(process, LegacyTrayStartupState::Absent);
        assert_eq!(status.conflict, LegacyTrayConflictState::Detected);
        assert!(status.blocks_machine_change());
        assert!(!status.can_request_exit);
    }
}

#[test]
fn verified_and_unknown_startup_states_project_distinct_resolution_capabilities() {
    let detected = LegacyTrayStatus::from_states(
        LegacyTrayProcessState::Absent,
        LegacyTrayStartupState::Detected {
            entries: vec![LegacyTrayStartupEntry {
                source_kind: LegacyTrayStartupSource::CurrentUserRun64,
                display_name: "MacTypeTray".to_owned(),
                target_path: PathBuf::from(r"C:\Program Files\MacType\MacTray.exe"),
            }],
        },
    );
    assert_eq!(detected.conflict, LegacyTrayConflictState::Detected);
    assert!(detected.can_disable_startup);

    let unknown = LegacyTrayStatus::from_states(
        LegacyTrayProcessState::Absent,
        LegacyTrayStartupState::Unknown {
            error: mactype_service_contract::StructuredServiceError {
                code: "legacy-tray-startup-query-failed".to_owned(),
                message: "startup inventory unavailable".to_owned(),
                win32_error: Some(5),
            },
        },
    );
    assert_eq!(unknown.conflict, LegacyTrayConflictState::Unknown);
    assert!(!unknown.can_disable_startup);
}

#[test]
fn process_inventory_ignores_non_mactray_images_and_the_session_zero_service_host() {
    let ignored_error = mactype_service_contract::StructuredServiceError {
        code: "must-not-inspect".to_owned(),
        message: "ignored candidates must not affect tray mode".to_owned(),
        win32_error: None,
    };
    let state = classify_tray_process_inventory(
        1,
        vec![
            LegacyTrayProcessObservation {
                image_name: "mactype-service.exe".to_owned(),
                pid: 100,
                session_id: 1,
                identity: Err(ignored_error.clone()),
            },
            LegacyTrayProcessObservation {
                image_name: "MacTray.exe".to_owned(),
                pid: 101,
                session_id: 0,
                identity: Err(ignored_error),
            },
        ],
    );

    assert_eq!(state, LegacyTrayProcessState::Absent);
}

#[test]
fn process_inventory_preserves_a_trusted_current_session_identity() {
    let path = PathBuf::from(r"C:\Program Files\MacType\MacTray.exe");
    let state = classify_tray_process_inventory(
        3,
        vec![LegacyTrayProcessObservation {
            image_name: "mactray.EXE".to_owned(),
            pid: 4242,
            session_id: 3,
            identity: Ok(LegacyTrayProcessIdentity {
                creation_time: 987,
                session_id: 3,
                path: path.clone(),
                trusted_path: true,
            }),
        }],
    );

    assert_eq!(
        state,
        LegacyTrayProcessState::TrustedCurrentSession {
            pid: 4242,
            creation_time: 987,
            path,
        }
    );
}

#[test]
fn multiple_interactive_mactray_processes_fail_closed() {
    let observations = [1_u32, 2_u32]
        .into_iter()
        .map(|pid| LegacyTrayProcessObservation {
            image_name: "MacTray.exe".to_owned(),
            pid,
            session_id: 3,
            identity: Ok(LegacyTrayProcessIdentity {
                creation_time: u64::from(pid),
                session_id: 3,
                path: PathBuf::from(r"C:\Program Files\MacType\MacTray.exe"),
                trusted_path: true,
            }),
        })
        .collect();

    let state = classify_tray_process_inventory(3, observations);
    let LegacyTrayProcessState::Unknown { error } = state else {
        panic!("multiple interactive MacTray processes must be unknown");
    };
    assert_eq!(error.code, "legacy-tray-process-multiple");
}

struct FakeTrayExitBackend {
    observations: std::collections::VecDeque<LegacyTrayProcessState>,
    outcome: LegacyTrayExitOutcome,
    official_exit_requests: usize,
}

impl LegacyTrayExitBackend for FakeTrayExitBackend {
    fn observe_process(&mut self) -> LegacyTrayProcessState {
        self.observations
            .pop_front()
            .expect("the exit contract observed the process too many times")
    }

    fn request_official_exit(
        &mut self,
        _expected: &LegacyTrayExitRequest,
    ) -> Result<LegacyTrayExitOutcome, String> {
        self.official_exit_requests += 1;
        Ok(self.outcome)
    }
}

fn trusted_tray_process(pid: u32, creation_time: u64, path: &str) -> LegacyTrayProcessState {
    LegacyTrayProcessState::TrustedCurrentSession {
        pid,
        creation_time,
        path: PathBuf::from(path),
    }
}

#[test]
fn graceful_exit_revalidates_the_exact_observed_identity_and_confirms_absence() {
    let request = LegacyTrayExitRequest {
        pid: 4242,
        creation_time: 987,
        path: PathBuf::from(r"C:\Program Files\MacType\MacTray.exe"),
    };
    let mut backend = FakeTrayExitBackend {
        observations: [
            trusted_tray_process(4242, 987, r"C:\Program Files\MacType\MacTray.exe"),
            trusted_tray_process(4242, 987, r"C:\Program Files\MacType\MacTray.exe"),
            LegacyTrayProcessState::Absent,
        ]
        .into(),
        outcome: LegacyTrayExitOutcome::Exited,
        official_exit_requests: 0,
    };

    request_tray_exit_with(&mut backend, &request).unwrap();

    assert_eq!(backend.official_exit_requests, 1);
    assert!(backend.observations.is_empty());
}

#[test]
fn pid_reuse_or_identity_change_never_reaches_the_exit_protocol() {
    let request = LegacyTrayExitRequest {
        pid: 4242,
        creation_time: 987,
        path: PathBuf::from(r"C:\Program Files\MacType\MacTray.exe"),
    };
    for changed in [
        trusted_tray_process(4242, 988, r"C:\Program Files\MacType\MacTray.exe"),
        trusted_tray_process(4243, 987, r"C:\Program Files\MacType\MacTray.exe"),
        trusted_tray_process(4242, 987, r"C:\Program Files\MacType-old\MacTray.exe"),
        LegacyTrayProcessState::TrustedOtherSession {
            session_id: 7,
            path: PathBuf::from(r"C:\Program Files\MacType\MacTray.exe"),
        },
    ] {
        let mut backend = FakeTrayExitBackend {
            observations: [
                trusted_tray_process(4242, 987, r"C:\Program Files\MacType\MacTray.exe"),
                changed,
            ]
            .into(),
            outcome: LegacyTrayExitOutcome::Exited,
            official_exit_requests: 0,
        };

        assert!(request_tray_exit_with(&mut backend, &request).is_err());
        assert_eq!(backend.official_exit_requests, 0);
    }
}

#[test]
fn graceful_exit_timeout_is_terminal_and_has_no_force_or_retry_path() {
    let request = LegacyTrayExitRequest {
        pid: 4242,
        creation_time: 987,
        path: PathBuf::from(r"C:\Program Files\MacType\MacTray.exe"),
    };
    let mut backend = FakeTrayExitBackend {
        observations: [
            trusted_tray_process(4242, 987, r"C:\Program Files\MacType\MacTray.exe"),
            trusted_tray_process(4242, 987, r"C:\Program Files\MacType\MacTray.exe"),
        ]
        .into(),
        outcome: LegacyTrayExitOutcome::TimedOut,
        official_exit_requests: 0,
    };

    let error = request_tray_exit_with(&mut backend, &request).unwrap_err();

    assert!(error.contains("timed out"));
    assert_eq!(backend.official_exit_requests, 1);
    assert!(backend.observations.is_empty());
}

#[test]
fn unavailable_official_exit_protocol_is_terminal_without_retry() {
    let request = LegacyTrayExitRequest {
        pid: 4242,
        creation_time: 987,
        path: PathBuf::from(r"C:\Program Files\MacType\MacTray.exe"),
    };
    let mut backend = FakeTrayExitBackend {
        observations: [
            trusted_tray_process(4242, 987, r"C:\Program Files\MacType\MacTray.exe"),
            trusted_tray_process(4242, 987, r"C:\Program Files\MacType\MacTray.exe"),
        ]
        .into(),
        outcome: LegacyTrayExitOutcome::ProtocolUnavailable,
        official_exit_requests: 0,
    };

    let error = request_tray_exit_with(&mut backend, &request).unwrap_err();

    assert!(error.contains("protocol is unavailable"), "{error}");
    assert_eq!(backend.official_exit_requests, 1);
    assert!(backend.observations.is_empty());
}

#[test]
fn process_creation_time_uses_an_exact_decimal_string_across_the_webview_boundary() {
    let creation_time = 133_967_890_123_456_789_u64;
    let serialized = serde_json::to_value(trusted_tray_process(
        4242,
        creation_time,
        r"C:\Program Files\MacType\MacTray.exe",
    ))
    .unwrap();
    assert_eq!(
        serialized
            .get("creationTime")
            .and_then(serde_json::Value::as_str),
        Some("133967890123456789")
    );

    let request: LegacyTrayExitRequest = serde_json::from_value(serde_json::json!({
        "pid": 4242,
        "creationTime": "133967890123456789",
        "path": r"C:\Program Files\MacType\MacTray.exe"
    }))
    .unwrap();
    assert_eq!(request.creation_time, creation_time);
}

#[test]
fn startup_command_ownership_requires_an_unambiguous_exact_tray_target() {
    let expected = Path::new(r"C:\Program Files\MacType\MacTray.exe");
    assert_eq!(
        classify_startup_command(r#""C:\Program Files\MacType\MacTray.exe""#, expected,),
        StartupTargetClassification::Owned(expected.to_path_buf())
    );
    for command in [
        r"C:\Program Files\MacType\MacTray.exe",
        r#""C:\Program Files\MacType\MacTray.exe" -service"#,
        r#""C:\Program Files\MacType\MacTray.exe" unexpected"#,
        r#""C:\Temp\MacTray.exe""#,
        r"MacTray.exe",
    ] {
        assert_eq!(
            classify_startup_command(command, expected),
            StartupTargetClassification::Untrusted
        );
    }
}

#[test]
fn startup_candidate_filter_does_not_confuse_control_center_autostart_with_mactray() {
    assert!(!is_legacy_tray_startup_candidate(
        "MacTypeControlCenter",
        r#""C:\Program Files\MacType Control Center\MacType Control Center.exe""#,
    ));
    assert!(is_legacy_tray_startup_candidate(
        "MacType",
        r#""C:\Program Files\MacType\MacTray.exe""#,
    ));
    assert!(is_legacy_tray_startup_candidate(
        "MacTray",
        r#""C:\Temp\renamed.exe""#,
    ));
}

#[test]
fn only_current_user_startup_sources_are_bound_to_the_invoking_user_sid() {
    for source in [
        LegacyTrayStartupSource::CurrentUserRun32,
        LegacyTrayStartupSource::CurrentUserRun64,
        LegacyTrayStartupSource::CurrentUserStartup,
    ] {
        assert!(startup_source_requires_current_user_sid(source));
    }
    for source in [
        LegacyTrayStartupSource::LocalMachineRun32,
        LegacyTrayStartupSource::LocalMachineRun64,
    ] {
        assert!(!startup_source_requires_current_user_sid(source));
    }
}

fn startup_artifact(source_kind: LegacyTrayStartupSource) -> LegacyTrayStartupArtifact {
    LegacyTrayStartupArtifact {
        entry: LegacyTrayStartupEntry {
            source_kind,
            display_name: "MacTypeTray".to_owned(),
            target_path: PathBuf::from(r"C:\Program Files\MacType\MacTray.exe"),
        },
        locator: LegacyTrayStartupLocator::Registry {
            hive: if matches!(
                source_kind,
                LegacyTrayStartupSource::LocalMachineRun32
                    | LegacyTrayStartupSource::LocalMachineRun64
            ) {
                "HKLM".to_owned()
            } else {
                "HKCU".to_owned()
            },
            view: if matches!(
                source_kind,
                LegacyTrayStartupSource::CurrentUserRun32
                    | LegacyTrayStartupSource::LocalMachineRun32
            ) {
                32
            } else {
                64
            },
            subkey: r"Software\Microsoft\Windows\CurrentVersion\Run".to_owned(),
            value_name: "MacTypeTray".to_owned(),
            value_type: 1,
        },
        raw_bytes: vec![0xff, 0xfe, 0x41, 0x00, 0x00, 0x00],
        normalized_target_path: PathBuf::from(r"C:\Program Files\MacType\MacTray.exe"),
        user_sid: "S-1-5-21-1000".to_owned(),
        recorded_at: 1234,
    }
}

#[test]
fn startup_inventory_preserves_all_fixed_sources_and_distinguishes_unknown_from_untrusted() {
    let sources = [
        LegacyTrayStartupSource::CurrentUserRun32,
        LegacyTrayStartupSource::CurrentUserRun64,
        LegacyTrayStartupSource::LocalMachineRun32,
        LegacyTrayStartupSource::LocalMachineRun64,
        LegacyTrayStartupSource::CurrentUserStartup,
    ];
    let detected = classify_startup_inventory(
        sources
            .into_iter()
            .map(|source| LegacyTrayStartupObservation::Owned(startup_artifact(source)))
            .collect(),
    );
    let LegacyTrayStartupState::Detected { entries } = detected else {
        panic!("owned fixed-source inventory must be detected");
    };
    assert_eq!(entries.len(), sources.len());
    for source in sources {
        assert!(entries.iter().any(|entry| entry.source_kind == source));
    }

    let untrusted = classify_startup_inventory(vec![LegacyTrayStartupObservation::Untrusted(
        LegacyTrayStartupEntry {
            source_kind: LegacyTrayStartupSource::CurrentUserRun64,
            display_name: "MacTypeTray".to_owned(),
            target_path: PathBuf::from(r"C:\Temp\MacTray.exe"),
        },
    )]);
    assert!(matches!(
        untrusted,
        LegacyTrayStartupState::Untrusted { .. }
    ));

    let unknown = classify_startup_inventory(vec![LegacyTrayStartupObservation::Unknown(
        mactype_service_contract::StructuredServiceError {
            code: "legacy-tray-startup-query-failed".to_owned(),
            message: "registry view is inaccessible".to_owned(),
            win32_error: Some(5),
        },
    )]);
    assert!(matches!(unknown, LegacyTrayStartupState::Unknown { .. }));
}

#[derive(Default)]
struct RecordingStartupDisable {
    events: Vec<StartupMutationEvent>,
    artifact: Option<LegacyTrayStartupArtifact>,
}

impl StartupDisableBackend for RecordingStartupDisable {
    fn observe_owned(&mut self) -> Result<Vec<LegacyTrayStartupArtifact>, String> {
        self.events.push(StartupMutationEvent::Observe);
        Ok(self.artifact.clone().into_iter().collect())
    }

    fn write_receipt(&mut self, _entries: &[LegacyTrayStartupArtifact]) -> Result<(), String> {
        self.events.push(StartupMutationEvent::WriteReceipt);
        Ok(())
    }

    fn read_verified_receipt(&mut self) -> Result<Vec<LegacyTrayStartupArtifact>, String> {
        self.events.push(StartupMutationEvent::ReadReceipt);
        Ok(self.artifact.clone().into_iter().collect())
    }

    fn remove_exact(&mut self, _entries: &[LegacyTrayStartupArtifact]) -> Result<(), String> {
        self.events.push(StartupMutationEvent::Remove);
        self.artifact = None;
        Ok(())
    }
}

#[test]
fn startup_disable_rechecks_and_verifies_the_receipt_before_removal() {
    let mut backend = RecordingStartupDisable {
        events: Vec::new(),
        artifact: Some(startup_artifact(LegacyTrayStartupSource::CurrentUserRun64)),
    };

    disable_startup_with(&mut backend).unwrap();

    assert_eq!(
        backend.events,
        [
            StartupMutationEvent::Observe,
            StartupMutationEvent::WriteReceipt,
            StartupMutationEvent::ReadReceipt,
            StartupMutationEvent::Observe,
            StartupMutationEvent::Remove,
            StartupMutationEvent::Observe,
        ]
    );
}

struct TimestampChangingStartupDisable {
    observations: std::collections::VecDeque<Vec<LegacyTrayStartupArtifact>>,
    receipt: Option<Vec<LegacyTrayStartupArtifact>>,
}

impl StartupDisableBackend for TimestampChangingStartupDisable {
    fn observe_owned(&mut self) -> Result<Vec<LegacyTrayStartupArtifact>, String> {
        self.observations
            .pop_front()
            .ok_or_else(|| "unexpected observation".to_owned())
    }

    fn write_receipt(&mut self, entries: &[LegacyTrayStartupArtifact]) -> Result<(), String> {
        self.receipt = Some(entries.to_vec());
        Ok(())
    }

    fn read_verified_receipt(&mut self) -> Result<Vec<LegacyTrayStartupArtifact>, String> {
        self.receipt
            .clone()
            .ok_or_else(|| "missing receipt".to_owned())
    }

    fn remove_exact(&mut self, _entries: &[LegacyTrayStartupArtifact]) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn startup_disable_compares_artifact_content_without_capture_timestamp_noise() {
    let original = startup_artifact(LegacyTrayStartupSource::CurrentUserRun64);
    let mut recaptured = original.clone();
    recaptured.recorded_at += 1;
    let mut backend = TimestampChangingStartupDisable {
        observations: [vec![original], vec![recaptured], Vec::new()].into(),
        receipt: None,
    };

    disable_startup_with(&mut backend).unwrap();
}

#[test]
fn startup_receipt_round_trips_raw_bytes_and_preserves_user_midflight_changes() {
    let artifact = startup_artifact(LegacyTrayStartupSource::CurrentUserRun64);
    let encoded = serde_json::to_vec(&artifact).unwrap();
    let decoded: LegacyTrayStartupArtifact = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, artifact);

    assert_eq!(
        plan_startup_restore(&artifact.raw_bytes, Some(&artifact.raw_bytes)).unwrap(),
        StartupRestoreAction::Noop
    );
    assert_eq!(
        plan_startup_restore(&artifact.raw_bytes, None).unwrap(),
        StartupRestoreAction::Restore
    );
    let changed = vec![1, 2, 3, 4];
    assert!(plan_startup_restore(&artifact.raw_bytes, Some(&changed)).is_err());
}
