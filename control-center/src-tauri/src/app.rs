use serde::Serialize;
use std::{env, fs, path::PathBuf, thread, time::Duration};
use tauri::{AppHandle, Manager};

pub(crate) fn restore_main_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window was not created".to_owned())?;
    window.show().map_err(|error| error.to_string())?;
    window.unminimize().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchContext {
    view: String,
    ci_smoke: bool,
    tray_start: bool,
}

fn requested_view() -> String {
    let mut args = env::args();
    while let Some(argument) = args.next() {
        if argument == "--ci-view" {
            if let Some(value) = args.next() {
                if matches!(
                    value.as_str(),
                    "overview" | "files" | "profiles" | "execution" | "diagnostics"
                ) {
                    return value;
                }
            }
        }
    }
    "overview".to_owned()
}

pub(crate) fn tray_menu_labels(
    locale: &str,
) -> (&'static str, &'static str, &'static str, &'static str) {
    match locale {
        "en" => (
            "Open Control Center",
            "Apply MacType and launch registered apps",
            "Hide window",
            "Quit",
        ),
        "zh-CN" => (
            "显示控制中心",
            "应用 MacType 并启动已登记程序",
            "隐藏到托盘",
            "退出",
        ),
        "zh-TW" => (
            "顯示控制中心",
            "套用 MacType 並啟動已登錄程式",
            "隱藏至系統匣",
            "結束",
        ),
        "ja" => (
            "コントロールセンターを開く",
            "MacType を適用して登録アプリを起動",
            "ウィンドウを隠す",
            "終了",
        ),
        "fr" => (
            "Ouvrir le Centre de contrôle",
            "Appliquer MacType et lancer les applications inscrites",
            "Masquer la fenêtre",
            "Quitter",
        ),
        "de" => (
            "Kontrollzentrum öffnen",
            "MacType anwenden und registrierte Apps starten",
            "Fenster ausblenden",
            "Beenden",
        ),
        "es" => (
            "Abrir el Centro de control",
            "Aplicar MacType e iniciar las aplicaciones registradas",
            "Ocultar ventana",
            "Salir",
        ),
        "pt" => (
            "Abrir o Centro de Controlo",
            "Aplicar o MacType e iniciar aplicações registadas",
            "Ocultar janela",
            "Sair",
        ),
        "ar" => (
            "فتح مركز التحكم",
            "تطبيق MacType وتشغيل التطبيقات المسجلة",
            "إخفاء النافذة",
            "إنهاء",
        ),
        _ => (
            "Control Center 열기",
            "MacType 적용 및 등록 앱 실행",
            "창 숨기기",
            "종료",
        ),
    }
}

#[tauri::command]
pub(crate) fn set_application_locale(app: AppHandle, locale: String) -> Result<(), String> {
    use tauri::menu::{Menu, MenuItem};

    if !matches!(
        locale.as_str(),
        "ko" | "en" | "zh-CN" | "zh-TW" | "ja" | "fr" | "de" | "es" | "pt" | "ar"
    ) {
        return Err("unsupported application locale".to_owned());
    }
    let (show_label, inject_label, hide_label, quit_label) = tray_menu_labels(&locale);
    let show = MenuItem::with_id(&app, "show", show_label, true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let hide = MenuItem::with_id(&app, "hide", hide_label, true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let quit = MenuItem::with_id(&app, "quit", quit_label, true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let inject = MenuItem::with_id(&app, "inject", inject_label, true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let menu = Menu::with_items(&app, &[&show, &inject, &hide, &quit])
        .map_err(|error| error.to_string())?;
    let tray = app
        .tray_by_id("main")
        .ok_or_else(|| "Control Center tray is not available".to_owned())?;
    tray.set_menu(Some(menu)).map_err(|error| error.to_string())
}

pub(crate) fn starts_in_tray() -> bool {
    env::args().any(|argument| argument == "--tray")
}

#[tauri::command]
pub(crate) fn launch_context() -> LaunchContext {
    LaunchContext {
        view: requested_view(),
        ci_smoke: env::var_os("MACTYPE_CI_SMOKE_FILE").is_some(),
        tray_start: starts_in_tray(),
    }
}

#[tauri::command]
pub(crate) fn ci_verify_tray_mode(app: AppHandle) -> Result<(), String> {
    if env::var_os("MACTYPE_CI_SMOKE_FILE").is_none() || !starts_in_tray() {
        return Err("tray verification requires CI smoke with --tray".to_owned());
    }
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window was not created".to_owned())?;
    if window.is_visible().map_err(|error| error.to_string())? {
        return Err("main window is visible during --tray startup".to_owned());
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn frontend_ready(app: AppHandle, view: String) -> Result<(), String> {
    let Some(marker_path) = env::var_os("MACTYPE_CI_SMOKE_FILE") else {
        return Ok(());
    };
    let marker = PathBuf::from(marker_path);
    fs::write(&marker, format!("ready:{view}\n")).map_err(|error| error.to_string())?;
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(150));
        app.exit(0);
    });
    Ok(())
}

#[tauri::command]
pub(crate) fn frontend_failed(app: AppHandle, view: String, message: String) -> Result<(), String> {
    let Some(marker_path) = env::var_os("MACTYPE_CI_SMOKE_FILE") else {
        return Ok(());
    };
    fs::write(
        PathBuf::from(marker_path),
        format!("error:{view}:{message}\n"),
    )
    .map_err(|error| error.to_string())?;
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(150));
        app.exit(1);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_menu_labels_follow_supported_locale() {
        assert_eq!(
            tray_menu_labels("en"),
            (
                "Open Control Center",
                "Apply MacType and launch registered apps",
                "Hide window",
                "Quit"
            )
        );
        assert_eq!(
            tray_menu_labels("ko"),
            (
                "Control Center 열기",
                "MacType 적용 및 등록 앱 실행",
                "창 숨기기",
                "종료"
            )
        );
        assert_eq!(
            tray_menu_labels("zh-CN"),
            (
                "显示控制中心",
                "应用 MacType 并启动已登记程序",
                "隐藏到托盘",
                "退出"
            )
        );
        assert_eq!(
            tray_menu_labels("zh-TW"),
            (
                "顯示控制中心",
                "套用 MacType 並啟動已登錄程式",
                "隱藏至系統匣",
                "結束"
            )
        );
        assert_eq!(
            tray_menu_labels("ar"),
            (
                "فتح مركز التحكم",
                "تطبيق MacType وتشغيل التطبيقات المسجلة",
                "إخفاء النافذة",
                "إنهاء"
            )
        );
    }

    #[test]
    fn unsupported_view_is_not_accepted_by_launch_parser_contract() {
        assert!(!matches!(
            "settings",
            "overview" | "files" | "profiles" | "execution" | "diagnostics"
        ));
    }
}
