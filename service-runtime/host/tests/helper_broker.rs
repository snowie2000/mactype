use std::ffi::OsString;
use std::fs;
use std::sync::Mutex;

use mactype_service_contract::MachinePaths;
use mactype_service_host::{
    BrokerDisposition, FixedHelperBroker, HelperInvocation, HelperLaunchError, HelperLaunchStage,
    HelperLauncher, HelperOutput, InjectionBroker, InjectionRequest, ProcessArchitecture,
    ProcessIdentity, ProtectedRuntimeAssets,
};

fn assets() -> (tempfile::TempDir, ProtectedRuntimeAssets) {
    let base = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let program_files = base.path().join("Program Files");
    let program_data = base.path().join("ProgramData");
    fs::create_dir_all(&program_files).unwrap();
    fs::create_dir_all(&program_data).unwrap();
    let paths = MachinePaths::from_trusted_os_roots(&program_files, &program_data).unwrap();
    let generation = paths.runtime_versions().join("0.2.0");
    fs::create_dir_all(&generation).unwrap();
    for name in [
        "mactype-service.exe",
        "mactype-injector32.exe",
        "mactype-injector64.exe",
        "MacType.dll",
        "MacType64.dll",
        "MacType.ini",
    ] {
        fs::write(generation.join(name), name.as_bytes()).unwrap();
    }
    fs::create_dir_all(paths.runtime_pointer().parent().unwrap()).unwrap();
    fs::write(
        paths.runtime_pointer(),
        br#"{"schema":1,"version":"0.2.0"}"#,
    )
    .unwrap();
    let assets = ProtectedRuntimeAssets::load(paths).unwrap();
    (base, assets)
}

#[derive(Default)]
struct RecordingLauncher {
    invocations: Mutex<Vec<HelperInvocation>>,
}

struct StaticLauncher {
    output: Mutex<Option<HelperOutput>>,
}

impl HelperLauncher for StaticLauncher {
    fn launch(&self, _invocation: &HelperInvocation) -> Result<HelperOutput, HelperLaunchError> {
        Ok(self.output.lock().unwrap().take().unwrap())
    }
}

impl HelperLauncher for RecordingLauncher {
    fn launch(&self, invocation: &HelperInvocation) -> Result<HelperOutput, HelperLaunchError> {
        self.invocations.lock().unwrap().push(invocation.clone());
        Ok(HelperOutput {
            exit_code: 0,
            stdout: format!(
                "{{\"schemaVersion\":1,\"status\":\"injected\",\"code\":\"module-loaded\",\"pid\":42,\"sessionId\":2,\"generationId\":\"{}\",\"module\":\"MacType.dll\",\"windowsError\":0,\"cleanupComplete\":true}}",
                invocation.generation_id
            )
            .into_bytes(),
        })
    }
}

#[test]
fn fixed_helper_broker_selects_architecture_and_emits_only_the_strict_cli_contract() {
    let (_base, assets) = assets();
    let launcher = RecordingLauncher::default();
    let broker = FixedHelperBroker::new(&assets, &launcher);
    let request = InjectionRequest {
        identity: ProcessIdentity {
            pid: 42,
            creation_time: 133_967_890_123_456_789,
            session_id: 2,
            architecture: ProcessArchitecture::X86,
            protected: false,
            critical: false,
        },
        generation_id: assets.generation_id().to_owned(),
    };

    let result = broker.inject(&request);

    assert_eq!(result.disposition, BrokerDisposition::Injected);
    let invocations = launcher.invocations.lock().unwrap();
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].timeout, std::time::Duration::from_secs(20));
    assert_eq!(invocations[0].executable, assets.injector32());
    assert_eq!(invocations[0].target, request.identity);
    assert_eq!(
        invocations[0].arguments_for_process_handle(4096),
        [
            OsString::from("--process-handle"),
            OsString::from("4096"),
            OsString::from("--pid"),
            OsString::from("42"),
            OsString::from("--creation-time"),
            OsString::from("133967890123456789"),
            OsString::from("--session-id"),
            OsString::from("2"),
            OsString::from("--generation-id"),
            OsString::from(assets.generation_id()),
        ]
    );
}

struct ErrorLauncher {
    kind: std::io::ErrorKind,
    stage: HelperLaunchStage,
}

impl HelperLauncher for ErrorLauncher {
    fn launch(&self, _invocation: &HelperInvocation) -> Result<HelperOutput, HelperLaunchError> {
        Err(HelperLaunchError::new(
            self.stage,
            std::io::Error::new(self.kind, "synthetic launcher failure"),
        ))
    }
}

#[test]
fn interrupted_helper_is_a_service_stop_cancellation() {
    let (_base, assets) = assets();
    let broker = FixedHelperBroker::new(
        &assets,
        ErrorLauncher {
            kind: std::io::ErrorKind::Interrupted,
            stage: HelperLaunchStage::BeforeResume,
        },
    );
    let result = broker.inject(&InjectionRequest {
        identity: ProcessIdentity {
            pid: 42,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
            critical: false,
        },
        generation_id: assets.generation_id().to_owned(),
    });

    assert_eq!(result.disposition, BrokerDisposition::Cancelled);
    assert_eq!(result.code, "helper-cancelled-service-stop");
}

#[test]
fn before_resume_launch_failure_never_claims_unknown_target_cleanup() {
    let (_base, assets) = assets();
    let broker = FixedHelperBroker::new(
        &assets,
        ErrorLauncher {
            kind: std::io::ErrorKind::InvalidInput,
            stage: HelperLaunchStage::BeforeResume,
        },
    );
    let result = broker.inject(&InjectionRequest {
        identity: ProcessIdentity {
            pid: 42,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
            critical: false,
        },
        generation_id: assets.generation_id().to_owned(),
    });

    assert_eq!(result.disposition, BrokerDisposition::LaunchFailed);
    assert_eq!(result.code, "helper-launch-failed-before-resume");
    assert!(!result.code.ends_with("-cleanup-unknown"));
}

#[test]
fn post_resume_service_stop_is_terminal_cleanup_unknown() {
    let (_base, assets) = assets();
    let broker = FixedHelperBroker::new(
        &assets,
        ErrorLauncher {
            kind: std::io::ErrorKind::Interrupted,
            stage: HelperLaunchStage::AfterResume,
        },
    );
    let result = broker.inject(&InjectionRequest {
        identity: ProcessIdentity {
            pid: 42,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
            critical: false,
        },
        generation_id: assets.generation_id().to_owned(),
    });

    assert_eq!(result.disposition, BrokerDisposition::Rejected);
    assert_eq!(result.code, "helper-service-stop-cleanup-unknown");
}

#[test]
fn absolute_helper_timeout_is_terminal_cleanup_unknown() {
    let (_base, assets) = assets();
    let broker = FixedHelperBroker::new(
        &assets,
        ErrorLauncher {
            kind: std::io::ErrorKind::TimedOut,
            stage: HelperLaunchStage::AfterResume,
        },
    );
    let result = broker.inject(&InjectionRequest {
        identity: ProcessIdentity {
            pid: 42,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
            critical: false,
        },
        generation_id: assets.generation_id().to_owned(),
    });

    assert_eq!(result.disposition, BrokerDisposition::Rejected);
    assert_eq!(result.code, "helper-absolute-timeout-cleanup-unknown");
}

#[cfg(windows)]
fn stop_already_requested() -> bool {
    true
}

#[cfg(windows)]
#[test]
fn stop_requested_before_launch_prevents_a_new_helper_process() {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use mactype_service_host::WindowsHelperLauncher;
    use mactype_service_platform::{Process, ProcessAccess};

    let creation_time = Process::open(std::process::id(), ProcessAccess::QueryLimited)
        .unwrap()
        .creation_time()
        .unwrap();

    let launcher = WindowsHelperLauncher::new(stop_already_requested);
    let started = Instant::now();
    let error = launcher
        .launch(&HelperInvocation {
            executable: PathBuf::from(r"C:\Windows\System32\notepad.exe"),
            target: ProcessIdentity {
                pid: std::process::id(),
                creation_time,
                session_id: 1,
                architecture: ProcessArchitecture::X64,
                protected: false,
                critical: false,
            },
            generation_id: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .into(),
            timeout: Duration::from_secs(2),
        })
        .expect_err("a helper must not start after service stop was requested");

    assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
    assert!(started.elapsed() < Duration::from_millis(250));
}

#[test]
fn incomplete_remote_thread_cleanup_is_a_terminal_broker_result() {
    let (_base, assets) = assets();
    let generation = assets.generation_id().to_owned();
    let launcher = StaticLauncher {
        output: Mutex::new(Some(HelperOutput {
            exit_code: 4,
            stdout: format!(
                "{{\"schemaVersion\":1,\"status\":\"timeout\",\"code\":\"remote-load-timeout\",\"pid\":42,\"sessionId\":2,\"generationId\":\"{generation}\",\"module\":\"MacType64.dll\",\"windowsError\":1460,\"cleanupComplete\":false}}"
            )
            .into_bytes(),
        })),
    };
    let broker = FixedHelperBroker::new(&assets, launcher);
    let result = broker.inject(&InjectionRequest {
        identity: ProcessIdentity {
            pid: 42,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
            critical: false,
        },
        generation_id: generation,
    });

    assert_eq!(result.disposition, BrokerDisposition::Rejected);
    assert_eq!(result.code, "helper-reported-cleanup-unknown");
    assert_eq!(result.win32_error, Some(1460));
}

#[test]
fn explicit_post_injection_unknown_code_is_preserved_for_generation_health() {
    let (_base, assets) = assets();
    let generation = assets.generation_id().to_owned();
    let launcher = StaticLauncher {
        output: Mutex::new(Some(HelperOutput {
            exit_code: 3,
            stdout: format!(
                "{{\"schemaVersion\":1,\"status\":\"failed\",\"code\":\"post-injection-state-cleanup-unknown\",\"pid\":42,\"sessionId\":2,\"generationId\":\"{generation}\",\"module\":\"MacType64.dll\",\"windowsError\":299,\"cleanupComplete\":false}}"
            )
            .into_bytes(),
        })),
    };
    let broker = FixedHelperBroker::new(&assets, launcher);
    let result = broker.inject(&InjectionRequest {
        identity: ProcessIdentity {
            pid: 42,
            creation_time: 100,
            session_id: 2,
            architecture: ProcessArchitecture::X64,
            protected: false,
            critical: false,
        },
        generation_id: generation,
    });

    assert_eq!(result.disposition, BrokerDisposition::Rejected);
    assert_eq!(result.code, "post-injection-state-cleanup-unknown");
    assert_eq!(result.win32_error, Some(299));
}
