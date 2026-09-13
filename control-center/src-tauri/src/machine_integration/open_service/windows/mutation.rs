use super::*;

pub(super) fn combine_mutation_recording(
    operation: Result<(), String>,
    recording: Result<(), String>,
) -> Result<(), String> {
    match (operation, recording) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(recording)) => Err(format!(
            "machine mutation completed but cleanup ownership is unknown: {recording}"
        )),
        (Err(error), Err(recording)) => Err(format!(
            "{error}; cleanup ownership is unknown: {recording}"
        )),
    }
}

fn record_optional_mutation(
    receipt: &mut Option<OptionalFileMutation>,
    before: Option<&[u8]>,
    after: Option<Vec<u8>>,
) -> Result<(), String> {
    if after.as_deref() == before {
        return Ok(());
    }
    let observed = OptionalFileMutation { after };
    match receipt {
        Some(existing) if existing != &observed => {
            Err("a protected file changed after its transaction receipt was recorded".to_owned())
        }
        Some(_) => Ok(()),
        None => {
            *receipt = Some(observed);
            Ok(())
        }
    }
}

pub(super) fn generation_manifest(
    path: &Path,
    maximum_files: usize,
) -> Result<BTreeMap<String, String>, String> {
    reject_reparse_ancestors(path)?;
    if !path.is_dir() {
        return Err("protected generation is not a directory".to_owned());
    }
    let entries = collect_bounded_directory_entries(
        fs::read_dir(path).map_err(|error| error.to_string())?,
        maximum_files,
        "protected generation file count",
    )?;
    let mut manifest = BTreeMap::new();
    for entry in entries {
        let entry_path = entry.path();
        reject_reparse_ancestors(&entry_path)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "protected generation filename is not Unicode".to_owned())?;
        let maximum = if name.eq_ignore_ascii_case("MacType.ini")
            || name.eq_ignore_ascii_case("profile.ini")
        {
            MAX_PROFILE_BYTES as u64
        } else if name.eq_ignore_ascii_case("metadata.json") {
            64 * 1024
        } else {
            MAX_RUNTIME_FILE_BYTES as u64
        };
        let bytes =
            read_bounded_regular_file(&entry_path, maximum, "transaction-owned generation file")?;
        manifest.insert(name, sha256_digest(&bytes));
    }
    Ok(manifest)
}

fn fixed_bundled_runtime_manifest() -> Result<BundledRuntimeManifest, String> {
    let runtime_root = fixed_setup_path()?
        .parent()
        .ok_or_else(|| "fixed setup broker has no runtime bundle root".to_owned())?
        .to_path_buf();
    let bytes = read_bounded_regular_file(
        &runtime_root.join("payload").join("manifest.json"),
        MAX_BUNDLED_MANIFEST_BYTES,
        "bundled runtime manifest",
    )?;
    parse_bundled_runtime_manifest(&bytes)
}

fn record_generation_receipt(
    receipts: &mut BTreeMap<String, BTreeMap<String, String>>,
    name: String,
    manifest: BTreeMap<String, String>,
) -> Result<(), String> {
    match receipts.get(&name) {
        Some(existing) if existing != &manifest => Err(format!(
            "protected generation {name} changed after its transaction receipt was recorded"
        )),
        Some(_) => Ok(()),
        None => {
            receipts.insert(name, manifest);
            Ok(())
        }
    }
}

pub(super) fn record_protected_mutations(
    snapshot: &SystemOpenServiceSnapshot,
    profile: &[u8],
) -> Result<(), String> {
    let paths = fixed_machine_paths()?;
    let runtime_pointer = snapshot_optional_file(paths.runtime_pointer(), 64 * 1024)?;
    let active_profile = snapshot_optional_file(paths.active_profile(), 64 * 1024)?;
    let previous_profile = snapshot_optional_file(paths.previous_profile(), 64 * 1024)?;
    let adjacent_profile = snapshot
        .adjacent_profile
        .as_ref()
        .map(|(path, _)| snapshot_optional_file(path, MAX_PROFILE_BYTES as u64))
        .transpose()?;
    let runtime_generations = snapshot_generation_directories(
        paths.runtime_versions(),
        safe_version,
        MAX_PROTECTED_GENERATION_DIRECTORIES,
    )?;
    let profile_generations = snapshot_generation_directories(
        paths.profile_generations(),
        safe_profile_generation,
        MAX_PROTECTED_GENERATION_DIRECTORIES,
    )?;
    let profile_generation = GenerationId::from_profile_bytes(profile);
    let expected_profile_directory = profile_generation.directory_name();

    let mut receipt = snapshot
        .mutations
        .lock()
        .map_err(|_| "protected mutation receipt lock is poisoned".to_owned())?;
    record_optional_mutation(
        &mut receipt.runtime_pointer,
        snapshot.runtime_pointer.as_deref(),
        runtime_pointer.clone(),
    )?;
    record_optional_mutation(
        &mut receipt.active_profile_pointer,
        snapshot.active_profile_pointer.as_deref(),
        active_profile,
    )?;
    record_optional_mutation(
        &mut receipt.previous_profile_pointer,
        snapshot.previous_profile_pointer.as_deref(),
        previous_profile,
    )?;
    if let (Some((_, before)), Some(after)) = (&snapshot.adjacent_profile, adjacent_profile) {
        record_optional_mutation(&mut receipt.adjacent_profile, before.as_deref(), after)?;
    }

    for name in profile_generations.difference(&snapshot.profile_generations) {
        if name != expected_profile_directory {
            return Err(format!(
                "unreceipted profile generation {name} appeared during migration"
            ));
        }
        let root = paths.profile_generations().join(name);
        let manifest = generation_manifest(&root, MAX_PROFILE_GENERATION_FILES)?;
        let expected_names = BTreeSet::from(["metadata.json".to_owned(), "profile.ini".to_owned()]);
        if manifest.keys().cloned().collect::<BTreeSet<_>>() != expected_names
            || read_bounded_regular_file(
                &root.join("profile.ini"),
                MAX_PROFILE_BYTES as u64,
                "transaction profile generation",
            )? != profile
        {
            return Err("new profile generation does not match the published payload".to_owned());
        }
        record_generation_receipt(&mut receipt.profile_generations, name.clone(), manifest)?;
    }

    let bundled = fixed_bundled_runtime_manifest()?;
    for name in runtime_generations.difference(&snapshot.runtime_generations) {
        if name != &bundled.version {
            return Err(format!(
                "unreceipted runtime generation {name} appeared during migration"
            ));
        }
        let root = paths.runtime_versions().join(name);
        let manifest = generation_manifest(&root, MAX_RUNTIME_GENERATION_FILES)?;
        if bundled
            .files
            .iter()
            .any(|(file, digest)| manifest.get(file) != Some(digest))
            || manifest.keys().any(|file| {
                file != "MacType.ini" && !IMMUTABLE_RUNTIME_FILES.contains(&file.as_str())
            })
            || manifest.len() < IMMUTABLE_RUNTIME_FILES.len()
            || manifest.len() > IMMUTABLE_RUNTIME_FILES.len() + 1
        {
            return Err("new runtime generation does not match the bundled manifest".to_owned());
        }
        if manifest.contains_key("MacType.ini")
            && read_bounded_regular_file(
                &root.join("MacType.ini"),
                MAX_PROFILE_BYTES as u64,
                "transaction runtime profile",
            )? != profile
        {
            return Err("new runtime generation contains a foreign profile".to_owned());
        }
        record_generation_receipt(&mut receipt.runtime_generations, name.clone(), manifest)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let path = std::env::current_dir()
                .expect("current directory")
                .join(format!(
                    ".mactype-{label}-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("system time")
                        .as_nanos()
                ));
            fs::create_dir(&path).expect("create test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn generation_manifest_rejects_more_than_the_fixed_generation_file_set() {
        let root = TestDirectory::new("oversized-generation-manifest");
        let file_count = IMMUTABLE_RUNTIME_FILES.len() + 2;
        for index in 0..file_count {
            fs::write(root.0.join(format!("entry-{index}.bin")), b"preserve")
                .expect("write generation file");
        }

        let error = generation_manifest(&root.0, MAX_RUNTIME_GENERATION_FILES)
            .expect_err("oversized manifest must fail closed");

        assert!(error.contains("file count"), "{error}");
        assert_eq!(
            fs::read_dir(&root.0).expect("read test directory").count(),
            file_count
        );
        for index in 0..file_count {
            assert_eq!(
                fs::read(root.0.join(format!("entry-{index}.bin")))
                    .expect("read preserved generation file"),
                b"preserve"
            );
        }
    }

    #[test]
    fn profile_generation_manifest_uses_the_smaller_expected_file_count() {
        let root = TestDirectory::new("oversized-profile-generation-manifest");
        for name in ["metadata.json", "profile.ini", "unexpected.bin"] {
            fs::write(root.0.join(name), b"preserve").expect("write profile generation file");
        }

        let error = generation_manifest(&root.0, MAX_PROFILE_GENERATION_FILES)
            .expect_err("profile generation with a third file must fail closed");

        assert!(error.contains("file count"), "{error}");
        for name in ["metadata.json", "profile.ini", "unexpected.bin"] {
            assert_eq!(
                fs::read(root.0.join(name)).expect("read preserved profile generation file"),
                b"preserve"
            );
        }
    }
}
