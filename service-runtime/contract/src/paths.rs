use std::fmt;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachinePaths {
    service_root: PathBuf,
    runtime_versions: PathBuf,
    runtime_pointer: PathBuf,
    runtime_activation_journal: PathBuf,
    profile_generations: PathBuf,
    active_profile: PathBuf,
    previous_profile: PathBuf,
    profile_activation_journal: PathBuf,
    event_log_dir: PathBuf,
    service_host_event_log: PathBuf,
    service_setup_event_log: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachinePathError;

impl fmt::Display for MachinePathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("machine roots must be absolute protected Windows roots")
    }
}

impl std::error::Error for MachinePathError {}

impl MachinePaths {
    pub fn from_trusted_os_roots(
        program_files: &Path,
        program_data: &Path,
    ) -> Result<Self, MachinePathError> {
        validate_protected_root(program_files)?;
        validate_protected_root(program_data)?;

        let service_root = program_files.join("MacType Control Center").join("Service");
        let data_root = program_data.join("MacType").join("ControlCenter");
        let event_log_dir = data_root.join("logs");

        Ok(Self {
            runtime_versions: service_root.join("bin"),
            runtime_pointer: service_root.join("current.json"),
            runtime_activation_journal: service_root.join("runtime-activation.json"),
            profile_generations: data_root.join("generations"),
            active_profile: data_root.join("active.json"),
            previous_profile: data_root.join("previous.json"),
            profile_activation_journal: data_root.join("profile-activation.json"),
            service_host_event_log: event_log_dir.join("service-host.log"),
            service_setup_event_log: event_log_dir.join("service-setup.log"),
            event_log_dir,
            service_root,
        })
    }

    pub fn service_root(&self) -> &Path {
        &self.service_root
    }

    pub fn runtime_versions(&self) -> &Path {
        &self.runtime_versions
    }

    pub fn runtime_pointer(&self) -> &Path {
        &self.runtime_pointer
    }

    pub fn runtime_activation_journal(&self) -> &Path {
        &self.runtime_activation_journal
    }

    pub fn profile_generations(&self) -> &Path {
        &self.profile_generations
    }

    pub fn active_profile(&self) -> &Path {
        &self.active_profile
    }

    pub fn previous_profile(&self) -> &Path {
        &self.previous_profile
    }

    pub fn profile_activation_journal(&self) -> &Path {
        &self.profile_activation_journal
    }

    pub fn event_log_dir(&self) -> &Path {
        &self.event_log_dir
    }

    pub fn service_host_event_log(&self) -> &Path {
        &self.service_host_event_log
    }

    pub fn service_setup_event_log(&self) -> &Path {
        &self.service_setup_event_log
    }
}

fn validate_protected_root(path: &Path) -> Result<(), MachinePathError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(MachinePathError);
    }

    let normalized = path.to_string_lossy().replace('/', "\\").to_lowercase();
    if normalized.contains("\\users\\")
        || normalized.contains("\\appdata\\")
        || normalized.contains("\\localappdata\\")
    {
        return Err(MachinePathError);
    }

    Ok(())
}
