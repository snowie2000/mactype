use super::{
    appinit::appinit_conflict, legacy_mactray, open_service, LegacyTrayStatus, MachineAction,
    MachineBackend,
};
use crate::service_contract::SystemServiceStatus;

pub(super) struct SystemMachineBackend;

impl MachineBackend for SystemMachineBackend {
    fn new_service_status(&mut self) -> SystemServiceStatus {
        open_service::status()
    }

    fn legacy_tray_status(&mut self) -> LegacyTrayStatus {
        legacy_mactray::tray_status()
    }

    fn appinit_conflict(&mut self) -> Result<bool, String> {
        appinit_conflict()
    }

    fn legacy_service_blocks_activation(&mut self) -> Result<bool, String> {
        legacy_mactray::legacy_service_blocks_activation()
    }

    fn execute(&mut self, action: MachineAction, profile: Option<&[u8]>) -> Result<(), String> {
        open_service::run_action(action.into(), profile)
    }
}

impl From<MachineAction> for open_service::SystemServiceAction {
    fn from(action: MachineAction) -> Self {
        match action {
            MachineAction::Install => Self::Install,
            MachineAction::Upgrade => Self::Upgrade,
            MachineAction::Repair => Self::Repair,
            MachineAction::Remove => Self::Remove,
            MachineAction::Start => Self::Start,
            MachineAction::Stop => Self::Stop,
            MachineAction::PublishProfile => Self::PublishProfile,
            MachineAction::MigrateFromLegacy => Self::MigrateFromLegacy,
            MachineAction::Rollback => Self::Rollback,
            MachineAction::RemoveLegacy => Self::RemoveLegacy,
        }
    }
}
