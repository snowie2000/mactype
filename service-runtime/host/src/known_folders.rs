use std::io;

use mactype_service_contract::MachinePaths;
use mactype_service_platform::{known_folder_path, KnownFolder};

pub fn machine_paths() -> io::Result<MachinePaths> {
    let program_files = known_folder_path(KnownFolder::ProgramFiles)?;
    let program_data = known_folder_path(KnownFolder::ProgramData)?;
    MachinePaths::from_trusted_os_roots(&program_files, &program_data)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
