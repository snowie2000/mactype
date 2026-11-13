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
    if runtime == RuntimeState::Running
        && let Some(live) =
            live.filter(|live| scm_process_id != 0 && live.server_pid == scm_process_id)
    {
        return Some(SelectedHealth {
            report: live.report,
            live: true,
        });
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
    let image_path = format!(r#""{}" --service"#, binary.display());
    if !mactype_service_contract::service_image_matches_protected_contract(
        service_root,
        &image_path,
    ) {
        return Err("the system service binary is outside the protected layout".to_owned());
    }
    Ok(binary)
}
