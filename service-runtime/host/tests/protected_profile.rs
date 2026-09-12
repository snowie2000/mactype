use std::fs;

use mactype_service_contract::{
    ComponentReadiness, GenerationPointer, MachinePaths, ProfileCatalog, SourceMetadata,
};
use mactype_service_host::{
    ProtectedProfileInitializer, RuntimeInitializer, ACTIVE_PROFILE_ABSENT_CODE,
    RUNTIME_PROFILE_ABSENT_CODE,
};

fn paths() -> (tempfile::TempDir, MachinePaths) {
    let base = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let program_files = base.path().join("Program Files");
    let program_data = base.path().join("ProgramData");
    fs::create_dir_all(&program_files).unwrap();
    fs::create_dir_all(&program_data).unwrap();
    (
        base,
        MachinePaths::from_trusted_os_roots(&program_files, &program_data).unwrap(),
    )
}

fn install_active_runtime(paths: &MachinePaths, profile_bytes: &[u8]) -> std::path::PathBuf {
    let runtime = paths.runtime_versions().join("0.2.0");
    fs::create_dir_all(&runtime).unwrap();
    for name in [
        "mactype-service.exe",
        "mactype-injector32.exe",
        "mactype-injector64.exe",
        "MacType.dll",
        "MacType64.dll",
    ] {
        fs::write(runtime.join(name), name.as_bytes()).unwrap();
    }
    fs::write(runtime.join("MacType.ini"), profile_bytes).unwrap();
    fs::create_dir_all(paths.runtime_pointer().parent().unwrap()).unwrap();
    fs::write(
        paths.runtime_pointer(),
        br#"{"schema":1,"version":"0.2.0"}"#,
    )
    .unwrap();
    runtime
}

fn install_active_profile(paths: &MachinePaths, bytes: &[u8]) {
    let mut catalog = ProfileCatalog::new();
    let generation = catalog
        .publish_machine_profile(
            bytes,
            SourceMetadata {
                display_name: "bounded host input".to_owned(),
            },
        )
        .unwrap();
    let directory = paths
        .profile_generations()
        .join(generation.directory_name());
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("profile.ini"), bytes).unwrap();
    fs::create_dir_all(paths.active_profile().parent().unwrap()).unwrap();
    fs::write(
        paths.active_profile(),
        serde_json::to_vec(&GenerationPointer::new(generation)).unwrap(),
    )
    .unwrap();
}

#[test]
fn initializer_rejects_an_oversized_active_profile_pointer_before_parsing() {
    let (_base, paths) = paths();
    fs::create_dir_all(paths.active_profile().parent().unwrap()).unwrap();
    fs::write(paths.active_profile(), vec![b'x'; 64 * 1024 + 1]).unwrap();

    let error = ProtectedProfileInitializer::new(paths)
        .initialize()
        .err()
        .expect("oversized active profile pointer must fail initialization");

    assert_eq!(error.code, "active-profile-invalid");
    assert!(error.message.contains("bounded regular file"));
}

#[test]
fn initializer_rejects_an_oversized_runtime_pointer_before_parsing() {
    let (_base, paths) = paths();
    let bytes = b"[General]\r\nHintingMode=0\r\n";
    install_active_profile(&paths, bytes);
    fs::create_dir_all(paths.runtime_pointer().parent().unwrap()).unwrap();
    fs::write(paths.runtime_pointer(), vec![b'x'; 64 * 1024 + 1]).unwrap();

    let error = ProtectedProfileInitializer::new(paths)
        .initialize()
        .err()
        .expect("oversized runtime pointer must fail initialization");

    assert_eq!(error.code, "active-runtime-invalid");
    assert!(error.message.contains("bounded regular file"));
}

#[test]
fn initializer_reports_the_verified_protected_active_profile_digest() {
    let (_base, paths) = paths();
    let bytes = b"[General]\r\nHintingMode=0\r\n";
    let mut catalog = ProfileCatalog::new();
    let generation = catalog
        .publish_machine_profile(
            bytes,
            SourceMetadata {
                display_name: "test".to_owned(),
            },
        )
        .unwrap();
    let directory = paths
        .profile_generations()
        .join(generation.directory_name());
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("profile.ini"), bytes).unwrap();
    fs::create_dir_all(paths.active_profile().parent().unwrap()).unwrap();
    fs::write(
        paths.active_profile(),
        serde_json::to_vec(&GenerationPointer::new(generation.clone())).unwrap(),
    )
    .unwrap();
    install_active_runtime(&paths, bytes);

    let initialized = ProtectedProfileInitializer::new(paths.clone())
        .initialize()
        .unwrap();
    assert_eq!(
        initialized.active_profile_digest.as_deref(),
        Some(generation.as_str())
    );
    assert_eq!(initialized.readiness.profile, ComponentReadiness::Ready);
    assert_eq!(
        initialized.readiness.observer,
        ComponentReadiness::NotRequired
    );

    fs::write(
        directory.join("profile.ini"),
        b"[General]\r\nHintingMode=1\r\n",
    )
    .unwrap();
    let error = ProtectedProfileInitializer::new(paths)
        .initialize()
        .err()
        .expect("tampered profile must fail initialization");
    assert_eq!(error.code, "active-profile-tampered");
}

#[test]
fn initializer_reports_an_absent_generated_runtime_profile() {
    let (_base, paths) = paths();
    let bytes = b"[General]\r\nHintingMode=0\r\n";
    install_active_profile(&paths, bytes);
    let runtime = install_active_runtime(&paths, bytes);
    fs::remove_file(runtime.join("MacType.ini")).unwrap();

    let error = ProtectedProfileInitializer::new(paths)
        .initialize()
        .err()
        .expect("the supported stopped runtime has no generated profile");

    assert_eq!(error.code, RUNTIME_PROFILE_ABSENT_CODE);
}

#[test]
fn initializer_rejects_a_dll_adjacent_profile_that_differs_from_the_active_generation() {
    let (_base, paths) = paths();
    let bytes = b"[General]\r\nHintingMode=0\r\n";
    let mut catalog = ProfileCatalog::new();
    let generation = catalog
        .publish_machine_profile(
            bytes,
            SourceMetadata {
                display_name: "test".to_owned(),
            },
        )
        .unwrap();
    let directory = paths
        .profile_generations()
        .join(generation.directory_name());
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("profile.ini"), bytes).unwrap();
    fs::create_dir_all(paths.active_profile().parent().unwrap()).unwrap();
    fs::write(
        paths.active_profile(),
        serde_json::to_vec(&GenerationPointer::new(generation)).unwrap(),
    )
    .unwrap();
    install_active_runtime(&paths, b"[General]\r\nHintingMode=1\r\n");

    let error = ProtectedProfileInitializer::new(paths)
        .initialize()
        .err()
        .expect("mismatched runtime profile must fail initialization");

    assert_eq!(error.code, "runtime-profile-mismatch");
}

#[test]
fn initializer_refuses_ready_while_a_durable_activation_recovery_is_pending() {
    let (_base, paths) = paths();
    let bytes = b"[General]\r\nHintingMode=0\r\n";
    let mut catalog = ProfileCatalog::new();
    let generation = catalog
        .publish_machine_profile(
            bytes,
            SourceMetadata {
                display_name: "test".to_owned(),
            },
        )
        .unwrap();
    let directory = paths
        .profile_generations()
        .join(generation.directory_name());
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("profile.ini"), bytes).unwrap();
    fs::create_dir_all(paths.active_profile().parent().unwrap()).unwrap();
    fs::write(
        paths.active_profile(),
        serde_json::to_vec(&GenerationPointer::new(generation)).unwrap(),
    )
    .unwrap();
    install_active_runtime(&paths, bytes);
    fs::write(paths.profile_activation_journal(), b"pending").unwrap();

    let error = ProtectedProfileInitializer::new(paths.clone())
        .initialize()
        .err()
        .expect("pending activation recovery must prevent Ready");
    assert_eq!(error.code, "activation-recovery-required");

    fs::remove_file(paths.profile_activation_journal()).unwrap();
    fs::write(paths.runtime_activation_journal(), b"pending").unwrap();
    let error = ProtectedProfileInitializer::new(paths)
        .initialize()
        .err()
        .expect("pending runtime recovery must prevent Ready");
    assert_eq!(error.code, "activation-recovery-required");
}

#[test]
fn initializer_refuses_a_runtime_activation_receipt_for_a_different_candidate() {
    let (_base, paths) = paths();
    let bytes = b"[General]\r\nHintingMode=0\r\n";
    install_active_profile(&paths, bytes);
    install_active_runtime(&paths, bytes);
    fs::write(
        paths.runtime_activation_journal(),
        br#"{"schema":3,"phase":"committed","previous":null,"activated":{"schema":1,"version":"0.3.0"}}"#,
    )
    .unwrap();

    let error = ProtectedProfileInitializer::new(paths)
        .initialize()
        .err()
        .expect("a receipt for a different runtime candidate must prevent Ready");

    assert_eq!(error.code, "activation-recovery-required");
}

#[test]
fn initializer_refuses_a_stale_matching_receipt_without_a_durable_commit_phase() {
    let (_base, paths) = paths();
    let bytes = b"[General]\r\nHintingMode=0\r\n";
    install_active_profile(&paths, bytes);
    install_active_runtime(&paths, bytes);
    fs::write(
        paths.runtime_activation_journal(),
        br#"{"schema":2,"previous":null,"activated":{"schema":1,"version":"0.2.0"}}"#,
    )
    .unwrap();

    let error = ProtectedProfileInitializer::new(paths)
        .initialize()
        .err()
        .expect("a stale matching receipt without an explicit commit must prevent Ready");

    assert_eq!(error.code, "activation-recovery-required");
}

#[test]
fn initializer_refuses_legacy_uncommitted_and_rollback_required_receipts() {
    let (_base, paths) = paths();
    let bytes = b"[General]\r\nHintingMode=0\r\n";
    install_active_profile(&paths, bytes);
    install_active_runtime(&paths, bytes);

    for receipt in [
        br#"{"schema":1,"previous":null}"#.as_slice(),
        br#"{"schema":3,"phase":"candidate","previous":null,"activated":{"schema":1,"version":"0.2.0"}}"#
            .as_slice(),
        br#"{"schema":3,"phase":"rollback-required","previous":null,"activated":{"schema":1,"version":"0.2.0"}}"#
            .as_slice(),
    ] {
        fs::write(paths.runtime_activation_journal(), receipt).unwrap();
        let error = ProtectedProfileInitializer::new(paths.clone())
            .initialize()
            .err()
            .expect("legacy and uncommitted activation receipts must prevent Ready");
        assert_eq!(error.code, "activation-recovery-required");
    }
}

#[test]
fn initializer_does_not_claim_ready_without_an_active_generation() {
    let (_base, paths) = paths();
    let error = ProtectedProfileInitializer::new(paths)
        .initialize()
        .err()
        .expect("missing active profile must fail initialization");
    assert_eq!(error.code, ACTIVE_PROFILE_ABSENT_CODE);
}

#[test]
fn initializer_reports_an_absent_active_profile_pointer() {
    let (_base, paths) = paths();
    install_active_runtime(
        &paths,
        b"[General]
HintingMode=0
",
    );

    let error = ProtectedProfileInitializer::new(paths)
        .initialize()
        .err()
        .expect("nothing has been published, so the start must end as a supported stop");

    assert_eq!(error.code, ACTIVE_PROFILE_ABSENT_CODE);
}

#[test]
fn initializer_keeps_a_dangling_active_profile_pointer_unavailable() {
    let (_base, paths) = paths();
    let bytes = b"[General]
HintingMode=0
";
    install_active_profile(&paths, bytes);
    install_active_runtime(&paths, bytes);
    let mut catalog = ProfileCatalog::new();
    let generation = catalog
        .publish_machine_profile(
            bytes,
            SourceMetadata {
                display_name: "test".to_owned(),
            },
        )
        .unwrap();
    fs::remove_file(
        paths
            .profile_generations()
            .join(generation.directory_name())
            .join("profile.ini"),
    )
    .unwrap();

    let error = ProtectedProfileInitializer::new(paths)
        .initialize()
        .err()
        .expect("a dangling pointer is a broken installation");

    assert_eq!(error.code, "active-profile-unavailable");
}

#[test]
fn profile_activation_journal_takes_priority_over_an_absent_active_pointer() {
    let (_base, paths) = paths();
    install_active_runtime(
        &paths,
        b"[General]
HintingMode=0
",
    );
    fs::create_dir_all(paths.profile_activation_journal().parent().unwrap()).unwrap();
    fs::write(paths.profile_activation_journal(), b"pending").unwrap();

    let error = ProtectedProfileInitializer::new(paths)
        .initialize()
        .err()
        .expect("a pending activation journal must fail closed");

    assert_eq!(error.code, "activation-recovery-required");
}
