use super::{model::*, storage::*};
use crate::machine_integration::legacy_mactray::{
    self, LegacyScmSnapshot, ServicePresence, ServiceRuntimeState,
};
use std::path::{Path, PathBuf};

pub(super) fn current_receipt() -> Result<(PathBuf, MigrationReceipt), String> {
    let storage = migration_storage_root()?;
    let current_path = storage.join(CURRENT_FILE);
    let pointer: CurrentMigration = read_json_bounded_under(&storage, &current_path)?;
    if pointer.schema != RECEIPT_SCHEMA
        || pointer.version != RECEIPT_VERSION
        || !valid_generation_name(&pointer.generation)
    {
        return Err("legacy migration current pointer is invalid".to_owned());
    }
    let generation_root = storage.join(&pointer.generation);
    validate_existing_path(&storage, &generation_root)?;
    let receipt_path = generation_root.join(RECEIPT_FILE);
    let receipt: MigrationReceipt = read_json_bounded_under(&generation_root, &receipt_path)?;
    validate_receipt(&generation_root, &receipt)?;
    Ok((generation_root, receipt))
}

pub(crate) fn backup_is_valid() -> bool {
    current_receipt().is_ok()
}

pub(crate) fn current_stage_name() -> Result<&'static str, String> {
    match current_receipt()?.1.current_stage() {
        Some(MigrationStage::BackupPrepared) => Ok("backup-prepared"),
        Some(MigrationStage::LegacyStopped) => Ok("legacy-stopped"),
        Some(MigrationStage::LegacyRemoved) => Ok("legacy-removed"),
        Some(MigrationStage::RollbackCompleted) => Ok("rollback-completed"),
        None => Err("legacy migration receipt has no completed stage".to_owned()),
    }
}

pub(super) fn validate_receipt(
    generation_root: &Path,
    receipt: &MigrationReceipt,
) -> Result<(), String> {
    if receipt.schema != RECEIPT_SCHEMA
        || receipt.version != RECEIPT_VERSION
        || !valid_generation_name(&receipt.generation)
        || generation_root.file_name() != Some(receipt.generation.as_ref())
        || !valid_stage_history(&receipt.completed_stages)
    {
        return Err("legacy migration receipt schema or stage history is invalid".to_owned());
    }
    let expected_root = expected_installation_root()?;
    let installation_root = PathBuf::from(&receipt.installation_root);
    if !installation_root
        .to_string_lossy()
        .eq_ignore_ascii_case(&expected_root.to_string_lossy())
    {
        return Err("legacy migration receipt points outside Program Files MacType".to_owned());
    }
    if !matches!(
        receipt.service.presence,
        ServicePresence::Owned | ServicePresence::CompatibleUnquoted
    ) || !matches!(
        receipt.service.state,
        ServiceRuntimeState::Running | ServiceRuntimeState::Stopped
    ) {
        return Err("legacy migration receipt contains an unsafe service state".to_owned());
    }
    validate_service_configuration(&installation_root, &receipt.service.configuration)?;
    legacy_mactray::validate_migration_snapshot(&receipt.service)?;
    if receipt.service_registry.export_file != SERVICE_REGISTRY_EXPORT {
        return Err("legacy migration receipt has an unexpected registry export name".to_owned());
    }
    let registry_export = generation_root.join(&receipt.service_registry.export_file);
    let registry_bytes =
        read_regular_bounded_under(generation_root, &registry_export, MAX_REGISTRY_EXPORT_BYTES)?;
    validate_registry_export_bytes(&receipt.service_registry, &registry_bytes)?;
    if receipt.files.is_empty() || receipt.files.len() > 2 {
        return Err("legacy migration receipt has an invalid profile backup set".to_owned());
    }
    let configuration_path = installation_root.join("MacType.ini");
    let mut configuration_backup_seen = false;
    let mut active_profile_backup_seen = false;
    for file in &receipt.files {
        let role = file.role();
        let original = PathBuf::from(file.original_path());
        match role {
            BackupRole::Configuration => {
                if configuration_backup_seen
                    || !original
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&configuration_path.to_string_lossy())
                {
                    return Err(
                        "legacy migration receipt has an invalid MacType.ini backup".to_owned()
                    );
                }
                configuration_backup_seen = true;
            }
            BackupRole::ActiveProfile => {
                if active_profile_backup_seen
                    || !original
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
                {
                    return Err(
                        "legacy migration receipt has an invalid active profile backup".to_owned(),
                    );
                }
                active_profile_backup_seen = true;
            }
            BackupRole::ConfigurationAndActiveProfile => {
                if configuration_backup_seen
                    || active_profile_backup_seen
                    || !original
                        .to_string_lossy()
                        .eq_ignore_ascii_case(&configuration_path.to_string_lossy())
                {
                    return Err(
                        "legacy migration receipt has an invalid shared profile backup".to_owned(),
                    );
                }
                configuration_backup_seen = true;
                active_profile_backup_seen = true;
            }
        }
        validate_existing_path(&installation_root, &original)?;
        match file {
            ProfileBackupReceipt::Present(file) => {
                let expected_name = match role {
                    BackupRole::Configuration | BackupRole::ConfigurationAndActiveProfile => {
                        CONFIGURATION_BACKUP
                    }
                    BackupRole::ActiveProfile => ACTIVE_PROFILE_BACKUP,
                };
                if file.backup_file != expected_name {
                    return Err("legacy migration receipt has an unexpected backup name".to_owned());
                }
                let backup = generation_root.join(&file.backup_file);
                validate_existing_path(generation_root, &backup)?;
                let bytes = read_bounded_under(generation_root, &backup, MAX_PROFILE_BYTES)?;
                validate_backup_bytes(file, &bytes)?;
            }
            ProfileBackupReceipt::Absent { .. } => {
                ensure_absent_restore_target(&original)?;
            }
        }
    }
    if !configuration_backup_seen || !active_profile_backup_seen {
        return Err("legacy migration receipt is missing a required profile backup".to_owned());
    }
    Ok(())
}

pub(super) fn write_receipt(
    generation_root: &Path,
    receipt: &MigrationReceipt,
) -> Result<(), String> {
    atomic_json(&generation_root.join(RECEIPT_FILE), receipt)
}

pub(super) fn append_stage(
    generation_root: &Path,
    receipt: &mut MigrationReceipt,
    expected: MigrationStage,
    next: MigrationStage,
) -> Result<(), String> {
    if receipt.current_stage() != Some(expected) {
        return Err(format!(
            "legacy migration stage must be {expected:?} before {next:?}"
        ));
    }
    receipt.completed_stages.push(next);
    write_receipt(generation_root, receipt)
}

pub(super) fn snapshot_matches(receipt: &MigrationReceipt) -> Result<LegacyScmSnapshot, String> {
    let snapshot = legacy_mactray::migration_snapshot(
        crate::machine_integration::registry_conflict_detected(),
    )?;
    // The migration deliberately disables the legacy start type between the stop
    // and the funeral, so treat start type as a runtime setting rather than an
    // identity field: the binary path, account, type, and dependencies still pin
    // identity, and a foreign swap would change one of those, not just the start
    // type. Normalize it before the equality check so the parked service still
    // matches the backup receipt.
    let mut live_configuration = snapshot.configuration.clone();
    live_configuration.start_type = receipt.service.configuration.start_type;
    if live_configuration != receipt.service.configuration
        || snapshot.extended != receipt.service.extended
    {
        return Err("legacy SCM configuration changed after backup".to_owned());
    }
    if !matches!(
        snapshot.state,
        ServiceRuntimeState::Running | ServiceRuntimeState::Stopped
    ) {
        return Err("legacy SCM service is in a transitional state".to_owned());
    }
    Ok(snapshot)
}
