use super::*;

fn status(presence: ServicePresence) -> LegacyServiceStatus {
    LegacyServiceStatus {
        presence,
        state: ServiceRuntimeState::Running,
        binary_path: None,
        win32_error: None,
        trusted_binary_available: true,
        registry_conflict: false,
        can_remove: false,
        can_stop: false,
    }
}

#[test]
fn foreign_legacy_service_is_rejected_before_migration() {
    let error = require_owned_legacy_service(&status(ServicePresence::Foreign)).unwrap_err();

    assert!(error.contains("owned"));
}

#[test]
fn inaccessible_and_delete_pending_services_are_rejected_before_migration() {
    for presence in [
        ServicePresence::Inaccessible,
        ServicePresence::DeletePending,
    ] {
        assert!(require_owned_legacy_service(&status(presence)).is_err());
    }
}

#[test]
fn appinit_conflict_is_rejected_before_migration() {
    let mut legacy = status(ServicePresence::Owned);
    legacy.registry_conflict = true;

    let error = require_owned_legacy_service(&legacy).unwrap_err();

    assert!(error.contains("AppInit"));
}

#[test]
fn missing_mactray_binary_only_allows_a_stopped_owned_service_to_migrate() {
    let mut legacy = status(ServicePresence::Owned);
    legacy.trusted_binary_available = false;

    let error = require_owned_legacy_service(&legacy).unwrap_err();
    assert!(error.contains("running"));
    assert!(error.contains("binary"));

    legacy.state = ServiceRuntimeState::Stopped;
    require_owned_legacy_service(&legacy).unwrap();

    legacy.state = ServiceRuntimeState::StopPending;
    assert!(require_owned_legacy_service(&legacy).is_err());
}

#[test]
fn receipt_rejects_nonempty_load_order_group_or_nonzero_tag() {
    let root = Path::new(r"C:\Program Files\MacType");
    let mut configuration = ServiceConfiguration {
        display_name: "MacType".to_owned(),
        binary_path: r#""C:\Program Files\MacType\MacTray.exe" -service"#.to_owned(),
        service_type: 0x10,
        start_type: 2,
        error_control: 1,
        load_order_group: None,
        tag_id: 0,
        account: "LocalSystem".to_owned(),
        dependencies: vec!["winmgmt".to_owned()],
    };

    validate_service_configuration(root, &configuration).unwrap();
    configuration.load_order_group = Some("NetworkProvider".to_owned());
    assert!(validate_service_configuration(root, &configuration).is_err());
    configuration.load_order_group = None;
    configuration.tag_id = 1;
    assert!(validate_service_configuration(root, &configuration).is_err());
}

#[test]
fn receipt_requires_the_exact_legacy_display_name_and_dependency_set() {
    let root = Path::new(r"C:\Program Files\MacType");
    let mut configuration = ServiceConfiguration {
        display_name: "MacType".to_owned(),
        binary_path: r#""C:\Program Files\MacType\MacTray.exe" -service"#.to_owned(),
        service_type: 0x10,
        start_type: 2,
        error_control: 1,
        load_order_group: None,
        tag_id: 0,
        account: "LocalSystem".to_owned(),
        dependencies: vec!["winmgmt".to_owned()],
    };

    configuration.display_name = "MacType Service".to_owned();
    assert!(validate_service_configuration(root, &configuration).is_err());

    configuration.display_name = "MacType".to_owned();
    configuration.dependencies.push("RpcSs".to_owned());
    assert!(validate_service_configuration(root, &configuration).is_err());
}

#[test]
fn alternative_profile_cannot_traverse_out_of_the_installation() {
    let root = Path::new(r"C:\Program Files\MacType");

    let error = contained_profile_path(root, Path::new(r"..\stolen.ini")).unwrap_err();

    assert!(error.contains("escape"));
}

#[test]
fn reparse_component_is_rejected_before_reading_a_profile() {
    let root = Path::new(r"C:\Program Files\MacType");
    let profile = root.join("ini").join("Community.ini");

    let error = validate_path_chain(root, &profile, |path| Ok(path.ends_with(Path::new("ini"))))
        .unwrap_err();

    assert!(error.contains("reparse"));
}

#[test]
fn modified_backup_is_rejected_by_length_and_sha256() {
    let original = b"[General]\r\nAlternativeFile=ini\\Default.ini\r\n";
    let receipt = BackupFileReceipt {
        role: BackupRole::Configuration,
        original_path: r"C:\Program Files\MacType\MacType.ini".to_owned(),
        backup_file: CONFIGURATION_BACKUP.to_owned(),
        byte_length: original.len() as u64,
        sha256: hex_sha256(original),
    };

    let error = validate_backup_bytes(&receipt, b"[General]\r\nTampered=1\r\n").unwrap_err();

    assert!(error.contains("integrity"));
}

#[test]
fn absent_profile_is_a_versioned_first_class_receipt_state() {
    let receipt = ProfileBackupReceipt::Absent {
        role: BackupRole::ConfigurationAndActiveProfile,
        original_path: r"C:\Program Files\MacType\MacType.ini".to_owned(),
    };

    let encoded = serde_json::to_vec(&receipt).unwrap();
    let decoded: ProfileBackupReceipt = serde_json::from_slice(&encoded).unwrap();

    assert_eq!(decoded.role(), BackupRole::ConfigurationAndActiveProfile);
    assert!(matches!(decoded, ProfileBackupReceipt::Absent { .. }));
}

#[test]
fn rollback_never_deletes_a_profile_that_was_originally_absent() {
    let path = Path::new(r"C:\Program Files\MacType\MacType.ini");

    ensure_absent_restore_target_with(path, |_| Ok(false)).unwrap();
    let error = ensure_absent_restore_target_with(path, |_| Ok(true)).unwrap_err();

    assert!(error.contains("cleanup is unknown"), "{error}");
    assert!(error.contains("refusing to delete"), "{error}");
}

#[test]
fn every_migration_artifact_rejects_a_reparse_point_opened_handle() {
    let root = Path::new(r"C:\ProgramData\MacType\ControlCenter\legacy-migration");
    let generation = root.join("migration-123-456");
    let artifacts = [
        root.join(CURRENT_FILE),
        generation.join(RECEIPT_FILE),
        generation.join(CONFIGURATION_BACKUP),
        generation.join(SERVICE_REGISTRY_EXPORT),
    ];

    for artifact in artifacts {
        let mut opened = false;
        let error = read_bounded_under_with(
            root,
            &artifact,
            MAX_RECEIPT_BYTES,
            |_| Ok(false),
            |_| {
                opened = true;
                Ok((
                    std::io::empty(),
                    OpenedFileMetadata {
                        is_regular_file: true,
                        is_reparse_point: true,
                        byte_length: 0,
                    },
                ))
            },
        )
        .unwrap_err();

        assert!(error.contains("reparse"));
        assert!(
            opened,
            "the final path must be inspected through its handle"
        );
    }
}

#[test]
fn opened_reparse_file_is_rejected_before_the_handle_is_read() {
    struct MustNotRead;

    impl Read for MustNotRead {
        fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
            panic!("a reparse-point handle must never be read")
        }
    }

    let mut opens = 0;
    let error = read_opened_bounded_with(
        Path::new(r"C:\ProgramData\MacType\ControlCenter\legacy-migration\current.json"),
        MAX_RECEIPT_BYTES,
        |_| {
            opens += 1;
            Ok((
                MustNotRead,
                OpenedFileMetadata {
                    is_regular_file: true,
                    is_reparse_point: true,
                    byte_length: 10,
                },
            ))
        },
    )
    .unwrap_err();

    assert_eq!(opens, 1);
    assert!(error.contains("reparse"));
}

#[test]
fn migration_acl_uses_fixed_system32_tool_and_sid_grants() {
    let target = Path::new(r"C:\ProgramData\MacType\ControlCenter\legacy-migration");
    let (program, arguments) = acl_invocation(Path::new(r"C:\Windows\System32"), target);

    assert_eq!(program, PathBuf::from(r"C:\Windows\System32\icacls.exe"));
    assert_eq!(arguments[0], target.as_os_str());
    assert!(arguments.iter().any(|value| value == "*S-1-5-18:(OI)(CI)F"));
    assert!(arguments
        .iter()
        .any(|value| value == "*S-1-5-32-544:(OI)(CI)F"));
    assert!(arguments
        .iter()
        .any(|value| value == "*S-1-5-32-545:(OI)(CI)RX"));
}

#[test]
fn service_registry_export_uses_only_the_fixed_system32_contract() {
    let generation =
        Path::new(r"C:\ProgramData\MacType\ControlCenter\legacy-migration\migration-1-2");
    let export = generation.join(SERVICE_REGISTRY_EXPORT);
    let (program, arguments) =
        registry_export_invocation(Path::new(r"C:\Windows\System32"), generation);

    assert_eq!(program, PathBuf::from(r"C:\Windows\System32\reg.exe"));
    assert_eq!(
        arguments,
        [
            OsString::from("export"),
            OsString::from(r"HKLM\SYSTEM\CurrentControlSet\Services\MacType"),
            export.into_os_string(),
            OsString::from("/y"),
        ]
    );
}

#[test]
fn registry_export_integrity_rejects_empty_tampered_and_oversized_content() {
    let original = b"Windows Registry Editor Version 5.00\r\n";
    let receipt = RegistryExportReceipt {
        export_file: SERVICE_REGISTRY_EXPORT.to_owned(),
        byte_length: original.len() as u64,
        sha256: hex_sha256(original),
    };

    validate_registry_export_bytes(&receipt, original).unwrap();
    assert!(validate_registry_export_bytes(&receipt, b"").is_err());
    assert!(validate_registry_export_bytes(&receipt, b"tampered").is_err());

    let oversized = vec![0u8; MAX_REGISTRY_EXPORT_BYTES as usize + 1];
    let oversized_receipt = RegistryExportReceipt {
        export_file: SERVICE_REGISTRY_EXPORT.to_owned(),
        byte_length: oversized.len() as u64,
        sha256: hex_sha256(&oversized),
    };
    assert!(validate_registry_export_bytes(&oversized_receipt, &oversized).is_err());
}

#[test]
fn acl_failure_prevents_current_pointer_publication() {
    let generation = Path::new(r"C:\ProgramData\MacType\migration-123-456");
    let mut published = false;

    let result = after_hardening_with(
        generation,
        |_| Err("simulated ACL failure".to_owned()),
        |_| {
            published = true;
            Ok(())
        },
    );

    assert!(result.is_err());
    assert!(!published);
}

#[test]
fn registry_export_failure_prevents_current_pointer_publication() {
    let generation = Path::new(r"C:\ProgramData\MacType\migration-123-456");
    let mut published = false;

    let result = after_registry_export_with(
        generation,
        |_| Err("simulated reg.exe failure".to_owned()),
        |_, _| {
            published = true;
            Ok(())
        },
    );

    assert!(result.is_err());
    assert!(!published);
}

#[test]
fn legacy_removal_requires_ready_digest_match_and_valid_backup() {
    for verification in [
        RemovalVerification {
            new_service_ready: false,
            active_digest_match: true,
            backup_valid: true,
        },
        RemovalVerification {
            new_service_ready: true,
            active_digest_match: false,
            backup_valid: true,
        },
        RemovalVerification {
            new_service_ready: true,
            active_digest_match: true,
            backup_valid: false,
        },
    ] {
        assert!(require_removal_verification(verification, true).is_err());
    }

    require_removal_verification(
        RemovalVerification {
            new_service_ready: true,
            active_digest_match: true,
            backup_valid: true,
        },
        true,
    )
    .unwrap();

    let forged_external_result = require_removal_verification(
        RemovalVerification {
            new_service_ready: true,
            active_digest_match: true,
            backup_valid: true,
        },
        false,
    );
    assert!(forged_external_result.is_err());
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RollbackEvent {
    Stop,
    Profiles,
    Service,
    Startup,
    RunningState,
}

struct RecordingRollback {
    events: Vec<RollbackEvent>,
    fail_at: Option<RollbackEvent>,
}

impl RecordingRollback {
    fn record(&mut self, event: RollbackEvent) -> Result<(), String> {
        self.events.push(event);
        if self.fail_at == Some(event) {
            Err("simulated restore failure".to_owned())
        } else {
            Ok(())
        }
    }
}

impl RollbackBackend for RecordingRollback {
    fn stop_before_restore(&mut self) -> Result<(), String> {
        self.record(RollbackEvent::Stop)
    }

    fn restore_profiles(&mut self) -> Result<(), String> {
        self.record(RollbackEvent::Profiles)
    }

    fn restore_service_configuration(&mut self) -> Result<(), String> {
        self.record(RollbackEvent::Service)
    }

    fn restore_legacy_tray_startup(&mut self) -> Result<(), String> {
        self.record(RollbackEvent::Startup)
    }

    fn restore_running_state(&mut self) -> Result<(), String> {
        self.record(RollbackEvent::RunningState)
    }
}

#[test]
fn rollback_restores_autostart_before_the_legacy_service_can_run_again() {
    let mut backend = RecordingRollback {
        events: Vec::new(),
        fail_at: None,
    };

    perform_rollback(&mut backend).unwrap();
    assert_eq!(
        backend.events,
        [
            RollbackEvent::Stop,
            RollbackEvent::Profiles,
            RollbackEvent::Service,
            RollbackEvent::Startup,
            RollbackEvent::RunningState,
        ]
    );
}

#[test]
fn rollback_restores_profiles_before_service_and_never_starts_after_failure() {
    let mut backend = RecordingRollback {
        events: Vec::new(),
        fail_at: Some(RollbackEvent::Service),
    };

    assert!(perform_rollback(&mut backend).is_err());
    assert_eq!(
        backend.events,
        [
            RollbackEvent::Stop,
            RollbackEvent::Profiles,
            RollbackEvent::Service,
        ]
    );
}

fn receipted_startup_artifact(
    source_kind: crate::machine_integration::legacy_mactray::LegacyTrayStartupSource,
    name: &str,
) -> crate::machine_integration::legacy_mactray::LegacyTrayStartupArtifact {
    use crate::machine_integration::legacy_mactray::{
        LegacyTrayStartupArtifact, LegacyTrayStartupEntry, LegacyTrayStartupLocator,
    };
    LegacyTrayStartupArtifact {
        entry: LegacyTrayStartupEntry {
            source_kind,
            display_name: name.to_owned(),
            target_path: PathBuf::from(r"C:\Program Files\MacType\MacTray.exe"),
        },
        locator: LegacyTrayStartupLocator::Registry {
            hive: if matches!(
                source_kind,
                crate::machine_integration::legacy_mactray::LegacyTrayStartupSource::LocalMachineRun32
                    | crate::machine_integration::legacy_mactray::LegacyTrayStartupSource::LocalMachineRun64
            ) {
                "HKLM".to_owned()
            } else {
                "HKCU".to_owned()
            },
            view: if matches!(
                source_kind,
                crate::machine_integration::legacy_mactray::LegacyTrayStartupSource::CurrentUserRun32
                    | crate::machine_integration::legacy_mactray::LegacyTrayStartupSource::LocalMachineRun32
            ) {
                32
            } else {
                64
            },
            subkey: r"Software\Microsoft\Windows\CurrentVersion\Run".to_owned(),
            value_name: name.to_owned(),
            value_type: 1,
        },
        raw_bytes: vec![0xff, 0xfe, 0x41, 0x00, 0x00, 0x00],
        normalized_target_path: PathBuf::from(r"C:\Program Files\MacType\MacTray.exe"),
        user_sid: "S-1-5-21-1000".to_owned(),
        recorded_at: 1234,
    }
}

#[test]
fn startup_receipt_is_versioned_scoped_and_preserves_every_original_byte() {
    use crate::machine_integration::legacy_mactray::LegacyTrayStartupSource;
    let artifact =
        receipted_startup_artifact(LegacyTrayStartupSource::CurrentUserRun64, "MacTypeTray");
    let receipt = build_startup_receipt(
        StartupReceiptScope::CurrentUser,
        "S-1-5-21-1000",
        vec![artifact.clone()],
        5678,
    )
    .unwrap();
    assert_eq!(receipt.restoration_state, StartupRestorationState::Pending);

    let encoded = serde_json::to_vec(&receipt).unwrap();
    let decoded: LegacyTrayStartupReceipt = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, receipt);
    assert_eq!(decoded.entries[0].raw_bytes, artifact.raw_bytes);
    assert_eq!(decoded.entries[0].user_sid, "S-1-5-21-1000");
}

#[test]
fn startup_receipt_rejects_unknown_fields_at_every_nested_boundary() {
    use crate::machine_integration::legacy_mactray::LegacyTrayStartupSource;
    let receipt = build_startup_receipt(
        StartupReceiptScope::CurrentUser,
        "S-1-5-21-1000",
        vec![receipted_startup_artifact(
            LegacyTrayStartupSource::CurrentUserRun64,
            "MacTypeTray",
        )],
        5678,
    )
    .unwrap();
    let original = serde_json::to_value(receipt).unwrap();

    for path in ["artifact", "entry", "locator"] {
        let mut tampered = original.clone();
        let object = match path {
            "artifact" => tampered["entries"][0].as_object_mut().unwrap(),
            "entry" => tampered["entries"][0]["entry"].as_object_mut().unwrap(),
            "locator" => tampered["entries"][0]["locator"].as_object_mut().unwrap(),
            _ => unreachable!(),
        };
        object.insert("unexpected".to_owned(), serde_json::json!(true));

        assert!(
            serde_json::from_value::<LegacyTrayStartupReceipt>(tampered).is_err(),
            "unknown {path} field must be rejected"
        );
    }
}

#[test]
fn startup_receipt_rejects_scope_mixing_duplicate_locators_and_sid_mismatch() {
    use crate::machine_integration::legacy_mactray::LegacyTrayStartupSource;
    let current =
        receipted_startup_artifact(LegacyTrayStartupSource::CurrentUserRun64, "MacTypeTray");
    let machine =
        receipted_startup_artifact(LegacyTrayStartupSource::LocalMachineRun64, "MacTypeTray");
    assert!(build_startup_receipt(
        StartupReceiptScope::CurrentUser,
        "S-1-5-21-1000",
        vec![machine],
        5678,
    )
    .is_err());
    assert!(build_startup_receipt(
        StartupReceiptScope::CurrentUser,
        "S-1-5-21-1000",
        vec![current.clone(), current.clone()],
        5678,
    )
    .is_err());
    let mut wrong_sid = current;
    wrong_sid.user_sid = "S-1-5-21-2000".to_owned();
    assert!(build_startup_receipt(
        StartupReceiptScope::CurrentUser,
        "S-1-5-21-1000",
        vec![wrong_sid],
        5678,
    )
    .is_err());
}

#[test]
fn pending_startup_receipt_is_never_overwritten_by_a_different_observation() {
    use crate::machine_integration::legacy_mactray::LegacyTrayStartupSource;
    let original = build_startup_receipt(
        StartupReceiptScope::CurrentUser,
        "S-1-5-21-1000",
        vec![receipted_startup_artifact(
            LegacyTrayStartupSource::CurrentUserRun64,
            "MacTypeTrayA",
        )],
        5678,
    )
    .unwrap();
    let replacement = build_startup_receipt(
        StartupReceiptScope::CurrentUser,
        "S-1-5-21-1000",
        vec![receipted_startup_artifact(
            LegacyTrayStartupSource::CurrentUserRun64,
            "MacTypeTrayB",
        )],
        6789,
    )
    .unwrap();

    assert!(select_startup_receipt_for_disable(Some(&original), replacement).is_err());
}

#[test]
fn identical_recapture_reuses_the_pending_receipt_without_timestamp_replacement() {
    use crate::machine_integration::legacy_mactray::LegacyTrayStartupSource;
    let original = build_startup_receipt(
        StartupReceiptScope::CurrentUser,
        "S-1-5-21-1000",
        vec![receipted_startup_artifact(
            LegacyTrayStartupSource::CurrentUserRun64,
            "MacTypeTray",
        )],
        5678,
    )
    .unwrap();
    let mut recaptured_artifact = original.entries[0].clone();
    recaptured_artifact.recorded_at += 100;
    let proposed = build_startup_receipt(
        StartupReceiptScope::CurrentUser,
        "S-1-5-21-1000",
        vec![recaptured_artifact],
        6789,
    )
    .unwrap();

    let selected = select_startup_receipt_for_disable(Some(&original), proposed).unwrap();
    assert_eq!(selected, original);
}

#[test]
fn current_user_restore_cli_accepts_one_fixed_switch_and_no_arguments() {
    use std::ffi::OsString;
    assert!(user_restore_requested_from_arguments([
        OsString::from("control-center.exe"),
        OsString::from("--restore-current-user-legacy-tray-autostart"),
    ])
    .unwrap());
    assert!(!user_restore_requested_from_arguments([
        OsString::from("control-center.exe"),
        OsString::from("--ordinary-launch"),
    ])
    .unwrap());
    assert!(user_restore_requested_from_arguments([
        OsString::from("control-center.exe"),
        OsString::from("--restore-current-user-legacy-tray-autostart"),
        OsString::from(r"HKCU\arbitrary"),
    ])
    .is_err());
}

#[derive(Default)]
struct RecordingStartupRestore {
    current: std::collections::BTreeMap<String, Option<Vec<u8>>>,
    restored: Vec<String>,
    marked: Option<StartupRestorationState>,
}

impl StartupRestoreBackend for RecordingStartupRestore {
    fn current_bytes(
        &mut self,
        artifact: &crate::machine_integration::legacy_mactray::LegacyTrayStartupArtifact,
    ) -> Result<Option<Vec<u8>>, String> {
        Ok(self
            .current
            .get(&artifact.entry.display_name)
            .cloned()
            .flatten())
    }

    fn restore_original(
        &mut self,
        artifact: &crate::machine_integration::legacy_mactray::LegacyTrayStartupArtifact,
    ) -> Result<(), String> {
        self.restored.push(artifact.entry.display_name.clone());
        Ok(())
    }

    fn mark_restoration(&mut self, state: StartupRestorationState) -> Result<(), String> {
        self.marked = Some(state);
        Ok(())
    }
}

#[test]
fn startup_restore_preflights_every_entry_and_never_overwrites_a_user_change() {
    use crate::machine_integration::legacy_mactray::LegacyTrayStartupSource;
    let first =
        receipted_startup_artifact(LegacyTrayStartupSource::CurrentUserRun64, "MacTypeTrayA");
    let second =
        receipted_startup_artifact(LegacyTrayStartupSource::CurrentUserRun32, "MacTypeTrayB");
    let receipt = build_startup_receipt(
        StartupReceiptScope::CurrentUser,
        "S-1-5-21-1000",
        vec![first, second],
        5678,
    )
    .unwrap();
    let mut backend = RecordingStartupRestore::default();
    backend.current.insert("MacTypeTrayA".to_owned(), None);
    backend
        .current
        .insert("MacTypeTrayB".to_owned(), Some(vec![1, 2, 3]));

    assert!(restore_startup_with(&mut backend, &receipt).is_err());
    assert!(backend.restored.is_empty());
    assert_eq!(
        backend.marked,
        Some(StartupRestorationState::ManualRequired)
    );
}

#[test]
fn startup_restore_recreates_only_receipted_absent_entries_then_marks_restored() {
    use crate::machine_integration::legacy_mactray::LegacyTrayStartupSource;
    let receipt = build_startup_receipt(
        StartupReceiptScope::CurrentUser,
        "S-1-5-21-1000",
        vec![
            receipted_startup_artifact(LegacyTrayStartupSource::CurrentUserRun64, "MacTypeTrayA"),
            receipted_startup_artifact(LegacyTrayStartupSource::CurrentUserRun32, "MacTypeTrayB"),
        ],
        5678,
    )
    .unwrap();
    let mut backend = RecordingStartupRestore::default();
    backend.current.insert("MacTypeTrayA".to_owned(), None);
    backend.current.insert("MacTypeTrayB".to_owned(), None);

    restore_startup_with(&mut backend, &receipt).unwrap();

    assert_eq!(backend.restored, ["MacTypeTrayA", "MacTypeTrayB"]);
    assert_eq!(backend.marked, Some(StartupRestorationState::Restored));
}
