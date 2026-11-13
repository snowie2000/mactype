use super::MigrationVerification;
use crate::machine_integration::{legacy_mactray, legacy_migration};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyMacTrayStatus {
    pub presence: legacy_mactray::ServicePresence,
    pub state: legacy_mactray::ServiceRuntimeState,
    pub binary_path: Option<String>,
    pub win32_error: Option<u32>,
    pub trusted_binary_available: bool,
    pub registry_conflict: bool,
    pub can_remove: bool,
    pub can_stop: bool,
    pub migration_available: bool,
    pub migration_backup_available: bool,
    pub blocks_activation: bool,
}

pub(super) fn legacy_migration_available(status: &legacy_mactray::LegacyServiceStatus) -> bool {
    let owned = matches!(
        status.presence,
        legacy_mactray::ServicePresence::Owned
            | legacy_mactray::ServicePresence::CompatibleUnquoted
    );
    owned
        && !status.registry_conflict
        && match status.state {
            legacy_mactray::ServiceRuntimeState::Stopped => true,
            legacy_mactray::ServiceRuntimeState::Running => status.trusted_binary_available,
            _ => false,
        }
}

pub(crate) fn legacy_status(
    registry_conflict: bool,
    system_service_active: bool,
    expected_profile_digest: Option<&str>,
    blocks_activation: bool,
) -> Option<LegacyMacTrayStatus> {
    let status = legacy_mactray::status(registry_conflict);
    if status.presence == legacy_mactray::ServicePresence::Absent {
        return None;
    }
    let owned = matches!(
        status.presence,
        legacy_mactray::ServicePresence::Owned
            | legacy_mactray::ServicePresence::CompatibleUnquoted
    );
    let backup_available = legacy_migration::backup_is_valid();
    let migration_verified =
        system_service_active && expected_profile_digest.is_some_and(migration_removal_verified);
    let migration_available = blocks_activation && legacy_migration_available(&status);
    Some(LegacyMacTrayStatus {
        presence: status.presence,
        state: status.state,
        binary_path: status.binary_path,
        win32_error: status.win32_error,
        trusted_binary_available: status.trusted_binary_available,
        registry_conflict: status.registry_conflict,
        // Removal also requires the legacy service currently Stopped: the backend
        // deletes only a stopped service, so a resumed/auto-restarted legacy must
        // not offer an enabled "Remove" the command would reject.
        can_remove: owned
            && matches!(status.state, legacy_mactray::ServiceRuntimeState::Stopped)
            && backup_available
            && migration_verified,
        can_stop: status.can_stop,
        migration_available,
        migration_backup_available: backup_available,
        blocks_activation,
    })
}

fn migration_removal_verified(expected: &str) -> bool {
    #[cfg(windows)]
    {
        super::windows::system_removal_verification(expected)
            .is_ok_and(MigrationVerification::permits_removal)
    }
    #[cfg(not(windows))]
    {
        let _ = expected;
        false
    }
}
