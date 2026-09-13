#![forbid(unsafe_code)]

mod event_log;
mod install_bootstrap;
mod profile_bridge;
mod profile_store;
mod runtime_installer;
mod storage;

#[cfg(windows)]
mod windows;

pub use install_bootstrap::{
    parse_setup_command, protected_installer_broker_layout, run_install_bootstrap_with,
    run_uninstall_owned_with, BootstrapBlocker, BootstrapMode, BootstrapOutcome,
    BootstrapPreflight, ConflictObservation, InstallBootstrapBackend, OpenServiceObservation,
    ProtectedProfileObservation, ProtectedRuntimeObservation, SetupCommand, UninstallBackend,
    UninstallOutcome,
};
pub use profile_store::ProfileStore;
pub use runtime_installer::{
    FixedPayload, InstalledRuntime, RuntimeInstaller, RuntimeServiceBinding,
};
pub use storage::SetupError;

#[cfg(all(windows, feature = "ci-test-adapter"))]
pub use windows::scm::{
    service_configuration_drift, service_configuration_matches_owned_contract,
    service_identity_matches_owned_contract, service_image_matches_protected_contract,
    ObservedServiceConfiguration,
};

pub fn run_broker_command(
    command: mactype_service_contract::BrokerCommand,
    profile_input: Option<&[u8]>,
) -> Result<String, SetupError> {
    let result = {
        #[cfg(windows)]
        {
            windows::run(command, profile_input)
        }
        #[cfg(not(windows))]
        {
            let _ = profile_input;
            Err(SetupError::Runtime(
                "the machine service broker requires Windows".to_owned(),
            ))
        }
    };
    if let Err(error) = &result {
        event_log::command_failed(SetupCommand::Broker(command), error, profile_input);
    }
    result
}

pub fn run_setup_command(
    command: SetupCommand,
    profile_input: Option<&[u8]>,
) -> Result<String, SetupError> {
    let result = match command {
        SetupCommand::Broker(command) => return run_broker_command(command, profile_input),
        SetupCommand::BootstrapInstall => {
            #[cfg(windows)]
            {
                windows::run_installer_bootstrap()
            }
            #[cfg(not(windows))]
            {
                Err(SetupError::Runtime(
                    "installer bootstrap requires Windows".to_owned(),
                ))
            }
        }
        SetupCommand::UninstallOwned => {
            #[cfg(windows)]
            {
                windows::run_owned_uninstall()
            }
            #[cfg(not(windows))]
            {
                Err(SetupError::Runtime(
                    "owned service uninstall requires Windows".to_owned(),
                ))
            }
        }
    };
    if let Err(error) = &result {
        event_log::command_failed(command, error, profile_input);
    }
    result
}
