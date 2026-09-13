use mactype_service_contract::MachinePaths;
use mactype_service_platform::{known_folder_path, KnownFolder};

use crate::SetupError;

pub fn machine_paths() -> Result<MachinePaths, SetupError> {
    let program_files = known_folder_path(KnownFolder::ProgramFiles).map_err(SetupError::Io)?;
    let program_data = known_folder_path(KnownFolder::ProgramData).map_err(SetupError::Io)?;
    MachinePaths::from_trusted_os_roots(&program_files, &program_data)
        .map_err(|error| SetupError::Runtime(error.to_string()))
}
