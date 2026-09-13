mod acl;
mod appinit;
mod broker;
mod installer;
mod known_folders;
mod legacy_tray;
mod machine_lock;
mod runtime_recovery;
pub(crate) mod scm;

use mactype_service_contract::{BrokerCommand, MachinePaths};

use crate::SetupError;

pub(crate) fn event_log_path() -> Result<std::path::PathBuf, SetupError> {
    let paths = known_folders::machine_paths()?;
    prepare_event_log_directory(&paths)?;
    Ok(paths.service_setup_event_log().to_owned())
}

pub(crate) fn prepare_event_log_directory(paths: &MachinePaths) -> Result<(), SetupError> {
    let data_root = paths
        .event_log_dir()
        .parent()
        .ok_or_else(|| SetupError::Runtime("protected event-log root is unavailable".to_owned()))?;
    crate::storage::create_protected_directory(data_root)?;
    acl::harden_machine_directory(data_root)?;
    crate::storage::create_protected_directory(paths.event_log_dir())?;
    acl::harden_machine_directory(data_root)
}

pub fn run_installer_bootstrap() -> Result<String, SetupError> {
    installer::run_bootstrap().map(|outcome| outcome.to_json("bootstrap-install"))
}

pub fn run_owned_uninstall() -> Result<String, SetupError> {
    installer::run_uninstall().map(|outcome| outcome.to_json())
}

pub fn run(command: BrokerCommand, profile_input: Option<&[u8]>) -> Result<String, SetupError> {
    broker::run(command, profile_input)
}
