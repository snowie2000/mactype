use super::{super::model::*, file_io::*};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(windows)]
fn known_folder(folder: mactype_service_platform::KnownFolder) -> Result<PathBuf, String> {
    let value = mactype_service_platform::known_folder_path(folder).map_err(|error| {
        let result = error.raw_os_error().unwrap_or(0);
        format!("SHGetKnownFolderPath failed with HRESULT {result}")
    })?;
    fs::canonicalize(value).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn program_data_root() -> Result<PathBuf, String> {
    known_folder(mactype_service_platform::KnownFolder::ProgramData)
}

#[cfg(not(windows))]
fn program_data_root() -> Result<PathBuf, String> {
    Err("legacy SCM migration is available only on Windows".to_owned())
}

#[cfg(windows)]
pub(in crate::machine_integration::legacy_migration) fn expected_installation_root(
) -> Result<PathBuf, String> {
    let program_files = known_folder(mactype_service_platform::KnownFolder::ProgramFiles)?;
    Ok(program_files.join("MacType"))
}

#[cfg(not(windows))]
pub(in crate::machine_integration::legacy_migration) fn expected_installation_root(
) -> Result<PathBuf, String> {
    Err("legacy SCM migration is available only on Windows".to_owned())
}

pub(in crate::machine_integration::legacy_migration) fn secure_create_tree(
    root: &Path,
    components: &[&str],
) -> Result<PathBuf, String> {
    if path_is_reparse(root)? {
        return Err("legacy migration storage root is a reparse point".to_owned());
    }
    let mut path = root.to_path_buf();
    for component in components {
        path.push(component);
        if path.exists() {
            if !path.is_dir() || path_is_reparse(&path)? {
                return Err(format!("unsafe migration directory {}", path.display()));
            }
        } else {
            fs::create_dir(&path).map_err(|error| error.to_string())?;
            if path_is_reparse(&path)? {
                return Err(format!("unsafe migration directory {}", path.display()));
            }
        }
    }
    Ok(path)
}

pub(in crate::machine_integration::legacy_migration) fn acl_invocation(
    system_directory: &Path,
    target: &Path,
) -> (PathBuf, Vec<OsString>) {
    (
        system_directory.join("icacls.exe"),
        [
            target.as_os_str().to_owned(),
            OsString::from("/inheritance:r"),
            OsString::from("/grant:r"),
            OsString::from("*S-1-5-18:(OI)(CI)F"),
            OsString::from("*S-1-5-32-544:(OI)(CI)F"),
            OsString::from("*S-1-5-32-545:(OI)(CI)RX"),
        ]
        .into_iter()
        .collect(),
    )
}

pub(in crate::machine_integration::legacy_migration) fn registry_export_invocation(
    system_directory: &Path,
    generation_root: &Path,
) -> (PathBuf, Vec<OsString>) {
    let export_path = generation_root.join(SERVICE_REGISTRY_EXPORT);
    (
        system_directory.join("reg.exe"),
        vec![
            OsString::from("export"),
            OsString::from(SERVICE_REGISTRY_KEY),
            export_path.into_os_string(),
            OsString::from("/y"),
        ],
    )
}

#[cfg(windows)]
fn system_directory() -> Result<PathBuf, String> {
    mactype_service_platform::system_directory().map_err(|error| error.to_string())
}

#[cfg(windows)]
pub(in crate::machine_integration::legacy_migration) fn harden_machine_directory(
    path: &Path,
) -> Result<(), String> {
    let (program, arguments) = acl_invocation(&system_directory()?, path);
    let status = Command::new(program)
        .args(arguments)
        .status()
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("could not apply the protected legacy migration ACL".to_owned())
    }
}

#[cfg(not(windows))]
pub(in crate::machine_integration::legacy_migration) fn harden_machine_directory(
    _path: &Path,
) -> Result<(), String> {
    Err("legacy SCM migration is available only on Windows".to_owned())
}

pub(in crate::machine_integration::legacy_migration) fn after_hardening_with<T>(
    path: &Path,
    harden: impl FnOnce(&Path) -> Result<(), String>,
    continuation: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    harden(path)?;
    continuation(path)
}

pub(in crate::machine_integration::legacy_migration) fn after_registry_export_with<T>(
    generation_root: &Path,
    export: impl FnOnce(&Path) -> Result<RegistryExportReceipt, String>,
    continuation: impl FnOnce(&Path, RegistryExportReceipt) -> Result<T, String>,
) -> Result<T, String> {
    let receipt = export(generation_root)?;
    continuation(generation_root, receipt)
}

#[cfg(windows)]
pub(in crate::machine_integration::legacy_migration) fn export_service_registry(
    generation_root: &Path,
) -> Result<RegistryExportReceipt, String> {
    let export_path = generation_root.join(SERVICE_REGISTRY_EXPORT);
    if export_path.exists() || path_is_reparse(&export_path)? {
        return Err("legacy service registry export target already exists".to_owned());
    }
    let (program, arguments) = registry_export_invocation(&system_directory()?, generation_root);
    let status = Command::new(program)
        .args(arguments)
        .status()
        .map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!(
            "fixed reg.exe service export failed with exit code {:?}",
            status.code()
        ));
    }
    let bytes =
        read_regular_bounded_under(generation_root, &export_path, MAX_REGISTRY_EXPORT_BYTES)?;
    let receipt = RegistryExportReceipt {
        export_file: SERVICE_REGISTRY_EXPORT.to_owned(),
        byte_length: bytes.len() as u64,
        sha256: hex_sha256(&bytes),
    };
    validate_registry_export_bytes(&receipt, &bytes)?;
    Ok(receipt)
}

#[cfg(not(windows))]
pub(in crate::machine_integration::legacy_migration) fn export_service_registry(
    _generation_root: &Path,
) -> Result<RegistryExportReceipt, String> {
    Err("legacy SCM migration is available only on Windows".to_owned())
}

pub(in crate::machine_integration::legacy_migration) fn migration_storage_root(
) -> Result<PathBuf, String> {
    let program_data = program_data_root()?;
    let storage = program_data
        .join("MacType")
        .join("ControlCenter")
        .join("legacy-migration");
    if !storage.is_dir() {
        return Err("legacy migration storage does not exist".to_owned());
    }
    validate_existing_path(&program_data, &storage)?;
    Ok(storage)
}

pub(in crate::machine_integration::legacy_migration) fn create_migration_storage_root(
) -> Result<PathBuf, String> {
    let program_data = program_data_root()?;
    let storage = secure_create_tree(
        &program_data,
        &["MacType", "ControlCenter", "legacy-migration"],
    )?;
    harden_machine_directory(&storage)?;
    Ok(storage)
}
