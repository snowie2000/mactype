#![cfg(windows)]

use std::process::Command;
use std::time::{Duration, Instant};

use mactype_service_host::{subscribe_process_creation, ProcessEventSource, WmiProcessEventSource};

#[test]
fn local_system_compatible_wmi_source_accepts_the_observed_temporary_query() {
    let mut source = WmiProcessEventSource::connect().unwrap();

    subscribe_process_creation(&mut source).unwrap();
    assert!(source
        .snapshot_pids()
        .unwrap()
        .contains(&std::process::id()));
}

#[test]
fn subscribed_wmi_source_reports_the_pid_of_a_new_process() {
    let mut source = WmiProcessEventSource::connect().unwrap();
    subscribe_process_creation(&mut source).unwrap();
    let mut child = Command::new("ping.exe")
        .args(["-n", "6", "127.0.0.1"])
        .spawn()
        .unwrap();
    let expected = child.id();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut observed = false;
    while Instant::now() < deadline {
        if source.next_pid(Duration::from_millis(1500)).unwrap() == Some(expected) {
            observed = true;
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();

    assert!(
        observed,
        "the temporary WMI subscription missed PID {expected}"
    );
}
