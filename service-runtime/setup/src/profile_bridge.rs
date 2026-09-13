mod cleanup;
mod runtime;

use mactype_service_contract::{
    GenerationId, GenerationPointer, MachinePaths, ProfileCatalog, SourceMetadata,
    MAX_PROFILE_BYTES,
};

use self::cleanup::remove_interrupted_generated_profile_writes;
use crate::storage::{
    atomic_write, read_bounded_regular_file, reject_reparse_ancestors, SetupError,
};

pub(crate) const GENERATED_PROFILE_NAME: &str = "MacType.ini";
const MAX_POINTER_BYTES: u64 = 64 * 1024;

pub(crate) struct ProfileRuntimeBridge {
    paths: MachinePaths,
}

pub(crate) enum MaterializedProfileClearError {
    Unchanged(SetupError),
    Unknown(SetupError),
}

pub(crate) enum MaterializedProfileObservation {
    Absent,
    ExactGeneration,
}

impl MaterializedProfileClearError {
    pub(crate) fn into_setup_error(self) -> SetupError {
        match self {
            Self::Unchanged(error) | Self::Unknown(error) => error,
        }
    }
}

impl ProfileRuntimeBridge {
    pub(crate) const fn new(paths: MachinePaths) -> Self {
        Self { paths }
    }

    pub(crate) fn materialize_generation(
        &self,
        generation: &GenerationId,
    ) -> Result<bool, SetupError> {
        let Some(runtime_root) = self.active_runtime_root()? else {
            return Ok(false);
        };
        remove_interrupted_generated_profile_writes(&runtime_root)?;
        let bytes = self.verified_generation_bytes(generation)?;

        let destination = runtime_root.join(GENERATED_PROFILE_NAME);
        if destination.exists() {
            reject_reparse_ancestors(&destination)?;
            if read_bounded_regular_file(
                &destination,
                MAX_PROFILE_BYTES as u64,
                "generated runtime profile",
            )? == bytes
            {
                return Ok(true);
            }
        }
        atomic_write(&destination, &bytes)?;
        Ok(true)
    }

    fn verified_generation_bytes(&self, generation: &GenerationId) -> Result<Vec<u8>, SetupError> {
        let profile_path = self
            .paths
            .profile_generations()
            .join(generation.directory_name())
            .join("profile.ini");
        reject_reparse_ancestors(&profile_path)?;
        let bytes = read_bounded_regular_file(
            &profile_path,
            MAX_PROFILE_BYTES as u64,
            "profile generation",
        )
        .map_err(|_| SetupError::TamperedGeneration)?;
        let mut catalog = ProfileCatalog::new();
        let calculated = catalog.publish_machine_profile(
            &bytes,
            SourceMetadata {
                display_name: "runtime materialization verification".to_owned(),
            },
        )?;
        if calculated != *generation {
            return Err(SetupError::TamperedGeneration);
        }
        Ok(bytes)
    }

    pub(crate) fn materialize_active(&self) -> Result<Option<GenerationId>, SetupError> {
        let pointer_path = self.paths.active_profile();
        if !pointer_path.exists() {
            return Ok(None);
        }
        let bytes =
            read_bounded_regular_file(pointer_path, MAX_POINTER_BYTES, "active profile pointer")?;
        let pointer: GenerationPointer =
            serde_json::from_slice(&bytes).map_err(|_| SetupError::InvalidPointer)?;
        self.materialize_generation(pointer.generation())?;
        Ok(Some(pointer.generation().clone()))
    }
}
