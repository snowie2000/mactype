use std::io;

use mactype_service_contract::StructuredServiceError;
use mactype_service_platform::{process_session_id, MachineKind, Process, ProcessAccess};
use windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER;

use crate::{
    ProcessArchitecture, ProcessIdentity, ProcessInspector, TargetLifecycle, TargetLiveness,
};

pub struct WindowsProcessInspector {
    service_pid: u32,
}

impl WindowsProcessInspector {
    pub const fn new(service_pid: u32) -> Self {
        Self { service_pid }
    }
}

impl ProcessInspector for WindowsProcessInspector {
    fn inspect(&self, pid: u32) -> Result<ProcessIdentity, StructuredServiceError> {
        if pid == 0 {
            return Err(service_error(
                "process-identity-invalid",
                "process ID zero cannot be inspected",
                None,
            ));
        }
        let process = open_for_identity(pid)?;
        let creation_time = process.creation_time().map_err(|error| {
            os_error(
                "process-creation-time-unavailable",
                "the observed process creation time could not be read",
                &error,
            )
        })?;
        let session_id = process_session_id(pid).map_err(|error| {
            os_error(
                "process-session-unavailable",
                "the observed process session could not be read",
                &error,
            )
        })?;
        let machine = process.machine().map_err(|error| {
            os_error(
                "process-architecture-unavailable",
                "the observed process architecture could not be read",
                &error,
            )
        })?;
        let architecture = classify_process_architecture(machine.process, machine.native)?;
        let protected = process.is_protected_or_unknown();
        let excluded_from_injection = pid == self.service_pid || must_skip_injection(&process);
        Ok(ProcessIdentity {
            pid,
            creation_time,
            session_id,
            architecture,
            protected,
            critical: excluded_from_injection,
        })
    }

    fn probe_target_lifecycle(&self, identity: &ProcessIdentity) -> TargetLifecycle {
        let process = match Process::open(identity.pid, ProcessAccess::QueryLimited) {
            Ok(process) => process,
            Err(error) if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) => {
                return TargetLifecycle::Exiting
            }
            Err(_) => return TargetLifecycle::Unknown,
        };
        match process.creation_time() {
            Ok(created) if created != identity.creation_time => return TargetLifecycle::Exiting,
            Ok(_) => {}
            Err(_) => return TargetLifecycle::Unknown,
        }
        if matches!(process.exit_code(), Ok(Some(_))) {
            return TargetLifecycle::Exiting;
        }
        match process.lifecycle_flags() {
            Ok(flags) if flags.deleting => TargetLifecycle::Exiting,
            Ok(flags) if flags.frozen => TargetLifecycle::Frozen,
            Ok(_) => TargetLifecycle::Running,
            Err(_) => TargetLifecycle::Unknown,
        }
    }

    fn process_basename(&self, identity: &ProcessIdentity) -> Option<String> {
        let process = Process::open(identity.pid, ProcessAccess::QueryLimited).ok()?;
        if process.creation_time().ok()? != identity.creation_time {
            return None;
        }
        image_name(&process)
    }

    fn probe_target_liveness(&self, identity: &ProcessIdentity) -> TargetLiveness {
        probe_windows_target_liveness(identity)
    }
}

fn probe_windows_target_liveness(identity: &ProcessIdentity) -> TargetLiveness {
    let process = match Process::open(identity.pid, ProcessAccess::QueryLimited) {
        Ok(process) => process,
        // A PID that has left the process table fails to open with
        // ERROR_INVALID_PARAMETER; every other failure (such as access
        // denied) leaves liveness undetermined.
        Err(error) => {
            return if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
                TargetLiveness::Vanished
            } else {
                TargetLiveness::Unknown
            };
        }
    };
    match process.creation_time() {
        // The PID was reused by a different process; the verified target is gone.
        Ok(creation_time) if creation_time != identity.creation_time => TargetLiveness::Vanished,
        Ok(_) => match process.exit_code() {
            // An open handle can keep an exited process object observable.
            Ok(Some(_)) => TargetLiveness::Vanished,
            Ok(None) => TargetLiveness::Alive,
            Err(_) => TargetLiveness::Unknown,
        },
        Err(_) => TargetLiveness::Unknown,
    }
}

fn open_for_identity(pid: u32) -> Result<Process, StructuredServiceError> {
    Process::open(pid, ProcessAccess::QueryLimited).map_err(|error| {
        os_error(
            "process-protected-or-inaccessible",
            "the observed process cannot be opened for identity verification",
            &error,
        )
    })
}

fn must_skip_injection(process: &Process) -> bool {
    if process.is_critical_or_unknown() {
        return true;
    }
    if process
        .dynamic_code_mitigation()
        .is_ok_and(|policy| policy.prohibit_dynamic_code && !policy.allow_thread_opt_out)
        || process.binary_signature_mitigation().is_ok_and(|policy| {
            policy.microsoft_signed_only || policy.store_signed_only || policy.mitigation_opt_in
        })
    {
        return true;
    }
    image_name(process).as_deref().map_or(true, |name| {
        is_important_windows_process(name) || is_installer_control_process(name)
    })
}

fn image_name(process: &Process) -> Option<String> {
    process
        .image_path()?
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
}

fn classify_process_architecture(
    process_machine: MachineKind,
    native_machine: MachineKind,
) -> Result<ProcessArchitecture, StructuredServiceError> {
    match (process_machine, native_machine) {
        (MachineKind::I386, _) | (MachineKind::Unknown, MachineKind::I386) => {
            Ok(ProcessArchitecture::X86)
        }
        (MachineKind::Amd64, _) | (MachineKind::Unknown, MachineKind::Amd64) => {
            Ok(ProcessArchitecture::X64)
        }
        _ => Err(service_error(
            "process-architecture-unsupported",
            "the observed process architecture has no compatible helper",
            None,
        )),
    }
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

/// A structured error carrying the Win32 code of the platform failure.
fn os_error(code: &str, message: &str, error: &io::Error) -> StructuredServiceError {
    service_error(code, message, error.raw_os_error())
}

fn service_error(code: &str, message: &str, win32_error: Option<i32>) -> StructuredServiceError {
    StructuredServiceError {
        code: code.to_owned(),
        message: message.to_owned(),
        win32_error: win32_error.map(|code| code as u32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::SystemInformation::IMAGE_FILE_MACHINE_ARM64;

    #[test]
    fn native_arm64_is_not_sent_to_the_x64_helper() {
        let arm64 = MachineKind::Other(IMAGE_FILE_MACHINE_ARM64);
        assert_eq!(
            classify_process_architecture(MachineKind::Amd64, arm64).unwrap(),
            ProcessArchitecture::X64
        );
        let error = classify_process_architecture(MachineKind::Unknown, arm64)
            .expect_err("native ARM64 has no fixed compatible helper");
        assert_eq!(error.code, "process-architecture-unsupported");
    }

    #[test]
    fn liveness_probe_distinguishes_a_running_process_from_a_reused_pid() {
        let inspector = WindowsProcessInspector::new(0);
        let own_identity = inspector.inspect(std::process::id()).unwrap();
        assert_eq!(
            probe_windows_target_liveness(&own_identity),
            TargetLiveness::Alive
        );

        let different_creation_time = ProcessIdentity {
            creation_time: own_identity.creation_time.wrapping_add(1),
            ..own_identity
        };
        assert_eq!(
            probe_windows_target_liveness(&different_creation_time),
            TargetLiveness::Vanished
        );
    }

    #[test]
    fn liveness_probe_reports_an_exited_child_as_vanished() {
        let mut child = std::process::Command::new("cmd")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let identity = WindowsProcessInspector::new(0).inspect(child.id()).unwrap();
        drop(child.stdin.take());
        child.wait().unwrap();

        assert_eq!(
            probe_windows_target_liveness(&identity),
            TargetLiveness::Vanished
        );
    }

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
