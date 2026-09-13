use super::{LegacyTrayProcessState, LegacyTrayStartupState, LegacyTrayStatus};

pub(crate) trait LegacyTrayStartupCoordinator {
    fn observe_status(&mut self) -> LegacyTrayStatus;
    fn disable_current_user(&mut self) -> Result<(), String>;
    fn disable_local_machine(&mut self) -> Result<(), String>;
    fn restore_local_machine(&mut self) -> Result<(), String>;
    fn restore_current_user(&mut self) -> Result<(), String>;
}

pub(crate) fn disable_legacy_tray_startup_with(
    backend: &mut impl LegacyTrayStartupCoordinator,
) -> Result<(), String> {
    let before = backend.observe_status();
    if !matches!(before.process, LegacyTrayProcessState::Absent) {
        return Err("MacTray must be absent before its startup entries can be disabled".to_owned());
    }
    match before.startup {
        LegacyTrayStartupState::Absent => return Ok(()),
        LegacyTrayStartupState::Detected { .. } => {}
        LegacyTrayStartupState::Untrusted { .. } => {
            return Err("untrusted MacTray startup entries require manual review".to_owned());
        }
        LegacyTrayStartupState::Unknown { .. } => {
            return Err("MacTray startup state is unknown".to_owned());
        }
    }

    backend.disable_current_user()?;
    if let Err(error) = backend.disable_local_machine() {
        return Err(with_cleanup_error(
            error,
            "current-user startup restoration",
            backend.restore_current_user(),
        ));
    }

    let after = backend.observe_status();
    if matches!(after.process, LegacyTrayProcessState::Absent)
        && matches!(after.startup, LegacyTrayStartupState::Absent)
    {
        return Ok(());
    }

    let machine_restore = backend.restore_local_machine();
    let user_restore = backend.restore_current_user();
    let mut message = "MacTray conflict state was not clear after startup disable".to_owned();
    if let Err(error) = machine_restore {
        message.push_str("; local-machine startup restoration failed: ");
        message.push_str(&error);
    }
    if let Err(error) = user_restore {
        message.push_str("; current-user startup restoration failed: ");
        message.push_str(&error);
    }
    Err(message)
}

fn with_cleanup_error(primary: String, cleanup_name: &str, cleanup: Result<(), String>) -> String {
    match cleanup {
        Ok(()) => primary,
        Err(cleanup) => format!("{primary}; {cleanup_name} failed: {cleanup}"),
    }
}
