use std::{env, fs::OpenOptions, io::Write, path::PathBuf};

const READY_MARKER_ENV: &str = "MACTYPE_CI_SINGLE_INSTANCE_READY";
const EVENT_MARKER_ENV: &str = "MACTYPE_CI_SINGLE_INSTANCE_EVENTS";

#[cfg(windows)]
const STARTUP_GATE_NAME: &str = "Local\\MacTypeControlCenter.StartupGate";

/// How long a launch waits for an instance that is already starting. A cold
/// first run behind antivirus or a first-run WebView2 setup can hold the gate
/// far longer than a warm one.
#[cfg(windows)]
const GATE_WAIT_MS: u32 = 120_000;

pub(crate) struct StartupGate {
    #[cfg(windows)]
    mutex: Option<mactype_service_platform::NamedMutex>,
}

impl StartupGate {
    #[cfg(windows)]
    pub(crate) fn acquire() -> Result<Self, String> {
        use mactype_service_platform::{MutexAcquisition, NamedMutex};
        use std::time::Duration;

        let mutex =
            NamedMutex::create_with_default_security(STARTUP_GATE_NAME).map_err(|error| {
                format!("failed to create the single-instance startup gate: Windows error {error}")
            })?;
        match mutex
            .acquire(Duration::from_millis(u64::from(GATE_WAIT_MS)))
            .map_err(|error| {
                format!("failed to acquire the single-instance startup gate: {error}")
            })? {
            MutexAcquisition::Acquired | MutexAcquisition::Abandoned => {
                Ok(Self { mutex: Some(mutex) })
            }
            MutexAcquisition::TimedOut => {
                // Another instance has been starting for the whole wait. That is
                // the situation the single-instance plugin exists to resolve: this
                // process hands its activation to the one already running and
                // exits. Refusing to start instead turned a slow first launch into
                // a crash dialog, because the caller can only panic here.
                Ok(Self { mutex: None })
            }
        }
    }

    #[cfg(not(windows))]
    pub(crate) fn acquire() -> Result<Self, String> {
        Ok(Self {})
    }

    #[cfg(windows)]
    pub(crate) fn release(mut self) -> Result<(), String> {
        self.mutex.take();
        Ok(())
    }

    #[cfg(not(windows))]
    pub(crate) fn release(self) -> Result<(), String> {
        Ok(())
    }
}

pub(crate) fn write_ready_marker() -> Result<(), String> {
    let Some(path) = env::var_os(READY_MARKER_ENV) else {
        return Ok(());
    };
    std::fs::write(PathBuf::from(path), format!("{}\n", std::process::id()))
        .map_err(|error| format!("failed to write single-instance ready marker: {error}"))
}

pub(crate) fn record_activation(
    args: Vec<String>,
    cwd: String,
    restored: bool,
) -> Result<(), String> {
    let Some(path) = env::var_os(EVENT_MARKER_ENV) else {
        return Ok(());
    };
    let event = serde_json::json!({
        "args": args,
        "cwd": cwd,
        "restored": restored,
        "pid": std::process::id(),
    });
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(PathBuf::from(path))
        .map_err(|error| format!("failed to open single-instance event marker: {error}"))?;
    writeln!(file, "{event}")
        .map_err(|error| format!("failed to write single-instance event marker: {error}"))
}
