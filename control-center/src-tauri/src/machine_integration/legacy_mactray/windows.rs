use super::*;
use std::path::PathBuf;

mod common;
mod control;
mod restore;
mod snapshot;

pub(super) fn expected_mactray_path() -> Option<PathBuf> {
    common::expected_mactray_path()
}

pub(super) fn query(registry_conflict: bool) -> LegacyServiceStatus {
    control::query(registry_conflict)
}

pub(super) fn migration_snapshot(registry_conflict: bool) -> Result<LegacyScmSnapshot, String> {
    snapshot::migration_snapshot(registry_conflict)
}

pub(super) fn validate_snapshot_for_restore(snapshot: &LegacyScmSnapshot) -> Result<(), String> {
    restore::validate_snapshot_for_restore(snapshot)
}

pub(super) fn migration_stop() -> Result<(), String> {
    control::migration_stop()
}

pub(super) fn migration_remove() -> Result<(), String> {
    control::migration_remove()
}

pub(super) fn migration_restore_configuration(snapshot: &LegacyScmSnapshot) -> Result<(), String> {
    restore::restore_service_configuration(snapshot)
}

pub(super) fn migration_restore_running_state(snapshot: &LegacyScmSnapshot) -> Result<(), String> {
    control::migration_restore_running_state(snapshot)
}
