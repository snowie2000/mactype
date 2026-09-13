use std::fs;

use mactype_service_contract::{GenerationId, GenerationPointer};

use super::{ProfileStore, MAX_POINTER_BYTES};
use crate::storage::{
    atomic_write, read_bounded_regular_file, reject_reparse_ancestors, SetupError,
};

impl ProfileStore {
    pub(super) fn read_optional_pointer(
        &self,
        path: &std::path::Path,
    ) -> Result<Option<GenerationPointer>, SetupError> {
        if !path.exists() {
            return Ok(None);
        }
        let bytes = read_bounded_regular_file(path, MAX_POINTER_BYTES, "profile pointer")?;
        let pointer = serde_json::from_slice(&bytes).map_err(|_| SetupError::InvalidPointer)?;
        Ok(Some(pointer))
    }

    pub(super) fn write_pointer(
        &self,
        path: &std::path::Path,
        generation: &GenerationId,
    ) -> Result<(), SetupError> {
        let bytes = serde_json::to_vec(&GenerationPointer::new(generation.clone()))?;
        atomic_write(path, &bytes)
    }

    pub(super) fn restore_pointer(
        &self,
        path: &std::path::Path,
        pointer: Option<&GenerationPointer>,
    ) -> Result<(), SetupError> {
        match pointer {
            Some(pointer) => self.write_pointer(path, pointer.generation()),
            None if path.exists() => {
                reject_reparse_ancestors(path)?;
                fs::remove_file(path)?;
                Ok(())
            }
            None => Ok(()),
        }
    }
}
