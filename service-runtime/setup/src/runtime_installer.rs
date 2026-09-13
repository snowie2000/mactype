mod deferred_delete;
mod deployment;
mod journal;
mod retention;
mod uninstall;

use std::fs;
use std::path::{Path, PathBuf};

use mactype_service_contract::MachinePaths;

use self::journal::{validate_runtime_pointer, RuntimePointer, MAX_POINTER_BYTES};
use crate::profile_bridge::ProfileRuntimeBridge;
use crate::storage::{
    atomic_write, read_bounded_regular_file, reject_reparse_ancestors, SetupError,
};

pub struct FixedPayload {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledRuntime {
    version: String,
    service_binary: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeServiceBinding {
    Candidate,
    Previous,
    Absent,
}

impl InstalledRuntime {
    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn service_binary(&self) -> &Path {
        &self.service_binary
    }
}

pub struct RuntimeInstaller {
    paths: MachinePaths,
}

impl RuntimeInstaller {
    pub const fn new(paths: MachinePaths) -> Self {
        Self { paths }
    }

    pub fn deploy_with_health_check<F>(
        &self,
        payload: &FixedPayload,
        health_check: F,
    ) -> Result<InstalledRuntime, SetupError>
    where
        F: FnOnce(&Path) -> Result<(), SetupError>,
    {
        self.deploy(
            payload,
            |_| Ok(()),
            |binary, _| health_check(binary),
            false,
            false,
        )
        .map(|(installed, ())| installed)
    }

    /// On error this retains RollbackRequired plus the candidate pointer. The caller must invoke
    /// `recover_interrupted_activation_with_service_binding`; its adapter restores the exact
    /// previous external binding before this type restores the pointer and removes the receipt.
    pub fn deploy_with_prepare_and_health_check<P, H, T>(
        &self,
        payload: &FixedPayload,
        prepare: P,
        health_check: H,
    ) -> Result<(InstalledRuntime, T), SetupError>
    where
        P: FnOnce(&Path) -> Result<T, SetupError>,
        H: FnOnce(&Path, &T) -> Result<(), SetupError>,
    {
        self.deploy(payload, prepare, health_check, false, true)
    }

    pub fn repair_with_health_check<F>(
        &self,
        payload: &FixedPayload,
        health_check: F,
    ) -> Result<InstalledRuntime, SetupError>
    where
        F: FnOnce(&Path) -> Result<(), SetupError>,
    {
        self.repair_current_with_health_check(payload, health_check)
    }

    pub fn repair_current_with_health_check<F>(
        &self,
        payload: &FixedPayload,
        health_check: F,
    ) -> Result<InstalledRuntime, SetupError>
    where
        F: FnOnce(&Path) -> Result<(), SetupError>,
    {
        self.validate_repair_payload(payload, true)?;
        self.deploy(
            payload,
            |_| Ok(()),
            |binary, _| health_check(binary),
            true,
            false,
        )
        .map(|(installed, ())| installed)
    }

    /// On error this retains RollbackRequired plus the candidate pointer. The caller must invoke
    /// `recover_interrupted_activation_with_service_binding`; same-version bindings are treated
    /// as the exact previous binding before the pointer and receipt are finalized.
    pub fn repair_current_with_prepare_and_health_check<P, H, T>(
        &self,
        payload: &FixedPayload,
        prepare: P,
        health_check: H,
    ) -> Result<(InstalledRuntime, T), SetupError>
    where
        P: FnOnce(&Path) -> Result<T, SetupError>,
        H: FnOnce(&Path, &T) -> Result<(), SetupError>,
    {
        self.validate_repair_payload(payload, false)?;
        self.deploy(payload, prepare, health_check, true, true)
    }

    fn validate_repair_payload(
        &self,
        payload: &FixedPayload,
        allow_generic_recovery: bool,
    ) -> Result<(), SetupError> {
        let current = if allow_generic_recovery {
            self.recover_interrupted_activation()?
        } else {
            self.reject_pending_runtime_transaction()?;
            self.current()?
        }
        .ok_or_else(|| {
            SetupError::Runtime("no active protected runtime is installed".to_owned())
        })?;
        let bundled_version = payload.load()?.verified.version().to_owned();
        if current.version() != bundled_version {
            return Err(SetupError::Runtime(
                "repair cannot replace an outdated runtime; use upgrade".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn restore_pinned_current_with_health_check<F>(
        &self,
        health_check: F,
    ) -> Result<InstalledRuntime, SetupError>
    where
        F: FnOnce(&Path) -> Result<(), SetupError>,
    {
        self.recover_interrupted_activation()?;
        let current = self.current()?.ok_or_else(|| {
            SetupError::Runtime("no active protected runtime is installed".to_owned())
        })?;
        let pinned = self.load_verified_migration_pins()?;
        if !pinned.contains_key(current.version()) {
            return Err(SetupError::Runtime(
                "the active runtime is not protected by a migration pin".to_owned(),
            ));
        }
        health_check(current.service_binary())?;
        Ok(current)
    }

    fn deploy<P, H, T>(
        &self,
        payload: &FixedPayload,
        prepare: P,
        health_check: H,
        replace_invalid: bool,
        defer_external_rollback: bool,
    ) -> Result<(InstalledRuntime, T), SetupError>
    where
        P: FnOnce(&Path) -> Result<T, SetupError>,
        H: FnOnce(&Path, &T) -> Result<(), SetupError>,
    {
        if defer_external_rollback {
            self.reject_pending_runtime_transaction()?;
        } else {
            self.recover_interrupted_activation()?;
        }
        let payload = payload.load()?;
        let version = payload.verified.version().to_owned();
        let destination = self.paths.runtime_versions().join(&version);
        self.stage_payload(&payload, &destination, replace_invalid)
            .map_err(|error| {
                error.at_machine_path("stage verified runtime payload", &destination)
            })?;
        let receipt_path = self
            .paths
            .service_root()
            .join("runtime-receipts")
            .join(format!("{version}.json"));
        self.write_runtime_receipt(&payload).map_err(|error| {
            error.at_machine_path("write runtime generation receipt", &receipt_path)
        })?;

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
        let activated_pointer = RuntimePointer::new(version.clone()).map_err(|_| {
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

        let service_binary = destination.join("mactype-service.exe");
        let runtime_profile = destination.join("MacType.ini");
        let activation = ProfileRuntimeBridge::new(self.paths.clone())
            .materialize_active()
            .map_err(|error| {
                error.at_machine_path("materialize active runtime profile", &runtime_profile)
            })
            .and_then(|_| {
                prepare(&service_binary).map_err(|error| {
                    error.at_machine_path("prepare runtime activation", &service_binary)
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
                health_check(&service_binary, &prepared).map_err(|error| {
                    error.at_machine_path("run runtime activation health check", &service_binary)
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
                if rollback_failures.is_empty() {
                    if let Err(rollback_error) = self.remove_activation_journal() {
                        rollback_failures.push(format!(
                            "activation journal cleanup failed: {rollback_error}"
                        ));
                    }
                }
                if rollback_failures.is_empty() {
                    return Err(error);
                }
                return Err(SetupError::CleanupUnknown(format!(
                    "runtime activation failed ({error}); rollback remained incomplete: {}. The activation journal was retained",
                    rollback_failures.join("; ")
                )));
            }
        };
        let receipt_removed =
            self.finalize_committed_activation(old_pointer.clone(), activated_pointer)?;
        if receipt_removed {
            if let Err(error) = self.finalize_retention(old_pointer.as_ref(), &version) {
                eprintln!("runtime retention deferred: {error}");
            }
        }

        Ok((
            InstalledRuntime {
                version,
                service_binary,
            },
            prepared,
        ))
    }

    fn reject_pending_runtime_transaction(&self) -> Result<(), SetupError> {
        let repair_journal = self.paths.service_root().join("runtime-repair.json");
        for (path, label) in [
            (self.paths.runtime_activation_journal(), "activation"),
            (repair_journal.as_path(), "repair"),
        ] {
            match fs::symlink_metadata(path) {
                Ok(_) => {
                    return Err(SetupError::Runtime(format!(
                        "a pending runtime {label} transaction requires exact service-binding recovery"
                    )));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(SetupError::CleanupUnknown(format!(
                        "pending runtime {label} transaction could not be inspected at {}: {error}",
                        path.display()
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn current(&self) -> Result<Option<InstalledRuntime>, SetupError> {
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

    pub fn inspect_current_stable(&self) -> Result<Option<InstalledRuntime>, SetupError> {
        let repair_journal = self.paths.service_root().join("runtime-repair.json");
        if self.paths.runtime_activation_journal().exists() || repair_journal.exists() {
            return Err(SetupError::Runtime(
                "a runtime transaction is pending".to_owned(),
            ));
        }
        let current = self.current()?;
        if let Some(current) = &current {
            let directory = current.service_binary().parent().ok_or_else(|| {
                SetupError::Runtime("active runtime has no generation directory".to_owned())
            })?;
            self.verify_runtime_generation_receipt(current.version(), directory)?;
        }
        if self.paths.runtime_activation_journal().exists() || repair_journal.exists() {
            return Err(SetupError::Runtime(
                "a runtime transaction is pending".to_owned(),
            ));
        }
        Ok(current)
    }
}
