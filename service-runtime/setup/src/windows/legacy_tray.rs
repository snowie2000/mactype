use std::fs;
use std::path::Path;

use mactype_service_platform::{
    interactive_processes, known_folder_path, process_session_id, read_shortcut, ComApartment,
    ComThreading, KnownFolder, RegistryKey, RegistryRoot, RegistryValueData, RegistryView,
};

use crate::ConflictObservation;

const RUN_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
const MAX_STARTUP_TEXT_BYTES: u64 = 1_048_576;

pub(super) fn observe_conflict() -> ConflictObservation {
    combine([
        observe_interactive_processes(),
        observe_run_entries(),
        observe_startup_folders(),
    ])
}

fn combine(observations: impl IntoIterator<Item = ConflictObservation>) -> ConflictObservation {
    let mut unknown = false;
    for observation in observations {
        match observation {
            ConflictObservation::Detected => return ConflictObservation::Detected,
            ConflictObservation::Unknown => unknown = true,
            ConflictObservation::Clear => {}
        }
    }
    if unknown {
        ConflictObservation::Unknown
    } else {
        ConflictObservation::Clear
    }
}

fn observe_interactive_processes() -> ConflictObservation {
    let processes = match interactive_processes() {
        Ok(processes) => processes,
        Err(_) => return ConflictObservation::Unknown,
    };
    for entry in processes {
        let Some(name) = entry.name else {
            return ConflictObservation::Unknown;
        };
        if !name.eq_ignore_ascii_case("MacTray.exe") {
            continue;
        }
        let Ok(confirmed_session) = process_session_id(entry.pid) else {
            return ConflictObservation::Unknown;
        };
        if confirmed_session != entry.session_id {
            return ConflictObservation::Unknown;
        }
        if confirmed_session != 0 {
            return ConflictObservation::Detected;
        }
    }
    ConflictObservation::Clear
}

fn observe_run_entries() -> ConflictObservation {
    combine([
        observe_run_view(RegistryRoot::CurrentUser, RegistryView::Native32),
        observe_run_view(RegistryRoot::CurrentUser, RegistryView::Native64),
        observe_run_view(RegistryRoot::LocalMachine, RegistryView::Native32),
        observe_run_view(RegistryRoot::LocalMachine, RegistryView::Native64),
    ])
}

fn observe_run_view(root: RegistryRoot, view: RegistryView) -> ConflictObservation {
    let key = match RegistryKey::open(root, RUN_KEY, view) {
        Ok(Some(key)) => key,
        Ok(None) => return ConflictObservation::Clear,
        Err(_) => return ConflictObservation::Unknown,
    };
    let values = match key.values() {
        Ok(values) => values,
        Err(_) => return ConflictObservation::Unknown,
    };
    for value in values {
        let observation = classify_run_value(&value.name, &value.data);
        if observation != ConflictObservation::Clear {
            return observation;
        }
    }
    ConflictObservation::Clear
}

fn classify_run_value(value_name: &str, data: &RegistryValueData) -> ConflictObservation {
    match data {
        RegistryValueData::String(Some(command)) => {
            if contains_mactray_target(command) {
                ConflictObservation::Detected
            } else {
                ConflictObservation::Clear
            }
        }
        RegistryValueData::String(None) => ConflictObservation::Unknown,
        RegistryValueData::Dword(_) | RegistryValueData::Other { .. } => {
            if value_name.to_ascii_lowercase().contains("mactray") {
                ConflictObservation::Unknown
            } else {
                ConflictObservation::Clear
            }
        }
    }
}

fn observe_startup_folders() -> ConflictObservation {
    let _apartment = match ComApartment::initialize(ComThreading::Apartment) {
        Ok(apartment) => apartment,
        Err(_) => return ConflictObservation::Unknown,
    };
    observe_startup_folder(KnownFolder::Startup)
}

fn observe_startup_folder(folder: KnownFolder) -> ConflictObservation {
    let folder = match known_folder_path(folder) {
        Ok(folder) => folder,
        Err(_) => return ConflictObservation::Unknown,
    };
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(_) => return ConflictObservation::Unknown,
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => return ConflictObservation::Unknown,
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => return ConflictObservation::Unknown,
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if file_type.is_dir() {
            continue;
        }
        if file_type.is_symlink() {
            let target = match fs::canonicalize(&path) {
                Ok(target) => target,
                Err(_) => return ConflictObservation::Unknown,
            };
            if target
                .file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("MacTray.exe"))
            {
                return ConflictObservation::Detected;
            }
        }
        if file_type.is_file() && is_direct_mactray_executable_name(&name) {
            return ConflictObservation::Detected;
        }
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default();
        if extension.eq_ignore_ascii_case("lnk") {
            match inspect_shortcut(&path) {
                Ok(true) => return ConflictObservation::Detected,
                Ok(false) => {}
                Err(()) => return ConflictObservation::Unknown,
            }
        } else {
            match startup_text_might_launch_mactray(&path) {
                ConflictObservation::Detected => return ConflictObservation::Detected,
                ConflictObservation::Unknown => return ConflictObservation::Unknown,
                ConflictObservation::Clear => {}
            }
        }
    }
    ConflictObservation::Clear
}

fn is_direct_mactray_executable_name(value: &str) -> bool {
    value.eq_ignore_ascii_case("MacTray.exe")
}

fn inspect_shortcut(path: &Path) -> Result<bool, ()> {
    let target = read_shortcut(path).ok_or(())?;
    if target.path.trim().is_empty() {
        return Err(());
    }
    if contains_mactray_target(&target.path) {
        return Ok(true);
    }
    Ok(contains_mactray_target(&target.arguments))
}

fn startup_text_might_launch_mactray(path: &Path) -> ConflictObservation {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if !["bat", "cmd", "js", "ps1", "url", "vbs"]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    {
        return ConflictObservation::Clear;
    }
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return ConflictObservation::Unknown,
    };
    if metadata.len() > MAX_STARTUP_TEXT_BYTES {
        return ConflictObservation::Unknown;
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return ConflictObservation::Unknown,
    };
    let ascii = String::from_utf8_lossy(&bytes);
    if contains_mactray_target(&ascii) {
        return ConflictObservation::Detected;
    }
    if bytes.len() % 2 == 0 {
        let units = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        if contains_mactray_target(&String::from_utf16_lossy(&units)) {
            return ConflictObservation::Detected;
        }
    }
    ConflictObservation::Clear
}

fn contains_mactray_target(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let needle = "mactray.exe";
    lower.match_indices(needle).any(|(start, _)| {
        let before = lower[..start].chars().next_back();
        let after = lower[start + needle.len()..].chars().next();
        before.map_or(true, target_boundary) && after.map_or(true, target_boundary)
    })
}

fn target_boundary(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '\\' | '/' | '"' | '\'' | '=' | ',' | ';' | '(' | ')'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_value_name_alone_is_not_mactray_target_evidence() {
        assert_eq!(
            classify_run_value(
                "MacTray",
                &RegistryValueData::String(Some(
                    r#""C:\Program Files\Other\Helper.exe""#.to_owned()
                ))
            ),
            ConflictObservation::Clear
        );
        assert_eq!(
            classify_run_value(
                "Unrelated",
                &RegistryValueData::String(Some(
                    r#""C:\Program Files\MacType\MacTray.exe""#.to_owned()
                )),
            ),
            ConflictObservation::Detected
        );
    }

    #[test]
    fn startup_filename_alone_only_identifies_the_direct_binary() {
        assert!(is_direct_mactray_executable_name("MacTray.exe"));
        assert!(is_direct_mactray_executable_name("mactray.EXE"));
        assert!(!is_direct_mactray_executable_name("MacTray.lnk"));
        assert!(!is_direct_mactray_executable_name(
            "MacTray migration notes.txt"
        ));
        assert!(!is_direct_mactray_executable_name("NotMacTray.exe"));
    }
}
