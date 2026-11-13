use crate::bounded_io::read_open_file_bounded;
use std::{fs, path::Path};

pub(super) fn read_bounded_regular_file(
    path: &Path,
    maximum_bytes: u64,
    description: &str,
) -> Result<Vec<u8>, String> {
    reject_reparse_chain(path)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if metadata_is_reparse(&metadata) {
        return Err(format!("{description} is not a bounded regular file"));
    }
    let maximum_bytes = usize::try_from(maximum_bytes)
        .map_err(|_| format!("{description} has an unsupported byte limit"))?;
    read_open_file_bounded(file, maximum_bytes, false, description)
}

pub(super) fn reject_reparse_chain(path: &Path) -> Result<(), String> {
    mactype_service_platform::validate_path_chain(path, None).map_err(|error| match error {
        mactype_service_platform::PathChainError::ReparsePoint(_) => {
            "reparse points are forbidden in the fixed file path".to_owned()
        }
        mactype_service_platform::PathChainError::Io { source, .. } => source.to_string(),
        error => error.to_string(),
    })
}

#[cfg(windows)]
fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse(_metadata: &fs::Metadata) -> bool {
    false
}
