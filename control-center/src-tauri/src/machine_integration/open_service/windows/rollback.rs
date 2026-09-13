use super::*;

pub(super) fn rollback_open_service_snapshot(
    snapshot: &SystemOpenServiceSnapshot,
) -> Result<(), String> {
    let current = query();
    let controllable = current.installation == InstallationState::Absent
        || (current.backend == ServiceBackend::OpenSource
            && matches!(
                current.installation,
                InstallationState::Current | InstallationState::Outdated
            ));
    if !controllable {
        return Err(
            "the fixed service name became foreign or inaccessible; SCM rollback was refused"
                .to_owned(),
        );
    }
    if matches!(
        current.runtime,
        RuntimeState::StartPending | RuntimeState::StopPending
    ) {
        return Err("the open service is transitional during rollback".to_owned());
    }
    if current.runtime == RuntimeState::Running {
        run_setup(SystemServiceAction::Stop, None)?;
    }
    if snapshot.status.installation == InstallationState::Absent
        && query().installation != InstallationState::Absent
    {
        run_setup(SystemServiceAction::Remove, None)?;
    }

    restore_protected_snapshot(snapshot)?;
    match snapshot.status.installation {
        InstallationState::Absent => {}
        InstallationState::Current | InstallationState::Outdated => {
            run_restore_pinned_runtime()?;
            if snapshot.status.runtime == RuntimeState::Running {
                run_setup(SystemServiceAction::Start, None)?;
            }
        }
        _ => return Err("the saved open service state is unsafe to restore".to_owned()),
    }
    release_migration_runtime_pin(snapshot)?;
    cleanup_snapshot_roots(snapshot)?;
    Ok(())
}

fn restore_protected_snapshot(snapshot: &SystemOpenServiceSnapshot) -> Result<(), String> {
    let paths = fixed_machine_paths()?;
    let receipt = snapshot
        .mutations
        .lock()
        .map_err(|_| "protected mutation receipt lock is poisoned".to_owned())?
        .clone();
    restore_receipted_optional_file(
        paths.runtime_pointer(),
        snapshot.runtime_pointer.as_deref(),
        receipt.runtime_pointer.as_ref(),
        64 * 1024,
    )?;
    restore_receipted_optional_file(
        paths.active_profile(),
        snapshot.active_profile_pointer.as_deref(),
        receipt.active_profile_pointer.as_ref(),
        64 * 1024,
    )?;
    restore_receipted_optional_file(
        paths.previous_profile(),
        snapshot.previous_profile_pointer.as_deref(),
        receipt.previous_profile_pointer.as_ref(),
        64 * 1024,
    )?;
    remove_receipted_generation_directories(
        paths.runtime_versions(),
        &snapshot.runtime_generations,
        &receipt.runtime_generations,
        safe_version,
        MAX_RUNTIME_GENERATION_FILES,
    )?;
    remove_receipted_generation_directories(
        paths.profile_generations(),
        &snapshot.profile_generations,
        &receipt.profile_generations,
        safe_profile_generation,
        MAX_PROFILE_GENERATION_FILES,
    )?;
    if let Some((path, state)) = &snapshot.adjacent_profile {
        restore_receipted_optional_file(
            path,
            state.as_deref(),
            receipt.adjacent_profile.as_ref(),
            MAX_PROFILE_BYTES as u64,
        )?;
    }
    Ok(())
}

fn cleanup_snapshot_roots(snapshot: &SystemOpenServiceSnapshot) -> Result<(), String> {
    let paths = fixed_machine_paths()?;
    if !snapshot.runtime_versions_existed {
        remove_empty_directory(paths.runtime_versions())?;
    }
    if !snapshot.profile_generations_existed {
        remove_empty_directory(paths.profile_generations())?;
    }
    if !snapshot.service_root_existed {
        remove_empty_directory(paths.service_root())?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::machine_integration::open_service) enum FileRollbackAction {
    Noop,
    Restore,
    Remove,
}

pub(in crate::machine_integration::open_service) fn plan_file_rollback(
    before: Option<&[u8]>,
    recorded_after: Option<Option<&[u8]>>,
    current: Option<&[u8]>,
) -> Result<FileRollbackAction, String> {
    if current == before {
        return Ok(FileRollbackAction::Noop);
    }
    if recorded_after.is_some_and(|after| after == current) {
        return Ok(if before.is_some() {
            FileRollbackAction::Restore
        } else {
            FileRollbackAction::Remove
        });
    }
    Err(
        "protected state cleanup is unknown; current bytes do not match this transaction receipt"
            .to_owned(),
    )
}

pub(in crate::machine_integration::open_service) fn plan_generation_cleanup(
    before: &BTreeSet<String>,
    receipts: &BTreeMap<String, BTreeMap<String, String>>,
    current: &BTreeMap<String, BTreeMap<String, String>>,
) -> Result<Vec<String>, String> {
    let mut removable = Vec::new();
    for (name, manifest) in current {
        if before.contains(name) {
            continue;
        }
        if receipts.get(name) != Some(manifest) {
            return Err(format!(
                "protected generation cleanup is unknown for {name}; refusing recursive deletion"
            ));
        }
        removable.push(name.clone());
    }
    Ok(removable)
}

fn restore_receipted_optional_file(
    path: &Path,
    before: Option<&[u8]>,
    receipt: Option<&OptionalFileMutation>,
    maximum: u64,
) -> Result<(), String> {
    let current = snapshot_optional_file(path, maximum).map_err(|error| {
        format!(
            "protected file cleanup is unknown for {}: {error}",
            path.display()
        )
    })?;
    match plan_file_rollback(
        before,
        receipt.map(|mutation| mutation.after.as_deref()),
        current.as_deref(),
    )? {
        FileRollbackAction::Noop => Ok(()),
        FileRollbackAction::Restore => restore_optional_file(path, before),
        FileRollbackAction::Remove => restore_optional_file(path, None),
    }
}

fn restore_optional_file(path: &Path, bytes: Option<&[u8]>) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "protected state path has no parent".to_owned())?;
    if let Some(bytes) = bytes {
        reject_reparse_ancestors(parent)?;
        if path.exists() {
            reject_reparse_ancestors(path)?;
        }
        let temporary = parent.join(format!(
            ".rollback-{}-{}.tmp",
            std::process::id(),
            path.file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("protected-state")
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        replace_file_atomic(&temporary, path).inspect_err(|_| {
            let _ = fs::remove_file(&temporary);
        })
    } else if path.exists() {
        reject_reparse_ancestors(path)?;
        if !fs::metadata(path)
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err("protected state path is not a regular file".to_owned());
        }
        fs::remove_file(path).map_err(|error| error.to_string())
    } else {
        Ok(())
    }
}

fn replace_file_atomic(source: &Path, destination: &Path) -> Result<(), String> {
    mactype_service_platform::replace_file(source, destination).map_err(|error| error.to_string())
}

fn remove_receipted_generation_directories(
    root: &Path,
    before: &BTreeSet<String>,
    receipts: &BTreeMap<String, BTreeMap<String, String>>,
    valid_name: fn(&str) -> bool,
    maximum_files: usize,
) -> Result<(), String> {
    let names =
        snapshot_generation_directories(root, valid_name, MAX_PROTECTED_GENERATION_DIRECTORIES)
            .map_err(|error| {
                format!(
                    "protected generation cleanup is unknown under {}: {error}",
                    root.display()
                )
            })?;
    let mut current = BTreeMap::new();
    for name in names {
        let manifest = if before.contains(&name) {
            BTreeMap::new()
        } else {
            generation_manifest(&root.join(&name), maximum_files).map_err(|error| {
                format!("protected generation cleanup is unknown for {name}: {error}")
            })?
        };
        current.insert(name, manifest);
    }
    let removable = plan_generation_cleanup(before, receipts, &current)?;
    for name in removable {
        let expected = receipts
            .get(&name)
            .ok_or_else(|| "protected generation receipt disappeared".to_owned())?;
        let path = root.join(&name);
        reject_reparse_ancestors(&path)?;
        if &generation_manifest(&path, maximum_files)? != expected {
            return Err(format!(
                "protected generation cleanup is unknown for {}; manifest changed before deletion",
                path.display()
            ));
        }
        fs::remove_dir_all(path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(in crate::machine_integration::open_service) fn remove_empty_directory(
    path: &Path,
) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    reject_reparse_ancestors(path)?;
    if fs::read_dir(path)
        .map_err(|error| error.to_string())?
        .next()
        .is_none()
    {
        fs::remove_dir(path).map_err(|error| error.to_string())?;
        Ok(())
    } else {
        Err(format!(
            "protected directory cleanup is unknown for {}; refusing to remove unreceipted content",
            path.display()
        ))
    }
}
