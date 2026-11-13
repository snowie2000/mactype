mod configuration;
mod health;
mod lifecycle;
mod observation;

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use mactype_service_contract::{effective_service_name, owned_service_identity};
use mactype_service_platform::{
    ServiceAccess, ServiceControlManager, ServiceHandle, ServiceManagerAccess,
};

use configuration::observed_configuration;

use crate::SetupError;

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
        if !owned_service_identity(&observed_configuration(&config), &self.protected_root) {
            return Err(SetupError::Runtime(
                "the fixed service name has a foreign identity; refusing to mutate it".to_owned(),
            ));
        }
        Ok(())
    }
}
