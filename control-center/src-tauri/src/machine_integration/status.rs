use super::{
    appinit::appinit_conflict, legacy_mactray, open_service, LegacyTrayConflictState, MachineStatus,
};
use crate::service_contract::SystemServiceStatus;

pub(super) fn project_new_service_capabilities(
    mut status: SystemServiceStatus,
    registry_conflict: bool,
    legacy_tray_conflict: LegacyTrayConflictState,
) -> SystemServiceStatus {
    if registry_conflict || legacy_tray_conflict != LegacyTrayConflictState::Clear {
        let can_stop = status.can_stop
            && status.backend == crate::service_contract::ServiceBackend::OpenSource
            && status.runtime == crate::service_contract::RuntimeState::Running;
        status.can_install = false;
        status.can_remove = false;
        status.can_start = false;
        status.can_stop = can_stop;
        status.can_repair = false;
        status.can_upgrade = false;
    }
    status
}

pub(super) fn project_system_injection_active(
    status: &SystemServiceStatus,
    registry_conflict: bool,
    legacy_tray_conflict: LegacyTrayConflictState,
    expected_profile_digest: Option<&str>,
) -> bool {
    !registry_conflict
        && legacy_tray_conflict == LegacyTrayConflictState::Clear
        && status.system_injection_active(expected_profile_digest)
}

pub(crate) fn status(active_profile: Option<&[u8]>) -> MachineStatus {
    let expected_profile_digest = active_profile.map(|profile| {
        mactype_service_contract::GenerationId::from_profile_bytes(profile)
            .as_str()
            .to_owned()
    });
    let registry_conflict = appinit_conflict().unwrap_or(true);
    let legacy_tray = legacy_mactray::tray_status();
    let raw_new_service = open_service::status();
    let system_injection_active = project_system_injection_active(
        &raw_new_service,
        registry_conflict,
        legacy_tray.conflict,
        expected_profile_digest.as_deref(),
    );
    let legacy_blocks_activation =
        legacy_mactray::legacy_service_blocks_activation().unwrap_or(true);
    let legacy_service = open_service::legacy_status(
        registry_conflict,
        system_injection_active,
        expected_profile_digest.as_deref(),
        legacy_blocks_activation,
    );
    let new_service =
        project_new_service_capabilities(raw_new_service, registry_conflict, legacy_tray.conflict);
    MachineStatus {
        new_service,
        legacy_service,
        legacy_tray,
        registry_conflict,
        system_injection_active,
        expected_profile_digest,
    }
}
