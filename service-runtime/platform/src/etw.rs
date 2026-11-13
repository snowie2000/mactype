//! A private real-time ETW session over the kernel's process-start events.
//!
//! WMI delivers `Win32_ProcessStartTrace` through a shared real-time session
//! whose buffers flush about once a second, so a program has already painted
//! its first frames by the time the service hears about it. Subscribing to
//! `Microsoft-Windows-Kernel-Process` directly, with a millisecond flush timer,
//! removes that wait.
//!
//! Callers get process identifiers over a bounded channel and an owned type
//! whose destructor stops the session; no trace handle, event record, or
//! payload pointer leaves this module.

use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::ptr::null;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::thread::JoinHandle;
use std::time::Duration;

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, ERROR_SUCCESS};
use windows_sys::Win32::System::Diagnostics::Etw::{
    CONTROLTRACE_HANDLE, CloseTrace, ControlTraceW, EVENT_CONTROL_CODE_ENABLE_PROVIDER,
    EVENT_RECORD, EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_LOGFILEW, EVENT_TRACE_PROPERTIES,
    EVENT_TRACE_REAL_TIME_MODE, EnableTraceEx2, OpenTraceW, PROCESS_TRACE_MODE_EVENT_RECORD,
    PROCESS_TRACE_MODE_REAL_TIME, PROCESSTRACE_HANDLE, ProcessTrace, StartTraceW,
    TRACE_LEVEL_INFORMATION, WNODE_FLAG_TRACED_GUID,
};
use windows_sys::core::GUID;

use crate::wide::wide_null;

/// `Microsoft-Windows-Kernel-Process`, the provider that announces a process
/// the moment the kernel finishes creating it.
const KERNEL_PROCESS_PROVIDER: GUID = GUID::from_u128(0x22fb2cd6_0e7b_422b_a0c7_2fad1fd0e716);

/// `WINEVENT_KEYWORD_PROCESS`: process lifetime events only, so the session
/// carries neither thread nor image-load traffic.
const PROCESS_KEYWORD: u64 = 0x10;

/// `ProcessStart`. Every version of the manifest (0 through 3) starts its
/// payload with the new process's `ProcessID` as a `win:UInt32`; the field
/// after it is a `FILETIME` in versions 0 to 2 and a sequence number in
/// version 3, which is why the decoder reads the first field and stops.
/// Checked against this machine's registered manifest with
/// `(Get-WinEvent -ListProvider Microsoft-Windows-Kernel-Process).Events`.
const PROCESS_START_EVENT_ID: u16 = 1;

/// `EVENT_TRACE_USE_MS_FLUSH_TIMER`, which reads `FlushTimer` as milliseconds
/// rather than seconds. The flag is absent from `evntrace.h` in the Windows SDK
/// installed here (10.0.26100.0, checked with a full-tree search of
/// `C:\Program Files (x86)\Windows Kits\10\Include`) and from windows-sys
/// 0.61.2; it is defined in the Enterprise WDK and documented as bit
/// `0x00000010` of `LogFileMode`, valid on Windows 7 and later, at
/// <https://www.geoffchappell.com/studies/windows/km/ntoskrnl/api/etw/traceapi/wmi_logger_information/logfilemode.htm>.
/// That bit is the one value the SDK header's own `LogFileMode` list skips.
const EVENT_TRACE_USE_MS_FLUSH_TIMER: u32 = 0x0000_0010;

/// `INVALID_PROCESSTRACE_HANDLE` is `(PROCESSTRACE_HANDLE)INVALID_HANDLE_VALUE`
/// (`evntrace.h`), so the sentinel is all pointer-width bits set, widened to
/// the 64-bit handle type.
const INVALID_PROCESSTRACE_HANDLE: u64 = usize::MAX as u64;

/// Milliseconds between flushes of a non-empty buffer.
const FLUSH_TIMER_MILLISECONDS: u32 = 25;

/// Buffer size in kilobytes. Small buffers fill quickly, which is what a
/// latency-sensitive session wants.
const BUFFER_SIZE_KILOBYTES: u32 = 8;
const MINIMUM_BUFFERS: u32 = 4;
const MAXIMUM_BUFFERS: u32 = 32;

/// How many process identifiers may wait for the consumer. Beyond this the
/// callback drops ids and raises the overflow flag, which is the caller's
/// signal to resubscribe and fold a fresh snapshot into its backlog.
const PENDING_PROCESS_ID_CAPACITY: usize = 4_096;

/// Room for both names `ControlTraceW` writes back into a properties block.
const CONTROL_NAME_UNITS: usize = 1_024;

/// What the trace had to say when the consumer asked for the next process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessStartEvent {
    /// A process with this identifier has just started.
    Started(u32),
    /// The wait elapsed with nothing to report.
    Idle,
    /// The consumer thread has finished, so this session delivers no more
    /// events. Only a restart brings the subscription back.
    Ended,
}

/// A live real-time trace session that reports process identifiers as the
/// kernel creates them. Dropping it stops the session and joins the consumer.
pub struct ProcessStartTrace {
    session: ControlledSession,
    trace: PROCESSTRACE_HANDLE,
    consumer: Option<JoinHandle<()>>,
    sink: Arc<TraceSink>,
    identifiers: Receiver<u32>,
}

impl ProcessStartTrace {
    /// Starts a private real-time session called `session_name` and begins
    /// consuming it on a dedicated thread.
    ///
    /// Starting a real-time session needs an elevated or LocalSystem token;
    /// an ordinary user token fails here with `ERROR_ACCESS_DENIED`. A session
    /// of the same name left behind by a process that died without cleaning up
    /// is stopped and the start retried once.
    pub fn start(session_name: &str) -> io::Result<Self> {
        let name = wide_null(session_name);
        let handle = start_session(&name)?;
        let session = ControlledSession::new(name.clone());
        enable_kernel_process_provider(handle)?;

        let (sender, identifiers) = sync_channel(PENDING_PROCESS_ID_CAPACITY);
        let sink = Arc::new(TraceSink {
            identifiers: sender,
            overflowed: AtomicBool::new(false),
        });
        let mut consumer_state = Box::new(ConsumerState::new(name, Arc::as_ptr(&sink)));
        let trace = open_trace(&mut consumer_state)?;

        let thread_sink = Arc::clone(&sink);
        let consumer = std::thread::Builder::new()
            .name("mactype-process-start-trace".to_owned())
            .spawn(move || {
                // Both values are owned by this thread for as long as the
                // callback can run: `state` keeps the logfile block and its
                // session name allocated, `sink` keeps the channel and the
                // overflow flag alive behind the context pointer.
                let state = consumer_state;
                let sink = thread_sink;
                let handles = [trace];
                // SAFETY: `handles` is a live one-element array holding the
                // handle `OpenTraceW` just returned, and both time bounds are
                // null, which asks for every event until the session stops.
                unsafe {
                    ProcessTrace(handles.as_ptr(), 1, null(), null());
                }
                drop(sink);
                drop(state);
            })
            .inspect_err(|_| close_trace(trace))?;

        Ok(Self {
            session,
            trace,
            consumer: Some(consumer),
            sink,
            identifiers,
        })
    }

    /// Waits up to `timeout` for the next process identifier.
    pub fn next_process_id(&self, timeout: Duration) -> ProcessStartEvent {
        match self.identifiers.recv_timeout(timeout) {
            Ok(pid) => ProcessStartEvent::Started(pid),
            Err(RecvTimeoutError::Timeout) => ProcessStartEvent::Idle,
            Err(RecvTimeoutError::Disconnected) => ProcessStartEvent::Ended,
        }
    }

    /// Whether the callback has had to drop identifiers because the consumer
    /// fell behind. Once set it stays set: this session has a hole in it and
    /// only a fresh one, with a fresh snapshot, closes the hole.
    pub fn dropped_process_ids(&self) -> bool {
        self.sink.overflowed.load(Ordering::Acquire)
    }
}

impl Drop for ProcessStartTrace {
    fn drop(&mut self) {
        // Stopping the session makes `ProcessTrace` return; closing the trace
        // makes it return even if the stop did not take. Neither call may
        // panic, and the join cannot outlive both of them.
        self.session.stop();
        close_trace(self.trace);
        if let Some(consumer) = self.consumer.take() {
            let _ = consumer.join();
        }
    }
}

/// What the callback writes to and what the owner reads from. Only the ETW
/// consumer thread touches the channel; both threads touch the flag.
struct TraceSink {
    identifiers: SyncSender<u32>,
    overflowed: AtomicBool,
}

/// The logfile block handed to `OpenTraceW`, together with the session-name
/// buffer it points at. ETW may write progress back into the block while
/// `ProcessTrace` runs, so nothing reads it again after the open.
struct ConsumerState {
    name: Vec<u16>,
    logfile: EVENT_TRACE_LOGFILEW,
}

impl ConsumerState {
    fn new(name: Vec<u16>, context: *const TraceSink) -> Self {
        let mut logfile = EVENT_TRACE_LOGFILEW::default();
        logfile.Anonymous1.ProcessTraceMode =
            PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
        logfile.Anonymous2.EventRecordCallback = Some(on_event_record);
        logfile.Context = context as *mut c_void;
        Self { name, logfile }
    }
}

// SAFETY: the two pointers inside the logfile block are the only reason the
// compiler holds this back. `LoggerName` addresses the `name` buffer the same
// value owns, so it moves with it, and `Context` addresses the contents of an
// `Arc<TraceSink>` the same thread holds a strong reference to for as long as
// `ProcessTrace` can call back. Nothing but ETW ever reads the block again.
unsafe impl Send for ConsumerState {}

/// A started session, stopped exactly once however the owner unwinds.
struct ControlledSession {
    name: Vec<u16>,
    stopped: AtomicBool,
}

impl ControlledSession {
    fn new(name: Vec<u16>) -> Self {
        Self {
            name,
            stopped: AtomicBool::new(false),
        }
    }

    fn stop(&self) {
        if !self.stopped.swap(true, Ordering::AcqRel) {
            stop_session_by_name(&self.name);
        }
    }
}

impl Drop for ControlledSession {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Reads the new process's identifier out of a `ProcessStart` payload.
///
/// `ProcessID` is the first field in every version of the manifest this
/// provider emits, so only its first four little-endian bytes are read; the
/// fields after it moved between versions and are none of the service's
/// business. A payload too short to hold the field, or one naming process
/// zero, is rejected rather than guessed at.
pub(crate) fn decode_process_start_id(payload: &[u8]) -> Option<u32> {
    let field: [u8; 4] = payload.get(..4)?.try_into().ok()?;
    let pid = u32::from_le_bytes(field);
    (pid != 0).then_some(pid)
}

/// The ETW consumer callback. It runs on the `ProcessTrace` thread for every
/// event the session carries, so it allocates nothing, waits for nothing, and
/// has no operation in it that can panic.
unsafe extern "system" fn on_event_record(record: *mut EVENT_RECORD) {
    // SAFETY: ETW passes a record that stays valid for the call, or nothing.
    let Some(record) = (unsafe { record.as_ref() }) else {
        return;
    };
    if !guid_equals(&record.EventHeader.ProviderId, &KERNEL_PROCESS_PROVIDER)
        || record.EventHeader.EventDescriptor.Id != PROCESS_START_EVENT_ID
    {
        return;
    }
    // SAFETY: `UserContext` is the `Context` given to `OpenTraceW`, which is
    // the address inside an `Arc<TraceSink>` the consumer thread owns for as
    // long as `ProcessTrace` can call back.
    let Some(sink) = (unsafe { record.UserContext.cast::<TraceSink>().as_ref() }) else {
        return;
    };
    if record.UserData.is_null() {
        return;
    }
    // SAFETY: ETW documents `UserData` as readable for `UserDataLength` bytes
    // for the duration of the callback, and the slice is only read here.
    let payload = unsafe {
        std::slice::from_raw_parts(
            record.UserData.cast::<u8>(),
            usize::from(record.UserDataLength),
        )
    };
    let Some(pid) = decode_process_start_id(payload) else {
        return;
    };
    if sink.identifiers.try_send(pid).is_err() {
        sink.overflowed.store(true, Ordering::Release);
    }
}

fn guid_equals(left: &GUID, right: &GUID) -> bool {
    left.data1 == right.data1
        && left.data2 == right.data2
        && left.data3 == right.data3
        && left.data4 == right.data4
}

fn start_session(name: &[u16]) -> io::Result<CONTROLTRACE_HANDLE> {
    match try_start_session(name) {
        Err(error) if error.raw_os_error() == Some(ERROR_ALREADY_EXISTS as i32) => {
            // A previous instance died without stopping its session. The name
            // is ours, so taking it back is the only way forward.
            stop_session_by_name(name);
            try_start_session(name)
        }
        result => result,
    }
}

fn try_start_session(name: &[u16]) -> io::Result<CONTROLTRACE_HANDLE> {
    let mut properties = TraceProperties::for_start(name);
    let mut handle = CONTROLTRACE_HANDLE { Value: 0 };
    // SAFETY: `handle` is a local out value, `name` is NUL-terminated, and the
    // properties block is `Wnode.BufferSize` bytes long with its session name
    // copied in at `LoggerNameOffset`, which is what this call reads and
    // writes back.
    let status = unsafe { StartTraceW(&mut handle, name.as_ptr(), properties.as_mut_ptr()) };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    Ok(handle)
}

fn stop_session_by_name(name: &[u16]) {
    let mut properties = TraceProperties::for_control();
    // SAFETY: a zero handle with a name addresses the session by name; the
    // properties block is large enough for both names the call writes back.
    unsafe {
        ControlTraceW(
            CONTROLTRACE_HANDLE { Value: 0 },
            name.as_ptr(),
            properties.as_mut_ptr(),
            EVENT_TRACE_CONTROL_STOP,
        );
    }
}

fn enable_kernel_process_provider(session: CONTROLTRACE_HANDLE) -> io::Result<()> {
    // SAFETY: the provider GUID is a local constant and no enable parameters
    // are passed, so the call reads nothing that outlives it.
    let status = unsafe {
        EnableTraceEx2(
            session,
            &KERNEL_PROCESS_PROVIDER,
            EVENT_CONTROL_CODE_ENABLE_PROVIDER,
            TRACE_LEVEL_INFORMATION as u8,
            PROCESS_KEYWORD,
            0,
            0,
            null(),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    Ok(())
}

fn open_trace(state: &mut ConsumerState) -> io::Result<PROCESSTRACE_HANDLE> {
    state.logfile.LoggerName = state.name.as_mut_ptr();
    // SAFETY: the logfile block is fully initialised, its `LoggerName` points
    // at the NUL-terminated buffer the same box owns, and the box outlives
    // every call ETW makes through it.
    let handle = unsafe { OpenTraceW(&mut state.logfile) };
    if handle.Value == INVALID_PROCESSTRACE_HANDLE {
        return Err(io::Error::last_os_error());
    }
    Ok(handle)
}

fn close_trace(trace: PROCESSTRACE_HANDLE) {
    // SAFETY: the handle came from `OpenTraceW` and is closed once. A close
    // issued while `ProcessTrace` still runs answers ERROR_CTX_CLOSE_PENDING
    // and completes when that call returns, which is what the caller wants.
    unsafe {
        CloseTrace(trace);
    }
}

/// An `EVENT_TRACE_PROPERTIES` allocation with the session name living in the
/// bytes after the fixed header, which is the shape both `StartTraceW` and
/// `ControlTraceW` require. The storage is `u64` so the block is aligned for
/// the structure, and it is zeroed, which is how ETW reads "unset".
struct TraceProperties {
    storage: Vec<u64>,
    bytes: u32,
}

impl TraceProperties {
    /// A block describing the session to start, with `name` copied into the
    /// tail and `LoggerNameOffset` pointing at it.
    fn for_start(name: &[u16]) -> Self {
        let header = size_of::<EVENT_TRACE_PROPERTIES>();
        let name_bytes = std::mem::size_of_val(name);
        let mut properties = Self::zeroed(header + name_bytes);
        let total = properties.bytes;
        let block = properties.as_mut();
        block.Wnode.BufferSize = total;
        block.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
        // QPC, the highest-resolution clock ETW offers for timestamps.
        block.Wnode.ClientContext = 1;
        block.BufferSize = BUFFER_SIZE_KILOBYTES;
        block.MinimumBuffers = MINIMUM_BUFFERS;
        block.MaximumBuffers = MAXIMUM_BUFFERS;
        block.LogFileMode = EVENT_TRACE_REAL_TIME_MODE | EVENT_TRACE_USE_MS_FLUSH_TIMER;
        block.FlushTimer = FLUSH_TIMER_MILLISECONDS;
        block.LoggerNameOffset = header as u32;
        // SAFETY: the allocation is `header + name_bytes` bytes long, so the
        // name fits exactly in the tail that begins at `header`, and the two
        // regions belong to different allocations.
        unsafe {
            std::ptr::copy_nonoverlapping(
                name.as_ptr().cast::<u8>(),
                properties.storage.as_mut_ptr().cast::<u8>().add(header),
                name_bytes,
            );
        }
        properties
    }

    /// A block for a control call, with room for the two names ETW writes
    /// back into it.
    fn for_control() -> Self {
        let header = size_of::<EVENT_TRACE_PROPERTIES>();
        let name_bytes = CONTROL_NAME_UNITS * size_of::<u16>();
        let mut properties = Self::zeroed(header + 2 * name_bytes);
        let total = properties.bytes;
        let block = properties.as_mut();
        block.Wnode.BufferSize = total;
        block.LoggerNameOffset = header as u32;
        block.LogFileNameOffset = (header + name_bytes) as u32;
        properties
    }

    fn zeroed(bytes: usize) -> Self {
        Self {
            storage: vec![0_u64; bytes.div_ceil(size_of::<u64>())],
            bytes: bytes as u32,
        }
    }

    fn as_mut_ptr(&mut self) -> *mut EVENT_TRACE_PROPERTIES {
        self.storage.as_mut_ptr().cast()
    }

    fn as_mut(&mut self) -> &mut EVENT_TRACE_PROPERTIES {
        // SAFETY: the storage is at least one `EVENT_TRACE_PROPERTIES` long,
        // is aligned for it because `u64` is its strictest member, and was
        // zeroed, which is a valid value of every field.
        unsafe { &mut *self.as_mut_ptr() }
    }
}

/// Every process identifier in the process table right now.
///
/// The ETW session only reports processes that start after it does, so the
/// caller needs this once at subscription time to reach everything already
/// running. A Toolhelp snapshot answers that in a single call.
pub fn running_process_ids() -> io::Result<Vec<u32>> {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    // SAFETY: the snapshot call takes plain integers and returns a handle or
    // `INVALID_HANDLE_VALUE`; no memory is shared with the callee.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    let snapshot = crate::handle::OwnedHandle::from_creation(snapshot)?;

    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: the handle is live and `entry` is a local structure with its
    // required `dwSize` set, which is the whole contract of this call.
    if unsafe { Process32FirstW(snapshot.as_raw(), &mut entry) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut pids = Vec::new();
    loop {
        if entry.th32ProcessID != 0 {
            pids.push(entry.th32ProcessID);
        }
        // SAFETY: same contract as the first read; the walk ends when the call
        // reports failure.
        if unsafe { Process32NextW(snapshot.as_raw(), &mut entry) } == 0 {
            break;
        }
    }
    pids.sort_unstable();
    pids.dedup();
    Ok(pids)
}

#[cfg(test)]
mod tests {
    use super::{TraceProperties, decode_process_start_id, guid_equals, running_process_ids};
    use std::mem::size_of;
    use windows_sys::Win32::System::Diagnostics::Etw::EVENT_TRACE_PROPERTIES;
    use windows_sys::core::GUID;

    #[test]
    fn a_process_start_payload_yields_its_leading_process_id() {
        // ProcessStart version 3: ProcessID, CreateTime, ParentProcessID, ...
        let mut payload = 4_242_u32.to_le_bytes().to_vec();
        payload.extend_from_slice(&0x0123_4567_89ab_cdef_u64.to_le_bytes());
        payload.extend_from_slice(&7_u32.to_le_bytes());

        assert_eq!(decode_process_start_id(&payload), Some(4_242));
    }

    #[test]
    fn a_payload_with_exactly_the_process_id_is_enough() {
        assert_eq!(decode_process_start_id(&1_u32.to_le_bytes()), Some(1));
    }

    #[test]
    fn a_payload_too_short_for_the_field_is_refused_rather_than_padded() {
        assert_eq!(decode_process_start_id(&[]), None);
        assert_eq!(decode_process_start_id(&[1]), None);
        assert_eq!(decode_process_start_id(&[1, 0, 0]), None);
    }

    #[test]
    fn process_zero_is_refused_because_no_process_may_carry_it() {
        assert_eq!(decode_process_start_id(&0_u32.to_le_bytes()), None);
        assert_eq!(decode_process_start_id(&[0, 0, 0, 0, 9, 9]), None);
    }

    #[test]
    fn guids_compare_by_every_field() {
        let provider = GUID::from_u128(0x22fb2cd6_0e7b_422b_a0c7_2fad1fd0e716);
        let same = GUID::from_u128(0x22fb2cd6_0e7b_422b_a0c7_2fad1fd0e716);
        assert!(guid_equals(&provider, &same));
        assert!(!guid_equals(
            &provider,
            &GUID::from_u128(0x22fb2cd6_0e7b_422b_a0c7_2fad1fd0e717)
        ));
        assert!(!guid_equals(&provider, &GUID::default()));
    }

    #[test]
    fn a_start_block_carries_its_name_after_the_header_and_says_so() {
        let name: Vec<u16> = "MacType-Test\0".encode_utf16().collect();
        let mut properties = TraceProperties::for_start(&name);
        let header = size_of::<EVENT_TRACE_PROPERTIES>();
        let expected = header + name.len() * size_of::<u16>();

        let block = properties.as_mut();
        assert_eq!(block.Wnode.BufferSize as usize, expected);
        assert_eq!(block.LoggerNameOffset as usize, header);
        assert_eq!(block.FlushTimer, super::FLUSH_TIMER_MILLISECONDS);
        assert_eq!(
            block.LogFileMode,
            super::EVENT_TRACE_REAL_TIME_MODE | super::EVENT_TRACE_USE_MS_FLUSH_TIMER
        );

        let bytes = properties.storage.as_ptr().cast::<u8>();
        // SAFETY: the allocation is `expected` bytes long and the name was
        // copied into the tail that starts at `header`.
        let tail =
            unsafe { std::slice::from_raw_parts(bytes.add(header).cast::<u16>(), name.len()) };
        assert_eq!(tail, name.as_slice());
    }

    #[test]
    fn a_control_block_reserves_room_for_both_names_etw_writes_back() {
        let mut properties = TraceProperties::for_control();
        let header = size_of::<EVENT_TRACE_PROPERTIES>();
        let block = properties.as_mut();

        assert_eq!(block.LoggerNameOffset as usize, header);
        assert!(block.LogFileNameOffset > block.LoggerNameOffset);
        assert!(block.Wnode.BufferSize > block.LogFileNameOffset);
    }

    #[test]
    fn the_snapshot_contains_this_very_process() {
        let pids = running_process_ids().unwrap();
        assert!(pids.contains(&std::process::id()));
        assert!(pids.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
