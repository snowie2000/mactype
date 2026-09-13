use super::super::*;
use mactype_service_platform::{
    known_folder_path, KnownFolder, ServiceAccess, ServiceControlManager, ServiceHandle,
    ServiceManagerAccess, ServiceState,
};
use std::path::PathBuf;
use windows_sys::Win32::Foundation::{ERROR_INVALID_DATA, ERROR_SERVICE_DOES_NOT_EXIST};

pub(super) fn win32_code(error: &std::io::Error) -> u32 {
    error
        .raw_os_error()
        .map_or(ERROR_INVALID_DATA, |code| code as u32)
}

fn program_files_root() -> Option<PathBuf> {
    let root = std::fs::canonicalize(known_folder_path(KnownFolder::ProgramFiles).ok()?).ok()?;
    root.is_dir().then_some(root)
}

pub(super) fn expected_mactray_path() -> Option<PathBuf> {
    program_files_root().map(|root| root.join("MacType").join("MacTray.exe"))
}

pub(super) fn trusted_mactray_path() -> Option<PathBuf> {
    let root = program_files_root()?;
    let candidate = std::fs::canonicalize(root.join("MacType").join("MacTray.exe")).ok()?;
    (candidate.is_file() && is_trusted_mactray_layout(&root, &candidate)).then_some(candidate)
}

pub(super) fn query_runtime(service: &ServiceHandle) -> Result<ServiceRuntimeState, u32> {
    let state = service.status().map_err(|error| win32_code(&error))?.state;
    Ok(match state {
        ServiceState::Stopped => ServiceRuntimeState::Stopped,
        ServiceState::StartPending => ServiceRuntimeState::StartPending,
        ServiceState::StopPending => ServiceRuntimeState::StopPending,
        ServiceState::Running => ServiceRuntimeState::Running,
        ServiceState::ContinuePending => ServiceRuntimeState::ContinuePending,
        ServiceState::PausePending => ServiceRuntimeState::PausePending,
        ServiceState::Paused => ServiceRuntimeState::Paused,
        ServiceState::Other(_) => ServiceRuntimeState::Unknown,
    })
}

pub(super) fn query_configuration(service: &ServiceHandle) -> Result<ServiceConfiguration, u32> {
    let configuration = service.config().map_err(|error| win32_code(&error))?;
    Ok(ServiceConfiguration {
        display_name: configuration.display_name,
        binary_path: configuration.image_path,
        service_type: configuration.service_type,
        start_type: configuration.start_type,
        error_control: configuration.error_control,
        load_order_group: (!configuration.load_order_group.is_empty())
            .then_some(configuration.load_order_group),
        tag_id: configuration.tag_id,
        account: configuration.account,
        dependencies: configuration.dependencies,
    })
}

pub(super) fn open_for(access: ServiceAccess) -> Result<ServiceHandle, u32> {
    let manager = ServiceControlManager::connect(ServiceManagerAccess::Connect)
        .map_err(|error| win32_code(&error))?;
    manager
        .open("MacType", access)
        .map_err(|error| win32_code(&error))?
        .ok_or(ERROR_SERVICE_DOES_NOT_EXIST)
}
