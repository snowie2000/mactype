use super::super::legacy_mactray::{
    LegacyScmSnapshot, LegacyServiceStatus, ServiceConfiguration, ServicePresence,
    ServiceRuntimeState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt::Write as _,
    path::{Component, Path, PathBuf},
};

pub(super) const RECEIPT_SCHEMA: &str = "mactype-control-center/legacy-migration";
pub(super) const RECEIPT_VERSION: u32 = 4;
pub(super) const MAX_PROFILE_BYTES: u64 = 4 * 1024 * 1024;
pub(super) const MAX_RECEIPT_BYTES: u64 = 256 * 1024;
pub(super) const MAX_REGISTRY_EXPORT_BYTES: u64 = 4 * 1024 * 1024;
pub(super) const CURRENT_FILE: &str = "current.json";
pub(super) const RECEIPT_FILE: &str = "receipt.json";
pub(super) const CONFIGURATION_BACKUP: &str = "MacType.ini.backup";
pub(super) const ACTIVE_PROFILE_BACKUP: &str = "active-profile.ini.backup";
pub(super) const SERVICE_REGISTRY_EXPORT: &str = "service.reg";
pub(super) const SERVICE_REGISTRY_KEY: &str = r"HKLM\SYSTEM\CurrentControlSet\Services\MacType";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MigrationStage {
    BackupPrepared,
    LegacyStopped,
    LegacyRemoved,
    RollbackCompleted,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum BackupRole {
    Configuration,
    ActiveProfile,
    ConfigurationAndActiveProfile,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BackupFileReceipt {
    pub(super) role: BackupRole,
    pub(super) original_path: String,
    pub(super) backup_file: String,
    pub(super) byte_length: u64,
    pub(super) sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ProfileBackupReceipt {
    Present(BackupFileReceipt),
    Absent {
        role: BackupRole,
        original_path: String,
    },
}

impl ProfileBackupReceipt {
    pub(super) fn role(&self) -> BackupRole {
        match self {
            Self::Present(file) => file.role,
            Self::Absent { role, .. } => *role,
        }
    }

    pub(super) fn original_path(&self) -> &str {
        match self {
            Self::Present(file) => &file.original_path,
            Self::Absent { original_path, .. } => original_path,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RegistryExportReceipt {
    pub(super) export_file: String,
    pub(super) byte_length: u64,
    pub(super) sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MigrationReceipt {
    pub(crate) schema: String,
    pub(crate) version: u32,
    pub(crate) generation: String,
    pub(crate) installation_root: String,
    pub(crate) service: LegacyScmSnapshot,
    pub(crate) completed_stages: Vec<MigrationStage>,
    pub(super) service_registry: RegistryExportReceipt,
    pub(super) files: Vec<ProfileBackupReceipt>,
    pub(super) prepared_unix_ms: u64,
}

impl MigrationReceipt {
    pub(super) fn current_stage(&self) -> Option<MigrationStage> {
        self.completed_stages.last().copied()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CurrentMigration {
    pub(super) schema: String,
    pub(super) version: u32,
    pub(super) generation: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemovalVerification {
    pub(crate) new_service_ready: bool,
    pub(crate) active_digest_match: bool,
    pub(crate) backup_valid: bool,
}

pub(super) fn require_owned_legacy_service(status: &LegacyServiceStatus) -> Result<(), String> {
    if status.registry_conflict {
        return Err("AppInit registry mode conflicts with legacy service migration".to_owned());
    }
    if !matches!(
        status.presence,
        ServicePresence::Owned | ServicePresence::CompatibleUnquoted
    ) {
        return Err("legacy migration requires an exactly owned service".to_owned());
    }
    match status.state {
        ServiceRuntimeState::Stopped => Ok(()),
        ServiceRuntimeState::Running if status.trusted_binary_available => Ok(()),
        ServiceRuntimeState::Running => Err(
            "a running legacy service cannot be migrated without its trusted MacTray binary"
                .to_owned(),
        ),
        _ => Err("legacy SCM service must be stably running or stopped".to_owned()),
    }
}

pub(super) fn contained_profile_path(root: &Path, configured: &Path) -> Result<PathBuf, String> {
    if configured
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("legacy AlternativeFile cannot escape the MacType installation".to_owned());
    }
    let candidate = if configured.is_absolute() {
        configured.to_path_buf()
    } else {
        root.join(configured)
    };
    if !candidate.starts_with(root) {
        return Err("legacy AlternativeFile cannot escape the MacType installation".to_owned());
    }
    if !candidate
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
    {
        return Err("legacy AlternativeFile must select an INI profile".to_owned());
    }
    Ok(candidate)
}

pub(super) fn validate_backup_bytes(
    receipt: &BackupFileReceipt,
    bytes: &[u8],
) -> Result<(), String> {
    if receipt.byte_length != bytes.len() as u64 || receipt.sha256 != hex_sha256(bytes) {
        Err("legacy migration backup failed its integrity check".to_owned())
    } else {
        Ok(())
    }
}

pub(super) fn validate_registry_export_bytes(
    receipt: &RegistryExportReceipt,
    bytes: &[u8],
) -> Result<(), String> {
    if receipt.export_file != SERVICE_REGISTRY_EXPORT
        || bytes.is_empty()
        || bytes.len() as u64 > MAX_REGISTRY_EXPORT_BYTES
        || receipt.byte_length != bytes.len() as u64
        || receipt.sha256 != hex_sha256(bytes)
    {
        Err("legacy service registry export failed its integrity check".to_owned())
    } else {
        Ok(())
    }
}

pub(super) fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

pub(super) fn require_removal_verification(
    verification: RemovalVerification,
    internally_valid_backup: bool,
) -> Result<(), String> {
    if verification.new_service_ready
        && verification.active_digest_match
        && verification.backup_valid
        && internally_valid_backup
    {
        Ok(())
    } else {
        Err(
            "legacy removal requires a ready new service, matching profile digest, and valid backup"
                .to_owned(),
        )
    }
}

pub(super) fn valid_stage_history(stages: &[MigrationStage]) -> bool {
    matches!(
        stages,
        [MigrationStage::BackupPrepared]
            | [
                MigrationStage::BackupPrepared,
                MigrationStage::LegacyStopped
            ]
            | [
                MigrationStage::BackupPrepared,
                MigrationStage::LegacyStopped,
                MigrationStage::LegacyRemoved,
            ]
            | [
                MigrationStage::BackupPrepared,
                MigrationStage::RollbackCompleted,
            ]
            | [
                MigrationStage::BackupPrepared,
                MigrationStage::LegacyStopped,
                MigrationStage::RollbackCompleted,
            ]
            | [
                MigrationStage::BackupPrepared,
                MigrationStage::LegacyStopped,
                MigrationStage::LegacyRemoved,
                MigrationStage::RollbackCompleted,
            ]
    )
}

pub(super) fn validate_service_configuration(
    installation_root: &Path,
    configuration: &ServiceConfiguration,
) -> Result<(), String> {
    let trusted_binary = installation_root.join("MacTray.exe");
    let trusted = trusted_binary.to_string_lossy();
    let trusted = trusted.strip_prefix(r"\\?\").unwrap_or(trusted.as_ref());
    let quoted = format!("\"{trusted}\" -service");
    let unquoted = format!("{trusted} -service");
    if configuration.service_type != 0x10
        || configuration.start_type != 2
        || configuration.error_control != 1
        || configuration.display_name != "MacType"
        || configuration.load_order_group.is_some()
        || configuration.tag_id != 0
        || !configuration.account.eq_ignore_ascii_case("LocalSystem")
        || configuration.dependencies.len() != 1
        || !configuration.dependencies[0].eq_ignore_ascii_case("winmgmt")
        || (!configuration.binary_path.eq_ignore_ascii_case(&quoted)
            && !configuration.binary_path.eq_ignore_ascii_case(&unquoted))
    {
        return Err("legacy migration receipt contains foreign SCM configuration".to_owned());
    }
    Ok(())
}
