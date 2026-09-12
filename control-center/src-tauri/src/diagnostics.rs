mod operation_log;

use crate::preview::{PreviewDiagnosticSnapshot, PreviewState};
use mactype_service_contract::event_log::{
    read_events, EventArea, EventRecord, EventSeverity, EventSource,
};
pub(crate) use operation_log::{
    record_activity, record_operation_failure, ActivityKind, InstallationPreflightDiagnostics,
    OperationFailure,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::State;

pub fn log_root() -> Result<PathBuf, String> {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("MacType").join("ControlCenter").join("logs"))
        .ok_or_else(|| "LOCALAPPDATA is not available".to_owned())
}

fn event_paths() -> Result<Vec<PathBuf>, String> {
    let mut paths = vec![log_root()?.join("control-center.log")];
    #[cfg(windows)]
    {
        let (program_files, program_data) = crate::machine_integration::machine_roots()?;
        let machine = mactype_service_contract::MachinePaths::from_trusted_os_roots(
            &program_files,
            &program_data,
        )
        .map_err(|error| error.to_string())?;
        paths.push(machine.service_host_event_log().to_owned());
        paths.push(machine.service_setup_event_log().to_owned());
    }
    Ok(paths)
}

fn read_all_events(paths: &[PathBuf], limit: usize) -> Vec<EventRecord> {
    if limit == 0 || paths.is_empty() {
        return Vec::new();
    }
    let local_root = paths[0].parent().unwrap_or_else(|| Path::new(""));
    let mut events = operation_log::read_all_at(local_root);
    events.extend(read_events(&paths[1..], usize::MAX));
    events.sort_by_key(|event| event.ts);
    let start = events.len().saturating_sub(limit);
    events.split_off(start)
}

fn export_to(directory: &Path, report: &str, timestamp: u128) -> Result<PathBuf, String> {
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let destination = directory.join(format!("diagnostics-{timestamp}.txt"));
    let temporary = directory.join(format!(".diagnostics-{timestamp}.tmp"));
    fs::write(&temporary, report.as_bytes()).map_err(|error| error.to_string())?;
    fs::rename(&temporary, &destination).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        error.to_string()
    })?;
    Ok(destination)
}

pub fn export(report: &str) -> Result<String, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    export_to(&log_root()?, report, timestamp).map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
pub(crate) fn open_log_folder() -> Result<String, String> {
    let directory = log_root()?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    #[cfg(windows)]
    Command::new("explorer.exe")
        .arg(&directory)
        .spawn()
        .map_err(|error| error.to_string())?;
    #[cfg(not(windows))]
    return Err("opening the log folder is supported only on Windows".to_owned());
    Ok(directory.to_string_lossy().into_owned())
}

#[cfg(windows)]
pub fn copy_to_clipboard(report: &str) -> Result<(), String> {
    mactype_service_platform::set_clipboard_unicode_text(report)
        .map_err(|error| format!("clipboard: {error}"))
}

#[cfg(not(windows))]
pub fn copy_to_clipboard(_report: &str) -> Result<(), String> {
    Err("copying diagnostics is supported only on Windows".to_owned())
}

fn diagnostic_report_text(snapshot: PreviewDiagnosticSnapshot) -> String {
    let status = snapshot.status;
    let mut report = String::from("MacType Control Center diagnostics\n");
    report.push_str(&format!(
        "controlCenterVersion={}\n",
        env!("CARGO_PKG_VERSION")
    ));
    report.push_str(&format!("os={}\n", env::consts::OS));
    report.push_str(&format!("arch={}\n", env::consts::ARCH));
    report.push_str(&format!("state={}\n", status.state));
    report.push_str(&format!(
        "installationRoot={}\n",
        status.root.as_deref().unwrap_or("not-found")
    ));
    report.push_str(&format!(
        "coreVersion={}\n",
        status.core_version.as_deref().unwrap_or("unknown")
    ));
    for finding in status.findings {
        report.push_str(&format!(
            "finding.{}={} ({})\n",
            finding.label,
            finding.value,
            if finding.ok { "ok" } else { "failed" }
        ));
    }
    let entries = snapshot.entries;
    report.push_str(&format!("previewLogEntries={}\n", entries.len()));
    for entry in entries.iter().rev().take(20).rev() {
        report.push_str("previewLog=");
        report.push_str(&entry.replace(['\r', '\n'], " "));
        report.push('\n');
    }
    report
}

#[tauri::command]
pub(crate) fn diagnostic_report(state: State<'_, PreviewState>) -> Result<String, String> {
    let mut report = diagnostic_report_text(state.diagnostic_snapshot()?);
    let paths = event_paths()?;
    let events = read_all_events(&paths, 200);
    report.push_str(&format!("eventLogEntries={}\n", events.len()));
    for event in events {
        report.push_str("eventLog=");
        report.push_str(&render_event(&event));
        report.push('\n');
    }
    for status in source_statuses(&paths) {
        report.push_str(&format!(
            "eventSource={} path={} readable={} bytes={}\n",
            source_name(status.source),
            status.path,
            status.readable,
            status.bytes
        ));
    }
    Ok(report)
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EventFilter {
    severities: Option<Vec<EventSeverity>>,
    areas: Option<Vec<EventArea>>,
    since_unix_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EventSourceStatus {
    source: EventSource,
    path: String,
    readable: bool,
    bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EventLogSummary {
    total: u32,
    warnings: u32,
    errors: u32,
    newest_ts: Option<u64>,
    sources: Vec<EventSourceStatus>,
}

#[tauri::command]
pub(crate) fn list_events(
    filter: Option<EventFilter>,
    limit: Option<u32>,
) -> Result<Vec<EventRecord>, String> {
    let paths = event_paths()?;
    let maximum = limit.unwrap_or(200).min(1000) as usize;
    let filter = filter.unwrap_or_default();
    let mut events = read_all_events(&paths, usize::MAX)
        .into_iter()
        .filter(|event| {
            filter
                .severities
                .as_ref()
                .map_or(true, |values| values.contains(&event.severity))
                && filter
                    .areas
                    .as_ref()
                    .map_or(true, |values| values.contains(&event.area))
                && filter.since_unix_ms.map_or(true, |since| event.ts >= since)
        })
        .collect::<Vec<_>>();
    let start = events.len().saturating_sub(maximum);
    Ok(events.split_off(start))
}

#[tauri::command]
pub(crate) fn event_log_summary() -> Result<EventLogSummary, String> {
    const SEVEN_DAYS_MS: u64 = 7 * 24 * 60 * 60 * 1000;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis() as u64;
    let paths = event_paths()?;
    let events = read_all_events(&paths, usize::MAX)
        .into_iter()
        .filter(|event| event.ts >= now.saturating_sub(SEVEN_DAYS_MS))
        .collect::<Vec<_>>();
    Ok(EventLogSummary {
        total: events.len() as u32,
        warnings: events
            .iter()
            .filter(|event| event.severity == EventSeverity::Warning)
            .count() as u32,
        errors: events
            .iter()
            .filter(|event| event.severity == EventSeverity::Error)
            .count() as u32,
        newest_ts: events.last().map(|event| event.ts),
        sources: source_statuses(&paths),
    })
}

#[tauri::command]
pub(crate) fn diagnostic_recent_logs() -> Result<Vec<String>, String> {
    Ok(list_events(None, Some(50))?
        .iter()
        .map(render_event)
        .collect())
}

#[tauri::command]
pub(crate) fn recent_activity() -> Result<Vec<EventRecord>, String> {
    let mut entries = list_events(
        Some(EventFilter {
            severities: Some(vec![EventSeverity::Info, EventSeverity::Notice]),
            areas: Some(vec![
                EventArea::Profile,
                EventArea::Service,
                EventArea::Tray,
                EventArea::Preview,
            ]),
            since_unix_ms: None,
        }),
        Some(8),
    )?;
    entries.sort_by_key(|event| event.ts);
    Ok(entries)
}

#[tauri::command]
pub(crate) fn export_diagnostics(state: State<'_, PreviewState>) -> Result<String, String> {
    let report = diagnostic_report(state)?;
    export(&report)
}

#[tauri::command]
pub(crate) fn copy_diagnostics(state: State<'_, PreviewState>) -> Result<(), String> {
    let report = diagnostic_report(state)?;
    copy_to_clipboard(&report)
}

pub(crate) fn record_session_ending() {
    let _ = operation_log::record_session_ending();
}

pub(crate) fn record_end_session_hook_failed(window: &str, stage: &str, error: &str) {
    operation_log::record_control_center_event(
        EventSeverity::Warning,
        EventArea::ControlCenter,
        "end-session-hook-failed",
        BTreeMap::from([
            ("window".to_owned(), window.to_owned()),
            ("stage".to_owned(), stage.to_owned()),
            ("error".to_owned(), error.to_owned()),
        ]),
    );
}

pub(crate) fn record_app_started() {
    operation_log::record_control_center_event(
        EventSeverity::Info,
        EventArea::ControlCenter,
        "app-started",
        BTreeMap::from([("version".to_owned(), env!("CARGO_PKG_VERSION").to_owned())]),
    );
}

pub(crate) fn record_legacy_tray_detected(kind: &str, process: Option<&str>) {
    let mut params = BTreeMap::from([("kind".to_owned(), kind.to_owned())]);
    if let Some(process) = process {
        params.insert("process".to_owned(), process.to_owned());
    }
    operation_log::record_control_center_event(
        EventSeverity::Notice,
        EventArea::Tray,
        "legacy-tray-detected",
        params,
    );
}

/// Preview helper lifecycle events: connected once per successful start of
/// the MacType engine, restarted after a request failure, failed when a start
/// does not produce a usable helper.
pub(crate) fn record_preview_event(
    severity: EventSeverity,
    code: &str,
    params: BTreeMap<String, String>,
) {
    operation_log::record_control_center_event(severity, EventArea::Preview, code, params);
}

pub(crate) fn watch_paths() -> Result<Vec<PathBuf>, String> {
    event_paths()
}

pub(crate) fn newest_timestamp() -> Option<u64> {
    event_paths()
        .ok()
        .and_then(|paths| read_all_events(&paths, 1).last().map(|event| event.ts))
}

pub(super) fn render_event(event: &EventRecord) -> String {
    let mut value = format!(
        "{} {} {} {}",
        event.ts,
        severity_name(event.severity),
        area_name(event.area),
        event.code
    );
    for (key, item) in &event.params {
        value.push(' ');
        value.push_str(key);
        value.push('=');
        value.push_str(item);
    }
    if let Some(detail) = &event.detail {
        value.push(' ');
        value.push_str(&detail.replace(['\r', '\n'], " "));
    }
    value
}

fn source_statuses(paths: &[PathBuf]) -> Vec<EventSourceStatus> {
    paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let metadata = fs::metadata(path);
            EventSourceStatus {
                source: match index {
                    0 => EventSource::ControlCenter,
                    1 => EventSource::ServiceHost,
                    _ => EventSource::ServiceSetup,
                },
                path: path.to_string_lossy().into_owned(),
                readable: fs::File::open(path).is_ok(),
                bytes: metadata.map_or(0, |metadata| metadata.len()),
            }
        })
        .collect()
}

fn source_name(source: EventSource) -> &'static str {
    match source {
        EventSource::ServiceHost => "service-host",
        EventSource::ServiceSetup => "service-setup",
        EventSource::ControlCenter => "control-center",
    }
}

fn severity_name(severity: EventSeverity) -> &'static str {
    match severity {
        EventSeverity::Info => "info",
        EventSeverity::Notice => "notice",
        EventSeverity::Warning => "warning",
        EventSeverity::Error => "error",
    }
}

fn area_name(area: EventArea) -> &'static str {
    match area {
        EventArea::Service => "service",
        EventArea::Setup => "setup",
        EventArea::Profile => "profile",
        EventArea::Preview => "preview",
        EventArea::Injection => "injection",
        EventArea::ControlCenter => "control-center",
        EventArea::Tray => "tray",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_export_is_atomic_and_preserves_unicode() {
        let root = env::temp_dir().join(format!("mactype-diagnostics-{}", std::process::id()));
        let path = export_to(&root, "MacType diagnostics\n코어=2022.7.12\n", 7).unwrap();
        assert_eq!(path.file_name().unwrap(), "diagnostics-7.txt");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "MacType diagnostics\n코어=2022.7.12\n"
        );
        assert!(!root.join(".diagnostics-7.tmp").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn operation_failure_is_an_event_record_without_secrets() {
        let root = env::temp_dir().join(format!("mactype-operation-log-{}", std::process::id()));
        let profile = "[General]\r\nSecretFont=private\r\n";
        operation_log::record_operation_failure_at(
            &root,
            &OperationFailure {
                operation: "install".to_owned(),
                stage: "broker".to_owned(),
                error_chain: format!("Win32 5: {profile} 00112233445566778899aabbccddeeff"),
                broker_exit_code: Some(21),
                channel_failure: None,
                rollback: "completed".to_owned(),
                final_state: "unchanged".to_owned(),
                installation_preflight: None,
            },
            &[profile],
        )
        .unwrap();
        let event = operation_log::read_all_at(&root).pop().unwrap();
        assert_eq!(event.code, "operation-failed");
        assert_eq!(event.params.get("win32Code").map(String::as_str), Some("5"));
        let disk = fs::read_to_string(root.join("control-center.log")).unwrap();
        assert!(!disk.contains(profile));
        assert!(!disk.contains("00112233445566778899aabbccddeeff"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn event_log_rotation_is_bounded_and_retains_recent_failures() {
        let root = env::temp_dir().join(format!("mactype-log-rotation-{}", std::process::id()));
        for index in 0..120 {
            operation_log::record_operation_failure_at(
                &root,
                &OperationFailure {
                    operation: "install".to_owned(),
                    stage: format!("fixture-{index}"),
                    error_chain: format!("failure-{index}: {}", "x".repeat(24 * 1024)),
                    broker_exit_code: Some(21),
                    channel_failure: None,
                    rollback: "not-applicable".to_owned(),
                    final_state: "unchanged".to_owned(),
                    installation_preflight: None,
                },
                &[],
            )
            .unwrap();
        }
        let files = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap())
            .collect::<Vec<_>>();
        assert!(files.len() <= 5, "rotation created too many files");
        assert!(files
            .iter()
            .all(|entry| entry.metadata().unwrap().len() <= 512 * 1024));
        let event = operation_log::read_all_at(&root).pop().unwrap();
        assert_eq!(
            event.params.get("stage").map(String::as_str),
            Some("fixture-119")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn activities_and_failures_share_the_log_without_exposing_profile_paths() {
        let root = env::temp_dir().join(format!("mactype-activity-log-{}", std::process::id()));
        operation_log::record_operation_failure_at(
            &root,
            &OperationFailure {
                operation: "publish-profile".to_owned(),
                stage: "fixture failure".to_owned(),
                error_chain: "must stay in diagnostics only".to_owned(),
                broker_exit_code: None,
                channel_failure: None,
                rollback: "not-applicable".to_owned(),
                final_state: "unchanged".to_owned(),
                installation_preflight: None,
            },
            &[],
        )
        .unwrap();
        for index in 0..6 {
            operation_log::record_activity_at(
                &root,
                ActivityKind::ProfileApplied,
                Some(&format!(r"C:\Users\Secret\Profiles\Profile-{index}.ini")),
            )
            .unwrap();
        }
        let events = operation_log::read_all_at(&root);
        assert_eq!(events.len(), 7);
        assert_eq!(events[0].code, "operation-failed");
        let activities = events
            .iter()
            .filter(|event| event.code == "profile-applied")
            .collect::<Vec<_>>();
        assert_eq!(activities.len(), 6);
        assert_eq!(activities[5].params["profile"], "Profile-5.ini");
        assert!(activities
            .iter()
            .all(|event| !event.params["profile"].contains("Secret")));
        let disk = fs::read_to_string(root.join("control-center.log")).unwrap();
        assert!(!disk.contains(r"C:\Users\Secret"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn operation_failure_preserves_preflight_diagnostics_in_detail() {
        let root = env::temp_dir().join(format!("mactype-preflight-log-{}", std::process::id()));
        operation_log::record_operation_failure_at(
            &root,
            &OperationFailure {
                operation: "install".to_owned(),
                stage: "installation-preflight".to_owned(),
                error_chain: "control-center-installation-required".to_owned(),
                broker_exit_code: None,
                channel_failure: None,
                rollback: "not-applicable".to_owned(),
                final_state: "unchanged".to_owned(),
                installation_preflight: Some(InstallationPreflightDiagnostics {
                    expected_installed_control_center: None,
                    current_executable: Some(r"D:\src\MacType Control Center.exe".to_owned()),
                    expected_executable_exists: None,
                    installed_control_center: "missing".to_owned(),
                    current_bundle: "incomplete".to_owned(),
                    selected_service_package: "none".to_owned(),
                    setup_broker: "not-checked".to_owned(),
                    runtime_manifest: "not-checked".to_owned(),
                    runtime_payload: "not-checked".to_owned(),
                    elevation_attempted: false,
                    elevated_revalidation: "not-attempted".to_owned(),
                    machine_state_changed: false,
                    rollback_required: false,
                }),
            },
            &[],
        )
        .unwrap();
        let event = operation_log::read_all_at(&root).pop().unwrap();
        let detail = event.detail.unwrap();
        assert!(detail.contains("Current executable: D:\\src\\MacType Control Center.exe"));
        assert!(detail.contains("Elevation attempted: no"));
        assert!(detail.contains("Rollback required: no"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn integrated_reader_keeps_path_order_for_equal_timestamps() {
        let root = env::temp_dir().join(format!("mactype-integrated-log-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("control-center.log"),
            "{\"timestampUnixMs\":7,\"activity\":\"profile-applied\",\"profile\":\"A.ini\"}\n",
        )
        .unwrap();
        let service = root.join("service.log");
        mactype_service_contract::event_log::EventLogWriter::new(service.clone())
            .append(&EventRecord::new(
                7,
                EventSeverity::Info,
                EventArea::Service,
                "service-started",
                BTreeMap::new(),
                None,
                EventSource::ServiceHost,
            ))
            .unwrap();
        let events = read_all_events(&[root.join("control-center.log"), service], 10);
        assert_eq!(events[0].source, EventSource::ControlCenter);
        assert_eq!(events[1].source, EventSource::ServiceHost);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_activity_and_failure_lines_are_converted() {
        let root = env::temp_dir().join(format!("mactype-legacy-log-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("control-center.log"),
            concat!(
                "{\"timestampUnixMs\":1,\"activity\":\"profile-applied\",\"profile\":\"C:\\\\Secret\\\\A.ini\"}\n",
                "{\"timestampUnixMs\":2,\"operation\":\"install\",\"stage\":\"broker\",\"errorChain\":\"failed\",\"win32Code\":5,\"brokerExitCode\":null,\"channelFailure\":null,\"rollback\":\"completed\",\"finalState\":\"unchanged\"}\n"
            ),
        ).unwrap();
        let events = operation_log::read_all_at(&root);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].code, "profile-applied");
        assert_eq!(
            events[0].params.get("profile").map(String::as_str),
            Some("A.ini")
        );
        assert_eq!(events[1].code, "operation-failed");
        fs::remove_dir_all(root).unwrap();
    }
}
