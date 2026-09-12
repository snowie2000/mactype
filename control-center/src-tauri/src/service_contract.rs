use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ServiceBackend {
    OpenSource,
    LegacyMacTray,
    Foreign,
    None,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum InstallationState {
    Absent,
    Current,
    Outdated,
    Invalid,
    Inaccessible,
    DeletePending,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RuntimeState {
    Stopped,
    StartPending,
    Running,
    StopPending,
    Paused,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum HealthState {
    Unknown,
    Initializing,
    Ready,
    Degraded,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ServiceManagementPackageState {
    Ready,
    NotInstalled,
    Incomplete,
    Untrusted,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SystemServiceStatus {
    pub backend: ServiceBackend,
    pub installation: InstallationState,
    pub runtime: RuntimeState,
    pub health: HealthState,
    pub binary_path: Option<String>,
    pub win32_error: Option<u32>,
    pub active_profile_digest: Option<String>,
    pub configuration_drift: bool,
    pub can_install: bool,
    pub can_remove: bool,
    pub can_start: bool,
    pub can_stop: bool,
    pub can_repair: bool,
    pub can_upgrade: bool,
}

impl SystemServiceStatus {
    pub(crate) fn system_injection_active(&self, expected_digest: Option<&str>) -> bool {
        self.backend == ServiceBackend::OpenSource
            && self.installation == InstallationState::Current
            && self.runtime == RuntimeState::Running
            && self.health == HealthState::Ready
            && expected_digest.is_some()
            && self.active_profile_digest.as_deref() == expected_digest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_status() -> SystemServiceStatus {
        SystemServiceStatus {
            backend: ServiceBackend::OpenSource,
            installation: InstallationState::Current,
            runtime: RuntimeState::Running,
            health: HealthState::Ready,
            binary_path: Some(
                r"C:\Program Files\MacType Control Center\Service\mactype-service.exe".to_owned(),
            ),
            win32_error: None,
            active_profile_digest: Some("sha256:expected".to_owned()),
            configuration_drift: false,
            can_install: false,
            can_remove: true,
            can_start: false,
            can_stop: true,
            can_repair: false,
            can_upgrade: false,
        }
    }

    #[test]
    fn system_injection_requires_open_ready_runtime_and_matching_profile() {
        let ready = ready_status();
        assert!(ready.system_injection_active(Some("sha256:expected")));
        assert!(!ready.system_injection_active(Some("sha256:different")));

        for status in [
            SystemServiceStatus {
                backend: ServiceBackend::LegacyMacTray,
                ..ready.clone()
            },
            SystemServiceStatus {
                installation: InstallationState::Outdated,
                ..ready.clone()
            },
            SystemServiceStatus {
                runtime: RuntimeState::Stopped,
                ..ready.clone()
            },
            SystemServiceStatus {
                health: HealthState::Degraded,
                ..ready.clone()
            },
        ] {
            assert!(!status.system_injection_active(Some("sha256:expected")));
        }
    }
}
