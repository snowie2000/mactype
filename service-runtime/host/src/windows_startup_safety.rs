mod appinit;
mod service_registry;

use std::path::Path;

use mactype_service_contract::{effective_service_name, StructuredServiceError};
use mactype_service_platform::RegistryView;

use crate::StartupSafetySnapshot;
use service_registry::ServiceManager;

pub struct WindowsStartupSafety;

impl WindowsStartupSafety {
    pub fn inspect(service_binary: &Path) -> Result<StartupSafetySnapshot, StructuredServiceError> {
        let manager = ServiceManager::open()?;
        let configured_image = manager.service_image(effective_service_name())?;
        let expected_image = format!(r#""{}" --service"#, service_binary.to_string_lossy());
        let current_executable = std::env::current_exe().map_err(|error| {
            service_error(
                "open-service-identity-unavailable",
                "the running service executable path could not be read",
                error.raw_os_error(),
            )
        })?;
        let current_owned = same_windows_path(&current_executable, service_binary)?;
        let open_service_image_owned = current_owned
            && configured_image
                .trim()
                .eq_ignore_ascii_case(expected_image.as_str());

        Ok(StartupSafetySnapshot {
            app_init32_enabled: appinit::mactype_enabled(RegistryView::Native32)?,
            app_init64_enabled: appinit::mactype_enabled(RegistryView::Native64)?,
            legacy_state: manager.legacy_state()?,
            open_service_image_owned,
        })
    }

    pub fn verify(service_binary: &Path) -> Result<(), StructuredServiceError> {
        Self::inspect(service_binary)?.validate()
    }
}

fn same_windows_path(left: &Path, right: &Path) -> Result<bool, StructuredServiceError> {
    let left = std::fs::canonicalize(left).map_err(|error| {
        service_error(
            "open-service-identity-unavailable",
            "the running service path could not be canonicalized",
            error.raw_os_error(),
        )
    })?;
    let right = std::fs::canonicalize(right).map_err(|error| {
        service_error(
            "open-service-identity-unavailable",
            "the expected protected service path could not be canonicalized",
            error.raw_os_error(),
        )
    })?;
    Ok(left
        .to_string_lossy()
        .replace('/', "\\")
        .eq_ignore_ascii_case(&right.to_string_lossy().replace('/', "\\")))
}

fn service_error(code: &str, message: &str, win32_error: Option<i32>) -> StructuredServiceError {
    StructuredServiceError {
        code: code.to_owned(),
        message: message.to_owned(),
        win32_error: win32_error.map(|code| code as u32),
    }
}
