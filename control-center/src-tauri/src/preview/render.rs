use super::{
    helper::PreviewManager,
    protocol::{
        HIDE_NATIVE_PREVIEW, NATIVE_PREVIEW_STATE, PREVIEW_RENDERED, RENDER_PREVIEW,
        SHOW_NATIVE_PREVIEW,
    },
    PreviewEngine,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};
use tauri::{AppHandle, Manager};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewSample {
    text: String,
    font_face: String,
    font_size_pt: f64,
    width_px: u32,
    height_px: u32,
    dpi: u32,
    foreground: String,
    background: String,
    /// Optional GDI style flags; older callers omit them and render regular text.
    #[serde(default)]
    bold: bool,
    #[serde(default)]
    italic: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RenderRequest<'a> {
    request_id: u64,
    profile_path: &'a str,
    overrides: &'a BTreeMap<String, f64>,
    sample: &'a PreviewSample,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenderMetadata {
    width: u32,
    height: u32,
    dpi: u32,
    elapsed_ms: u64,
    core_version: u32,
    engine: PreviewEngine,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewResult {
    request_id: u64,
    image_path: String,
    width: u32,
    height: u32,
    dpi: u32,
    elapsed_ms: u64,
    core_version: u32,
    engine: PreviewEngine,
}

pub(super) fn render_preview(
    app: &AppHandle,
    manager: &mut PreviewManager,
    install_root: &Path,
    profile_path: &str,
    overrides: &BTreeMap<String, f64>,
    sample: &PreviewSample,
    engine: PreviewEngine,
) -> Result<PreviewResult, String> {
    let response = manager.request_built(install_root, engine, RENDER_PREVIEW, |request_id| {
        serde_json::to_vec(&RenderRequest {
            request_id,
            profile_path,
            overrides,
            sample,
        })
        .map_err(|error| error.to_string())
    })?;
    if response.kind != PREVIEW_RENDERED || !response.binary.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("preview helper returned an invalid PNG response".to_owned());
    }
    let metadata: RenderMetadata =
        serde_json::from_slice(&response.json).map_err(|error| error.to_string())?;
    if metadata.engine != engine {
        return Err("preview helper returned a mismatched engine".to_owned());
    }
    let directory = app
        .path()
        .app_local_data_dir()
        .map_err(|error| error.to_string())?
        .join("preview");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let image = directory.join(format!("preview-{}.png", response.request_id));
    fs::write(&image, response.binary).map_err(|error| error.to_string())?;
    Ok(PreviewResult {
        request_id: response.request_id,
        image_path: image.to_string_lossy().into_owned(),
        width: metadata.width,
        height: metadata.height,
        dpi: metadata.dpi,
        elapsed_ms: metadata.elapsed_ms,
        core_version: metadata.core_version,
        engine: metadata.engine,
    })
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePreviewChrome {
    pub(crate) skin: String,
    pub(crate) canvas: String,
    pub(crate) surface: String,
    pub(crate) surface_subtle: String,
    pub(crate) border: String,
    pub(crate) text: String,
    pub(crate) muted: String,
    pub(crate) accent: String,
    pub(crate) on_accent: String,
    pub(crate) radius: u32,
    pub(crate) control_height: u32,
    pub(crate) toolbar_height: u32,
    pub(crate) status_height: u32,
    pub(crate) canvas_radius: u32,
    pub(crate) canvas_inset: u32,
    pub(crate) mono_status: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePreviewOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) display_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) listing_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) font_face: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) font_size_pt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) foreground: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) inverted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) zoom: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) sizes: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) labels: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) chrome: Option<NativePreviewChrome>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePreviewState {
    pub(crate) visible: bool,
    pub(crate) display_mode: String,
    pub(crate) background: String,
    #[serde(default)]
    pub(crate) foreground: String,
    #[serde(default)]
    pub(crate) inverted: bool,
    #[serde(default)]
    pub(crate) zoom: u32,
    #[serde(default)]
    pub(crate) font_face: String,
    #[serde(default)]
    pub(crate) font_size_pt: u32,
    #[serde(default)]
    pub(crate) bold: bool,
    #[serde(default)]
    pub(crate) italic: bool,
    #[serde(default)]
    pub(crate) topmost: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) skin: Option<String>,
}

pub(super) fn set_native_preview(
    manager: &mut PreviewManager,
    install_root: &Path,
    visible: bool,
    options: Option<NativePreviewOptions>,
) -> Result<NativePreviewState, String> {
    let kind = if visible {
        SHOW_NATIVE_PREVIEW
    } else {
        HIDE_NATIVE_PREVIEW
    };
    let body = if visible {
        serde_json::to_vec(&options.unwrap_or_default()).map_err(|error| error.to_string())?
    } else {
        Vec::new()
    };
    let response = manager.request(install_root, PreviewEngine::Mactype, kind, body)?;
    if response.kind != NATIVE_PREVIEW_STATE {
        return Err("preview helper returned an invalid native-window response".to_owned());
    }
    serde_json::from_slice(&response.json).map_err(|error| error.to_string())
}

#[cfg(test)]
mod native_preview_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_preview_options_round_trip_full_json_and_omit_absent_fields() {
        let documented = json!({
            "displayMode": "sample",
            "text": r"sample text, may contain \n",
            "listingText": "pangram for listing mode",
            "fontFace": "Segoe UI",
            "fontSizePt": 14,
            "bold": false,
            "italic": false,
            "foreground": "#181D23",
            "background": "#EEF1F4",
            "theme": "light",
            "inverted": false,
            "zoom": 1,
            "sizes": [8, 9, 10, 11, 12, 14, 16, 18, 20, 24],
            "chrome": {
                "skin": "classic",
                "canvas": "#F3F5F7",
                "surface": "#FFFFFF",
                "surfaceSubtle": "#E9EDF1",
                "border": "#C9D1D8",
                "text": "#17212B",
                "muted": "#5A6773",
                "accent": "#0067C0",
                "onAccent": "#FFFFFF",
                "radius": 4,
                "controlHeight": 32,
                "toolbarHeight": 44,
                "statusHeight": 28,
                "canvasRadius": 4,
                "canvasInset": 18,
                "monoStatus": false
            },
            "labels": {
                "title": "…",
                "fontFace": "…",
                "fontSize": "…",
                "bold": "…",
                "italic": "…",
                "modeSample": "…",
                "modeLadder": "…",
                "modeCompare": "…",
                "modeListing": "…",
                "invert": "…",
                "loupe": "…",
                "zoom": "…",
                "topmost": "…",
                "editText": "…",
                "savePng": "…",
                "copy": "…",
                "compareMacType": "…",
                "compareWindows": "…",
                "compareUnavailable": "…",
                "engineMacType": "…",
                "coreVersion": "코어 {version}",
                "pngFilter": "…",
                "saved": "…",
                "copied": "…"
            }
        });
        let options: NativePreviewOptions = serde_json::from_value(documented.clone()).unwrap();
        assert_eq!(serde_json::to_value(options).unwrap(), documented);
        assert_eq!(
            serde_json::to_value(NativePreviewOptions::default()).unwrap(),
            json!({})
        );
    }

    #[test]
    fn native_preview_state_parses_old_three_field_response() {
        let state: NativePreviewState = serde_json::from_value(json!({
            "visible": true,
            "displayMode": "sample",
            "background": "#EEF1F4"
        }))
        .unwrap();

        assert!(state.visible);
        assert_eq!(state.display_mode, "sample");
        assert_eq!(state.background, "#EEF1F4");
        assert_eq!(state.foreground, "");
        assert!(!state.inverted);
        assert_eq!(state.zoom, 0);
        assert_eq!(state.font_face, "");
        assert_eq!(state.font_size_pt, 0);
        assert!(!state.bold);
        assert!(!state.italic);
        assert!(!state.topmost);
        assert_eq!(state.skin, None);
    }

    #[test]
    fn native_preview_state_parses_complete_response() {
        let documented = json!({
            "visible": true,
            "displayMode": "sample",
            "background": "#EEF1F4",
            "foreground": "#181D23",
            "inverted": false,
            "zoom": 1,
            "fontFace": "Segoe UI",
            "fontSizePt": 14,
            "bold": false,
            "italic": false,
            "topmost": false,
            "skin": "classic"
        });
        let state: NativePreviewState = serde_json::from_value(documented.clone()).unwrap();

        assert_eq!(serde_json::to_value(state).unwrap(), documented);
    }
}
