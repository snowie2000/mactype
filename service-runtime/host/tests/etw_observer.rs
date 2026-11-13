#![cfg(windows)]

//! The ETW process observer against the live kernel provider.
//!
//! Starting a real-time trace session needs an elevated or LocalSystem token,
//! so on an ordinary developer shell this file prints why it is doing nothing
//! and returns. It is also the empirical check on the one claim the payload
//! decoder rests on: that `ProcessStart` begins with the new process's
//! `ProcessID`. A child whose PID we already know has to come back under that
//! exact number.

use std::process::Command;
use std::time::{Duration, Instant};

use mactype_service_host::{EtwProcessEventSource, ProcessEventSource};

/// Three seconds is far longer than the session's 25 ms flush timer needs; it
/// is the bound that fails the test rather than the time it expects to take.
const ARRIVAL_BUDGET: Duration = Duration::from_secs(3);

fn unique_session_name(purpose: &str) -> String {
    format!("MacType-Test-{purpose}-{}", std::process::id())
}

/// Whether this process can open a real-time trace session at all. The token
/// check is the session itself: anything else would guess at the rule the
/// kernel applies.
fn source_or_skip(purpose: &str) -> Option<EtwProcessEventSource> {
    match EtwProcessEventSource::start(unique_session_name(purpose)) {
        Ok(source) => Some(source),
        Err(error) => {
            println!(
                "skipped: a real-time trace session needs an elevated or LocalSystem token \
                 ({}: {}, win32={:?})",
                error.code, error.message, error.win32_error
            );
            None
        }
    }
}

#[test]
fn the_snapshot_reaches_every_process_that_started_before_the_session() {
    let Some(mut source) = source_or_skip("snapshot") else {
        return;
    };

    source.subscribe("").unwrap();
    let pids = source.snapshot_pids().unwrap();

    assert!(pids.contains(&std::process::id()));
}

#[test]
fn a_child_arrives_under_the_process_id_its_payload_claims() {
    let Some(mut source) = source_or_skip("child") else {
        return;
    };
    source.subscribe("").unwrap();

    let mut child = Command::new("cmd.exe")
        .args(["/d", "/c", "exit 0"])
        .spawn()
        .unwrap();
    let expected = child.id();
    child.wait().unwrap();

    let deadline = Instant::now() + ARRIVAL_BUDGET;
    let mut observed = false;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match source.next_pid(remaining.min(Duration::from_millis(250))) {
            Ok(Some(pid)) if pid == expected => {
                observed = true;
                break;
            }
            Ok(_) => {}
            Err(error) => panic!("the ETW observer failed: {} {}", error.code, error.message),
        }
    }

    assert!(
        observed,
        "the kernel process provider never announced PID {expected}, so the ProcessStart \
         payload does not begin with ProcessID as the decoder assumes"
    );
}
