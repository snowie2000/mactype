use super::{DesignationEffect, MachineAction, MachineBackend, MachineStatus};
use crate::{
    machine_integration::{
        open_service::action_failure::ActionFailure, orchestrator::execute_machine_action_with,
    },
    service_contract::{
        InstallationState, RuntimeState, ServiceBackend, ServiceManagementPackageState,
        SystemServiceStatus,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DesignationPlan {
    PublishLive,
    HoldForNextStart,
    KeepLocalUntilServiceStart,
}

pub(crate) fn profile_publication_supported(
    service: &SystemServiceStatus,
    registry_mode_detected: bool,
) -> bool {
    !registry_mode_detected
        && service.backend != ServiceBackend::Foreign
        && matches!(
            service.installation,
            InstallationState::Absent | InstallationState::Current | InstallationState::Outdated
        )
        && matches!(
            service.runtime,
            RuntimeState::Running | RuntimeState::Stopped
        )
}

pub(super) fn system_modes_supported(
    service: &SystemServiceStatus,
    registry_mode_detected: bool,
    service_management_package: ServiceManagementPackageState,
) -> bool {
    service_management_package == ServiceManagementPackageState::Ready
        && profile_publication_supported(service, registry_mode_detected)
}

pub(crate) fn designation_plan(status: &MachineStatus) -> DesignationPlan {
    designation_plan_with_package(status, super::service_management_package_state())
}

pub(super) fn designation_plan_with_package(
    status: &MachineStatus,
    service_management_package: ServiceManagementPackageState,
) -> DesignationPlan {
    let service = &status.new_service;
    if service.runtime == RuntimeState::Running {
        DesignationPlan::PublishLive
    } else if service.runtime == RuntimeState::Stopped
        && matches!(
            service.installation,
            InstallationState::Current | InstallationState::Outdated
        )
        && system_modes_supported(
            service,
            status.registry_conflict,
            service_management_package,
        )
        && !status.registry_conflict
        && !status.legacy_tray.blocks_machine_change()
    {
        DesignationPlan::HoldForNextStart
    } else {
        DesignationPlan::KeepLocalUntilServiceStart
    }
}

pub(super) fn designation_status_with(backend: &mut impl MachineBackend) -> MachineStatus {
    let registry_conflict = backend.appinit_conflict().unwrap_or(true);
    let legacy_tray = backend.legacy_tray_status();
    let new_service = backend.new_service_status();
    MachineStatus {
        new_service,
        legacy_service: None,
        legacy_tray,
        registry_conflict,
        system_injection_active: false,
        expected_profile_digest: None,
    }
}

#[cfg(test)]
pub(super) fn designate_run_profile_with(
    backend: &mut impl MachineBackend,
    profile: &[u8],
) -> Result<DesignationEffect, ActionFailure> {
    let status = designation_status_with(backend);
    let plan = designation_plan_with_package(&status, backend.service_management_package_state());
    execute_designation_plan_with(backend, profile, plan)
}

pub(super) fn execute_designation_plan_with(
    backend: &mut impl MachineBackend,
    profile: &[u8],
    plan: DesignationPlan,
) -> Result<DesignationEffect, ActionFailure> {
    if profile.is_empty() || profile.len() > mactype_service_contract::MAX_PROFILE_BYTES {
        return Err(ActionFailure::internal(
            "the designated profile payload is outside the allowed range",
        ));
    }
    match plan {
        DesignationPlan::PublishLive => {
            execute_machine_action_with(backend, MachineAction::PublishProfile, Some(profile))?;
            Ok(DesignationEffect::Live)
        }
        DesignationPlan::HoldForNextStart => {
            execute_machine_action_with(backend, MachineAction::DesignateProfile, Some(profile))?;
            Ok(DesignationEffect::NextStart)
        }
        DesignationPlan::KeepLocalUntilServiceStart => Ok(DesignationEffect::NextStart),
    }
}
