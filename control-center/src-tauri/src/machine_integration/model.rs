use super::open_service;
use super::LegacyTrayStatus;
use crate::service_contract::SystemServiceStatus;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MachineAction {
    Install,
    Upgrade,
    Repair,
    Remove,
    Start,
    Stop,
    PublishProfile,
    MigrateFromLegacy,
    Rollback,
    RemoveLegacy,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PublicMachineAction {
    Install,
    Upgrade,
    Repair,
    Remove,
    Start,
    Stop,
    PublishProfile,
    MigrateFromLegacy,
    RemoveLegacy,
}

impl From<PublicMachineAction> for MachineAction {
    fn from(action: PublicMachineAction) -> Self {
        match action {
            PublicMachineAction::Install => Self::Install,
            PublicMachineAction::Upgrade => Self::Upgrade,
            PublicMachineAction::Repair => Self::Repair,
            PublicMachineAction::Remove => Self::Remove,
            PublicMachineAction::Start => Self::Start,
            PublicMachineAction::Stop => Self::Stop,
            PublicMachineAction::PublishProfile => Self::PublishProfile,
            PublicMachineAction::MigrateFromLegacy => Self::MigrateFromLegacy,
            PublicMachineAction::RemoveLegacy => Self::RemoveLegacy,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrayLoginState {
    Paused,
    Observing,
    UsingRunningNewService,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MachineStatus {
    pub new_service: SystemServiceStatus,
    pub legacy_service: Option<open_service::LegacyMacTrayStatus>,
    pub legacy_tray: LegacyTrayStatus,
    pub registry_conflict: bool,
    pub system_injection_active: bool,
    pub expected_profile_digest: Option<String>,
}

pub(crate) trait MachineBackend {
    fn new_service_status(&mut self) -> SystemServiceStatus;
    fn legacy_tray_status(&mut self) -> LegacyTrayStatus;
    fn appinit_conflict(&mut self) -> Result<bool, String>;
    /// Whether the legacy "MacType" SCM service can still inject or auto-start.
    /// A verified Stopped + Disabled service may remain installed for explicit
    /// removal without blocking the new service.
    fn legacy_service_blocks_activation(&mut self) -> Result<bool, String>;
    fn execute(&mut self, action: MachineAction, profile: Option<&[u8]>) -> Result<(), String>;
}
