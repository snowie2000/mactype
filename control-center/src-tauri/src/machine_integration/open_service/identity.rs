use super::runtime::safe_runtime_version;
use crate::service_contract::{
    InstallationState, RuntimeState, ServiceBackend, SystemServiceStatus,
};
use mactype_service_contract::HealthReport;
use std::path::{Path, PathBuf};

pub(super) fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

pub(super) fn configured_service_binary(image_path: &str) -> Option<PathBuf> {
    let rest = image_path.strip_prefix('"')?;
    let quote = rest.find('"')?;
    if &rest[quote + 1..] != " --service" {
        return None;
    }
    Some(PathBuf::from(&rest[..quote]))
}

pub(super) fn is_protected_service_binary(root: &Path, binary: &Path) -> bool {
    let Ok(relative) = binary.strip_prefix(root) else {
        return false;
    };
    let components = relative.components().collect::<Vec<_>>();
    components.len() == 3
        && components[0].as_os_str().eq_ignore_ascii_case("bin")
        && safe_runtime_version(&components[1].as_os_str().to_string_lossy())
        && components[2]
            .as_os_str()
            .eq_ignore_ascii_case("mactype-service.exe")
}

pub(super) fn classify_owned_installation(
    configured: &Path,
    protected_current: &Path,
    bundled: &Path,
) -> InstallationState {
    if same_path(configured, protected_current) && same_path(configured, bundled) {
        InstallationState::Current
    } else {
        InstallationState::Outdated
    }
}

#[derive(Clone, Copy)]
pub(super) struct ObservedCoreServiceConfiguration<'a> {
    pub(super) service_type: u32,
    pub(super) start_type: u32,
    pub(super) error_control: u32,
    pub(super) account: &'a str,
    pub(super) display_name: &'a str,
    pub(super) load_order_group: &'a str,
    pub(super) tag_id: u32,
    pub(super) dependencies_empty: bool,
    pub(super) protected_image: bool,
}

pub(super) fn owned_core_service_identity(observed: &ObservedCoreServiceConfiguration<'_>) -> bool {
    observed.service_type == 0x10
        && observed.account.eq_ignore_ascii_case("LocalSystem")
        && observed.protected_image
}

pub(super) fn core_service_configuration_drift(
    observed: &ObservedCoreServiceConfiguration<'_>,
) -> bool {
    observed.start_type != 2
        || observed.error_control != 1
        || observed.display_name != "MacType Control Center Service"
        || !observed.load_order_group.is_empty()
        || (observed.tag_id != 0 && !observed.load_order_group.is_empty())
        || !observed.dependencies_empty
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CoreServiceCapabilities {
    pub(super) can_remove: bool,
    pub(super) can_start: bool,
    pub(super) can_stop: bool,
    pub(super) can_repair: bool,
    pub(super) can_upgrade: bool,
}

pub(super) fn core_service_capabilities(
    runtime: RuntimeState,
    installation: InstallationState,
    configuration_drift: bool,
) -> CoreServiceCapabilities {
    let stable = matches!(runtime, RuntimeState::Running | RuntimeState::Stopped);
    CoreServiceCapabilities {
        can_remove: stable,
        can_start: runtime == RuntimeState::Stopped
            && installation == InstallationState::Current
            && !configuration_drift,
        can_stop: runtime == RuntimeState::Running,
        can_repair: stable && installation == InstallationState::Current,
        can_upgrade: stable && installation == InstallationState::Outdated,
    }
}

pub(super) struct SelectedHealth {
    pub(super) report: HealthReport,
    pub(super) live: bool,
}

pub(super) struct LiveHealthReport {
    pub(super) server_pid: u32,
    pub(super) report: HealthReport,
}

pub(super) fn select_service_health(
    runtime: RuntimeState,
    scm_process_id: u32,
    live: Option<LiveHealthReport>,
    persisted: Option<HealthReport>,
) -> Option<SelectedHealth> {
    if !matches!(runtime, RuntimeState::Running | RuntimeState::Stopped) {
        return None;
    }
    if runtime == RuntimeState::Running {
        if let Some(live) =
            live.filter(|live| scm_process_id != 0 && live.server_pid == scm_process_id)
        {
            return Some(SelectedHealth {
                report: live.report,
                live: true,
            });
        }
    }
    persisted
        .filter(|report| {
            matches!(
                report.health,
                mactype_service_contract::HealthState::Degraded
                    | mactype_service_contract::HealthState::Failed
            )
        })
        .map(|report| SelectedHealth {
            report,
            live: false,
        })
}

pub(super) fn validated_reveal_binary(
    service_root: &Path,
    status: &SystemServiceStatus,
) -> Result<PathBuf, String> {
    if status.backend != ServiceBackend::OpenSource
        || !matches!(
            status.installation,
            InstallationState::Current | InstallationState::Outdated
        )
        || !matches!(
            status.runtime,
            RuntimeState::Running | RuntimeState::Stopped
        )
    {
        return Err("the system service is not an owned stable installation".to_owned());
    }
    let binary = status
        .binary_path
        .as_deref()
        .and_then(configured_service_binary)
        .ok_or_else(|| "the system service ImagePath is invalid".to_owned())?;
    if !is_protected_service_binary(service_root, &binary) {
        return Err("the system service binary is outside the protected layout".to_owned());
    }
    Ok(binary)
}
