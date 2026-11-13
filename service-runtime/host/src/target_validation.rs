use mactype_service_contract::StructuredServiceError;

use crate::{InspectedProcess, ProcessIdentity, ProcessInspector, TargetLifecycle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessTargetDecision {
    Eligible(ProcessIdentity),
    Skipped(SkipReason),
    Deferred {
        identity: ProcessIdentity,
        reason: DeferralReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    SelfProcess,
    SessionZero,
    Protected,
    CriticalOrUnknown,
    DynamicCodeMitigation,
    BinarySignatureMitigation,
    ImportantWindowsProcess,
    InstallerControlProcess,
    ImageNameUnavailable,
    Exiting,
    InspectionFailed(&'static str),
}

impl SkipReason {
    /// The stable name this skip is counted under. A skipped target writes no
    /// event of its own, so without this the reason dies in the function that
    /// computed it and an operator sees only a total.
    pub const fn code(self) -> &'static str {
        match self {
            Self::SelfProcess => "self-process",
            Self::SessionZero => "session-zero",
            Self::Protected => "protected",
            Self::CriticalOrUnknown => "critical-or-unknown",
            Self::DynamicCodeMitigation => "dynamic-code-mitigation",
            Self::BinarySignatureMitigation => "binary-signature-mitigation",
            Self::ImportantWindowsProcess => "important-windows-process",
            Self::InstallerControlProcess => "installer-control-process",
            Self::ImageNameUnavailable => "image-name-unavailable",
            Self::Exiting => "exiting",
            Self::InspectionFailed(_) => "inspection-failed",
        }
    }
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
        let InspectedProcess { identity, facts } = match self.inspector.inspect(pid) {
            Ok(inspected) => inspected,
            Err(error) => match target_scoped_inspection_failure(&error.code) {
                Some(code) => {
                    return Ok(ProcessTargetDecision::Skipped(
                        SkipReason::InspectionFailed(code),
                    ));
                }
                None => return Err(error),
            },
        };
        if identity.pid != pid {
            return Err(service_error(
                "process-identity-mismatch",
                "the inspected process identity does not match the observed PID",
            ));
        }
        if identity.pid == self.service_pid {
            return Ok(ProcessTargetDecision::Skipped(SkipReason::SelfProcess));
        }
        if identity.session_id == 0 {
            return Ok(ProcessTargetDecision::Skipped(SkipReason::SessionZero));
        }
        if identity.protected {
            return Ok(ProcessTargetDecision::Skipped(SkipReason::Protected));
        }
        if facts.critical_or_unknown {
            return Ok(ProcessTargetDecision::Skipped(
                SkipReason::CriticalOrUnknown,
            ));
        }
        if facts.prohibits_dynamic_code {
            return Ok(ProcessTargetDecision::Skipped(
                SkipReason::DynamicCodeMitigation,
            ));
        }
        if facts.restricts_binary_signature {
            return Ok(ProcessTargetDecision::Skipped(
                SkipReason::BinarySignatureMitigation,
            ));
        }
        let Some(image_name) = facts.image_name.as_deref() else {
            return Ok(ProcessTargetDecision::Skipped(
                SkipReason::ImageNameUnavailable,
            ));
        };
        if is_important_windows_process(image_name) {
            return Ok(ProcessTargetDecision::Skipped(
                SkipReason::ImportantWindowsProcess,
            ));
        }
        if is_installer_control_process(image_name) {
            return Ok(ProcessTargetDecision::Skipped(
                SkipReason::InstallerControlProcess,
            ));
        }
        match self.inspector.probe_target_lifecycle(&identity) {
            TargetLifecycle::Exiting => Ok(ProcessTargetDecision::Skipped(SkipReason::Exiting)),
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

fn target_scoped_inspection_failure(code: &str) -> Option<&'static str> {
    match code {
        "process-protected-or-inaccessible" => Some("process-protected-or-inaccessible"),
        "process-creation-time-unavailable" => Some("process-creation-time-unavailable"),
        "process-session-unavailable" => Some("process-session-unavailable"),
        "process-architecture-unavailable" => Some("process-architecture-unavailable"),
        "process-architecture-unsupported" => Some("process-architecture-unsupported"),
        _ => None,
    }
}

/// Whether this image is a relay root, meaning its children inherit MacType
/// through the in-process child-process relay rather than through a separate
/// injection. The interactive session's shell is the only one, because it is
/// the parent of everything the operator launches, so a backlog that reaches
/// it last leaves every program started meanwhile without the relay.
pub fn is_relay_root(image_name: &str) -> bool {
    image_name == "explorer.exe"
}

fn is_important_windows_process(name: &str) -> bool {
    matches!(
        name,
        "smss.exe"
            | "csrss.exe"
            | "wininit.exe"
            | "winlogon.exe"
            | "services.exe"
            | "lsass.exe"
            | "fontdrvhost.exe"
    )
}

fn is_installer_control_process(name: &str) -> bool {
    name == "mactype-service-setup.exe" || is_inno_uninstaller(name)
}

fn is_inno_uninstaller(name: &str) -> bool {
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    if !matches!(extension, "exe" | "tmp") {
        return false;
    }
    let stem = stem.strip_prefix('_').unwrap_or(stem);
    stem.strip_prefix("unins")
        .is_some_and(|sequence| sequence.bytes().all(|character| character.is_ascii_digit()))
}

fn service_error(code: &str, message: &str) -> StructuredServiceError {
    StructuredServiceError {
        code: code.to_owned(),
        message: message.to_owned(),
        win32_error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_important_windows_process, is_installer_control_process};

    #[test]
    fn installer_control_processes_are_never_injection_targets() {
        for name in [
            "mactype-service-setup.exe",
            "unins000.exe",
            "unins000.tmp",
            "_unins.tmp",
            "_unins001.exe",
            "_unins001.tmp",
        ] {
            assert!(
                is_installer_control_process(name),
                "installer control process was eligible for injection: {name}"
            );
            assert!(
                !is_important_windows_process(name),
                "installer control process leaked into the Windows system-process predicate: {name}"
            );
        }

        for name in [
            "mactype-service-setup.exe.disabled",
            "uninstall-helper.exe",
            "unison.exe",
        ] {
            assert!(
                !is_installer_control_process(name),
                "unrelated process was excluded by an over-broad name rule: {name}"
            );
        }

        assert!(is_important_windows_process("services.exe"));
        assert!(!is_installer_control_process("services.exe"));
    }
}
