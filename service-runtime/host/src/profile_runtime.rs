use std::io;
use std::path::Path;

use mactype_service_contract::{
    parse_runtime_activation_receipt, ComponentReadiness, GenerationId, GenerationPointer,
    MachinePaths, ParsedRuntimeActivationReceipt, ProfileCatalog, ReadinessReport,
    RuntimeActivationPhase, RuntimeGenerationPointer, SourceMetadata, StructuredServiceError,
    MAX_PROFILE_BYTES, MAX_RUNTIME_ACTIVATION_RECEIPT_BYTES,
};

use crate::protected_path::{
    has_reparse_ancestor, read_bounded_regular_file, runtime_pointer_version, MAX_POINTER_BYTES,
};
use crate::{InitializedRuntime, RuntimeInitializer};

pub const ACTIVE_PROFILE_ABSENT_CODE: &str = "active-profile-absent";

pub struct ProtectedProfileInitializer {
    paths: MachinePaths,
}

impl ProtectedProfileInitializer {
    pub const fn new(paths: MachinePaths) -> Self {
        Self { paths }
    }
}

impl RuntimeInitializer for ProtectedProfileInitializer {
    fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError> {
        if self.paths.profile_activation_journal().exists() {
            reject_reparse(self.paths.profile_activation_journal())?;
            return Err(activation_recovery_required());
        }
        validate_runtime_activation_receipt(&self.paths)?;
        let active_profile = self.paths.active_profile();
        reject_reparse(active_profile)?;
        let pointer_bytes =
            read_bounded_regular_file(active_profile, MAX_POINTER_BYTES).map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    service_error(
                        ACTIVE_PROFILE_ABSENT_CODE,
                        "no profile has been published yet, so the service stays stopped until setup publishes one",
                    )
                } else if error.kind() == io::ErrorKind::InvalidData {
                    service_error(
                        "active-profile-invalid",
                        "the protected active profile pointer is not a bounded regular file",
                    )
                } else {
                    service_error(
                        "active-profile-unavailable",
                        "the protected active profile pointer could not be read",
                    )
                }
            })?;
        let pointer: GenerationPointer = serde_json::from_slice(&pointer_bytes).map_err(|_| {
            service_error(
                "active-profile-invalid",
                "the protected active profile pointer is invalid",
            )
        })?;

        let profile_path = self
            .paths
            .profile_generations()
            .join(pointer.generation().directory_name())
            .join("profile.ini");
        let bytes = read_bounded_protected_file(
            &profile_path,
            MAX_PROFILE_BYTES as u64,
            (
                "active-profile-unavailable",
                "the protected profile generation could not be read",
            ),
            (
                "active-profile-invalid",
                "the protected profile generation is not a bounded regular file",
            ),
        )?;
        let mut catalog = ProfileCatalog::new();
        let calculated = catalog
            .publish_machine_profile(
                &bytes,
                SourceMetadata {
                    display_name: "service verification".to_owned(),
                },
            )
            .map_err(|_| {
                service_error(
                    "active-profile-invalid",
                    "the protected profile is not a valid INI",
                )
            })?;
        if &calculated != pointer.generation() {
            return Err(service_error(
                "active-profile-tampered",
                "the protected profile digest does not match its generation",
            ));
        }
        let runtime_profile = active_runtime_profile_path(&self.paths)?;
        if let Some(runtime_root) = runtime_profile.parent() {
            match crate::runtime_assets::validate_runtime_file_set(runtime_root) {
                Err(error) if error.code == crate::runtime_assets::RUNTIME_PROFILE_ABSENT_CODE => {
                    return Err(error);
                }
                _ => {}
            }
        }
        let runtime_bytes = read_bounded_protected_file(
            &runtime_profile,
            MAX_PROFILE_BYTES as u64,
            (
                "runtime-profile-unavailable",
                "the DLL-adjacent generated MacType.ini could not be read",
            ),
            (
                "runtime-profile-invalid",
                "the DLL-adjacent generated MacType.ini is not a bounded regular file",
            ),
        )?;
        if runtime_bytes != bytes
            || GenerationId::from_profile_bytes(&runtime_bytes) != *pointer.generation()
        {
            return Err(service_error(
                "runtime-profile-mismatch",
                "the DLL-adjacent generated MacType.ini does not match the active profile",
            ));
        }

        Ok(InitializedRuntime::ready(
            Some(calculated.as_str().to_owned()),
            ReadinessReport {
                profile: ComponentReadiness::Ready,
                observer: ComponentReadiness::NotRequired,
                injector32: ComponentReadiness::NotRequired,
                injector64: ComponentReadiness::NotRequired,
            },
        ))
    }
}

fn active_runtime_profile_path(
    paths: &MachinePaths,
) -> Result<std::path::PathBuf, StructuredServiceError> {
    let pointer_path = paths.runtime_pointer();
    let bytes = read_bounded_protected_file(
        pointer_path,
        MAX_POINTER_BYTES,
        (
            "active-runtime-unavailable",
            "the protected active runtime pointer could not be read",
        ),
        (
            "active-runtime-invalid",
            "the protected active runtime pointer is not a bounded regular file",
        ),
    )?;
    let version = runtime_pointer_version(&bytes).ok_or_else(|| {
        service_error(
            "active-runtime-invalid",
            "the protected active runtime pointer has an unsupported value",
        )
    })?;
    let runtime_root = paths.runtime_versions().join(version);
    reject_reparse(&runtime_root)?;
    if !runtime_root.is_dir() {
        return Err(service_error(
            "active-runtime-unavailable",
            "the protected active runtime generation is missing",
        ));
    }
    Ok(runtime_root.join("MacType.ini"))
}

fn validate_runtime_activation_receipt(paths: &MachinePaths) -> Result<(), StructuredServiceError> {
    let journal_path = paths.runtime_activation_journal();
    if !journal_path.exists() {
        return Ok(());
    }
    let journal_bytes = read_bounded_protected_file(
        journal_path,
        MAX_RUNTIME_ACTIVATION_RECEIPT_BYTES,
        (
            "activation-recovery-required",
            "the runtime activation receipt could not be read",
        ),
        (
            "activation-recovery-required",
            "the runtime activation receipt is not a bounded regular file",
        ),
    )?;
    let ParsedRuntimeActivationReceipt::Current(receipt) =
        parse_runtime_activation_receipt(&journal_bytes)
            .map_err(|_| activation_recovery_required())?
    else {
        return Err(activation_recovery_required());
    };
    if receipt.phase() != RuntimeActivationPhase::Committed {
        return Err(activation_recovery_required());
    }

    let pointer_bytes = read_bounded_protected_file(
        paths.runtime_pointer(),
        MAX_POINTER_BYTES,
        (
            "activation-recovery-required",
            "the active runtime pointer could not be read during activation",
        ),
        (
            "activation-recovery-required",
            "the active runtime pointer is not a bounded regular file during activation",
        ),
    )?;
    let active = RuntimeGenerationPointer::parse(&pointer_bytes)
        .map_err(|_| activation_recovery_required())?;
    if &active != receipt.activated() {
        return Err(activation_recovery_required());
    }
    Ok(())
}

fn activation_recovery_required() -> StructuredServiceError {
    service_error(
        "activation-recovery-required",
        "a protected activation journal requires setup recovery before start unless it durably commits and exactly owns the active runtime candidate",
    )
}

fn read_bounded_protected_file(
    path: &Path,
    maximum_bytes: u64,
    unavailable: (&str, &str),
    invalid: (&str, &str),
) -> Result<Vec<u8>, StructuredServiceError> {
    reject_reparse(path)?;
    read_bounded_regular_file(path, maximum_bytes).map_err(|error| {
        if error.kind() == io::ErrorKind::InvalidData {
            service_error(invalid.0, invalid.1)
        } else {
            service_error(unavailable.0, unavailable.1)
        }
    })
}

fn reject_reparse(path: &Path) -> Result<(), StructuredServiceError> {
    if has_reparse_ancestor(path).map_err(|_| {
        service_error(
            "active-profile-inaccessible",
            "the protected profile path could not be inspected",
        )
    })? {
        return Err(service_error(
            "active-profile-reparse",
            "reparse points are forbidden in the protected profile path",
        ));
    }
    Ok(())
}

fn service_error(code: &str, message: &str) -> StructuredServiceError {
    StructuredServiceError {
        code: code.to_owned(),
        message: message.to_owned(),
        win32_error: None,
    }
}
