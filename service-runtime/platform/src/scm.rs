//! Service Control Manager handles and the operations the setup broker and
//! the startup safety probe perform through them.

use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    ERROR_INSUFFICIENT_BUFFER, ERROR_SERVICE_ALREADY_RUNNING, ERROR_SERVICE_DOES_NOT_EXIST,
    ERROR_SERVICE_NOT_ACTIVE,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, UNPROTECTED_DACL_SECURITY_INFORMATION,
};
use windows_sys::Win32::System::Services::{
    ChangeServiceConfig2W, ChangeServiceConfigW, CloseServiceHandle, ControlService,
    CreateServiceW, DeleteService, OpenSCManagerW, OpenServiceW, QueryServiceConfig2W,
    QueryServiceConfigW, QueryServiceObjectSecurity, QueryServiceStatusEx,
    SetServiceObjectSecurity, StartServiceW, QUERY_SERVICE_CONFIGW, SC_ACTION, SC_ACTION_NONE,
    SC_ACTION_OWN_RESTART, SC_ACTION_REBOOT, SC_ACTION_RESTART, SC_ACTION_RUN_COMMAND, SC_HANDLE,
    SC_MANAGER_CONNECT, SC_MANAGER_CREATE_SERVICE, SC_STATUS_PROCESS_INFO, SERVICE_ALL_ACCESS,
    SERVICE_AUTO_START, SERVICE_CHANGE_CONFIG, SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
    SERVICE_CONFIG_DESCRIPTION, SERVICE_CONFIG_FAILURE_ACTIONS,
    SERVICE_CONFIG_FAILURE_ACTIONS_FLAG, SERVICE_CONFIG_PRESHUTDOWN_INFO,
    SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO, SERVICE_CONFIG_SERVICE_SID_INFO,
    SERVICE_CONFIG_TRIGGER_INFO, SERVICE_CONTINUE_PENDING, SERVICE_CONTROL_STOP,
    SERVICE_DELAYED_AUTO_START_INFO, SERVICE_DESCRIPTIONW, SERVICE_ERROR_NORMAL,
    SERVICE_FAILURE_ACTIONSW, SERVICE_FAILURE_ACTIONS_FLAG, SERVICE_NO_CHANGE, SERVICE_PAUSED,
    SERVICE_PAUSE_PENDING, SERVICE_PRESHUTDOWN_INFO, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS,
    SERVICE_REQUIRED_PRIVILEGES_INFOW, SERVICE_RUNNING, SERVICE_SID_INFO, SERVICE_START,
    SERVICE_START_PENDING, SERVICE_STATUS, SERVICE_STATUS_PROCESS, SERVICE_STOP, SERVICE_STOPPED,
    SERVICE_STOP_PENDING, SERVICE_TRIGGER_INFO, SERVICE_WIN32_OWN_PROCESS,
};

use crate::scm_response::ScmResponse;
use crate::security::SelfRelativeSecurityDescriptor;
use crate::wide::{multi_string, wide_null};

const MAX_SERVICE_DEPENDENCIES: usize = 256;
const MAX_FAILURE_ACTIONS: usize = 64;
const MAX_REQUIRED_PRIVILEGES: usize = 64;
const MAX_SECURITY_DESCRIPTOR_BYTES: u32 = 64 * 1024;
const SERVICE_DELETE: u32 = 0x0001_0000;
const READ_CONTROL: u32 = 0x0002_0000;
const WRITE_DAC: u32 = 0x0004_0000;
const WRITE_OWNER: u32 = 0x0008_0000;

/// What the manager connection may do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceManagerAccess {
    Connect,
    ConnectAndCreate,
}

/// What an opened service handle may do. Each variant is a fixed rights set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceAccess {
    QueryStatus,
    QueryConfig,
    QueryStatusAndConfig,
    Inspect,
    Start,
    Stop,
    Reconfigure,
    ChangeStartType,
    Restore,
    Delete,
}

impl ServiceAccess {
    /// The Win32 access mask this variant opens the service with. Exposed so
    /// callers can assert a policy about the rights they hold (for example
    /// that reconfiguration also carries `SERVICE_START`).
    pub fn rights(self) -> u32 {
        match self {
            Self::QueryStatus => SERVICE_QUERY_STATUS,
            Self::QueryConfig => SERVICE_QUERY_CONFIG,
            Self::QueryStatusAndConfig => SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG,
            Self::Inspect => SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG | READ_CONTROL,
            Self::Start => SERVICE_START | SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG,
            Self::Stop => SERVICE_STOP | SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG,
            Self::Reconfigure => {
                SERVICE_CHANGE_CONFIG | SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG | SERVICE_START
            }
            Self::ChangeStartType => SERVICE_CHANGE_CONFIG | SERVICE_QUERY_STATUS,
            Self::Restore => {
                SERVICE_CHANGE_CONFIG
                    | SERVICE_QUERY_CONFIG
                    | READ_CONTROL
                    | WRITE_DAC
                    | WRITE_OWNER
            }
            Self::Delete => SERVICE_DELETE | SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG,
        }
    }
}

/// A service state as the SCM reports it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceState {
    Stopped,
    StartPending,
    StopPending,
    Running,
    ContinuePending,
    PausePending,
    Paused,
    Other(u32),
}

impl ServiceState {
    pub fn from_raw(value: u32) -> Self {
        match value {
            SERVICE_STOPPED => Self::Stopped,
            SERVICE_START_PENDING => Self::StartPending,
            SERVICE_STOP_PENDING => Self::StopPending,
            SERVICE_RUNNING => Self::Running,
            SERVICE_CONTINUE_PENDING => Self::ContinuePending,
            SERVICE_PAUSE_PENDING => Self::PausePending,
            SERVICE_PAUSED => Self::Paused,
            other => Self::Other(other),
        }
    }

    pub fn as_raw(self) -> u32 {
        match self {
            Self::Stopped => SERVICE_STOPPED,
            Self::StartPending => SERVICE_START_PENDING,
            Self::StopPending => SERVICE_STOP_PENDING,
            Self::Running => SERVICE_RUNNING,
            Self::ContinuePending => SERVICE_CONTINUE_PENDING,
            Self::PausePending => SERVICE_PAUSE_PENDING,
            Self::Paused => SERVICE_PAUSED,
            Self::Other(value) => value,
        }
    }
}

/// The process-level status of a service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceStatusSnapshot {
    pub state: ServiceState,
    pub process_id: u32,
    pub win32_exit_code: u32,
    pub service_specific_exit_code: u32,
}

/// A service's configuration, copied into owned strings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceConfig {
    pub service_type: u32,
    pub start_type: u32,
    pub error_control: u32,
    pub image_path: String,
    pub account: String,
    pub display_name: String,
    pub load_order_group: String,
    pub tag_id: u32,
    pub dependencies: Vec<String>,
}

pub struct ServiceCreation<'a> {
    pub display_name: &'a str,
    pub service_type: u32,
    pub start_type: u32,
    pub error_control: u32,
    pub image_path: &'a str,
    pub load_order_group: Option<&'a str>,
    pub dependencies: &'a [String],
    pub account: &'a str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailureActions {
    pub reset_period_seconds: u32,
    pub reboot_message: Option<String>,
    pub command: Option<String>,
    pub actions: Vec<FailureAction>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FailureAction {
    pub kind: FailureActionKind,
    pub delay_ms: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureActionKind {
    None,
    Restart,
    Reboot,
    RunCommand,
    OwnRestart,
}

impl FailureActionKind {
    pub fn from_raw(value: i32) -> Option<Self> {
        match value {
            SC_ACTION_NONE => Some(Self::None),
            SC_ACTION_RESTART => Some(Self::Restart),
            SC_ACTION_REBOOT => Some(Self::Reboot),
            SC_ACTION_RUN_COMMAND => Some(Self::RunCommand),
            SC_ACTION_OWN_RESTART => Some(Self::OwnRestart),
            _ => None,
        }
    }

    pub fn as_raw(self) -> i32 {
        match self {
            Self::None => SC_ACTION_NONE,
            Self::Restart => SC_ACTION_RESTART,
            Self::Reboot => SC_ACTION_REBOOT,
            Self::RunCommand => SC_ACTION_RUN_COMMAND,
            Self::OwnRestart => SC_ACTION_OWN_RESTART,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TriggerInfo {
    pub count: u32,
    pub has_trigger_data: bool,
    pub has_reserved_data: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartOutcome {
    Started,
    AlreadyRunning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopOutcome {
    StopRequested,
    NotActive,
}

/// A connection to the SCM.
#[derive(Debug)]
pub struct ServiceControlManager(ScHandle);

impl ServiceControlManager {
    pub fn connect(access: ServiceManagerAccess) -> io::Result<Self> {
        let rights = match access {
            ServiceManagerAccess::Connect => SC_MANAGER_CONNECT,
            ServiceManagerAccess::ConnectAndCreate => {
                SC_MANAGER_CONNECT | SC_MANAGER_CREATE_SERVICE
            }
        };
        // SAFETY: null machine and database names select the local active
        // database; the call takes no other pointers.
        let handle = unsafe { OpenSCManagerW(null(), null(), rights) };
        Ok(Self(ScHandle::from_open(handle)?))
    }

    /// Opens `name`. `Ok(None)` when no such service exists; other failures,
    /// including a delete-pending record, carry their Win32 code.
    pub fn open(&self, name: &str, access: ServiceAccess) -> io::Result<Option<ServiceHandle>> {
        let name = wide_null(name);
        // SAFETY: the manager handle is live and `name` is NUL-terminated.
        let handle = unsafe { OpenServiceW(self.0.as_raw(), name.as_ptr(), access.rights()) };
        if handle.is_null() {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST as i32) {
                return Ok(None);
            }
            return Err(error);
        }
        Ok(Some(ServiceHandle(ScHandle(handle))))
    }

    /// Creates a service from the complete supplied configuration.
    pub fn create(
        &self,
        name: &str,
        creation: &ServiceCreation<'_>,
        access: ServiceAccess,
    ) -> io::Result<ServiceHandle> {
        validate_strings(
            [
                name,
                creation.display_name,
                creation.image_path,
                creation.account,
            ]
            .into_iter()
            .chain(creation.load_order_group)
            .chain(creation.dependencies.iter().map(String::as_str)),
        )?;
        let name = wide_null(name);
        let display_name = wide_null(creation.display_name);
        let image_path = wide_null(creation.image_path);
        let load_order_group = creation.load_order_group.map(wide_null);
        let dependencies = multi_string(creation.dependencies);
        let account = wide_null(creation.account);
        let mut tag_id = 0_u32;
        // SAFETY: the manager handle is live; every non-null string pointer is
        // NUL-terminated and all buffers and optional tag output outlive the call.
        let handle = unsafe {
            CreateServiceW(
                self.0.as_raw(),
                name.as_ptr(),
                display_name.as_ptr(),
                access.rights(),
                creation.service_type,
                creation.start_type,
                creation.error_control,
                image_path.as_ptr(),
                load_order_group
                    .as_ref()
                    .map_or(null(), |group| group.as_ptr()),
                load_order_group
                    .as_ref()
                    .map_or(null_mut(), |_| &mut tag_id),
                dependencies.as_ptr(),
                account.as_ptr(),
                null(),
            )
        };
        Ok(ServiceHandle(ScHandle::from_open(handle)?))
    }

    /// Creates an auto-start, own-process, LocalSystem service with full
    /// access on the returned handle.
    pub fn create_own_process_auto_start(
        &self,
        name: &str,
        display_name: &str,
        image_path: &str,
    ) -> io::Result<ServiceHandle> {
        let name = wide_null(name);
        let display_name = wide_null(display_name);
        let image_path = wide_null(image_path);
        // SAFETY: the manager handle is live and every string is
        // NUL-terminated; the null arguments select no load order group, tag,
        // dependencies, account (LocalSystem), or password.
        let handle = unsafe {
            CreateServiceW(
                self.0.as_raw(),
                name.as_ptr(),
                display_name.as_ptr(),
                SERVICE_ALL_ACCESS,
                SERVICE_WIN32_OWN_PROCESS,
                SERVICE_AUTO_START,
                SERVICE_ERROR_NORMAL,
                image_path.as_ptr(),
                null(),
                null_mut(),
                null(),
                null(),
                null(),
            )
        };
        Ok(ServiceHandle(ScHandle::from_open(handle)?))
    }
}

/// An open service handle.
#[derive(Debug)]
pub struct ServiceHandle(ScHandle);

impl ServiceHandle {
    pub fn status(&self) -> io::Result<ServiceStatusSnapshot> {
        let mut status = SERVICE_STATUS_PROCESS::default();
        let mut needed = 0;
        // SAFETY: the handle is live; the buffer pointer and length describe
        // exactly the local status structure.
        if unsafe {
            QueryServiceStatusEx(
                self.0.as_raw(),
                SC_STATUS_PROCESS_INFO,
                (&mut status as *mut SERVICE_STATUS_PROCESS).cast::<u8>(),
                size_of::<SERVICE_STATUS_PROCESS>() as u32,
                &mut needed,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(ServiceStatusSnapshot {
            state: ServiceState::from_raw(status.dwCurrentState),
            process_id: status.dwProcessId,
            win32_exit_code: status.dwWin32ExitCode,
            service_specific_exit_code: status.dwServiceSpecificExitCode,
        })
    }

    pub fn config(&self) -> io::Result<ServiceConfig> {
        let response = ScmResponse::query(
            size_of::<QUERY_SERVICE_CONFIGW>(),
            |buffer, capacity, needed| {
                // SAFETY: the handle is live; `buffer` and `capacity` describe
                // the response allocation or the documented null size probe,
                // and `needed` is a local out value.
                unsafe { QueryServiceConfigW(self.0.as_raw(), buffer.cast(), capacity, needed) }
            },
        )?;
        parse_config(&response)
    }

    fn config2(&self, level: u32, minimum_bytes: usize) -> io::Result<ScmResponse> {
        ScmResponse::query(minimum_bytes, |buffer, capacity, needed| {
            // SAFETY: the handle is live; `buffer` and `capacity` describe the
            // response allocation or the documented null size probe, and
            // `needed` is a local out value.
            unsafe { QueryServiceConfig2W(self.0.as_raw(), level, buffer, capacity, needed) }
        })
    }

    pub fn description(&self) -> io::Result<Option<String>> {
        let response = self.config2(
            SERVICE_CONFIG_DESCRIPTION,
            size_of::<SERVICE_DESCRIPTIONW>(),
        )?;
        let description = response.header::<SERVICE_DESCRIPTIONW>()?;
        strict_optional_text(&response, description.lpDescription)
    }

    pub fn failure_actions(&self) -> io::Result<FailureActions> {
        let response = self.config2(
            SERVICE_CONFIG_FAILURE_ACTIONS,
            size_of::<SERVICE_FAILURE_ACTIONSW>(),
        )?;
        let failure = response.header::<SERVICE_FAILURE_ACTIONSW>()?;
        let actions = response
            .array::<SC_ACTION>(failure.lpsaActions, failure.cActions, MAX_FAILURE_ACTIONS)?
            .into_iter()
            .map(|action| {
                let kind = FailureActionKind::from_raw(action.Type).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "unsupported failure action")
                })?;
                Ok(FailureAction {
                    kind,
                    delay_ms: action.Delay,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(FailureActions {
            reset_period_seconds: failure.dwResetPeriod,
            reboot_message: strict_optional_text(&response, failure.lpRebootMsg)?,
            command: strict_optional_text(&response, failure.lpCommand)?,
            actions,
        })
    }

    pub fn failure_actions_on_non_crash_failures(&self) -> io::Result<bool> {
        let response = self.config2(
            SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
            size_of::<SERVICE_FAILURE_ACTIONS_FLAG>(),
        )?;
        Ok(response
            .header::<SERVICE_FAILURE_ACTIONS_FLAG>()?
            .fFailureActionsOnNonCrashFailures
            != 0)
    }

    pub fn delayed_auto_start(&self) -> io::Result<bool> {
        let response = self.config2(
            SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
            size_of::<SERVICE_DELAYED_AUTO_START_INFO>(),
        )?;
        Ok(response
            .header::<SERVICE_DELAYED_AUTO_START_INFO>()?
            .fDelayedAutostart
            != 0)
    }

    pub fn service_sid_type(&self) -> io::Result<u32> {
        let response = self.config2(
            SERVICE_CONFIG_SERVICE_SID_INFO,
            size_of::<SERVICE_SID_INFO>(),
        )?;
        Ok(response.header::<SERVICE_SID_INFO>()?.dwServiceSidType)
    }

    pub fn required_privileges(&self) -> io::Result<Vec<String>> {
        let response = self.config2(
            SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO,
            size_of::<SERVICE_REQUIRED_PRIVILEGES_INFOW>(),
        )?;
        let privileges = response.header::<SERVICE_REQUIRED_PRIVILEGES_INFOW>()?;
        response
            .multi_units(privileges.pmszRequiredPrivileges, MAX_REQUIRED_PRIVILEGES)?
            .into_iter()
            .map(|units| {
                String::from_utf16(units).map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "required privilege is not valid UTF-16",
                    )
                })
            })
            .collect()
    }

    pub fn preshutdown_timeout_ms(&self) -> io::Result<u32> {
        let response = self.config2(
            SERVICE_CONFIG_PRESHUTDOWN_INFO,
            size_of::<SERVICE_PRESHUTDOWN_INFO>(),
        )?;
        Ok(response
            .header::<SERVICE_PRESHUTDOWN_INFO>()?
            .dwPreshutdownTimeout)
    }

    pub fn trigger_info(&self) -> io::Result<TriggerInfo> {
        let response = self.config2(
            SERVICE_CONFIG_TRIGGER_INFO,
            size_of::<SERVICE_TRIGGER_INFO>(),
        )?;
        let trigger = response.header::<SERVICE_TRIGGER_INFO>()?;
        Ok(TriggerInfo {
            count: trigger.cTriggers,
            has_trigger_data: !trigger.pTriggers.is_null(),
            has_reserved_data: !trigger.pReserved.is_null(),
        })
    }

    pub fn object_security(&self) -> io::Result<SelfRelativeSecurityDescriptor> {
        let information =
            OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
        let mut needed = 0_u32;
        // SAFETY: the handle is live; a null descriptor and zero capacity form
        // the documented size probe, and `needed` is a local out value.
        let result = unsafe {
            QueryServiceObjectSecurity(self.0.as_raw(), information, null_mut(), 0, &mut needed)
        };
        let error = io::Error::last_os_error();
        if result != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "service security size probe succeeded without a buffer",
            ));
        }
        if error.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32) {
            return Err(error);
        }
        if needed == 0 || needed > MAX_SECURITY_DESCRIPTOR_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "service security descriptor size is out of range",
            ));
        }

        let mut descriptor = vec![0_u8; needed as usize];
        // SAFETY: the handle is live; `descriptor` is writable for the capacity
        // passed with it, and `needed` is a local out value.
        if unsafe {
            QueryServiceObjectSecurity(
                self.0.as_raw(),
                information,
                descriptor.as_mut_ptr().cast(),
                descriptor.len() as u32,
                &mut needed,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if needed as usize > descriptor.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "service security descriptor changed during the read",
            ));
        }
        descriptor.truncate(needed as usize);
        SelfRelativeSecurityDescriptor::from_bytes(
            descriptor,
            MAX_SECURITY_DESCRIPTOR_BYTES as usize,
        )
    }

    pub fn start(&self) -> io::Result<StartOutcome> {
        // SAFETY: the handle is live; no arguments are passed to the service.
        if unsafe { StartServiceW(self.0.as_raw(), 0, null()) } == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_SERVICE_ALREADY_RUNNING as i32) {
                return Ok(StartOutcome::AlreadyRunning);
            }
            return Err(error);
        }
        Ok(StartOutcome::Started)
    }

    pub fn stop(&self) -> io::Result<StopOutcome> {
        let mut status = SERVICE_STATUS::default();
        // SAFETY: the handle is live; `status` is a local out structure.
        if unsafe { ControlService(self.0.as_raw(), SERVICE_CONTROL_STOP, &mut status) } == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_SERVICE_NOT_ACTIVE as i32) {
                return Ok(StopOutcome::NotActive);
            }
            return Err(error);
        }
        Ok(StopOutcome::StopRequested)
    }

    pub fn delete(&self) -> io::Result<()> {
        // SAFETY: the handle is live; the call takes no pointers.
        if unsafe { DeleteService(self.0.as_raw()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Restores every core service configuration field.
    pub fn set_config(&self, config: &ServiceConfig) -> io::Result<()> {
        validate_strings(
            [
                config.image_path.as_str(),
                config.account.as_str(),
                config.display_name.as_str(),
                config.load_order_group.as_str(),
            ]
            .into_iter()
            .chain(config.dependencies.iter().map(String::as_str)),
        )?;
        let image_path = wide_null(&config.image_path);
        let load_order_group =
            (!config.load_order_group.is_empty()).then(|| wide_null(&config.load_order_group));
        let dependencies = multi_string(&config.dependencies);
        let account = wide_null(&config.account);
        let display_name = wide_null(&config.display_name);
        let mut tag_id = 0_u32;
        // SAFETY: the handle is live; every non-null string is NUL-terminated,
        // the multi-string and optional tag output remain live for the call.
        if unsafe {
            ChangeServiceConfigW(
                self.0.as_raw(),
                config.service_type,
                config.start_type,
                config.error_control,
                image_path.as_ptr(),
                load_order_group
                    .as_ref()
                    .map_or(null(), |group| group.as_ptr()),
                load_order_group
                    .as_ref()
                    .map_or(null_mut(), |_| &mut tag_id),
                dependencies.as_ptr(),
                account.as_ptr(),
                null(),
                display_name.as_ptr(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn set_start_type(&self, start_type: u32) -> io::Result<()> {
        // SAFETY: the handle is live and every null pointer selects no change.
        if unsafe {
            ChangeServiceConfigW(
                self.0.as_raw(),
                SERVICE_NO_CHANGE,
                start_type,
                SERVICE_NO_CHANGE,
                null(),
                null(),
                null_mut(),
                null(),
                null(),
                null(),
                null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Rewrites the image path and display name and forces auto start,
    /// leaving every other field unchanged.
    pub fn set_image_and_display_name(
        &self,
        image_path: &str,
        display_name: &str,
    ) -> io::Result<()> {
        let image_path = wide_null(image_path);
        let display_name = wide_null(display_name);
        // SAFETY: the handle is live; both strings are NUL-terminated; the
        // remaining arguments are the documented "no change" values.
        if unsafe {
            ChangeServiceConfigW(
                self.0.as_raw(),
                SERVICE_NO_CHANGE,
                SERVICE_AUTO_START,
                SERVICE_NO_CHANGE,
                image_path.as_ptr(),
                null(),
                null_mut(),
                null(),
                null(),
                null(),
                display_name.as_ptr(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Restores error control and clears the load-order group and dependencies.
    pub fn restore_owned_mutable_configuration(&self) -> io::Result<()> {
        let empty_group = wide_null("");
        let empty_dependencies = multi_string(&[]);
        // SAFETY: the handle is live; the empty group string and dependency
        // multi-string outlive the call. Null pointers preserve image, account,
        // password, display name, and the existing tag value.
        if unsafe {
            ChangeServiceConfigW(
                self.0.as_raw(),
                SERVICE_NO_CHANGE,
                SERVICE_NO_CHANGE,
                SERVICE_ERROR_NORMAL,
                null(),
                empty_group.as_ptr(),
                null_mut(),
                empty_dependencies.as_ptr(),
                null(),
                null(),
                null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn set_optional_description(&self, description: Option<&str>) -> io::Result<()> {
        if let Some(description) = description {
            validate_strings(std::iter::once(description))?;
        }
        let mut description = description.map(wide_null);
        let information = SERVICE_DESCRIPTIONW {
            lpDescription: description
                .as_mut()
                .map_or(null_mut(), |value| value.as_mut_ptr()),
        };
        // SAFETY: the structure and optional text buffer outlive the call.
        self.change_config2(
            SERVICE_CONFIG_DESCRIPTION,
            (&information as *const SERVICE_DESCRIPTIONW).cast(),
        )
    }

    pub fn set_description(&self, text: &str) -> io::Result<()> {
        validate_strings(std::iter::once(text))?;
        let mut text = wide_null(text);
        let description = SERVICE_DESCRIPTIONW {
            lpDescription: text.as_mut_ptr(),
        };
        // SAFETY: the description structure and the buffer it points at
        // outlive the call.
        self.change_config2(
            SERVICE_CONFIG_DESCRIPTION,
            (&description as *const SERVICE_DESCRIPTIONW).cast(),
        )
    }

    /// Restart-on-failure actions with the given delays, counted from a reset
    /// period in seconds.
    pub fn set_restart_actions(
        &self,
        reset_period_seconds: u32,
        delays_ms: &[u32],
    ) -> io::Result<()> {
        let mut actions: Vec<SC_ACTION> = delays_ms
            .iter()
            .map(|delay| SC_ACTION {
                Type: SC_ACTION_RESTART,
                Delay: *delay,
            })
            .collect();
        let failure = SERVICE_FAILURE_ACTIONSW {
            dwResetPeriod: reset_period_seconds,
            lpRebootMsg: null_mut(),
            lpCommand: null_mut(),
            cActions: actions.len() as u32,
            lpsaActions: actions.as_mut_ptr(),
        };
        // SAFETY: the failure structure and the action array outlive the call.
        self.change_config2(
            SERVICE_CONFIG_FAILURE_ACTIONS,
            (&failure as *const SERVICE_FAILURE_ACTIONSW).cast(),
        )
    }

    pub fn set_failure_actions(&self, actions: &FailureActions) -> io::Result<()> {
        validate_strings(
            actions
                .reboot_message
                .iter()
                .chain(actions.command.iter())
                .map(String::as_str),
        )?;
        let mut reboot_message = actions.reboot_message.as_deref().map(wide_null);
        let mut command = actions.command.as_deref().map(wide_null);
        let mut encoded_actions: Vec<SC_ACTION> = actions
            .actions
            .iter()
            .map(|action| SC_ACTION {
                Type: action.kind.as_raw(),
                Delay: action.delay_ms,
            })
            .collect();
        let information = SERVICE_FAILURE_ACTIONSW {
            dwResetPeriod: actions.reset_period_seconds,
            lpRebootMsg: reboot_message
                .as_mut()
                .map_or(null_mut(), |value| value.as_mut_ptr()),
            lpCommand: command
                .as_mut()
                .map_or(null_mut(), |value| value.as_mut_ptr()),
            cActions: encoded_actions.len() as u32,
            lpsaActions: if encoded_actions.is_empty() {
                null_mut()
            } else {
                encoded_actions.as_mut_ptr()
            },
        };
        // SAFETY: the structure, optional strings, and action array outlive the call.
        self.change_config2(
            SERVICE_CONFIG_FAILURE_ACTIONS,
            (&information as *const SERVICE_FAILURE_ACTIONSW).cast(),
        )
    }

    pub fn set_failure_actions_on_non_crash_failures(&self, enabled: bool) -> io::Result<()> {
        let flag = SERVICE_FAILURE_ACTIONS_FLAG {
            fFailureActionsOnNonCrashFailures: i32::from(enabled),
        };
        // SAFETY: the flag structure outlives the call.
        self.change_config2(
            SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
            (&flag as *const SERVICE_FAILURE_ACTIONS_FLAG).cast(),
        )
    }

    pub fn set_delayed_auto_start(&self, enabled: bool) -> io::Result<()> {
        let information = SERVICE_DELAYED_AUTO_START_INFO {
            fDelayedAutostart: i32::from(enabled),
        };
        // SAFETY: the information structure outlives the call.
        self.change_config2(
            SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
            (&information as *const SERVICE_DELAYED_AUTO_START_INFO).cast(),
        )
    }

    pub fn set_service_sid_type(&self, sid_type: u32) -> io::Result<()> {
        let information = SERVICE_SID_INFO {
            dwServiceSidType: sid_type,
        };
        // SAFETY: the information structure outlives the call.
        self.change_config2(
            SERVICE_CONFIG_SERVICE_SID_INFO,
            (&information as *const SERVICE_SID_INFO).cast(),
        )
    }

    pub fn set_required_privileges(&self, privileges: &[String]) -> io::Result<()> {
        validate_strings(privileges.iter().map(String::as_str))?;
        let mut privileges = multi_string(privileges);
        let information = SERVICE_REQUIRED_PRIVILEGES_INFOW {
            pmszRequiredPrivileges: privileges.as_mut_ptr(),
        };
        // SAFETY: the information structure and multi-string outlive the call.
        self.change_config2(
            SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO,
            (&information as *const SERVICE_REQUIRED_PRIVILEGES_INFOW).cast(),
        )
    }

    pub fn set_preshutdown_timeout_ms(&self, timeout_ms: u32) -> io::Result<()> {
        let information = SERVICE_PRESHUTDOWN_INFO {
            dwPreshutdownTimeout: timeout_ms,
        };
        // SAFETY: the information structure outlives the call.
        self.change_config2(
            SERVICE_CONFIG_PRESHUTDOWN_INFO,
            (&information as *const SERVICE_PRESHUTDOWN_INFO).cast(),
        )
    }

    pub fn clear_triggers(&self) -> io::Result<()> {
        let information = SERVICE_TRIGGER_INFO::default();
        // SAFETY: the zeroed information structure outlives the call.
        self.change_config2(
            SERVICE_CONFIG_TRIGGER_INFO,
            (&information as *const SERVICE_TRIGGER_INFO).cast(),
        )
    }

    pub fn set_object_security(
        &self,
        descriptor: &SelfRelativeSecurityDescriptor,
    ) -> io::Result<()> {
        let protection = if descriptor.dacl_protected()? {
            PROTECTED_DACL_SECURITY_INFORMATION
        } else {
            UNPROTECTED_DACL_SECURITY_INFORMATION
        };
        let information = OWNER_SECURITY_INFORMATION
            | GROUP_SECURITY_INFORMATION
            | DACL_SECURITY_INFORMATION
            | protection;
        // SAFETY: the service handle and validated descriptor are live; the API
        // consumes the descriptor only for the duration of this call.
        let result = unsafe {
            SetServiceObjectSecurity(self.0.as_raw(), information, descriptor.as_ptr().cast_mut())
        };
        let error = io::Error::last_os_error();
        if result == 0 {
            return Err(error);
        }
        Ok(())
    }

    fn change_config2(&self, level: u32, data: *const c_void) -> io::Result<()> {
        // SAFETY: the handle is live and `data` points at a live structure of
        // the type `level` selects, kept alive by the caller.
        if unsafe { ChangeServiceConfig2W(self.0.as_raw(), level, data) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

fn strict_optional_text(response: &ScmResponse, pointer: *const u16) -> io::Result<Option<String>> {
    let Some(units) = response.wide_units(pointer)? else {
        return Ok(None);
    };
    if units.is_empty() {
        return Ok(None);
    }
    String::from_utf16(units).map(Some).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "service configuration string is not valid UTF-16",
        )
    })
}

fn validate_strings<'a>(values: impl IntoIterator<Item = &'a str>) -> io::Result<()> {
    if values.into_iter().any(|value| value.contains('\0')) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "service configuration string contains a NUL",
        ));
    }
    Ok(())
}

fn parse_config(response: &ScmResponse) -> io::Result<ServiceConfig> {
    let config = response.header::<QUERY_SERVICE_CONFIGW>()?;
    let text = |pointer| -> io::Result<String> {
        Ok(response
            .wide_units(pointer)?
            .map(String::from_utf16_lossy)
            .unwrap_or_default())
    };
    Ok(ServiceConfig {
        service_type: config.dwServiceType,
        start_type: config.dwStartType,
        error_control: config.dwErrorControl,
        image_path: text(config.lpBinaryPathName)?,
        account: text(config.lpServiceStartName)?,
        display_name: text(config.lpDisplayName)?,
        load_order_group: text(config.lpLoadOrderGroup)?,
        tag_id: config.dwTagId,
        dependencies: response
            .multi_units(config.lpDependencies, MAX_SERVICE_DEPENDENCIES)?
            .into_iter()
            .map(String::from_utf16_lossy)
            .collect(),
    })
}

/// An `SC_HANDLE` closed exactly once.
#[derive(Debug)]
struct ScHandle(SC_HANDLE);

impl ScHandle {
    fn from_open(handle: SC_HANDLE) -> io::Result<Self> {
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(handle))
    }

    fn as_raw(&self) -> SC_HANDLE {
        self.0
    }
}

impl Drop for ScHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from an SCM open or create call and is
        // closed once.
        unsafe { CloseServiceHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use std::mem::{size_of, size_of_val};
    use std::ptr::{null_mut, write};

    use windows_sys::Win32::System::Services::QUERY_SERVICE_CONFIGW;

    use super::{
        parse_config, FailureActionKind, ServiceAccess, ServiceControlManager,
        ServiceManagerAccess, ServiceState,
    };
    use crate::scm_response::ScmResponse;

    fn append_units(storage: &mut [u16], cursor: &mut usize, units: &[u16]) -> *mut u16 {
        let offset = *cursor;
        storage[offset..offset + units.len()].copy_from_slice(units);
        *cursor += units.len();
        storage.as_mut_ptr().wrapping_add(offset)
    }

    fn synthetic_config_response(dependencies: &[u16]) -> ScmResponse {
        let mut words = vec![0_usize; 64];
        let capacity = size_of_val(words.as_slice());
        // SAFETY: the slice covers the live word allocation as UTF-16 units;
        // `usize` alignment satisfies `u16` alignment.
        let storage = unsafe {
            std::slice::from_raw_parts_mut(
                words.as_mut_ptr().cast::<u16>(),
                capacity / size_of::<u16>(),
            )
        };
        let mut cursor = size_of::<QUERY_SERVICE_CONFIGW>().div_ceil(size_of::<u16>());
        let image_path = append_units(
            storage,
            &mut cursor,
            &"C:\\service.exe\0".encode_utf16().collect::<Vec<_>>(),
        );
        let account = append_units(
            storage,
            &mut cursor,
            &"LocalSystem\0".encode_utf16().collect::<Vec<_>>(),
        );
        let display_name = append_units(
            storage,
            &mut cursor,
            &"Synthetic Service\0".encode_utf16().collect::<Vec<_>>(),
        );
        let load_order_group = append_units(
            storage,
            &mut cursor,
            &"NetworkProvider\0".encode_utf16().collect::<Vec<_>>(),
        );
        let dependencies = append_units(storage, &mut cursor, dependencies);
        let header = QUERY_SERVICE_CONFIGW {
            dwServiceType: 0x20,
            dwStartType: 2,
            dwErrorControl: 1,
            lpBinaryPathName: image_path,
            lpLoadOrderGroup: load_order_group,
            dwTagId: 7,
            lpDependencies: dependencies,
            lpServiceStartName: account,
            lpDisplayName: display_name,
        };
        // SAFETY: the word buffer is aligned for QUERY_SERVICE_CONFIGW and has
        // enough space for the complete header at its start.
        unsafe { write(words.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>(), header) };
        ScmResponse::from_words(words, cursor * size_of::<u16>())
    }

    #[test]
    fn parse_config_reads_every_dependency_of_a_multi_string_block() {
        let dependencies: Vec<u16> = "RPCSS\0http\0\0".encode_utf16().collect();
        let response = synthetic_config_response(&dependencies);
        let config = parse_config(&response).unwrap();

        assert_eq!(config.dependencies, ["RPCSS", "http"]);
        assert_eq!(config.image_path, "C:\\service.exe");
        assert_eq!(config.account, "LocalSystem");
        assert_eq!(config.display_name, "Synthetic Service");
        assert_eq!(config.load_order_group, "NetworkProvider");
    }

    #[test]
    fn parse_config_rejects_a_dependency_block_without_its_terminator() {
        let dependencies: Vec<u16> = "RPCSS\0http\0".encode_utf16().collect();
        let response = synthetic_config_response(&dependencies);
        assert!(parse_config(&response).is_err());
    }

    #[test]
    fn parse_config_rejects_a_string_pointer_outside_the_response() {
        let mut words = vec![0_usize; 16];
        let outside = words.as_mut_ptr().cast::<u16>().wrapping_sub(1);
        let header = QUERY_SERVICE_CONFIGW {
            dwServiceType: 0,
            dwStartType: 0,
            dwErrorControl: 0,
            lpBinaryPathName: outside,
            lpLoadOrderGroup: null_mut(),
            dwTagId: 0,
            lpDependencies: null_mut(),
            lpServiceStartName: null_mut(),
            lpDisplayName: null_mut(),
        };
        // SAFETY: the word buffer is aligned for QUERY_SERVICE_CONFIGW and has
        // enough space for the complete header at its start.
        unsafe { write(words.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>(), header) };
        let byte_length = size_of_val(words.as_slice());
        let response = ScmResponse::from_words(words, byte_length);
        assert!(parse_config(&response).is_err());
    }

    #[test]
    fn set_config_rejects_embedded_nuls_before_calling_the_scm() {
        let manager = ServiceControlManager::connect(ServiceManagerAccess::Connect).unwrap();
        let service = manager
            .open("EventLog", ServiceAccess::QueryStatus)
            .unwrap()
            .unwrap();
        let config = super::ServiceConfig {
            service_type: 0,
            start_type: 0,
            error_control: 0,
            image_path: "bad\0path".to_owned(),
            account: String::new(),
            display_name: String::new(),
            load_order_group: String::new(),
            tag_id: 0,
            dependencies: Vec::new(),
        };
        assert_eq!(
            service.set_config(&config).unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn failure_action_kinds_round_trip() {
        for kind in [
            FailureActionKind::None,
            FailureActionKind::Restart,
            FailureActionKind::Reboot,
            FailureActionKind::RunCommand,
            FailureActionKind::OwnRestart,
        ] {
            assert_eq!(FailureActionKind::from_raw(kind.as_raw()), Some(kind));
        }
        assert_eq!(FailureActionKind::from_raw(i32::MAX), None);
    }

    #[test]
    fn event_log_reports_every_optional_configuration_and_its_security() {
        let manager = ServiceControlManager::connect(ServiceManagerAccess::Connect).unwrap();
        let service = manager
            .open("EventLog", ServiceAccess::Inspect)
            .unwrap()
            .expect("the Windows Event Log service exists");

        assert!(service
            .description()
            .unwrap()
            .is_some_and(|text| !text.is_empty()));
        service.failure_actions().unwrap();
        service.failure_actions_on_non_crash_failures().unwrap();
        service.delayed_auto_start().unwrap();
        service.service_sid_type().unwrap();
        assert!(!service.required_privileges().unwrap().is_empty());
        service.preshutdown_timeout_ms().unwrap();
        service.trigger_info().unwrap();
        let security = service.object_security().unwrap();
        assert!(!security.as_bytes().is_empty());
        assert!(security.as_bytes().len() < 64 * 1024);
    }

    #[test]
    fn a_well_known_service_reports_status_and_config_and_a_missing_one_is_none() {
        let manager = ServiceControlManager::connect(ServiceManagerAccess::Connect).unwrap();
        let service = manager
            .open("EventLog", ServiceAccess::QueryStatusAndConfig)
            .unwrap()
            .expect("the Windows Event Log service exists");
        let status = service.status().unwrap();
        assert!(matches!(
            status.state,
            ServiceState::Running | ServiceState::Stopped | ServiceState::StartPending
        ));
        let config = service.config().unwrap();
        assert!(config.image_path.to_ascii_lowercase().contains("svchost"));
        assert!(!config.display_name.is_empty());
        assert!(manager
            .open(
                "mactype-platform-missing-service",
                ServiceAccess::QueryStatus
            )
            .unwrap()
            .is_none());
    }

    #[test]
    fn lanman_workstation_reports_all_of_its_dependencies() {
        let manager = ServiceControlManager::connect(ServiceManagerAccess::Connect).unwrap();
        let service = manager
            .open("LanmanWorkstation", ServiceAccess::QueryConfig)
            .unwrap()
            .expect("the Workstation service exists");
        let config = service.config().unwrap();

        assert!(config.dependencies.len() >= 2);
        assert!(config
            .dependencies
            .iter()
            .any(|dependency| dependency.eq_ignore_ascii_case("NSI")));
    }
}
