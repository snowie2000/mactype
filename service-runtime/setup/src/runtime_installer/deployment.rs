use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
#[cfg(feature = "ci-test-adapter")]
use std::path::PathBuf;

use mactype_service_contract::{
    verify_runtime_manifest, VerifiedRuntimeManifest, MAX_PROFILE_BYTES, MAX_RUNTIME_FILE_BYTES,
};

use super::{FixedPayload, RuntimeInstaller};
use crate::profile_bridge::GENERATED_PROFILE_NAME;
use crate::storage::{
    create_protected_directory, read_bounded_directory, read_bounded_regular_file,
    reject_reparse_ancestors, temporary_nonce, SetupError,
};

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

pub(super) struct LoadedPayload {
    pub(super) verified: VerifiedRuntimeManifest,
    pub(super) files: BTreeMap<String, Vec<u8>>,
}

impl FixedPayload {
    pub fn beside_setup_executable() -> Result<Self, SetupError> {
        let executable = std::env::current_exe()?;
        let parent = executable
            .parent()
            .ok_or_else(|| SetupError::Runtime("setup executable has no parent".to_owned()))?;
        Ok(Self {
            root: parent.join("payload"),
        })
    }

    #[cfg(feature = "ci-test-adapter")]
    pub fn from_test_root(root: PathBuf) -> Result<Self, SetupError> {
        if !root.is_absolute() {
            return Err(SetupError::Runtime(
                "test payload root must be absolute".to_owned(),
            ));
        }
        Ok(Self { root })
    }

    pub(super) fn load(&self) -> Result<LoadedPayload, SetupError> {
        reject_reparse_ancestors(&self.root)?;
        let manifest_path = self.root.join("manifest.json");
        let files_root = self.root.join("files");
        reject_reparse_ancestors(&manifest_path)?;
        reject_reparse_ancestors(&files_root)?;
        let manifest =
            read_bounded_regular_file(&manifest_path, MAX_MANIFEST_BYTES, "runtime manifest")
                .map_err(|error| SetupError::Manifest(error.to_string()))?;
        let mut files = BTreeMap::new();
        for entry in read_bounded_directory(
            &files_root,
            mactype_service_contract::IMMUTABLE_RUNTIME_FILES.len(),
            "payload entry count",
        )? {
            let path = entry.path();
            reject_reparse_ancestors(&path)?;
            let metadata = entry.metadata()?;
            if !metadata.is_file() || metadata.len() > MAX_RUNTIME_FILE_BYTES as u64 {
                return Err(SetupError::Manifest(
                    "payload entry is not a bounded regular file".to_owned(),
                ));
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| SetupError::Manifest("payload filename is not Unicode".to_owned()))?;
            let file = OpenOptions::new().read(true).open(path)?;
            let mut bytes = Vec::with_capacity(metadata.len() as usize);
            file.take(MAX_RUNTIME_FILE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() > MAX_RUNTIME_FILE_BYTES {
                return Err(SetupError::Manifest("payload file is too large".to_owned()));
            }
            files.insert(name, bytes);
        }
        let verified = verify_runtime_manifest(&manifest, &files)
            .map_err(|error| SetupError::Manifest(error.to_string()))?;
        Ok(LoadedPayload { verified, files })
    }
}

impl RuntimeInstaller {
    pub(super) fn stage_payload(
        &self,
        payload: &LoadedPayload,
        destination: &Path,
        replace_invalid: bool,
    ) -> Result<(), SetupError> {
        create_protected_directory(self.paths.runtime_versions())?;
        remove_legacy_staging_collision(
            self.paths.runtime_versions(),
            payload.verified.version(),
            &payload.files,
        )?;
        if destination.exists() {
            reject_reparse_ancestors(destination)?;
            match verify_existing_payload(destination, &payload.files) {
                Ok(()) => return Ok(()),
                Err(error) if !replace_invalid => return Err(error),
                Err(_) => return self.replace_runtime_payload(destination, payload),
            }
        }

        let staging = self.paths.runtime_versions().join(format!(
            ".staging-{}-{}",
            payload.verified.version(),
            temporary_nonce()
        ));
        if staging.exists() {
            return Err(SetupError::Runtime(
                "runtime staging directory already exists".to_owned(),
            ));
        }
        create_protected_directory(&staging)?;
        let result = (|| {
            for (name, bytes) in &payload.files {
                write_synced(&staging.join(name), bytes)?;
            }
            fs::rename(&staging, destination)?;
            verify_existing_payload(destination, &payload.files)
        })();
        if result.is_err() && staging.exists() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    }
}

fn remove_legacy_staging_collision(
    root: &Path,
    version: &str,
    expected: &BTreeMap<String, Vec<u8>>,
) -> Result<(), SetupError> {
    let staging = root.join(format!(".staging-{version}-{}", std::process::id()));
    if !staging.exists() {
        return Ok(());
    }
    reject_reparse_ancestors(&staging)?;
    if !fs::metadata(&staging)?.is_dir() {
        return Err(SetupError::Runtime(
            "runtime staging collision is not a protected directory".to_owned(),
        ));
    }
    let mut files = Vec::new();
    for entry in read_bounded_directory(&staging, expected.len(), "runtime staging entry count")? {
        let name = entry.file_name().into_string().map_err(|_| {
            SetupError::Runtime("runtime staging filename is not Unicode".to_owned())
        })?;
        if !expected.contains_key(&name) {
            return Err(SetupError::Runtime(
                "runtime staging collision contains an unexpected entry".to_owned(),
            ));
        }
        let path = entry.path();
        reject_reparse_ancestors(&path)?;
        let metadata = entry.metadata()?;
        if !metadata.is_file() || metadata.len() > MAX_RUNTIME_FILE_BYTES as u64 {
            return Err(SetupError::Runtime(
                "runtime staging collision contains a non-regular entry".to_owned(),
            ));
        }
        files.push(path);
    }
    for path in files {
        fs::remove_file(path)?;
    }
    fs::remove_dir(staging)?;
    Ok(())
}

pub(super) fn verify_existing_payload(
    directory: &Path,
    expected: &BTreeMap<String, Vec<u8>>,
) -> Result<(), SetupError> {
    reject_reparse_ancestors(directory)?;
    let entries = read_bounded_directory(
        directory,
        expected.len() + 1,
        "installed runtime entry count",
    )?;
    if entries.len() < expected.len() {
        return Err(SetupError::Runtime(
            "installed runtime contains an unexpected file set".to_owned(),
        ));
    }
    let mut verified_manifest_files = 0usize;
    for entry in entries {
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| SetupError::Runtime("runtime filename is not Unicode".to_owned()))?;
        let path = entry.path();
        reject_reparse_ancestors(&path)?;
        if name == GENERATED_PROFILE_NAME {
            let metadata = entry.metadata()?;
            if !metadata.is_file()
                || metadata.len() == 0
                || metadata.len() > MAX_PROFILE_BYTES as u64
            {
                return Err(SetupError::Runtime(
                    "generated runtime profile is not a bounded regular file".to_owned(),
                ));
            }
            continue;
        }
        let bytes = expected
            .get(&name)
            .ok_or_else(|| SetupError::Runtime("runtime contains an unsigned file".to_owned()))?;
        if read_bounded_regular_file(
            &path,
            MAX_RUNTIME_FILE_BYTES as u64,
            "installed runtime file",
        )? != *bytes
        {
            return Err(SetupError::Runtime(
                "installed runtime hash verification failed".to_owned(),
            ));
        }
        verified_manifest_files += 1;
    }
    if verified_manifest_files != expected.len() {
        return Err(SetupError::Runtime(
            "installed runtime is missing a manifest file".to_owned(),
        ));
    }
    Ok(())
}

pub(super) fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), SetupError> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
