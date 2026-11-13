use std::collections::BTreeMap;
use std::fs;

use mactype_service_contract::{MachinePaths, sha256_digest};
use mactype_service_setup::FixedPayload;

pub(crate) fn test_paths() -> (tempfile::TempDir, MachinePaths) {
    // AppData roots are rejected, and deep checkouts exceed legacy Win32 path limits.
    #[cfg(windows)]
    let temporary_root =
        std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("Temp");
    #[cfg(not(windows))]
    let temporary_root = std::env::current_dir().unwrap();
    let base = tempfile::tempdir_in(temporary_root).unwrap();
    let program_files = base.path().join("Program Files");
    let program_data = base.path().join("ProgramData");
    fs::create_dir_all(&program_files).unwrap();
    fs::create_dir_all(&program_data).unwrap();
    let paths = MachinePaths::from_trusted_os_roots(&program_files, &program_data).unwrap();
    (base, paths)
}

pub(crate) fn payload(base: &std::path::Path, version: &str, bytes: &[u8]) -> FixedPayload {
    let root = base.join(format!("payload-{version}"));
    let files_root = root.join("files");
    fs::create_dir_all(&files_root).unwrap();
    let payload_files = [
        ("mactype-service.exe", bytes),
        ("mactype-injector32.exe", b"injector-32"),
        ("mactype-injector64.exe", b"injector-64"),
        ("MacType.dll", b"mactype-32"),
        ("MacType64.dll", b"mactype-64"),
    ];
    let mut files = BTreeMap::new();
    for (name, contents) in payload_files {
        fs::write(files_root.join(name), contents).unwrap();
        files.insert(name.to_owned(), sha256_digest(contents));
    }
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema": 1,
            "version": version,
            "files": files,
        }))
        .unwrap(),
    )
    .unwrap();
    FixedPayload::from_test_root(root).unwrap()
}
