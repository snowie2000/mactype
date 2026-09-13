mod generation;
mod journal;
mod pointer;

use mactype_service_contract::{
    GenerationId, GenerationPointer, MachinePaths, ProfileCatalog, SourceMetadata,
};
use serde::{Deserialize, Serialize};

use crate::profile_bridge::{MaterializedProfileClearError, ProfileRuntimeBridge};
use crate::storage::{reject_reparse_ancestors, SetupError};

const LEGACY_PROFILE_ACTIVATION_SCHEMA: u32 = 1;
const PROFILE_ACTIVATION_SCHEMA: u32 = 2;
const MAX_ACTIVATION_JOURNAL_BYTES: u64 = 16 * 1024;
const MAX_POINTER_BYTES: u64 = 64 * 1024;

#[derive(Clone)]
pub struct ProfileStore {
    paths: MachinePaths,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ProfileActivationPhase {
    PointerTransition,
    FirstRollbackDeletePending,
    FirstRollbackDeleteConfirmed,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileActivationJournal {
    schema: u32,
    phase: ProfileActivationPhase,
    active_before: Option<GenerationPointer>,
    previous_before: Option<GenerationPointer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyProfileActivationJournal {
    schema: u32,
    active_before: Option<GenerationPointer>,
    previous_before: Option<GenerationPointer>,
}

impl ProfileStore {
    pub const fn new(paths: MachinePaths) -> Self {
        Self { paths }
    }

    pub fn publish_and_activate(
        &self,
        profile_bytes: &[u8],
        source_metadata: SourceMetadata,
    ) -> Result<GenerationId, SetupError> {
        self.recover_interrupted_activation()?;
        if source_metadata.display_name.trim().is_empty()
            || source_metadata.display_name.len() > 256
        {
            return Err(SetupError::InvalidMetadata);
        }

        let mut catalog = ProfileCatalog::new();
        let generation = catalog.publish_machine_profile(profile_bytes, source_metadata.clone())?;
        self.publish_generation(&generation, profile_bytes, &source_metadata)?;
        self.activate(&generation)?;
        Ok(generation)
    }

    pub fn activate(&self, generation: &GenerationId) -> Result<(), SetupError> {
        self.activate_with_post_pointer_hook(generation, || {})
    }

    #[cfg(feature = "ci-test-adapter")]
    pub fn activate_with_post_pointer_hook_for_ci<F>(
        &self,
        generation: &GenerationId,
        hook: F,
    ) -> Result<(), SetupError>
    where
        F: FnOnce(),
    {
        self.activate_with_post_pointer_hook(generation, hook)
    }

    fn activate_with_post_pointer_hook<F>(
        &self,
        generation: &GenerationId,
        hook: F,
    ) -> Result<(), SetupError>
    where
        F: FnOnce(),
    {
        self.recover_interrupted_activation()?;
        self.verify_generation(generation)?;
        let active = self.read_optional_pointer(self.paths.active_profile())?;
        if active.as_ref().map(GenerationPointer::generation) == Some(generation) {
            ProfileRuntimeBridge::new(self.paths.clone()).materialize_generation(generation)?;
            return Ok(());
        }

        let previous_before = self.read_optional_pointer(self.paths.previous_profile())?;
        self.write_activation_journal(
            ProfileActivationPhase::PointerTransition,
            active.clone(),
            previous_before,
        )?;
        let transition = (|| {
            if let Some(active) = active {
                self.verify_generation(active.generation())?;
                self.write_pointer(self.paths.previous_profile(), active.generation())?;
            }
            self.write_pointer(self.paths.active_profile(), generation)?;
            hook();
            ProfileRuntimeBridge::new(self.paths.clone()).materialize_generation(generation)?;
            Ok(())
        })();
        self.finish_activation_transaction(transition)
    }

    pub fn rollback(&self) -> Result<Option<GenerationId>, SetupError> {
        self.rollback_with_clear_and_post_clear_hook(
            |bridge, generation| bridge.clear_materialized_generation(generation),
            || {},
        )
    }

    #[cfg(feature = "ci-test-adapter")]
    pub fn rollback_with_post_clear_hook_for_ci<F>(
        &self,
        hook: F,
    ) -> Result<Option<GenerationId>, SetupError>
    where
        F: FnOnce(),
    {
        self.rollback_with_clear_and_post_clear_hook(
            |bridge, generation| bridge.clear_materialized_generation(generation),
            hook,
        )
    }

    #[cfg(feature = "ci-test-adapter")]
    pub fn rollback_with_unknown_clear_for_ci(&self) -> Result<Option<GenerationId>, SetupError> {
        self.rollback_with_clear_and_post_clear_hook(
            |_bridge, _generation| {
                Err(MaterializedProfileClearError::Unknown(
                    SetupError::CleanupUnknown(
                        "simulated unknown generated profile deletion result".to_owned(),
                    ),
                ))
            },
            || {},
        )
    }

    fn rollback_with_clear_and_post_clear_hook<C, F>(
        &self,
        clear: C,
        hook: F,
    ) -> Result<Option<GenerationId>, SetupError>
    where
        C: FnOnce(
            &ProfileRuntimeBridge,
            &GenerationId,
        ) -> Result<bool, MaterializedProfileClearError>,
        F: FnOnce(),
    {
        self.recover_interrupted_activation()?;
        let active = self
            .read_optional_pointer(self.paths.active_profile())?
            .ok_or(SetupError::InvalidPointer)?;
        let previous = self.read_optional_pointer(self.paths.previous_profile())?;

        self.verify_generation(active.generation())?;
        if let Some(previous) = &previous {
            self.verify_generation(previous.generation())?;
        }
        let initial_phase = if previous.is_none() {
            ProfileActivationPhase::FirstRollbackDeletePending
        } else {
            ProfileActivationPhase::PointerTransition
        };
        self.write_activation_journal(initial_phase, Some(active.clone()), previous.clone())?;
        if previous.is_none() {
            let bridge = ProfileRuntimeBridge::new(self.paths.clone());
            match clear(&bridge, active.generation()) {
                Ok(_) => {}
                Err(MaterializedProfileClearError::Unchanged(error)) => {
                    self.remove_activation_journal().map_err(|journal_error| {
                        SetupError::CleanupUnknown(format!(
                            "profile was unchanged after cleanup rejection ({error}), but the activation journal could not be removed ({journal_error})"
                        ))
                    })?;
                    return Err(error);
                }
                Err(MaterializedProfileClearError::Unknown(error)) => return Err(error),
            }
            bridge.ensure_materialized_profile_absent()?;
            self.write_activation_journal(
                ProfileActivationPhase::FirstRollbackDeleteConfirmed,
                Some(active.clone()),
                previous.clone(),
            )
            .map_err(|error| {
                SetupError::CleanupUnknown(format!(
                    "generated profile was deleted, but its confirmed rollback phase could not be persisted: {error}"
                ))
            })?;
            hook();
        }
        let transition = match &previous {
            Some(previous) => (|| {
                self.write_pointer(self.paths.previous_profile(), active.generation())?;
                self.write_pointer(self.paths.active_profile(), previous.generation())?;
                ProfileRuntimeBridge::new(self.paths.clone())
                    .materialize_generation(previous.generation())?;
                Ok(())
            })(),
            None => (|| {
                self.restore_pointer(self.paths.active_profile(), None)?;
                self.restore_pointer(self.paths.previous_profile(), None)?;
                Ok(())
            })(),
        };
        self.finish_activation_transaction(transition)?;
        Ok(previous.map(|pointer| pointer.generation().clone()))
    }

    pub fn active_generation(&self) -> Result<Option<GenerationId>, SetupError> {
        self.recover_interrupted_activation()?;
        let pointer = self.read_optional_pointer(self.paths.active_profile())?;
        if let Some(pointer) = &pointer {
            self.verify_generation(pointer.generation())?;
        }
        Ok(pointer.map(|pointer| pointer.generation().clone()))
    }

    pub fn inspect_active_generation_stable(&self) -> Result<Option<GenerationId>, SetupError> {
        let journal = self.activation_journal_path();
        if journal.exists() {
            reject_reparse_ancestors(&journal)?;
            return Err(SetupError::Runtime(
                "a profile transaction is pending".to_owned(),
            ));
        }
        let pointer = self.read_optional_pointer(self.paths.active_profile())?;
        if journal.exists() {
            return Err(SetupError::Runtime(
                "a profile transaction is pending".to_owned(),
            ));
        }
        if let Some(pointer) = &pointer {
            self.verify_generation(pointer.generation())?;
        }
        Ok(pointer.map(|pointer| pointer.generation().clone()))
    }

    pub fn synchronize_active_runtime(&self) -> Result<GenerationId, SetupError> {
        let generation = self
            .active_generation()?
            .ok_or_else(|| SetupError::Runtime("no active machine profile exists".to_owned()))?;
        if !ProfileRuntimeBridge::new(self.paths.clone()).materialize_generation(&generation)? {
            return Err(SetupError::Runtime(
                "no active protected runtime is installed".to_owned(),
            ));
        }
        Ok(generation)
    }
}
