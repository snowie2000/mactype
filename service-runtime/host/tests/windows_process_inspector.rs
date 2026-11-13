#![cfg(windows)]

use mactype_service_host::{ProcessInspector, WindowsProcessInspector};
use mactype_service_platform::process_session_id;

#[test]
fn windows_inspector_reports_identity_and_facts_for_the_current_process() {
    let pid = std::process::id();
    let inspector = WindowsProcessInspector::new();

    let inspected = inspector.inspect(pid).unwrap();

    assert_eq!(inspected.identity.pid, pid);
    assert!(inspected.identity.creation_time > 0);
    assert_eq!(
        inspected.identity.session_id,
        process_session_id(pid).unwrap()
    );
    assert!(!inspected.identity.protected);
    assert!(!inspected.facts.critical_or_unknown);
    assert!(!inspected.facts.prohibits_dynamic_code);
    assert!(!inspected.facts.restricts_binary_signature);
    let expected_name = std::env::current_exe()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_ascii_lowercase();
    assert_eq!(inspected.facts.image_name.as_deref(), Some(&*expected_name));
}
