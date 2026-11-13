use super::{ActionFailure, RollbackOutcome, SystemServiceAction};

pub(super) trait StartupReceiptRestorer {
    fn restore_local_machine(&mut self) -> Result<(), ActionFailure>;
    fn restore_current_user(&mut self) -> Result<(), ActionFailure>;
}

pub(super) fn finish_action_with_startup_receipts(
    restorer: &mut impl StartupReceiptRestorer,
    action: SystemServiceAction,
    action_result: Result<(), ActionFailure>,
) -> Result<(), ActionFailure> {
    let must_restore = action_result
        .as_ref()
        .is_err_and(|failure| failure.diagnostics().is_none())
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
    action: Result<(), ActionFailure>,
    machine: Result<(), ActionFailure>,
    user: Result<(), ActionFailure>,
) -> Result<(), ActionFailure> {
    let mut failure = action.err();
    let restoration_failed = machine.is_err() || user.is_err();
    if let Err(error) = machine {
        let suffix = format!("; local-machine startup restoration failed: {error}");
        failure = Some(match failure {
            Some(failure) => failure.append_detail(suffix),
            None => ActionFailure::internal_at("startup-restoration", suffix),
        });
    }
    if let Err(error) = user {
        let suffix = format!("; current-user startup restoration failed: {error}");
        failure = Some(match failure {
            Some(failure) => failure.append_detail(suffix),
            None => ActionFailure::internal_at("startup-restoration", suffix),
        });
    }
    if restoration_failed {
        failure = failure.map(|failure| failure.with_rollback(RollbackOutcome::Failed));
    }
    failure.map_or(Ok(()), Err)
}
