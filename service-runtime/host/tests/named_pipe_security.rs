#![cfg(windows)]

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use mactype_service_contract::HealthReport;
use mactype_service_host::{HealthPublisher, NamedPipeHealthPublisher, HEALTH_PIPE_SECURITY_SDDL};
use mactype_service_platform::NamedPipeClient;

#[test]
fn health_pipe_acl_is_explicit_read_only_for_authenticated_users() {
    assert_eq!(
        HEALTH_PIPE_SECURITY_SDDL,
        "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GR;;;AU)"
    );
    assert!(HEALTH_PIPE_SECURITY_SDDL.contains("(A;;GA;;;SY)"));
    assert!(HEALTH_PIPE_SECURITY_SDDL.contains("(A;;GA;;;BA)"));
    assert!(HEALTH_PIPE_SECURITY_SDDL.contains("(A;;GR;;;AU)"));
    assert!(!HEALTH_PIPE_SECURITY_SDDL.contains("(A;;GW;;;AU)"));
    assert!(!HEALTH_PIPE_SECURITY_SDDL.contains("(A;;GA;;;AU)"));
}

#[test]
fn stalled_health_client_cannot_block_service_shutdown() {
    let pipe_name = format!(
        r"\\.\pipe\MacTypeControlCenter.health.shutdown-test.{}",
        std::process::id()
    );
    let publisher = NamedPipeHealthPublisher::start(&pipe_name).unwrap();
    publisher
        .publish(&HealthReport::ready(
            "test",
            Some(
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_owned(),
            ),
        ))
        .unwrap();

    // A client that connects and never reads: the server must not wait on it.
    let client = NamedPipeClient::open_read(&pipe_name).unwrap();
    thread::sleep(Duration::from_millis(250));

    let (finished_tx, finished_rx) = mpsc::channel();
    let started = Instant::now();
    let drop_worker = thread::spawn(move || {
        drop(publisher);
        let _ = finished_tx.send(());
    });
    let bounded = finished_rx.recv_timeout(Duration::from_secs(1)).is_ok();
    drop(client);
    drop_worker.join().unwrap();

    assert!(bounded, "stalled health client blocked publisher shutdown");
    assert!(started.elapsed() < Duration::from_secs(2));
}
