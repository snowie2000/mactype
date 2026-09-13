use super::{model::*, receipt::*, storage::*};
use crate::machine_integration::legacy_mactray::{self, ServicePresence, ServiceRuntimeState};
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn prepare_backup() -> Result<MigrationReceipt, String> {
    let status = legacy_mactray::status(crate::machine_integration::registry_conflict_detected());
    require_owned_legacy_service(&status)?;
    if !matches!(
        status.state,
        ServiceRuntimeState::Running | ServiceRuntimeState::Stopped
    ) {
        return Err("legacy SCM service must be stably running or stopped".to_owned());
    }
    let service = legacy_mactray::migration_snapshot(status.registry_conflict)?;
    let installation_root = legacy_mactray::trusted_installation_root()
        .ok_or_else(|| "trusted Program Files MacType installation was not found".to_owned())?;
    let configuration = installation_root.join("MacType.ini");
    let configuration_bytes =
        read_optional_regular_bounded_under(&installation_root, &configuration, MAX_PROFILE_BYTES)?;
    let alternative = match configuration_bytes.as_deref() {
        Some(bytes) => crate::profile::legacy_alternative_file_bytes(bytes)?,
        None => None,
    };
    let active_profile = alternative
        .as_deref()
        .map(|path| contained_profile_path(&installation_root, path))
        .transpose()?
        .unwrap_or_else(|| configuration.clone());
    let same_profile = configuration
        .to_string_lossy()
        .eq_ignore_ascii_case(&active_profile.to_string_lossy());
    let active_profile_bytes = if same_profile {
        None
    } else {
        read_optional_regular_bounded_under(&installation_root, &active_profile, MAX_PROFILE_BYTES)?
    };

    let storage = create_migration_storage_root()?;
    let generation = generation_name()?;
    let generation_root = secure_create_tree(&storage, &[&generation])?;
    after_hardening_with(
        &generation_root,
        harden_machine_directory,
        |generation_root| {
            let mut files = vec![profile_backup_receipt(
                generation_root,
                &configuration,
                if same_profile {
                    BackupRole::ConfigurationAndActiveProfile
                } else {
                    BackupRole::Configuration
                },
                configuration_bytes.as_deref(),
            )?];
            if !same_profile {
                files.push(profile_backup_receipt(
                    generation_root,
                    &active_profile,
                    BackupRole::ActiveProfile,
                    active_profile_bytes.as_deref(),
                )?);
            }
            after_registry_export_with(
                generation_root,
                export_service_registry,
                |generation_root, service_registry| {
                    let prepared_unix_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(|error| error.to_string())?
                        .as_millis()
                        .try_into()
                        .map_err(|_| "system clock exceeds receipt range".to_owned())?;
                    let receipt = MigrationReceipt {
                        schema: RECEIPT_SCHEMA.to_owned(),
                        version: RECEIPT_VERSION,
                        generation: generation.clone(),
                        installation_root: installation_root.to_string_lossy().into_owned(),
                        service,
                        completed_stages: vec![MigrationStage::BackupPrepared],
                        service_registry,
                        files,
                        prepared_unix_ms,
                    };
                    write_receipt(generation_root, &receipt)?;
                    validate_receipt(generation_root, &receipt)?;
                    atomic_json(
                        &storage.join(CURRENT_FILE),
                        &CurrentMigration {
                            schema: RECEIPT_SCHEMA.to_owned(),
                            version: RECEIPT_VERSION,
                            generation,
                        },
                    )?;
                    Ok(receipt)
                },
            )
        },
    )
}

pub(crate) fn stop_legacy() -> Result<MigrationReceipt, String> {
    let (generation_root, mut receipt) = current_receipt()?;
    if receipt.current_stage() == Some(MigrationStage::LegacyStopped) {
        return Ok(receipt);
    }
    if receipt.current_stage() != Some(MigrationStage::BackupPrepared) {
        return Err("legacy service can only be stopped after a valid backup".to_owned());
    }
    snapshot_matches(&receipt)?;
    legacy_mactray::stop_for_migration()?;
    if let Err(error) = append_stage(
        &generation_root,
        &mut receipt,
        MigrationStage::BackupPrepared,
        MigrationStage::LegacyStopped,
    ) {
        let restore = legacy_mactray::restore_running_state_after_migration(&receipt.service);
        return Err(match restore {
            Ok(()) => format!("could not record legacy stop; prior state restored: {error}"),
            Err(restore) => format!(
                "could not record legacy stop ({error}); restoring prior state also failed ({restore})"
            ),
        });
    }
    Ok(receipt)
}

pub(crate) fn remove_after_verified(
    verification: RemovalVerification,
) -> Result<MigrationReceipt, String> {
    let validated_backup = current_receipt();
    require_removal_verification(verification, validated_backup.is_ok())?;
    let (generation_root, mut receipt) = validated_backup?;
    if receipt.current_stage() == Some(MigrationStage::LegacyRemoved) {
        return Ok(receipt);
    }
    if receipt.current_stage() != Some(MigrationStage::LegacyStopped) {
        return Err("legacy service must be stopped before verified removal".to_owned());
    }
    let snapshot = snapshot_matches(&receipt)?;
    if snapshot.state != ServiceRuntimeState::Stopped {
        return Err("legacy service resumed after migration stop".to_owned());
    }
    if let Err(error) = legacy_mactray::remove_for_migration() {
        // Do not roll back on a removal failure: rollback restarts the legacy
        // service, but the new service is already the verified live injector by
        // now, so a restart would double-inject. The legacy service stays stopped
        // and disabled (migration_stop), which is safe; the caller can retry.
        return Err(format!(
            "legacy removal failed; the legacy service remains stopped and disabled: {error}"
        ));
    }
    if let Err(error) = append_stage(
        &generation_root,
        &mut receipt,
        MigrationStage::LegacyStopped,
        MigrationStage::LegacyRemoved,
    ) {
        // The destructive delete already succeeded; do not recreate the service
        // the user just removed. Only the receipt bookkeeping failed to advance.
        return Err(format!(
            "legacy service was removed but recording the removal receipt failed: {error}"
        ));
    }
    Ok(receipt)
}

pub(super) trait RollbackBackend {
    fn stop_before_restore(&mut self) -> Result<(), String>;
    fn restore_profiles(&mut self) -> Result<(), String>;
    fn restore_service_configuration(&mut self) -> Result<(), String>;
    fn restore_legacy_tray_startup(&mut self) -> Result<(), String>;
    fn restore_running_state(&mut self) -> Result<(), String>;
}

pub(super) fn perform_rollback(backend: &mut impl RollbackBackend) -> Result<(), String> {
    backend.stop_before_restore()?;
    backend.restore_profiles()?;
    backend.restore_service_configuration()?;
    backend.restore_legacy_tray_startup()?;
    backend.restore_running_state()
}

struct SystemRollback<'a> {
    generation_root: &'a Path,
    receipt: &'a MigrationReceipt,
}

impl SystemRollback<'_> {
    fn restore_file(&self, file: &BackupFileReceipt) -> Result<(), String> {
        let backup = self.generation_root.join(&file.backup_file);
        let bytes = read_bounded_under(self.generation_root, &backup, MAX_PROFILE_BYTES)?;
        validate_backup_bytes(file, &bytes)?;
        let destination = PathBuf::from(&file.original_path);
        let installation_root = PathBuf::from(&self.receipt.installation_root);
        validate_existing_path(&installation_root, &destination)?;
        atomic_write(&destination, &bytes)
    }

    fn restore_profile(&self, file: &ProfileBackupReceipt) -> Result<(), String> {
        match file {
            ProfileBackupReceipt::Present(file) => self.restore_file(file),
            ProfileBackupReceipt::Absent { original_path, .. } => {
                let destination = PathBuf::from(original_path);
                let installation_root = PathBuf::from(&self.receipt.installation_root);
                validate_existing_path(&installation_root, &destination)?;
                ensure_absent_restore_target(&destination)
            }
        }
    }
}

impl RollbackBackend for SystemRollback<'_> {
    fn stop_before_restore(&mut self) -> Result<(), String> {
        let status =
            legacy_mactray::status(crate::machine_integration::registry_conflict_detected());
        if status.registry_conflict {
            return Err("AppInit registry mode blocks safe legacy rollback".to_owned());
        }
        match status.presence {
            ServicePresence::Absent => Ok(()),
            ServicePresence::Owned | ServicePresence::CompatibleUnquoted => {
                legacy_mactray::stop_for_migration()
            }
            ServicePresence::Foreign
            | ServicePresence::DeletePending
            | ServicePresence::Inaccessible => {
                Err("unsafe SCM state blocks legacy rollback".to_owned())
            }
        }
    }

    fn restore_profiles(&mut self) -> Result<(), String> {
        for file in self
            .receipt
            .files
            .iter()
            .filter(|file| file.role() == BackupRole::ActiveProfile)
        {
            self.restore_profile(file)?;
        }
        for file in self
            .receipt
            .files
            .iter()
            .filter(|file| file.role() != BackupRole::ActiveProfile)
        {
            self.restore_profile(file)?;
        }
        Ok(())
    }

    fn restore_service_configuration(&mut self) -> Result<(), String> {
        legacy_mactray::restore_configuration_after_migration(&self.receipt.service)
    }

    fn restore_legacy_tray_startup(&mut self) -> Result<(), String> {
        super::restore_startup_scope(super::StartupReceiptScope::LocalMachine)
    }

    fn restore_running_state(&mut self) -> Result<(), String> {
        legacy_mactray::restore_running_state_after_migration(&self.receipt.service)
    }
}

pub(crate) fn rollback() -> Result<MigrationReceipt, String> {
    let (generation_root, mut receipt) = current_receipt()?;
    if receipt.current_stage() == Some(MigrationStage::RollbackCompleted) {
        return Ok(receipt);
    }
    let mut backend = SystemRollback {
        generation_root: &generation_root,
        receipt: &receipt,
    };
    perform_rollback(&mut backend)?;
    let expected = receipt
        .current_stage()
        .ok_or_else(|| "legacy migration receipt has no completed stage".to_owned())?;
    append_stage(
        &generation_root,
        &mut receipt,
        expected,
        MigrationStage::RollbackCompleted,
    )?;
    Ok(receipt)
}
