use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use mactype_service_contract::{
    sha256_digest, IMMUTABLE_RUNTIME_FILES, MAX_PROFILE_BYTES, MAX_RUNTIME_FILE_BYTES,
};
use serde::{Deserialize, Serialize};

use super::super::deployment::LoadedPayload;
use super::super::journal::safe_version_component;
use super::super::RuntimeInstaller;
use crate::profile_bridge::GENERATED_PROFILE_NAME;
use crate::storage::{
    atomic_write, create_protected_directory, read_bounded_directory, read_bounded_regular_file,
    reject_reparse_ancestors, SetupError,
};

const RUNTIME_RECEIPT_SCHEMA: u32 = 1;
const MAX_RUNTIME_RECEIPT_BYTES: u64 = 16 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeReceipt {
    schema: u32,
    version: String,
    files: BTreeMap<String, String>,
}

impl RuntimeReceipt {
    fn is_valid_for(&self, version: &str) -> bool {
        self.schema == RUNTIME_RECEIPT_SCHEMA
            && self.version == version
            && safe_version_component(&self.version)
            && self.files.len() == IMMUTABLE_RUNTIME_FILES.len()
            && IMMUTABLE_RUNTIME_FILES
                .iter()
                .all(|name| self.files.contains_key(*name))
            && self.files.iter().all(|(name, digest)| {
                IMMUTABLE_RUNTIME_FILES.contains(&name.as_str())
                    && digest.len() == 71
                    && digest.starts_with("sha256:")
                    && digest[7..]
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
    }
}

impl RuntimeInstaller {
    pub(in crate::runtime_installer) fn verify_runtime_generation_receipt(
        &self,
        version: &str,
        directory: &Path,
    ) -> Result<(), SetupError> {
        reject_reparse_ancestors(directory)?;
        if !fs::metadata(directory)?.is_dir() {
            return Err(SetupError::Runtime(
                "runtime generation is not a regular directory".to_owned(),
            ));
        }
        let receipt_bytes = read_bounded_regular_file(
            &self.runtime_receipt_path(version),
            MAX_RUNTIME_RECEIPT_BYTES,
            "runtime generation receipt",
        )?;
        let receipt: RuntimeReceipt = serde_json::from_slice(&receipt_bytes)
            .map_err(|_| SetupError::Runtime("runtime generation receipt is invalid".to_owned()))?;
        if !receipt.is_valid_for(version) {
            return Err(SetupError::Runtime(
                "runtime generation receipt violates the fixed manifest contract".to_owned(),
            ));
        }
        let entries = read_bounded_directory(
            directory,
            IMMUTABLE_RUNTIME_FILES.len() + 1,
            "runtime generation entry count",
        )?;
        if entries.len() < IMMUTABLE_RUNTIME_FILES.len() {
            return Err(SetupError::Runtime(
                "runtime generation contains an unexpected file set".to_owned(),
            ));
        }
        let mut verified = BTreeSet::new();
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
                        "generated profile is not a bounded regular file".to_owned(),
                    ));
                }
                continue;
            }
            let expected = receipt.files.get(&name).ok_or_else(|| {
                SetupError::Runtime("runtime generation contains an unsigned file".to_owned())
            })?;
            let bytes = read_bounded_regular_file(
                &path,
                MAX_RUNTIME_FILE_BYTES as u64,
                "runtime generation file",
            )?;
            if &sha256_digest(&bytes) != expected {
                return Err(SetupError::Runtime(
                    "runtime generation file differs from its receipt".to_owned(),
                ));
            }
            verified.insert(name);
        }
        if verified.len() != IMMUTABLE_RUNTIME_FILES.len() {
            return Err(SetupError::Runtime(
                "runtime generation is missing a receipt file".to_owned(),
            ));
        }
        Ok(())
    }

    pub(in crate::runtime_installer) fn write_runtime_receipt(
        &self,
        payload: &LoadedPayload,
    ) -> Result<(), SetupError> {
        create_protected_directory(&self.runtime_receipts_root())?;
        let files = payload
            .files
            .iter()
            .map(|(name, bytes)| (name.clone(), sha256_digest(bytes)))
            .collect();
        atomic_write(
            &self.runtime_receipt_path(payload.verified.version()),
            &serde_json::to_vec(&RuntimeReceipt {
                schema: RUNTIME_RECEIPT_SCHEMA,
                version: payload.verified.version().to_owned(),
                files,
            })?,
        )
    }

    pub(in crate::runtime_installer) fn runtime_receipts_root(&self) -> PathBuf {
        self.paths.service_root().join("runtime-receipts")
    }

    pub(super) fn runtime_receipt_path(&self, version: &str) -> PathBuf {
        self.runtime_receipts_root().join(format!("{version}.json"))
    }
}
