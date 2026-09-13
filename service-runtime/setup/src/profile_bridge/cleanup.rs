use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use mactype_service_contract::{GenerationId, MAX_PROFILE_BYTES};

use super::{
    MaterializedProfileClearError, MaterializedProfileObservation, ProfileRuntimeBridge,
    GENERATED_PROFILE_NAME,
};
use crate::storage::{
    read_bounded_directory, read_bounded_regular_file, reject_reparse_ancestors, SetupError,
};

const GENERATED_PROFILE_TEMP_PREFIX: &str = ".MacType.ini.new-";

impl ProfileRuntimeBridge {
    pub(crate) fn clear_materialized_generation(
        &self,
        generation: &GenerationId,
    ) -> Result<bool, MaterializedProfileClearError> {
        let Some(runtime_root) = self
            .active_runtime_root()
            .map_err(MaterializedProfileClearError::Unchanged)?
        else {
            return Ok(false);
        };
        let path = runtime_root.join(GENERATED_PROFILE_NAME);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                return Err(MaterializedProfileClearError::Unchanged(
                    cleanup_unknown_io(error),
                ));
            }
            Ok(_) => {}
        }
        reject_reparse_ancestors(&path).map_err(MaterializedProfileClearError::Unchanged)?;
        let expected = self
            .verified_generation_bytes(generation)
            .map_err(MaterializedProfileClearError::Unchanged)?;
        remove_exact_materialized_profile(&path, &expected)?;
        Ok(true)
    }

    pub(crate) fn ensure_materialized_profile_absent(&self) -> Result<(), SetupError> {
        let Some(runtime_root) = self.active_runtime_root()? else {
            return Ok(());
        };
        let path = runtime_root.join(GENERATED_PROFILE_NAME);
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(SetupError::CleanupUnknown(format!(
                "generated runtime profile could not be inspected: {error}"
            ))),
            Ok(_) => Err(SetupError::CleanupUnknown(
                "generated runtime profile exists without a verified generation identity"
                    .to_owned(),
            )),
        }
    }

    pub(crate) fn observe_materialized_generation(
        &self,
        generation: &GenerationId,
    ) -> Result<MaterializedProfileObservation, SetupError> {
        let Some(runtime_root) = self.active_runtime_root()? else {
            return Ok(MaterializedProfileObservation::Absent);
        };
        let expected = self.verified_generation_bytes(generation)?;
        let path = runtime_root.join(GENERATED_PROFILE_NAME);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(MaterializedProfileObservation::Absent)
            }
            Err(error) => Err(SetupError::CleanupUnknown(format!(
                "uncertain profile cleanup could not inspect its destination: {error}"
            ))),
            Ok(metadata) if !metadata.file_type().is_file() => Err(SetupError::CleanupUnknown(
                "uncertain profile cleanup found a non-regular destination".to_owned(),
            )),
            Ok(_) => {
                reject_reparse_ancestors(&path).map_err(|error| {
                    SetupError::CleanupUnknown(format!(
                        "uncertain profile cleanup found an unsafe destination: {error}"
                    ))
                })?;
                let actual = read_bounded_regular_file(
                    &path,
                    MAX_PROFILE_BYTES as u64,
                    "uncertain generated runtime profile",
                )
                .map_err(|error| {
                    SetupError::CleanupUnknown(format!(
                        "uncertain profile cleanup could not verify its destination: {error}"
                    ))
                })?;
                if actual != expected {
                    return Err(SetupError::CleanupUnknown(
                        "uncertain profile cleanup found foreign destination bytes".to_owned(),
                    ));
                }
                Ok(MaterializedProfileObservation::ExactGeneration)
            }
        }
    }

    pub(crate) fn restore_generation_after_confirmed_clear(
        &self,
        generation: &GenerationId,
    ) -> Result<bool, SetupError> {
        let Some(runtime_root) = self.active_runtime_root()? else {
            return Ok(false);
        };
        let expected = self.verified_generation_bytes(generation)?;
        let path = runtime_root.join(GENERATED_PROFILE_NAME);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut file = OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                    .map_err(|error| {
                        SetupError::CleanupUnknown(format!(
                            "confirmed profile restoration could not exclusively create its destination: {error}"
                        ))
                    })?;
                file.write_all(&expected).map_err(|error| {
                    SetupError::CleanupUnknown(format!(
                        "confirmed profile restoration did not write exact bytes: {error}"
                    ))
                })?;
                file.sync_all().map_err(|error| {
                    SetupError::CleanupUnknown(format!(
                        "confirmed profile restoration could not persist exact bytes: {error}"
                    ))
                })?;
                Ok(true)
            }
            Err(error) => Err(SetupError::CleanupUnknown(format!(
                "confirmed profile restoration could not inspect its destination: {error}"
            ))),
            Ok(metadata) if !metadata.file_type().is_file() => Err(SetupError::CleanupUnknown(
                "confirmed profile restoration found a non-regular destination".to_owned(),
            )),
            Ok(_) => {
                reject_reparse_ancestors(&path).map_err(|error| {
                    SetupError::CleanupUnknown(format!(
                        "confirmed profile restoration found an unsafe destination: {error}"
                    ))
                })?;
                let actual = read_bounded_regular_file(
                    &path,
                    MAX_PROFILE_BYTES as u64,
                    "confirmed generated runtime profile",
                )
                .map_err(|error| {
                    SetupError::CleanupUnknown(format!(
                        "confirmed profile restoration could not verify its destination: {error}"
                    ))
                })?;
                if actual != expected {
                    return Err(SetupError::CleanupUnknown(
                        "confirmed profile restoration found foreign destination bytes".to_owned(),
                    ));
                }
                Ok(true)
            }
        }
    }
}

fn read_materialized_profile(file: &mut File) -> Result<Vec<u8>, SetupError> {
    let metadata = file.metadata().map_err(cleanup_unknown_io)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_PROFILE_BYTES as u64 {
        return Err(SetupError::CleanupUnknown(
            "generated runtime profile is not a bounded regular file".to_owned(),
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_PROFILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(cleanup_unknown_io)?;
    if bytes.is_empty() || bytes.len() > MAX_PROFILE_BYTES {
        return Err(SetupError::CleanupUnknown(
            "generated runtime profile changed while it was inspected".to_owned(),
        ));
    }
    Ok(bytes)
}

fn cleanup_unknown_io(error: std::io::Error) -> SetupError {
    SetupError::CleanupUnknown(format!(
        "generated runtime profile could not be exclusively inspected: {error}"
    ))
}

#[cfg(windows)]
fn remove_exact_materialized_profile(
    path: &Path,
    expected: &[u8],
) -> Result<(), MaterializedProfileClearError> {
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
        .map_err(cleanup_unknown_io)
        .map_err(MaterializedProfileClearError::Unchanged)?;
    let metadata = file
        .metadata()
        .map_err(cleanup_unknown_io)
        .map_err(MaterializedProfileClearError::Unchanged)?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(MaterializedProfileClearError::Unchanged(
            SetupError::CleanupUnknown(
                "generated runtime profile became a reparse point".to_owned(),
            ),
        ));
    }
    if read_materialized_profile(&mut file).map_err(MaterializedProfileClearError::Unchanged)?
        != expected
    {
        return Err(MaterializedProfileClearError::Unchanged(
            SetupError::CleanupUnknown(
                "generated runtime profile differs from the active generation".to_owned(),
            ),
        ));
    }
    mark_open_file_for_deletion(&file)
        .map_err(|error| MaterializedProfileClearError::Unknown(cleanup_unknown_io(error)))?;
    drop(file);
    Ok(())
}

#[cfg(not(windows))]
fn remove_exact_materialized_profile(
    path: &Path,
    expected: &[u8],
) -> Result<(), MaterializedProfileClearError> {
    let mut file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(cleanup_unknown_io)
        .map_err(MaterializedProfileClearError::Unchanged)?;
    if read_materialized_profile(&mut file).map_err(MaterializedProfileClearError::Unchanged)?
        != expected
    {
        return Err(MaterializedProfileClearError::Unchanged(
            SetupError::CleanupUnknown(
                "generated runtime profile differs from the active generation".to_owned(),
            ),
        ));
    }
    fs::remove_file(path)
        .map_err(cleanup_unknown_io)
        .map_err(MaterializedProfileClearError::Unknown)
}

pub(super) fn remove_interrupted_generated_profile_writes(
    runtime_root: &Path,
) -> Result<(), SetupError> {
    const MAX_RUNTIME_ENTRIES_DURING_PROFILE_RECOVERY: usize = 16;

    for entry in read_bounded_directory(
        runtime_root,
        MAX_RUNTIME_ENTRIES_DURING_PROFILE_RECOVERY,
        "runtime profile recovery entry count",
    )? {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(suffix) = name.strip_prefix(GENERATED_PROFILE_TEMP_PREFIX) else {
            continue;
        };
        let components = suffix.split('-').collect::<Vec<_>>();
        if !matches!(components.len(), 2 | 3)
            || components.iter().any(|component| {
                component.is_empty() || !component.bytes().all(|byte| byte.is_ascii_digit())
            })
        {
            return Err(SetupError::Runtime(
                "generated runtime profile temporary entry is not owned by setup".to_owned(),
            ));
        }
        let path = entry.path();
        reject_reparse_ancestors(&path)?;
        if !entry.metadata()?.is_file() {
            return Err(SetupError::Runtime(
                "generated runtime profile temporary path is not a regular file".to_owned(),
            ));
        }
        fs::remove_file(path)?;
    }
    Ok(())
}
