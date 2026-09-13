use std::path::Path;

use mactype_service_contract::MachinePaths;

use super::scm::ServiceManager;
use crate::{
    InstalledRuntime, OpenServiceObservation, RuntimeInstaller, RuntimeServiceBinding, SetupError,
};

pub(super) fn recover(
    paths: &MachinePaths,
    manager: &ServiceManager,
) -> Result<Option<InstalledRuntime>, SetupError> {
    RuntimeInstaller::new(paths.clone()).recover_interrupted_activation_with_service_binding(
        |candidate, previous| inspect_binding(manager, candidate, previous),
        |candidate, previous| restore_previous_binding(manager, candidate, previous),
    )
}

fn inspect_binding(
    manager: &ServiceManager,
    candidate: Option<&Path>,
    previous: Option<&Path>,
) -> Result<RuntimeServiceBinding, SetupError> {
    match manager.observe_fixed_service() {
        OpenServiceObservation::Absent => Ok(RuntimeServiceBinding::Absent),
        OpenServiceObservation::OwnedStopped | OpenServiceObservation::OwnedRunning => {
            if let Some(candidate) = candidate {
                if manager.owned_service_points_to(candidate)? {
                    return Ok(RuntimeServiceBinding::Candidate);
                }
            }
            if let Some(previous) = previous {
                if manager.owned_service_points_to(previous)? {
                    return Ok(RuntimeServiceBinding::Previous);
                }
            }
            Err(SetupError::CleanupUnknown(
                "owned service image matches neither side of the runtime activation receipt"
                    .to_owned(),
            ))
        }
        OpenServiceObservation::Foreign => Err(SetupError::CleanupUnknown(
            "fixed service name became foreign during runtime activation recovery".to_owned(),
        )),
        OpenServiceObservation::Unknown => Err(SetupError::CleanupUnknown(
            "fixed service state is unknown during runtime activation recovery".to_owned(),
        )),
    }
}

fn restore_previous_binding(
    manager: &ServiceManager,
    candidate: &Path,
    previous: Option<&Path>,
) -> Result<(), SetupError> {
    match manager.observe_fixed_service() {
        OpenServiceObservation::OwnedRunning => {
            if !manager.owned_service_points_to(candidate)? {
                return Err(candidate_changed_before_rollback());
            }
            manager.stop()?;
        }
        OpenServiceObservation::OwnedStopped => {
            if !manager.owned_service_points_to(candidate)? {
                return Err(candidate_changed_before_rollback());
            }
        }
        OpenServiceObservation::Absent
        | OpenServiceObservation::Foreign
        | OpenServiceObservation::Unknown => {
            return Err(candidate_changed_before_rollback());
        }
    }
    match previous {
        Some(previous) => manager.reconfigure(previous),
        None => manager.remove(),
    }
}

fn candidate_changed_before_rollback() -> SetupError {
    SetupError::CleanupUnknown(
        "candidate service binding changed before exact runtime rollback".to_owned(),
    )
}
