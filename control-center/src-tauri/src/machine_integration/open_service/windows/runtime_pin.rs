use super::*;

#[derive(Clone)]
pub(super) struct MigrationRuntimePinLease {
    path: PathBuf,
    bytes: Vec<u8>,
}

fn collect_runtime_generation_names(
    entries: impl IntoIterator<Item = std::io::Result<std::ffi::OsString>>,
) -> Result<BTreeSet<String>, String> {
    collect_bounded_directory_entries(
        entries,
        MAX_RUNTIME_GENERATION_FILES,
        "migration runtime filename count",
    )?
    .into_iter()
    .map(|name| {
        name.into_string()
            .map_err(|_| "migration runtime filename is not Unicode".to_owned())
    })
    .collect()
}

pub(super) fn create_migration_runtime_pin(
    paths: &MachinePaths,
    generations: &BTreeSet<String>,
) -> Result<Option<MigrationRuntimePinLease>, String> {
    if generations.is_empty() {
        return Ok(None);
    }
    let mut runtimes = Vec::with_capacity(generations.len());
    for version in generations {
        if !safe_version(version) {
            return Err("migration runtime generation name is invalid".to_owned());
        }
        let root = paths.runtime_versions().join(version);
        reject_reparse_ancestors(&root)?;
        if !fs::metadata(&root)
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            return Err("migration runtime generation is not a directory".to_owned());
        }
        let names = collect_runtime_generation_names(
            fs::read_dir(&root)
                .map_err(|error| error.to_string())?
                .map(|entry| entry.map(|entry| entry.file_name())),
        )?;
        if names.len() < IMMUTABLE_RUNTIME_FILES.len()
            || names.len() > IMMUTABLE_RUNTIME_FILES.len() + 1
            || IMMUTABLE_RUNTIME_FILES
                .iter()
                .any(|name| !names.contains(*name))
            || names.iter().any(|name| {
                name != "MacType.ini" && !IMMUTABLE_RUNTIME_FILES.contains(&name.as_str())
            })
        {
            return Err("migration runtime contains an unexpected file set".to_owned());
        }
        let files = IMMUTABLE_RUNTIME_FILES
            .iter()
            .map(|name| {
                read_bounded_regular_file(
                    &root.join(name),
                    MAX_RUNTIME_FILE_BYTES as u64,
                    "migration runtime file",
                )
                .map(|bytes| ((*name).to_owned(), sha256_digest(&bytes)))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let generated_profile = names
            .contains("MacType.ini")
            .then(|| {
                read_bounded_regular_file(
                    &root.join("MacType.ini"),
                    MAX_PROFILE_BYTES as u64,
                    "migration runtime generated profile",
                )
                .map(|bytes| sha256_digest(&bytes))
            })
            .transpose()?;
        runtimes.push(
            MigrationPinnedRuntime::new(version.clone(), files, generated_profile)
                .map_err(|error| error.to_string())?,
        );
    }

    let nonce = profile_transfer_nonce_text(&random_profile_transfer_nonce()?);
    let pin =
        MigrationRuntimePin::new(nonce.clone(), runtimes).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(&pin).map_err(|error| error.to_string())?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_MIGRATION_RUNTIME_PIN_BYTES {
        return Err("migration runtime pin exceeds the fixed size limit".to_owned());
    }
    let root = paths.service_root().join("migration-runtime-pins");
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    reject_reparse_ancestors(&root)?;
    if !fs::metadata(&root)
        .map_err(|error| error.to_string())?
        .is_dir()
    {
        return Err("migration runtime pin root is not a directory".to_owned());
    }
    let path = root.join(format!("{nonce}.json"));
    let temporary = root.join(format!(".{nonce}.creating"));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        fs::rename(&temporary, &path).map_err(|error| error.to_string())
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(Some(MigrationRuntimePinLease { path, bytes }))
}

pub(super) fn release_migration_runtime_pin(
    snapshot: &SystemOpenServiceSnapshot,
) -> Result<(), String> {
    let Some(lease) = snapshot.migration_runtime_pin.as_ref() else {
        return Ok(());
    };
    let paths = fixed_machine_paths()?;
    let root = paths.service_root().join("migration-runtime-pins");
    if !lease
        .path
        .parent()
        .is_some_and(|parent| same_path(parent, &root))
    {
        return Err("migration runtime pin escaped the protected root".to_owned());
    }
    let current = read_bounded_regular_file(
        &lease.path,
        MAX_MIGRATION_RUNTIME_PIN_BYTES,
        "migration runtime pin",
    )?;
    if current != lease.bytes {
        return Err("migration runtime pin changed before release".to_owned());
    }
    let pin: MigrationRuntimePin =
        serde_json::from_slice(&current).map_err(|error| error.to_string())?;
    pin.validate().map_err(|error| error.to_string())?;
    let expected_name = format!("{}.json", pin.nonce());
    if lease.path.file_name() != Some(OsStr::new(&expected_name)) {
        return Err("migration runtime pin filename is invalid".to_owned());
    }
    fs::remove_file(&lease.path).map_err(|error| error.to_string())?;
    if fs::read_dir(&root)
        .map_err(|error| error.to_string())?
        .next()
        .is_none()
    {
        fs::remove_dir(root).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::ffi::OsString;

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
    fn runtime_generation_filename_scan_stops_at_maximum_plus_one() {
        let observed = Cell::new(0usize);
        let entries = std::iter::from_fn(|| {
            let index = observed.get();
            if index >= MAX_RUNTIME_GENERATION_FILES + 8 {
                return None;
            }
            observed.set(index + 1);
            Some(Ok(OsString::from(format!("entry-{index}"))))
        });

        let error = collect_runtime_generation_names(entries)
            .expect_err("oversized runtime filename iterator must fail closed");

        assert!(error.contains("runtime filename count"), "{error}");
        assert_eq!(observed.get(), MAX_RUNTIME_GENERATION_FILES + 1);
    }

    #[test]
    fn oversized_runtime_generation_preserves_files_and_creates_no_pin() {
        let root = TestDirectory::new("oversized-runtime-pin");
        let program_files = root.0.join("ProgramFiles");
        let program_data = root.0.join("ProgramData");
        fs::create_dir(&program_files).expect("create Program Files root");
        fs::create_dir(&program_data).expect("create ProgramData root");
        let paths = MachinePaths::from_trusted_os_roots(&program_files, &program_data)
            .expect("create machine paths");
        let version = "1.0.0";
        let generation = paths.runtime_versions().join(version);
        fs::create_dir_all(&generation).expect("create runtime generation");
        let mut expected = BTreeMap::new();
        for name in IMMUTABLE_RUNTIME_FILES
            .into_iter()
            .chain(["MacType.ini", "unexpected.bin"])
        {
            let bytes = format!("preserve-{name}").into_bytes();
            fs::write(generation.join(name), &bytes).expect("write runtime generation file");
            expected.insert(name.to_owned(), bytes);
        }

        let result = create_migration_runtime_pin(&paths, &BTreeSet::from([version.to_owned()]));

        let error = match result {
            Ok(_) => panic!("oversized runtime generation must fail closed"),
            Err(error) => error,
        };
        assert!(error.contains("runtime filename count"), "{error}");
        assert!(!paths.service_root().join("migration-runtime-pins").exists());
        for (name, bytes) in expected {
            assert_eq!(
                fs::read(generation.join(name)).expect("read preserved runtime generation file"),
                bytes
            );
        }
    }
}
