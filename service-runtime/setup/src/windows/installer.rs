mod output;
mod preflight;
mod transaction;

use mactype_service_contract::MachinePaths;

use super::{known_folders, machine_lock, runtime_recovery, scm};
use crate::{
    run_install_bootstrap_with, run_uninstall_owned_with, BootstrapOutcome, BootstrapPreflight,
    InstallBootstrapBackend, OpenServiceObservation, RuntimeInstaller, SetupError,
    UninstallBackend, UninstallOutcome,
};

pub fn run_bootstrap() -> Result<BootstrapOutcome, SetupError> {
    let _lock = machine_lock::MachineSetupLock::acquire()?;
    let paths = known_folders::machine_paths()?;
    preflight::validate_and_harden_installer_root(&paths)?;
    let manager = scm::ServiceManager::connect(paths.service_root().to_owned())?;
    runtime_recovery::recover(&paths, &manager)?;
    run_install_bootstrap_with(&mut WindowsInstallerBackend::new(paths, manager))
}

pub fn run_uninstall() -> Result<UninstallOutcome, SetupError> {
    let _lock = machine_lock::MachineSetupLock::acquire()?;
    let paths = known_folders::machine_paths()?;
    preflight::validate_and_harden_installer_root(&paths)?;
    let manager = scm::ServiceManager::connect(paths.service_root().to_owned())?;
    runtime_recovery::recover(&paths, &manager)?;
    run_uninstall_owned_with(&mut WindowsInstallerBackend::new(paths, manager))
}

struct WindowsInstallerBackend {
    paths: MachinePaths,
    manager: scm::ServiceManager,
    inspected: Option<BootstrapPreflight>,
}

impl WindowsInstallerBackend {
    const fn new(paths: MachinePaths, manager: scm::ServiceManager) -> Self {
        Self {
            paths,
            manager,
            inspected: None,
        }
    }
}

impl InstallBootstrapBackend for WindowsInstallerBackend {
    fn inspect(&mut self) -> BootstrapPreflight {
        let snapshot = self.inspect_snapshot();
        self.inspected = Some(snapshot.clone());
        snapshot
    }

    fn apply_atomically(&mut self, mode: &crate::BootstrapMode) -> Result<String, SetupError> {
        let expected = self.inspected.clone().ok_or_else(|| {
            SetupError::Runtime("bootstrap mutation was requested before preflight".to_owned())
        })?;
        let actual = self.inspect_snapshot();
        if actual != expected {
            return Err(SetupError::Runtime(
                "machine integration state changed after bootstrap preflight".to_owned(),
            ));
        }
        self.apply_transaction(&actual, mode)
    }
}

impl UninstallBackend for WindowsInstallerBackend {
    fn inspect_open_service(&mut self) -> OpenServiceObservation {
        self.manager.observe_fixed_service()
    }

    fn remove_owned_installation(
        &mut self,
        observed_service: OpenServiceObservation,
    ) -> Result<bool, SetupError> {
        RuntimeInstaller::new(self.paths.clone()).remove_receipted_installation_after(|| {
            let actual = self.manager.observe_fixed_service();
            match (observed_service, actual) {
                (OpenServiceObservation::Absent, OpenServiceObservation::Absent) => Ok(()),
                (
                    OpenServiceObservation::OwnedStopped | OpenServiceObservation::OwnedRunning,
                    OpenServiceObservation::Absent,
                ) => Ok(()),
                (
                    OpenServiceObservation::OwnedStopped | OpenServiceObservation::OwnedRunning,
                    OpenServiceObservation::OwnedStopped | OpenServiceObservation::OwnedRunning,
                ) => self.manager.remove(),
                _ => Err(SetupError::Runtime(
                    "open service identity changed during uninstall".to_owned(),
                )),
            }
        })
    }
}
