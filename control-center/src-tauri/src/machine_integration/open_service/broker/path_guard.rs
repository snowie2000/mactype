use std::path::Path;

pub(in crate::machine_integration::open_service) fn reject_reparse_ancestors(
    path: &Path,
) -> Result<(), String> {
    mactype_service_platform::validate_path_chain(path, None).map_err(|error| match error {
        mactype_service_platform::PathChainError::ReparsePoint(_) => {
            "reparse points are forbidden in the broker staging path".to_owned()
        }
        mactype_service_platform::PathChainError::Io { source, .. } => source.to_string(),
        error => error.to_string(),
    })
}
