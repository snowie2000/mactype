use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualLaunchCandidate {
    pub pid: u32,
    pub name: String,
    pub path: String,
    pub window_title: Option<String>,
}

pub(super) fn list_manual_launch_candidates_impl() -> Result<Vec<ManualLaunchCandidate>, String> {
    #[cfg(windows)]
    {
        windows::list_candidates()
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

fn order_candidates(mut candidates: Vec<ManualLaunchCandidate>) -> Vec<ManualLaunchCandidate> {
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|candidate| seen.insert(candidate.pid));
    candidates.sort_by(|left, right| {
        right
            .window_title
            .is_some()
            .cmp(&left.window_title.is_some())
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.pid.cmp(&right.pid))
    });
    candidates
}

#[cfg_attr(not(windows), allow(dead_code))]
fn is_under_directory(path: &str, root: &str) -> bool {
    let normalized_path = path.replace('/', "\\").to_lowercase();
    let normalized_root = root.replace('/', "\\").to_lowercase();
    let normalized_root = normalized_root.trim_end_matches('\\');
    if normalized_root.is_empty() {
        return false;
    }
    normalized_path == normalized_root
        || normalized_path.starts_with(&format!("{normalized_root}\\"))
}

#[cfg(windows)]
mod windows {
    use super::{is_under_directory, order_candidates, ManualLaunchCandidate};
    use mactype_service_platform::{
        interactive_processes, process_session_id, top_level_windows, Process, ProcessAccess,
    };
    use std::{collections::HashMap, env};

    pub(super) fn list_candidates() -> Result<Vec<ManualLaunchCandidate>, String> {
        let current_pid = std::process::id();
        let current_session = process_session_id(current_pid)
            .map_err(|_| "the current session could not be identified".to_owned())?;
        let window_titles = visible_window_titles();
        let windows_directory = env::var_os("WINDIR")
            .or_else(|| env::var_os("SystemRoot"))
            .map(|value| value.to_string_lossy().into_owned());
        let processes = interactive_processes()
            .map_err(|_| "the running process inventory could not be enumerated".to_owned())?;
        let mut candidates = Vec::new();
        for process in processes {
            if process.pid == 0 || process.pid == 4 || process.pid == current_pid {
                continue;
            }
            if process.session_id != current_session {
                continue;
            }
            let Some(path) = Process::open(process.pid, ProcessAccess::QueryLimited)
                .ok()
                .and_then(|process| process.image_path())
                .map(|path| path.to_string_lossy().into_owned())
            else {
                continue;
            };
            if windows_directory
                .as_deref()
                .is_some_and(|root| is_under_directory(&path, root))
            {
                continue;
            }
            let name = path
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or(path.as_str())
                .to_owned();
            candidates.push(ManualLaunchCandidate {
                pid: process.pid,
                name,
                path,
                window_title: window_titles.get(&process.pid).cloned(),
            });
        }
        Ok(order_candidates(candidates))
    }

    /// Maps each owning PID to the title of its topmost visible top-level
    /// window; tool windows and untitled windows never contribute a title.
    fn visible_window_titles() -> HashMap<u32, String> {
        let Ok(windows) = top_level_windows(512) else {
            return HashMap::new();
        };
        let mut titles = HashMap::new();
        for window in windows {
            if window.process_id == 0 || !window.visible || window.tool_window {
                continue;
            }
            let Some(title) = window.title.filter(|title| !title.trim().is_empty()) else {
                continue;
            };
            titles.entry(window.process_id).or_insert(title);
        }
        titles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(pid: u32, name: &str, window_title: Option<&str>) -> ManualLaunchCandidate {
        ManualLaunchCandidate {
            pid,
            name: name.to_owned(),
            path: format!("C:\\Tools\\{name}"),
            window_title: window_title.map(str::to_owned),
        }
    }

    #[test]
    fn candidates_list_windowed_processes_first_in_alphabetical_groups() {
        let ordered = order_candidates(vec![
            candidate(10, "zulu.exe", None),
            candidate(20, "notepad.exe", Some("Untitled - Notepad")),
            candidate(30, "alpha.exe", None),
            candidate(40, "Code.exe", Some("Visual Studio Code")),
        ]);

        let names: Vec<&str> = ordered.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["Code.exe", "notepad.exe", "alpha.exe", "zulu.exe"]);
    }

    #[test]
    fn candidates_are_deduplicated_by_pid() {
        let ordered = order_candidates(vec![
            candidate(20, "notepad.exe", Some("Untitled - Notepad")),
            candidate(20, "notepad.exe", None),
        ]);

        assert_eq!(ordered.len(), 1);
        assert_eq!(
            ordered[0].window_title.as_deref(),
            Some("Untitled - Notepad")
        );
    }

    #[test]
    fn windows_directory_exclusion_is_case_insensitive_and_boundary_safe() {
        assert!(is_under_directory(
            "C:\\Windows\\System32\\notepad.exe",
            "C:\\WINDOWS"
        ));
        assert!(is_under_directory(
            "C:/windows/explorer.exe",
            "C:\\Windows\\"
        ));
        assert!(!is_under_directory(
            "C:\\WindowsApps\\tool.exe",
            "C:\\Windows"
        ));
        assert!(!is_under_directory("C:\\Tools\\notepad.exe", "C:\\Windows"));
        assert!(!is_under_directory("C:\\Tools\\notepad.exe", ""));
    }
}
