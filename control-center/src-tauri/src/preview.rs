use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Mutex};
use tauri::{AppHandle, State};

mod commands;
mod helper;
mod installation;
mod protocol;
mod render;
mod state;

use helper::PreviewManager;
pub(crate) use installation::InstallationStatus;
pub(crate) use render::{NativePreviewOptions, NativePreviewState, PreviewResult, PreviewSample};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PreviewEngine {
    #[default]
    Mactype,
    Plain,
}

impl PreviewEngine {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Mactype => "mactype",
            Self::Plain => "plain",
        }
    }
}

#[derive(Default)]
pub(crate) struct PreviewState(Mutex<PreviewManager>);

pub(crate) struct PreviewDiagnosticSnapshot {
    pub(crate) status: InstallationStatus,
    pub(crate) entries: Vec<String>,
}

#[tauri::command]
pub(crate) fn scan_installation(
    state: State<'_, PreviewState>,
) -> Result<InstallationStatus, String> {
    commands::scan_installation(state.inner())
}

#[tauri::command]
pub(crate) fn rediscover_installation(
    state: State<'_, PreviewState>,
) -> Result<InstallationStatus, String> {
    commands::rediscover_installation(state.inner())
}

#[tauri::command]
pub(crate) fn reconnect_preview(
    state: State<'_, PreviewState>,
) -> Result<InstallationStatus, String> {
    commands::reconnect_preview(state.inner())
}

#[tauri::command]
pub(crate) fn render_profile_preview(
    app: AppHandle,
    profile_path: String,
    overrides: BTreeMap<String, f64>,
    sample: PreviewSample,
    engine: Option<PreviewEngine>,
    state: State<'_, PreviewState>,
) -> Result<PreviewResult, String> {
    commands::render_profile_preview(
        app,
        profile_path,
        overrides,
        sample,
        engine.unwrap_or_default(),
        state.inner(),
    )
}

#[tauri::command]
pub(crate) fn set_native_preview(
    app: AppHandle,
    visible: bool,
    options: Option<NativePreviewOptions>,
    state: State<'_, PreviewState>,
) -> Result<NativePreviewState, String> {
    commands::set_native_preview(app, visible, options, state.inner())
}

#[tauri::command]
pub(crate) fn preview_diagnostics(state: State<'_, PreviewState>) -> Result<Vec<String>, String> {
    commands::preview_diagnostics(state.inner())
}

#[tauri::command]
pub(crate) fn ci_force_preview_crash(state: State<'_, PreviewState>) -> Result<(), String> {
    commands::ci_force_preview_crash(state.inner())
}
