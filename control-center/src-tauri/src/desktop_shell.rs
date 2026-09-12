use crate::{app, execution, preview_studio, single_instance};
use serde::Serialize;
use std::{
    env,
    error::Error,
    fs,
    path::Path,
    thread,
    time::{Duration, SystemTime},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    App, AppHandle, Emitter, Manager, Window, WindowEvent, Wry,
};

enum TrayBackgroundAction {
    Login,
    Apply,
}

pub(crate) fn install(
    app: &mut App<Wry>,
    startup_gate: single_instance::StartupGate,
) -> Result<(), Box<dyn Error>> {
    install_tray(app)?;
    #[cfg(windows)]
    {
        if let Some(window) = app.get_webview_window("main") {
            match window.hwnd() {
                Ok(hwnd) => install_end_session_hook(hwnd.0 as isize, "main"),
                Err(error) => crate::diagnostics::record_end_session_hook_failed(
                    "main",
                    "obtain-hwnd",
                    &error.to_string(),
                ),
            }
        }
        if let Some(tray) = app.tray_by_id("main") {
            if let Err(error) = tray.with_inner_tray_icon(|tray| {
                install_end_session_hook(tray.window_handle() as isize, "tray")
            }) {
                crate::diagnostics::record_end_session_hook_failed(
                    "tray",
                    "access-inner-tray",
                    &error.to_string(),
                );
            }
        }
    }
    crate::diagnostics::record_app_started();
    start_event_log_watcher(app.handle().clone());
    if app::starts_in_tray() {
        if let Some(window) = app.get_webview_window("main") {
            window.hide()?;
        }
        spawn_background(app.handle().clone(), TrayBackgroundAction::Login);
    }
    startup_gate.release()?;
    single_instance::write_ready_marker()?;
    Ok(())
}

pub(crate) fn handle_window_event(window: &Window<Wry>, event: &WindowEvent) {
    if window.label() != "main" {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        if let Some(studio) = window
            .app_handle()
            .get_webview_window(preview_studio::STUDIO_WINDOW_LABEL)
        {
            let _ = studio.destroy();
        }
        if env::var_os("MACTYPE_CI_SMOKE_FILE").is_none() {
            api.prevent_close();
            let _ = window.hide();
        }
    }
}

#[cfg(windows)]
fn install_end_session_hook(hwnd: isize, window: &str) {
    if let Err(error) = mactype_service_platform::install_end_session_exit_hook(
        hwnd,
        crate::diagnostics::record_session_ending,
    ) {
        crate::diagnostics::record_end_session_hook_failed(
            window,
            "install-subclass",
            &error.to_string(),
        );
    }
}

fn install_tray(app: &mut App<Wry>) -> tauri::Result<()> {
    let (show_label, inject_label, hide_label, quit_label) = app::tray_menu_labels("ko");
    let show = MenuItem::with_id(app, "show", show_label, true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", hide_label, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;
    let inject = MenuItem::with_id(app, "inject", inject_label, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &inject, &hide, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("MacType Control Center")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                let _ = app::restore_main_window(app);
            }
            "hide" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            "inject" => spawn_background(app.clone(), TrayBackgroundAction::Apply),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EventLogChanged {
    newest_ts: Option<u64>,
}

#[derive(Clone, Eq, PartialEq)]
struct FileStamp {
    length: u64,
    modified: Option<SystemTime>,
}

fn start_event_log_watcher(app: AppHandle<Wry>) {
    let Ok(paths) = crate::diagnostics::watch_paths() else {
        return;
    };
    thread::spawn(move || {
        let mut stamps = paths.iter().map(|path| stamp(path)).collect::<Vec<_>>();
        loop {
            thread::sleep(Duration::from_secs(2));
            let current = paths.iter().map(|path| stamp(path)).collect::<Vec<_>>();
            if current != stamps {
                stamps = current;
                let _ = app.emit(
                    "event-log:changed",
                    EventLogChanged {
                        newest_ts: crate::diagnostics::newest_timestamp(),
                    },
                );
            }
        }
    });
}

fn stamp(path: &Path) -> FileStamp {
    fs::metadata(path).map_or(
        FileStamp {
            length: 0,
            modified: None,
        },
        |metadata| FileStamp {
            length: metadata.len(),
            modified: metadata.modified().ok(),
        },
    )
}

fn spawn_background(app: AppHandle<Wry>, action: TrayBackgroundAction) {
    thread::spawn(move || run_background(app, action));
}

fn run_background(app: AppHandle<Wry>, action: TrayBackgroundAction) {
    let result = match action {
        TrayBackgroundAction::Login => execution::observe_machine_on_tray_login()
            .and_then(|_| execution::launch_registered_targets().map(|_| ())),
        TrayBackgroundAction::Apply => execution::apply_system_injection_from_tray_menu()
            .and_then(|()| execution::launch_registered_targets().map(|_| ())),
    };
    if let Err(error) = result {
        eprintln!("tray background operation failed: {error}");
        let _ = app.emit("machine-integration-error", &error);
        let _ = app::restore_main_window(&app);
    }
}
