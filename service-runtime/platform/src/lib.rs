//! Safe, typed ownership of every Win32 call the MacType service makes.
//!
//! This is the only crate in the service workspace that may contain `unsafe`.
//! `mactype-service-host` and `mactype-service-setup` forbid it at their crate
//! roots, so a raw handle, pointer, or buffer length never leaves this crate:
//! callers receive owned types whose destructors release the object, values
//! that were bounds-checked before they were read, and `io::Error`s that carry
//! the Win32 code. Every `unsafe` block below names, in a `SAFETY:` comment,
//! the invariant the surrounding type upholds for it.
#![cfg(windows)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

mod clipboard;
mod com;
mod end_session;
mod environment;
mod event;
mod file;
mod gdi;
mod handle;
mod job;
mod known_folders;
mod launch;
mod mutex;
mod pipe;
mod process;
mod random;
mod registry;
mod scm;
mod scm_response;
mod security;
mod service_control;
mod shell_link;
mod wide;
mod window;
mod wmi;
mod wts;

pub use clipboard::set_clipboard_unicode_text;
pub use com::{ComApartment, ComThreading};
pub use end_session::{end_session_action, install_end_session_exit_hook, EndSessionAction};
pub use environment::expand_environment_strings;
pub use event::ManualResetEvent;
pub use file::{
    delay_delete_until_reboot, file_attributes, is_reparse_point, mark_open_file_for_deletion,
    replace_file, replace_file_preserving_attributes,
};
pub use gdi::installed_font_families;
pub use handle::{OwnedHandle, WaitOutcome};
pub use job::{JobLimits, JobObject};
pub use known_folders::{known_folder_path, system_directory, KnownFolder};
pub use launch::{
    anonymous_pipe, null_device, read_bounded, ProcessLaunch, StandardHandles, SuspendedChild,
};
pub use mutex::{MutexAcquisition, NamedMutex};
pub use pipe::{
    cancelled_pipe_operations, named_pipe_server_process_id, ConnectOutcome, NamedPipeClient,
    NamedPipeServer, OverlappedPipeClient, OverlappedPipeServer, PipeAccess, PipeDirection,
    PipeError, PipeWait,
};
pub use process::{
    process_session_id, terminate_current_process, MachineKind, Process, ProcessAccess,
    ProcessBinarySignatureMitigation, ProcessDynamicCodeMitigation, ProcessLifecycleFlags,
    ProcessMachine,
};
pub use random::system_random_bytes;
pub use registry::{
    DeleteValueOutcome, RawRegistryValue, RegistryKey, RegistryRoot, RegistryValue,
    RegistryValueData, RegistryView,
};
pub use scm::{
    FailureAction, FailureActionKind, FailureActions, ServiceAccess, ServiceConfig,
    ServiceControlManager, ServiceCreation, ServiceHandle, ServiceManagerAccess, ServiceState,
    ServiceStatusSnapshot, StartOutcome, StopOutcome, TriggerInfo,
};
pub use security::{
    current_token_is_member_of, current_user_sid_string, AclValidationError, AllowedAce,
    LocalSecurityDescriptor, OwnedSid, PrivilegeGuard, SecurityAclError, SecurityDescriptor,
    SelfRelativeSecurityDescriptor, ValidatedDacl, ValidatedSid,
};
pub use service_control::{
    register_control_handler, report_status, run_service_dispatcher, RawServiceStatus,
    ServiceControlCallback, ServiceStatusHandle,
};
pub use shell_link::{read_shortcut, ShortcutTarget};
pub use window::{
    register_window_message, send_notify_message, top_level_windows, TopLevelWindow, WindowHandle,
};
pub use wmi::{
    WmiConnection, WmiEnumerator, WmiError, WmiLocator, WmiNamespace, WmiObject, WmiPropertyError,
    WmiPropertyStep,
};
pub use wts::{interactive_processes, SessionProcess};
