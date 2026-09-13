//! Terminal Services process enumeration.

use std::io;
use std::ptr::null_mut;

use windows_sys::Win32::System::RemoteDesktop::{
    WTSEnumerateProcessesW, WTSFreeMemory, WTS_PROCESS_INFOW,
};

use crate::wide::bounded_string;

const MAX_PROCESS_NAME_UNITS: usize = 32_768;

/// One process as Terminal Services reports it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionProcess {
    pub pid: u32,
    pub session_id: u32,
    /// `None` when the name was unreadable or exceeded the fixed bound.
    pub name: Option<String>,
}

/// Every process on the local machine with its session, copied out of the
/// WTS buffer before it is freed.
pub fn interactive_processes() -> io::Result<Vec<SessionProcess>> {
    let mut processes = null_mut();
    let mut count = 0_u32;
    // SAFETY: a null server handle means the local machine; both out pointers
    // are locals that receive a WTS-allocated array and its length.
    if unsafe { WTSEnumerateProcessesW(null_mut(), 0, 1, &mut processes, &mut count) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let list = WtsProcessList(processes);
    if count == 0 {
        return Ok(Vec::new());
    }
    if list.0.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "WTS reported processes without a buffer",
        ));
    }
    // SAFETY: WTS returned `count` contiguous entries in `list.0`, which stays
    // allocated until `list` is dropped after this copy.
    let entries = unsafe { std::slice::from_raw_parts(list.0, count as usize) };
    Ok(entries
        .iter()
        .map(|entry| SessionProcess {
            pid: entry.ProcessId,
            session_id: entry.SessionId,
            // SAFETY: the name pointer belongs to the same WTS allocation and
            // is read within a fixed bound.
            name: unsafe { bounded_string(entry.pProcessName, MAX_PROCESS_NAME_UNITS) },
        })
        .collect())
}

struct WtsProcessList(*mut WTS_PROCESS_INFOW);

impl Drop for WtsProcessList {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the buffer was allocated by WTSEnumerateProcessesW and is
            // freed exactly once.
            unsafe { WTSFreeMemory(self.0.cast()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::interactive_processes;

    #[test]
    fn the_current_process_appears_with_its_session_and_name() {
        let processes = interactive_processes().unwrap();
        let own = processes
            .iter()
            .find(|process| process.pid == std::process::id())
            .expect("the enumerating process is listed");
        assert!(own.name.as_deref().is_some_and(|name| !name.is_empty()));
        assert_eq!(
            own.session_id,
            crate::process::process_session_id(std::process::id()).unwrap()
        );
    }
}
