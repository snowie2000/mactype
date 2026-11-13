mod activation_transaction;
mod deferred_delete;
mod deployment;
mod generation_store;
mod journal;
mod retention;
mod uninstall;

use std::fs;
use std::path::{Path, PathBuf};

use mactype_service_contract::MachinePaths;

use self::activation_transaction::RuntimeActivationTransaction;
use self::generation_store::RuntimeGenerationStore;
use crate::storage::SetupError;

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

    fn activation_transaction(&self) -> RuntimeActivationTransaction<'_> {
        RuntimeActivationTransaction::new(&self.paths)
    }

    fn generation_store(&self) -> RuntimeGenerationStore<'_> {
        RuntimeGenerationStore::new(&self.paths)
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

    pub fn recover_interrupted_activation(&self) -> Result<Option<InstalledRuntime>, SetupError> {
        self.recover_interrupted_repair()?;
        self.activation_transaction().recover()
    }

    pub fn recover_interrupted_activation_with_service_binding<I, R>(
        &self,
        inspect_service_binding: I,
        restore_previous_service_binding: R,
    ) -> Result<Option<InstalledRuntime>, SetupError>
    where
        I: FnMut(Option<&Path>, Option<&Path>) -> Result<RuntimeServiceBinding, SetupError>,
        R: FnOnce(&Path, Option<&Path>) -> Result<(), SetupError>,
    {
        self.recover_interrupted_repair()?;
        self.activation_transaction()
            .recover_with_service_binding(inspect_service_binding, restore_previous_service_binding)
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
        if !self.generation_store().verify_pinned(current.version())? {
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
        let destination =
            self.generation_store()
                .stage(&payload, replace_invalid, |destination, payload| {
                    self.replace_runtime_payload(destination, payload)
                })?;

        let service_binary = destination.join("mactype-service.exe");
        let activation = self.activation_transaction().activate(
            &version,
            &service_binary,
            prepare,
            health_check,
            defer_external_rollback,
        )?;
        let prepared = self.activation_transaction().finalize(activation)?;

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
        self.activation_transaction().current()
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
            self.generation_store()
                .verify(current.version(), directory)?;
        }
        if self.paths.runtime_activation_journal().exists() || repair_journal.exists() {
            return Err(SetupError::Runtime(
                "a runtime transaction is pending".to_owned(),
            ));
        }
        Ok(current)
    }
}
