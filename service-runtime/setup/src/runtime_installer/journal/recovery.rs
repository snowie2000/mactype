use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::Path;

use mactype_service_contract::{
    parse_runtime_activation_receipt, ParsedRuntimeActivationReceipt, RuntimeActivationPhase,
};

use super::{
    validate_runtime_pointer, RuntimePointer, MAX_ACTIVATION_JOURNAL_BYTES, MAX_POINTER_BYTES,
};
use crate::profile_bridge::ProfileRuntimeBridge;
use crate::runtime_installer::{InstalledRuntime, RuntimeInstaller, RuntimeServiceBinding};
use crate::storage::{atomic_write, read_bounded_regular_file, SetupError};

impl RuntimeInstaller {
    pub fn recover_interrupted_activation(&self) -> Result<Option<InstalledRuntime>, SetupError> {
        self.recover_interrupted_repair()?;
        let journal_path = self.activation_journal_path();
        if !journal_path.exists() {
            return self.current();
        }
        let bytes = read_bounded_regular_file(
            &journal_path,
            MAX_ACTIVATION_JOURNAL_BYTES,
            "runtime activation journal",
        )?;
        let journal = parse_runtime_activation_receipt(&bytes)
            .map_err(|_| SetupError::Runtime("runtime activation journal is invalid".to_owned()))?;
        if journal.phase() == Some(RuntimeActivationPhase::Committed) {
            self.finish_committed_activation_recovery(&journal)?;
        } else {
            self.restore_runtime_pointer(journal.previous(), journal.activated())?;
            if journal.previous().is_some() {
                ProfileRuntimeBridge::new(self.paths.clone()).materialize_active()?;
            }
        }
        let recovered = self.current()?;
        self.remove_activation_journal()?;
        Ok(recovered)
    }

    pub fn recover_interrupted_activation_with_service_binding<I, R>(
        &self,
        mut inspect_service_binding: I,
        restore_previous_service_binding: R,
    ) -> Result<Option<InstalledRuntime>, SetupError>
    where
        I: FnMut(Option<&Path>, Option<&Path>) -> Result<RuntimeServiceBinding, SetupError>,
        R: FnOnce(&Path, Option<&Path>) -> Result<(), SetupError>,
    {
        self.recover_interrupted_repair()?;
        let journal_path = self.activation_journal_path();
        if !journal_path.exists() {
            return self.current();
        }
        let bytes = read_bounded_regular_file(
            &journal_path,
            MAX_ACTIVATION_JOURNAL_BYTES,
            "runtime activation journal",
        )?;
        let journal = parse_runtime_activation_receipt(&bytes)
            .map_err(|_| SetupError::Runtime("runtime activation journal is invalid".to_owned()))?;
        let derived_activated;
        let activated = match journal.activated() {
            Some(activated) => Some(activated),
            None => {
                derived_activated = self.derive_legacy_candidate(&journal)?;
                derived_activated.as_ref()
            }
        };
        let candidate_binary = activated
            .map(|activated| self.verified_activation_service_binary(activated, "candidate"))
            .transpose()?;
        let previous_binary = journal
            .previous()
            .map(|previous| self.verified_activation_service_binary(previous, "previous"))
            .transpose()?;
        let mut binding =
            inspect_service_binding(candidate_binary.as_deref(), previous_binary.as_deref())
                .map_err(|error| {
                    SetupError::CleanupUnknown(format!(
                        "SCM binding could not be classified during runtime recovery: {error}"
                    ))
                })?;

        match journal.phase() {
            Some(RuntimeActivationPhase::Committed) => {
                let activated = activated.ok_or_else(|| {
                    SetupError::CleanupUnknown(
                        "committed runtime activation has no candidate pointer".to_owned(),
                    )
                })?;
                self.require_activation_pointer(activated, "committed candidate")?;
                if binding != RuntimeServiceBinding::Candidate {
                    return Err(SetupError::CleanupUnknown(
                        "committed runtime activation is not bound to the candidate SCM image"
                            .to_owned(),
                    ));
                }
                self.finish_committed_activation_recovery(&journal)?;
            }
            Some(RuntimeActivationPhase::Candidate)
            | Some(RuntimeActivationPhase::RollbackRequired)
            | None => {
                if binding == RuntimeServiceBinding::Candidate
                    && candidate_binary.is_some()
                    && candidate_binary.as_deref() == previous_binary.as_deref()
                {
                    binding = RuntimeServiceBinding::Previous;
                }
                if binding == RuntimeServiceBinding::Candidate {
                    let candidate_binary = candidate_binary.as_deref().ok_or_else(|| {
                        SetupError::CleanupUnknown(
                            "uncommitted runtime activation has a candidate service binding but no candidate pointer"
                                .to_owned(),
                        )
                    })?;
                    restore_previous_service_binding(candidate_binary, previous_binary.as_deref())
                        .map_err(|error| {
                            SetupError::CleanupUnknown(format!(
                                "SCM rollback did not complete before pointer recovery: {error}"
                            ))
                        })?;
                    binding =
                        inspect_service_binding(Some(candidate_binary), previous_binary.as_deref())
                            .map_err(|error| {
                                SetupError::CleanupUnknown(format!(
                                    "SCM rollback result could not be verified: {error}"
                                ))
                            })?;
                }
                let previous_binding_is_exact = matches!(
                    (binding, previous_binary.as_ref()),
                    (RuntimeServiceBinding::Previous, Some(_))
                        | (RuntimeServiceBinding::Absent, None)
                );
                if !previous_binding_is_exact {
                    return Err(SetupError::CleanupUnknown(
                        "uncommitted runtime activation did not reach the exact previous SCM binding"
                            .to_owned(),
                    ));
                }
                self.rollback_activation_pointer_and_profile(journal.previous(), activated)?;
            }
        }
        let recovered = self.current()?;
        self.remove_activation_journal()?;
        Ok(recovered)
    }

    fn verified_activation_service_binary(
        &self,
        pointer: &RuntimePointer,
        role: &str,
    ) -> Result<std::path::PathBuf, SetupError> {
        let directory = self.paths.runtime_versions().join(pointer.version());
        self.verify_runtime_generation_receipt(pointer.version(), &directory)
            .map_err(|error| {
                SetupError::CleanupUnknown(format!(
                    "runtime activation {role} generation could not be verified for SCM recovery: {error}"
                ))
            })?;
        Ok(directory.join("mactype-service.exe"))
    }

    fn derive_legacy_candidate(
        &self,
        journal: &ParsedRuntimeActivationReceipt,
    ) -> Result<Option<RuntimePointer>, SetupError> {
        let active = self.current_pointer_for_committed_recovery()?;
        match (active, journal.previous()) {
            (Some(active), Some(previous)) if &active == previous => Ok(None),
            (Some(active), _) => Ok(Some(active)),
            (None, None) => Ok(None),
            (None, Some(_)) => Err(SetupError::CleanupUnknown(
                "legacy runtime activation lost its active pointer after a possible switch"
                    .to_owned(),
            )),
        }
    }

    fn require_activation_pointer(
        &self,
        expected: &RuntimePointer,
        role: &str,
    ) -> Result<(), SetupError> {
        let actual = self.current_pointer_for_committed_recovery()?;
        if actual.as_ref() == Some(expected) {
            return Ok(());
        }
        Err(SetupError::CleanupUnknown(format!(
            "runtime activation pointer does not match the {role}"
        )))
    }

    fn rollback_activation_pointer_and_profile(
        &self,
        previous: Option<&RuntimePointer>,
        activated: Option<&RuntimePointer>,
    ) -> Result<(), SetupError> {
        self.restore_runtime_pointer(previous, activated)?;
        if previous.is_some() {
            ProfileRuntimeBridge::new(self.paths.clone()).materialize_active()?;
        }
        Ok(())
    }

    fn finish_committed_activation_recovery(
        &self,
        journal: &ParsedRuntimeActivationReceipt,
    ) -> Result<(), SetupError> {
        let activated = journal.activated().ok_or_else(|| {
            SetupError::CleanupUnknown(
                "committed runtime activation receipt has no candidate pointer".to_owned(),
            )
        })?;
        let actual = self.current_pointer_for_committed_recovery()?;
        if actual.as_ref() == Some(activated) {
            self.verify_and_materialize_committed_runtime("candidate")?;
            return Ok(());
        }
        if actual.as_ref() == journal.previous() {
            if journal.previous().is_some() {
                self.verify_and_materialize_committed_runtime("previous generation")?;
            }
            return Ok(());
        }
        Err(SetupError::CleanupUnknown(
            "active runtime pointer matches neither side of the committed activation receipt"
                .to_owned(),
        ))
    }

    fn verify_and_materialize_committed_runtime(&self, role: &str) -> Result<(), SetupError> {
        let current = self.current()?.ok_or_else(|| {
            SetupError::CleanupUnknown(format!(
                "committed runtime activation {role} disappeared during recovery"
            ))
        })?;
        let directory = current.service_binary().parent().ok_or_else(|| {
            SetupError::CleanupUnknown(format!(
                "committed runtime activation {role} has no generation directory"
            ))
        })?;
        self.verify_runtime_generation_receipt(current.version(), directory)
            .map_err(|error| {
                SetupError::CleanupUnknown(format!(
                    "committed runtime activation {role} could not be verified: {error}"
                ))
            })?;
        ProfileRuntimeBridge::new(self.paths.clone()).materialize_active()?;
        Ok(())
    }

    fn current_pointer_for_committed_recovery(&self) -> Result<Option<RuntimePointer>, SetupError> {
        let path = self.paths.runtime_pointer();
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(SetupError::CleanupUnknown(format!(
                    "active runtime pointer could not be inspected during committed recovery: {error}"
                )));
            }
        };
        if !metadata.file_type().is_file() {
            return Err(SetupError::CleanupUnknown(
                "active runtime pointer became non-regular during committed recovery".to_owned(),
            ));
        }
        let bytes = read_bounded_regular_file(path, MAX_POINTER_BYTES, "active runtime pointer")
            .map_err(|error| {
                SetupError::CleanupUnknown(format!(
                    "active runtime pointer could not be read during committed recovery: {error}"
                ))
            })?;
        validate_runtime_pointer(&bytes).map(Some).map_err(|error| {
            SetupError::CleanupUnknown(format!(
                "active runtime pointer is invalid during committed recovery: {error}"
            ))
        })
    }

    pub(in crate::runtime_installer) fn restore_runtime_pointer(
        &self,
        previous: Option<&RuntimePointer>,
        activated: Option<&RuntimePointer>,
    ) -> Result<(), SetupError> {
        let path = self.paths.runtime_pointer();
        let actual = match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(SetupError::CleanupUnknown(format!(
                    "active runtime pointer could not be inspected during rollback: {error}"
                )));
            }
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err(SetupError::CleanupUnknown(
                    "active runtime pointer became a non-regular path during rollback".to_owned(),
                ));
            }
            Ok(_) => Some(
                read_bounded_regular_file(path, MAX_POINTER_BYTES, "active runtime pointer")
                    .map_err(|error| {
                        SetupError::CleanupUnknown(format!(
                            "active runtime pointer could not be verified during rollback: {error}"
                        ))
                    })?,
            ),
        };
        let previous_bytes = previous.map(pointer_bytes).transpose()?;
        let activated_bytes = activated.map(pointer_bytes).transpose()?;

        match (actual, previous_bytes, activated_bytes) {
            (None, Some(previous), _) => atomic_write(path, &previous),
            (None, None, _) => Ok(()),
            (Some(actual), Some(previous), _) if actual == previous => Ok(()),
            (Some(actual), Some(previous), Some(activated)) if actual == activated => {
                atomic_write(path, &previous)
            }
            (Some(actual), None, Some(activated)) if actual == activated => {
                remove_exact_runtime_pointer(path, &activated)
            }
            _ => Err(SetupError::CleanupUnknown(
                "active runtime pointer no longer matches the activation transaction receipt"
                    .to_owned(),
            )),
        }
    }
}

fn pointer_bytes(pointer: &RuntimePointer) -> Result<Vec<u8>, SetupError> {
    pointer
        .to_bytes()
        .map_err(|_| SetupError::Runtime("active runtime pointer is invalid".to_owned()))
}

#[cfg(windows)]
fn remove_exact_runtime_pointer(path: &Path, expected: &[u8]) -> Result<(), SetupError> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

    use mactype_service_platform::mark_open_file_for_deletion;
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
        FILE_SHARE_READ,
    };

    let mut file = OpenOptions::new()
        .access_mode(FILE_GENERIC_READ | DELETE)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|error| {
            SetupError::CleanupUnknown(format!(
                "owned runtime pointer could not be opened for exact deletion: {error}"
            ))
        })?;
    let metadata = file.metadata().map_err(|error| {
        SetupError::CleanupUnknown(format!(
            "owned runtime pointer metadata could not be verified: {error}"
        ))
    })?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_POINTER_BYTES
    {
        return Err(SetupError::CleanupUnknown(
            "owned runtime pointer became non-regular, reparse, empty, or oversized".to_owned(),
        ));
    }
    let mut actual = Vec::with_capacity(metadata.len() as usize);
    (&mut file)
        .take(MAX_POINTER_BYTES + 1)
        .read_to_end(&mut actual)
        .map_err(|error| {
            SetupError::CleanupUnknown(format!(
                "owned runtime pointer bytes could not be verified: {error}"
            ))
        })?;
    if actual != expected {
        return Err(SetupError::CleanupUnknown(
            "owned runtime pointer changed before exact deletion".to_owned(),
        ));
    }
    mark_open_file_for_deletion(&file).map_err(|error| {
        SetupError::CleanupUnknown(format!(
            "owned runtime pointer exact deletion result is unknown: {error}"
        ))
    })?;
    drop(file);
    Ok(())
}

#[cfg(not(windows))]
fn remove_exact_runtime_pointer(path: &Path, expected: &[u8]) -> Result<(), SetupError> {
    let actual = read_bounded_regular_file(path, MAX_POINTER_BYTES, "owned runtime pointer")
        .map_err(|error| {
            SetupError::CleanupUnknown(format!(
                "owned runtime pointer could not be verified before deletion: {error}"
            ))
        })?;
    if actual != expected {
        return Err(SetupError::CleanupUnknown(
            "owned runtime pointer changed before exact deletion".to_owned(),
        ));
    }
    fs::remove_file(path).map_err(|error| {
        SetupError::CleanupUnknown(format!(
            "owned runtime pointer exact deletion result is unknown: {error}"
        ))
    })
}
