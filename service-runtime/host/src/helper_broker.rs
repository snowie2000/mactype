use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::{
    BrokerDisposition, BrokerResult, HostEvent, HostEventSink, InjectionBroker, InjectionRequest,
    ProcessArchitecture, ProcessIdentity, ProtectedRuntimeAssets,
};

const HELPER_TIMEOUT: Duration = Duration::from_secs(20);
pub(crate) const MAX_HELPER_OUTPUT_BYTES: usize = 1024;

/// The code the helper itself reports when it finds this generation's module
/// already loaded in the target (`service-injector/src/injector.cpp`). The
/// broker answers with the same code when it can see that for itself, so the
/// skip tally counts a relayed target once, under one name, however it was
/// noticed.
const MODULE_ALREADY_LOADED_CODE: &str = "module-already-loaded";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperInvocation {
    pub executable: PathBuf,
    pub target: crate::ProcessIdentity,
    pub generation_id: String,
    pub timeout: Duration,
}

impl HelperInvocation {
    pub fn arguments_for_process_handle(&self, process_handle: usize) -> Vec<OsString> {
        vec![
            "--process-handle".into(),
            process_handle.to_string().into(),
            "--pid".into(),
            self.target.pid.to_string().into(),
            "--creation-time".into(),
            self.target.creation_time.to_string().into(),
            "--session-id".into(),
            self.target.session_id.to_string().into(),
            "--generation-id".into(),
            self.generation_id.clone().into(),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperOutput {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelperLaunchStage {
    BeforeResume,
    AfterResume,
}

#[derive(Debug)]
pub struct HelperLaunchError {
    stage: HelperLaunchStage,
    source: io::Error,
}

impl HelperLaunchError {
    pub fn new(stage: HelperLaunchStage, source: io::Error) -> Self {
        Self { stage, source }
    }

    pub fn after_resume(source: io::Error) -> Self {
        Self::new(HelperLaunchStage::AfterResume, source)
    }

    pub const fn stage(&self) -> HelperLaunchStage {
        self.stage
    }

    pub fn kind(&self) -> io::ErrorKind {
        self.source.kind()
    }

    pub fn raw_os_error(&self) -> Option<i32> {
        self.source.raw_os_error()
    }
}

impl From<io::Error> for HelperLaunchError {
    fn from(source: io::Error) -> Self {
        Self::new(HelperLaunchStage::BeforeResume, source)
    }
}

impl fmt::Display for HelperLaunchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(formatter)
    }
}

impl std::error::Error for HelperLaunchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

pub trait HelperLauncher {
    fn launch(&self, invocation: &HelperInvocation) -> Result<HelperOutput, HelperLaunchError>;
}

impl<T> HelperLauncher for &T
where
    T: HelperLauncher + ?Sized,
{
    fn launch(&self, invocation: &HelperInvocation) -> Result<HelperOutput, HelperLaunchError> {
        (**self).launch(invocation)
    }
}

/// What a target's module list said about this generation's renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixedModuleState {
    /// Exactly this generation's module, at its path in the protected runtime,
    /// is loaded in the target already.
    Loaded,
    /// The list was read and the module was not in it.
    Absent,
    /// The question could not be answered.
    Unknown,
}

/// Asks whether a target already carries the renderer, so the broker can skip
/// a helper launch that has nothing left to do.
pub trait FixedModuleProbe {
    fn fixed_module_state(&self, identity: &ProcessIdentity, module: &Path) -> FixedModuleState;
}

/// The probe the service runs with: a module-list read on the live target.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlatformFixedModuleProbe;

#[cfg(windows)]
impl FixedModuleProbe for PlatformFixedModuleProbe {
    fn fixed_module_state(&self, identity: &ProcessIdentity, module: &Path) -> FixedModuleState {
        use mactype_service_platform::ModulePresence;

        match mactype_service_platform::process_module_presence(
            identity.pid,
            identity.creation_time,
            module,
        ) {
            ModulePresence::Loaded => FixedModuleState::Loaded,
            ModulePresence::Absent => FixedModuleState::Absent,
            ModulePresence::Unknown => FixedModuleState::Unknown,
        }
    }
}

#[cfg(not(windows))]
impl FixedModuleProbe for PlatformFixedModuleProbe {
    fn fixed_module_state(&self, _identity: &ProcessIdentity, _module: &Path) -> FixedModuleState {
        FixedModuleState::Unknown
    }
}

pub struct FixedHelperBroker<L> {
    assets: ProtectedRuntimeAssets,
    launcher: L,
    events: Arc<dyn HostEventSink>,
    module_probe: Box<dyn FixedModuleProbe>,
}

impl<L> FixedHelperBroker<L> {
    pub fn new(
        assets: &ProtectedRuntimeAssets,
        launcher: L,
        events: Arc<dyn HostEventSink>,
    ) -> Self {
        Self::with_module_probe(assets, launcher, events, Box::new(PlatformFixedModuleProbe))
    }

    pub fn with_module_probe(
        assets: &ProtectedRuntimeAssets,
        launcher: L,
        events: Arc<dyn HostEventSink>,
        module_probe: Box<dyn FixedModuleProbe>,
    ) -> Self {
        Self {
            assets: assets.clone(),
            launcher,
            events,
            module_probe,
        }
    }

    fn invocation(&self, request: &InjectionRequest) -> HelperInvocation {
        let executable = match request.identity.architecture {
            ProcessArchitecture::X86 => self.assets.injector32(),
            ProcessArchitecture::X64 => self.assets.injector64(),
        };
        HelperInvocation {
            executable: executable.to_owned(),
            target: request.identity.clone(),
            generation_id: request.generation_id.clone(),
            timeout: HELPER_TIMEOUT,
        }
    }

    /// This generation's renderer for `architecture`, at its path inside the
    /// protected runtime generation root.
    fn fixed_module(&self, architecture: ProcessArchitecture) -> PathBuf {
        self.assets.root().join(match architecture {
            ProcessArchitecture::X86 => "MacType.dll",
            ProcessArchitecture::X64 => "MacType64.dll",
        })
    }
}

impl<L> InjectionBroker for FixedHelperBroker<L>
where
    L: HelperLauncher,
{
    fn verify_ready(
        &self,
        architecture: ProcessArchitecture,
    ) -> Result<(), mactype_service_contract::StructuredServiceError> {
        let helper = match architecture {
            ProcessArchitecture::X86 => self.assets.injector32(),
            ProcessArchitecture::X64 => self.assets.injector64(),
        };
        if helper.is_file() && helper.parent() == Some(self.assets.root()) {
            Ok(())
        } else {
            self.events.record(HostEvent::HelperBrokerFailed {
                architecture,
                code: "runtime-helper-unavailable".to_owned(),
                detail: Some(format!("helper={}", helper.display())),
            });
            Err(mactype_service_contract::StructuredServiceError {
                code: "runtime-helper-unavailable".to_owned(),
                message: "the fixed helper is not ready in the protected runtime generation"
                    .to_owned(),
                win32_error: None,
            })
        }
    }

    fn inject(&self, request: &InjectionRequest) -> BrokerResult {
        if request.generation_id != self.assets.generation_id() {
            return invalid_response("runtime-generation-mismatch", None);
        }
        // Launching the helper costs a process creation and several hundred
        // milliseconds to arrive at the very answer we can read here, so a
        // target that already carries the renderer is answered directly. Only
        // a positive match skips; absent, unanswerable, and every failure go
        // on to the helper exactly as before.
        let module = self.fixed_module(request.identity.architecture);
        if self
            .module_probe
            .fixed_module_state(&request.identity, &module)
            == FixedModuleState::Loaded
        {
            return BrokerResult {
                disposition: BrokerDisposition::Skipped,
                code: MODULE_ALREADY_LOADED_CODE.to_owned(),
                win32_error: None,
            };
        }
        let invocation = self.invocation(request);
        let result = match self.launcher.launch(&invocation) {
            Ok(output) => parse_output(request, output),
            Err(error)
                if error.stage() == HelperLaunchStage::BeforeResume
                    && error.kind() == io::ErrorKind::Interrupted =>
            {
                BrokerResult {
                    disposition: BrokerDisposition::Cancelled,
                    code: "helper-cancelled-service-stop".to_owned(),
                    win32_error: error.raw_os_error().map(|code| code as u32),
                }
            }
            Err(error) => BrokerResult {
                disposition: if error.stage() == HelperLaunchStage::BeforeResume {
                    BrokerDisposition::LaunchFailed
                } else {
                    BrokerDisposition::Rejected
                },
                code: if error.stage() == HelperLaunchStage::BeforeResume {
                    "helper-launch-failed-before-resume"
                } else if error.kind() == io::ErrorKind::Interrupted {
                    "helper-service-stop-cleanup-unknown"
                } else if error.kind() == io::ErrorKind::TimedOut {
                    "helper-absolute-timeout-cleanup-unknown"
                } else {
                    "helper-launch-failed-cleanup-unknown"
                }
                .to_owned(),
                win32_error: error.raw_os_error().map(|code| code as u32),
            },
        };
        if result.code.ends_with("-cleanup-unknown")
            || matches!(
                result.code.as_str(),
                "helper-response-invalid" | "helper-response-too-large" | "helper-exit-mismatch"
            )
        {
            self.events.record(HostEvent::HelperBrokerFailed {
                architecture: request.identity.architecture,
                code: result.code.clone(),
                detail: Some(format!(
                    "pid={} creation_time={} win32={:?}",
                    request.identity.pid, request.identity.creation_time, result.win32_error
                )),
            });
        }
        result
    }
}

fn parse_output(request: &InjectionRequest, output: HelperOutput) -> BrokerResult {
    if output.stdout.len() > MAX_HELPER_OUTPUT_BYTES {
        return invalid_response("helper-response-too-large", None);
    }
    let value: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(value) => value,
        Err(_) => return invalid_response("helper-response-invalid", None),
    };
    let object = match value.as_object() {
        Some(object) if object.len() == 9 => object,
        _ => return invalid_response("helper-response-invalid", None),
    };
    let status = object.get("status").and_then(serde_json::Value::as_str);
    let Some(code) = object
        .get("code")
        .and_then(serde_json::Value::as_str)
        .filter(|code| !code.is_empty())
    else {
        return invalid_response("helper-response-invalid", None);
    };
    let pid = object.get("pid").and_then(serde_json::Value::as_u64);
    let session = object.get("sessionId").and_then(serde_json::Value::as_u64);
    let generation = object
        .get("generationId")
        .and_then(serde_json::Value::as_str);
    let module = object.get("module").and_then(serde_json::Value::as_str);
    let windows_error = object
        .get("windowsError")
        .and_then(serde_json::Value::as_u64);
    let cleanup = object
        .get("cleanupComplete")
        .and_then(serde_json::Value::as_bool);
    let expected_module = match request.identity.architecture {
        ProcessArchitecture::X86 => "MacType.dll",
        ProcessArchitecture::X64 => "MacType64.dll",
    };
    if object
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
        || pid != Some(u64::from(request.identity.pid))
        || session != Some(u64::from(request.identity.session_id))
        || generation != Some(request.generation_id.as_str())
        || module != Some(expected_module)
        || windows_error.is_none_or(|value| value > u64::from(u32::MAX))
        || cleanup.is_none()
    {
        return invalid_response("helper-response-invalid", None);
    }

    let (mut disposition, expected_exit) = match status {
        Some("injected") => (BrokerDisposition::Injected, 0),
        Some("skipped") if code == "process-frozen" => (BrokerDisposition::TargetFrozen, 0),
        Some("skipped") => (BrokerDisposition::Skipped, 0),
        Some("rejected") => (BrokerDisposition::Rejected, 2),
        Some("failed") => (BrokerDisposition::RetryableFailure, 3),
        Some("timeout") => (BrokerDisposition::RetryableFailure, 4),
        _ => return invalid_response("helper-response-invalid", None),
    };
    if output.exit_code != expected_exit {
        return invalid_response("helper-exit-mismatch", None);
    }
    if cleanup == Some(false) {
        disposition = BrokerDisposition::Rejected;
    }
    BrokerResult {
        disposition,
        code: if cleanup == Some(false) {
            if code.ends_with("-cleanup-unknown") {
                code.to_owned()
            } else {
                "helper-reported-cleanup-unknown".to_owned()
            }
        } else {
            code.to_owned()
        },
        win32_error: windows_error
            .filter(|value| *value != 0)
            .map(|value| value as u32),
    }
}

fn invalid_response(code: &str, win32_error: Option<u32>) -> BrokerResult {
    BrokerResult {
        disposition: BrokerDisposition::RetryableFailure,
        code: code.to_owned(),
        win32_error,
    }
}
