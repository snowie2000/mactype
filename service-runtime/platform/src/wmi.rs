//! WMI access for process creation events.
//!
//! Every COM call on the WMI interfaces is `unsafe` in the `windows` crate
//! because the callee may read caller memory; the wrappers here pass only
//! owned `BSTR`s, local out values, and interface pointers the crate keeps
//! alive, and hand back plain integers or owned objects.

use std::time::Duration;

use windows::core::{IUnknown, Interface, BSTR, PCWSTR};
use windows::Win32::Foundation::RPC_E_TOO_LATE;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeSecurity, CoSetProxyBlanket, CLSCTX_INPROC_SERVER, EOAC_NONE,
    RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::System::Wmi::{
    IEnumWbemClassObject, IWbemClassObject, IWbemLocator, IWbemServices, WbemLocator,
    WBEM_E_ACCESS_DENIED, WBEM_E_INVALID_CLASS, WBEM_FLAG_FORWARD_ONLY,
    WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_S_TIMEDOUT,
};

/// A COM failure, reduced to its `HRESULT`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WmiError {
    pub hresult: i32,
}

impl From<windows::core::Error> for WmiError {
    fn from(error: windows::core::Error) -> Self {
        Self {
            hresult: error.code().0,
        }
    }
}

impl WmiError {
    /// Whether an immediate process-start trace is unavailable and the host
    /// should use its bounded intrinsic creation-event fallback.
    pub fn permits_intrinsic_process_fallback(self) -> bool {
        self.hresult == WBEM_E_ACCESS_DENIED.0 || self.hresult == WBEM_E_INVALID_CLASS.0
    }
}

/// The step of a typed property read that failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WmiPropertyStep {
    /// `IWbemClassObject::Get` failed: the object has no such property.
    Get,
    /// The property is present but does not hold a value of the requested
    /// kind.
    Convert,
    /// The property holds an object, but not a WMI class object.
    Cast,
}

/// A typed property read failure with the step it happened in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WmiPropertyError {
    pub step: WmiPropertyStep,
    pub error: WmiError,
}

impl WmiPropertyError {
    fn at(step: WmiPropertyStep, error: windows::core::Error) -> Self {
        Self {
            step,
            error: error.into(),
        }
    }
}

/// A connection to `ROOT\CIMV2` with the proxy blanket the service needs.
/// Requires a multithreaded COM apartment on the calling thread.
pub struct WmiConnection {
    services: IWbemServices,
}

impl WmiConnection {
    /// Initializes process-wide COM security for the service identity. A
    /// second initialization (`RPC_E_TOO_LATE`) is accepted as already done.
    pub fn initialize_security() -> Result<(), WmiError> {
        // SAFETY: every optional argument is `None`; the integers select the
        // call-level authentication and impersonation the service needs.
        match unsafe {
            CoInitializeSecurity(
                None,
                -1,
                None,
                None,
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
                None,
            )
        } {
            Ok(()) => Ok(()),
            Err(error) if error.code() == RPC_E_TOO_LATE => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    /// Creates the locator, opens `ROOT\CIMV2`, and sets the proxy blanket
    /// in one call. A caller that reports the three steps separately drives
    /// [`WmiLocator`] itself.
    pub fn connect_cimv2() -> Result<Self, WmiError> {
        WmiLocator::create()?.open_cimv2()?.with_service_blanket()
    }

    /// A forward-only event subscription for `wql`.
    pub fn notification_query(&self, wql: &str) -> Result<WmiEnumerator, WmiError> {
        let flags = WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY;
        // SAFETY: both strings are owned BSTRs that outlive the call; the
        // context is `None`.
        let enumerator = unsafe {
            self.services
                .ExecNotificationQuery(&BSTR::from("WQL"), &BSTR::from(wql), flags, None)
        }?;
        Ok(WmiEnumerator { enumerator })
    }

    /// A forward-only instance query for `wql`.
    pub fn query(&self, wql: &str) -> Result<WmiEnumerator, WmiError> {
        let flags = WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY;
        // SAFETY: both strings are owned BSTRs that outlive the call; the
        // context is `None`.
        let enumerator = unsafe {
            self.services
                .ExecQuery(&BSTR::from("WQL"), &BSTR::from(wql), flags, None)
        }?;
        Ok(WmiEnumerator { enumerator })
    }
}

/// The WMI locator: the first of the three steps that make a
/// [`WmiConnection`], kept separate so a caller can tell a locator failure
/// from a namespace or blanket failure.
pub struct WmiLocator {
    locator: IWbemLocator,
}

impl WmiLocator {
    /// Creates the locator. Requires a COM apartment on the calling thread.
    pub fn create() -> Result<Self, WmiError> {
        // SAFETY: the class and interface identifiers are WMI's own
        // constants; no outer unknown is passed.
        let locator: IWbemLocator =
            unsafe { CoCreateInstance(&WbemLocator, None::<&IUnknown>, CLSCTX_INPROC_SERVER) }?;
        Ok(Self { locator })
    }

    /// Opens `ROOT\CIMV2`: the second step. The namespace still needs its
    /// proxy blanket before it can be queried.
    pub fn open_cimv2(&self) -> Result<WmiNamespace, WmiError> {
        let empty = BSTR::new();
        // SAFETY: every string is an owned BSTR that outlives the call and
        // the context is `None`.
        let services = unsafe {
            self.locator.ConnectServer(
                &BSTR::from("ROOT\\CIMV2"),
                &empty,
                &empty,
                &empty,
                0,
                &empty,
                None,
            )
        }?;
        Ok(WmiNamespace { services })
    }
}

/// An open namespace whose proxy blanket is not set yet: the result of the
/// second connection step.
pub struct WmiNamespace {
    services: IWbemServices,
}

impl WmiNamespace {
    /// Sets the call-level, impersonating proxy blanket the service identity
    /// needs: the third step, which completes the connection.
    pub fn with_service_blanket(self) -> Result<WmiConnection, WmiError> {
        // SAFETY: `services` is a live interface; every optional argument is
        // `None` and the integers select the documented blanket.
        unsafe {
            CoSetProxyBlanket(
                &self.services,
                RPC_C_AUTHN_WINNT,
                RPC_C_AUTHZ_NONE,
                None,
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            )
        }?;
        Ok(WmiConnection {
            services: self.services,
        })
    }
}

/// An enumerator over query results or events.
pub struct WmiEnumerator {
    enumerator: IEnumWbemClassObject,
}

impl WmiEnumerator {
    /// The next object, or `None` when the enumeration is exhausted or
    /// `timeout` passed without an event.
    pub fn next(&self, timeout: Duration) -> Result<Option<WmiObject>, WmiError> {
        let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        let mut objects = [None];
        let mut returned = 0;
        // SAFETY: `objects` and `returned` are locals the binding fills; the
        // enumerator is a live interface.
        let result = unsafe {
            self.enumerator
                .Next(timeout_ms, &mut objects, &mut returned)
        };
        if result.0 == WBEM_S_TIMEDOUT.0 || returned == 0 {
            return Ok(None);
        }
        result.ok()?;
        Ok(objects[0].take().map(|object| WmiObject { object }))
    }
}

/// One WMI object.
pub struct WmiObject {
    object: IWbemClassObject,
}

impl WmiObject {
    /// The `ProcessID` property, when present and numeric.
    pub fn process_id(&self) -> Result<u32, WmiError> {
        self.process_id_with_step().map_err(|failure| failure.error)
    }

    /// [`process_id`](Self::process_id), naming whether the property was
    /// missing or not numeric.
    pub fn process_id_with_step(&self) -> Result<u32, WmiPropertyError> {
        let value = self.get(windows::core::w!("ProcessID"))?;
        u32::try_from(&value).map_err(|error| WmiPropertyError::at(WmiPropertyStep::Convert, error))
    }

    /// The `TargetInstance` of an event, as an object.
    pub fn target_instance(&self) -> Result<WmiObject, WmiError> {
        self.target_instance_with_step()
            .map_err(|failure| failure.error)
    }

    /// [`target_instance`](Self::target_instance), naming whether the
    /// property was missing, not an object, or not a WMI class object.
    pub fn target_instance_with_step(&self) -> Result<WmiObject, WmiPropertyError> {
        let value = self.get(windows::core::w!("TargetInstance"))?;
        let unknown = IUnknown::try_from(&value)
            .map_err(|error| WmiPropertyError::at(WmiPropertyStep::Convert, error))?;
        let object: IWbemClassObject = unknown
            .cast()
            .map_err(|error| WmiPropertyError::at(WmiPropertyStep::Cast, error))?;
        Ok(WmiObject { object })
    }

    fn get(&self, name: PCWSTR) -> Result<VARIANT, WmiPropertyError> {
        let mut value = VARIANT::default();
        // SAFETY: `name` is a NUL-terminated static wide string and `value`
        // is a local out variant; the object is a live interface.
        unsafe { self.object.Get(name, 0, &mut value, None, None) }
            .map_err(|error| WmiPropertyError::at(WmiPropertyStep::Get, error))?;
        Ok(value)
    }
}
