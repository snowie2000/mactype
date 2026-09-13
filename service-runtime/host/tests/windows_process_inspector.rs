#![cfg(windows)]

use mactype_service_host::{ProcessInspector, WindowsProcessInspector};
use mactype_service_platform::process_session_id;

#[test]
fn windows_inspector_requeries_creation_time_session_and_architecture_from_the_process() {
    let pid = std::process::id();
    let inspector = WindowsProcessInspector::new(pid.wrapping_add(1));

    let identity = inspector.inspect(pid).unwrap();

    assert_eq!(identity.pid, pid);
    assert!(identity.creation_time > 0);
    assert_eq!(identity.session_id, process_session_id(pid).unwrap());
}
