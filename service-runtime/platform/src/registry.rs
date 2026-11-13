//! Read-only registry access with bounded buffers.

use std::io;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_READ, KEY_SET_VALUE,
    KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_DWORD, REG_EXPAND_SZ, REG_SZ, RegCloseKey,
    RegDeleteValueW, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};

use crate::bounded_read::{CallOutcome, RetryError, retry_bytes};
use crate::wide::wide_null;

const MAX_VALUE_NAME_UNITS: usize = 16_384;
const MAX_VALUE_BYTES: usize = 1_048_576;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryRoot {
    LocalMachine,
    CurrentUser,
}

impl RegistryRoot {
    fn raw(self) -> HKEY {
        match self {
            Self::LocalMachine => HKEY_LOCAL_MACHINE,
            Self::CurrentUser => HKEY_CURRENT_USER,
        }
    }
}

/// The registry view (WOW64 redirection) to open.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryView {
    Native32,
    Native64,
}

impl RegistryView {
    fn flag(self) -> u32 {
        match self {
            Self::Native32 => KEY_WOW64_32KEY,
            Self::Native64 => KEY_WOW64_64KEY,
        }
    }
}

/// The data of an enumerated value, typed by the registry's own type tag.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryValueData {
    /// `REG_SZ` or `REG_EXPAND_SZ`, unexpanded, with trailing NULs removed.
    /// `None` when the bytes were not valid UTF-16.
    String(Option<String>),
    Dword(u32),
    Other {
        kind: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryValue {
    pub name: String,
    pub data: RegistryValueData,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawRegistryValue {
    pub name: String,
    pub kind: u32,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeleteValueOutcome {
    Deleted,
    Absent,
}

/// An open registry key with read access only.
#[derive(Debug)]
pub struct RegistryKey(HKEY);

impl RegistryKey {
    /// Opens `path` under `root` in `view`. `Ok(None)` when the key does not
    /// exist; any other failure carries the Win32 status.
    pub fn open(root: RegistryRoot, path: &str, view: RegistryView) -> io::Result<Option<Self>> {
        let path = wide_null(path);
        let mut key = null_mut();
        // SAFETY: `path` is NUL-terminated and `key` is a local out pointer.
        let status = unsafe {
            RegOpenKeyExW(
                root.raw(),
                path.as_ptr(),
                0,
                KEY_READ | view.flag(),
                &mut key,
            )
        };
        if matches!(status, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
            return Ok(None);
        }
        if status != ERROR_SUCCESS || key.is_null() {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        Ok(Some(Self(key)))
    }

    /// Opens an existing key for value queries and writes.
    pub fn open_writable(
        root: RegistryRoot,
        path: &str,
        view: RegistryView,
    ) -> io::Result<Option<Self>> {
        let path = checked_wide(path, "registry path")?;
        let mut key = null_mut();
        // SAFETY: `path` is NUL-terminated and `key` is a local out pointer.
        let status = unsafe {
            RegOpenKeyExW(
                root.raw(),
                path.as_ptr(),
                0,
                KEY_QUERY_VALUE | KEY_SET_VALUE | view.flag(),
                &mut key,
            )
        };
        if matches!(status, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
            return Ok(None);
        }
        if status != ERROR_SUCCESS || key.is_null() {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        Ok(Some(Self(key)))
    }

    /// Reads a `REG_DWORD` value. `Ok(None)` when the value does not exist; a
    /// value of another type or size is an `InvalidData` error.
    pub fn read_dword(&self, name: &str) -> io::Result<Option<u32>> {
        let name = wide_null(name);
        let mut kind = 0;
        let mut value = 0_u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: the key is open; `name` is NUL-terminated; the data pointer
        // and `size` describe exactly the local `u32`.
        let status = unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                null(),
                &mut kind,
                (&mut value as *mut u32).cast::<u8>(),
                &mut size,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        if kind != REG_DWORD || size != std::mem::size_of::<u32>() as u32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "registry value is not a DWORD",
            ));
        }
        Ok(Some(value))
    }

    /// Reads a `REG_SZ` or `REG_EXPAND_SZ` value as text without expansion.
    /// The first NUL terminates the result; malformed UTF-16 is replaced.
    pub fn read_string(&self, name: &str) -> io::Result<Option<String>> {
        Ok(self.read_string_units(name)?.map(|mut units| {
            units.truncate(
                units
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(units.len()),
            );
            String::from_utf16_lossy(&units)
        }))
    }

    /// Reads a `REG_SZ` or `REG_EXPAND_SZ` value as raw UTF-16 units without
    /// expansion. `Ok(None)` when the value does not exist. This is the string
    /// accessor that keeps embedded and trailing NULs. The result is bounded to
    /// one megabyte so the caller can decide how strictly to parse it.
    pub fn read_string_units(&self, name: &str) -> io::Result<Option<Vec<u16>>> {
        let name = wide_null(name);
        let mut kind = 0;
        let mut size = 0;
        // SAFETY: the key is open; `name` is NUL-terminated; a null data
        // pointer asks only for the size, which lands in the local `size`.
        let status = unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                null(),
                &mut kind,
                null_mut(),
                &mut size,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        if !matches!(kind, REG_SZ | REG_EXPAND_SZ)
            || size == 0
            || size % 2 != 0
            || size as usize > MAX_VALUE_BYTES
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "registry value is not a bounded string",
            ));
        }
        let mut value = vec![0_u16; size as usize / 2];
        let mut actual_kind = 0;
        let mut actual_size = size;
        // SAFETY: the key is open; `value` is writable for `actual_size` bytes,
        // which is the capacity passed alongside it.
        let status = unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                null(),
                &mut actual_kind,
                value.as_mut_ptr().cast::<u8>(),
                &mut actual_size,
            )
        };
        if status != ERROR_SUCCESS
            || actual_kind != kind
            || actual_size == 0
            || actual_size % 2 != 0
            || actual_size > size
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "registry value changed while it was being read",
            ));
        }
        value.truncate(actual_size as usize / 2);
        Ok(Some(value))
    }

    /// Reads a value without interpreting its registry type or bytes.
    pub fn read_raw(
        &self,
        name: &str,
        maximum_bytes: usize,
    ) -> io::Result<Option<RawRegistryValue>> {
        let wide_name = checked_wide(name, "registry value name")?;
        let mut kind = 0_u32;
        let mut first_call = true;
        let mut missing = false;
        let bytes = read_raw_bytes(maximum_bytes, |buffer, capacity, needed| {
            let is_first_call = first_call;
            first_call = false;
            // SAFETY: the key and NUL-terminated name are live; bounded_read
            // supplies a null pointer only for zero capacity and otherwise a
            // writable allocation described by `capacity`; outputs are local.
            let status = unsafe {
                RegQueryValueExW(
                    self.0,
                    wide_name.as_ptr(),
                    null(),
                    &mut kind,
                    buffer,
                    needed,
                )
            };
            match status {
                ERROR_FILE_NOT_FOUND if is_first_call => {
                    missing = true;
                    *needed = 0;
                    Ok(CallOutcome::Complete)
                }
                ERROR_FILE_NOT_FOUND => Err(io::Error::from_raw_os_error(status as i32)),
                ERROR_SUCCESS if capacity == 0 && *needed > 0 => Ok(CallOutcome::MoreData(
                    io::Error::from_raw_os_error(ERROR_MORE_DATA as i32),
                )),
                ERROR_SUCCESS => Ok(CallOutcome::Complete),
                ERROR_MORE_DATA => Ok(CallOutcome::MoreData(io::Error::from_raw_os_error(
                    status as i32,
                ))),
                other => Err(io::Error::from_raw_os_error(other as i32)),
            }
        })?;
        if missing {
            return Ok(None);
        }
        Ok(Some(RawRegistryValue {
            name: name.to_owned(),
            kind,
            bytes,
        }))
    }

    /// Enumerates every raw value within caller-supplied fixed bounds.
    pub fn values_raw(
        &self,
        maximum_name_units: usize,
        maximum_value_bytes: usize,
    ) -> io::Result<Vec<RawRegistryValue>> {
        if maximum_name_units == 0
            || maximum_name_units > u32::MAX as usize
            || maximum_value_bytes > u32::MAX as usize
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "registry enumeration bound is out of range",
            ));
        }
        // RegEnumValueW's input capacity includes space for the terminating NUL.
        let mut name = vec![0_u16; maximum_name_units];
        let mut values = Vec::new();
        for index in 0.. {
            let mut name_length = 0_u32;
            let mut kind = 0_u32;
            let mut no_more_items = false;
            let data =
                read_enumerated_raw_bytes(maximum_value_bytes, |buffer, capacity, data_length| {
                    name_length = maximum_name_units as u32;
                    // SAFETY: the key and name allocation are live;
                    // bounded_read supplies a null data pointer only for zero
                    // capacity and otherwise a writable allocation described
                    // by `capacity`; all output lengths are live locals.
                    let status = unsafe {
                        RegEnumValueW(
                            self.0,
                            index,
                            name.as_mut_ptr(),
                            &mut name_length,
                            null(),
                            &mut kind,
                            buffer,
                            data_length,
                        )
                    };
                    match status {
                        ERROR_NO_MORE_ITEMS => {
                            no_more_items = true;
                            *data_length = 0;
                            Ok(CallOutcome::Complete)
                        }
                        ERROR_SUCCESS if capacity == 0 && *data_length > 0 => {
                            Ok(CallOutcome::MoreData(io::Error::from_raw_os_error(
                                ERROR_MORE_DATA as i32,
                            )))
                        }
                        ERROR_SUCCESS => Ok(CallOutcome::Complete),
                        ERROR_MORE_DATA if *data_length <= capacity => Err(bound_error()),
                        ERROR_MORE_DATA => Ok(CallOutcome::MoreData(io::Error::from_raw_os_error(
                            status as i32,
                        ))),
                        other => Err(io::Error::from_raw_os_error(other as i32)),
                    }
                })?;
            if no_more_items {
                break;
            }
            if name_length as usize >= maximum_name_units {
                return Err(bound_error());
            }
            let value_name = String::from_utf16(&name[..name_length as usize]).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "registry value name is not valid UTF-16",
                )
            })?;
            values.push(RawRegistryValue {
                name: value_name,
                kind,
                bytes: data,
            });
        }
        Ok(values)
    }

    pub fn set_raw(&self, name: &str, kind: u32, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() > u32::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "registry value is too large",
            ));
        }
        let name = checked_wide(name, "registry value name")?;
        // SAFETY: the key is live, `name` is NUL-terminated, and `bytes` remains
        // readable for the exact byte count passed.
        let status = unsafe {
            RegSetValueExW(
                self.0,
                name.as_ptr(),
                0,
                kind,
                bytes.as_ptr(),
                bytes.len() as u32,
            )
        };
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        Ok(())
    }

    pub fn set_string(&self, name: &str, value: &str) -> io::Result<()> {
        let value = checked_wide(value, "registry string")?;
        let bytes: Vec<u8> = value.iter().flat_map(|unit| unit.to_le_bytes()).collect();
        self.set_raw(name, REG_SZ, &bytes)
    }

    pub fn delete_value(&self, name: &str) -> io::Result<DeleteValueOutcome> {
        let name = checked_wide(name, "registry value name")?;
        // SAFETY: the key is live and `name` is NUL-terminated.
        let status = unsafe { RegDeleteValueW(self.0, name.as_ptr()) };
        match status {
            ERROR_SUCCESS => Ok(DeleteValueOutcome::Deleted),
            ERROR_FILE_NOT_FOUND => Ok(DeleteValueOutcome::Absent),
            other => Err(io::Error::from_raw_os_error(other as i32)),
        }
    }

    /// Enumerates every value of the key. A name or datum outside the fixed
    /// bounds ends the enumeration with an error rather than a partial list.
    pub fn values(&self) -> io::Result<Vec<RegistryValue>> {
        let mut values = Vec::new();
        let mut name = vec![0_u16; MAX_VALUE_NAME_UNITS + 1];
        for index in 0.. {
            let mut name_length = 0_u32;
            let mut kind = 0_u32;
            let mut no_more_items = false;
            let bytes = read_enumerated_bytes(|buffer, capacity, data_length| {
                name_length = MAX_VALUE_NAME_UNITS as u32;
                // SAFETY: the key and name allocation are live; bounded_read
                // supplies a null data pointer only for zero capacity and
                // otherwise a writable allocation described by `capacity`;
                // all output lengths are live locals.
                let status = unsafe {
                    RegEnumValueW(
                        self.0,
                        index,
                        name.as_mut_ptr(),
                        &mut name_length,
                        null(),
                        &mut kind,
                        buffer,
                        data_length,
                    )
                };
                match status {
                    ERROR_NO_MORE_ITEMS => {
                        no_more_items = true;
                        *data_length = 0;
                        Ok(CallOutcome::Complete)
                    }
                    ERROR_SUCCESS if capacity == 0 && *data_length > 0 => Ok(
                        CallOutcome::MoreData(io::Error::from_raw_os_error(ERROR_MORE_DATA as i32)),
                    ),
                    ERROR_SUCCESS => Ok(CallOutcome::Complete),
                    ERROR_MORE_DATA if *data_length <= capacity => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "registry value exceeds the fixed bound",
                    )),
                    ERROR_MORE_DATA => Ok(CallOutcome::MoreData(io::Error::from_raw_os_error(
                        status as i32,
                    ))),
                    other => Err(io::Error::from_raw_os_error(other as i32)),
                }
            })?;
            if no_more_items {
                break;
            }
            if name_length as usize > MAX_VALUE_NAME_UNITS {
                return Err(io::Error::from_raw_os_error(ERROR_SUCCESS as i32));
            }
            let name = String::from_utf16(&name[..name_length as usize]).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "registry value name is not UTF-16",
                )
            })?;
            let data = match kind {
                REG_SZ | REG_EXPAND_SZ => RegistryValueData::String(decode_string(&bytes)),
                REG_DWORD if bytes.len() == 4 => RegistryValueData::Dword(u32::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3],
                ])),
                other => RegistryValueData::Other { kind: other },
            };
            values.push(RegistryValue { name, data });
        }
        Ok(values)
    }
}

impl Drop for RegistryKey {
    fn drop(&mut self) {
        // SAFETY: the key was opened by `RegOpenKeyExW` and is closed once.
        unsafe { RegCloseKey(self.0) };
    }
}

fn read_raw_bytes(
    maximum_bytes: usize,
    call: impl FnMut(*mut u8, u32, &mut u32) -> io::Result<CallOutcome>,
) -> io::Result<Vec<u8>> {
    retry_bytes(0, maximum_bytes, call).map_err(|error| match error {
        RetryError::Call(error) => error,
        RetryError::LimitExceeded | RetryError::ReturnedLengthExceedsLimit => bound_error(),
        RetryError::ReturnedLengthExceedsCapacity => io::Error::new(
            io::ErrorKind::InvalidData,
            "registry value changed while it was being read",
        ),
    })
}

fn read_enumerated_raw_bytes(
    maximum_bytes: usize,
    call: impl FnMut(*mut u8, u32, &mut u32) -> io::Result<CallOutcome>,
) -> io::Result<Vec<u8>> {
    retry_bytes(0, maximum_bytes, call).map_err(|error| match error {
        RetryError::Call(error) => error,
        RetryError::LimitExceeded
        | RetryError::ReturnedLengthExceedsCapacity
        | RetryError::ReturnedLengthExceedsLimit => bound_error(),
    })
}

fn read_enumerated_bytes(
    call: impl FnMut(*mut u8, u32, &mut u32) -> io::Result<CallOutcome>,
) -> io::Result<Vec<u8>> {
    retry_bytes(0, MAX_VALUE_BYTES, call).map_err(|error| match error {
        RetryError::Call(error) => error,
        RetryError::LimitExceeded => io::Error::new(
            io::ErrorKind::InvalidData,
            "registry value exceeds the fixed bound",
        ),
        RetryError::ReturnedLengthExceedsCapacity | RetryError::ReturnedLengthExceedsLimit => {
            io::Error::from_raw_os_error(ERROR_SUCCESS as i32)
        }
    })
}

fn checked_wide(value: &str, field: &'static str) -> io::Result<Vec<u16>> {
    if value.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{field} contains a NUL"),
        ));
    }
    Ok(wide_null(value))
}

fn bound_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "registry value exceeds the bound",
    )
}

fn decode_string(bytes: &[u8]) -> Option<String> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let mut units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    while units.last() == Some(&0) {
        units.pop();
    }
    String::from_utf16(&units).ok()
}

#[cfg(test)]
mod tests {
    use std::ptr::null_mut;
    use std::time::{SystemTime, UNIX_EPOCH};

    use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_BINARY, REG_OPTION_NON_VOLATILE,
        RegCreateKeyExW, RegDeleteKeyW,
    };

    use super::{
        DeleteValueOutcome, MAX_VALUE_BYTES, RegistryKey, RegistryRoot, RegistryValueData,
        RegistryView, decode_string, read_enumerated_bytes, read_enumerated_raw_bytes,
        read_raw_bytes,
    };
    use crate::bounded_read::CallOutcome;
    use crate::wide::wide_null;

    #[test]
    fn the_windows_version_key_reads_through_every_accessor() {
        let key = RegistryKey::open(
            RegistryRoot::LocalMachine,
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            RegistryView::Native64,
        )
        .unwrap()
        .expect("the CurrentVersion key always exists");
        let product = key.read_string_units("ProductName").unwrap().unwrap();
        assert!(!product.is_empty());
        assert!(!key.read_string("ProductName").unwrap().unwrap().is_empty());
        assert!(
            key.read_dword("CurrentMajorVersionNumber")
                .unwrap()
                .is_some()
        );
        assert!(key.read_dword("ProductName").is_err());
        assert_eq!(key.read_dword("mactype-platform-missing").unwrap(), None);
        let values = key.values().unwrap();
        assert!(values.iter().any(|value| {
            value.name == "ProductName" && matches!(value.data, RegistryValueData::String(Some(_)))
        }));
    }

    #[test]
    fn a_missing_key_is_none_not_an_error() {
        assert!(
            RegistryKey::open(
                RegistryRoot::CurrentUser,
                r"SOFTWARE\mactype-platform-missing-key",
                RegistryView::Native32,
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn registry_strings_drop_trailing_nuls_and_reject_odd_lengths() {
        let mut bytes = Vec::new();
        for unit in "run".encode_utf16().chain([0, 0]) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(decode_string(&bytes).as_deref(), Some("run"));
        assert_eq!(decode_string(&bytes[..5]), None);
    }

    #[test]
    fn raw_value_reader_enforces_its_byte_cap() {
        let error = read_raw_bytes(4, |_, _, needed| {
            *needed = 5;
            Ok(CallOutcome::MoreData(std::io::Error::from_raw_os_error(
                ERROR_MORE_DATA as i32,
            )))
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "registry value exceeds the bound");
    }

    #[test]
    fn raw_value_enumerator_enforces_its_byte_cap() {
        let error = read_enumerated_raw_bytes(4, |_, _, needed| {
            *needed = 5;
            Ok(CallOutcome::MoreData(std::io::Error::from_raw_os_error(
                ERROR_MORE_DATA as i32,
            )))
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "registry value exceeds the bound");
    }

    #[test]
    fn fixed_value_enumerator_enforces_its_byte_cap() {
        let error = read_enumerated_bytes(|_, _, needed| {
            *needed = (MAX_VALUE_BYTES + 1) as u32;
            Ok(CallOutcome::MoreData(std::io::Error::from_raw_os_error(
                ERROR_MORE_DATA as i32,
            )))
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "registry value exceeds the fixed bound");
    }

    #[test]
    fn writable_key_raw_values_round_trip_with_bounds_and_deletion() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = format!(
            r"Software\MacType\platform-tests\{}-{nonce}",
            std::process::id()
        );
        let wide_path = wide_null(&path);
        let mut raw = null_mut();
        let mut disposition = 0_u32;
        // SAFETY: the path is NUL-terminated and both outputs are live locals.
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                wide_path.as_ptr(),
                0,
                null_mut(),
                REG_OPTION_NON_VOLATILE,
                KEY_READ | KEY_WRITE,
                null_mut(),
                &mut raw,
                &mut disposition,
            )
        };
        assert_eq!(status, ERROR_SUCCESS);
        // SAFETY: RegCreateKeyExW returned a newly owned non-null handle.
        let created = RegistryKey(raw);
        drop(created);

        let key =
            RegistryKey::open_writable(RegistryRoot::CurrentUser, &path, RegistryView::Native64)
                .unwrap()
                .unwrap();
        key.set_string("text", "hello").unwrap();
        key.set_raw("bytes", REG_BINARY, &[1, 2, 3, 4]).unwrap();
        let raw = key.read_raw("text", 64).unwrap().unwrap();
        assert_eq!(raw.name, "text");
        assert_eq!(raw.kind, super::REG_SZ);
        assert_eq!(
            String::from_utf16_lossy(
                &raw.bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                    .collect::<Vec<_>>()
            ),
            "hello\0"
        );
        assert_eq!(
            key.read_raw("bytes", 3).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        let values = key.values_raw(64, 64).unwrap();
        assert!(values.iter().any(|value| value.name == "text"));
        assert!(
            values
                .iter()
                .any(|value| value.name == "bytes" && value.bytes == [1, 2, 3, 4])
        );
        assert_eq!(
            key.delete_value("bytes").unwrap(),
            DeleteValueOutcome::Deleted
        );
        assert_eq!(
            key.delete_value("bytes").unwrap(),
            DeleteValueOutcome::Absent
        );
        assert!(key.set_string("bad", "a\0b").is_err());
        drop(key);

        // SAFETY: the path is NUL-terminated and the test closed every key
        // handle before deleting this leaf key.
        let deleted = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, wide_path.as_ptr()) };
        assert_eq!(deleted, ERROR_SUCCESS);
        // Leave no empty parent behind; another test may still own it, in
        // which case the delete fails harmlessly.
        let parent = wide_null(r"Software\MacType\platform-tests");
        // SAFETY: the path is NUL-terminated; a non-empty or missing key makes
        // the call fail without side effects.
        unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, parent.as_ptr()) };
    }
}
