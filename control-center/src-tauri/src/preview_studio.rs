use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub(crate) const STUDIO_WINDOW_LABEL: &str = "preview-studio";
const MAX_EXPORT_SIZE: usize = 32 * 1024 * 1024;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
static STUDIO_LIFECYCLE: Mutex<()> = Mutex::new(());
static STUDIO_SMOKE_READY_COUNT: AtomicUsize = AtomicUsize::new(0);

#[tauri::command]
pub(crate) async fn open_preview_studio(app: AppHandle) -> Result<(), String> {
    // WebView2 creation deadlocks in a synchronous Windows IPC command.
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = STUDIO_LIFECYCLE.lock().map_err(|error| error.to_string())?;
        let (completed, receiver) = std::sync::mpsc::channel();
        let main_thread_app = app.clone();
        app.run_on_main_thread(move || {
            let _ = completed.send(open_studio_window(&main_thread_app));
        })
        .map_err(|error| error.to_string())?;
        receiver.recv().map_err(|error| error.to_string())?
    })
    .await
    .map_err(|error| error.to_string())?
}

fn open_studio_window(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(STUDIO_WINDOW_LABEL) {
        window.show().map_err(|error| error.to_string())?;
        window.unminimize().map_err(|error| error.to_string())?;
        return window.set_focus().map_err(|error| error.to_string());
    }

    let window = WebviewWindowBuilder::new(
        app,
        STUDIO_WINDOW_LABEL,
        WebviewUrl::App("index.html?window=preview-studio".into()),
    )
    .title("MacType Preview Studio")
    .inner_size(1180.0, 760.0)
    .min_inner_size(880.0, 560.0)
    .decorations(false)
    .resizable(true)
    .center()
    .build()
    .map_err(|error| error.to_string())?;
    #[cfg(windows)]
    match window.hwnd() {
        Ok(hwnd) => {
            if let Err(error) = mactype_service_platform::install_end_session_exit_hook(
                hwnd.0 as isize,
                crate::diagnostics::record_session_ending,
            ) {
                crate::diagnostics::record_end_session_hook_failed(
                    STUDIO_WINDOW_LABEL,
                    "install-subclass",
                    &error.to_string(),
                );
            }
        }
        Err(error) => crate::diagnostics::record_end_session_hook_failed(
            STUDIO_WINDOW_LABEL,
            "obtain-hwnd",
            &error.to_string(),
        ),
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn close_preview_studio(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = STUDIO_LIFECYCLE.lock().map_err(|error| error.to_string())?;
        if let Some(window) = app.get_webview_window(STUDIO_WINDOW_LABEL) {
            let (destroyed, receiver) = std::sync::mpsc::channel();
            window.on_window_event(move |event| {
                if matches!(event, tauri::WindowEvent::Destroyed) {
                    let _ = destroyed.send(());
                }
            });
            window.destroy().map_err(|error| error.to_string())?;
            receiver
                .recv_timeout(std::time::Duration::from_secs(5))
                .map_err(|error| format!("Preview Studio did not finish closing: {error}"))?;
        }
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) async fn preview_studio_ready(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    if !crate::app::preview_studio_smoke() || window.label() != STUDIO_WINDOW_LABEL {
        return Ok(());
    }
    if !window.is_visible().map_err(|error| error.to_string())? {
        return Err("Preview Studio rendered in a hidden window".to_owned());
    }
    match STUDIO_SMOKE_READY_COUNT.fetch_add(1, Ordering::SeqCst) {
        0 => {
            window.hide().map_err(|error| error.to_string())?;
            open_preview_studio(app.clone()).await?;
            if !window.is_visible().map_err(|error| error.to_string())? {
                return Err("Preview Studio did not restore its existing window".to_owned());
            }
            close_preview_studio(app.clone()).await?;
            open_preview_studio(app).await
        }
        1 => {
            close_preview_studio(app.clone()).await?;
            crate::app::frontend_ready(app, "preview-studio".to_owned())
        }
        _ => Ok(()),
    }
}

#[tauri::command]
pub(crate) fn write_preview_export(path: String, png_base64: String) -> Result<String, String> {
    let destination = validate_export_path(&path)?;
    let png = decode_base64(&png_base64)?;
    if png.len() > MAX_EXPORT_SIZE {
        return Err("preview export exceeds 32 MiB".to_owned());
    }
    if !png.starts_with(PNG_SIGNATURE) {
        return Err("preview export is not a PNG image".to_owned());
    }
    write_atomic(&destination, &png)?;
    Ok(destination.to_string_lossy().into_owned())
}

fn validate_export_path(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err("preview export path must be absolute".to_owned());
    }
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    {
        return Err("preview export path must use the .png extension".to_owned());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "preview export path has no parent directory".to_owned())?;
    if !parent.is_dir() {
        return Err("preview export parent directory does not exist".to_owned());
    }
    Ok(path)
}

fn decode_base64(value: &str) -> Result<Vec<u8>, String> {
    if value.len() % 4 != 0 {
        return Err("preview export base64 must use standard padding".to_owned());
    }
    if value.len() / 4 > (MAX_EXPORT_SIZE / 3) + 2 {
        return Err("preview export exceeds 32 MiB".to_owned());
    }

    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(value.len() / 4 * 3);
    for (block_index, block) in bytes.chunks_exact(4).enumerate() {
        let final_block = block_index + 1 == bytes.len() / 4;
        let a = base64_value(block[0])?;
        let b = base64_value(block[1])?;
        let c_padding = block[2] == b'=';
        let d_padding = block[3] == b'=';
        if c_padding {
            if !final_block || !d_padding || b & 0x0f != 0 {
                return Err("preview export base64 has invalid padding".to_owned());
            }
            decoded.push((a << 2) | (b >> 4));
        } else {
            let c = base64_value(block[2])?;
            decoded.push((a << 2) | (b >> 4));
            decoded.push((b << 4) | (c >> 2));
            if d_padding {
                if !final_block || c & 0x03 != 0 {
                    return Err("preview export base64 has invalid padding".to_owned());
                }
            } else {
                let d = base64_value(block[3])?;
                decoded.push((c << 6) | d);
            }
        }
        if decoded.len() > MAX_EXPORT_SIZE {
            return Err("preview export exceeds 32 MiB".to_owned());
        }
    }
    Ok(decoded)
}

fn base64_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err("preview export base64 has an invalid alphabet".to_owned()),
    }
}

fn write_atomic(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "preview export path has no parent directory".to_owned())?;
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "preview export file name is invalid".to_owned())?;
    let mut last_error = None;
    for attempt in 0..32_u32 {
        let temporary = parent.join(format!(".{file_name}.{}.{attempt}.tmp", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(mut file) => {
                let result = (|| {
                    file.write_all(bytes).map_err(|error| error.to_string())?;
                    file.sync_all().map_err(|error| error.to_string())?;
                    drop(file);
                    #[cfg(windows)]
                    {
                        mactype_service_platform::replace_file(&temporary, destination)
                            .map_err(|error| error.to_string())
                    }
                    #[cfg(not(windows))]
                    {
                        fs::rename(&temporary, destination).map_err(|error| error.to_string())
                    }
                })();
                if result.is_err() {
                    let _ = fs::remove_file(&temporary);
                }
                return result;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                last_error = Some(error.to_string());
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Err(last_error.unwrap_or_else(|| "could not create preview export temporary file".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_base64_accepts_standard_padded_input() {
        assert_eq!(decode_base64("iVBORw0KGgo=").unwrap(), PNG_SIGNATURE);
        assert_eq!(decode_base64("TWFu").unwrap(), b"Man");
    }

    #[test]
    fn strict_base64_rejects_unpadded_url_safe_and_bad_padding() {
        assert!(decode_base64("iVBORw0KGgo").is_err());
        assert!(decode_base64("____").is_err());
        assert!(decode_base64("A===").is_err());
        assert!(decode_base64("AB==").is_err());
        assert!(decode_base64("AAF=").is_err());
    }

    #[test]
    fn export_path_requires_absolute_png_with_existing_parent() {
        assert!(validate_export_path("relative.png").is_err());
        let path = std::env::temp_dir().join("preview-export.PNG");
        assert_eq!(validate_export_path(path.to_str().unwrap()).unwrap(), path);
        let text = std::env::temp_dir().join("preview-export.txt");
        assert!(validate_export_path(text.to_str().unwrap()).is_err());
    }
}
