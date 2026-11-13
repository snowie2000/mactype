mod capability_table;
mod migration;
mod profile_transfer;
mod request;
mod startup_lifecycle;
mod status;
mod windows_safety;

use super::{
    SystemServiceAction,
    action_failure::{
        ActionBlocker, ActionFailure, AppInitConflictContext, LegacyServiceBlockContext,
        LegacyTrayBlockContext, RollbackOutcome,
    },
    operation_failure_metadata,
};

#[test]
fn broker_stage_codes_recover_typed_blockers_without_prose_matching() {
    let blockers = [
        ActionBlocker::AdministratorApprovalCancelled,
        ActionBlocker::AppInitConflict(AppInitConflictContext::MachineIntegrationChange),
        ActionBlocker::AppInitConflict(AppInitConflictContext::MachineIntegrationChanges),
        ActionBlocker::AppInitConflict(AppInitConflictContext::ServiceChange),
        ActionBlocker::AppInitRegistryModeConflict,
        ActionBlocker::LegacyTrayModeBlocks(LegacyTrayBlockContext::MachineIntegrationChange),
        ActionBlocker::LegacyTrayModeBlocks(LegacyTrayBlockContext::ServiceChange),
        ActionBlocker::LegacyServiceStillInstalled(LegacyServiceBlockContext::ApplyProfile),
        ActionBlocker::LegacyServiceStillInstalled(LegacyServiceBlockContext::StartNewService),
        ActionBlocker::FixedServiceNameForeignOrInaccessible,
    ];
    for blocker in blockers {
        assert_eq!(
            ActionBlocker::from_stage_code(blocker.stage_code(), None),
            Some(blocker)
        );
    }
    for stage in [
        "blocker/installation-required",
        "blocker/installation-incomplete",
        "blocker/installation-untrusted",
    ] {
        assert!(matches!(
            ActionBlocker::from_stage_code(stage, None),
            Some(ActionBlocker::InstallationPreflight { .. })
        ));
    }
    assert!(ActionBlocker::from_stage_code("unknown", None).is_none());
}

#[test]
fn operation_diagnostics_use_typed_stage_and_rollback_outcome() {
    let failure = ActionFailure::internal_at(
        "typed-stage",
        "prose-stage: detail says rollback failed but no rollback ran",
    )
    .with_rollback(RollbackOutcome::NotApplicable);

    let (stage, rollback) = operation_failure_metadata(SystemServiceAction::Repair, &failure);

    assert_eq!(stage, "typed-stage");
    assert_eq!(rollback, RollbackOutcome::NotApplicable);
}
