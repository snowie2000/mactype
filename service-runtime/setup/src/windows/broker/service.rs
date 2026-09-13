use super::super::runtime_recovery;
use super::BrokerContext;
use crate::storage::create_protected_directory;
use crate::{FixedPayload, ProfileStore, RuntimeInstaller, SetupError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RepairStep {
    Stop,
    Repair,
    StartReady,
}

struct RepairPlan(&'static [RepairStep]);

fn repair_plan(was_running: bool) -> RepairPlan {
    const STOPPED: &[RepairStep] = &[RepairStep::Repair];
    const RUNNING: &[RepairStep] = &[RepairStep::Stop, RepairStep::Repair, RepairStep::StartReady];
    RepairPlan(if was_running { RUNNING } else { STOPPED })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpgradeStep {
    Stop,
    Upgrade,
    StartReady,
}

struct UpgradePlan(&'static [UpgradeStep]);

fn upgrade_plan(was_running: bool) -> UpgradePlan {
    const STOPPED: &[UpgradeStep] = &[UpgradeStep::Upgrade];
    const RUNNING: &[UpgradeStep] = &[
        UpgradeStep::Stop,
        UpgradeStep::Upgrade,
        UpgradeStep::StartReady,
    ];
    UpgradePlan(if was_running { RUNNING } else { STOPPED })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpgradeRecoveryStep {
    RecoverExactBinding,
    StartPreviousReady,
}

struct UpgradeRecoveryPlan(&'static [UpgradeRecoveryStep]);

fn upgrade_recovery_plan(was_running: bool) -> UpgradeRecoveryPlan {
    const STOPPED: &[UpgradeRecoveryStep] = &[UpgradeRecoveryStep::RecoverExactBinding];
    const RUNNING: &[UpgradeRecoveryStep] = &[
        UpgradeRecoveryStep::RecoverExactBinding,
        UpgradeRecoveryStep::StartPreviousReady,
    ];
    UpgradeRecoveryPlan(if was_running { RUNNING } else { STOPPED })
}

pub(super) fn install(context: &BrokerContext) -> Result<String, SetupError> {
    create_protected_directory(context.paths.service_root())?;
    super::super::acl::harden_machine_directory(context.paths.service_root())?;
    let payload = FixedPayload::beside_setup_executable()?;
    let installed = RuntimeInstaller::new(context.paths.clone())
        .deploy_with_prepare_and_health_check(
            &payload,
            |binary| context.manager.install(binary),
            |_, _| Ok(()),
        );
    let installed = match installed {
        Ok((installed, ())) => installed,
        Err(operation) => {
            if let Err(restoration) = runtime_recovery::recover(&context.paths, &context.manager) {
                return Err(combine_operation_and_restore_error(operation, restoration));
            }
            crate::event_log::rollback_completed("service");
            return Err(operation);
        }
    };
    super::super::acl::harden_machine_directory(context.paths.service_root())?;
    Ok(version_result("install", installed.version()))
}

pub(super) fn upgrade(context: &BrokerContext) -> Result<String, SetupError> {
    let was_running = context.manager.is_running()?;
    let plan = upgrade_plan(was_running);
    let recovery_plan = upgrade_recovery_plan(was_running);
    if plan.0.contains(&UpgradeStep::Stop) {
        context.manager.stop()?;
    }
    let payload = FixedPayload::beside_setup_executable()?;
    let installer = RuntimeInstaller::new(context.paths.clone());
    let previous = installer.inspect_current_stable()?.ok_or_else(|| {
        SetupError::Runtime("no active protected runtime exists before upgrade".to_owned())
    })?;
    let installed = installer
        .deploy_with_prepare_and_health_check(
            &payload,
            |binary| context.manager.reconfigure(binary),
            |_, _| {
                if plan.0.contains(&UpgradeStep::StartReady) {
                    context.manager.start_and_wait_ready()?;
                }
                Ok(())
            },
        )
        .map(|(installed, ())| installed);
    let installed = match installed {
        Ok(installed) => installed,
        Err(operation) => {
            if let Err(restoration) = restore_upgrade_state(context, &previous, &recovery_plan) {
                return Err(combine_operation_and_restore_error(operation, restoration));
            }
            crate::event_log::rollback_completed("upgrade");
            return Err(operation);
        }
    };
    super::super::acl::harden_machine_directory(context.paths.service_root())?;
    Ok(version_result("upgrade", installed.version()))
}

pub(super) fn repair(context: &BrokerContext) -> Result<String, SetupError> {
    let plan = repair_plan(context.manager.is_running().map_err(|error| {
        error.at_machine_path(
            "inspect service state before repair",
            context.paths.service_root(),
        )
    })?);
    if plan.0.contains(&RepairStep::Stop) {
        context.manager.stop().map_err(|error| {
            error.at_machine_path("stop service before repair", context.paths.service_root())
        })?;
    }
    let repair = (|| {
        let payload = FixedPayload::beside_setup_executable().map_err(|error| {
            error.at_machine_path("locate fixed repair payload", context.paths.service_root())
        })?;
        RuntimeInstaller::new(context.paths.clone())
            .repair_current_with_prepare_and_health_check(
                &payload,
                |binary| context.manager.reconfigure(binary),
                |_, _| Ok(()),
            )
            .map_err(|error| {
                error.at_machine_path(
                    "repair protected runtime transaction",
                    context.paths.service_root(),
                )
            })?;
        ProfileStore::new(context.paths.clone())
            .synchronize_active_runtime()
            .map_err(|error| {
                error.at_machine_path(
                    "synchronize active profile after repair",
                    context.paths.active_profile(),
                )
            })?;
        super::super::acl::harden_machine_directory(context.paths.service_root())?;
        if plan.0.contains(&RepairStep::StartReady) {
            context.manager.start_and_wait_ready().map_err(|error| {
                error.at_machine_path("restart service after repair", context.paths.service_root())
            })?;
        }
        Ok::<(), SetupError>(())
    })();
    if let Err(operation) = repair {
        if let Err(restoration) = runtime_recovery::recover(&context.paths, &context.manager) {
            return Err(combine_operation_and_restore_error(operation, restoration));
        }
        if plan.0.contains(&RepairStep::StartReady) {
            if let Err(restoration) = context.manager.start_and_wait_ready() {
                return Err(combine_operation_and_restore_error(operation, restoration));
            }
        }
        crate::event_log::rollback_completed("service");
        return Err(operation);
    }
    Ok("{\"ok\":true,\"verb\":\"repair\"}".to_owned())
}

pub(super) fn remove(context: &BrokerContext) -> Result<String, SetupError> {
    context.manager.remove()?;
    Ok("{\"ok\":true,\"verb\":\"remove\"}".to_owned())
}

pub(super) fn start(context: &BrokerContext) -> Result<String, SetupError> {
    let current = RuntimeInstaller::new(context.paths.clone())
        .current()?
        .ok_or_else(|| {
            SetupError::Runtime("no active protected runtime is installed".to_owned())
        })?;
    ProfileStore::new(context.paths.clone()).synchronize_active_runtime()?;
    super::super::acl::harden_machine_directory(context.paths.service_root())?;
    context.manager.reconfigure(current.service_binary())?;
    context.manager.start_and_wait_ready()?;
    Ok("{\"ok\":true,\"verb\":\"start\",\"health\":\"ready\"}".to_owned())
}

pub(super) fn stop(context: &BrokerContext) -> Result<String, SetupError> {
    context.manager.stop()?;
    Ok("{\"ok\":true,\"verb\":\"stop\"}".to_owned())
}

pub(super) fn restore_runtime(context: &BrokerContext) -> Result<String, SetupError> {
    if context.manager.is_running()? {
        context.manager.stop()?;
    }
    let restored = RuntimeInstaller::new(context.paths.clone())
        .restore_pinned_current_with_health_check(|binary| context.manager.reconfigure(binary))?;
    super::super::acl::harden_machine_directory(context.paths.service_root())?;
    Ok(version_result("restore-runtime", restored.version()))
}

fn restore_upgrade_state(
    context: &BrokerContext,
    previous: &crate::InstalledRuntime,
    plan: &UpgradeRecoveryPlan,
) -> Result<(), SetupError> {
    debug_assert!(plan.0.contains(&UpgradeRecoveryStep::RecoverExactBinding));
    let recovered = runtime_recovery::recover(&context.paths, &context.manager)?;
    if recovered.as_ref() != Some(previous) {
        return Err(SetupError::CleanupUnknown(
            "failed upgrade did not recover the exact previous runtime; refusing to infer a service restart"
                .to_owned(),
        ));
    }
    if plan.0.contains(&UpgradeRecoveryStep::StartPreviousReady) {
        context.manager.start_and_wait_ready()?;
    }
    Ok(())
}

fn combine_operation_and_restore_error(
    operation: SetupError,
    restoration: SetupError,
) -> SetupError {
    SetupError::Runtime(format!(
        "{operation}; additionally failed to restore the caller's service state: {restoration}"
    ))
}

fn version_result(verb: &str, version: &str) -> String {
    format!("{{\"ok\":true,\"verb\":\"{verb}\",\"version\":\"{version}\"}}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_preserves_the_callers_runtime_state() {
        assert_eq!(repair_plan(false).0, &[RepairStep::Repair]);
        assert_eq!(
            repair_plan(true).0,
            &[RepairStep::Stop, RepairStep::Repair, RepairStep::StartReady]
        );
    }

    #[test]
    fn upgrade_preserves_the_callers_runtime_state() {
        assert_eq!(upgrade_plan(false).0, &[UpgradeStep::Upgrade]);
        assert_eq!(
            upgrade_plan(true).0,
            &[
                UpgradeStep::Stop,
                UpgradeStep::Upgrade,
                UpgradeStep::StartReady,
            ]
        );
    }

    #[test]
    fn running_upgrade_failure_recovers_the_exact_binding_before_restart() {
        assert_eq!(
            upgrade_recovery_plan(true).0,
            &[
                UpgradeRecoveryStep::RecoverExactBinding,
                UpgradeRecoveryStep::StartPreviousReady,
            ]
        );
        assert_eq!(
            upgrade_recovery_plan(false).0,
            &[UpgradeRecoveryStep::RecoverExactBinding]
        );
    }

    #[test]
    fn upgrade_and_restoration_failures_are_both_reported() {
        let error = combine_operation_and_restore_error(
            SetupError::Runtime("upgrade failed".to_owned()),
            SetupError::Runtime("restart failed".to_owned()),
        );
        let message = error.to_string();
        assert!(message.contains("upgrade failed"));
        assert!(message.contains("restart failed"));
        assert!(message.contains("restore the caller's service state"));
    }

    #[test]
    fn repair_and_restoration_failures_are_both_reported() {
        let error = combine_operation_and_restore_error(
            SetupError::Runtime("repair failed".to_owned()),
            SetupError::Runtime("state restart failed".to_owned()),
        );
        let message = error.to_string();
        assert!(message.contains("repair failed"));
        assert!(message.contains("state restart failed"));
    }
}
