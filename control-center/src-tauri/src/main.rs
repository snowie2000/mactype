#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

fn main() {
    if let Some(exit_code) = mactype_control_center_lib::dispatch_privileged_command() {
        std::process::exit(exit_code);
    }
    mactype_control_center_lib::run();
}
