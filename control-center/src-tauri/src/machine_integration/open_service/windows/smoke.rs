use super::*;

pub(in crate::machine_integration::open_service) fn system_removal_verification(
    expected_digest: &str,
) -> Result<MigrationVerification, String> {
    let status = query();
    let scm_running_ready = status.backend == ServiceBackend::OpenSource
        && status.installation == InstallationState::Current
        && status.runtime == RuntimeState::Running
        && status.health == HealthState::Ready;
    let active_digest_match = status.active_profile_digest.as_deref() == Some(expected_digest);
    if !scm_running_ready || !active_digest_match {
        return Ok(MigrationVerification {
            scm_running_ready,
            active_digest_match,
            telemetry_verified: false,
        });
    }
    let report = read_health_for_scm_process(running_service_process_id()?)?;
    let runtime_generation_id = protected_runtime_generation_id()?;
    Ok(MigrationVerification {
        scm_running_ready,
        active_digest_match,
        telemetry_verified: report.verified_for_migration(&runtime_generation_id, expected_digest),
    })
}

fn protected_runtime_generation_id() -> Result<String, String> {
    let paths = fixed_machine_paths()?;
    let root = active_runtime_root(&paths)?
        .ok_or_else(|| "the protected active runtime is unavailable".to_owned())?;
    runtime_generation_id_from_root(&root)
}

fn runtime_generation_id_from_root(root: &Path) -> Result<String, String> {
    let mut files = BTreeMap::new();
    for name in IMMUTABLE_RUNTIME_FILES {
        let path = root.join(name);
        let bytes = read_bounded_regular_file(
            &path,
            MAX_RUNTIME_FILE_BYTES as u64,
            "protected runtime component",
        )?;
        files.insert(name.to_owned(), bytes);
    }
    runtime_generation_id(&files).map_err(|error| error.to_string())
}

struct MarkerProcess(Child);

impl MarkerProcess {
    fn start(executable: &Path) -> Result<Self, String> {
        let child = Command::new(executable)
            .args(["-n", "30", "-w", "1000", "127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(Self(child))
    }

    fn pid(&self) -> u32 {
        self.0.id()
    }

    fn exited(&mut self) -> Result<bool, String> {
        self.0
            .try_wait()
            .map(|status| status.is_some())
            .map_err(|error| error.to_string())
    }
}

impl Drop for MarkerProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(super) fn system_injection_smoke(expected_digest: &str) -> Result<bool, String> {
    let runtime_generation_id = protected_runtime_generation_id()?;
    let service_process_id = running_service_process_id()?;
    for (architecture, executable) in fixed_marker_executables()? {
        let mut marker = MarkerProcess::start(&executable)?;
        if !wait_for_marker_injection(
            &mut marker,
            architecture,
            &runtime_generation_id,
            expected_digest,
            service_process_id,
        )? {
            return Ok(false);
        }
    }
    read_verified_health_with(
        40,
        || read_health_for_scm_process(service_process_id),
        || thread::sleep(Duration::from_millis(25)),
        |report| report.verified_for_migration(&runtime_generation_id, expected_digest),
    )
}

fn read_verified_health_with(
    maximum_attempts: usize,
    mut read: impl FnMut() -> Result<HealthReport, String>,
    mut wait: impl FnMut(),
    verify: impl Fn(&HealthReport) -> bool,
) -> Result<bool, String> {
    if maximum_attempts == 0 {
        return Err("final injection smoke has no health-read retry budget".to_owned());
    }
    let mut last_transport_error = None;
    for attempt in 0..maximum_attempts {
        match read() {
            Ok(report) => return Ok(verify(&report)),
            Err(error) => last_transport_error = Some(error),
        }
        if attempt + 1 < maximum_attempts {
            wait();
        }
    }
    Err(format!(
        "final injection smoke could not read bounded live health after {maximum_attempts} attempts: {}",
        last_transport_error.unwrap_or_else(|| "unknown transport failure".to_owned())
    ))
}

fn fixed_marker_executables() -> Result<[(InjectionArchitecture, PathBuf); 2], String> {
    let machine = Process::open(std::process::id(), ProcessAccess::QueryLimited)
        .and_then(|process| process.machine())
        .map_err(|error| {
            format!("could not determine the native architecture for migration smoke: {error}")
        })?;
    let process_machine = match machine.process {
        MachineKind::I386 => IMAGE_FILE_MACHINE_I386,
        MachineKind::Amd64 => IMAGE_FILE_MACHINE_AMD64,
        MachineKind::Unknown => IMAGE_FILE_MACHINE_UNKNOWN,
        MachineKind::Other(value) => value,
    };
    let native_machine = match machine.native {
        MachineKind::I386 => IMAGE_FILE_MACHINE_I386,
        MachineKind::Amd64 => IMAGE_FILE_MACHINE_AMD64,
        MachineKind::Unknown => IMAGE_FILE_MACHINE_UNKNOWN,
        MachineKind::Other(value) => value,
    };
    let windows = known_folder(KnownFolder::Windows)?;
    let x64_system = windows.join(marker_x64_system_directory(
        process_machine,
        native_machine,
    )?);
    let executables = [
        (
            InjectionArchitecture::X86,
            windows.join("SysWOW64").join("ping.exe"),
        ),
        (InjectionArchitecture::X64, x64_system.join("ping.exe")),
    ];
    for (_, executable) in &executables {
        if !executable.is_file() {
            return Err(format!(
                "the fixed migration marker is unavailable: {}",
                executable.display()
            ));
        }
    }
    Ok(executables)
}

pub(in crate::machine_integration::open_service) fn marker_x64_system_directory(
    process_machine: u16,
    native_machine: u16,
) -> Result<&'static str, String> {
    if native_machine == IMAGE_FILE_MACHINE_ARM64 {
        return Err("native ARM64 cannot run the required x86 and x64 migration smoke".to_owned());
    }
    if native_machine != IMAGE_FILE_MACHINE_AMD64 {
        return Err(
            "migration smoke requires native AMD64 with both x86 and x64 support".to_owned(),
        );
    }
    match process_machine {
        IMAGE_FILE_MACHINE_I386 => Ok("Sysnative"),
        IMAGE_FILE_MACHINE_UNKNOWN | IMAGE_FILE_MACHINE_AMD64 => Ok("System32"),
        _ => Err("the Control Center process architecture is unsupported".to_owned()),
    }
}

fn wait_for_marker_injection(
    marker: &mut MarkerProcess,
    architecture: InjectionArchitecture,
    runtime_generation_id: &str,
    expected_digest: &str,
    service_process_id: u32,
) -> Result<bool, String> {
    const TIMEOUT: Duration = Duration::from_secs(20);
    const POLL: Duration = Duration::from_millis(100);
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        if marker.exited()? {
            return Err("the fixed migration marker exited before injection".to_owned());
        }
        if let Ok(report) = read_health_for_scm_process(service_process_id) {
            let telemetry = match architecture {
                InjectionArchitecture::X86 => &report.injection.x86,
                InjectionArchitecture::X64 => &report.injection.x64,
            };
            if telemetry.last_success.as_ref().is_some_and(|success| {
                success.pid == marker.pid()
                    && success.runtime_generation_id == runtime_generation_id
                    && success.profile_digest == expected_digest
            }) {
                return Ok(true);
            }
        }
        thread::sleep(POLL);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn final_smoke_retries_only_transport_errors_and_accepts_verified_health() {
        let mut observations = VecDeque::from([
            Err("all pipe instances are busy (os error 231)".to_owned()),
            Err("all pipe instances are busy (os error 231)".to_owned()),
            Ok(HealthReport::ready(
                "0.2.0",
                Some("sha256:expected".to_owned()),
            )),
        ]);
        let mut waits = 0;

        let verified = read_verified_health_with(
            4,
            || observations.pop_front().unwrap(),
            || waits += 1,
            |report| report.active_profile_digest.as_deref() == Some("sha256:expected"),
        )
        .unwrap();

        assert!(verified);
        assert_eq!(waits, 2);
    }

    #[test]
    fn final_smoke_fails_closed_without_retrying_a_mismatched_report() {
        let mut observations = VecDeque::from([
            Ok(HealthReport::ready(
                "0.2.0",
                Some("sha256:wrong".to_owned()),
            )),
            Ok(HealthReport::ready(
                "0.2.0",
                Some("sha256:expected".to_owned()),
            )),
        ]);
        let mut waits = 0;

        let verified = read_verified_health_with(
            3,
            || observations.pop_front().unwrap(),
            || waits += 1,
            |report| report.active_profile_digest.as_deref() == Some("sha256:expected"),
        )
        .unwrap();

        assert!(!verified);
        assert_eq!(waits, 0);
        assert_eq!(observations.len(), 1);
    }

    #[test]
    fn oversized_protected_runtime_component_is_rejected_before_generation() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mactype-protected-runtime-generation-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        for name in IMMUTABLE_RUNTIME_FILES {
            fs::write(root.join(name), b"runtime component").unwrap();
        }
        fs::OpenOptions::new()
            .write(true)
            .open(root.join(IMMUTABLE_RUNTIME_FILES[0]))
            .unwrap()
            .set_len(MAX_RUNTIME_FILE_BYTES as u64 + 1)
            .unwrap();

        let error = runtime_generation_id_from_root(&root).unwrap_err();

        assert!(error.contains("byte limit"), "{error}");
        fs::remove_dir_all(root).unwrap();
    }
}
