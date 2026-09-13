use std::fs;
#[cfg(feature = "ci-test-adapter")]
use std::panic::{catch_unwind, AssertUnwindSafe};

use mactype_service_contract::{
    GenerationPointer, MachinePaths, ProfileCatalog, SourceMetadata, MAX_PROFILE_BYTES,
};
use mactype_service_setup::{ProfileStore, SetupError};

fn profile(gamma: &str) -> Vec<u8> {
    format!("[General]\r\nGammaValue={gamma}\r\n").into_bytes()
}

fn test_paths() -> (tempfile::TempDir, MachinePaths) {
    let base = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let program_files = base.path().join("Program Files");
    let program_data = base.path().join("ProgramData");
    fs::create_dir_all(&program_files).unwrap();
    fs::create_dir_all(&program_data).unwrap();
    let paths = MachinePaths::from_trusted_os_roots(&program_files, &program_data).unwrap();
    (base, paths)
}

fn source(name: &str) -> SourceMetadata {
    SourceMetadata {
        display_name: name.to_owned(),
    }
}

fn read_pointer(path: &std::path::Path) -> GenerationPointer {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn install_active_runtime(paths: &MachinePaths, version: &str) -> std::path::PathBuf {
    let runtime = paths.runtime_versions().join(version);
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
    fs::create_dir_all(paths.runtime_pointer().parent().unwrap()).unwrap();
    fs::write(
        paths.runtime_pointer(),
        serde_json::to_vec(&serde_json::json!({
            "schema": 1,
            "version": version,
        }))
        .unwrap(),
    )
    .unwrap();
    runtime
}

#[test]
fn bootstrap_preflight_refuses_a_pending_profile_transaction_without_recovering_it() {
    let (_base, paths) = test_paths();
    fs::create_dir_all(paths.profile_activation_journal().parent().unwrap()).unwrap();
    let pending = b"pending-profile-transaction";
    fs::write(paths.profile_activation_journal(), pending).unwrap();
    let store = ProfileStore::new(paths.clone());

    let error = store.inspect_active_generation_stable().unwrap_err();

    assert!(error.to_string().contains("profile transaction is pending"));
    assert_eq!(
        fs::read(paths.profile_activation_journal()).unwrap(),
        pending
    );
}

#[test]
fn publishing_active_profile_materializes_exact_bytes_beside_active_runtime_dlls() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths);
    let bytes = b"[General]\r\nFontName=\x81\x40\r\n".to_vec();

    store
        .publish_and_activate(&bytes, source("materialized"))
        .unwrap();

    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), bytes);
}

#[test]
fn reactivating_the_current_generation_repairs_a_missing_adjacent_profile() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths);
    let bytes = profile("1.25");
    let generation = store
        .publish_and_activate(&bytes, source("repair"))
        .unwrap();
    fs::remove_file(runtime.join("MacType.ini")).unwrap();

    assert_eq!(store.synchronize_active_runtime().unwrap(), generation);

    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), bytes);
}

#[test]
fn synchronization_removes_an_interrupted_generated_profile_temporary_file() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths);
    let bytes = profile("1.25");
    store
        .publish_and_activate(&bytes, source("interrupted atomic write"))
        .unwrap();
    let stale = runtime.join(".MacType.ini.new-4242-1");
    fs::write(&stale, b"partial profile bytes").unwrap();

    store.synchronize_active_runtime().unwrap();

    assert!(!stale.exists());
    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), bytes);
}

#[test]
fn generated_profile_cleanup_never_deletes_an_unrecognized_temporary_entry() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths);
    store
        .publish_and_activate(&profile("1.25"), source("unrecognized atomic write"))
        .unwrap();
    let unrelated = runtime.join(".MacType.ini.new-not-owned");
    fs::write(&unrelated, b"operator file").unwrap();

    assert!(store.synchronize_active_runtime().is_err());

    assert!(unrelated.exists());
}

#[test]
fn publishing_recovers_an_exact_stale_profile_staging_directory_after_pid_reuse() {
    let (_base, paths) = test_paths();
    let bytes = profile("1.25");
    let mut catalog = ProfileCatalog::new();
    let generation = catalog
        .publish_machine_profile(&bytes, source("stale staging"))
        .unwrap();
    fs::create_dir_all(paths.profile_generations()).unwrap();
    let stale = paths.profile_generations().join(format!(
        ".staging-{}-{}",
        generation.directory_name(),
        std::process::id()
    ));
    fs::create_dir(&stale).unwrap();
    fs::write(stale.join("profile.ini"), b"partial").unwrap();
    let unrelated = paths
        .profile_generations()
        .join(".staging-not-a-digest-not-owned");
    fs::create_dir(&unrelated).unwrap();

    ProfileStore::new(paths)
        .publish_and_activate(&bytes, source("recovered staging"))
        .unwrap();

    assert!(!stale.exists());
    assert!(unrelated.exists());
}

#[test]
fn profile_staging_cleanup_never_deletes_an_unexpected_entry() {
    let (_base, paths) = test_paths();
    let bytes = profile("1.25");
    let mut catalog = ProfileCatalog::new();
    let generation = catalog
        .publish_machine_profile(&bytes, source("untrusted staging"))
        .unwrap();
    let stale = paths.profile_generations().join(format!(
        ".staging-{}-{}",
        generation.directory_name(),
        std::process::id()
    ));
    fs::create_dir_all(&stale).unwrap();
    let unexpected = stale.join("do-not-delete.txt");
    fs::write(&unexpected, b"operator file").unwrap();

    assert!(ProfileStore::new(paths)
        .publish_and_activate(&bytes, source("must fail closed"))
        .is_err());

    assert!(unexpected.exists());
}

#[test]
fn active_profile_pointer_must_be_a_bounded_regular_file() {
    let (_base, paths) = test_paths();
    fs::create_dir_all(paths.active_profile().parent().unwrap()).unwrap();
    fs::write(paths.active_profile(), vec![b'x'; 64 * 1024 + 1]).unwrap();

    let error = ProfileStore::new(paths).active_generation().unwrap_err();

    assert!(error
        .to_string()
        .contains("profile pointer is not a bounded regular file"));
}

#[test]
fn active_runtime_pointer_must_be_a_bounded_regular_file_before_materialization() {
    let (_base, paths) = test_paths();
    install_active_runtime(&paths, "0.2.0");
    fs::write(paths.runtime_pointer(), vec![b'x'; 64 * 1024 + 1]).unwrap();

    let error = ProfileStore::new(paths)
        .publish_and_activate(&profile("1.25"), source("bounded runtime pointer"))
        .unwrap_err();

    assert!(error
        .to_string()
        .contains("active runtime pointer is not a bounded regular file"));
}

#[test]
fn rollback_restores_the_previous_profile_bytes_beside_the_active_runtime_dlls() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths);
    let first = profile("1.0");
    let second = profile("1.4");
    store.publish_and_activate(&first, source("first")).unwrap();
    store
        .publish_and_activate(&second, source("second"))
        .unwrap();

    store.rollback().unwrap();

    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), first);
}

#[test]
fn first_profile_rollback_restores_the_absent_profile_state() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths.clone());
    let generation = store
        .publish_and_activate(&profile("1.0"), source("first"))
        .unwrap();

    store.rollback().unwrap();

    assert!(!paths.active_profile().exists());
    assert!(!paths.previous_profile().exists());
    assert!(!runtime.join("MacType.ini").exists());
    assert!(paths
        .profile_generations()
        .join(generation.directory_name())
        .join("profile.ini")
        .is_file());
}

#[test]
fn first_profile_rollback_preserves_a_foreign_adjacent_profile() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths.clone());
    let generation = store
        .publish_and_activate(&profile("1.0"), source("first"))
        .unwrap();
    let foreign = profile("9.9");
    fs::write(runtime.join("MacType.ini"), &foreign).unwrap();

    let error = store.rollback().unwrap_err();
    assert!(matches!(error, SetupError::CleanupUnknown(_)));

    assert_eq!(
        read_pointer(paths.active_profile()).generation(),
        &generation
    );
    assert!(!paths.previous_profile().exists());
    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), foreign);
    assert!(!paths.profile_activation_journal().exists());
}

#[test]
fn first_profile_rollback_preserves_a_non_regular_adjacent_profile() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths.clone());
    let generation = store
        .publish_and_activate(&profile("1.0"), source("first"))
        .unwrap();
    fs::remove_file(runtime.join("MacType.ini")).unwrap();
    fs::create_dir(runtime.join("MacType.ini")).unwrap();

    let error = store.rollback().unwrap_err();
    assert!(matches!(error, SetupError::CleanupUnknown(_)));

    assert_eq!(
        read_pointer(paths.active_profile()).generation(),
        &generation
    );
    assert!(runtime.join("MacType.ini").is_dir());
    assert!(!paths.profile_activation_journal().exists());
}

#[cfg(windows)]
#[test]
fn first_profile_rollback_fails_closed_while_the_adjacent_profile_is_in_use() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths.clone());
    let generation = store
        .publish_and_activate(&profile("1.0"), source("first"))
        .unwrap();
    let _lock = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(runtime.join("MacType.ini"))
        .unwrap();

    let error = store.rollback().unwrap_err();
    assert!(matches!(error, SetupError::CleanupUnknown(_)));

    assert_eq!(
        read_pointer(paths.active_profile()).generation(),
        &generation
    );
    assert!(runtime.join("MacType.ini").is_file());
    assert!(!paths.profile_activation_journal().exists());
}

#[cfg(feature = "ci-test-adapter")]
#[test]
fn interrupted_first_profile_rollback_recovers_the_active_pointer_and_exact_bytes() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths.clone());
    let bytes = profile("1.0");
    let generation = store.publish_and_activate(&bytes, source("first")).unwrap();

    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        let _ = store.rollback_with_post_clear_hook_for_ci(|| {
            panic!("simulated power interruption after exact profile deletion");
        });
    }));
    assert!(interrupted.is_err());
    assert!(paths.profile_activation_journal().is_file());
    assert_eq!(
        read_pointer(paths.active_profile()).generation(),
        &generation
    );
    assert!(!runtime.join("MacType.ini").exists());

    assert!(store.recover_interrupted_activation().unwrap());

    assert_eq!(
        read_pointer(paths.active_profile()).generation(),
        &generation
    );
    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), bytes);
    assert!(!paths.previous_profile().exists());
    assert!(!paths.profile_activation_journal().exists());
}

#[cfg(feature = "ci-test-adapter")]
#[test]
fn uncertain_first_profile_cleanup_never_overwrites_a_later_foreign_file() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths.clone());
    let generation = store
        .publish_and_activate(&profile("1.0"), source("first"))
        .unwrap();

    let error = store
        .rollback_with_unknown_clear_for_ci()
        .expect_err("the simulated deletion result must remain uncertain");
    assert!(matches!(error, SetupError::CleanupUnknown(_)));
    assert!(paths.profile_activation_journal().is_file());

    let foreign = profile("9.9");
    fs::write(runtime.join("MacType.ini"), &foreign).unwrap();
    let recovery = store.recover_interrupted_activation().unwrap_err();

    assert!(matches!(recovery, SetupError::CleanupUnknown(_)));
    assert_eq!(
        read_pointer(paths.active_profile()).generation(),
        &generation
    );
    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), foreign);
    assert!(paths.profile_activation_journal().is_file());
}

#[cfg(feature = "ci-test-adapter")]
#[test]
fn uncertain_first_profile_cleanup_with_exact_bytes_converges_without_deleting_them() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths.clone());
    let bytes = profile("1.0");
    let generation = store.publish_and_activate(&bytes, source("first")).unwrap();

    store.rollback_with_unknown_clear_for_ci().unwrap_err();
    assert!(store.recover_interrupted_activation().unwrap());

    assert_eq!(
        read_pointer(paths.active_profile()).generation(),
        &generation
    );
    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), bytes);
    assert!(!paths.profile_activation_journal().exists());
}

#[cfg(feature = "ci-test-adapter")]
#[test]
fn uncertain_first_profile_cleanup_with_absent_path_restores_exact_bytes() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths.clone());
    let bytes = profile("1.0");
    let generation = store.publish_and_activate(&bytes, source("first")).unwrap();

    store.rollback_with_unknown_clear_for_ci().unwrap_err();
    fs::remove_file(runtime.join("MacType.ini")).unwrap();
    assert!(store.recover_interrupted_activation().unwrap());

    assert_eq!(
        read_pointer(paths.active_profile()).generation(),
        &generation
    );
    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), bytes);
    assert!(!paths.profile_activation_journal().exists());
}

#[cfg(feature = "ci-test-adapter")]
#[test]
fn uncertain_first_profile_cleanup_never_replaces_a_later_non_regular_path() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths.clone());
    let generation = store
        .publish_and_activate(&profile("1.0"), source("first"))
        .unwrap();

    store.rollback_with_unknown_clear_for_ci().unwrap_err();
    fs::remove_file(runtime.join("MacType.ini")).unwrap();
    fs::create_dir(runtime.join("MacType.ini")).unwrap();
    let recovery = store.recover_interrupted_activation().unwrap_err();

    assert!(matches!(recovery, SetupError::CleanupUnknown(_)));
    assert_eq!(
        read_pointer(paths.active_profile()).generation(),
        &generation
    );
    assert!(runtime.join("MacType.ini").is_dir());
    assert!(paths.profile_activation_journal().is_file());
}

#[test]
fn schema_one_profile_activation_journal_remains_recoverable() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths.clone());
    let bytes = profile("1.0");
    let generation = store
        .publish_and_activate(&bytes, source("legacy journal"))
        .unwrap();
    let active = read_pointer(paths.active_profile());
    fs::write(
        paths.profile_activation_journal(),
        serde_json::to_vec(&serde_json::json!({
            "schema": 1,
            "active_before": active,
            "previous_before": null,
        }))
        .unwrap(),
    )
    .unwrap();

    assert!(store.recover_interrupted_activation().unwrap());

    assert_eq!(
        read_pointer(paths.active_profile()).generation(),
        &generation
    );
    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), bytes);
    assert!(!paths.profile_activation_journal().exists());
}

#[test]
fn profile_activation_journal_rejects_unknown_fields_without_mutation() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths.clone());
    let bytes = profile("1.0");
    let generation = store
        .publish_and_activate(&bytes, source("strict journal"))
        .unwrap();
    let active = read_pointer(paths.active_profile());
    fs::write(
        paths.profile_activation_journal(),
        serde_json::to_vec(&serde_json::json!({
            "schema": 2,
            "phase": "pointer-transition",
            "active_before": active,
            "previous_before": null,
            "unexpected": true,
        }))
        .unwrap(),
    )
    .unwrap();

    assert!(store.recover_interrupted_activation().is_err());

    assert_eq!(
        read_pointer(paths.active_profile()).generation(),
        &generation
    );
    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), bytes);
    assert!(paths.profile_activation_journal().is_file());
}

#[cfg(windows)]
#[test]
fn failed_materialization_restores_profile_pointers_and_previous_adjacent_bytes() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths.clone());
    let first_bytes = profile("1.0");
    let first = store
        .publish_and_activate(&first_bytes, source("first"))
        .unwrap();
    let _replacement_lock = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(runtime.join("MacType.ini"))
        .unwrap();

    assert!(store
        .publish_and_activate(&profile("1.4"), source("second"))
        .is_err());

    assert_eq!(read_pointer(paths.active_profile()).generation(), &first);
    assert!(!paths.previous_profile().exists());
    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), first_bytes);
    let mut names = fs::read_dir(runtime)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec![
            "MacType.dll",
            "MacType.ini",
            "MacType64.dll",
            "mactype-injector32.exe",
            "mactype-injector64.exe",
            "mactype-service.exe",
        ]
    );
}

#[cfg(windows)]
#[test]
fn failed_rollback_keeps_active_previous_and_adjacent_profile_consistent() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths.clone());
    let first = store
        .publish_and_activate(&profile("1.0"), source("first"))
        .unwrap();
    let second_bytes = profile("1.4");
    let second = store
        .publish_and_activate(&second_bytes, source("second"))
        .unwrap();
    let _replacement_lock = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(runtime.join("MacType.ini"))
        .unwrap();

    assert!(store.rollback().is_err());

    assert_eq!(read_pointer(paths.active_profile()).generation(), &second);
    assert_eq!(read_pointer(paths.previous_profile()).generation(), &first);
    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), second_bytes);
}

#[cfg(feature = "ci-test-adapter")]
#[test]
fn interrupted_profile_activation_recovers_pointers_and_adjacent_bytes_before_start() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths.clone());
    let first_bytes = profile("1.0");
    let first = store
        .publish_and_activate(&first_bytes, source("first"))
        .unwrap();
    let second = store
        .publish_and_activate(&profile("1.4"), source("second"))
        .unwrap();
    store.rollback().unwrap();

    let interrupted = catch_unwind(AssertUnwindSafe(|| {
        let _ = store.activate_with_post_pointer_hook_for_ci(&second, || {
            panic!("simulated power interruption before profile materialization");
        });
    }));
    assert!(interrupted.is_err());
    assert_eq!(read_pointer(paths.active_profile()).generation(), &second);
    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), first_bytes);

    assert!(store.recover_interrupted_activation().unwrap());

    assert_eq!(read_pointer(paths.active_profile()).generation(), &first);
    assert_eq!(read_pointer(paths.previous_profile()).generation(), &second);
    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), first_bytes);
    assert!(!paths.profile_activation_journal().exists());
}

#[test]
fn protected_profile_store_publishes_activates_and_rolls_back_exact_bytes() {
    let (_base, paths) = test_paths();
    let store = ProfileStore::new(paths.clone());

    let first_bytes = profile("1.0");
    let first = store
        .publish_and_activate(&first_bytes, source("first"))
        .unwrap();
    assert_eq!(
        fs::read(
            paths
                .profile_generations()
                .join(first.directory_name())
                .join("profile.ini")
        )
        .unwrap(),
        first_bytes
    );
    assert_eq!(read_pointer(paths.active_profile()).generation(), &first);
    assert!(!paths.previous_profile().exists());

    let second = store
        .publish_and_activate(&profile("1.2"), source("second"))
        .unwrap();
    assert_eq!(read_pointer(paths.active_profile()).generation(), &second);
    assert_eq!(read_pointer(paths.previous_profile()).generation(), &first);

    let restored = store.rollback().unwrap();
    assert_eq!(restored, Some(first.clone()));
    assert_eq!(read_pointer(paths.active_profile()).generation(), &first);
    assert_eq!(read_pointer(paths.previous_profile()).generation(), &second);
}

#[test]
fn invalid_publish_and_tampered_rollback_preserve_active_pointer() {
    let (_base, paths) = test_paths();
    let runtime = install_active_runtime(&paths, "0.2.0");
    let store = ProfileStore::new(paths.clone());
    let first_bytes = profile("1.0");
    let first = store
        .publish_and_activate(&first_bytes, source("first"))
        .unwrap();
    assert!(store
        .publish_and_activate(b"invalid", source("bad"))
        .is_err());
    assert_eq!(read_pointer(paths.active_profile()).generation(), &first);
    assert!(store
        .publish_and_activate(&vec![b'x'; MAX_PROFILE_BYTES + 1], source("oversized"))
        .is_err());
    assert_eq!(read_pointer(paths.active_profile()).generation(), &first);
    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), first_bytes);

    let second_bytes = profile("1.2");
    let second = store
        .publish_and_activate(&second_bytes, source("second"))
        .unwrap();
    fs::write(
        paths
            .profile_generations()
            .join(first.directory_name())
            .join("profile.ini"),
        profile("9.9"),
    )
    .unwrap();

    assert!(store.rollback().is_err());
    assert_eq!(read_pointer(paths.active_profile()).generation(), &second);
    assert_eq!(read_pointer(paths.previous_profile()).generation(), &first);
    assert_eq!(fs::read(runtime.join("MacType.ini")).unwrap(), second_bytes);
}
