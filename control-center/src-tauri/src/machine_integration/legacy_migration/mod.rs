mod model;
mod receipt;
mod startup_receipt;
mod storage;
mod transaction;

pub(crate) use model::RemovalVerification;
pub(crate) use receipt::{backup_is_valid, current_stage_name};
pub(crate) use startup_receipt::{
    disable_startup_scope, dispatch_current_user_restore_command, restore_startup_scope,
    StartupReceiptScope,
};
pub(crate) use transaction::{prepare_backup, remove_after_verified, rollback, stop_legacy};

#[cfg(test)]
use super::legacy_mactray::{
    LegacyServiceStatus, ServiceConfiguration, ServicePresence, ServiceRuntimeState,
};
#[cfg(test)]
use model::{
    contained_profile_path, hex_sha256, require_owned_legacy_service, require_removal_verification,
    validate_backup_bytes, validate_registry_export_bytes, validate_service_configuration,
    BackupFileReceipt, BackupRole, ProfileBackupReceipt, RegistryExportReceipt,
    CONFIGURATION_BACKUP, CURRENT_FILE, MAX_RECEIPT_BYTES, MAX_REGISTRY_EXPORT_BYTES, RECEIPT_FILE,
    SERVICE_REGISTRY_EXPORT,
};
#[cfg(test)]
use startup_receipt::{
    build_startup_receipt, restore_startup_with, select_startup_receipt_for_disable,
    user_restore_requested_from_arguments, LegacyTrayStartupReceipt, StartupRestorationState,
    StartupRestoreBackend,
};
#[cfg(test)]
use std::{
    ffi::OsString,
    io::Read,
    path::{Path, PathBuf},
};
#[cfg(test)]
use storage::{
    acl_invocation, after_hardening_with, after_registry_export_with,
    ensure_absent_restore_target_with, read_bounded_under_with, read_opened_bounded_with,
    registry_export_invocation, validate_path_chain, OpenedFileMetadata,
};
#[cfg(test)]
use transaction::{perform_rollback, RollbackBackend};

#[cfg(test)]
mod tests;
