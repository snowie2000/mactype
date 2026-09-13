use std::io;

use mactype_service_contract::StructuredServiceError;
use mactype_service_platform::{
    ServiceAccess, ServiceControlManager, ServiceHandle, ServiceManagerAccess, ServiceState,
};
use windows_sys::Win32::Foundation::ERROR_SERVICE_DOES_NOT_EXIST;

use crate::LegacyServiceRuntimeState;

const LEGACY_SERVICE_NAME: &str = "MacType";

pub(super) struct ServiceManager(ServiceControlManager);

impl ServiceManager {
    pub(super) fn open() -> Result<Self, StructuredServiceError> {
        ServiceControlManager::connect(ServiceManagerAccess::Connect)
            .map(Self)
            .map_err(|error| {
                os_error(
                    "scm-inspection-unavailable",
                    "the Service Control Manager could not be opened for inspection",
                    &error,
                )
            })
    }

    pub(super) fn service_image(&self, name: &str) -> Result<String, StructuredServiceError> {
        let service = self.open_service(name, ServiceAccess::QueryConfig)?;
        let config = service.config().map_err(|error| {
            os_error(
                "open-service-config-unavailable",
                "the open service ImagePath could not be read",
                &error,
            )
        })?;
        if config.image_path.is_empty() {
            return Err(service_error(
                "open-service-config-invalid",
                "the open service ImagePath is empty or invalid",
                None,
            ));
        }
        Ok(config.image_path)
    }

    pub(super) fn legacy_state(&self) -> Result<LegacyServiceRuntimeState, StructuredServiceError> {
        let service = match self.0.open(LEGACY_SERVICE_NAME, ServiceAccess::QueryStatus) {
            Ok(Some(service)) => service,
            Ok(None) => return Ok(LegacyServiceRuntimeState::Absent),
            Err(error) => {
                return Err(os_error(
                    "legacy-service-inspection-failed",
                    "the legacy service state could not be inspected",
                    &error,
                ))
            }
        };
        let status = service.status().map_err(|error| {
            os_error(
                "legacy-service-inspection-failed",
                "the legacy service state could not be read",
                &error,
            )
        })?;
        Ok(match status.state {
            ServiceState::Stopped => LegacyServiceRuntimeState::Stopped,
            ServiceState::StartPending => LegacyServiceRuntimeState::StartPending,
            ServiceState::Running => LegacyServiceRuntimeState::Running,
            _ => LegacyServiceRuntimeState::Unknown,
        })
    }

    fn open_service(
        &self,
        name: &str,
        access: ServiceAccess,
    ) -> Result<ServiceHandle, StructuredServiceError> {
        const CODE: &str = "open-service-config-unavailable";
        const MESSAGE: &str = "the fixed open service registration could not be opened";
        match self.0.open(name, access) {
            Ok(Some(service)) => Ok(service),
            Ok(None) => Err(service_error(
                CODE,
                MESSAGE,
                Some(ERROR_SERVICE_DOES_NOT_EXIST as i32),
            )),
            Err(error) => Err(os_error(CODE, MESSAGE, &error)),
        }
    }
}

/// A structured error carrying the Win32 code of the platform failure.
fn os_error(code: &str, message: &str, error: &io::Error) -> StructuredServiceError {
    service_error(code, message, error.raw_os_error())
}

fn service_error(code: &str, message: &str, win32_error: Option<i32>) -> StructuredServiceError {
    StructuredServiceError {
        code: code.to_owned(),
        message: message.to_owned(),
        win32_error: win32_error.map(|code| code as u32),
    }
}
