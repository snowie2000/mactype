use super::{ProfileDocument, ProfileSnapshot, ProfileState};
use std::path::PathBuf;

impl ProfileState {
    pub(crate) fn active_payload(&self) -> Result<(PathBuf, Vec<u8>), String> {
        self.read(|document| {
            if document.is_dirty() {
                return Err("save profile changes before applying them to MacType".to_owned());
            }
            Ok((document.path().to_path_buf(), document.encoded()?))
        })
    }

    pub(super) fn set(&self, document: ProfileDocument) -> Result<(), String> {
        *self
            .0
            .lock()
            .map_err(|_| "profile lock is poisoned".to_owned())? = Some(document);
        Ok(())
    }

    pub(super) fn read<T>(
        &self,
        operation: impl FnOnce(&ProfileDocument) -> Result<T, String>,
    ) -> Result<T, String> {
        let guard = self
            .0
            .lock()
            .map_err(|_| "profile lock is poisoned".to_owned())?;
        let document = guard
            .as_ref()
            .ok_or_else(|| "no profile is open".to_owned())?;
        operation(document)
    }

    pub(super) fn snapshot(&self) -> Result<Option<ProfileSnapshot>, String> {
        let guard = self
            .0
            .lock()
            .map_err(|_| "profile lock is poisoned".to_owned())?;
        Ok(guard.as_ref().map(ProfileDocument::snapshot))
    }

    pub(super) fn edit<T>(
        &self,
        operation: impl FnOnce(&mut ProfileDocument) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = self
            .0
            .lock()
            .map_err(|_| "profile lock is poisoned".to_owned())?;
        let document = guard
            .as_mut()
            .ok_or_else(|| "no profile is open".to_owned())?;
        operation(document)
    }

    pub(super) fn replace_from<T>(
        &self,
        operation: impl FnOnce(&ProfileDocument) -> Result<(ProfileDocument, T), String>,
    ) -> Result<T, String> {
        let mut guard = self
            .0
            .lock()
            .map_err(|_| "profile lock is poisoned".to_owned())?;
        let current = guard
            .as_ref()
            .ok_or_else(|| "no profile is open".to_owned())?;
        let (replacement, result) = operation(current)?;
        *guard = Some(replacement);
        Ok(result)
    }
}
