use super::{
    identity::identify_profile,
    legacy::{
        discover_legacy_profile_at, find_default_profile, import_profile_to, user_profile_root,
    },
    AdvancedProfile, IndividualSetting, LegacyProfileCandidate, ProfileDocument, ProfileEntry,
    ProfileLocation, ProfileSnapshot, ProfileState, MAX_PROFILE_DIRECTORY_ENTRIES,
};
use crate::{bounded_io::read_bounded_file, execution, installation_root};
use std::{env, fs, path::Path, path::PathBuf};

pub(super) fn list_profiles() -> Result<Vec<ProfileEntry>, String> {
    let root =
        installation_root().ok_or_else(|| "MacType installation was not found".to_owned())?;
    let user_root = user_profile_root();
    let paths = list_profile_paths_from(&root, user_root.as_deref())?;
    let mut profiles = paths
        .into_iter()
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
        })
        .map(|path| ProfileEntry {
            display_path: identify_profile(&path).display_path,
            name: path
                .file_stem()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: path.to_string_lossy().into_owned(),
        })
        .collect::<Vec<_>>();
    profiles.sort_by_key(|profile| profile.name.to_lowercase());
    Ok(profiles)
}

pub(super) fn list_profile_paths_from(
    installation: &Path,
    user_root: Option<&Path>,
) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    let mut entry_count = 0;
    append_profile_directory(&installation.join("ini"), &mut paths, &mut entry_count)?;
    let root_profile = installation.join("MacType.ini");
    if paths.is_empty() && root_profile.is_file() {
        paths.push(root_profile);
    }
    if let Some(user_root) = user_root {
        append_profile_directory(user_root, &mut paths, &mut entry_count)?;
    }
    Ok(paths)
}

fn append_profile_directory(
    directory: &Path,
    paths: &mut Vec<PathBuf>,
    entry_count: &mut usize,
) -> Result<(), String> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Ok(());
    };
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        *entry_count += 1;
        if *entry_count > MAX_PROFILE_DIRECTORY_ENTRIES {
            return Err(format!(
                "profile directories contain more than {MAX_PROFILE_DIRECTORY_ENTRIES} entries"
            ));
        }
        paths.push(entry.path());
    }
    Ok(())
}

pub(super) fn open_profile(path: String, state: &ProfileState) -> Result<ProfileSnapshot, String> {
    let document = ProfileDocument::open(PathBuf::from(path))?;
    let snapshot = document.snapshot();
    state.set(document)?;
    Ok(snapshot)
}

pub(super) fn open_default_profile(
    state: &ProfileState,
) -> Result<Option<ProfileSnapshot>, String> {
    let Some(path) = find_default_profile()? else {
        return Ok(None);
    };
    open_profile(path.to_string_lossy().into_owned(), state).map(Some)
}

pub(super) fn current_profile(state: &ProfileState) -> Result<Option<ProfileSnapshot>, String> {
    state.snapshot()
}

pub(super) fn discover_legacy_profile() -> Result<Option<LegacyProfileCandidate>, String> {
    let Some(root) = installation_root() else {
        return Ok(None);
    };
    discover_legacy_profile_at(&root)
}

pub(super) fn import_profile(
    path: String,
    state: &ProfileState,
) -> Result<ProfileSnapshot, String> {
    let directory =
        user_profile_root().ok_or_else(|| "LOCALAPPDATA is not available".to_owned())?;
    let document = import_profile_to(Path::new(&path), &directory)?;
    let snapshot = document.snapshot();
    state.set(document)?;
    Ok(snapshot)
}

pub(super) fn update_profile_setting(
    setting_id: String,
    value: f64,
    state: &ProfileState,
) -> Result<ProfileSnapshot, String> {
    state.edit(|document| {
        document.update_value(&setting_id, value)?;
        Ok(document.snapshot())
    })
}

pub(super) fn update_profile_individuals(
    entries: Vec<IndividualSetting>,
    state: &ProfileState,
) -> Result<ProfileSnapshot, String> {
    state.edit(|document| {
        document.update_individuals(entries)?;
        Ok(document.snapshot())
    })
}

pub(super) fn update_profile_list(
    kind: String,
    entries: Vec<String>,
    state: &ProfileState,
) -> Result<ProfileSnapshot, String> {
    state.edit(|document| {
        document.update_list(&kind, entries)?;
        Ok(document.snapshot())
    })
}

pub(super) fn update_profile_advanced(
    advanced: AdvancedProfile,
    state: &ProfileState,
) -> Result<ProfileSnapshot, String> {
    state.edit(|document| {
        document.update_advanced(advanced)?;
        Ok(document.snapshot())
    })
}

pub(super) fn duplicate_profile(
    name: String,
    state: &ProfileState,
) -> Result<ProfileSnapshot, String> {
    let directory =
        user_profile_root().ok_or_else(|| "LOCALAPPDATA is not available".to_owned())?;
    state.replace_from(|current| {
        let duplicate = current.duplicate_in(&directory, &name)?;
        let snapshot = duplicate.snapshot();
        Ok((duplicate, snapshot))
    })
}

pub(super) fn save_profile(state: &ProfileState) -> Result<ProfileSnapshot, String> {
    state.edit(|document| {
        let identity = identify_profile(document.path());
        if !identity.can_save {
            return Err(match identity.location {
                ProfileLocation::External => {
                    "external profiles must be imported or saved as a managed profile before saving"
                        .to_owned()
                }
                ProfileLocation::Installation | ProfileLocation::Personal => {
                    "the original profile is read-only; use Save As before saving changes"
                        .to_owned()
                }
            });
        }
        document.save()?;
        Ok(document.snapshot())
    })
}

pub(super) fn undo_profile(state: &ProfileState) -> Result<ProfileSnapshot, String> {
    state.edit(|document| {
        document.undo();
        Ok(document.snapshot())
    })
}

pub(super) fn redo_profile(state: &ProfileState) -> Result<ProfileSnapshot, String> {
    state.edit(|document| {
        document.redo();
        Ok(document.snapshot())
    })
}

pub(super) fn reset_profile_defaults(state: &ProfileState) -> Result<ProfileSnapshot, String> {
    state.edit(|document| {
        document.reset_settings_to_defaults()?;
        Ok(document.snapshot())
    })
}

pub(super) fn discard_profile_changes(state: &ProfileState) -> Result<ProfileSnapshot, String> {
    state.replace_from(|current| {
        let document = ProfileDocument::open(current.path())?;
        let snapshot = document.snapshot();
        Ok((document, snapshot))
    })
}

pub(super) fn export_profile(path: String, state: &ProfileState) -> Result<String, String> {
    let destination = PathBuf::from(path);
    state.read(|document| document.export_to(&destination))?;
    Ok(destination.to_string_lossy().into_owned())
}

pub(super) fn reveal_profile_file(state: &ProfileState) -> Result<String, String> {
    let path = state.read(|document| Ok(document.path().to_path_buf()))?;
    reveal_file(&path)?;
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(windows)]
fn reveal_file(path: &Path) -> Result<(), String> {
    std::process::Command::new("explorer.exe")
        .arg("/select,")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn reveal_file(_path: &Path) -> Result<(), String> {
    Err("revealing a profile file is supported only on Windows".to_owned())
}

pub(super) fn ci_verify_profile_workflow(state: &ProfileState) -> Result<(), String> {
    if env::var_os("MACTYPE_CI_SMOKE_FILE").is_none() {
        return Err(
            "profile workflow verification is available only during CI smoke tests".to_owned(),
        );
    }
    let name = format!("phase3-{}", std::process::id());
    let original_path = state.read(|current| Ok(current.path().to_path_buf()))?;
    let duplicated = duplicate_profile(name.clone(), state)?;
    let path = PathBuf::from(&duplicated.path);
    update_profile_setting("normal_weight".to_owned(), 7.0, state)?;
    update_profile_individuals(
        vec![IndividualSetting {
            font_face: "CI Test Font".to_owned(),
            values: vec![Some(1), Some(2), None, Some(3), None, Some(1)],
        }],
        state,
    )?;
    update_profile_list(
        "excludeModules".to_owned(),
        vec!["ci-test.exe".to_owned()],
        state,
    )?;
    let saved = save_profile(state)?;
    if saved.path != duplicated.path || saved.display_path != format!(r"Profiles\{name}.ini") {
        return Err("direct save did not preserve the personal profile identity".to_owned());
    }
    let reopened_document = ProfileDocument::open(&path)?;
    let reopened = reopened_document.snapshot();
    if reopened.values.get("normal_weight") != Some(&7.0)
        || reopened.individuals.len() != 1
        || reopened.lists.exclude_modules != vec!["ci-test.exe".to_owned()]
    {
        return Err("saved Phase 3 profile did not reopen with the expected values".to_owned());
    }
    let installation =
        installation_root().ok_or_else(|| "MacType installation was not found".to_owned())?;
    let (source_path, encoded) = state.active_payload()?;
    let applied = execution::apply_profile(&installation, &source_path, &encoded)?;
    let applied_root = PathBuf::from(&applied.runtime_root);
    let applied_profile = read_bounded_file(
        &applied_root.join("profile.ini"),
        mactype_service_contract::MAX_PROFILE_BYTES,
        "CI applied profile",
    )?;
    let runtime_configuration = read_bounded_file(
        &applied_root.join("MacType.ini"),
        64 * 1024,
        "CI runtime configuration",
    )?;
    if applied_profile != encoded
        || runtime_configuration != b"[General]\r\nAlternativeFile=profile.ini\r\n"
        || !applied_root.join("MacLoader.exe").is_file()
        || !applied_root.join("MacType.dll").is_file()
    {
        return Err(
            "applied profile runtime is incomplete or does not preserve the profile".to_owned(),
        );
    }
    if applied.source_profile != format!(r"Profiles\{name}.ini") {
        return Err("applied profile did not retain its portable profile identity".to_owned());
    }
    open_profile(original_path.to_string_lossy().into_owned(), state)?;
    fs::remove_file(&path).map_err(|error| error.to_string())?;
    let backup = path.with_extension("ini.bak");
    if backup.exists() {
        fs::remove_file(backup).map_err(|error| error.to_string())?;
    }
    Ok(())
}
