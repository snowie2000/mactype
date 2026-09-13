use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;

use crate::HelperLaunchStage;

static STOP_DURING_HELPER: AtomicBool = AtomicBool::new(false);
static HELPER_TEST_LOCK: Mutex<()> = Mutex::new(());

fn stop_during_helper() -> bool {
    STOP_DURING_HELPER.load(Ordering::Acquire)
}

fn current_process_invocation(executable: PathBuf, timeout: Duration) -> HelperInvocation {
    let process = Process::open(std::process::id(), ProcessAccess::QueryLimited).unwrap();
    HelperInvocation {
        executable,
        target: crate::ProcessIdentity {
            pid: std::process::id(),
            creation_time: process.creation_time().unwrap(),
            session_id: 1,
            architecture: crate::ProcessArchitecture::X64,
            protected: false,
            critical: false,
        },
        generation_id: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            .to_owned(),
        timeout,
    }
}

fn forget_last_child() {
    *LAST_TEST_CHILD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

/// The launcher's last child must be gone, and promptly. The launcher
/// terminates the child's job and then waits up to TERMINATION_CONFIRMATION
/// for the process object to signal, but that wait is bounded by the same
/// absolute deadline, so on a loaded host the kernel can still be tearing the
/// process down when the launcher returns. No user-mode code runs in the
/// child after the terminate call, so the only thing left to prove is that
/// the exact process object signals soon after: the bound here is well below
/// the child's own five second sleep, so a helper the launcher failed to
/// terminate would still fail this check.
fn assert_last_child_exited() {
    let child = LAST_TEST_CHILD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
        .expect("the launcher recorded the child it created");
    assert_eq!(
        child.wait(Some(Duration::from_secs(2))).unwrap(),
        WaitOutcome::Signaled
    );
}

#[test]
fn helper_job_enforces_single_process_and_kill_on_close() {
    let job = JobObject::single_process_kill_on_close().unwrap();
    let limits = job.limits().unwrap();
    assert_eq!(limits.active_process_limit, 1);
    assert!(limits.active_process_limit_enabled);
    assert!(limits.kill_on_close);
}

#[test]
fn in_flight_helper_is_cancelled_without_waiting_for_its_twenty_second_timeout() {
    let _guard = HELPER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    STOP_DURING_HELPER.store(false, Ordering::Release);
    forget_last_child();
    let launcher = WindowsHelperLauncher::new(stop_during_helper);
    let executable = PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
    let invocation = current_process_invocation(executable, Duration::from_secs(20));
    let stop = thread::spawn(|| {
        thread::sleep(Duration::from_millis(100));
        STOP_DURING_HELPER.store(true, Ordering::Release);
    });
    let started = Instant::now();
    let error = launcher
        .launch_process(&invocation, |_| {
            [
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 20; exit 0",
            ]
            .into_iter()
            .map(OsString::from)
            .collect()
        })
        .unwrap_err();
    stop.join().unwrap();
    STOP_DURING_HELPER.store(false, Ordering::Release);
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(error.stage(), HelperLaunchStage::AfterResume);
    assert!(started.elapsed() < Duration::from_secs(3));

    assert_last_child_exited();
}

#[test]
fn absolute_timeout_terminates_the_helper_job_without_an_orphan() {
    let _guard = HELPER_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    STOP_DURING_HELPER.store(false, Ordering::Release);
    forget_last_child();
    let launcher = WindowsHelperLauncher::new(stop_during_helper);
    let executable = PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
    let invocation = current_process_invocation(executable, Duration::from_millis(700));
    let started = Instant::now();
    let error = launcher
        .launch_process(&invocation, |_| {
            [
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 5; exit 0",
            ]
            .into_iter()
            .map(OsString::from)
            .collect()
        })
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert_eq!(error.stage(), HelperLaunchStage::AfterResume);
    // The lower bound proves the launcher waited for the absolute timeout
    // instead of failing outright. The upper bound only has to exclude the
    // child's own five second sleep; holding it to 750 ms measured the host
    // scheduler, and a busy machine lost that margin.
    assert!(started.elapsed() >= Duration::from_millis(400));
    assert!(started.elapsed() < Duration::from_secs(4));

    assert_last_child_exited();
}

#[test]
fn closing_the_service_owned_job_terminates_a_running_helper() {
    let job = JobObject::single_process_kill_on_close().unwrap();
    let executable = PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
    let arguments = [
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Start-Sleep -Seconds 5; exit 0",
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    let null = null_device().unwrap();
    let child = SuspendedChild::create(&ProcessLaunch {
        executable: &executable,
        arguments: &arguments,
        inherit: &[&null],
        standard: StandardHandles {
            input: &null,
            output: &null,
            error: &null,
        },
    })
    .unwrap();
    job.assign(child.process()).unwrap();
    let process = child.resume().unwrap();
    assert_eq!(
        process.wait(Some(Duration::ZERO)).unwrap(),
        WaitOutcome::TimedOut
    );

    drop(job);

    assert_eq!(
        process.wait(Some(Duration::from_secs(2))).unwrap(),
        WaitOutcome::Signaled
    );
}
