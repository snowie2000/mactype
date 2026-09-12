use std::fs;

use mactype_service_contract::{
    MachinePaths, IMMUTABLE_RUNTIME_FILES, MAX_PROFILE_BYTES, MAX_RUNTIME_FILE_BYTES,
};
use mactype_service_host::{ProtectedRuntimeAssets, RUNTIME_PROFILE_ABSENT_CODE};

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

fn install_runtime_fixture(paths: &MachinePaths) -> std::path::PathBuf {
    let generation = paths.runtime_versions().join("0.2.0");
    fs::create_dir_all(&generation).unwrap();
    for name in [
        "mactype-service.exe",
        "mactype-injector32.exe",
        "mactype-injector64.exe",
        "MacType.dll",
        "MacType64.dll",
        "MacType.ini",
    ] {
        fs::write(generation.join(name), name.as_bytes()).unwrap();
    }
    fs::create_dir_all(paths.runtime_pointer().parent().unwrap()).unwrap();
    fs::write(
        paths.runtime_pointer(),
        br#"{"schema":1,"version":"0.2.0"}"#,
    )
    .unwrap();
    generation
}

#[test]
fn active_runtime_rejects_each_oversized_immutable_component_at_the_file_boundary() {
    for oversized_name in IMMUTABLE_RUNTIME_FILES {
        let (_base, paths) = paths();
        let generation = install_runtime_fixture(&paths);
        fs::File::create(generation.join(oversized_name))
            .unwrap()
            .set_len(MAX_RUNTIME_FILE_BYTES as u64 + 1)
            .unwrap();

        let error = ProtectedRuntimeAssets::load(paths).unwrap_err();

        assert_eq!(error.code, "runtime-component-invalid", "{oversized_name}");
        assert!(error.message.contains("bounded"), "{oversized_name}");
    }
}

#[test]
fn active_runtime_rejects_an_oversized_generated_profile_at_the_profile_boundary() {
    let (_base, paths) = paths();
    let generation = install_runtime_fixture(&paths);
    fs::File::create(generation.join("MacType.ini"))
        .unwrap()
        .set_len(MAX_PROFILE_BYTES as u64 + 1)
        .unwrap();

    let error = ProtectedRuntimeAssets::load(paths).unwrap_err();

    assert_eq!(error.code, "runtime-component-invalid");
    assert!(error.message.contains("bounded"));
}

#[test]
fn helpers_and_dlls_are_selected_only_from_the_active_protected_runtime_generation() {
    let (_base, paths) = paths();
    let generation = install_runtime_fixture(&paths);

    let assets = ProtectedRuntimeAssets::load(paths.clone()).unwrap();

    assert_eq!(assets.root(), generation);
    assert_eq!(
        assets.injector32(),
        generation.join("mactype-injector32.exe")
    );
    assert_eq!(
        assets.injector64(),
        generation.join("mactype-injector64.exe")
    );
    assert_eq!(assets.generation_id().len(), 64);
    assert!(assets
        .generation_id()
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
}

#[test]
fn active_runtime_reports_a_missing_generated_profile_as_a_supported_stop() {
    let (_base, paths) = paths();
    let generation = install_runtime_fixture(&paths);
    fs::remove_file(generation.join("MacType.ini")).unwrap();

    let error = ProtectedRuntimeAssets::load(paths)
        .expect_err("a stopped runtime has no generated profile");

    assert_eq!(error.code, RUNTIME_PROFILE_ABSENT_CODE);
}

#[test]
fn active_runtime_with_a_missing_profile_and_stray_file_remains_invalid() {
    let (_base, paths) = paths();
    let generation = install_runtime_fixture(&paths);
    fs::remove_file(generation.join("MacType.ini")).unwrap();
    fs::write(generation.join("unsigned.dll"), b"unexpected").unwrap();

    let error = ProtectedRuntimeAssets::load(paths)
        .expect_err("a stray runtime file must not become a supported stop");

    assert_eq!(error.code, "runtime-file-set-invalid");
}

#[test]
fn active_runtime_rejects_every_file_beyond_manifest_assets_and_generated_mactype_ini() {
    let (_base, paths) = paths();
    let generation = install_runtime_fixture(&paths);
    fs::write(generation.join("unsigned.dll"), b"unexpected").unwrap();

    let error = ProtectedRuntimeAssets::load(paths)
        .expect_err("an unexpected runtime file must fail initialization");

    assert_eq!(error.code, "runtime-file-set-invalid");
}

#[test]
fn active_runtime_rejects_an_oversized_pointer_before_parsing() {
    let (_base, paths) = paths();
    fs::create_dir_all(paths.runtime_pointer().parent().unwrap()).unwrap();
    fs::write(paths.runtime_pointer(), vec![b'x'; 64 * 1024 + 1]).unwrap();

    let error = ProtectedRuntimeAssets::load(paths)
        .expect_err("an oversized runtime pointer must fail before JSON parsing");

    assert_eq!(error.code, "active-runtime-invalid");
    assert!(error.message.contains("bounded regular file"));
}
