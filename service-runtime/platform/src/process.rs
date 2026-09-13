//! Process handles and the identity queries the service makes on them.

use std::ffi::OsStr;
use std::io;
use std::mem::size_of;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use windows_sys::Wdk::System::SystemServices::PROCESS_EXTENDED_BASIC_INFORMATION;
use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
use windows_sys::Win32::Foundation::{
    DuplicateHandle, RtlNtStatusToDosError, DUPLICATE_SAME_ACCESS, ERROR_ACCESS_DENIED,
    ERROR_INVALID_DATA, ERROR_MR_MID_NOT_FOUND, FILETIME, STATUS_ACCESS_DENIED, STILL_ACTIVE,
};
use windows_sys::Win32::Storage::FileSystem::SYNCHRONIZE;
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::SystemInformation::{
    IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_I386, IMAGE_FILE_MACHINE_UNKNOWN,
};
use windows_sys::Win32::System::SystemServices::{
    PROCESS_MITIGATION_BINARY_SIGNATURE_POLICY, PROCESS_MITIGATION_DYNAMIC_CODE_POLICY,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, GetProcessId, GetProcessInformation,
    GetProcessMitigationPolicy, GetProcessTimes, IsProcessCritical, IsWow64Process2, OpenProcess,
    ProcessDynamicCodePolicy, ProcessProtectionLevelInfo, ProcessSignaturePolicy,
    QueryFullProcessImageNameW, TerminateProcess, PROCESS_ALL_ACCESS, PROCESS_CREATE_THREAD,
    PROCESS_NAME_WIN32, PROCESS_PROTECTION_LEVEL_INFORMATION, PROCESS_QUERY_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
    PROTECTION_LEVEL_NONE,
};

use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};

use crate::handle::{OwnedHandle, WaitOutcome};
use crate::wide::{wide_null, wide_path};

const MAX_IMAGE_PATH_UNITS: usize = 32_768;

/// The exact access a process handle is opened with. Each variant is a fixed
/// rights set the service needs; there is no way to request arbitrary rights.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessAccess {
    /// `PROCESS_QUERY_LIMITED_INFORMATION`: identity and liveness queries.
    QueryLimited,
    /// `SYNCHRONIZE` only: waiting for exit.
    Synchronize,
    /// Identity and liveness queries together with waiting for exit.
    QueryLimitedAndSynchronize,
    /// The rights `CreateProcess` granted the spawning process: everything.
    /// Only [`Process::from_child`] produces this; [`Process::open`] refuses
    /// it, so a PID can never be reopened with full access.
    AsSpawned,
    /// The rights the fixed injection helper needs on its target. The handle
    /// is opened inheritable because it reaches the helper through handle
    /// inheritance and never by PID.
    InjectionTarget,
}

impl ProcessAccess {
    fn rights(self) -> u32 {
        match self {
            Self::QueryLimited => PROCESS_QUERY_LIMITED_INFORMATION,
            Self::Synchronize => SYNCHRONIZE,
            Self::QueryLimitedAndSynchronize => PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
            Self::AsSpawned => PROCESS_ALL_ACCESS,
            Self::InjectionTarget => {
                PROCESS_CREATE_THREAD
                    | PROCESS_QUERY_INFORMATION
                    | PROCESS_QUERY_LIMITED_INFORMATION
                    | PROCESS_VM_OPERATION
                    | PROCESS_VM_WRITE
                    | PROCESS_VM_READ
                    | SYNCHRONIZE
            }
        }
    }

    fn inheritable(self) -> bool {
        matches!(self, Self::InjectionTarget)
    }
}

/// The machine a process runs as, next to the machine the OS runs on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessMachine {
    pub process: MachineKind,
    pub native: MachineKind,
}

/// The dynamic-code mitigation bits the service admission policy consumes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessDynamicCodeMitigation {
    pub prohibit_dynamic_code: bool,
    pub allow_thread_opt_out: bool,
}

/// The binary-signature mitigation bits the service admission policy consumes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessBinarySignatureMitigation {
    pub microsoft_signed_only: bool,
    pub store_signed_only: bool,
    pub mitigation_opt_in: bool,
}

/// The lifecycle facts `NtQueryInformationProcess` exposes beyond the
/// basic block: whether Process Lifecycle Management has frozen the
/// process and whether the kernel has started deleting it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessLifecycleFlags {
    pub frozen: bool,
    pub deleting: bool,
}

fn decode_lifecycle_flags(flags: u32) -> ProcessLifecycleFlags {
    ProcessLifecycleFlags {
        deleting: flags & 0x4 != 0,
        frozen: flags & 0x10 != 0,
    }
}

/// An image machine, reduced to the cases the service can act on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachineKind {
    I386,
    Amd64,
    /// `IMAGE_FILE_MACHINE_UNKNOWN` in the process slot means "same as native".
    Unknown,
    Other(u16),
}

impl MachineKind {
    pub(crate) fn from_raw(value: u16) -> Self {
        match value {
            IMAGE_FILE_MACHINE_I386 => Self::I386,
            IMAGE_FILE_MACHINE_AMD64 => Self::Amd64,
            IMAGE_FILE_MACHINE_UNKNOWN => Self::Unknown,
            other => Self::Other(other),
        }
    }
}

/// An open process handle with a fixed rights set.
#[derive(Debug)]
pub struct Process {
    handle: OwnedHandle,
    access: ProcessAccess,
}

impl Process {
    /// Opens `pid` with exactly `access`. The error carries the Win32 code; a
    /// PID that has left the process table reports `ERROR_INVALID_PARAMETER`.
    pub fn open(pid: u32, access: ProcessAccess) -> io::Result<Self> {
        if access == ProcessAccess::AsSpawned {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "full access is only available on a process this one spawned",
            ));
        }
        // SAFETY: OpenProcess takes plain integers and returns a new handle or
        // null; no memory is shared with the callee.
        let handle = unsafe { OpenProcess(access.rights(), i32::from(access.inheritable()), pid) };
        Ok(Self {
            handle: OwnedHandle::from_creation(handle)?,
            access,
        })
    }

    /// Duplicates the handle `std` holds for `child`, so the result keeps
    /// every right `CreateProcess` granted (job assignment and termination
    /// among them, which a reopen by PID would not get) and addresses the
    /// same process object even after its PID is reused.
    pub fn from_child(child: &std::process::Child) -> io::Result<Self> {
        use std::os::windows::io::AsRawHandle;
        let mut duplicate = std::ptr::null_mut();
        // SAFETY: the source handle is the live one `child` owns for the
        // duration of this call, both process arguments are the current
        // process pseudo-handle, and `duplicate` is a local out value.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                child.as_raw_handle(),
                GetCurrentProcess(),
                &mut duplicate,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            handle: OwnedHandle::from_creation(duplicate)?,
            access: ProcessAccess::AsSpawned,
        })
    }

    /// Starts `executable` with `parameters` through the shell's `runas`
    /// verb, which shows the elevation prompt, and returns the process the
    /// shell created. The handle is the one `ShellExecuteExW` hands back, so
    /// it carries the creator's rights. A user who declines the prompt makes
    /// this fail with `ERROR_CANCELLED` as the Win32 code.
    pub fn launch_elevated(executable: &Path, parameters: &OsStr) -> io::Result<Self> {
        let verb = wide_null("runas");
        let executable = wide_path(executable);
        let parameters = wide_null(parameters);
        let mut info = SHELLEXECUTEINFOW {
            cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: verb.as_ptr(),
            lpFile: executable.as_ptr(),
            lpParameters: parameters.as_ptr(),
            nShow: 0,
            ..Default::default()
        };
        // SAFETY: `info` is a fully initialised structure whose string
        // pointers address NUL-terminated buffers that outlive the call; the
        // shell writes only `hProcess` and `hInstApp` back into it.
        if unsafe { ShellExecuteExW(&mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            handle: OwnedHandle::from_creation(info.hProcess)?,
            access: ProcessAccess::AsSpawned,
        })
    }

    pub(crate) fn from_owned(handle: OwnedHandle, access: ProcessAccess) -> Self {
        Self { handle, access }
    }

    /// The process identifier behind the handle.
    pub fn pid(&self) -> io::Result<u32> {
        // SAFETY: the handle is live; the call takes no pointers.
        let pid = unsafe { GetProcessId(self.handle.as_raw()) };
        if pid == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(pid)
    }

    pub fn access(&self) -> ProcessAccess {
        self.access
    }

    /// The owned handle, for handle-inheritance lists and waits.
    pub fn handle(&self) -> &OwnedHandle {
        &self.handle
    }

    /// Creation time as a 64-bit `FILETIME`, the value the service uses with
    /// the PID to identify a process across its whole lifetime.
    pub fn creation_time(&self) -> io::Result<u64> {
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        // SAFETY: the handle is live and every out pointer refers to a local
        // `FILETIME` that outlives the call.
        if unsafe {
            GetProcessTimes(
                self.handle.as_raw(),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
    }

    /// The process and native machine reported by `IsWow64Process2`.
    pub fn machine(&self) -> io::Result<ProcessMachine> {
        let mut process = IMAGE_FILE_MACHINE_UNKNOWN;
        let mut native = IMAGE_FILE_MACHINE_UNKNOWN;
        // SAFETY: the handle is live; both out pointers are local `u16`s.
        if unsafe { IsWow64Process2(self.handle.as_raw(), &mut process, &mut native) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(ProcessMachine {
            process: MachineKind::from_raw(process),
            native: MachineKind::from_raw(native),
        })
    }

    /// Returns whether the process is frozen or is being deleted.
    ///
    /// This query works with `PROCESS_QUERY_LIMITED_INFORMATION`.
    pub fn lifecycle_flags(&self) -> io::Result<ProcessLifecycleFlags> {
        let mut information = PROCESS_EXTENDED_BASIC_INFORMATION {
            Size: size_of::<PROCESS_EXTENDED_BASIC_INFORMATION>(),
            ..Default::default()
        };
        let mut returned = 0_u32;
        // SAFETY: the handle is live; `information` is zeroed with its required
        // Size set, and both writable pointers describe local values that
        // outlive the native query.
        let status = unsafe {
            NtQueryInformationProcess(
                self.handle.as_raw(),
                ProcessBasicInformation,
                (&mut information as *mut PROCESS_EXTENDED_BASIC_INFORMATION).cast(),
                size_of::<PROCESS_EXTENDED_BASIC_INFORMATION>() as u32,
                &mut returned,
            )
        };
        if status < 0 {
            // SAFETY: the conversion accepts the status value returned by the
            // immediately preceding native query and has no pointer arguments.
            let mut code = unsafe { RtlNtStatusToDosError(status) };
            if code == 0 || code == ERROR_MR_MID_NOT_FOUND || code == u32::MAX {
                code = if status == STATUS_ACCESS_DENIED {
                    ERROR_ACCESS_DENIED
                } else {
                    ERROR_INVALID_DATA
                };
            }
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        if returned < size_of::<PROCESS_EXTENDED_BASIC_INFORMATION>() as u32 {
            return Err(io::Error::from_raw_os_error(ERROR_INVALID_DATA as i32));
        }
        // SAFETY: the successful native query initialized the union storage,
        // and the binding exposes the lifecycle bitfield as `Flags`.
        Ok(decode_lifecycle_flags(unsafe {
            information.Anonymous.Flags
        }))
    }

    /// `Some(code)` once the process has exited, `None` while it still runs.
    pub fn exit_code(&self) -> io::Result<Option<u32>> {
        let mut exit_code = 0_u32;
        // SAFETY: the handle is live; the out pointer is a local `u32`.
        if unsafe { GetExitCodeProcess(self.handle.as_raw(), &mut exit_code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((exit_code != STILL_ACTIVE as u32).then_some(exit_code))
    }

    /// Whether the process runs at any protection level.
    pub fn protection(&self) -> io::Result<bool> {
        let mut information = PROCESS_PROTECTION_LEVEL_INFORMATION::default();
        // SAFETY: the handle is live; the buffer pointer and length describe
        // exactly the local `PROCESS_PROTECTION_LEVEL_INFORMATION`.
        if unsafe {
            GetProcessInformation(
                self.handle.as_raw(),
                ProcessProtectionLevelInfo,
                (&mut information as *mut PROCESS_PROTECTION_LEVEL_INFORMATION).cast(),
                size_of::<PROCESS_PROTECTION_LEVEL_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(information.ProtectionLevel != PROTECTION_LEVEL_NONE)
    }

    /// Whether the process runs at any protection level, or whether that
    /// cannot be determined. Both answers keep the service away from it.
    pub fn is_protected_or_unknown(&self) -> bool {
        let mut information = PROCESS_PROTECTION_LEVEL_INFORMATION::default();
        // SAFETY: the handle is live; the buffer pointer and length describe
        // exactly the local `PROCESS_PROTECTION_LEVEL_INFORMATION`.
        if unsafe {
            GetProcessInformation(
                self.handle.as_raw(),
                ProcessProtectionLevelInfo,
                (&mut information as *mut PROCESS_PROTECTION_LEVEL_INFORMATION).cast(),
                size_of::<PROCESS_PROTECTION_LEVEL_INFORMATION>() as u32,
            )
        } == 0
        {
            return true;
        }
        information.ProtectionLevel != PROTECTION_LEVEL_NONE
    }

    /// Whether the process is marked critical.
    pub fn criticality(&self) -> io::Result<bool> {
        let mut critical = 0;
        // SAFETY: the handle is live; the out pointer is a local `BOOL`.
        if unsafe { IsProcessCritical(self.handle.as_raw(), &mut critical) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(critical != 0)
    }

    /// Whether the process is marked critical, or whether that cannot be
    /// determined. Both answers keep the service away from it.
    pub fn is_critical_or_unknown(&self) -> bool {
        let mut critical = 0;
        // SAFETY: the handle is live; the out pointer is a local `BOOL`.
        let queried = unsafe { IsProcessCritical(self.handle.as_raw(), &mut critical) };
        queried == 0 || critical != 0
    }

    /// The dynamic-code mitigation bits, preserving a failed policy query.
    pub fn dynamic_code_mitigation(&self) -> io::Result<ProcessDynamicCodeMitigation> {
        let mut policy = PROCESS_MITIGATION_DYNAMIC_CODE_POLICY::default();
        // SAFETY: the handle is live; the buffer pointer and length describe
        // exactly the local mitigation policy structure.
        if unsafe {
            GetProcessMitigationPolicy(
                self.handle.as_raw(),
                ProcessDynamicCodePolicy,
                (&mut policy as *mut PROCESS_MITIGATION_DYNAMIC_CODE_POLICY).cast(),
                size_of::<PROCESS_MITIGATION_DYNAMIC_CODE_POLICY>(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the binding exposes the policy bitfield through this union
        // member, which was initialized by the successful query above.
        let flags = unsafe { policy.Anonymous.Flags };
        Ok(ProcessDynamicCodeMitigation {
            prohibit_dynamic_code: flags & (1 << 0) != 0,
            allow_thread_opt_out: flags & (1 << 1) != 0,
        })
    }

    /// The binary-signature mitigation bits, preserving a failed policy query.
    pub fn binary_signature_mitigation(&self) -> io::Result<ProcessBinarySignatureMitigation> {
        let mut policy = PROCESS_MITIGATION_BINARY_SIGNATURE_POLICY::default();
        // SAFETY: the handle is live; the buffer pointer and length describe
        // exactly the local mitigation policy structure.
        if unsafe {
            GetProcessMitigationPolicy(
                self.handle.as_raw(),
                ProcessSignaturePolicy,
                (&mut policy as *mut PROCESS_MITIGATION_BINARY_SIGNATURE_POLICY).cast(),
                size_of::<PROCESS_MITIGATION_BINARY_SIGNATURE_POLICY>(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the binding exposes the policy bitfield through this union
        // member, which was initialized by the successful query above.
        let flags = unsafe { policy.Anonymous.Flags };
        Ok(ProcessBinarySignatureMitigation {
            microsoft_signed_only: flags & (1 << 0) != 0,
            store_signed_only: flags & (1 << 1) != 0,
            mitigation_opt_in: flags & (1 << 2) != 0,
        })
    }

    /// The Win32 image path, or `None` when it cannot be read.
    pub fn image_path(&self) -> Option<PathBuf> {
        let mut buffer = vec![0_u16; MAX_IMAGE_PATH_UNITS];
        let mut length = buffer.len() as u32;
        // SAFETY: the handle is live; `buffer` is writable for `length` units
        // and the API writes at most that many, updating `length` to the
        // number it filled.
        if unsafe {
            QueryFullProcessImageNameW(
                self.handle.as_raw(),
                PROCESS_NAME_WIN32,
                buffer.as_mut_ptr(),
                &mut length,
            )
        } == 0
        {
            return None;
        }
        let length = (length as usize).min(buffer.len());
        Some(PathBuf::from(std::ffi::OsString::from_wide(
            &buffer[..length],
        )))
    }

    /// The Win32 image path, preserving query and bounds failures.
    pub fn image_path_checked(&self) -> io::Result<PathBuf> {
        let mut buffer = vec![0_u16; MAX_IMAGE_PATH_UNITS];
        let mut length = buffer.len() as u32;
        // SAFETY: the handle is live; `buffer` is writable for `length` units,
        // and the API updates `length` with the number of units written.
        if unsafe {
            QueryFullProcessImageNameW(
                self.handle.as_raw(),
                PROCESS_NAME_WIN32,
                buffer.as_mut_ptr(),
                &mut length,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if length == 0 || length as usize >= buffer.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "process image path length is out of range",
            ));
        }
        buffer.truncate(length as usize);
        Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer)))
    }

    /// Terminates the process with `exit_code`.
    pub fn terminate(&self, exit_code: u32) -> io::Result<()> {
        // SAFETY: the handle is live; the call takes no pointers.
        if unsafe { TerminateProcess(self.handle.as_raw(), exit_code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Waits for the process to exit.
    pub fn wait(&self, timeout: Option<Duration>) -> io::Result<WaitOutcome> {
        self.handle.wait(timeout)
    }
}

/// The session a PID belongs to.
pub fn process_session_id(pid: u32) -> io::Result<u32> {
    let mut session_id = 0;
    // SAFETY: plain integer in, local `u32` out.
    if unsafe { ProcessIdToSessionId(pid, &mut session_id) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(session_id)
}

/// Ends the current process immediately with `exit_code`, without running
/// destructors. Used only by the CI crash adapter to simulate a service crash.
pub fn terminate_current_process(exit_code: u32) -> io::Error {
    // SAFETY: the pseudo-handle from GetCurrentProcess is always valid and is
    // never closed; on success the call does not return.
    unsafe { TerminateProcess(GetCurrentProcess(), exit_code) };
    io::Error::last_os_error()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER;

    use super::{
        decode_lifecycle_flags, process_session_id, MachineKind, Process, ProcessAccess,
        ProcessLifecycleFlags,
    };
    use crate::handle::WaitOutcome;
    use crate::job::JobObject;

    #[test]
    fn a_spawned_child_keeps_the_rights_a_job_assignment_needs() {
        let mut child = std::process::Command::new("cmd")
            .args(["/c", "ping -n 6 127.0.0.1 > nul"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let process = Process::from_child(&child).unwrap();
        assert_eq!(process.access(), ProcessAccess::AsSpawned);
        assert_eq!(process.pid().unwrap(), child.id());
        assert_eq!(process.exit_code().unwrap(), None);

        let job = JobObject::kill_on_close().unwrap();
        job.assign(&process).unwrap();
        drop(job);

        assert_eq!(
            process.wait(Some(Duration::from_secs(2))).unwrap(),
            WaitOutcome::Signaled
        );
        child.wait().unwrap();
        assert!(Process::open(std::process::id(), ProcessAccess::AsSpawned).is_err());
    }

    #[test]
    fn lifecycle_flag_decoder_uses_the_documented_bits_only() {
        for (raw, expected) in [
            (
                0,
                ProcessLifecycleFlags {
                    frozen: false,
                    deleting: false,
                },
            ),
            (
                0x4,
                ProcessLifecycleFlags {
                    frozen: false,
                    deleting: true,
                },
            ),
            (
                0x10,
                ProcessLifecycleFlags {
                    frozen: true,
                    deleting: false,
                },
            ),
            (
                0x14,
                ProcessLifecycleFlags {
                    frozen: true,
                    deleting: true,
                },
            ),
            (
                0x58,
                ProcessLifecycleFlags {
                    frozen: true,
                    deleting: false,
                },
            ),
        ] {
            assert_eq!(decode_lifecycle_flags(raw), expected);
        }
    }

    #[test]
    fn the_current_process_answers_every_identity_query() {
        let process = Process::open(std::process::id(), ProcessAccess::QueryLimited).unwrap();
        assert!(process.creation_time().unwrap() > 0);
        assert_eq!(
            process.lifecycle_flags().unwrap(),
            ProcessLifecycleFlags {
                frozen: false,
                deleting: false,
            }
        );
        assert_eq!(process.exit_code().unwrap(), None);
        assert!(!process.is_critical_or_unknown());
        assert!(!process.is_protected_or_unknown());
        let machine = process.machine().unwrap();
        assert!(matches!(
            machine.native,
            MachineKind::Amd64 | MachineKind::I386 | MachineKind::Other(_)
        ));
        let image = process.image_path().unwrap();
        assert!(image.file_name().is_some());
        assert_eq!(process.image_path_checked().unwrap(), image);
        assert_eq!(
            process_session_id(std::process::id()).unwrap(),
            process_session_id(std::process::id()).unwrap()
        );
    }

    #[test]
    fn an_exited_child_reports_its_exit_code_and_a_dead_pid_is_invalid() {
        let mut child = std::process::Command::new("cmd")
            .args(["/d", "/c", "exit 7"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let process = Process::open(child.id(), ProcessAccess::Synchronize).unwrap();
        child.wait().unwrap();
        assert_eq!(
            process.wait(Some(Duration::from_secs(5))).unwrap(),
            WaitOutcome::Signaled
        );
        if let Ok(queryable) = Process::open(child.id(), ProcessAccess::QueryLimited) {
            assert_eq!(queryable.exit_code().unwrap(), Some(7));
        }
        drop(process);

        let error = Process::open(u32::MAX - 1, ProcessAccess::QueryLimited).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(ERROR_INVALID_PARAMETER as i32));
    }
}
