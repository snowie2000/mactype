#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use mactype_service_contract::{GenerationPointer, MachinePaths, ProfileCatalog, SourceMetadata};

pub(crate) struct ProtectedTree {
    _root: tempfile::TempDir,
    paths: MachinePaths,
}

impl ProtectedTree {
    pub(crate) fn new() -> Self {
        let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let program_files = root.path().join("Program Files");
        let program_data = root.path().join("ProgramData");
        fs::create_dir_all(&program_files).unwrap();
        fs::create_dir_all(&program_data).unwrap();
        let paths = MachinePaths::from_trusted_os_roots(&program_files, &program_data).unwrap();
        Self { _root: root, paths }
    }

    pub(crate) const fn paths(&self) -> &MachinePaths {
        &self.paths
    }

    pub(crate) fn install_runtime(&self, profile: Option<&[u8]>) -> PathBuf {
        let generation = self.paths.runtime_versions().join("0.2.0");
        fs::create_dir_all(&generation).unwrap();
        for name in [
            "mactype-service.exe",
            "mactype-injector32.exe",
            "mactype-injector64.exe",
            "MacType.dll",
            "MacType64.dll",
        ] {
            fs::write(generation.join(name), name.as_bytes()).unwrap();
        }
        if let Some(profile) = profile {
            fs::write(generation.join("MacType.ini"), profile).unwrap();
        }
        fs::create_dir_all(self.paths.runtime_pointer().parent().unwrap()).unwrap();
        fs::write(
            self.paths.runtime_pointer(),
            br#"{"schema":1,"version":"0.2.0"}"#,
        )
        .unwrap();
        generation
    }

    pub(crate) fn install_active_profile(&self, bytes: &[u8]) {
        let mut catalog = ProfileCatalog::new();
        let generation = catalog
            .publish_machine_profile(
                bytes,
                SourceMetadata {
                    display_name: "bounded host input".to_owned(),
                },
            )
            .unwrap();
        let directory = self
            .paths
            .profile_generations()
            .join(generation.directory_name());
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("profile.ini"), bytes).unwrap();
        fs::create_dir_all(self.paths.active_profile().parent().unwrap()).unwrap();
        fs::write(
            self.paths.active_profile(),
            serde_json::to_vec(&GenerationPointer::new(generation)).unwrap(),
        )
        .unwrap();
    }
}
