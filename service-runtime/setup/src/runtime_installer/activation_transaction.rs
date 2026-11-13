//! One journaled transaction that makes a staged generation the active one.
//! The ordering is what recovery relies on: the candidate journal is written
//! before the pointer switches, the pointer switches before the profile is
//! materialized and the external prepare step runs, the commit phase is
//! durable before the health check runs, and a failed activation persists the
//! rollback-required phase before any rollback is attempted. Nothing rolls
//! back from Drop: an interrupted process leaves the journal for recovery.

use std::path::Path;

use mactype_service_contract::MachinePaths;

use super::InstalledRuntime;
use super::generation_store::RuntimeGenerationStore;
use super::journal::{MAX_POINTER_BYTES, RuntimePointer, validate_runtime_pointer};
use crate::profile_bridge::ProfileRuntimeBridge;
use crate::storage::{
    SetupError, atomic_write, read_bounded_regular_file, reject_reparse_ancestors,
};

pub(super) struct RuntimeActivationTransaction<'a> {
    pub(in crate::runtime_installer) paths: &'a MachinePaths,
}

pub(super) struct ActivatedRuntime<T> {
    previous: Option<RuntimePointer>,
    activated: RuntimePointer,
    prepared: T,
}

impl<'a> RuntimeActivationTransaction<'a> {
    pub(super) const fn new(paths: &'a MachinePaths) -> Self {
        Self { paths }
    }

    pub(super) fn activate<P, H, T>(
        &self,
        version: &str,
        service_binary: &Path,
        prepare: P,
        health_check: H,
        defer_external_rollback: bool,
    ) -> Result<ActivatedRuntime<T>, SetupError>
    where
        P: FnOnce(&Path) -> Result<T, SetupError>,
        H: FnOnce(&Path, &T) -> Result<(), SetupError>,
    {
        let old_pointer = if self.paths.runtime_pointer().exists() {
            let bytes = read_bounded_regular_file(
                self.paths.runtime_pointer(),
                MAX_POINTER_BYTES,
                "active runtime pointer",
            )?;
            Some(validate_runtime_pointer(&bytes)?)
        } else {
            None
        };
        let activated_pointer = RuntimePointer::new(version.to_owned()).map_err(|_| {
            SetupError::Runtime("verified runtime version cannot form a pointer".to_owned())
        })?;
        let pointer = activated_pointer.to_bytes().map_err(|_| {
            SetupError::Runtime("verified runtime version cannot form a pointer".to_owned())
        })?;
        self.write_activation_journal(old_pointer.clone(), activated_pointer.clone())
            .map_err(|error| {
                error.at_machine_path(
                    "write candidate runtime activation receipt",
                    self.paths.runtime_activation_journal(),
                )
            })?;
        atomic_write(self.paths.runtime_pointer(), &pointer).map_err(|error| {
            error.at_machine_path(
                "switch active runtime pointer",
                self.paths.runtime_pointer(),
            )
        })?;

        let runtime_profile = service_binary.with_file_name("MacType.ini");
        let activation = ProfileRuntimeBridge::new(self.paths.clone())
            .materialize_active()
            .map_err(|error| {
                error.at_machine_path("materialize active runtime profile", &runtime_profile)
            })
            .and_then(|_| {
                prepare(service_binary).map_err(|error| {
                    error.at_machine_path("prepare runtime activation", service_binary)
                })
            })
            .and_then(|prepared| {
                self.commit_activation_journal(old_pointer.clone(), activated_pointer.clone())
                    .map_err(|error| {
                        error.at_machine_path(
                            "commit runtime activation receipt",
                            self.paths.runtime_activation_journal(),
                        )
                    })?;
                health_check(service_binary, &prepared).map_err(|error| {
                    error.at_machine_path("run runtime activation health check", service_binary)
                })?;
                Ok(prepared)
            });
        let prepared = match activation {
            Ok(prepared) => prepared,
            Err(error) => {
                if let Err(rollback_receipt_error) =
                    self.require_activation_rollback(old_pointer.clone(), activated_pointer.clone())
                {
                    return Err(SetupError::CleanupUnknown(format!(
                        "runtime activation failed ({error}); rollback was not attempted because its fail-closed receipt could not be persisted ({rollback_receipt_error})"
                    )));
                }
                if defer_external_rollback {
                    return Err(error);
                }
                let mut rollback_failures = Vec::new();
                let pointer_restored = match self
                    .restore_runtime_pointer(old_pointer.as_ref(), Some(&activated_pointer))
                {
                    Ok(()) => true,
                    Err(rollback_error) => {
                        rollback_failures
                            .push(format!("pointer restoration failed: {rollback_error}"));
                        false
                    }
                };
                if pointer_restored {
                    if let Err(rollback_error) =
                        ProfileRuntimeBridge::new(self.paths.clone()).materialize_active()
                    {
                        rollback_failures.push(format!(
                            "profile rematerialization failed: {rollback_error}"
                        ));
                    }
                } else {
                    rollback_failures.push(
                        "profile rematerialization was skipped because pointer ownership was unknown"
                            .to_owned(),
                    );
                }
                if rollback_failures.is_empty()
                    && let Err(rollback_error) = self.remove_activation_journal()
                {
                    rollback_failures.push(format!(
                        "activation journal cleanup failed: {rollback_error}"
                    ));
                }
                if rollback_failures.is_empty() {
                    return Err(error);
                }
                return Err(SetupError::RollbackFailed {
                    operation: format!("runtime activation failed ({error})"),
                    restoration: format!(
                        "rollback remained incomplete: {}. The activation journal was retained",
                        rollback_failures.join("; ")
                    ),
                });
            }
        };

        Ok(ActivatedRuntime {
            previous: old_pointer,
            activated: activated_pointer,
            prepared,
        })
    }

    pub(super) fn finalize<T>(&self, activation: ActivatedRuntime<T>) -> Result<T, SetupError> {
        let version = activation.activated.version().to_owned();
        let receipt_removed =
            self.finalize_committed_activation(activation.previous.clone(), activation.activated)?;
        if receipt_removed
            && let Err(error) = self
                .generation_store()
                .retain(activation.previous.as_ref(), &version)
        {
            eprintln!("runtime retention deferred: {error}");
        }
        Ok(activation.prepared)
    }

    pub(in crate::runtime_installer) fn current(
        &self,
    ) -> Result<Option<InstalledRuntime>, SetupError> {
        if !self.paths.runtime_pointer().exists() {
            return Ok(None);
        }
        let bytes = read_bounded_regular_file(
            self.paths.runtime_pointer(),
            MAX_POINTER_BYTES,
            "active runtime pointer",
        )?;
        let pointer = validate_runtime_pointer(&bytes)?;
        let service_binary = self
            .paths
            .runtime_versions()
            .join(pointer.version())
            .join("mactype-service.exe");
        reject_reparse_ancestors(&service_binary)?;
        if !service_binary.is_file() {
            return Err(SetupError::Runtime(
                "active service binary is missing".to_owned(),
            ));
        }
        Ok(Some(InstalledRuntime {
            version: pointer.version().to_owned(),
            service_binary,
        }))
    }

    pub(in crate::runtime_installer) fn generation_store(&self) -> RuntimeGenerationStore<'_> {
        RuntimeGenerationStore::new(self.paths)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use mactype_service_contract::{MachinePaths, RuntimeGenerationPointer};

    use super::RuntimeActivationTransaction;

    #[test]
    fn commit_failure_after_candidate_write_restores_the_previous_pointer() {
        let base = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let program_files = base.path().join("Program Files");
        let program_data = base.path().join("ProgramData");
        fs::create_dir_all(&program_files).unwrap();
        fs::create_dir_all(&program_data).unwrap();
        let paths = MachinePaths::from_trusted_os_roots(&program_files, &program_data).unwrap();
        let previous = RuntimeGenerationPointer::new("0.2.0").unwrap();
        fs::create_dir_all(paths.runtime_versions().join("0.2.0")).unwrap();
        fs::create_dir_all(paths.runtime_versions().join("0.3.0")).unwrap();
        fs::write(paths.runtime_pointer(), previous.to_bytes().unwrap()).unwrap();
        let transaction = RuntimeActivationTransaction::new(&paths);
        let service_binary = paths
            .runtime_versions()
            .join("0.3.0")
            .join("mactype-service.exe");

        let error = match transaction.activate(
            "0.3.0",
            &service_binary,
            |_| {
                fs::write(paths.runtime_pointer(), previous.to_bytes().unwrap())?;
                Ok(())
            },
            |_, _| Ok(()),
            false,
        ) {
            Ok(_) => panic!("pointer mismatch must fail the commit"),
            Err(error) => error,
        };

        assert!(
            error
                .to_string()
                .contains("commit runtime activation receipt")
        );
        assert_eq!(
            fs::read(paths.runtime_pointer()).unwrap(),
            previous.to_bytes().unwrap()
        );
        assert!(!paths.runtime_activation_journal().exists());
    }
}
