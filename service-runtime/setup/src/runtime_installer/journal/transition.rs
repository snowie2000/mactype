use std::fs;
use std::path::PathBuf;

use mactype_service_contract::{RuntimeActivationPhase, RuntimeActivationReceipt};

use super::{activation_receipt_bytes, validate_runtime_pointer, RuntimePointer};
use crate::runtime_installer::RuntimeInstaller;
use crate::storage::{
    atomic_write, read_bounded_regular_file, reject_reparse_ancestors, SetupError,
};

impl RuntimeInstaller {
    pub(in crate::runtime_installer) fn write_activation_journal(
        &self,
        previous: Option<RuntimePointer>,
        activated: RuntimePointer,
    ) -> Result<(), SetupError> {
        let bytes =
            activation_receipt_bytes(&RuntimeActivationReceipt::candidate(previous, activated))?;
        atomic_write(&self.activation_journal_path(), &bytes)
    }

    pub(in crate::runtime_installer) fn commit_activation_journal(
        &self,
        previous: Option<RuntimePointer>,
        activated: RuntimePointer,
    ) -> Result<(), SetupError> {
        let receipt = RuntimeActivationReceipt::candidate(previous, activated.clone());
        let candidate = activation_receipt_bytes(&receipt)?;
        self.verify_exact_activation_receipt(&candidate, RuntimeActivationPhase::Candidate)?;
        let pointer = read_bounded_regular_file(
            self.paths.runtime_pointer(),
            super::MAX_POINTER_BYTES,
            "active runtime pointer",
        )?;
        if validate_runtime_pointer(&pointer)? != activated {
            return Err(SetupError::CleanupUnknown(
                "active runtime pointer changed before the candidate phase could commit".to_owned(),
            ));
        }
        self.persist_activation_phase(&receipt, RuntimeActivationPhase::Committed)
    }

    pub(in crate::runtime_installer) fn require_activation_rollback(
        &self,
        previous: Option<RuntimePointer>,
        activated: RuntimePointer,
    ) -> Result<(), SetupError> {
        let receipt = RuntimeActivationReceipt::candidate(previous, activated);
        let candidate = activation_receipt_bytes(&receipt)?;
        let committed =
            activation_receipt_bytes(&receipt.with_phase(RuntimeActivationPhase::Committed))?;
        let rollback = activation_receipt_bytes(
            &receipt.with_phase(RuntimeActivationPhase::RollbackRequired),
        )?;
        let actual = read_bounded_regular_file(
            &self.activation_journal_path(),
            super::MAX_ACTIVATION_JOURNAL_BYTES,
            "runtime activation journal",
        )?;
        if actual == rollback {
            return Ok(());
        }
        if actual != candidate && actual != committed {
            return Err(SetupError::CleanupUnknown(
                "runtime activation receipt changed before rollback could be required".to_owned(),
            ));
        }
        self.persist_activation_phase(&receipt, RuntimeActivationPhase::RollbackRequired)
    }

    pub(in crate::runtime_installer) fn finalize_committed_activation(
        &self,
        previous: Option<RuntimePointer>,
        activated: RuntimePointer,
    ) -> Result<bool, SetupError> {
        let committed = activation_receipt_bytes(
            &RuntimeActivationReceipt::candidate(previous, activated)
                .with_phase(RuntimeActivationPhase::Committed),
        )?;
        self.verify_exact_activation_receipt(&committed, RuntimeActivationPhase::Committed)?;
        match self.remove_activation_journal() {
            Ok(()) => Ok(true),
            Err(removal) => {
                self.verify_exact_activation_receipt(
                    &committed,
                    RuntimeActivationPhase::Committed,
                )
                .map_err(|verification| {
                    SetupError::CleanupUnknown(format!(
                        "Ready activation receipt cleanup failed ({removal}) and the committed receipt could not be reverified ({verification})"
                    ))
                })?;
                eprintln!(
                    "Ready activation receipt cleanup deferred while its exact committed bytes remain: {removal}"
                );
                Ok(false)
            }
        }
    }

    fn verify_exact_activation_receipt(
        &self,
        expected: &[u8],
        phase: RuntimeActivationPhase,
    ) -> Result<(), SetupError> {
        let actual = read_bounded_regular_file(
            &self.activation_journal_path(),
            super::MAX_ACTIVATION_JOURNAL_BYTES,
            "runtime activation journal",
        )?;
        if actual != expected {
            return Err(SetupError::CleanupUnknown(format!(
                "runtime activation receipt changed before the {phase:?} phase could transition"
            )));
        }
        Ok(())
    }

    fn persist_activation_phase(
        &self,
        receipt: &RuntimeActivationReceipt,
        phase: RuntimeActivationPhase,
    ) -> Result<(), SetupError> {
        let transitioned = activation_receipt_bytes(&receipt.with_phase(phase))?;
        atomic_write(&self.activation_journal_path(), &transitioned)?;
        let persisted = read_bounded_regular_file(
            &self.activation_journal_path(),
            super::MAX_ACTIVATION_JOURNAL_BYTES,
            "runtime activation journal",
        )?;
        if persisted != transitioned {
            return Err(SetupError::CleanupUnknown(format!(
                "the {phase:?} runtime activation receipt could not be verified"
            )));
        }
        Ok(())
    }

    pub(in crate::runtime_installer) fn remove_activation_journal(&self) -> Result<(), SetupError> {
        let path = self.activation_journal_path();
        if path.exists() {
            reject_reparse_ancestors(&path)?;
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub(in crate::runtime_installer) fn activation_journal_path(&self) -> PathBuf {
        self.paths.runtime_activation_journal().to_owned()
    }
}
