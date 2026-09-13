use super::*;

pub(super) const MAX_RUNTIME_GENERATION_FILES: usize = IMMUTABLE_RUNTIME_FILES.len() + 1;
pub(super) const MAX_PROFILE_GENERATION_FILES: usize = 2;
pub(super) const MAX_PROTECTED_GENERATION_DIRECTORIES: usize = 4096;

pub(super) fn collect_bounded_directory_entries<T>(
    entries: impl IntoIterator<Item = std::io::Result<T>>,
    maximum: usize,
    context: &str,
) -> Result<Vec<T>, String> {
    let mut collected = Vec::with_capacity(maximum);
    for entry in entries {
        if collected.len() == maximum {
            return Err(format!("{context} exceeds its fixed limit"));
        }
        collected.push(entry.map_err(|error| error.to_string())?);
    }
    Ok(collected)
}

#[derive(Clone)]
pub(in crate::machine_integration::open_service) struct SystemOpenServiceSnapshot {
    pub(super) status: SystemServiceStatus,
    pub(super) service_root_existed: bool,
    pub(super) runtime_versions_existed: bool,
    pub(super) profile_generations_existed: bool,
    pub(super) runtime_pointer: Option<Vec<u8>>,
    pub(super) active_profile_pointer: Option<Vec<u8>>,
    pub(super) previous_profile_pointer: Option<Vec<u8>>,
    pub(super) runtime_generations: BTreeSet<String>,
    pub(super) profile_generations: BTreeSet<String>,
    pub(super) adjacent_profile: Option<(PathBuf, Option<Vec<u8>>)>,
    pub(super) migration_runtime_pin: Option<MigrationRuntimePinLease>,
    pub(super) mutations: Arc<Mutex<ProtectedMutationReceipt>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OptionalFileMutation {
    pub(super) after: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ProtectedMutationReceipt {
    pub(super) runtime_pointer: Option<OptionalFileMutation>,
    pub(super) active_profile_pointer: Option<OptionalFileMutation>,
    pub(super) previous_profile_pointer: Option<OptionalFileMutation>,
    pub(super) adjacent_profile: Option<OptionalFileMutation>,
    pub(super) runtime_generations: BTreeMap<String, BTreeMap<String, String>>,
    pub(super) profile_generations: BTreeMap<String, BTreeMap<String, String>>,
}

pub(super) fn machine_roots_impl() -> Result<(PathBuf, PathBuf), String> {
    Ok((
        known_folder(KnownFolder::ProgramFiles)?,
        known_folder(KnownFolder::ProgramData)?,
    ))
}

pub(super) fn fixed_machine_paths() -> Result<MachinePaths, String> {
    let (program_files, program_data) = machine_roots_impl()?;
    MachinePaths::from_trusted_os_roots(&program_files, &program_data)
        .map_err(|error| error.to_string())
}

pub(super) fn capture_open_service_snapshot() -> Result<SystemOpenServiceSnapshot, String> {
    let status = query();
    let stable = matches!(
        status.runtime,
        RuntimeState::Running | RuntimeState::Stopped
    );
    let installation_safe = match status.installation {
        InstallationState::Absent => true,
        InstallationState::Current | InstallationState::Outdated => {
            status.backend == ServiceBackend::OpenSource && stable
        }
        InstallationState::Invalid
        | InstallationState::Inaccessible
        | InstallationState::DeletePending => false,
    };
    if !installation_safe {
        return Err("the open service is foreign, transitional, or inaccessible".to_owned());
    }

    let paths = fixed_machine_paths()?;
    let service_root_existed = paths.service_root().is_dir();
    let runtime_versions_existed = paths.runtime_versions().is_dir();
    let profile_generations_existed = paths.profile_generations().is_dir();
    let runtime_pointer = snapshot_optional_file(paths.runtime_pointer(), 64 * 1024)?;
    let active_profile_pointer = snapshot_optional_file(paths.active_profile(), 64 * 1024)?;
    let previous_profile_pointer = snapshot_optional_file(paths.previous_profile(), 64 * 1024)?;
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
    let adjacent_profile = active_runtime_root(&paths)?
        .map(|root| {
            let path = root.join("MacType.ini");
            snapshot_optional_file(&path, MAX_PROFILE_BYTES as u64).map(|state| (path, state))
        })
        .transpose()?;
    let migration_runtime_pin = create_migration_runtime_pin(&paths, &runtime_generations)?;

    Ok(SystemOpenServiceSnapshot {
        status,
        service_root_existed,
        runtime_versions_existed,
        profile_generations_existed,
        runtime_pointer,
        active_profile_pointer,
        previous_profile_pointer,
        runtime_generations,
        profile_generations,
        adjacent_profile,
        migration_runtime_pin,
        mutations: Arc::new(Mutex::new(ProtectedMutationReceipt::default())),
    })
}

pub(super) fn ensure_open_service_unchanged(
    snapshot: &SystemOpenServiceSnapshot,
) -> Result<(), String> {
    let current = query();
    let stable = matches!(
        current.runtime,
        RuntimeState::Running | RuntimeState::Stopped
    );
    let unchanged = match snapshot.status.installation {
        InstallationState::Absent => current.installation == InstallationState::Absent,
        InstallationState::Current | InstallationState::Outdated => {
            current.backend == ServiceBackend::OpenSource
                && current.installation == snapshot.status.installation
                && stable
                && current
                    .binary_path
                    .as_deref()
                    .zip(snapshot.status.binary_path.as_deref())
                    .is_some_and(|(current, saved)| current.eq_ignore_ascii_case(saved))
        }
        _ => false,
    };
    if unchanged {
        Ok(())
    } else {
        Err("the fixed open service state changed during migration".to_owned())
    }
}

pub(super) fn snapshot_optional_file(path: &Path, maximum: u64) -> Result<Option<Vec<u8>>, String> {
    if !path.exists() {
        return Ok(None);
    }
    reject_reparse_ancestors(path)?;
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum {
        return Err("protected machine state contains an invalid file".to_owned());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.is_empty() || bytes.len() > maximum as usize {
        return Err("protected machine state exceeds its fixed size limit".to_owned());
    }
    Ok(Some(bytes))
}

pub(super) fn snapshot_generation_directories(
    root: &Path,
    valid_name: fn(&str) -> bool,
    maximum: usize,
) -> Result<BTreeSet<String>, String> {
    if !root.exists() {
        return Ok(BTreeSet::new());
    }
    reject_reparse_ancestors(root)?;
    if !root.is_dir() {
        return Err("protected generation root is not a directory".to_owned());
    }
    let entries = collect_bounded_directory_entries(
        fs::read_dir(root).map_err(|error| error.to_string())?,
        maximum,
        "protected generation directory count",
    )?;
    let mut names = BTreeSet::new();
    for entry in entries {
        let path = entry.path();
        reject_reparse_ancestors(&path)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "protected generation name is not Unicode".to_owned())?;
        if !valid_name(&name)
            || !entry
                .metadata()
                .map_err(|error| error.to_string())?
                .is_dir()
        {
            return Err("protected generation root contains an unexpected entry".to_owned());
        }
        names.insert(name);
    }
    Ok(names)
}

pub(super) fn safe_profile_generation(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn active_runtime_root(paths: &MachinePaths) -> Result<Option<PathBuf>, String> {
    let Some(bytes) = snapshot_optional_file(paths.runtime_pointer(), 64 * 1024)? else {
        return Ok(None);
    };
    let pointer: RuntimePointer =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if pointer.schema != 1 || !safe_version(&pointer.version) {
        return Err("invalid protected runtime pointer".to_owned());
    }
    let root = paths.runtime_versions().join(pointer.version);
    reject_reparse_ancestors(&root)?;
    if !root.is_dir() {
        return Err("protected runtime generation is unavailable".to_owned());
    }
    Ok(Some(root))
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
    fn generation_snapshot_rejects_the_maximum_plus_one_without_mutation() {
        const OVERSIZED_GENERATION_COUNT: usize = 4097;
        let root = TestDirectory::new("oversized-generation-root");
        for index in 0..OVERSIZED_GENERATION_COUNT {
            fs::create_dir(root.0.join(format!("1.0.{index}")))
                .expect("create generation directory");
        }

        let error = snapshot_generation_directories(
            &root.0,
            safe_version,
            MAX_PROTECTED_GENERATION_DIRECTORIES,
        )
        .expect_err("oversized generation root must fail closed");

        assert!(error.contains("generation directory count"), "{error}");
        assert_eq!(
            fs::read_dir(&root.0).expect("read generation root").count(),
            OVERSIZED_GENERATION_COUNT
        );
    }
}
