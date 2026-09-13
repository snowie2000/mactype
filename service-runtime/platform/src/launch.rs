//! Creating a suspended child with an explicit handle-inheritance list, plus
//! the anonymous pipe and NUL device handles it is given as standard streams.

use std::ffi::{OsStr, OsString};
use std::io;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    SetHandleInformation, ERROR_BROKEN_PIPE, HANDLE, HANDLE_FLAG_INHERIT,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList, ResumeThread,
    UpdateProcThreadAttribute, CREATE_NO_WINDOW, CREATE_SUSPENDED, EXTENDED_STARTUPINFO_PRESENT,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
    STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

use crate::handle::OwnedHandle;
use crate::process::{Process, ProcessAccess};
use crate::wide::wide_null;

/// The three standard handles a child is started with. Every handle must also
/// appear in the launch's inheritance list.
pub struct StandardHandles<'a> {
    pub input: &'a OwnedHandle,
    pub output: &'a OwnedHandle,
    pub error: &'a OwnedHandle,
}

/// Everything a child launch needs, gathered before `CreateProcessW` runs.
pub struct ProcessLaunch<'a> {
    pub executable: &'a Path,
    pub arguments: &'a [OsString],
    /// The only handles the child inherits; every other inheritable handle in
    /// this process stays private.
    pub inherit: &'a [&'a OwnedHandle],
    pub standard: StandardHandles<'a>,
}

/// A child created suspended. Nothing runs until [`SuspendedChild::resume`].
#[derive(Debug)]
pub struct SuspendedChild {
    process: Process,
    thread: OwnedHandle,
    pid: u32,
}

impl SuspendedChild {
    /// Creates the child with no window, suspended, inheriting exactly the
    /// listed handles.
    pub fn create(launch: &ProcessLaunch<'_>) -> io::Result<Self> {
        let inherited: Vec<HANDLE> = launch
            .inherit
            .iter()
            .map(|handle| handle.as_raw())
            .collect();
        let mut attributes = AttributeList::with_handles(&inherited)?;
        let mut command_line = command_line(launch.executable, launch.arguments);
        let application = wide_null(launch.executable.as_os_str());

        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = launch.standard.input.as_raw();
        startup.StartupInfo.hStdOutput = launch.standard.output.as_raw();
        startup.StartupInfo.hStdError = launch.standard.error.as_raw();
        startup.lpAttributeList = attributes.as_mut_ptr();

        let mut information = PROCESS_INFORMATION::default();
        // SAFETY: `application` and `command_line` are NUL-terminated buffers
        // that outlive the call (the command line is writable, as the API
        // requires); `startup` carries a live attribute list; `information`
        // is a local out structure; every handle referenced is live.
        if unsafe {
            CreateProcessW(
                application.as_ptr(),
                command_line.as_mut_ptr(),
                null(),
                null(),
                1,
                CREATE_NO_WINDOW | CREATE_SUSPENDED | EXTENDED_STARTUPINFO_PRESENT,
                null(),
                null(),
                &startup.StartupInfo,
                &mut information,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // Both handles are owned from here even if one of them is unexpectedly
        // invalid, so a partial failure cannot leak the other.
        let process = OwnedHandle::from_creation(information.hProcess);
        let thread = OwnedHandle::from_creation(information.hThread);
        Ok(Self {
            process: Process::from_owned(process?, ProcessAccess::InjectionTarget),
            thread: thread?,
            pid: information.dwProcessId,
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn process(&self) -> &Process {
        &self.process
    }

    /// Starts the child's initial thread while keeping both handles with this
    /// value. A failed start leaves the still-suspended child owned by the
    /// caller, who can terminate it and confirm the exit through the process
    /// handle that [`SuspendedChild::abandon`] hands over.
    pub fn start(&self) -> io::Result<()> {
        // SAFETY: the thread handle is live and owned; the call takes no
        // pointers.
        if unsafe { ResumeThread(self.thread.as_raw()) } == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Starts the child's initial thread. The thread handle is released here;
    /// the process handle stays with the returned value.
    pub fn resume(self) -> io::Result<Process> {
        self.start()?;
        Ok(self.into_process())
    }

    /// Gives up the child without resuming it. The caller keeps the process
    /// handle to terminate and confirm it.
    pub fn abandon(self) -> Process {
        self.into_process()
    }

    /// Releases the thread handle and hands over the process handle, whether
    /// or not the child was started.
    pub fn into_process(self) -> Process {
        self.process
    }
}

/// A `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` attribute list. The storage is
/// allocated to the size the OS reports and deleted when dropped.
struct AttributeList {
    storage: Vec<usize>,
    initialized: bool,
}

impl AttributeList {
    fn with_handles(handles: &[HANDLE]) -> io::Result<Self> {
        let mut bytes = 0_usize;
        // SAFETY: a null list with a size probe is the documented way to ask
        // for the required byte count; the out pointer is a local.
        unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut bytes) };
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut list = Self {
            storage: vec![0_usize; bytes.div_ceil(size_of::<usize>())],
            initialized: false,
        };
        // SAFETY: `storage` is at least `bytes` long and word-aligned, and the
        // list is deleted in `Drop` only after this initialization succeeds.
        if unsafe { InitializeProcThreadAttributeList(list.as_mut_ptr(), 1, 0, &mut bytes) } == 0 {
            return Err(io::Error::last_os_error());
        }
        list.initialized = true;
        // SAFETY: the list is initialized; `handles` is a live slice whose
        // byte length is passed alongside its pointer.
        if unsafe {
            UpdateProcThreadAttribute(
                list.as_mut_ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                std::mem::size_of_val(handles),
                null_mut(),
                null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(list)
    }

    fn as_mut_ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_mut_ptr().cast()
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: the list was initialized in `with_handles` and has not
            // been deleted since.
            unsafe { DeleteProcThreadAttributeList(self.as_mut_ptr()) };
        }
    }
}

/// An anonymous pipe whose write end is inheritable and whose read end stays
/// private to this process.
pub fn anonymous_pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let security = inheritable_attributes();
    let mut read = null_mut();
    let mut write = null_mut();
    // SAFETY: both out pointers are locals and the attribute block outlives
    // the call.
    if unsafe { CreatePipe(&mut read, &mut write, &security, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let read = OwnedHandle::from_creation(read);
    let write = OwnedHandle::from_creation(write);
    let (read, write) = (read?, write?);
    // SAFETY: the read handle is live; the flag mask and value are integers.
    if unsafe { SetHandleInformation(read.as_raw(), HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((read, write))
}

/// An inheritable read/write handle to the NUL device.
pub fn null_device() -> io::Result<OwnedHandle> {
    let security = inheritable_attributes();
    let name = wide_null(OsStr::new("NUL"));
    // SAFETY: `name` is NUL-terminated and the attribute block outlives the
    // call; no template handle is passed.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &security,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };
    OwnedHandle::from_creation(handle)
}

/// Reads `handle` to end-of-stream, keeping at most `maximum + 1` bytes so a
/// caller can tell "exactly the bound" from "over the bound".
pub fn read_bounded(handle: &OwnedHandle, maximum: usize) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 512];
    loop {
        let mut bytes_read = 0;
        // SAFETY: the handle is live; `buffer` is writable for its full length,
        // which is what the length argument states.
        if unsafe {
            ReadFile(
                handle.as_raw(),
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut bytes_read,
                null_mut(),
            )
        } == 0
        {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                break;
            }
            return Err(error);
        }
        if bytes_read == 0 {
            break;
        }
        let remaining = (maximum + 1).saturating_sub(output.len());
        output.extend_from_slice(&buffer[..(bytes_read as usize).min(remaining)]);
    }
    Ok(output)
}

fn inheritable_attributes() -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    }
}

/// Quotes the executable and every argument the way `CommandLineToArgvW`
/// parses them, so an argument reaches the child byte for byte.
pub(crate) fn command_line(executable: &Path, arguments: &[OsString]) -> Vec<u16> {
    let mut result = Vec::new();
    append_quoted(&mut result, executable.as_os_str());
    for argument in arguments {
        result.push(u16::from(b' '));
        append_quoted(&mut result, argument);
    }
    result.push(0);
    result
}

fn append_quoted(output: &mut Vec<u16>, argument: &OsStr) {
    let quote = u16::from(b'"');
    let backslash = u16::from(b'\\');
    output.push(quote);
    let mut slashes = 0_usize;
    for value in argument.encode_wide() {
        if value == backslash {
            slashes += 1;
        } else if value == quote {
            output.extend(std::iter::repeat(backslash).take(slashes * 2 + 1));
            output.push(value);
            slashes = 0;
        } else {
            output.extend(std::iter::repeat(backslash).take(slashes));
            output.push(value);
            slashes = 0;
        }
    }
    output.extend(std::iter::repeat(backslash).take(slashes * 2));
    output.push(quote);
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;

    use super::command_line;

    fn rendered(executable: &str, arguments: &[&str]) -> String {
        let arguments: Vec<OsString> = arguments.iter().map(OsString::from).collect();
        let units = command_line(Path::new(executable), &arguments);
        String::from_utf16(&units[..units.len() - 1]).unwrap()
    }

    #[test]
    fn arguments_are_quoted_and_escaped_for_the_child_parser() {
        assert_eq!(rendered("a.exe", &[]), "\"a.exe\"");
        assert_eq!(rendered("a.exe", &["plain"]), "\"a.exe\" \"plain\"");
        assert_eq!(
            rendered("a.exe", &["with \"quote\""]),
            "\"a.exe\" \"with \\\"quote\\\"\""
        );
        assert_eq!(
            rendered("a.exe", &["trailing\\"]),
            "\"a.exe\" \"trailing\\\\\""
        );
    }
}
