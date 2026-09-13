use super::{LegacyScmSnapshot, ServiceConfiguration, ServiceExtendedConfiguration};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum ServiceRestoreStep {
    Core,
    Description,
    FailureActions,
    FailureActionsFlag,
    DelayedAutoStart,
    ServiceSidType,
    RequiredPrivileges,
    PreshutdownTimeout,
    Triggers,
    SecurityDescriptor,
}

impl ServiceRestoreStep {
    fn label(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Description => "description",
            Self::FailureActions => "failure-actions",
            Self::FailureActionsFlag => "failure-actions-flag",
            Self::DelayedAutoStart => "delayed-auto-start",
            Self::ServiceSidType => "service-sid-type",
            Self::RequiredPrivileges => "required-privileges",
            Self::PreshutdownTimeout => "preshutdown-timeout",
            Self::Triggers => "triggers",
            Self::SecurityDescriptor => "security-descriptor",
        }
    }
}

pub(super) const SERVICE_RESTORE_ORDER: [ServiceRestoreStep; 10] = [
    ServiceRestoreStep::Core,
    ServiceRestoreStep::Description,
    ServiceRestoreStep::FailureActions,
    ServiceRestoreStep::FailureActionsFlag,
    ServiceRestoreStep::DelayedAutoStart,
    ServiceRestoreStep::ServiceSidType,
    ServiceRestoreStep::RequiredPrivileges,
    ServiceRestoreStep::PreshutdownTimeout,
    ServiceRestoreStep::Triggers,
    ServiceRestoreStep::SecurityDescriptor,
];

pub(super) trait ServiceConfigurationRestorer {
    fn restore(&mut self, step: ServiceRestoreStep) -> Result<(), String>;
}

pub(super) fn perform_service_configuration_restore(
    restorer: &mut impl ServiceConfigurationRestorer,
) -> Result<(), String> {
    let mut failures = Vec::new();
    for step in SERVICE_RESTORE_ORDER {
        if let Err(error) = restorer.restore(step) {
            failures.push(format!("{}: {error}", step.label()));
            if step == ServiceRestoreStep::Core {
                break;
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "legacy SCM configuration restore failed [{}]",
            failures.join("; ")
        ))
    }
}

pub(super) fn verify_restored_configuration(
    expected: &LegacyScmSnapshot,
    actual_configuration: &ServiceConfiguration,
    actual_extended: &ServiceExtendedConfiguration,
) -> Result<(), String> {
    if actual_configuration != &expected.configuration {
        return Err("legacy SCM core configuration did not round-trip exactly".to_owned());
    }
    if actual_extended != &expected.extended {
        return Err(
            "legacy SCM Config2 or security configuration did not round-trip exactly".to_owned(),
        );
    }
    Ok(())
}
