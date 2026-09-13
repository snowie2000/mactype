use super::{
    super::{
        file_guard::read_bounded_regular_file,
        runtime::{parse_bundled_runtime_manifest, MAX_BUNDLED_MANIFEST_BYTES},
        INSTALLATION_INCOMPLETE_PREFIX, INSTALLATION_REQUIRED_PREFIX,
        INSTALLATION_UNTRUSTED_PREFIX,
    },
    path_guard::reject_reparse_ancestors,
};
use crate::diagnostics::InstallationPreflightDiagnostics;
use mactype_service_platform::{
    known_folder_path, KnownFolder, RegistryKey, RegistryRoot, RegistryView,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const CONTROL_CENTER_FILE: &str = "MacType Control Center.exe";
const SETUP_BROKER_RELATIVE_PATH: &str = r"service-runtime\mactype-service-setup.exe";
const RUNTIME_MANIFEST_RELATIVE_PATH: &str = r"service-runtime\payload\manifest.json";
const INSTALL_RECEIPT_REGISTRY_KEY: &str = r"SOFTWARE\MacType\ControlCenter";
const UNINSTALL_REGISTRY_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{AF6B9697-3DF2-46C4-B203-79194967AE7A}_is1";
const INSTALL_LOCATION_VALUE: &str = "InstallLocation";
const MAX_INSTALL_LOCATION_BYTES: u32 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServicePackageSource {
    Installed,
    CurrentBundle,
}

#[derive(Clone, Debug)]
pub(in crate::machine_integration::open_service) struct ServicePackage {
    pub(in crate::machine_integration::open_service) control_center: PathBuf,
    pub(in crate::machine_integration::open_service) setup_broker: PathBuf,
}

#[derive(Clone, Debug)]
pub(in crate::machine_integration::open_service) struct InstallationPreflightFailure {
    pub(in crate::machine_integration::open_service) error: String,
    pub(in crate::machine_integration::open_service) diagnostics:
        Box<InstallationPreflightDiagnostics>,
}

fn path_is_descendant_of(path: &Path, root: &Path) -> bool {
    let path = path.components().collect::<Vec<_>>();
    let root = root.components().collect::<Vec<_>>();
    path.len() > root.len()
        && path.iter().zip(&root).all(|(candidate, expected)| {
            candidate
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&expected.as_os_str().to_string_lossy())
        })
}

fn diagnostics(current_executable: Option<PathBuf>) -> InstallationPreflightDiagnostics {
    InstallationPreflightDiagnostics {
        expected_installed_control_center: None,
        current_executable: current_executable.map(|path| path.to_string_lossy().into_owned()),
        expected_executable_exists: None,
        installed_control_center: "not-checked".to_owned(),
        current_bundle: "not-checked".to_owned(),
        selected_service_package: "none".to_owned(),
        setup_broker: "not-checked".to_owned(),
        runtime_manifest: "not-checked".to_owned(),
        runtime_payload: "not-checked".to_owned(),
        elevation_attempted: false,
        elevated_revalidation: "not-attempted".to_owned(),
        machine_state_changed: false,
        rollback_required: false,
    }
}

fn failure(
    prefix: &str,
    message: &str,
    diagnostics: InstallationPreflightDiagnostics,
) -> InstallationPreflightFailure {
    InstallationPreflightFailure {
        error: format!("{prefix} {message}"),
        diagnostics: Box::new(diagnostics),
    }
}

fn required(diagnostics: InstallationPreflightDiagnostics) -> InstallationPreflightFailure {
    failure(
        INSTALLATION_REQUIRED_PREFIX,
        "No complete Control Center service package is available. Keep the complete \
         Integration/Developer bundle together or run the installer.",
        diagnostics,
    )
}

fn incomplete(diagnostics: InstallationPreflightDiagnostics) -> InstallationPreflightFailure {
    failure(
        INSTALLATION_INCOMPLETE_PREFIX,
        "The Control Center service package is incomplete or damaged. Restore the complete \
         Integration/Developer bundle, or run the installer, before managing the service.",
        diagnostics,
    )
}

fn untrusted(
    detail: impl std::fmt::Display,
    diagnostics: InstallationPreflightDiagnostics,
) -> InstallationPreflightFailure {
    failure(
        INSTALLATION_UNTRUSTED_PREFIX,
        &format!("The Control Center service package did not pass verification. {detail}"),
        diagnostics,
    )
}

fn inspect_regular_file(path: &Path) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    reject_reparse_ancestors(path)?;
    Ok(true)
}

fn after_reparse_check<T>(
    path: &Path,
    reject: impl FnOnce(&Path) -> Result<(), String>,
    inspect: impl FnOnce(&Path) -> T,
) -> Result<T, String> {
    reject(path)?;
    Ok(inspect(path))
}

#[cfg(test)]
pub(in crate::machine_integration::open_service) fn current_executable_path_gate_for_test(
    path: &Path,
    reject: impl FnOnce(&Path) -> Result<(), String>,
    canonicalize: impl FnOnce(&Path) -> PathBuf,
) -> Result<PathBuf, String> {
    after_reparse_check(path, reject, canonicalize)
}

fn set_package_state(
    observation: &mut InstallationPreflightDiagnostics,
    source: ServicePackageSource,
    state: &str,
) {
    match source {
        ServicePackageSource::Installed => {
            observation.installed_control_center = state.to_owned();
        }
        ServicePackageSource::CurrentBundle => {
            observation.current_bundle = state.to_owned();
        }
    }
}

fn package_incomplete(
    source: ServicePackageSource,
    mut observation: InstallationPreflightDiagnostics,
) -> InstallationPreflightFailure {
    set_package_state(&mut observation, source, "incomplete");
    incomplete(observation)
}

fn package_untrusted(
    source: ServicePackageSource,
    detail: impl std::fmt::Display,
    mut observation: InstallationPreflightDiagnostics,
) -> InstallationPreflightFailure {
    set_package_state(&mut observation, source, "untrusted");
    untrusted(detail, observation)
}

fn validate_complete_package(
    root: &Path,
    control_center: PathBuf,
    source: ServicePackageSource,
    mut observation: InstallationPreflightDiagnostics,
) -> Result<ServicePackage, InstallationPreflightFailure> {
    set_package_state(&mut observation, source, "checking");
    let setup_broker = root.join(SETUP_BROKER_RELATIVE_PATH);
    let runtime_manifest = root.join(RUNTIME_MANIFEST_RELATIVE_PATH);
    let control_center_present = match inspect_regular_file(&control_center) {
        Ok(present) => {
            if source == ServicePackageSource::Installed {
                observation.installed_control_center =
                    if present { "present" } else { "missing" }.to_owned();
            }
            present
        }
        Err(error) => {
            return Err(package_untrusted(source, error, observation));
        }
    };
    let setup_present = match inspect_regular_file(&setup_broker) {
        Ok(present) => {
            observation.setup_broker = if present { "present" } else { "missing" }.to_owned();
            present
        }
        Err(error) => {
            observation.setup_broker = "untrusted".to_owned();
            return Err(package_untrusted(source, error, observation));
        }
    };
    let manifest_present = match inspect_regular_file(&runtime_manifest) {
        Ok(present) => {
            observation.runtime_manifest = if present { "present" } else { "missing" }.to_owned();
            present
        }
        Err(error) => {
            observation.runtime_manifest = "untrusted".to_owned();
            return Err(package_untrusted(source, error, observation));
        }
    };
    if !control_center_present || !setup_present || !manifest_present {
        return Err(package_incomplete(source, observation));
    }
    let manifest = match read_bounded_regular_file(
        &runtime_manifest,
        MAX_BUNDLED_MANIFEST_BYTES,
        "service package runtime manifest",
    ) {
        Ok(bytes) => bytes,
        Err(error) => {
            observation.runtime_manifest = "untrusted".to_owned();
            return Err(package_untrusted(source, error, observation));
        }
    };
    if parse_bundled_runtime_manifest(&manifest).is_err() {
        observation.runtime_manifest = "invalid".to_owned();
        return Err(package_incomplete(source, observation));
    }
    let payload_root = root.join("service-runtime").join("payload").join("files");
    for name in mactype_service_contract::IMMUTABLE_RUNTIME_FILES {
        match inspect_regular_file(&payload_root.join(name)) {
            Ok(true) => {}
            Ok(false) => {
                observation.runtime_payload = "missing".to_owned();
                return Err(package_incomplete(source, observation));
            }
            Err(error) => {
                observation.runtime_payload = "untrusted".to_owned();
                return Err(package_untrusted(source, error, observation));
            }
        }
    }
    let entries = match fs::read_dir(&payload_root) {
        Ok(entries) => entries,
        Err(error) => {
            observation.runtime_payload = "untrusted".to_owned();
            return Err(package_untrusted(source, error, observation));
        }
    };
    let mut payload_names = Vec::new();
    for entry in entries.take(mactype_service_contract::IMMUTABLE_RUNTIME_FILES.len() + 1) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                observation.runtime_payload = "untrusted".to_owned();
                return Err(package_untrusted(source, error, observation));
            }
        };
        let name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(_) => {
                observation.runtime_payload = "untrusted".to_owned();
                return Err(package_untrusted(
                    source,
                    "The runtime payload contains a non-Unicode filename.",
                    observation,
                ));
            }
        };
        payload_names.push(name);
    }
    if payload_names.len() != mactype_service_contract::IMMUTABLE_RUNTIME_FILES.len()
        || payload_names
            .iter()
            .any(|name| !mactype_service_contract::IMMUTABLE_RUNTIME_FILES.contains(&name.as_str()))
    {
        observation.runtime_payload = "invalid".to_owned();
        return Err(package_untrusted(
            source,
            "The runtime payload does not contain the exact manifest-declared file set.",
            observation,
        ));
    }
    let mut payload_files = BTreeMap::new();
    for name in mactype_service_contract::IMMUTABLE_RUNTIME_FILES {
        let bytes = match read_bounded_regular_file(
            &payload_root.join(name),
            mactype_service_contract::MAX_RUNTIME_FILE_BYTES as u64,
            "service package runtime payload",
        ) {
            Ok(bytes) => bytes,
            Err(error) => {
                observation.runtime_payload = "untrusted".to_owned();
                return Err(package_untrusted(source, error, observation));
            }
        };
        payload_files.insert(name.to_owned(), bytes);
    }
    if let Err(error) = mactype_service_contract::verify_runtime_manifest(&manifest, &payload_files)
    {
        observation.runtime_payload = "invalid".to_owned();
        return Err(package_untrusted(source, error, observation));
    }
    observation.runtime_payload = "present".to_owned();
    set_package_state(&mut observation, source, "ready");
    observation.selected_service_package = match source {
        ServicePackageSource::Installed => "installed",
        ServicePackageSource::CurrentBundle => "current-bundle",
    }
    .to_owned();
    Ok(ServicePackage {
        control_center,
        setup_broker,
    })
}

fn resolve_installed_package(
    program_files: &Path,
    install_location: Option<&Path>,
    current_executable: Option<PathBuf>,
) -> Result<ServicePackage, InstallationPreflightFailure> {
    let mut observation = diagnostics(current_executable);
    let Some(install_location) = install_location else {
        observation.installed_control_center = "missing".to_owned();
        return Err(required(observation));
    };

    let expected_control_center = install_location.join(CONTROL_CENTER_FILE);
    observation.expected_installed_control_center =
        Some(expected_control_center.to_string_lossy().into_owned());
    observation.expected_executable_exists = expected_control_center.try_exists().ok();

    let install_metadata = match fs::symlink_metadata(install_location) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            observation.installed_control_center = "missing".to_owned();
            return Err(incomplete(observation));
        }
        Err(error) => {
            observation.installed_control_center = "unknown".to_owned();
            return Err(untrusted(error, observation));
        }
    };
    if !install_metadata.is_dir() {
        observation.installed_control_center = "untrusted".to_owned();
        return Err(untrusted(
            "The registered installation root is not a directory.",
            observation,
        ));
    }
    if let Err(error) = reject_reparse_ancestors(install_location) {
        observation.installed_control_center = "untrusted".to_owned();
        return Err(untrusted(error, observation));
    }

    let canonical_program_files =
        fs::canonicalize(program_files).map_err(|error| untrusted(error, observation.clone()))?;
    let canonical_install_location = fs::canonicalize(install_location)
        .map_err(|error| untrusted(error, observation.clone()))?;
    if !path_is_descendant_of(&canonical_install_location, &canonical_program_files) {
        observation.installed_control_center = "untrusted".to_owned();
        return Err(untrusted(
            "The registered location is outside Program Files.",
            observation,
        ));
    }

    validate_complete_package(
        &canonical_install_location,
        canonical_install_location.join(CONTROL_CENTER_FILE),
        ServicePackageSource::Installed,
        observation,
    )
}

fn resolve_current_package_with_diagnostics(
    current_executable: &Path,
    mut observation: InstallationPreflightDiagnostics,
) -> Result<ServicePackage, InstallationPreflightFailure> {
    observation.current_executable = Some(current_executable.to_string_lossy().into_owned());
    let canonicalization =
        match after_reparse_check(current_executable, reject_reparse_ancestors, |path| {
            fs::canonicalize(path)
        }) {
            Ok(canonicalization) => canonicalization,
            Err(error) => {
                return Err(package_untrusted(
                    ServicePackageSource::CurrentBundle,
                    error,
                    observation,
                ));
            }
        };
    let canonical_executable = match canonicalization {
        Ok(executable) => executable,
        Err(_) => {
            observation.current_bundle = "incomplete".to_owned();
            return Err(incomplete(observation));
        }
    };
    if let Err(error) = inspect_regular_file(&canonical_executable) {
        return Err(package_untrusted(
            ServicePackageSource::CurrentBundle,
            error,
            observation,
        ));
    }
    let root = canonical_executable
        .parent()
        .map(Path::to_owned)
        .ok_or_else(|| {
            untrusted(
                "The current Control Center executable has no application root.",
                observation.clone(),
            )
        })?;
    validate_complete_package(
        &root,
        canonical_executable,
        ServicePackageSource::CurrentBundle,
        observation,
    )
}

fn fallback_to_current_package(
    installed: Result<ServicePackage, InstallationPreflightFailure>,
    current_executable: &Path,
) -> Result<ServicePackage, InstallationPreflightFailure> {
    match installed {
        Ok(package) => Ok(package),
        Err(failure) => {
            resolve_current_package_with_diagnostics(current_executable, *failure.diagnostics)
        }
    }
}

#[cfg(test)]
pub(in crate::machine_integration::open_service) fn resolve_installed_package_for_trusted_layout(
    program_files: &Path,
    install_location: Option<&Path>,
) -> Result<PathBuf, String> {
    resolve_installed_package(program_files, install_location, None)
        .map(|package| package.control_center)
        .map_err(|failure| failure.error)
}

#[cfg(test)]
pub(in crate::machine_integration::open_service) fn resolve_service_package_for_layouts(
    program_files: &Path,
    install_location: Option<&Path>,
    current_executable: &Path,
) -> Result<ServicePackage, String> {
    fallback_to_current_package(
        resolve_installed_package(
            program_files,
            install_location,
            Some(current_executable.to_owned()),
        ),
        current_executable,
    )
    .map_err(|failure| failure.error)
}

#[cfg(test)]
pub(in crate::machine_integration::open_service) fn service_package_preflight_for_layouts(
    program_files: &Path,
    install_location: Option<&Path>,
    current_executable: &Path,
) -> Result<ServicePackage, InstallationPreflightFailure> {
    fallback_to_current_package(
        resolve_installed_package(
            program_files,
            install_location,
            Some(current_executable.to_owned()),
        ),
        current_executable,
    )
}

pub(in crate::machine_integration::open_service) fn service_package(
) -> Result<ServicePackage, InstallationPreflightFailure> {
    let current_executable =
        std::env::current_exe().map_err(|error| untrusted(error, diagnostics(None)))?;
    let installed = (|| {
        let mut initial = diagnostics(Some(current_executable.clone()));
        let program_files = known_folder_path(KnownFolder::ProgramFiles)
            .map_err(|error| untrusted(error, initial.clone()))?;
        let install_location = registered_install_location().map_err(|error| {
            initial.installed_control_center = "unknown".to_owned();
            untrusted(error, initial)
        })?;
        resolve_installed_package(
            &program_files,
            install_location.as_deref(),
            Some(current_executable.clone()),
        )
    })();
    fallback_to_current_package(installed, &current_executable)
}

pub(in crate::machine_integration::open_service) fn current_service_package(
) -> Result<ServicePackage, InstallationPreflightFailure> {
    let current_executable =
        std::env::current_exe().map_err(|error| untrusted(error, diagnostics(None)))?;
    resolve_elevated_current_package(&current_executable)
}

fn resolve_elevated_current_package(
    current_executable: &Path,
) -> Result<ServicePackage, InstallationPreflightFailure> {
    let mut observation = diagnostics(Some(current_executable.to_owned()));
    observation.elevation_attempted = true;
    observation.elevated_revalidation = "checking".to_owned();
    match resolve_current_package_with_diagnostics(current_executable, observation) {
        Ok(package) => Ok(package),
        Err(mut failure) => {
            failure.diagnostics.elevation_attempted = true;
            failure.diagnostics.elevated_revalidation = "failed".to_owned();
            failure.diagnostics.machine_state_changed = false;
            failure.diagnostics.rollback_required = false;
            Err(failure)
        }
    }
}

#[cfg(test)]
pub(in crate::machine_integration::open_service) fn elevated_package_preflight_for_layout(
    current_executable: &Path,
) -> Result<ServicePackage, InstallationPreflightFailure> {
    resolve_elevated_current_package(current_executable)
}

fn registered_install_location() -> Result<Option<PathBuf>, String> {
    match registered_install_location_from_key(INSTALL_RECEIPT_REGISTRY_KEY)? {
        Some(location) => Ok(Some(location)),
        None => registered_install_location_from_key(UNINSTALL_REGISTRY_KEY),
    }
}

fn registered_install_location_from_key(registry_key: &str) -> Result<Option<PathBuf>, String> {
    let Some(key) = RegistryKey::open(
        RegistryRoot::LocalMachine,
        registry_key,
        RegistryView::Native64,
    )
    .map_err(|error| {
        format!(
            "the installer registration could not be read safely (Win32 {})",
            error.raw_os_error().unwrap_or(0)
        )
    })?
    else {
        return Ok(None);
    };
    let Some(value) = key.read_string(INSTALL_LOCATION_VALUE).map_err(|error| {
        format!(
            "the installer registration changed while it was read (Win32 {})",
            error.raw_os_error().unwrap_or(0)
        )
    })?
    else {
        return Ok(None);
    };
    if value.is_empty()
        || value.encode_utf16().any(|unit| unit == 0)
        || value.len() > MAX_INSTALL_LOCATION_BYTES as usize
    {
        return Err("the installer registration contains an invalid path".to_owned());
    }
    Ok(Some(PathBuf::from(value)))
}
