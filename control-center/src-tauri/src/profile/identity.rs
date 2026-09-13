use super::{legacy::user_profile_root, ProfileLocation};
use crate::installation_root;
use std::{
    fs::{self, OpenOptions},
    path::{Component, Path, PathBuf},
};

pub(super) struct ProfileIdentity {
    pub(super) display_path: String,
    pub(super) location: ProfileLocation,
    pub(super) can_save: bool,
}

pub(super) fn identify_profile(path: &Path) -> ProfileIdentity {
    identify_profile_at(
        path,
        installation_root().as_deref(),
        user_profile_root().as_deref(),
    )
}

pub(super) fn identify_profile_at(
    path: &Path,
    installation: Option<&Path>,
    personal: Option<&Path>,
) -> ProfileIdentity {
    let (display_path, location) = profile_reference_at(path, installation, personal);
    ProfileIdentity {
        display_path,
        location,
        can_save: location != ProfileLocation::External && can_write_file(path),
    }
}

pub(crate) fn source_profile_reference(installation: &Path, path: &Path) -> PathBuf {
    PathBuf::from(profile_reference_at(path, Some(installation), user_profile_root().as_deref()).0)
}

fn profile_reference_at(
    path: &Path,
    installation: Option<&Path>,
    personal: Option<&Path>,
) -> (String, ProfileLocation) {
    if let Some(relative) = installation.and_then(|root| relative_to(path, root)) {
        return (windows_relative(&relative), ProfileLocation::Installation);
    }
    if let Some(relative) = personal.and_then(|root| relative_to(path, root)) {
        let relative = windows_relative(&relative);
        return (
            if relative.is_empty() {
                "Profiles".to_owned()
            } else {
                format!(r"Profiles\{relative}")
            },
            ProfileLocation::Personal,
        );
    }
    (
        path.to_string_lossy().into_owned(),
        ProfileLocation::External,
    )
}

fn relative_to(path: &Path, root: &Path) -> Option<PathBuf> {
    relative_suffix(path, root).or_else(|| {
        let resolved_path = fs::canonicalize(path).ok()?;
        let resolved_root = fs::canonicalize(root).ok()?;
        relative_suffix(&resolved_path, &resolved_root)
    })
}

fn relative_suffix(path: &Path, root: &Path) -> Option<PathBuf> {
    path.strip_prefix(root)
        .ok()
        .filter(|relative| {
            relative
                .components()
                .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
        })
        .map(Path::to_path_buf)
}

fn windows_relative(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(r"\")
}

fn can_write_file(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| {
        metadata.is_file()
            && !metadata.permissions().readonly()
            && OpenOptions::new().write(true).open(path).is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("mactype-{label}-{unique}"))
    }

    #[test]
    fn installed_and_personal_profiles_receive_portable_references() {
        let root = test_root("profile-reference");
        let installation = root.join("MacType");
        let personal = root.join("Personal");
        let installed = installation.join("ini").join("Default.ini");
        let copied = personal.join("Community.ini");
        fs::create_dir_all(installed.parent().unwrap()).unwrap();
        fs::create_dir_all(&personal).unwrap();
        fs::write(&installed, b"[General]\n").unwrap();
        fs::write(&copied, b"[General]\n").unwrap();

        let installed_identity =
            identify_profile_at(&installed, Some(&installation), Some(&personal));
        let copied_identity = identify_profile_at(&copied, Some(&installation), Some(&personal));

        assert_eq!(installed_identity.display_path, r"ini\Default.ini");
        assert_eq!(installed_identity.location, ProfileLocation::Installation);
        assert!(installed_identity.can_save);
        assert_eq!(copied_identity.display_path, r"Profiles\Community.ini");
        assert_eq!(copied_identity.location, ProfileLocation::Personal);
        assert!(copied_identity.can_save);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[allow(clippy::permissions_set_readonly_false)] // Clearing a Windows read-only test file does not widen Unix modes.
    fn external_and_read_only_profiles_require_an_explicit_copy() {
        let root = test_root("profile-copy-required");
        let installation = root.join("MacType");
        let personal = root.join("Personal");
        let read_only = installation.join("ini").join("Locked.ini");
        let external = root.join("Downloads").join("External.ini");
        fs::create_dir_all(read_only.parent().unwrap()).unwrap();
        fs::create_dir_all(external.parent().unwrap()).unwrap();
        fs::write(&read_only, b"[General]\n").unwrap();
        fs::write(&external, b"[General]\n").unwrap();
        let mut permissions = fs::metadata(&read_only).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&read_only, permissions).unwrap();

        let read_only_identity =
            identify_profile_at(&read_only, Some(&installation), Some(&personal));
        let external_identity =
            identify_profile_at(&external, Some(&installation), Some(&personal));

        assert_eq!(read_only_identity.display_path, r"ini\Locked.ini");
        assert!(!read_only_identity.can_save);
        assert_eq!(external_identity.location, ProfileLocation::External);
        assert!(!external_identity.can_save);
        #[cfg(windows)]
        {
            let mut permissions = fs::metadata(&read_only).unwrap().permissions();
            permissions.set_readonly(false);
            fs::set_permissions(&read_only, permissions).unwrap();
        }
        fs::remove_dir_all(root).unwrap();
    }
}
