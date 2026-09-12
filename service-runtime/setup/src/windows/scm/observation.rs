use std::path::Path;

use mactype_service_platform::{ServiceAccess, ServiceState};

use super::configuration::{
    observed_configuration, quoted_image_path, service_identity_matches_owned_contract,
};
use super::ServiceManager;
use crate::{ConflictObservation, OpenServiceObservation, SetupError};

fn classify_fixed_service(
    identity_matches: bool,
    state: Option<ServiceState>,
) -> OpenServiceObservation {
    if !identity_matches {
        return OpenServiceObservation::Foreign;
    }
    match state {
        Some(ServiceState::Stopped) => OpenServiceObservation::OwnedStopped,
        Some(ServiceState::Running) => OpenServiceObservation::OwnedRunning,
        _ => OpenServiceObservation::Unknown,
    }
}

impl ServiceManager {
    pub fn observe_fixed_service(&self) -> OpenServiceObservation {
        let service = match self.open_service(ServiceAccess::QueryStatusAndConfig) {
            Ok(Some(service)) => service,
            Ok(None) => return OpenServiceObservation::Absent,
            Err(_) => return OpenServiceObservation::Unknown,
        };
        let config = match service.config() {
            Ok(config) => config,
            Err(_) => return OpenServiceObservation::Unknown,
        };
        let identity_matches = service_identity_matches_owned_contract(
            &self.protected_root,
            &observed_configuration(&config),
        );
        let state = service.status().ok().map(|status| status.state);
        classify_fixed_service(identity_matches, state)
    }

    pub fn observe_legacy_service(&self) -> ConflictObservation {
        match self.open_named_service("MacType", ServiceAccess::QueryStatus) {
            Ok(Some(_)) => ConflictObservation::Detected,
            Ok(None) => ConflictObservation::Clear,
            Err(_) => ConflictObservation::Unknown,
        }
    }

    pub fn owned_service_points_to(&self, expected_binary: &Path) -> Result<bool, SetupError> {
        let Some(service) = self.open_service(ServiceAccess::QueryConfig)? else {
            return Ok(false);
        };
        self.ensure_owned(&service)?;
        let config = service.config()?;
        Ok(config
            .image_path
            .eq_ignore_ascii_case(&quoted_image_path(expected_binary)?))
    }
}

#[cfg(test)]
mod tests {
    use super::classify_fixed_service;
    use crate::OpenServiceObservation;
    use mactype_service_platform::ServiceState;

    #[test]
    fn owned_identity_classifies_running_and_stopped_despite_configuration_drift() {
        assert_eq!(
            classify_fixed_service(true, Some(ServiceState::Stopped)),
            OpenServiceObservation::OwnedStopped
        );
        assert_eq!(
            classify_fixed_service(true, Some(ServiceState::Running)),
            OpenServiceObservation::OwnedRunning
        );
    }

    #[test]
    fn foreign_identity_and_transitional_states_fail_closed() {
        assert_eq!(
            classify_fixed_service(false, Some(ServiceState::Running)),
            OpenServiceObservation::Foreign
        );
        assert_eq!(
            classify_fixed_service(true, Some(ServiceState::StartPending)),
            OpenServiceObservation::Unknown
        );
        assert_eq!(
            classify_fixed_service(true, None),
            OpenServiceObservation::Unknown
        );
    }
}
