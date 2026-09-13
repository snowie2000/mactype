use std::fs;
use std::path::PathBuf;

use mactype_service_contract::GenerationPointer;

use super::{
    LegacyProfileActivationJournal, ProfileActivationJournal, ProfileActivationPhase, ProfileStore,
    LEGACY_PROFILE_ACTIVATION_SCHEMA, MAX_ACTIVATION_JOURNAL_BYTES, PROFILE_ACTIVATION_SCHEMA,
};
use crate::profile_bridge::{
    MaterializedProfileClearError, MaterializedProfileObservation, ProfileRuntimeBridge,
};
use crate::storage::{
    atomic_write, read_bounded_regular_file, reject_reparse_ancestors, SetupError,
};

impl ProfileStore {
    pub fn recover_interrupted_activation(&self) -> Result<bool, SetupError> {
        let journal_path = self.activation_journal_path();
        if !journal_path.exists() {
            return Ok(false);
        }
        let bytes = read_bounded_regular_file(
            &journal_path,
            MAX_ACTIVATION_JOURNAL_BYTES,
            "profile activation journal",
        )?;
        let journal = parse_activation_journal(&bytes)?;
        if let Some(active) = &journal.active_before {
            self.verify_generation(active.generation())?;
        }
        if let Some(previous) = &journal.previous_before {
            self.verify_generation(previous.generation())?;
        }

        let bridge = ProfileRuntimeBridge::new(self.paths.clone());
        match journal.phase {
            ProfileActivationPhase::FirstRollbackDeletePending => {
                let active_before = journal.active_before.as_ref().ok_or_else(|| {
                    SetupError::CleanupUnknown(
                        "uncertain first-profile rollback journal has no active generation"
                            .to_owned(),
                    )
                })?;
                let active_now = self.read_optional_pointer(self.paths.active_profile())?;
                let previous_now = self.read_optional_pointer(self.paths.previous_profile())?;
                if active_now.as_ref() != Some(active_before)
                    || previous_now.as_ref() != journal.previous_before.as_ref()
                {
                    return Err(SetupError::CleanupUnknown(
                        "profile pointers changed while first-profile deletion was uncertain"
                            .to_owned(),
                    ));
                }
                match bridge.observe_materialized_generation(active_before.generation())? {
                    MaterializedProfileObservation::ExactGeneration => {}
                    MaterializedProfileObservation::Absent => {
                        bridge
                            .restore_generation_after_confirmed_clear(active_before.generation())?;
                    }
                }
                self.remove_activation_journal()?;
                return Ok(true);
            }
            ProfileActivationPhase::PointerTransition => {
                let active_after = self.read_optional_pointer(self.paths.active_profile())?;
                if let Some(active_after) = &active_after {
                    self.verify_generation(active_after.generation())?;
                }
                if journal.active_before.is_none() {
                    match &active_after {
                        Some(active_after) => {
                            bridge
                                .clear_materialized_generation(active_after.generation())
                                .map_err(MaterializedProfileClearError::into_setup_error)?;
                        }
                        None => bridge.ensure_materialized_profile_absent()?,
                    }
                }
            }
            ProfileActivationPhase::FirstRollbackDeleteConfirmed => {}
        }

        self.restore_pointer(self.paths.active_profile(), journal.active_before.as_ref())?;
        self.restore_pointer(
            self.paths.previous_profile(),
            journal.previous_before.as_ref(),
        )?;
        if let Some(active) = journal.active_before {
            if matches!(
                journal.phase,
                ProfileActivationPhase::FirstRollbackDeleteConfirmed
            ) {
                bridge.restore_generation_after_confirmed_clear(active.generation())?;
            } else {
                bridge.materialize_generation(active.generation())?;
            }
        }
        self.remove_activation_journal()?;
        Ok(true)
    }

    pub(super) fn finish_activation_transaction(
        &self,
        transition: Result<(), SetupError>,
    ) -> Result<(), SetupError> {
        match transition {
            Ok(()) => self.remove_activation_journal(),
            Err(error) => match self.recover_interrupted_activation() {
                Ok(_) => Err(error),
                Err(recovery_error) => Err(SetupError::Runtime(format!(
                    "profile transition failed ({error}) and journal recovery failed ({recovery_error})"
                ))),
            },
        }
    }

    pub(super) fn activation_journal_path(&self) -> PathBuf {
        self.paths.profile_activation_journal().to_owned()
    }

    pub(super) fn write_activation_journal(
        &self,
        phase: ProfileActivationPhase,
        active_before: Option<GenerationPointer>,
        previous_before: Option<GenerationPointer>,
    ) -> Result<(), SetupError> {
        let bytes = serde_json::to_vec(&ProfileActivationJournal {
            schema: PROFILE_ACTIVATION_SCHEMA,
            phase,
            active_before,
            previous_before,
        })?;
        atomic_write(&self.activation_journal_path(), &bytes)
    }

    pub(super) fn remove_activation_journal(&self) -> Result<(), SetupError> {
        let path = self.activation_journal_path();
        if path.exists() {
            reject_reparse_ancestors(&path)?;
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

fn parse_activation_journal(bytes: &[u8]) -> Result<ProfileActivationJournal, SetupError> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|_| SetupError::Runtime("profile activation journal is invalid".to_owned()))?;
    let schema = value
        .get("schema")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| SetupError::Runtime("profile activation journal is invalid".to_owned()))?;
    match schema {
        matched_schema if matched_schema == u64::from(PROFILE_ACTIVATION_SCHEMA) => {
            serde_json::from_value(value).map_err(|_| {
                SetupError::Runtime("profile activation journal is invalid".to_owned())
            })
        }
        matched_schema if matched_schema == u64::from(LEGACY_PROFILE_ACTIVATION_SCHEMA) => {
            let legacy: LegacyProfileActivationJournal =
                serde_json::from_value(value).map_err(|_| {
                    SetupError::Runtime("profile activation journal is invalid".to_owned())
                })?;
            if legacy.schema != LEGACY_PROFILE_ACTIVATION_SCHEMA {
                return Err(SetupError::Runtime(
                    "profile activation journal has an unsupported schema".to_owned(),
                ));
            }
            Ok(ProfileActivationJournal {
                schema: PROFILE_ACTIVATION_SCHEMA,
                phase: ProfileActivationPhase::PointerTransition,
                active_before: legacy.active_before,
                previous_before: legacy.previous_before,
            })
        }
        _ => Err(SetupError::Runtime(
            "profile activation journal has an unsupported schema".to_owned(),
        )),
    }
}
