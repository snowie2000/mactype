use super::{installation_preflight_failure, SystemServiceAction};

pub(super) trait StartupReceiptRestorer {
    fn restore_local_machine(&mut self) -> Result<(), String>;
    fn restore_current_user(&mut self) -> Result<(), String>;
}

pub(super) fn finish_action_with_startup_receipts(
    restorer: &mut impl StartupReceiptRestorer,
    action: SystemServiceAction,
    action_result: Result<(), String>,
) -> Result<(), String> {
    let must_restore = action_result
        .as_ref()
        .is_err_and(|error| !installation_preflight_failure(error))
        && matches!(
            action,
            SystemServiceAction::Install | SystemServiceAction::MigrateFromLegacy
        );
    if !must_restore {
        return action_result;
    }

    let machine = restorer.restore_local_machine();
    let user = restorer.restore_current_user();
    combine_action_and_restoration(action_result, machine, user)
}

fn combine_action_and_restoration(
    action: Result<(), String>,
    machine: Result<(), String>,
    user: Result<(), String>,
) -> Result<(), String> {
    let mut errors = Vec::new();
    if let Err(error) = action {
        errors.push(error);
    }
    if let Err(error) = machine {
        errors.push(format!("local-machine startup restoration failed: {error}"));
    }
    if let Err(error) = user {
        errors.push(format!("current-user startup restoration failed: {error}"));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}
