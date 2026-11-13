use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::thread;
use std::time::Duration;

use mactype_service_contract::MachinePaths;
use mactype_service_platform::{is_reparse_point, terminate_current_process};

use super::stop_requested;

pub(super) fn spawn_crash_once_adapter(paths: MachinePaths) {
    let Some(data_root) = paths.active_profile().parent().map(Path::to_owned) else {
        return;
    };
    thread::spawn(move || {
        let request = data_root.join("ci-test-adapter").join("crash-once.request");
        while !stop_requested() {
            if consume_crash_once_request(&request).unwrap_or(false) {
                let _ = terminate_current_process(0x4d54_0001);
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
    });
}

fn consume_crash_once_request(request: &Path) -> io::Result<bool> {
    if !request.exists() {
        return Ok(false);
    }
    reject_reparse_ancestors(request)?;
    let mut file = fs::OpenOptions::new().read(true).open(request)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 64 {
        return Ok(false);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.by_ref().take(65).read_to_end(&mut bytes)?;
    if !valid_crash_once_marker(&bytes) {
        return Ok(false);
    }
    drop(file);
    let consumed = request.with_file_name("crash-once.consumed");
    if consumed.exists() {
        reject_reparse_ancestors(&consumed)?;
        let metadata = fs::metadata(&consumed)?;
        if !metadata.is_file() || metadata.len() > 64 {
            return Ok(false);
        }
        fs::remove_file(&consumed)?;
    }
    fs::rename(request, consumed)?;
    Ok(true)
}

pub(super) fn valid_crash_once_marker(bytes: &[u8]) -> bool {
    bytes == b"mactype-ci-crash-once\n"
}

fn reject_reparse_ancestors(path: &Path) -> io::Result<()> {
    for ancestor in path.ancestors().filter(|candidate| candidate.exists()) {
        if is_reparse_point(ancestor)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ci-test-adapter crash marker path contains a reparse point",
            ));
        }
    }
    Ok(())
}
