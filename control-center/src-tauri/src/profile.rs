use codec::OriginalLegacyLines;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
    sync::Mutex,
};
use tauri::State;

mod codec;
mod commands;
mod document;
mod identity;
mod legacy;
mod mutation;
mod state;
mod storage;

const MAX_PROFILE_DIRECTORY_ENTRIES: usize = 512;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod upstream_corpus_tests;

pub(crate) use identity::source_profile_reference;
pub(crate) use legacy::{bundled_default_profile_at, legacy_alternative_file_bytes};

#[cfg(test)]
use document::hash;
#[cfg(test)]
use legacy::{discover_legacy_profile_at, import_profile_to, install_system_profile_at};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    EucKr,
    Gb18030,
    Big5,
    ShiftJis,
    Windows1252,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BomKind {
    None,
    Utf8,
    Utf16Le,
    Utf16Be,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LineEnding {
    CrLf,
    Lf,
    Cr,
}

#[derive(Clone, Debug, PartialEq)]
enum IniNode {
    Blank {
        raw: String,
    },
    Comment {
        raw: String,
    },
    Section {
        name: String,
        raw: String,
    },
    KeyValue {
        section: String,
        key: String,
        value: String,
        prefix: String,
        separator: String,
        suffix: String,
        raw: String,
    },
    Unknown {
        section: String,
        raw: String,
    },
}

impl IniNode {
    fn raw(&self) -> &str {
        match self {
            Self::Blank { raw }
            | Self::Comment { raw }
            | Self::Section { raw, .. }
            | Self::KeyValue { raw, .. }
            | Self::Unknown { raw, .. } => raw,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ProfileRevision {
    nodes: Vec<IniNode>,
    dirty_keys: BTreeSet<String>,
}

#[derive(Debug)]
struct ProfileDocument {
    path: PathBuf,
    encoding: TextEncoding,
    bom: BomKind,
    line_ending: LineEnding,
    nodes: Vec<IniNode>,
    original_hash: [u8; 32],
    original_legacy_lines: Option<OriginalLegacyLines>,
    saved_values: BTreeMap<String, f64>,
    dirty_keys: BTreeSet<String>,
    undo_history: VecDeque<ProfileRevision>,
    redo_history: VecDeque<ProfileRevision>,
}

#[derive(Default)]
pub(crate) struct ProfileState(Mutex<Option<ProfileDocument>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileEntry {
    name: String,
    path: String,
    display_path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileLocation {
    Installation,
    Personal,
    External,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyProfileCandidate {
    name: String,
    path: String,
    source: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSnapshot {
    pub path: String,
    pub display_path: String,
    pub location: ProfileLocation,
    pub can_save: bool,
    pub encoding: TextEncoding,
    pub bom: BomKind,
    pub line_ending: LineEnding,
    pub original_hash: String,
    pub values: BTreeMap<String, f64>,
    pub saved_values: BTreeMap<String, f64>,
    pub dirty_keys: Vec<String>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub individuals: Vec<IndividualSetting>,
    pub lists: ProfileLists,
    pub advanced: AdvancedProfile,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndividualSetting {
    pub font_face: String,
    pub values: Vec<Option<i32>>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileLists {
    pub exclude_fonts: Vec<String>,
    pub include_fonts: Vec<String>,
    pub exclude_modules: Vec<String>,
    pub include_modules: Vec<String>,
    pub unload_dlls: Vec<String>,
    pub exclude_substitution_modules: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShadowSetting {
    pub offset_x: i32,
    pub offset_y: i32,
    pub dark_alpha: i32,
    pub dark_color: u32,
    pub light_alpha: i32,
    pub light_color: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedProfile {
    pub shadow: Option<ShadowSetting>,
    pub lcd_filter_weight: Option<Vec<i32>>,
    pub pixel_layout: Option<Vec<i32>>,
    pub font_substitutes: Vec<String>,
}

#[tauri::command]
pub(crate) fn list_profiles() -> Result<Vec<ProfileEntry>, String> {
    commands::list_profiles()
}

#[tauri::command]
pub(crate) fn open_profile(
    path: String,
    state: State<'_, ProfileState>,
) -> Result<ProfileSnapshot, String> {
    commands::open_profile(path, state.inner())
}

#[tauri::command]
pub(crate) fn open_default_profile(
    state: State<'_, ProfileState>,
) -> Result<Option<ProfileSnapshot>, String> {
    commands::open_default_profile(state.inner())
}

#[tauri::command]
pub(crate) fn current_profile(
    state: State<'_, ProfileState>,
) -> Result<Option<ProfileSnapshot>, String> {
    commands::current_profile(state.inner())
}

#[tauri::command]
pub(crate) fn discover_legacy_profile() -> Result<Option<LegacyProfileCandidate>, String> {
    commands::discover_legacy_profile()
}

#[tauri::command]
pub(crate) fn import_profile(
    path: String,
    state: State<'_, ProfileState>,
) -> Result<ProfileSnapshot, String> {
    commands::import_profile(path, state.inner())
}

#[tauri::command]
pub(crate) fn update_profile_setting(
    setting_id: String,
    value: f64,
    state: State<'_, ProfileState>,
) -> Result<ProfileSnapshot, String> {
    commands::update_profile_setting(setting_id, value, state.inner())
}

#[tauri::command]
pub(crate) fn update_profile_individuals(
    entries: Vec<IndividualSetting>,
    state: State<'_, ProfileState>,
) -> Result<ProfileSnapshot, String> {
    commands::update_profile_individuals(entries, state.inner())
}

#[tauri::command]
pub(crate) fn update_profile_list(
    kind: String,
    entries: Vec<String>,
    state: State<'_, ProfileState>,
) -> Result<ProfileSnapshot, String> {
    commands::update_profile_list(kind, entries, state.inner())
}

#[tauri::command]
pub(crate) fn update_profile_advanced(
    advanced: AdvancedProfile,
    state: State<'_, ProfileState>,
) -> Result<ProfileSnapshot, String> {
    commands::update_profile_advanced(advanced, state.inner())
}

#[tauri::command]
pub(crate) fn duplicate_profile(
    name: String,
    state: State<'_, ProfileState>,
) -> Result<ProfileSnapshot, String> {
    commands::duplicate_profile(name, state.inner())
}

#[tauri::command]
pub(crate) fn save_profile(state: State<'_, ProfileState>) -> Result<ProfileSnapshot, String> {
    commands::save_profile(state.inner())
}

#[tauri::command]
pub(crate) fn undo_profile(state: State<'_, ProfileState>) -> Result<ProfileSnapshot, String> {
    commands::undo_profile(state.inner())
}

#[tauri::command]
pub(crate) fn redo_profile(state: State<'_, ProfileState>) -> Result<ProfileSnapshot, String> {
    commands::redo_profile(state.inner())
}

#[tauri::command]
pub(crate) fn discard_profile_changes(
    state: State<'_, ProfileState>,
) -> Result<ProfileSnapshot, String> {
    commands::discard_profile_changes(state.inner())
}

#[tauri::command]
pub(crate) fn reset_profile_defaults(
    state: State<'_, ProfileState>,
) -> Result<ProfileSnapshot, String> {
    commands::reset_profile_defaults(state.inner())
}

#[tauri::command]
pub(crate) fn export_profile(
    path: String,
    state: State<'_, ProfileState>,
) -> Result<String, String> {
    commands::export_profile(path, state.inner())
}

#[tauri::command]
pub(crate) fn reveal_profile_file(state: State<'_, ProfileState>) -> Result<String, String> {
    commands::reveal_profile_file(state.inner())
}

#[tauri::command]
pub(crate) fn ci_verify_profile_workflow(state: State<'_, ProfileState>) -> Result<(), String> {
    commands::ci_verify_profile_workflow(state.inner())
}
