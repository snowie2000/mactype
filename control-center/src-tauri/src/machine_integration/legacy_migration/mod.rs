mod model;
mod receipt;
mod startup_receipt;
mod storage;
mod transaction;

pub(crate) use model::RemovalVerification;
pub(crate) use receipt::{backup_is_valid, current_stage_name};
pub(crate) use startup_receipt::{
    StartupReceiptScope, disable_startup_scope, dispatch_current_user_restore_command,
    restore_startup_scope,
};
pub(super) use transaction::prepare_backup;
pub(crate) use transaction::{remove_after_verified, rollback, stop_legacy};

#[cfg(test)]
use super::legacy_mactray::{
    LegacyServiceStatus, ServiceConfiguration, ServicePresence, ServiceRuntimeState,
};
#[cfg(test)]
use model::{
    BackupFileReceipt, BackupRole, CONFIGURATION_BACKUP, CURRENT_FILE, MAX_RECEIPT_BYTES,
    MAX_REGISTRY_EXPORT_BYTES, ProfileBackupReceipt, RECEIPT_FILE, RegistryExportReceipt,
    SERVICE_REGISTRY_EXPORT, contained_profile_path, hex_sha256, require_owned_legacy_service,
    require_removal_verification, validate_backup_bytes, validate_registry_export_bytes,
    validate_service_configuration,
};
#[cfg(test)]
use startup_receipt::{
    LegacyTrayStartupReceipt, StartupRestorationState, StartupRestoreBackend,
    build_startup_receipt, restore_startup_with, select_startup_receipt_for_disable,
    user_restore_requested_from_arguments,
};
#[cfg(test)]
use std::{
    ffi::OsString,
    io::Read,
    path::{Path, PathBuf},
};
#[cfg(test)]
use storage::{
    OpenedFileMetadata, acl_invocation, after_hardening_with, after_registry_export_with,
    ensure_absent_restore_target_with, read_bounded_under_with, read_opened_bounded_with,
    registry_export_invocation,
};
#[cfg(test)]
use transaction::{RollbackBackend, perform_rollback};

#[cfg(test)]
mod tests;
