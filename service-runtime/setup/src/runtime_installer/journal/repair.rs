use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use mactype_service_contract::sha256_digest;
use serde::{Deserialize, Serialize};

mod receipt;
mod recovery;

use super::safe_version_component;
use crate::runtime_installer::deployment::{verify_existing_payload, write_synced, LoadedPayload};
use crate::runtime_installer::RuntimeInstaller;
use crate::storage::{
    atomic_write, create_protected_directory, read_bounded_regular_file, reject_reparse_ancestors,
    temporary_nonce, SetupError,
};
use receipt::{
    read_runtime_directory_receipt, remove_repair_directory, remove_verified_backup_directory,
    valid_runtime_directory_receipt, verify_runtime_directory_receipt,
};
use recovery::{recover_prepared_repair, recover_unverified_repair, recover_verified_repair};

const RUNTIME_REPAIR_SCHEMA: u32 = 2;
const MAX_REPAIR_JOURNAL_BYTES: u64 = 16 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeRepairJournal {
    schema: u32,
    version: String,
    staging: String,
    backup: String,
    phase: RuntimeRepairPhase,
    old_receipt: RuntimeDirectoryReceipt,
    new_receipt: RuntimeDirectoryReceipt,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum RuntimeRepairPhase {
    Prepared,
    OldMoved,
    NewPlacedUnverified,
    NewVerified,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeDirectoryReceipt {
    files: BTreeMap<String, String>,
}

impl RuntimeInstaller {
    pub(in crate::runtime_installer) fn replace_runtime_payload(
        &self,
        destination: &Path,
        payload: &LoadedPayload,
    ) -> Result<(), SetupError> {
        let versions_root = self.paths.runtime_versions();
        let repair_journal_path = self.repair_journal_path();
        let nonce = temporary_nonce();
        let staging = versions_root.join(format!(
            ".repair-new-{}-{nonce}",
            payload.verified.version()
        ));
        let backup = versions_root.join(format!(
            ".repair-old-{}-{nonce}",
            payload.verified.version()
        ));
        if staging.exists() || backup.exists() {
            return Err(SetupError::Runtime(
                "runtime repair staging collision".to_owned(),
            ));
        }
        create_protected_directory(&staging)?;
        let staged = (|| {
            for (name, bytes) in &payload.files {
                write_synced(&staging.join(name), bytes)?;
            }
            verify_existing_payload(&staging, &payload.files)
        })();
        if let Err(error) = staged {
            let _ = remove_repair_directory(&staging);
            return Err(error);
        }

        let staging_name = staging
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                SetupError::Runtime("runtime repair staging name is invalid".to_owned())
            })?
            .to_owned();
        let backup_name = backup
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| SetupError::Runtime("runtime repair backup name is invalid".to_owned()))?
            .to_owned();
        let mut journal = RuntimeRepairJournal {
            schema: RUNTIME_REPAIR_SCHEMA,
            version: payload.verified.version().to_owned(),
            staging: staging_name,
            backup: backup_name,
            phase: RuntimeRepairPhase::Prepared,
            old_receipt: read_runtime_directory_receipt(destination)?,
            new_receipt: RuntimeDirectoryReceipt {
                files: payload
                    .files
                    .iter()
                    .map(|(name, bytes)| (name.clone(), sha256_digest(bytes)))
                    .collect(),
            },
        };
        self.write_repair_journal(&journal)?;
        if let Err(error) = fs::rename(destination, &backup) {
            return self.fail_repair_and_recover(error.into());
        }
        journal.phase = RuntimeRepairPhase::OldMoved;
        if let Err(error) = self.write_repair_journal(&journal) {
            return self.fail_repair_and_recover(error);
        }
        if let Err(error) = fs::rename(&staging, destination) {
            return self.fail_repair_and_recover(error.into());
        }
        journal.phase = RuntimeRepairPhase::NewPlacedUnverified;
        if let Err(error) = self.write_repair_journal(&journal) {
            return self.fail_repair_and_recover(error);
        }
        if let Err(error) = verify_existing_payload(destination, &payload.files) {
            return self.fail_repair_and_recover(error);
        }
        journal.phase = RuntimeRepairPhase::NewVerified;
        if let Err(error) = self.write_repair_journal(&journal) {
            return self.fail_repair_and_recover(error);
        }
        verify_runtime_directory_receipt(&backup, &journal.old_receipt)?;
        remove_verified_backup_directory(&backup, &journal.old_receipt)?;
        reject_reparse_ancestors(&repair_journal_path)?;
        fs::remove_file(repair_journal_path)?;
        Ok(())
    }

    fn write_repair_journal(&self, journal: &RuntimeRepairJournal) -> Result<(), SetupError> {
        atomic_write(&self.repair_journal_path(), &serde_json::to_vec(journal)?)
    }

    fn fail_repair_and_recover(&self, operation: SetupError) -> Result<(), SetupError> {
        match self.recover_interrupted_repair() {
            Ok(()) => Err(operation),
            Err(recovery) => Err(SetupError::CleanupUnknown(format!(
                "runtime repair failed ({operation}) and rollback remained incomplete ({recovery}); the repair journal and verified backup were preserved"
            ))),
        }
    }

    fn repair_journal_path(&self) -> PathBuf {
        self.paths.service_root().join("runtime-repair.json")
    }

    pub(super) fn recover_interrupted_repair(&self) -> Result<(), SetupError> {
        let journal_path = self.repair_journal_path();
        if !journal_path.exists() {
            return Ok(());
        }
        let bytes = read_bounded_regular_file(
            &journal_path,
            MAX_REPAIR_JOURNAL_BYTES,
            "runtime repair journal",
        )?;
        let journal: RuntimeRepairJournal = serde_json::from_slice(&bytes)
            .map_err(|_| SetupError::Runtime("runtime repair journal is invalid".to_owned()))?;
        if journal.schema != RUNTIME_REPAIR_SCHEMA
            || !safe_version_component(&journal.version)
            || !safe_repair_entry(&journal.staging, ".repair-new-", &journal.version)
            || !safe_repair_entry(&journal.backup, ".repair-old-", &journal.version)
            || journal.staging == journal.backup
            || !valid_runtime_directory_receipt(&journal.old_receipt)
            || !valid_runtime_directory_receipt(&journal.new_receipt)
        {
            return Err(SetupError::Runtime(
                "runtime repair journal has an unsupported value".to_owned(),
            ));
        }

        let destination = self.paths.runtime_versions().join(&journal.version);
        let staging = self.paths.runtime_versions().join(&journal.staging);
        let backup = self.paths.runtime_versions().join(&journal.backup);
        for path in [&destination, &staging, &backup] {
            if path.exists() {
                reject_reparse_ancestors(path)?;
            }
        }
        match journal.phase {
            RuntimeRepairPhase::Prepared => recover_prepared_repair(
                &destination,
                &staging,
                &backup,
                &journal.old_receipt,
                &journal.new_receipt,
            )?,
            RuntimeRepairPhase::OldMoved | RuntimeRepairPhase::NewPlacedUnverified => {
                recover_unverified_repair(
                    &destination,
                    &staging,
                    &backup,
                    &journal.old_receipt,
                    &journal.new_receipt,
                )?
            }
            RuntimeRepairPhase::NewVerified => recover_verified_repair(
                &destination,
                &staging,
                &backup,
                &journal.old_receipt,
                &journal.new_receipt,
            )?,
        }
        reject_reparse_ancestors(&journal_path)?;
        fs::remove_file(journal_path)?;
        Ok(())
    }
}

fn safe_repair_entry(value: &str, prefix: &str, version: &str) -> bool {
    let expected = format!("{prefix}{version}-");
    value.starts_with(&expected)
        && value.len() > expected.len()
        && value.len() <= 192
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
}
