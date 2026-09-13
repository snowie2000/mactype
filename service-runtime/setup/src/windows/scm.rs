mod configuration;
mod health;
mod lifecycle;
mod observation;

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use mactype_service_contract::effective_service_name;
use mactype_service_platform::{
    ServiceAccess, ServiceControlManager, ServiceHandle, ServiceManagerAccess,
};

use configuration::{
    observed_configuration, service_identity_matches_owned_contract as owns_config,
};
#[cfg(feature = "ci-test-adapter")]
pub use configuration::{
    service_configuration_drift, service_configuration_matches_owned_contract,
    service_identity_matches_owned_contract, service_image_matches_protected_contract,
    ObservedServiceConfiguration,
};

use crate::SetupError;

const DISPLAY_NAME: &str = "MacType Control Center Service";
const DESCRIPTION: &str = "Runs the open MacType machine integration runtime.";
const STATE_TIMEOUT: Duration = Duration::from_secs(30);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(20);

pub struct ServiceManager {
    manager: ServiceControlManager,
    protected_root: PathBuf,
}

impl ServiceManager {
    pub fn connect(protected_root: PathBuf) -> Result<Self, SetupError> {
        let manager = ServiceControlManager::connect(ServiceManagerAccess::ConnectAndCreate)
            .map_err(SetupError::Io)?;
        Ok(Self {
            manager,
            protected_root,
        })
    }

    fn open_service(&self, access: ServiceAccess) -> io::Result<Option<ServiceHandle>> {
        self.open_named_service(effective_service_name(), access)
    }

    fn open_named_service(
        &self,
        service_name: &str,
        access: ServiceAccess,
    ) -> io::Result<Option<ServiceHandle>> {
        self.manager.open(service_name, access)
    }

    fn ensure_owned(&self, service: &ServiceHandle) -> Result<(), SetupError> {
        let config = service.config()?;
        if !owns_config(&self.protected_root, &observed_configuration(&config)) {
            return Err(SetupError::Runtime(
                "the fixed service name has a foreign identity; refusing to mutate it".to_owned(),
            ));
        }
        Ok(())
    }
}
