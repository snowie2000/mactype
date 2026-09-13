use std::path::{Component, Path};

use mactype_service_contract::{parse_broker_command, BrokerCommand, BrokerCommandError};

use crate::SetupError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupCommand {
    Broker(BrokerCommand),
    BootstrapInstall,
    UninstallOwned,
}

pub fn parse_setup_command<I, S>(arguments: I) -> Result<SetupCommand, BrokerCommandError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut arguments = arguments.into_iter();
    let verb = arguments.next().ok_or(BrokerCommandError)?;
    if arguments.next().is_some() {
        return Err(BrokerCommandError);
    }
    if verb.as_ref() == "bootstrap-install" {
        return Ok(SetupCommand::BootstrapInstall);
    }
    if verb.as_ref() == "uninstall-owned" {
        return Ok(SetupCommand::UninstallOwned);
    }
    parse_broker_command(std::iter::once(verb)).map(SetupCommand::Broker)
}

pub fn protected_installer_broker_layout(program_files: &Path, executable: &Path) -> bool {
    if !program_files.is_absolute()
        || !executable.is_absolute()
        || program_files
            .components()
            .chain(executable.components())
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return false;
    }
    let expected = program_files
        .join("MacType Control Center")
        .join("service-runtime")
        .join("mactype-service-setup.exe");
    expected
        .to_string_lossy()
        .eq_ignore_ascii_case(&executable.to_string_lossy())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictObservation {
    Clear,
    Detected,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenServiceObservation {
    Absent,
    OwnedStopped,
    OwnedRunning,
    Foreign,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtectedProfileObservation {
    Absent,
    Active(String),
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtectedRuntimeObservation {
    Absent,
    Active,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapPreflight {
    pub open_service: OpenServiceObservation,
    pub protected_profile: ProtectedProfileObservation,
    pub protected_runtime: ProtectedRuntimeObservation,
    pub legacy_service: ConflictObservation,
    pub legacy_tray: ConflictObservation,
    pub appinit: ConflictObservation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootstrapMode {
    FreshBundledDefault,
    PreserveExistingProfile { generation: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootstrapBlocker {
    LegacyService,
    LegacyTrayMode,
    AppInit,
    ForeignOpenService,
    UnknownMachineState,
    InconsistentOwnedState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootstrapOutcome {
    Applied {
        active_profile_digest: String,
        preserved_existing_profile: bool,
    },
    SkippedBlocked {
        reason: BootstrapBlocker,
    },
}

pub trait InstallBootstrapBackend {
    fn inspect(&mut self) -> BootstrapPreflight;

    fn apply_atomically(&mut self, mode: &BootstrapMode) -> Result<String, SetupError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UninstallOutcome {
    Removed,
    AlreadyAbsent,
    SkippedBlocked { reason: BootstrapBlocker },
}

pub trait UninstallBackend {
    fn inspect_open_service(&mut self) -> OpenServiceObservation;

    fn remove_owned_installation(
        &mut self,
        observed_service: OpenServiceObservation,
    ) -> Result<bool, SetupError>;
}

pub fn run_uninstall_owned_with<B>(backend: &mut B) -> Result<UninstallOutcome, SetupError>
where
    B: UninstallBackend,
{
    let observation = backend.inspect_open_service();
    match observation {
        OpenServiceObservation::Absent
        | OpenServiceObservation::OwnedStopped
        | OpenServiceObservation::OwnedRunning => {
            let runtime_removed =
                backend
                    .remove_owned_installation(observation)
                    .map_err(|error| {
                        SetupError::Runtime(format!("owned installation removal failed: {error}"))
                    })?;
            if observation == OpenServiceObservation::Absent && !runtime_removed {
                Ok(UninstallOutcome::AlreadyAbsent)
            } else {
                Ok(UninstallOutcome::Removed)
            }
        }
        OpenServiceObservation::Foreign => Ok(UninstallOutcome::SkippedBlocked {
            reason: BootstrapBlocker::ForeignOpenService,
        }),
        OpenServiceObservation::Unknown => Ok(UninstallOutcome::SkippedBlocked {
            reason: BootstrapBlocker::UnknownMachineState,
        }),
    }
}

pub fn run_install_bootstrap_with<B>(backend: &mut B) -> Result<BootstrapOutcome, SetupError>
where
    B: InstallBootstrapBackend,
{
    let preflight = backend.inspect();
    if preflight.legacy_service == ConflictObservation::Detected {
        return Ok(BootstrapOutcome::SkippedBlocked {
            reason: BootstrapBlocker::LegacyService,
        });
    }
    if preflight.legacy_tray == ConflictObservation::Detected {
        return Ok(BootstrapOutcome::SkippedBlocked {
            reason: BootstrapBlocker::LegacyTrayMode,
        });
    }
    if preflight.appinit == ConflictObservation::Detected {
        return Ok(BootstrapOutcome::SkippedBlocked {
            reason: BootstrapBlocker::AppInit,
        });
    }
    if preflight.open_service == OpenServiceObservation::Foreign {
        return Ok(BootstrapOutcome::SkippedBlocked {
            reason: BootstrapBlocker::ForeignOpenService,
        });
    }
    if preflight.open_service == OpenServiceObservation::Unknown
        || preflight.protected_profile == ProtectedProfileObservation::Unknown
        || preflight.protected_runtime == ProtectedRuntimeObservation::Unknown
        || preflight.legacy_service == ConflictObservation::Unknown
        || preflight.legacy_tray == ConflictObservation::Unknown
        || preflight.appinit == ConflictObservation::Unknown
    {
        return Ok(BootstrapOutcome::SkippedBlocked {
            reason: BootstrapBlocker::UnknownMachineState,
        });
    }
    let service_is_owned = matches!(
        preflight.open_service,
        OpenServiceObservation::OwnedStopped | OpenServiceObservation::OwnedRunning
    );
    if (service_is_owned && preflight.protected_runtime == ProtectedRuntimeObservation::Absent)
        || (preflight.open_service == OpenServiceObservation::OwnedRunning
            && preflight.protected_profile == ProtectedProfileObservation::Absent)
    {
        return Ok(BootstrapOutcome::SkippedBlocked {
            reason: BootstrapBlocker::InconsistentOwnedState,
        });
    }
    let (mode, preserved_existing_profile) = match preflight.protected_profile {
        ProtectedProfileObservation::Active(generation) => {
            (BootstrapMode::PreserveExistingProfile { generation }, true)
        }
        ProtectedProfileObservation::Absent | ProtectedProfileObservation::Unknown => {
            (BootstrapMode::FreshBundledDefault, false)
        }
    };
    let digest = backend.apply_atomically(&mode)?;
    if let BootstrapMode::PreserveExistingProfile { generation } = &mode {
        let expected = format!("sha256:{generation}");
        if digest != expected {
            return Err(SetupError::Runtime(format!(
                "Ready profile digest mismatch: expected {expected}, received {digest}"
            )));
        }
    }
    Ok(BootstrapOutcome::Applied {
        active_profile_digest: digest,
        preserved_existing_profile,
    })
}
