use std::io;
use std::path::Path;

use mactype_service_contract::{
    ComponentReadiness, GenerationId, GenerationPointer, MAX_PROFILE_BYTES,
    MAX_RUNTIME_ACTIVATION_RECEIPT_BYTES, MachinePaths, ParsedRuntimeActivationReceipt,
    ProfileCatalog, ReadinessReport, RuntimeActivationPhase, RuntimeGenerationPointer,
    SourceMetadata, StructuredServiceError, parse_runtime_activation_receipt,
};

use crate::active_generation::{self, ActiveRuntimeGeneration};
use crate::protected_path::{MAX_POINTER_BYTES, has_reparse_ancestor, read_bounded_regular_file};
use crate::{InitializedRuntime, RuntimeInitializer};

pub const ACTIVE_PROFILE_ABSENT_CODE: &str = "active-profile-absent";

pub struct ProtectedProfileInitializer {
    paths: MachinePaths,
}

impl ProtectedProfileInitializer {
    pub const fn new(paths: MachinePaths) -> Self {
        Self { paths }
    }

    pub(crate) fn prepare(&self) -> Result<PreparedProfileInitialization, StructuredServiceError> {
        if self.paths.profile_activation_journal().exists() {
            reject_reparse(self.paths.profile_activation_journal())?;
            return Err(activation_recovery_required());
        }
        let committed_runtime = committed_runtime_activation(&self.paths)?;
        let verified_profile = if committed_runtime.is_none() {
            Some(self.verify_active_profile()?)
        } else {
            None
        };
        Ok(PreparedProfileInitialization {
            committed_runtime,
            verified_profile,
        })
    }

    // A committed activation receipt owns the runtime pointer until the
    // installer finalizes it, so a pointer or generation that cannot be
    // resolved while the receipt exists is a recovery case, not a broken
    // installation.
    pub(crate) fn resolve_generation(
        &self,
        prepared: &PreparedProfileInitialization,
    ) -> Result<ActiveRuntimeGeneration, StructuredServiceError> {
        active_generation::resolve(&self.paths).map_err(|error| {
            if prepared.committed_runtime.is_some() {
                activation_recovery_required()
            } else {
                error
            }
        })
    }

    pub(crate) fn initialize_with_generation(
        &self,
        prepared: PreparedProfileInitialization,
        generation: &ActiveRuntimeGeneration,
    ) -> Result<InitializedRuntime, StructuredServiceError> {
        if prepared
            .committed_runtime
            .as_ref()
            .is_some_and(|pointer| pointer != generation.pointer())
        {
            return Err(activation_recovery_required());
        }

        let verified_profile = match prepared.verified_profile {
            Some(profile) => profile,
            None => self.verify_active_profile()?,
        };

        match crate::runtime_assets::validate_runtime_file_set(generation.root()) {
            Err(error) if error.code == crate::runtime_assets::RUNTIME_PROFILE_ABSENT_CODE => {
                return Err(error);
            }
            _ => {}
        }
        let runtime_profile = generation.root().join("MacType.ini");
        let runtime_bytes = read_bounded_runtime_profile(&runtime_profile)?;
        if runtime_bytes != verified_profile.bytes
            || GenerationId::from_profile_bytes(&runtime_bytes) != verified_profile.generation
        {
            return Err(service_error(
                "runtime-profile-mismatch",
                "the DLL-adjacent generated MacType.ini does not match the active profile",
            ));
        }

        Ok(InitializedRuntime::ready(
            Some(verified_profile.generation.as_str().to_owned()),
            ReadinessReport {
                profile: ComponentReadiness::Ready,
                observer: ComponentReadiness::NotRequired,
                injector32: ComponentReadiness::NotRequired,
                injector64: ComponentReadiness::NotRequired,
            },
        ))
    }

    fn verify_active_profile(&self) -> Result<VerifiedProfile, StructuredServiceError> {
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
        Ok(VerifiedProfile {
            bytes,
            generation: calculated,
        })
    }
}

impl RuntimeInitializer for ProtectedProfileInitializer {
    fn initialize(&self) -> Result<InitializedRuntime, StructuredServiceError> {
        let prepared = self.prepare()?;
        let generation = self.resolve_generation(&prepared)?;
        self.initialize_with_generation(prepared, &generation)
    }
}

pub(crate) struct PreparedProfileInitialization {
    committed_runtime: Option<RuntimeGenerationPointer>,
    verified_profile: Option<VerifiedProfile>,
}

struct VerifiedProfile {
    bytes: Vec<u8>,
    generation: GenerationId,
}

fn committed_runtime_activation(
    paths: &MachinePaths,
) -> Result<Option<RuntimeGenerationPointer>, StructuredServiceError> {
    let journal_path = paths.runtime_activation_journal();
    if !journal_path.exists() {
        return Ok(None);
    }
    let journal_bytes = read_bounded_activation_receipt(journal_path)?;
    let ParsedRuntimeActivationReceipt::Current(receipt) =
        parse_runtime_activation_receipt(&journal_bytes)
            .map_err(|_| activation_recovery_required())?
    else {
        return Err(activation_recovery_required());
    };
    if receipt.phase() != RuntimeActivationPhase::Committed {
        return Err(activation_recovery_required());
    }
    Ok(Some(receipt.activated().clone()))
}

fn read_bounded_activation_receipt(path: &Path) -> Result<Vec<u8>, StructuredServiceError> {
    if has_reparse_ancestor(path).map_err(|_| activation_recovery_required())? {
        return Err(activation_recovery_required());
    }
    read_bounded_regular_file(path, MAX_RUNTIME_ACTIVATION_RECEIPT_BYTES).map_err(|error| {
        if error.kind() == io::ErrorKind::InvalidData {
            service_error(
                "activation-recovery-required",
                "the runtime activation receipt is not a bounded regular file",
            )
        } else {
            service_error(
                "activation-recovery-required",
                "the runtime activation receipt could not be read",
            )
        }
    })
}

fn activation_recovery_required() -> StructuredServiceError {
    service_error(
        "activation-recovery-required",
        "a protected activation journal requires setup recovery before start unless it durably commits and exactly owns the active runtime candidate",
    )
}

fn read_bounded_runtime_profile(path: &Path) -> Result<Vec<u8>, StructuredServiceError> {
    active_generation::reject_reparse(path)?;
    read_bounded_regular_file(path, MAX_PROFILE_BYTES as u64).map_err(|error| {
        if error.kind() == io::ErrorKind::InvalidData {
            service_error(
                "runtime-profile-invalid",
                "the DLL-adjacent generated MacType.ini is not a bounded regular file",
            )
        } else {
            service_error(
                "runtime-profile-unavailable",
                "the DLL-adjacent generated MacType.ini could not be read",
            )
        }
    })
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

#[cfg(test)]
mod tests {
    use std::fs;

    use mactype_service_contract::MachinePaths;

    use super::ProtectedProfileInitializer;
    use crate::active_generation;

    #[test]
    #[cfg_attr(
        all(miri, windows),
        ignore = "Windows Miri does not implement CreateDirectoryW"
    )]
    fn committed_receipt_is_compared_with_the_already_resolved_pointer() {
        let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let program_files = root.path().join("Program Files");
        let program_data = root.path().join("ProgramData");
        fs::create_dir_all(&program_files).unwrap();
        fs::create_dir_all(&program_data).unwrap();
        let paths = MachinePaths::from_trusted_os_roots(&program_files, &program_data).unwrap();
        fs::create_dir_all(paths.runtime_versions().join("0.2.0")).unwrap();
        fs::create_dir_all(paths.runtime_versions().join("0.3.0")).unwrap();
        fs::write(
            paths.runtime_pointer(),
            br#"{"schema":1,"version":"0.2.0"}"#,
        )
        .unwrap();
        fs::write(
            paths.runtime_activation_journal(),
            br#"{"schema":3,"phase":"committed","previous":null,"activated":{"schema":1,"version":"0.3.0"}}"#,
        )
        .unwrap();

        let initializer = ProtectedProfileInitializer::new(paths.clone());
        let prepared = initializer.prepare().unwrap();
        let generation = active_generation::resolve(&paths).unwrap();
        fs::write(
            paths.runtime_pointer(),
            br#"{"schema":1,"version":"0.3.0"}"#,
        )
        .unwrap();

        let error = match initializer.initialize_with_generation(prepared, &generation) {
            Ok(_) => panic!("the committed receipt does not own the resolved runtime binding"),
            Err(error) => error,
        };

        assert_eq!(error.code, "activation-recovery-required");
    }

    #[test]
    #[cfg_attr(
        all(miri, windows),
        ignore = "Windows Miri does not implement CreateDirectoryW"
    )]
    fn an_unresolvable_pointer_under_a_committed_receipt_asks_for_recovery() {
        let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let program_files = root.path().join("Program Files");
        let program_data = root.path().join("ProgramData");
        fs::create_dir_all(&program_files).unwrap();
        fs::create_dir_all(&program_data).unwrap();
        let paths = MachinePaths::from_trusted_os_roots(&program_files, &program_data).unwrap();
        fs::create_dir_all(paths.runtime_activation_journal().parent().unwrap()).unwrap();
        fs::write(
            paths.runtime_activation_journal(),
            br#"{"schema":3,"phase":"committed","previous":null,"activated":{"schema":1,"version":"0.3.0"}}"#,
        )
        .unwrap();

        let initializer = ProtectedProfileInitializer::new(paths.clone());
        let prepared = initializer.prepare().unwrap();
        assert_eq!(
            active_generation::resolve(&paths).unwrap_err().code,
            "active-runtime-unavailable"
        );
        assert_eq!(
            initializer.resolve_generation(&prepared).unwrap_err().code,
            "activation-recovery-required"
        );
    }
}
