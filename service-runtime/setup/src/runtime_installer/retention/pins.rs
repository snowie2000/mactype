use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use mactype_service_contract::{
    sha256_digest, MigrationPinnedRuntime, MigrationRuntimePin, IMMUTABLE_RUNTIME_FILES,
    MAX_MIGRATION_RUNTIME_PIN_BYTES, MAX_PINNED_RUNTIMES, MAX_PROFILE_BYTES,
    MAX_RUNTIME_FILE_BYTES,
};

use super::super::RuntimeInstaller;
use crate::profile_bridge::GENERATED_PROFILE_NAME;
use crate::storage::{
    read_bounded_directory, read_bounded_regular_file, reject_reparse_ancestors, SetupError,
};

const MIGRATION_RUNTIME_PINS_DIRECTORY: &str = "migration-runtime-pins";

impl RuntimeInstaller {
    pub(in crate::runtime_installer) fn load_verified_migration_pins(
        &self,
    ) -> Result<BTreeMap<String, MigrationPinnedRuntime>, SetupError> {
        let root = self.migration_runtime_pins_root();
        if !root.exists() {
            return Ok(BTreeMap::new());
        }
        reject_reparse_ancestors(&root)?;
        if !fs::metadata(&root)?.is_dir() {
            return Err(SetupError::Runtime(
                "migration runtime pin root is not a directory".to_owned(),
            ));
        }
        let entries =
            read_bounded_directory(&root, MAX_PINNED_RUNTIMES, "migration runtime pin count")?;
        let mut pinned = BTreeMap::new();
        for entry in entries {
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| SetupError::Runtime("migration pin name is not Unicode".to_owned()))?;
            let nonce = name.strip_suffix(".json").ok_or_else(|| {
                SetupError::Runtime("migration pin filename is not canonical".to_owned())
            })?;
            if nonce.len() != 32
                || !nonce
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(SetupError::Runtime(
                    "migration pin filename is not canonical".to_owned(),
                ));
            }
            let bytes = read_bounded_regular_file(
                &entry.path(),
                MAX_MIGRATION_RUNTIME_PIN_BYTES,
                "migration runtime pin",
            )?;
            let pin: MigrationRuntimePin = serde_json::from_slice(&bytes)
                .map_err(|_| SetupError::Runtime("migration runtime pin is invalid".to_owned()))?;
            pin.validate().map_err(|error| {
                SetupError::Runtime(format!("migration runtime pin is invalid: {error}"))
            })?;
            if pin.nonce() != nonce {
                return Err(SetupError::Runtime(
                    "migration runtime pin nonce does not match its filename".to_owned(),
                ));
            }
            for runtime in pin.runtimes() {
                self.verify_pinned_runtime(runtime)?;
                match pinned.get(runtime.version()) {
                    Some(existing) if existing != runtime => {
                        return Err(SetupError::Runtime(
                            "migration pins disagree about a runtime generation".to_owned(),
                        ));
                    }
                    Some(_) => {}
                    None => {
                        pinned.insert(runtime.version().to_owned(), runtime.clone());
                        if pinned.len() > MAX_PINNED_RUNTIMES {
                            return Err(SetupError::Runtime(
                                "migration pinned runtime count exceeds the fixed limit".to_owned(),
                            ));
                        }
                    }
                }
            }
        }
        Ok(pinned)
    }

    fn verify_pinned_runtime(&self, runtime: &MigrationPinnedRuntime) -> Result<(), SetupError> {
        runtime.validate().map_err(|error| {
            SetupError::Runtime(format!("migration runtime pin is invalid: {error}"))
        })?;
        let root = self.paths.runtime_versions().join(runtime.version());
        reject_reparse_ancestors(&root)?;
        if !fs::metadata(&root)?.is_dir() {
            return Err(SetupError::Runtime(
                "pinned runtime generation is unavailable".to_owned(),
            ));
        }
        let expected_count =
            IMMUTABLE_RUNTIME_FILES.len() + usize::from(runtime.generated_profile().is_some());
        let entries = read_bounded_directory(&root, expected_count, "pinned runtime entry count")?;
        if entries.len() != expected_count {
            return Err(SetupError::Runtime(
                "pinned runtime contains an unexpected file set".to_owned(),
            ));
        }
        for (name, expected) in runtime.files() {
            let bytes = read_bounded_regular_file(
                &root.join(name),
                MAX_RUNTIME_FILE_BYTES as u64,
                "pinned runtime file",
            )?;
            if sha256_digest(&bytes) != *expected {
                return Err(SetupError::Runtime(
                    "pinned runtime file differs from its migration hash".to_owned(),
                ));
            }
        }
        if let Some(expected) = runtime.generated_profile() {
            let bytes = read_bounded_regular_file(
                &root.join(GENERATED_PROFILE_NAME),
                MAX_PROFILE_BYTES as u64,
                "pinned generated profile",
            )?;
            if sha256_digest(&bytes) != expected {
                return Err(SetupError::Runtime(
                    "pinned generated profile differs from its migration hash".to_owned(),
                ));
            }
        }
        Ok(())
    }

    pub(in crate::runtime_installer) fn migration_runtime_pins_root(&self) -> PathBuf {
        self.paths
            .service_root()
            .join(MIGRATION_RUNTIME_PINS_DIRECTORY)
    }
}
