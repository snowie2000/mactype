use mactype_service_platform::ServiceHandle;

use super::super::DESCRIPTION;
use crate::SetupError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ServiceMetadataOperation {
    Description(&'static str),
    RestartOnFailure {
        reset_period_seconds: u32,
        delays_milliseconds: [u32; 2],
    },
    IncludeNonCrashFailures(bool),
}

const fn service_metadata_plan() -> [ServiceMetadataOperation; 3] {
    [
        ServiceMetadataOperation::Description(DESCRIPTION),
        ServiceMetadataOperation::RestartOnFailure {
            reset_period_seconds: 86_400,
            delays_milliseconds: [5_000, 30_000],
        },
        ServiceMetadataOperation::IncludeNonCrashFailures(true),
    ]
}

trait MetadataConfigurationAdapter {
    fn apply(&mut self, operation: ServiceMetadataOperation) -> Result<(), SetupError>;
}

fn apply_metadata_configuration(
    adapter: &mut impl MetadataConfigurationAdapter,
) -> Result<(), SetupError> {
    for operation in service_metadata_plan() {
        adapter.apply(operation)?;
    }
    Ok(())
}

struct WindowsMetadataConfigurationAdapter<'a> {
    service: &'a ServiceHandle,
}

impl MetadataConfigurationAdapter for WindowsMetadataConfigurationAdapter<'_> {
    fn apply(&mut self, operation: ServiceMetadataOperation) -> Result<(), SetupError> {
        match operation {
            ServiceMetadataOperation::Description(text) => self.service.set_description(text),
            ServiceMetadataOperation::RestartOnFailure {
                reset_period_seconds,
                delays_milliseconds,
            } => self
                .service
                .set_restart_actions(reset_period_seconds, &delays_milliseconds),
            ServiceMetadataOperation::IncludeNonCrashFailures(enabled) => self
                .service
                .set_failure_actions_on_non_crash_failures(enabled),
        }
        .map_err(SetupError::Io)
    }
}

pub(in crate::windows::scm) fn configure_metadata(
    service: &ServiceHandle,
) -> Result<(), SetupError> {
    apply_metadata_configuration(&mut WindowsMetadataConfigurationAdapter { service })
}

#[cfg(test)]
mod tests {
    use super::{
        apply_metadata_configuration, service_metadata_plan, MetadataConfigurationAdapter,
        ServiceMetadataOperation,
    };
    use crate::SetupError;

    #[derive(Default)]
    struct RecordingMetadataAdapter {
        attempted: Vec<ServiceMetadataOperation>,
        fail_at: Option<usize>,
    }

    impl MetadataConfigurationAdapter for RecordingMetadataAdapter {
        fn apply(&mut self, operation: ServiceMetadataOperation) -> Result<(), SetupError> {
            let index = self.attempted.len();
            self.attempted.push(operation);
            if self.fail_at == Some(index) {
                return Err(SetupError::Runtime(format!(
                    "metadata operation {index} failed"
                )));
            }
            Ok(())
        }
    }

    #[test]
    fn service_recovery_contract_restarts_after_five_and_thirty_seconds_for_non_crash_failures() {
        assert_eq!(
            service_metadata_plan(),
            [
                ServiceMetadataOperation::Description(
                    "Runs the open MacType machine integration runtime."
                ),
                ServiceMetadataOperation::RestartOnFailure {
                    reset_period_seconds: 86_400,
                    delays_milliseconds: [5_000, 30_000],
                },
                ServiceMetadataOperation::IncludeNonCrashFailures(true),
            ]
        );
    }

    #[test]
    fn metadata_configuration_applies_the_complete_recovery_contract_in_order() {
        let mut adapter = RecordingMetadataAdapter::default();

        apply_metadata_configuration(&mut adapter).unwrap();

        assert_eq!(adapter.attempted, service_metadata_plan());
    }

    #[test]
    fn every_metadata_configuration_failure_is_returned_immediately() {
        for fail_at in 0..service_metadata_plan().len() {
            let mut adapter = RecordingMetadataAdapter {
                attempted: Vec::new(),
                fail_at: Some(fail_at),
            };

            let error = apply_metadata_configuration(&mut adapter).unwrap_err();

            assert!(error
                .to_string()
                .contains(&format!("metadata operation {fail_at} failed")));
            assert_eq!(adapter.attempted, service_metadata_plan()[..=fail_at]);
        }
    }
}
