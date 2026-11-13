use crate::diagnostics::InstallationPreflightDiagnostics;

pub(in crate::machine_integration) const INTERNAL_OPERATION_FAILURE_PREFIX: &str =
    "control-center-internal-operation-failed:";
pub(in crate::machine_integration) const INSTALLATION_REQUIRED_PREFIX: &str =
    "control-center-installation-required:";
pub(in crate::machine_integration) const INSTALLATION_INCOMPLETE_PREFIX: &str =
    "control-center-installation-incomplete:";
pub(in crate::machine_integration) const INSTALLATION_UNTRUSTED_PREFIX: &str =
    "control-center-installation-untrusted:";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::machine_integration) enum AppInitConflictContext {
    MachineIntegrationChange,
    MachineIntegrationChanges,
    ServiceChange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::machine_integration) enum LegacyTrayBlockContext {
    MachineIntegrationChange,
    ServiceChange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::machine_integration) enum LegacyServiceBlockContext {
    ApplyProfile,
    StartNewService,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::machine_integration) enum InstallationPreflightKind {
    Required,
    Incomplete,
    Untrusted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::machine_integration) enum ActionBlocker {
    AdministratorApprovalCancelled,
    AppInitConflict(AppInitConflictContext),
    AppInitRegistryModeConflict,
    LegacyTrayModeBlocks(LegacyTrayBlockContext),
    LegacyServiceStillInstalled(LegacyServiceBlockContext),
    FixedServiceNameForeignOrInaccessible,
    InstallationPreflight {
        kind: InstallationPreflightKind,
        diagnostics: Box<InstallationPreflightDiagnostics>,
    },
}

impl ActionBlocker {
    pub(in crate::machine_integration) fn message(&self) -> &'static str {
        match self {
            Self::AdministratorApprovalCancelled => "administrator approval was cancelled",
            Self::AppInitConflict(AppInitConflictContext::MachineIntegrationChange) => {
                "AppInit conflicts block this machine integration change"
            }
            Self::AppInitConflict(AppInitConflictContext::MachineIntegrationChanges) => {
                "AppInit conflicts block machine integration changes"
            }
            Self::AppInitConflict(AppInitConflictContext::ServiceChange) => {
                "AppInit conflicts block this service change"
            }
            Self::AppInitRegistryModeConflict => {
                "AppInit registry mode conflicts with legacy service migration"
            }
            Self::LegacyTrayModeBlocks(LegacyTrayBlockContext::MachineIntegrationChange) => {
                "the legacy MacTray tray mode blocks this machine integration change"
            }
            Self::LegacyTrayModeBlocks(LegacyTrayBlockContext::ServiceChange) => {
                "the legacy MacTray tray mode blocks this service change"
            }
            Self::LegacyServiceStillInstalled(LegacyServiceBlockContext::ApplyProfile) => {
                "a legacy MacType service is still installed; migrate it before applying the profile"
            }
            Self::LegacyServiceStillInstalled(LegacyServiceBlockContext::StartNewService) => {
                "a legacy MacType service is still installed; migrate it before starting the new service"
            }
            Self::FixedServiceNameForeignOrInaccessible => {
                "the fixed service name became foreign or inaccessible; SCM rollback was refused"
            }
            Self::InstallationPreflight {
                kind: InstallationPreflightKind::Required,
                ..
            } => INSTALLATION_REQUIRED_PREFIX,
            Self::InstallationPreflight {
                kind: InstallationPreflightKind::Incomplete,
                ..
            } => INSTALLATION_INCOMPLETE_PREFIX,
            Self::InstallationPreflight {
                kind: InstallationPreflightKind::Untrusted,
                ..
            } => INSTALLATION_UNTRUSTED_PREFIX,
        }
    }

    pub(in crate::machine_integration) fn stage_code(&self) -> &'static str {
        match self {
            Self::AdministratorApprovalCancelled => "blocker/administrator-approval-cancelled",
            Self::AppInitConflict(AppInitConflictContext::MachineIntegrationChange) => {
                "blocker/appinit-machine-integration-change"
            }
            Self::AppInitConflict(AppInitConflictContext::MachineIntegrationChanges) => {
                "blocker/appinit-machine-integration-changes"
            }
            Self::AppInitConflict(AppInitConflictContext::ServiceChange) => {
                "blocker/appinit-service-change"
            }
            Self::AppInitRegistryModeConflict => "blocker/appinit-registry-mode-conflict",
            Self::LegacyTrayModeBlocks(LegacyTrayBlockContext::MachineIntegrationChange) => {
                "blocker/legacy-tray-machine-integration-change"
            }
            Self::LegacyTrayModeBlocks(LegacyTrayBlockContext::ServiceChange) => {
                "blocker/legacy-tray-service-change"
            }
            Self::LegacyServiceStillInstalled(LegacyServiceBlockContext::ApplyProfile) => {
                "blocker/legacy-service-apply-profile"
            }
            Self::LegacyServiceStillInstalled(LegacyServiceBlockContext::StartNewService) => {
                "blocker/legacy-service-start-new-service"
            }
            Self::FixedServiceNameForeignOrInaccessible => {
                "blocker/fixed-service-name-foreign-or-inaccessible"
            }
            Self::InstallationPreflight {
                kind: InstallationPreflightKind::Required,
                ..
            } => "blocker/installation-required",
            Self::InstallationPreflight {
                kind: InstallationPreflightKind::Incomplete,
                ..
            } => "blocker/installation-incomplete",
            Self::InstallationPreflight {
                kind: InstallationPreflightKind::Untrusted,
                ..
            } => "blocker/installation-untrusted",
        }
    }

    pub(in crate::machine_integration) fn from_stage_code(
        stage: &str,
        diagnostics: Option<InstallationPreflightDiagnostics>,
    ) -> Option<Self> {
        Some(match stage {
            "blocker/administrator-approval-cancelled" => Self::AdministratorApprovalCancelled,
            "blocker/appinit-machine-integration-change" => {
                Self::AppInitConflict(AppInitConflictContext::MachineIntegrationChange)
            }
            "blocker/appinit-machine-integration-changes" => {
                Self::AppInitConflict(AppInitConflictContext::MachineIntegrationChanges)
            }
            "blocker/appinit-service-change" => {
                Self::AppInitConflict(AppInitConflictContext::ServiceChange)
            }
            "blocker/appinit-registry-mode-conflict" => Self::AppInitRegistryModeConflict,
            "blocker/legacy-tray-machine-integration-change" => {
                Self::LegacyTrayModeBlocks(LegacyTrayBlockContext::MachineIntegrationChange)
            }
            "blocker/legacy-tray-service-change" => {
                Self::LegacyTrayModeBlocks(LegacyTrayBlockContext::ServiceChange)
            }
            "blocker/legacy-service-apply-profile" => {
                Self::LegacyServiceStillInstalled(LegacyServiceBlockContext::ApplyProfile)
            }
            "blocker/legacy-service-start-new-service" => {
                Self::LegacyServiceStillInstalled(LegacyServiceBlockContext::StartNewService)
            }
            "blocker/fixed-service-name-foreign-or-inaccessible" => {
                Self::FixedServiceNameForeignOrInaccessible
            }
            "blocker/installation-required" => Self::InstallationPreflight {
                kind: InstallationPreflightKind::Required,
                diagnostics: Box::new(diagnostics.unwrap_or_else(unavailable_preflight)),
            },
            "blocker/installation-incomplete" => Self::InstallationPreflight {
                kind: InstallationPreflightKind::Incomplete,
                diagnostics: Box::new(diagnostics.unwrap_or_else(unavailable_preflight)),
            },
            "blocker/installation-untrusted" => Self::InstallationPreflight {
                kind: InstallationPreflightKind::Untrusted,
                diagnostics: Box::new(diagnostics.unwrap_or_else(unavailable_preflight)),
            },
            _ => return None,
        })
    }
}

fn unavailable_preflight() -> InstallationPreflightDiagnostics {
    InstallationPreflightDiagnostics {
        expected_installed_control_center: None,
        current_executable: None,
        expected_executable_exists: None,
        installed_control_center: "unavailable".to_owned(),
        current_bundle: "unavailable".to_owned(),
        selected_service_package: "none".to_owned(),
        setup_broker: "unavailable".to_owned(),
        runtime_manifest: "unavailable".to_owned(),
        runtime_payload: "unavailable".to_owned(),
        elevation_attempted: true,
        elevated_revalidation: "unavailable".to_owned(),
        machine_state_changed: false,
        rollback_required: false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::machine_integration) enum RollbackOutcome {
    Failed,
    Completed,
    FailClosedLegacyStopped,
    NotApplicable,
    NotApplicableOrUnavailable,
}

impl RollbackOutcome {
    pub(in crate::machine_integration) fn as_str(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Completed => "completed",
            Self::FailClosedLegacyStopped => "fail-closed-legacy-stopped",
            Self::NotApplicable => "not-applicable",
            Self::NotApplicableOrUnavailable => "not-applicable-or-unavailable",
        }
    }

    pub(in crate::machine_integration) fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "failed" => Self::Failed,
            "completed" => Self::Completed,
            "fail-closed-legacy-stopped" => Self::FailClosedLegacyStopped,
            "not-applicable" => Self::NotApplicable,
            "not-applicable-or-unavailable" => Self::NotApplicableOrUnavailable,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::machine_integration) enum ActionFailureKind {
    Blocked(ActionBlocker),
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::machine_integration) struct ActionFailure {
    pub(in crate::machine_integration) kind: ActionFailureKind,
    pub(in crate::machine_integration) stage: Option<String>,
    pub(in crate::machine_integration) detail: String,
    pub(in crate::machine_integration) rollback: RollbackOutcome,
    pub(in crate::machine_integration) channel_failure: Option<String>,
}

impl ActionFailure {
    pub(in crate::machine_integration) fn blocked(blocker: ActionBlocker) -> Self {
        let detail = blocker.message().to_owned();
        let rollback = if matches!(blocker, ActionBlocker::InstallationPreflight { .. }) {
            RollbackOutcome::NotApplicable
        } else {
            RollbackOutcome::NotApplicableOrUnavailable
        };
        Self {
            kind: ActionFailureKind::Blocked(blocker),
            stage: None,
            detail,
            rollback,
            channel_failure: None,
        }
    }

    pub(in crate::machine_integration) fn blocked_with_detail(
        blocker: ActionBlocker,
        detail: String,
    ) -> Self {
        let mut failure = Self::blocked(blocker);
        failure.detail = detail;
        failure
    }

    pub(in crate::machine_integration) fn internal(detail: impl Into<String>) -> Self {
        Self {
            kind: ActionFailureKind::Internal,
            stage: None,
            detail: detail.into(),
            rollback: RollbackOutcome::NotApplicableOrUnavailable,
            channel_failure: None,
        }
    }

    pub(in crate::machine_integration) fn internal_at(
        stage: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        let mut failure = Self::internal(detail);
        failure.stage = Some(stage.into());
        failure
    }

    pub(in crate::machine_integration) fn installation_preflight(
        kind: InstallationPreflightKind,
        diagnostics: Box<InstallationPreflightDiagnostics>,
        detail: String,
    ) -> Self {
        Self::blocked_with_detail(
            ActionBlocker::InstallationPreflight { kind, diagnostics },
            detail,
        )
    }

    pub(in crate::machine_integration) fn with_rollback(
        mut self,
        rollback: RollbackOutcome,
    ) -> Self {
        self.rollback = rollback;
        self
    }

    pub(in crate::machine_integration) fn with_channel_failure(
        mut self,
        channel: impl Into<String>,
    ) -> Self {
        self.channel_failure = Some(channel.into());
        self
    }

    pub(in crate::machine_integration) fn append_detail(mut self, suffix: impl AsRef<str>) -> Self {
        self.detail.push_str(suffix.as_ref());
        self
    }

    pub(in crate::machine_integration) fn diagnostics(
        &self,
    ) -> Option<&InstallationPreflightDiagnostics> {
        match &self.kind {
            ActionFailureKind::Blocked(ActionBlocker::InstallationPreflight {
                diagnostics,
                ..
            }) => Some(diagnostics),
            _ => None,
        }
    }

    pub(in crate::machine_integration) fn user_message(&self) -> String {
        self.detail.clone()
    }
}

#[cfg(test)]
impl ActionFailure {
    pub(in crate::machine_integration) fn contains(&self, value: &str) -> bool {
        self.detail.contains(value)
    }

    pub(in crate::machine_integration) fn starts_with(&self, value: &str) -> bool {
        self.detail.starts_with(value)
    }
}

impl std::fmt::Display for ActionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl From<String> for ActionFailure {
    fn from(detail: String) -> Self {
        Self::internal(detail)
    }
}

impl From<&str> for ActionFailure {
    fn from(detail: &str) -> Self {
        Self::internal(detail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostics() -> Box<InstallationPreflightDiagnostics> {
        Box::new(InstallationPreflightDiagnostics {
            expected_installed_control_center: None,
            current_executable: None,
            expected_executable_exists: None,
            installed_control_center: "not-checked".to_owned(),
            current_bundle: "not-checked".to_owned(),
            selected_service_package: "none".to_owned(),
            setup_broker: "not-checked".to_owned(),
            runtime_manifest: "not-checked".to_owned(),
            runtime_payload: "not-checked".to_owned(),
            elevation_attempted: false,
            elevated_revalidation: "not-attempted".to_owned(),
            machine_state_changed: false,
            rollback_required: false,
        })
    }

    #[test]
    fn every_blocker_renders_its_stable_user_message_or_prefix() {
        let blockers = [
            (
                ActionBlocker::AdministratorApprovalCancelled,
                "administrator approval was cancelled",
            ),
            (
                ActionBlocker::AppInitConflict(AppInitConflictContext::MachineIntegrationChange),
                "AppInit conflicts block this machine integration change",
            ),
            (
                ActionBlocker::AppInitConflict(AppInitConflictContext::MachineIntegrationChanges),
                "AppInit conflicts block machine integration changes",
            ),
            (
                ActionBlocker::AppInitConflict(AppInitConflictContext::ServiceChange),
                "AppInit conflicts block this service change",
            ),
            (
                ActionBlocker::AppInitRegistryModeConflict,
                "AppInit registry mode conflicts with legacy service migration",
            ),
            (
                ActionBlocker::LegacyTrayModeBlocks(
                    LegacyTrayBlockContext::MachineIntegrationChange,
                ),
                "the legacy MacTray tray mode blocks this machine integration change",
            ),
            (
                ActionBlocker::LegacyTrayModeBlocks(LegacyTrayBlockContext::ServiceChange),
                "the legacy MacTray tray mode blocks this service change",
            ),
            (
                ActionBlocker::LegacyServiceStillInstalled(LegacyServiceBlockContext::ApplyProfile),
                "a legacy MacType service is still installed; migrate it before applying the profile",
            ),
            (
                ActionBlocker::LegacyServiceStillInstalled(
                    LegacyServiceBlockContext::StartNewService,
                ),
                "a legacy MacType service is still installed; migrate it before starting the new service",
            ),
            (
                ActionBlocker::FixedServiceNameForeignOrInaccessible,
                "the fixed service name became foreign or inaccessible; SCM rollback was refused",
            ),
            (
                ActionBlocker::InstallationPreflight {
                    kind: InstallationPreflightKind::Required,
                    diagnostics: diagnostics(),
                },
                INSTALLATION_REQUIRED_PREFIX,
            ),
            (
                ActionBlocker::InstallationPreflight {
                    kind: InstallationPreflightKind::Incomplete,
                    diagnostics: diagnostics(),
                },
                INSTALLATION_INCOMPLETE_PREFIX,
            ),
            (
                ActionBlocker::InstallationPreflight {
                    kind: InstallationPreflightKind::Untrusted,
                    diagnostics: diagnostics(),
                },
                INSTALLATION_UNTRUSTED_PREFIX,
            ),
        ];
        for (blocker, expected) in blockers {
            let failure = ActionFailure::blocked(blocker);
            assert_eq!(failure.user_message(), expected);
        }
    }
}
