use mactype_service_contract::{GenerationId, SourceMetadata};

use super::preflight::validate_mode_matches_profile;
use super::WindowsInstallerBackend;
use crate::storage::create_protected_directory;
use crate::{
    BootstrapMode, BootstrapPreflight, FixedPayload, OpenServiceObservation, ProfileStore,
    RuntimeInstaller, SetupError,
};

const BUNDLED_DEFAULT_PROFILE: &[u8] =
    include_bytes!("../../../../../distribution/ini/Default.ini");

impl WindowsInstallerBackend {
    pub(super) fn apply_transaction(
        &self,
        snapshot: &BootstrapPreflight,
        mode: &BootstrapMode,
    ) -> Result<String, SetupError> {
        let installer = RuntimeInstaller::new(self.paths.clone());
        let store = ProfileStore::new(self.paths.clone());
        let previous_runtime = installer.inspect_current_stable()?;
        let previous_profile = store.inspect_active_generation_stable()?;
        validate_mode_matches_profile(mode, previous_profile.as_ref())?;

        create_protected_directory(self.paths.service_root())?;
        super::super::acl::harden_machine_directory(self.paths.service_root())?;
        if snapshot.open_service == OpenServiceObservation::OwnedRunning {
            self.manager.stop()?;
        }

        let payload = FixedPayload::beside_setup_executable()?;
        let mut published_default = false;
        let activation = installer.deploy_with_prepare_and_health_check(
            &payload,
            |binary| {
                let generation = match mode {
                    BootstrapMode::FreshBundledDefault => {
                        if store.inspect_active_generation_stable()?.is_some() {
                            return Err(SetupError::Runtime(
                                "an active profile appeared after fresh-install preflight"
                                    .to_owned(),
                            ));
                        }
                        let data_root = self.paths.active_profile().parent().ok_or_else(|| {
                            SetupError::Runtime("protected profile root is unavailable".to_owned())
                        })?;
                        create_protected_directory(data_root)?;
                        super::super::acl::harden_machine_directory(data_root)?;
                        let generation = store.publish_and_activate(
                            BUNDLED_DEFAULT_PROFILE,
                            SourceMetadata {
                                display_name: "MacType Control Center bundled default".to_owned(),
                            },
                        )?;
                        published_default = true;
                        generation
                    }
                    BootstrapMode::PreserveExistingProfile { generation } => {
                        let generation = GenerationId::parse(format!("sha256:{generation}"))?;
                        if store.inspect_active_generation_stable()?.as_ref() != Some(&generation) {
                            return Err(SetupError::Runtime(
                                "protected active profile changed after preflight".to_owned(),
                            ));
                        }
                        store.synchronize_active_runtime()?;
                        generation
                    }
                };
                let data_root = self.paths.active_profile().parent().ok_or_else(|| {
                    SetupError::Runtime("protected profile root is unavailable".to_owned())
                })?;
                super::super::acl::harden_machine_directory(data_root)?;
                super::super::acl::harden_machine_directory(self.paths.service_root())?;

                match snapshot.open_service {
                    OpenServiceObservation::Absent => self.manager.install(binary)?,
                    OpenServiceObservation::OwnedStopped | OpenServiceObservation::OwnedRunning => {
                        self.manager.reconfigure(binary)?;
                    }
                    OpenServiceObservation::Foreign | OpenServiceObservation::Unknown => {
                        return Err(SetupError::Runtime(
                            "bootstrap service identity changed after preflight".to_owned(),
                        ));
                    }
                }
                Ok(generation)
            },
            |_, generation| {
                self.manager
                    .start_and_wait_ready_for_profile(generation.as_str())
            },
        );

        match activation {
            Ok((_, generation)) => Ok(generation.as_str().to_owned()),
            Err(operation) => {
                let restoration = self.restore_after_failure(
                    snapshot,
                    previous_runtime,
                    previous_profile.as_ref(),
                    published_default,
                );
                match restoration {
                    Ok(()) => Err(operation),
                    Err(restoration) => Err(SetupError::CleanupUnknown(format!(
                        "bootstrap failed ({operation}); rollback failed ({restoration})"
                    ))),
                }
            }
        }
    }

    fn restore_after_failure(
        &self,
        snapshot: &BootstrapPreflight,
        previous_runtime: Option<crate::InstalledRuntime>,
        previous_profile: Option<&GenerationId>,
        published_default: bool,
    ) -> Result<(), SetupError> {
        let mut failures = Vec::new();
        let recovered = match super::super::runtime_recovery::recover(&self.paths, &self.manager) {
            Ok(recovered) => recovered,
            Err(error) => {
                return Err(SetupError::CleanupUnknown(format!(
                    "recover exact service and runtime binding: {error}"
                )));
            }
        };
        if recovered.as_ref() != previous_runtime.as_ref() {
            return Err(SetupError::CleanupUnknown(
                "bootstrap failure did not recover the exact previous runtime; refusing to infer profile or service state"
                    .to_owned(),
            ));
        }
        let store = ProfileStore::new(self.paths.clone());
        if published_default {
            if let Err(error) = store.rollback() {
                failures.push(format!("restore profile: {error}"));
            }
        }
        match snapshot.open_service {
            OpenServiceObservation::Absent | OpenServiceObservation::OwnedStopped => {}
            OpenServiceObservation::OwnedRunning => match previous_profile {
                Some(generation) => {
                    if let Err(error) = store.synchronize_active_runtime() {
                        failures.push(format!("restore runtime profile: {error}"));
                    } else if let Err(error) = self
                        .manager
                        .start_and_wait_ready_for_profile(generation.as_str())
                    {
                        failures.push(format!("restart prior service: {error}"));
                    }
                }
                None => {
                    failures.push("running service snapshot had no protected profile".to_owned())
                }
            },
            OpenServiceObservation::Foreign | OpenServiceObservation::Unknown => {
                failures.push("service identity changed during rollback".to_owned());
            }
        }
        if failures.is_empty() {
            crate::event_log::rollback_completed("bootstrap-install");
            Ok(())
        } else {
            Err(SetupError::CleanupUnknown(failures.join("; ")))
        }
    }
}
