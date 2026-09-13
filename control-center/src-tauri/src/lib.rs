#![forbid(unsafe_code)]

mod app;
mod bounded_io;
mod desktop_shell;
mod diagnostics;
mod execution;
mod fonts;
mod generated_settings;
mod machine_integration;
mod preview;
mod profile;
mod service_contract;
mod single_instance;

use std::{env, path::PathBuf};

pub(crate) fn installation_root() -> Option<PathBuf> {
    env::var_os("MACTYPE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            env::current_exe()
                .ok()
                .and_then(|executable| executable.parent().map(PathBuf::from))
                .filter(|path| path.join("MacType.dll").is_file())
        })
        .or_else(|| env::var_os("ProgramFiles").map(|path| PathBuf::from(path).join("MacType")))
}

pub fn dispatch_privileged_command() -> Option<i32> {
    machine_integration::dispatch_privileged_command()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let startup_gate = single_instance::StartupGate::acquire()
        .expect("failed to acquire the single-instance startup gate");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            let restored = app::restore_main_window(app).is_ok();
            let _ = single_instance::record_activation(args, cwd, restored);
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(profile::ProfileState::default())
        .manage(preview::PreviewState::default())
        .invoke_handler(tauri::generate_handler![
            app::launch_context,
            fonts::installed_font_families,
            app::set_application_locale,
            preview::scan_installation,
            preview::rediscover_installation,
            preview::reconnect_preview,
            diagnostics::diagnostic_report,
            diagnostics::diagnostic_recent_logs,
            diagnostics::list_events,
            diagnostics::event_log_summary,
            diagnostics::recent_activity,
            diagnostics::export_diagnostics,
            diagnostics::copy_diagnostics,
            diagnostics::open_log_folder,
            execution::execution_status,
            execution::request_legacy_tray_exit,
            execution::disable_legacy_tray_autostart,
            execution::manage_system_service,
            machine_integration::reveal_system_service,
            execution::set_session_autostart,
            execution::launch_with_mactype,
            execution::list_manual_launch_candidates,
            execution::apply_open_profile,
            execution::activate_system_injection,
            execution::register_session_target,
            execution::remove_session_target,
            execution::launch_registered_targets,
            profile::list_profiles,
            profile::open_profile,
            profile::open_default_profile,
            profile::current_profile,
            profile::discover_legacy_profile,
            profile::import_profile,
            profile::update_profile_setting,
            profile::update_profile_individuals,
            profile::update_profile_list,
            profile::update_profile_advanced,
            profile::undo_profile,
            profile::redo_profile,
            profile::discard_profile_changes,
            profile::reset_profile_defaults,
            profile::export_profile,
            profile::reveal_profile_file,
            profile::duplicate_profile,
            profile::save_profile,
            preview::render_profile_preview,
            preview::set_native_preview,
            preview::preview_diagnostics,
            preview::ci_force_preview_crash,
            profile::ci_verify_profile_workflow,
            execution::ci_verify_injection_workflow,
            app::ci_verify_tray_mode,
            app::frontend_ready,
            app::frontend_failed
        ])
        .setup(move |app| desktop_shell::install(app, startup_gate))
        .on_window_event(desktop_shell::handle_window_event)
        .run(tauri::generate_context!())
        .expect("failed to run MacType Control Center");
}
