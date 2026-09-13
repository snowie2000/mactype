use mactype_service_contract::StructuredServiceError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyServiceRuntimeState {
    Absent,
    Stopped,
    StartPending,
    Running,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupSafetySnapshot {
    pub app_init32_enabled: bool,
    pub app_init64_enabled: bool,
    pub legacy_state: LegacyServiceRuntimeState,
    pub open_service_image_owned: bool,
}

impl StartupSafetySnapshot {
    pub fn validate(&self) -> Result<(), StructuredServiceError> {
        if self.app_init32_enabled || self.app_init64_enabled {
            return Err(service_error(
                "appinit-conflict",
                "enabled AppInit injection conflicts with the open service",
            ));
        }
        if !matches!(
            self.legacy_state,
            LegacyServiceRuntimeState::Absent | LegacyServiceRuntimeState::Stopped
        ) {
            return Err(service_error(
                "legacy-service-conflict",
                "the legacy MacType service must be stopped before the open service becomes ready",
            ));
        }
        if !self.open_service_image_owned {
            return Err(service_error(
                "open-service-ownership-conflict",
                "the SCM service image is not the fixed active protected runtime",
            ));
        }
        Ok(())
    }
}

fn service_error(code: &str, message: &str) -> StructuredServiceError {
    StructuredServiceError {
        code: code.to_owned(),
        message: message.to_owned(),
        win32_error: None,
    }
}
