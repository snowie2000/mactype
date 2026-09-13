use std::path::Path;

pub(in crate::machine_integration::open_service) fn reject_reparse_ancestors(
    path: &Path,
) -> Result<(), String> {
    for ancestor in path.ancestors().filter(|candidate| candidate.exists()) {
        if mactype_service_platform::is_reparse_point(ancestor)
            .map_err(|error| error.to_string())?
        {
            return Err("reparse points are forbidden in the broker staging path".to_owned());
        }
    }
    Ok(())
}
