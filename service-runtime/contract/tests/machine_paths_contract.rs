use std::path::Path;

use mactype_service_contract::{MachinePaths, SERVICE_NAME};

#[test]
fn machine_paths_are_derived_only_below_protected_windows_roots() {
    let paths = MachinePaths::from_trusted_os_roots(
        Path::new(r"C:\Program Files"),
        Path::new(r"C:\ProgramData"),
    )
    .unwrap();

    assert_eq!(SERVICE_NAME, "MacTypeControlCenter");
    assert_eq!(
        paths.service_root(),
        Path::new(r"C:\Program Files\MacType Control Center\Service")
    );
    assert_eq!(
        paths.runtime_versions(),
        Path::new(r"C:\Program Files\MacType Control Center\Service\bin")
    );
    assert_eq!(
        paths.runtime_pointer(),
        Path::new(r"C:\Program Files\MacType Control Center\Service\current.json")
    );
    assert_eq!(
        paths.runtime_activation_journal(),
        Path::new(r"C:\Program Files\MacType Control Center\Service\runtime-activation.json")
    );
    assert_eq!(
        paths.profile_generations(),
        Path::new(r"C:\ProgramData\MacType\ControlCenter\generations")
    );
    assert_eq!(
        paths.active_profile(),
        Path::new(r"C:\ProgramData\MacType\ControlCenter\active.json")
    );
    assert_eq!(
        paths.previous_profile(),
        Path::new(r"C:\ProgramData\MacType\ControlCenter\previous.json")
    );
    assert_eq!(
        paths.profile_activation_journal(),
        Path::new(r"C:\ProgramData\MacType\ControlCenter\profile-activation.json")
    );
    assert_eq!(
        paths.event_log_dir(),
        Path::new(r"C:\ProgramData\MacType\ControlCenter\logs")
    );
    assert_eq!(
        paths.service_host_event_log(),
        Path::new(r"C:\ProgramData\MacType\ControlCenter\logs\service-host.log")
    );
    assert_eq!(
        paths.service_setup_event_log(),
        Path::new(r"C:\ProgramData\MacType\ControlCenter\logs\service-setup.log")
    );
}

#[test]
fn user_writable_or_relative_roots_are_rejected() {
    for (program_files, program_data) in [
        (r"relative\Program Files", r"C:\ProgramData"),
        (r"C:\Program Files", r"relative\ProgramData"),
        (r"C:\Users\person\AppData\Local\Programs", r"C:\ProgramData"),
        (
            r"C:\Program Files",
            r"C:\Users\person\AppData\Local\MacType",
        ),
    ] {
        assert!(MachinePaths::from_trusted_os_roots(
            Path::new(program_files),
            Path::new(program_data)
        )
        .is_err());
    }
}
