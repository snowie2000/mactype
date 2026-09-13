mod health;

pub(super) use health::read_health_for_scm_process;
use health::{read_health, read_persisted_health};

use super::{
    identity::{
        core_service_capabilities, core_service_configuration_drift, owned_core_service_identity,
        ObservedCoreServiceConfiguration,
    },
    windows::{machine_roots, RuntimePointer},
    *,
};
use mactype_service_contract::{HealthState as ContractHealthState, SERVICE_NAME};
use mactype_service_platform::{
    known_folder_path, KnownFolder, ServiceAccess, ServiceControlManager, ServiceManagerAccess,
    ServiceState,
};
use windows_sys::Win32::Foundation::ERROR_SERVICE_MARKED_FOR_DELETE;

pub(super) fn known_folder(folder: KnownFolder) -> Result<PathBuf, String> {
    known_folder_path(folder).map_err(|error| {
        format!(
            "SHGetKnownFolderPath failed with HRESULT {:#x}",
            error.raw_os_error().unwrap_or(0)
        )
    })
}

fn runtime_state(state: ServiceState) -> RuntimeState {
    match state {
        ServiceState::Stopped => RuntimeState::Stopped,
        ServiceState::StartPending => RuntimeState::StartPending,
        ServiceState::Running => RuntimeState::Running,
        ServiceState::StopPending => RuntimeState::StopPending,
        ServiceState::Paused => RuntimeState::Paused,
        _ => RuntimeState::Unknown,
    }
}

pub(in crate::machine_integration::open_service) fn running_service_process_id(
) -> Result<u32, String> {
    let manager = ServiceControlManager::connect(ServiceManagerAccess::Connect)
        .map_err(|error| error.to_string())?;
    let service = manager
        .open(SERVICE_NAME, ServiceAccess::QueryStatus)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "the new service has no stable running SCM process".to_owned())?;
    let status = service.status().map_err(|error| {
        format!(
            "QueryServiceStatusEx failed with {}",
            error.raw_os_error().unwrap_or(0)
        )
    })?;
    if runtime_state(status.state) != RuntimeState::Running || status.process_id == 0 {
        return Err("the new service has no stable running SCM process".to_owned());
    }
    Ok(status.process_id)
}

pub(super) fn query() -> SystemServiceStatus {
    let manager = match ServiceControlManager::connect(ServiceManagerAccess::Connect) {
        Ok(manager) => manager,
        Err(error) => {
            return inaccessible(error.raw_os_error().unwrap_or(0) as u32, None);
        }
    };
    let service = match manager.open(SERVICE_NAME, ServiceAccess::QueryStatusAndConfig) {
        Ok(Some(service)) => service,
        Ok(None) => return absent_status(),
        Err(error) => {
            let code = error.raw_os_error().unwrap_or(0) as u32;
            return if error.raw_os_error() == Some(ERROR_SERVICE_MARKED_FOR_DELETE as i32) {
                SystemServiceStatus {
                    backend: ServiceBackend::OpenSource,
                    installation: InstallationState::DeletePending,
                    runtime: RuntimeState::Unknown,
                    win32_error: Some(code),
                    can_install: false,
                    ..absent_status()
                }
            } else {
                inaccessible(code, None)
            };
        }
    };
    let configuration = match service.config() {
        Ok(configuration) => configuration,
        Err(error) => return inaccessible(error.raw_os_error().unwrap_or(0) as u32, None),
    };
    let binary_path = Some(configuration.image_path.clone());
    let status = match service.status() {
        Ok(status) => status,
        Err(error) => {
            return inaccessible(error.raw_os_error().unwrap_or(0) as u32, binary_path);
        }
    };
    let runtime = runtime_state(status.state);
    let service_process_id = status.process_id;
    let (program_files, _) = match machine_roots() {
        Ok(roots) => roots,
        Err(_) => return inaccessible(3, binary_path),
    };
    let service_root = program_files.join("MacType Control Center").join("Service");
    let expected = current_service_binary(&service_root);
    let bundled = bundled_service_binary(&service_root);
    let configured = configured_service_binary(&configuration.image_path);
    let protected = configured
        .as_deref()
        .is_some_and(|path| is_protected_service_binary(&service_root, path));
    let observed = ObservedCoreServiceConfiguration {
        service_type: configuration.service_type,
        start_type: configuration.start_type,
        error_control: configuration.error_control,
        account: &configuration.account,
        display_name: &configuration.display_name,
        load_order_group: &configuration.load_order_group,
        tag_id: configuration.tag_id,
        dependencies_empty: configuration.dependencies.is_empty(),
        protected_image: protected,
    };
    if !owned_core_service_identity(&observed) {
        return SystemServiceStatus {
            backend: ServiceBackend::Foreign,
            installation: InstallationState::Invalid,
            runtime,
            health: HealthState::Unknown,
            binary_path,
            win32_error: None,
            active_profile_digest: None,
            configuration_drift: false,
            can_install: false,
            can_remove: false,
            can_start: false,
            can_stop: false,
            can_repair: false,
            can_upgrade: false,
        };
    }
    let configuration_drift = core_service_configuration_drift(&observed);
    let installation = match (configured.as_ref(), expected.as_ref(), bundled.as_ref()) {
        (Some(configured), Ok(expected), Ok(bundled)) => {
            classify_owned_installation(configured, expected, bundled)
        }
        (_, Err(_), Ok(_)) => InstallationState::Outdated,
        _ => InstallationState::Invalid,
    };
    let live_report = (runtime == RuntimeState::Running)
        .then(read_health)
        .and_then(Result::ok);
    let persisted_report = read_persisted_health(&service_root).ok();
    let selected =
        select_service_health(runtime, service_process_id, live_report, persisted_report);
    let health = selected
        .as_ref()
        .map(|selected| match selected.report.health {
            ContractHealthState::Unknown => HealthState::Unknown,
            ContractHealthState::Initializing => HealthState::Initializing,
            ContractHealthState::Ready => HealthState::Ready,
            ContractHealthState::Degraded => HealthState::Degraded,
            ContractHealthState::Failed => HealthState::Failed,
        })
        .unwrap_or(HealthState::Unknown);
    let active_profile_digest = selected.and_then(|selected| {
        selected
            .live
            .then_some(selected.report.active_profile_digest)
            .flatten()
    });
    let capabilities = core_service_capabilities(runtime, installation, configuration_drift);
    SystemServiceStatus {
        backend: ServiceBackend::OpenSource,
        installation,
        runtime,
        health,
        binary_path,
        win32_error: None,
        active_profile_digest,
        configuration_drift,
        can_install: false,
        can_remove: capabilities.can_remove,
        can_start: capabilities.can_start,
        can_stop: capabilities.can_stop,
        can_repair: capabilities.can_repair,
        can_upgrade: capabilities.can_upgrade,
    }
}

fn current_service_binary(service_root: &Path) -> Result<PathBuf, String> {
    let pointer_path = service_root.join("current.json");
    let pointer: RuntimePointer = serde_json::from_slice(&read_bounded_regular_file(
        &pointer_path,
        64 * 1024,
        "protected runtime pointer",
    )?)
    .map_err(|error| error.to_string())?;
    if pointer.schema != 1 || !safe_version(&pointer.version) {
        return Err("invalid protected runtime pointer".to_owned());
    }
    let binary = service_root
        .join("bin")
        .join(pointer.version)
        .join("mactype-service.exe");
    reject_reparse_chain(&binary)?;
    if !binary.is_file() {
        return Err("protected service binary is missing".to_owned());
    }
    Ok(binary)
}

pub(super) fn safe_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        && !matches!(value, "." | "..")
}

pub(super) fn reveal_system_service() -> Result<(), String> {
    let status = query();
    let (program_files, _) = machine_roots()?;
    let service_root = program_files.join("MacType Control Center").join("Service");
    let binary = validated_reveal_binary(&service_root, &status)?;
    reject_reparse_chain(&binary)?;
    let metadata = fs::metadata(&binary).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("the protected system service binary is missing".to_owned());
    }
    let explorer = known_folder(KnownFolder::Windows)?.join("explorer.exe");
    reject_reparse_chain(&explorer)?;
    if !explorer.is_file() {
        return Err("the fixed Windows Explorer executable is unavailable".to_owned());
    }
    let mut selection = OsString::from("/select,");
    selection.push(&binary);
    Command::new(explorer)
        .arg(selection)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn inaccessible(error: u32, binary_path: Option<String>) -> SystemServiceStatus {
    SystemServiceStatus {
        backend: ServiceBackend::None,
        installation: InstallationState::Inaccessible,
        runtime: RuntimeState::Unknown,
        health: HealthState::Unknown,
        binary_path,
        win32_error: Some(error),
        active_profile_digest: None,
        configuration_drift: false,
        can_install: false,
        can_remove: false,
        can_start: false,
        can_stop: false,
        can_repair: false,
        can_upgrade: false,
    }
}
