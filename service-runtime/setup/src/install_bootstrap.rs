use std::path::{Component, Path};

use mactype_service_contract::{BrokerCommand, BrokerCommandError, parse_broker_command};

use crate::SetupError;

/// Whether the installer bootstrap must leave the service running or may
/// leave a stopped or absent service alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapStartPolicy {
    EnsureRunning,
    PreserveObservedState,
}

impl BootstrapStartPolicy {
    pub const fn verb(self) -> &'static str {
        match self {
            Self::EnsureRunning => "bootstrap-install",
            Self::PreserveObservedState => "bootstrap-install-preserve-run-state",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupCommand {
    Broker(BrokerCommand),
    BootstrapInstall(BootstrapStartPolicy),
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
    for policy in [
        BootstrapStartPolicy::EnsureRunning,
        BootstrapStartPolicy::PreserveObservedState,
    ] {
        if verb.as_ref() == policy.verb() {
            return Ok(SetupCommand::BootstrapInstall(policy));
        }
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
pub enum BootstrapProfileMode {
    PublishBundledDefault,
    PreserveExisting { generation: String },
    LeaveUnpublished,
}

/// A profile is published only when the service will start: an install that
/// stays stopped keeps the protected store empty, which the host reports as
/// its supported stopped state at every boot until a profile is published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapPlan {
    pub profile: BootstrapProfileMode,
    pub start_service: bool,
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
        active_profile_digest: Option<String>,
        preserved_existing_profile: bool,
        service_started: bool,
    },
    SkippedBlocked {
        reason: BootstrapBlocker,
    },
}

pub trait InstallBootstrapBackend {
    fn inspect(&mut self) -> BootstrapPreflight;

    fn apply_atomically(&mut self, plan: &BootstrapPlan) -> Result<Option<String>, SetupError>;
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

pub fn run_install_bootstrap_with<B>(
    backend: &mut B,
    policy: BootstrapStartPolicy,
) -> Result<BootstrapOutcome, SetupError>
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
    let start_service = match policy {
        BootstrapStartPolicy::EnsureRunning => true,
        BootstrapStartPolicy::PreserveObservedState => {
            preflight.open_service == OpenServiceObservation::OwnedRunning
        }
    };
    let (profile, preserved_existing_profile) = match preflight.protected_profile {
        ProtectedProfileObservation::Active(generation) => {
            (BootstrapProfileMode::PreserveExisting { generation }, true)
        }
        ProtectedProfileObservation::Absent | ProtectedProfileObservation::Unknown => {
            if start_service {
                (BootstrapProfileMode::PublishBundledDefault, false)
            } else {
                (BootstrapProfileMode::LeaveUnpublished, false)
            }
        }
    };
    let plan = BootstrapPlan {
        profile,
        start_service,
    };
    let digest = backend.apply_atomically(&plan)?;
    let active_profile_digest = match (&plan.profile, digest) {
        (BootstrapProfileMode::PreserveExisting { generation }, Some(digest)) => {
            let expected = format!("sha256:{generation}");
            if digest != expected {
                return Err(SetupError::Runtime(format!(
                    "Ready profile digest mismatch: expected {expected}, received {digest}"
                )));
            }
            Some(digest)
        }
        (BootstrapProfileMode::PublishBundledDefault, Some(digest)) => Some(digest),
        (BootstrapProfileMode::LeaveUnpublished, None) => None,
        (BootstrapProfileMode::LeaveUnpublished, Some(digest)) => {
            return Err(SetupError::Runtime(format!(
                "bootstrap left the profile unpublished but reported digest {digest}"
            )));
        }
        (BootstrapProfileMode::PreserveExisting { .. }, None)
        | (BootstrapProfileMode::PublishBundledDefault, None) => {
            return Err(SetupError::Runtime(
                "bootstrap published a profile but reported no Ready digest".to_owned(),
            ));
        }
    };
    Ok(BootstrapOutcome::Applied {
        active_profile_digest,
        preserved_existing_profile,
        service_started: plan.start_service,
    })
}
