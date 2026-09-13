use std::fs::{self, OpenOptions};
use std::io::Write;

use mactype_service_contract::{GenerationId, ProfileCatalog, SourceMetadata, MAX_PROFILE_BYTES};
use serde::Serialize;

use super::ProfileStore;
use crate::storage::{
    create_protected_directory, read_bounded_directory, read_bounded_regular_file,
    reject_reparse_ancestors, temporary_nonce, SetupError,
};

#[derive(Serialize)]
struct StoredMetadata<'a> {
    schema: u32,
    source: &'a SourceMetadata,
}

impl ProfileStore {
    pub(super) fn publish_generation(
        &self,
        generation: &GenerationId,
        profile_bytes: &[u8],
        source_metadata: &SourceMetadata,
    ) -> Result<(), SetupError> {
        let root = self.paths.profile_generations();
        create_protected_directory(root)?;
        remove_legacy_staging_collision(root, generation)?;
        let destination = root.join(generation.directory_name());
        if destination.exists() {
            self.verify_generation(generation)?;
            if read_bounded_regular_file(
                &destination.join("profile.ini"),
                MAX_PROFILE_BYTES as u64,
                "profile generation",
            )? != profile_bytes
            {
                return Err(SetupError::TamperedGeneration);
            }
            return Ok(());
        }

        let staging = root.join(format!(
            ".staging-{}-{}",
            generation.directory_name(),
            temporary_nonce()
        ));
        if staging.exists() {
            return Err(SetupError::Runtime(
                "profile staging directory already exists".to_owned(),
            ));
        }
        create_protected_directory(&staging)?;
        let result = (|| {
            write_synced(&staging.join("profile.ini"), profile_bytes)?;
            let metadata = serde_json::to_vec(&StoredMetadata {
                schema: 1,
                source: source_metadata,
            })?;
            write_synced(&staging.join("metadata.json"), &metadata)?;
            fs::rename(&staging, &destination)?;
            self.verify_generation(generation)
        })();
        if result.is_err() && staging.exists() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    }

    pub(super) fn verify_generation(&self, generation: &GenerationId) -> Result<(), SetupError> {
        let directory = self
            .paths
            .profile_generations()
            .join(generation.directory_name());
        reject_reparse_ancestors(&directory)?;
        let profile_path = directory.join("profile.ini");
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
                display_name: "verification".to_owned(),
            },
        )?;
        if &calculated != generation {
            return Err(SetupError::TamperedGeneration);
        }
        Ok(())
    }
}

fn remove_legacy_staging_collision(
    root: &std::path::Path,
    generation: &GenerationId,
) -> Result<(), SetupError> {
    let staging = root.join(format!(
        ".staging-{}-{}",
        generation.directory_name(),
        std::process::id()
    ));
    if !staging.exists() {
        return Ok(());
    }
    reject_reparse_ancestors(&staging)?;
    if !fs::metadata(&staging)?.is_dir() {
        return Err(SetupError::Runtime(
            "profile staging collision is not a protected directory".to_owned(),
        ));
    }
    let mut files = Vec::new();
    for entry in read_bounded_directory(&staging, 2, "profile staging entry count")? {
        let name = entry.file_name();
        if name != "profile.ini" && name != "metadata.json" {
            return Err(SetupError::Runtime(
                "profile staging collision contains an unexpected entry".to_owned(),
            ));
        }
        let path = entry.path();
        reject_reparse_ancestors(&path)?;
        if !entry.metadata()?.is_file() {
            return Err(SetupError::Runtime(
                "profile staging collision contains a non-regular entry".to_owned(),
            ));
        }
        files.push(path);
    }
    for path in files {
        fs::remove_file(path)?;
    }
    fs::remove_dir(staging)?;
    Ok(())
}

fn write_synced(path: &std::path::Path, bytes: &[u8]) -> Result<(), SetupError> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
