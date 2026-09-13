use mactype_service_contract::StructuredServiceError;

use crate::{ProcessIdentity, ProcessInspector, TargetLifecycle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessTargetDecision {
    Eligible(ProcessIdentity),
    Skipped,
    Deferred {
        identity: ProcessIdentity,
        reason: DeferralReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeferralReason {
    Frozen,
    HelperLaunchFailed,
}

pub struct ProcessTargetValidator<'a> {
    service_pid: u32,
    inspector: &'a dyn ProcessInspector,
}

impl<'a> ProcessTargetValidator<'a> {
    pub const fn new(service_pid: u32, inspector: &'a dyn ProcessInspector) -> Self {
        Self {
            service_pid,
            inspector,
        }
    }

    pub fn validate(&self, pid: u32) -> Result<ProcessTargetDecision, StructuredServiceError> {
        let identity = match self.inspector.inspect(pid) {
            Ok(identity) => identity,
            Err(error) if target_scoped_inspection_failure(&error.code) => {
                return Ok(ProcessTargetDecision::Skipped);
            }
            Err(error) => return Err(error),
        };
        if identity.pid != pid {
            return Err(service_error(
                "process-identity-mismatch",
                "the inspected process identity does not match the observed PID",
            ));
        }
        if identity.pid == self.service_pid
            || identity.session_id == 0
            || identity.protected
            || identity.critical
        {
            return Ok(ProcessTargetDecision::Skipped);
        }
        match self.inspector.probe_target_lifecycle(&identity) {
            TargetLifecycle::Exiting => Ok(ProcessTargetDecision::Skipped),
            TargetLifecycle::Frozen => Ok(ProcessTargetDecision::Deferred {
                identity,
                reason: DeferralReason::Frozen,
            }),
            TargetLifecycle::Running | TargetLifecycle::Unknown => {
                Ok(ProcessTargetDecision::Eligible(identity))
            }
        }
    }
}

fn target_scoped_inspection_failure(code: &str) -> bool {
    matches!(
        code,
        "process-protected-or-inaccessible"
            | "process-creation-time-unavailable"
            | "process-session-unavailable"
            | "process-architecture-unavailable"
            | "process-architecture-unsupported"
    )
}

fn service_error(code: &str, message: &str) -> StructuredServiceError {
    StructuredServiceError {
        code: code.to_owned(),
        message: message.to_owned(),
        win32_error: None,
    }
}
