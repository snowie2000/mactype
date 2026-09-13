use super::super::*;

#[cfg(windows)]
static PROFILE_PIPE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn broker_result_frame_preserves_a_multistage_failure_chain() {
    let nonce = [0x2a; PROFILE_TRANSFER_NONCE_BYTES];
    let result = BrokerResultMessage::failure(
        "migrate-from-legacy",
        "activate open service",
        "setup broker start failed: CreateServiceW failed with Win32 5 (Access is denied); legacy service restore completed",
    );

    let frame = encode_broker_result_frame(&result, &nonce).unwrap();
    let decoded = decode_broker_result_frame(&frame, &nonce).unwrap();

    assert_eq!(decoded, result);
    assert_eq!(decoded.disposition, BrokerResultDisposition::Failure);
    assert!(decoded.error_chain.contains("CreateServiceW"));
    assert!(decoded
        .error_chain
        .contains("legacy service restore completed"));
}

#[cfg(windows)]
#[test]
fn broker_result_pipe_returns_the_child_failure_to_the_parent() {
    let _serial = PROFILE_PIPE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let nonce = [0x39; PROFILE_TRANSFER_NONCE_BYTES];
    let server = windows::BrokerResultPipeServer::create_with_nonce(nonce).unwrap();
    let token = server.token().clone();
    let child = std::thread::spawn(move || {
        let writer =
            windows::BrokerResultPipeWriter::connect(&token, std::time::Duration::from_secs(2))
                .unwrap();
        writer
            .send(
                &BrokerResultMessage::failure(
                    "migrate-from-legacy",
                    "activate open service",
                    "setup broker start failed with Win32 5 (Access is denied)",
                ),
                std::time::Duration::from_secs(2),
            )
            .unwrap();
    });

    let result = server
        .receive_from(std::process::id(), None, std::time::Duration::from_secs(2))
        .unwrap();
    child.join().unwrap();

    assert_eq!(result.disposition, BrokerResultDisposition::Failure);
    assert_eq!(result.stage, "activate open service");
    assert!(result.error_chain.contains("Win32 5"));
}

#[test]
fn profile_transfer_frame_is_versioned_bounded_nonce_bound_and_hashed() {
    let nonce = [0x5a; PROFILE_TRANSFER_NONCE_BYTES];
    let payload = b"[General]\r\nGammaValue=1.2\r\n";
    let frame = encode_profile_transfer_frame(payload, &nonce).unwrap();
    assert_eq!(
        decode_profile_transfer_frame(&frame, &nonce).unwrap(),
        payload
    );

    for index in [0_usize, 4, 8, 12, 28] {
        let mut malformed = frame.clone();
        malformed[index] ^= 0xff;
        assert!(decode_profile_transfer_frame(&malformed, &nonce).is_err());
    }
    assert!(decode_profile_transfer_frame(&frame, &[0x6b; PROFILE_TRANSFER_NONCE_BYTES]).is_err());
    assert!(encode_profile_transfer_frame(&[], &nonce).is_err());
    assert!(encode_profile_transfer_frame(
        &vec![0_u8; mactype_service_contract::MAX_PROFILE_BYTES + 1],
        &nonce,
    )
    .is_err());
}

#[cfg(windows)]
#[test]
fn profile_pipe_is_first_instance_peer_bound_and_bounded_by_time() {
    let _serial = PROFILE_PIPE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let payload = b"[General]\r\nGammaValue=1.3\r\n";
    // No Authenticated-Users ACE: only the elevated broker (SY/BA) and the pipe
    // owner (OW) may read, so a local process cannot first-connect and DoS the
    // single pipe instance during the UAC window.
    assert_eq!(
        windows::PROFILE_PIPE_SDDL,
        "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;OW)"
    );
    let first_nonce = [0x3c; PROFILE_TRANSFER_NONCE_BYTES];
    let first = windows::ProfilePipeServer::create_with_nonce(payload, first_nonce).unwrap();
    assert!(windows::ProfilePipeServer::create_with_nonce(payload, first_nonce).is_err());
    drop(first);

    let server = windows::ProfilePipeServer::create(payload).unwrap();
    let token = server.token().clone();
    let client = std::thread::spawn(move || {
        windows::receive_profile_from_pipe_bounded(&token, std::time::Duration::from_secs(2))
    });
    server
        .send_to(std::process::id(), None, std::time::Duration::from_secs(2))
        .unwrap();
    assert_eq!(client.join().unwrap().unwrap(), payload);

    let attacked_nonce = [0x4d; PROFILE_TRANSFER_NONCE_BYTES];
    let attacked = windows::ProfilePipeServer::create_with_nonce(payload, attacked_nonce).unwrap();
    let token = attacked.token().clone();
    let attacker = std::thread::spawn(move || {
        windows::receive_profile_from_pipe_bounded(&token, std::time::Duration::from_secs(2))
    });
    assert!(attacked
        .send_to(
            std::process::id().wrapping_add(1),
            None,
            std::time::Duration::from_secs(2),
        )
        .is_err());
    assert!(attacker.join().unwrap().is_err());
    drop(windows::ProfilePipeServer::create_with_nonce(payload, attacked_nonce).unwrap());

    let idle_nonce = [0x6e; PROFILE_TRANSFER_NONCE_BYTES];
    let idle = windows::ProfilePipeServer::create_with_nonce(payload, idle_nonce).unwrap();
    assert!(idle
        .send_to(
            std::process::id(),
            None,
            std::time::Duration::from_millis(25),
        )
        .is_err());
    drop(windows::ProfilePipeServer::create_with_nonce(payload, idle_nonce).unwrap());

    let stalled_nonce = [0x7f; PROFILE_TRANSFER_NONCE_BYTES];
    let stalled = windows::ProfilePipeServer::create_with_nonce(payload, stalled_nonce).unwrap();
    let token = stalled.token().clone();
    let stalled_client = std::thread::spawn(move || {
        windows::receive_profile_from_pipe_bounded(&token, std::time::Duration::from_millis(25))
    });
    assert!(stalled_client.join().unwrap().is_err());
    drop(stalled);
    drop(windows::ProfilePipeServer::create_with_nonce(payload, stalled_nonce).unwrap());
}

#[cfg(windows)]
#[test]
fn profile_pipe_send_times_out_when_the_expected_broker_never_reads() {
    use std::os::windows::fs::OpenOptionsExt;

    let _serial = PROFILE_PIPE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let payload = vec![b'x'; mactype_service_contract::MAX_PROFILE_BYTES];
    let nonce = [0x91; PROFILE_TRANSFER_NONCE_BYTES];
    let server = windows::ProfilePipeServer::create_with_nonce(&payload, nonce).unwrap();
    let token = server.token().clone();
    let pipe_name = format!(
        r"\\.\pipe\MacTypeControlCenter.profile.v1.{}.{}",
        token.server_pid,
        token
            .nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let (connected_tx, connected_rx) = std::sync::mpsc::channel();
    let client = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let pipe = loop {
            let mut options = std::fs::OpenOptions::new();
            options
                .read(true)
                .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OVERLAPPED);
            match options.open(&pipe_name) {
                Ok(pipe) => break pipe,
                Err(_) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                Err(error) => panic!("stalled broker could not connect: {error}"),
            }
        };
        connected_tx.send(()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(250));
        drop(pipe);
    });
    connected_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();

    let started = std::time::Instant::now();
    let error = server
        .send_to(
            std::process::id(),
            None,
            std::time::Duration::from_millis(25),
        )
        .unwrap_err();
    assert!(error.contains("timed out"), "{error}");
    assert!(started.elapsed() < std::time::Duration::from_millis(200));
    client.join().unwrap();
    drop(windows::ProfilePipeServer::create_with_nonce(&payload, nonce).unwrap());

    let mut exited_child = std::process::Command::new("cmd")
        .args(["/d", "/c", "exit 0"])
        .spawn()
        .unwrap();
    exited_child.wait().unwrap();
    let exited = mactype_service_platform::Process::from_child(&exited_child).unwrap();
    let exit_nonce = [0x92; PROFILE_TRANSFER_NONCE_BYTES];
    let server = windows::ProfilePipeServer::create_with_nonce(&payload, exit_nonce).unwrap();
    let token = server.token().clone();
    let pipe_name = format!(
        r"\\.\pipe\MacTypeControlCenter.profile.v1.{}.{}",
        token.server_pid,
        token
            .nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let (connected_tx, connected_rx) = std::sync::mpsc::channel();
    let client = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let pipe = loop {
            let mut options = std::fs::OpenOptions::new();
            options
                .read(true)
                .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OVERLAPPED);
            match options.open(&pipe_name) {
                Ok(pipe) => break pipe,
                Err(_) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                Err(error) => panic!("exiting broker could not connect: {error}"),
            }
        };
        connected_tx.send(()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(250));
        drop(pipe);
    });
    connected_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    windows::reset_profile_pipe_reap_count();
    let started = std::time::Instant::now();
    let error = server
        .send_to(
            std::process::id(),
            Some(&exited),
            std::time::Duration::from_secs(2),
        )
        .unwrap_err();
    assert!(error.contains("broker exited"), "{error}");
    assert!(started.elapsed() < std::time::Duration::from_millis(200));
    assert_eq!(windows::profile_pipe_reap_count(), 1);
    client.join().unwrap();
    drop(windows::ProfilePipeServer::create_with_nonce(&payload, exit_nonce).unwrap());
}

#[cfg(windows)]
#[test]
fn profile_pipe_read_error_cancels_and_reaps_the_pending_operation() {
    let _serial = PROFILE_PIPE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let payload = b"[General]\r\nGammaValue=1.3\r\n";
    let nonce = [0xa2; PROFILE_TRANSFER_NONCE_BYTES];
    let server = windows::ProfilePipeServer::create_with_nonce(payload, nonce).unwrap();
    let token = server.token().clone();
    let client = std::thread::spawn(move || {
        windows::reset_profile_pipe_reap_count();
        let result =
            windows::receive_profile_from_pipe_bounded(&token, std::time::Duration::from_secs(2));
        (result, windows::profile_pipe_reap_count())
    });
    let (result, reap_count) = client.join().unwrap();
    assert!(result.is_err());
    assert_eq!(reap_count, 1);
    drop(server);
    drop(windows::ProfilePipeServer::create_with_nonce(payload, nonce).unwrap());
}

#[cfg(windows)]
#[test]
fn broker_termination_distinguishes_exit_and_reports_unconfirmed_cleanup() {
    use std::collections::VecDeque;

    struct FakeBrokerProcessControl {
        waits: VecDeque<std::io::Result<mactype_service_platform::WaitOutcome>>,
        terminate_error: Option<i32>,
        wait_timeouts: Vec<std::time::Duration>,
    }

    impl windows::BrokerProcessControl for FakeBrokerProcessControl {
        fn wait(
            &mut self,
            _process: &mactype_service_platform::Process,
            timeout: std::time::Duration,
        ) -> std::io::Result<mactype_service_platform::WaitOutcome> {
            self.wait_timeouts.push(timeout);
            self.waits.pop_front().unwrap()
        }

        fn terminate(
            &mut self,
            _process: &mactype_service_platform::Process,
            _exit_code: u32,
        ) -> std::io::Result<()> {
            self.terminate_error
                .map_or(Ok(()), |code| Err(std::io::Error::from_raw_os_error(code)))
        }
    }

    let process = mactype_service_platform::Process::open(
        std::process::id(),
        mactype_service_platform::ProcessAccess::QueryLimited,
    )
    .unwrap();
    let mut already_exited = FakeBrokerProcessControl {
        waits: VecDeque::from([Ok(mactype_service_platform::WaitOutcome::Signaled)]),
        terminate_error: None,
        wait_timeouts: Vec::new(),
    };
    assert_eq!(
        windows::terminate_broker_process_with(&process, &mut already_exited).unwrap(),
        windows::BrokerTermination::AlreadyExited
    );
    assert_eq!(already_exited.wait_timeouts, [std::time::Duration::ZERO]);

    let mut terminated = FakeBrokerProcessControl {
        waits: VecDeque::from([
            Ok(mactype_service_platform::WaitOutcome::TimedOut),
            Ok(mactype_service_platform::WaitOutcome::Signaled),
        ]),
        terminate_error: None,
        wait_timeouts: Vec::new(),
    };
    assert_eq!(
        windows::terminate_broker_process_with(&process, &mut terminated).unwrap(),
        windows::BrokerTermination::Terminated
    );
    assert_eq!(
        terminated.wait_timeouts,
        [
            std::time::Duration::ZERO,
            std::time::Duration::from_millis(5_000)
        ]
    );

    let mut unconfirmed = FakeBrokerProcessControl {
        waits: VecDeque::from([
            Ok(mactype_service_platform::WaitOutcome::TimedOut),
            Ok(mactype_service_platform::WaitOutcome::TimedOut),
        ]),
        terminate_error: None,
        wait_timeouts: Vec::new(),
    };
    let cleanup = windows::terminate_broker_process_with(&process, &mut unconfirmed);
    let error = windows::combine_broker_cleanup_error("profile transfer failed", cleanup);
    assert!(error.contains("profile transfer failed"), "{error}");
    assert!(error.contains("cleanup is unknown"), "{error}");
    assert!(error.contains("5000"), "{error}");
    assert_eq!(
        unconfirmed.wait_timeouts,
        [
            std::time::Duration::ZERO,
            std::time::Duration::from_millis(5_000)
        ]
    );
}
