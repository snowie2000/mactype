use super::{legacy_mactray, legacy_migration, open_service};

struct SystemStartupCoordinator;

impl legacy_mactray::LegacyTrayStartupCoordinator for SystemStartupCoordinator {
    fn observe_status(&mut self) -> legacy_mactray::LegacyTrayStatus {
        legacy_mactray::tray_status()
    }

    fn disable_current_user(&mut self) -> Result<(), String> {
        legacy_migration::disable_startup_scope(legacy_migration::StartupReceiptScope::CurrentUser)
    }

    fn disable_local_machine(&mut self) -> Result<(), String> {
        open_service::run_action(
            open_service::SystemServiceAction::DisableLegacyTrayAutostart,
            None,
        )
    }

    fn restore_local_machine(&mut self) -> Result<(), String> {
        open_service::run_action(
            open_service::SystemServiceAction::RestoreLegacyTrayAutostart,
            None,
        )
    }

    fn restore_current_user(&mut self) -> Result<(), String> {
        legacy_migration::restore_startup_scope(legacy_migration::StartupReceiptScope::CurrentUser)
    }
}

pub(super) fn disable() -> Result<(), String> {
    legacy_mactray::disable_legacy_tray_startup_with(&mut SystemStartupCoordinator)
}
