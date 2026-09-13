//! Named-pipe servers and clients used by the service protocols.

use std::fmt;
use std::fs::OpenOptions;
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_IO_PENDING, ERROR_NOT_FOUND, ERROR_NO_DATA, ERROR_PIPE_BUSY,
    ERROR_PIPE_CONNECTED, ERROR_PIPE_LISTENING, HANDLE, STATUS_PENDING, TRUE, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_FIRST_PIPE_INSTANCE,
    FILE_FLAG_OVERLAPPED, FILE_GENERIC_READ, FILE_SHARE_READ, OPEN_EXISTING, PIPE_ACCESS_INBOUND,
    PIPE_ACCESS_OUTBOUND, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
    GetNamedPipeServerProcessId, PIPE_NOWAIT, PIPE_READMODE_BYTE, PIPE_READMODE_MESSAGE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_TYPE_MESSAGE,
};
use windows_sys::Win32::System::IO::{
    CancelIoEx, GetOverlappedResult, GetOverlappedResultEx, OVERLAPPED,
};

use crate::event::ManualResetEvent;
use crate::handle::{OwnedHandle, WaitOutcome};
use crate::process::Process;
use crate::security::SecurityDescriptor;
use crate::wide::wide_null;

static CANCELLED_PIPE_OPERATIONS: AtomicU64 = AtomicU64::new(0);

/// What a non-blocking connect attempt observed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectOutcome {
    /// A client is attached and can be written to.
    Connected,
    /// No client yet; try again later.
    Listening,
}

/// A single-instance, outbound, message-mode pipe server that never blocks
/// and never accepts remote clients.
#[derive(Debug)]
pub struct NamedPipeServer(OwnedHandle);

// SAFETY: the pipe handle is a kernel object; every call on it is serialized
// by the kernel, so the server may be driven from a worker thread.
unsafe impl Send for NamedPipeServer {}

impl NamedPipeServer {
    /// Creates `name` with the given descriptor and an outbound buffer of
    /// `buffer_bytes`.
    pub fn create(
        name: &str,
        descriptor: &SecurityDescriptor,
        buffer_bytes: u32,
    ) -> io::Result<Self> {
        let name = wide_null(name);
        let attributes = descriptor.attributes(false);
        // SAFETY: `name` is NUL-terminated and the attribute block, with the
        // descriptor it points at, outlives the call.
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_OUTBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_MESSAGE
                    | PIPE_READMODE_MESSAGE
                    | PIPE_NOWAIT
                    | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                buffer_bytes,
                0,
                250,
                attributes.as_ptr(),
            )
        };
        Ok(Self(OwnedHandle::from_creation(handle)?))
    }

    /// Polls for a client. Errors other than "still listening" are returned;
    /// the caller decides whether to disconnect and continue.
    pub fn connect(&self) -> io::Result<ConnectOutcome> {
        // SAFETY: the handle is live; no overlapped structure is passed.
        if unsafe { ConnectNamedPipe(self.0.as_raw(), null_mut()) } != 0 {
            return Ok(ConnectOutcome::Connected);
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error().map(|code| code as u32) {
            Some(ERROR_PIPE_CONNECTED) => Ok(ConnectOutcome::Connected),
            Some(ERROR_PIPE_LISTENING | ERROR_NO_DATA) => Ok(ConnectOutcome::Listening),
            _ => Err(error),
        }
    }

    /// Writes one whole message. A partial write is reported as `WriteZero`.
    pub fn write_message(&self, bytes: &[u8]) -> io::Result<()> {
        let mut written = 0;
        // SAFETY: the handle is live; `bytes` is a live slice whose length is
        // passed alongside its pointer; `written` is a local out value.
        if unsafe {
            WriteFile(
                self.0.as_raw(),
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut written,
                null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if written as usize != bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "the pipe accepted only part of a message",
            ));
        }
        Ok(())
    }

    /// Drops the current client, if any, so the next connect can proceed.
    pub fn disconnect(&self) {
        // SAFETY: the handle is live; disconnecting with no client is a
        // harmless failure.
        unsafe { DisconnectNamedPipe(self.0.as_raw()) };
    }
}

impl Drop for NamedPipeServer {
    fn drop(&mut self) {
        self.disconnect();
    }
}

impl AsRawHandle for NamedPipeServer {
    fn as_raw_handle(&self) -> std::os::windows::io::RawHandle {
        self.0.as_raw()
    }
}

/// Returns the PID of the process that created this named-pipe instance.
pub fn named_pipe_server_process_id(
    pipe: &impl std::os::windows::io::AsRawHandle,
) -> io::Result<u32> {
    let mut pid = 0;
    // SAFETY: the caller lends a live named-pipe handle for this call; `pid` is
    // a local out value.
    if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &mut pid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if pid == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "named pipe server returned process ID zero",
        ));
    }
    Ok(pid)
}

/// A read-only client connection to a named pipe.
#[derive(Debug)]
pub struct NamedPipeClient(OwnedHandle);

impl AsRawHandle for NamedPipeClient {
    fn as_raw_handle(&self) -> std::os::windows::io::RawHandle {
        self.0.as_raw()
    }
}

impl NamedPipeClient {
    /// Opens `name` for reading. The error carries the Win32 code, so a
    /// caller polling for a server that is not up yet can keep waiting.
    pub fn open_read(name: &str) -> io::Result<Self> {
        let name = wide_null(name);
        // SAFETY: `name` is NUL-terminated; no security attributes or template
        // handle are passed.
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                FILE_GENERIC_READ,
                FILE_SHARE_READ,
                null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            )
        };
        Ok(Self(OwnedHandle::from_creation(handle)?))
    }

    /// The PID of the process serving the other end.
    pub fn server_process_id(&self) -> io::Result<u32> {
        named_pipe_server_process_id(self)
    }

    /// Reads one message into `buffer`, returning the byte count.
    pub fn read(&self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut read = 0;
        // SAFETY: the handle is live; `buffer` is writable for the length
        // passed alongside it; `read` is a local out value.
        if unsafe {
            ReadFile(
                self.0.as_raw(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut read,
                null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok((read as usize).min(buffer.len()))
    }
}

/// How long an overlapped operation may wait, and whose exit ends it early.
pub struct PipeWait<'a> {
    pub deadline: Instant,
    /// Each kernel wait is at most this long; the peer and the deadline are
    /// checked between slices.
    pub poll: Duration,
    /// A process whose exit aborts the wait with [`PipeError::PeerExited`].
    pub peer: Option<&'a Process>,
}

/// A failure from a deadline-bound overlapped pipe operation.
#[derive(Debug)]
pub enum PipeError {
    /// `peer` signaled before the operation completed.
    PeerExited,
    /// `deadline` passed before the operation completed.
    TimedOut,
    /// Checking the peer failed.
    PeerCheck(io::Error),
    /// The operation itself failed with this Win32 error.
    Io(io::Error),
    /// The operation made no progress where progress was required.
    NoProgress,
    /// Cancelling a pending operation failed after the operation was reaped.
    Cancel(io::Error),
}

impl fmt::Display for PipeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PeerExited => formatter.write_str("the peer process exited"),
            Self::TimedOut => formatter.write_str("the operation timed out"),
            Self::PeerCheck(error) => write!(formatter, "checking the peer failed: {error}"),
            Self::Io(error) => error.fmt(formatter),
            Self::NoProgress => formatter.write_str("no progress"),
            Self::Cancel(error) => {
                write!(
                    formatter,
                    "cancelling the pending operation failed: {error}"
                )
            }
        }
    }
}

impl std::error::Error for PipeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PeerCheck(error) | Self::Io(error) | Self::Cancel(error) => Some(error),
            Self::PeerExited | Self::TimedOut | Self::NoProgress => None,
        }
    }
}

/// Direction of data flow through an overlapped pipe server.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeDirection {
    Inbound,
    Outbound,
}

/// A single-instance byte pipe server whose operations have explicit deadlines.
pub struct OverlappedPipeServer(OwnedHandle);

impl OverlappedPipeServer {
    pub fn create_single_instance(
        name: &str,
        direction: PipeDirection,
        descriptor: &SecurityDescriptor,
        out_buffer_bytes: u32,
        in_buffer_bytes: u32,
        default_timeout: Duration,
    ) -> io::Result<Self> {
        let name = wide_null(name);
        let attributes = descriptor.attributes(false);
        let access = match direction {
            PipeDirection::Inbound => PIPE_ACCESS_INBOUND,
            PipeDirection::Outbound => PIPE_ACCESS_OUTBOUND,
        };
        // SAFETY: `name` is NUL-terminated and the borrowed security attributes
        // and descriptor remain live until CreateNamedPipeW returns.
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                access | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                out_buffer_bytes,
                in_buffer_bytes,
                u32::try_from(default_timeout.as_millis()).unwrap_or(u32::MAX),
                attributes.as_ptr(),
            )
        };
        Ok(Self(OwnedHandle::from_creation(handle)?))
    }

    pub fn connect(&self, wait: &PipeWait<'_>) -> Result<(), PipeError> {
        let event = ManualResetEvent::new().map_err(PipeError::Io)?;
        let mut overlapped = new_overlapped(&event);
        // SAFETY: the server handle was opened for overlapped I/O; `overlapped`
        // and its event stay alive through synchronous completion or wait/reap.
        if unsafe { ConnectNamedPipe(self.0.as_raw(), &mut overlapped) } != 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        match win32_code(&error) {
            Some(ERROR_PIPE_CONNECTED) => Ok(()),
            Some(ERROR_IO_PENDING) => {
                wait_for_overlapped(self.0.as_raw(), &overlapped, wait).map(drop)
            }
            _ => Err(PipeError::Io(error)),
        }
    }

    pub fn client_process_id(&self) -> io::Result<u32> {
        let mut pid = 0;
        // SAFETY: the server owns a live named-pipe handle and `pid` is a local
        // output that remains valid for the call.
        if unsafe { GetNamedPipeClientProcessId(self.0.as_raw(), &mut pid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if pid == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "named pipe client returned process ID zero",
            ));
        }
        Ok(pid)
    }

    pub fn write_all(&self, bytes: &[u8], wait: &PipeWait<'_>) -> Result<(), PipeError> {
        write_all_overlapped(self.0.as_raw(), bytes, wait)
    }

    pub fn read(&self, buffer: &mut [u8], wait: &PipeWait<'_>) -> Result<usize, PipeError> {
        read_overlapped(self.0.as_raw(), buffer, wait)
    }
}

/// Access requested when opening an overlapped pipe client.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipeAccess {
    Read,
    Write,
}

/// A deadline-bound overlapped client connection.
pub struct OverlappedPipeClient(std::fs::File);

impl OverlappedPipeClient {
    pub fn open(
        name: &str,
        access: PipeAccess,
        deadline: Instant,
        poll: Duration,
    ) -> Result<Self, PipeError> {
        loop {
            let mut options = OpenOptions::new();
            match access {
                PipeAccess::Read => {
                    options.read(true);
                }
                PipeAccess::Write => {
                    options.write(true);
                }
            }
            options.custom_flags(
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
            );
            match options.open(name) {
                Ok(file) => return Ok(Self(file)),
                Err(error)
                    if matches!(
                        win32_code(&error),
                        Some(ERROR_FILE_NOT_FOUND | ERROR_PIPE_BUSY)
                    ) =>
                {
                    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                        return Err(PipeError::TimedOut);
                    };
                    std::thread::sleep(remaining.min(poll));
                }
                Err(error) => return Err(PipeError::Io(error)),
            }
        }
    }

    pub fn server_process_id(&self) -> io::Result<u32> {
        named_pipe_server_process_id(&self.0)
    }

    pub fn read(&self, buffer: &mut [u8], wait: &PipeWait<'_>) -> Result<usize, PipeError> {
        read_overlapped(self.0.as_raw_handle(), buffer, wait)
    }

    pub fn write_all(&self, bytes: &[u8], wait: &PipeWait<'_>) -> Result<(), PipeError> {
        write_all_overlapped(self.0.as_raw_handle(), bytes, wait)
    }
}

/// Number of pending pipe operations for which cancellation and reaping ran.
pub fn cancelled_pipe_operations() -> u64 {
    CANCELLED_PIPE_OPERATIONS.load(Ordering::Relaxed)
}

fn new_overlapped(event: &ManualResetEvent) -> OVERLAPPED {
    OVERLAPPED {
        hEvent: event.handle().as_raw(),
        ..Default::default()
    }
}

fn read_overlapped(
    handle: HANDLE,
    buffer: &mut [u8],
    wait: &PipeWait<'_>,
) -> Result<usize, PipeError> {
    let event = ManualResetEvent::new().map_err(PipeError::Io)?;
    let mut overlapped = new_overlapped(&event);
    let request = buffer.len().min(u32::MAX as usize) as u32;
    // SAFETY: `handle` is a live pipe opened for overlapped I/O; `buffer` is
    // writable for `request` bytes; the OVERLAPPED and its event stay alive
    // through synchronous completion or wait/reap. The byte-count pointer is
    // null as required for asynchronous handles.
    let completed = unsafe {
        ReadFile(
            handle,
            buffer.as_mut_ptr(),
            request,
            null_mut(),
            &mut overlapped,
        )
    };
    let read = if completed != 0 {
        completed_overlapped_result(handle, &overlapped)?
    } else {
        let error = io::Error::last_os_error();
        if win32_code(&error) != Some(ERROR_IO_PENDING) {
            return Err(PipeError::Io(error));
        }
        wait_for_overlapped(handle, &overlapped, wait)?
    };
    Ok((read as usize).min(request as usize))
}

fn write_all_overlapped(
    handle: HANDLE,
    mut bytes: &[u8],
    wait: &PipeWait<'_>,
) -> Result<(), PipeError> {
    while !bytes.is_empty() {
        let request = bytes.len().min(u32::MAX as usize);
        let event = ManualResetEvent::new().map_err(PipeError::Io)?;
        let mut overlapped = new_overlapped(&event);
        // SAFETY: `handle` is a live pipe opened for overlapped I/O; `bytes` is
        // readable for `request` bytes; the OVERLAPPED and its event stay alive
        // through synchronous completion or wait/reap. The byte-count pointer
        // is null as required for asynchronous handles.
        let completed = unsafe {
            WriteFile(
                handle,
                bytes.as_ptr(),
                request as u32,
                null_mut(),
                &mut overlapped,
            )
        };
        let written = if completed != 0 {
            completed_overlapped_result(handle, &overlapped)?
        } else {
            let error = io::Error::last_os_error();
            if win32_code(&error) != Some(ERROR_IO_PENDING) {
                return Err(PipeError::Io(error));
            }
            wait_for_overlapped(handle, &overlapped, wait)?
        };
        if written == 0 {
            return Err(PipeError::NoProgress);
        }
        let written = (written as usize).min(request);
        bytes = &bytes[written..];
    }
    Ok(())
}

fn completed_overlapped_result(handle: HANDLE, overlapped: &OVERLAPPED) -> Result<u32, PipeError> {
    let mut transferred = 0;
    // SAFETY: the operation reported synchronous completion; `handle` and its
    // unique OVERLAPPED remain live while the completed byte count is queried.
    if unsafe { GetOverlappedResult(handle, overlapped, &mut transferred, 0) } == 0 {
        return Err(PipeError::Io(io::Error::last_os_error()));
    }
    Ok(transferred)
}

fn wait_for_overlapped(
    handle: HANDLE,
    overlapped: &OVERLAPPED,
    wait: &PipeWait<'_>,
) -> Result<u32, PipeError> {
    loop {
        if let Some(peer) = wait.peer {
            match peer.wait(Some(Duration::ZERO)) {
                Ok(WaitOutcome::Signaled) => {
                    return cancel_and_reap(handle, overlapped, PipeError::PeerExited);
                }
                Ok(WaitOutcome::TimedOut) => {}
                Ok(WaitOutcome::Abandoned) => {
                    let error = io::Error::other("the peer process wait was abandoned");
                    return cancel_and_reap(handle, overlapped, PipeError::PeerCheck(error));
                }
                Err(error) => {
                    return cancel_and_reap(handle, overlapped, PipeError::PeerCheck(error));
                }
            }
        }
        let Some(remaining) = wait.deadline.checked_duration_since(Instant::now()) else {
            return cancel_and_reap(handle, overlapped, PipeError::TimedOut);
        };
        let slice_milliseconds = remaining
            .min(wait.poll)
            .as_millis()
            .clamp(1, u128::from(u32::MAX)) as u32;
        let mut transferred = 0;
        // SAFETY: `handle` is live; `overlapped` and its event remain live until
        // this operation completes or is cancelled and reaped; `transferred` is
        // a local output.
        if unsafe {
            GetOverlappedResultEx(handle, overlapped, &mut transferred, slice_milliseconds, 0)
        } != 0
        {
            return Ok(transferred);
        }
        let error = io::Error::last_os_error();
        if win32_code(&error) == Some(WAIT_TIMEOUT) {
            continue;
        }
        if overlapped_has_completed(overlapped) {
            // The operation finished with this error (a broken pipe, an
            // abort); there is nothing left to cancel or reap.
            return Err(PipeError::Io(error));
        }
        return cancel_and_reap(handle, overlapped, PipeError::Io(error));
    }
}

/// `HasOverlappedIoCompleted`: the kernel replaces the pending status once the
/// operation has finished, whatever its outcome.
fn overlapped_has_completed(overlapped: &OVERLAPPED) -> bool {
    overlapped.Internal != STATUS_PENDING as usize
}

fn cancel_and_reap(
    handle: HANDLE,
    overlapped: &OVERLAPPED,
    original: PipeError,
) -> Result<u32, PipeError> {
    CANCELLED_PIPE_OPERATIONS.fetch_add(1, Ordering::Relaxed);
    // SAFETY: `handle` is live and `overlapped` names the pending operation that
    // this function always reaps before either value can cease to be valid.
    let cancellation = unsafe { CancelIoEx(handle, overlapped) };
    let cancellation_error = if cancellation == 0 {
        let error = io::Error::last_os_error();
        (win32_code(&error) != Some(ERROR_NOT_FOUND)).then_some(error)
    } else {
        None
    };
    let mut transferred = 0;
    // SAFETY: the same live handle and OVERLAPPED are retained after cancellation;
    // waiting here completes the mandatory reap before their backing storage drops.
    unsafe { GetOverlappedResult(handle, overlapped, &mut transferred, TRUE) };
    match cancellation_error {
        Some(error) => Err(PipeError::Cancel(error)),
        None => Err(original),
    }
}

fn win32_code(error: &io::Error) -> Option<u32> {
    error.raw_os_error().map(|code| code as u32)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use super::{
        cancelled_pipe_operations, ConnectOutcome, NamedPipeClient, NamedPipeServer,
        OverlappedPipeClient, OverlappedPipeServer, PipeAccess, PipeDirection, PipeError, PipeWait,
    };
    use crate::process::Process;
    use crate::security::SecurityDescriptor;

    static NEXT_NONCE: AtomicU64 = AtomicU64::new(0);
    static OVERLAPPED_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn pipe_name() -> String {
        format!(
            r"\\.\pipe\mactype-platform-tests-{}-{}",
            std::process::id(),
            NEXT_NONCE.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn descriptor() -> SecurityDescriptor {
        SecurityDescriptor::from_sddl("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;OW)").unwrap()
    }

    fn wait(seconds: u64) -> PipeWait<'static> {
        PipeWait {
            deadline: Instant::now() + Duration::from_secs(seconds),
            poll: Duration::from_millis(5),
            peer: None,
        }
    }

    #[test]
    fn a_client_reads_the_message_the_server_wrote_and_sees_its_pid() {
        let name = format!(r"\\.\pipe\mactype-platform-{}", std::process::id());
        let descriptor =
            SecurityDescriptor::from_sddl("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GR;;;AU)").unwrap();
        let server = NamedPipeServer::create(&name, &descriptor, 4096).unwrap();
        assert_eq!(server.connect().unwrap(), ConnectOutcome::Listening);

        let client = NamedPipeClient::open_read(&name).unwrap();
        assert_eq!(client.server_process_id().unwrap(), std::process::id());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if server.connect().unwrap() == ConnectOutcome::Connected {
                break;
            }
            assert!(Instant::now() < deadline, "the client never connected");
            std::thread::sleep(Duration::from_millis(5));
        }
        server.write_message(b"hello\n").unwrap();
        let mut buffer = [0_u8; 64];
        let read = client.read(&mut buffer).unwrap();
        assert_eq!(&buffer[..read], b"hello\n");
        server.disconnect();
    }

    #[test]
    fn an_outbound_overlapped_server_transfers_a_large_byte_stream() {
        let _serial = OVERLAPPED_TEST_LOCK.lock().unwrap();
        let name = pipe_name();
        let server = OverlappedPipeServer::create_single_instance(
            &name,
            PipeDirection::Outbound,
            &descriptor(),
            64 * 1024,
            0,
            Duration::from_millis(250),
        )
        .unwrap();
        let expected = (0..200 * 1024)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let client_name = name.clone();
        let client = std::thread::spawn(move || {
            let client = OverlappedPipeClient::open(
                &client_name,
                PipeAccess::Read,
                Instant::now() + Duration::from_secs(2),
                Duration::from_millis(5),
            )
            .unwrap();
            let mut received = Vec::new();
            let mut chunk = vec![0_u8; 64 * 1024];
            while received.len() < 200 * 1024 {
                let read = client.read(&mut chunk, &wait(2)).unwrap();
                if read == 0 {
                    break;
                }
                received.extend_from_slice(&chunk[..read]);
            }
            received
        });
        server.connect(&wait(2)).unwrap();
        assert_eq!(server.client_process_id().unwrap(), std::process::id());
        server.write_all(&expected, &wait(2)).unwrap();
        assert_eq!(client.join().unwrap(), expected);
    }

    #[test]
    fn an_inbound_overlapped_server_receives_a_large_byte_stream() {
        let _serial = OVERLAPPED_TEST_LOCK.lock().unwrap();
        let name = pipe_name();
        let server = OverlappedPipeServer::create_single_instance(
            &name,
            PipeDirection::Inbound,
            &descriptor(),
            0,
            64 * 1024,
            Duration::from_millis(250),
        )
        .unwrap();
        let expected = (0..200 * 1024)
            .map(|index| (index % 239) as u8)
            .collect::<Vec<_>>();
        let client_name = name.clone();
        let sent = expected.clone();
        let client = std::thread::spawn(move || {
            let client = OverlappedPipeClient::open(
                &client_name,
                PipeAccess::Write,
                Instant::now() + Duration::from_secs(2),
                Duration::from_millis(5),
            )
            .unwrap();
            client.write_all(&sent, &wait(2)).unwrap();
        });
        server.connect(&wait(2)).unwrap();
        assert_eq!(server.client_process_id().unwrap(), std::process::id());
        let mut received = Vec::new();
        let mut chunk = vec![0_u8; 64 * 1024];
        while received.len() < expected.len() {
            let read = server.read(&mut chunk, &wait(2)).unwrap();
            if read == 0 {
                break;
            }
            received.extend_from_slice(&chunk[..read]);
        }
        client.join().unwrap();
        assert_eq!(received, expected);
    }

    #[test]
    fn connect_timeout_cancels_and_reaps_once() {
        let _serial = OVERLAPPED_TEST_LOCK.lock().unwrap();
        let server = OverlappedPipeServer::create_single_instance(
            &pipe_name(),
            PipeDirection::Outbound,
            &descriptor(),
            4096,
            0,
            Duration::from_millis(250),
        )
        .unwrap();
        let before = cancelled_pipe_operations();
        let result = server.connect(&PipeWait {
            deadline: Instant::now() + Duration::from_millis(25),
            poll: Duration::from_millis(5),
            peer: None,
        });
        assert!(matches!(result, Err(PipeError::TimedOut)));
        assert_eq!(cancelled_pipe_operations(), before + 1);
    }

    #[test]
    fn exited_peer_cancels_and_reaps_connect_once() {
        let _serial = OVERLAPPED_TEST_LOCK.lock().unwrap();
        let mut child = std::process::Command::new("cmd")
            .args(["/d", "/c", "exit 0"])
            .spawn()
            .unwrap();
        child.wait().unwrap();
        let peer = Process::from_child(&child).unwrap();
        let server = OverlappedPipeServer::create_single_instance(
            &pipe_name(),
            PipeDirection::Outbound,
            &descriptor(),
            4096,
            0,
            Duration::from_millis(250),
        )
        .unwrap();
        let before = cancelled_pipe_operations();
        let result = server.connect(&PipeWait {
            deadline: Instant::now() + Duration::from_secs(2),
            poll: Duration::from_millis(5),
            peer: Some(&peer),
        });
        assert!(matches!(result, Err(PipeError::PeerExited)));
        assert_eq!(cancelled_pipe_operations(), before + 1);
    }
}
