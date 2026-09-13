use super::super::*;

#[cfg(windows)]
#[test]
fn marker_layout_explicitly_rejects_native_arm64() {
    use windows_sys::Win32::System::SystemInformation::{
        IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64, IMAGE_FILE_MACHINE_I386,
        IMAGE_FILE_MACHINE_UNKNOWN,
    };

    assert!(windows::marker_x64_system_directory(
        IMAGE_FILE_MACHINE_UNKNOWN,
        IMAGE_FILE_MACHINE_ARM64,
    )
    .is_err());
    assert_eq!(
        windows::marker_x64_system_directory(IMAGE_FILE_MACHINE_UNKNOWN, IMAGE_FILE_MACHINE_AMD64,)
            .unwrap(),
        "System32"
    );
    assert_eq!(
        windows::marker_x64_system_directory(IMAGE_FILE_MACHINE_I386, IMAGE_FILE_MACHINE_AMD64,)
            .unwrap(),
        "Sysnative"
    );
}

#[cfg(windows)]
#[test]
fn privileged_descendant_job_kills_a_running_child_when_its_owner_disappears() {
    use mactype_service_platform::{Process, WaitOutcome};
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::Duration;
    use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

    let job = windows::KillOnCloseJob::new().unwrap();
    assert!(job.kill_on_close_enabled().unwrap());
    let mut child = Command::new(r"C:\Windows\System32\ping.exe")
        .args(["-n", "30", "-w", "1000", "127.0.0.1"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .unwrap();
    let process = Process::from_child(&child).unwrap();
    job.assign(&process).unwrap();
    assert_eq!(
        process.wait(Some(Duration::ZERO)).unwrap(),
        WaitOutcome::TimedOut
    );

    drop(job);

    assert_eq!(
        process.wait(Some(Duration::from_secs(2))).unwrap(),
        WaitOutcome::Signaled
    );
    child.wait().unwrap();
}

#[cfg(windows)]
#[test]
fn absent_snapshot_never_authorizes_deleting_an_unreceipted_or_changed_pointer() {
    use windows::FileRollbackAction;

    assert_eq!(
        windows::plan_file_rollback(None, Some(Some(b"owned")), Some(b"owned")).unwrap(),
        FileRollbackAction::Remove
    );
    for (receipt, current) in [
        (None, Some(b"foreign".as_slice())),
        (Some(Some(b"owned".as_slice())), Some(b"changed".as_slice())),
    ] {
        let error = windows::plan_file_rollback(None, receipt, current).unwrap_err();
        assert!(error.contains("cleanup is unknown"), "{error}");
    }
}

#[cfg(windows)]
#[test]
fn generation_cleanup_requires_an_exact_transaction_manifest_receipt() {
    use std::collections::{BTreeMap, BTreeSet};

    let before = BTreeSet::from(["before".to_owned()]);
    let owned_manifest = BTreeMap::from([("profile.ini".to_owned(), "sha256:owned".to_owned())]);
    let receipts = BTreeMap::from([("owned".to_owned(), owned_manifest.clone())]);
    let current = BTreeMap::from([
        ("before".to_owned(), BTreeMap::new()),
        ("owned".to_owned(), owned_manifest.clone()),
    ]);
    assert_eq!(
        windows::plan_generation_cleanup(&before, &receipts, &current).unwrap(),
        ["owned".to_owned()]
    );

    let foreign = BTreeMap::from([
        ("before".to_owned(), BTreeMap::new()),
        ("foreign".to_owned(), owned_manifest.clone()),
    ]);
    assert!(
        windows::plan_generation_cleanup(&before, &receipts, &foreign)
            .unwrap_err()
            .contains("cleanup is unknown")
    );

    let changed = BTreeMap::from([
        ("before".to_owned(), BTreeMap::new()),
        (
            "owned".to_owned(),
            BTreeMap::from([("profile.ini".to_owned(), "sha256:changed".to_owned())]),
        ),
    ]);
    assert!(
        windows::plan_generation_cleanup(&before, &receipts, &changed)
            .unwrap_err()
            .contains("cleanup is unknown")
    );
}

#[cfg(windows)]
#[test]
fn absent_service_root_preserves_unreceipted_content_and_reports_cleanup_unknown() {
    let root = std::env::temp_dir().join(format!(
        "mactype-absent-root-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let foreign = root.join("foreign.txt");
    std::fs::write(&foreign, b"not owned by the migration").unwrap();

    let error = windows::remove_empty_directory(&root).unwrap_err();

    assert!(error.contains("cleanup is unknown"), "{error}");
    assert!(foreign.is_file());
    std::fs::remove_file(foreign).unwrap();
    std::fs::remove_dir(root).unwrap();
}
