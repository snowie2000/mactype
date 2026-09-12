use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use mactype_service_contract::{
    runtime_generation_id, MachinePaths, StructuredServiceError, IMMUTABLE_RUNTIME_FILES,
    MAX_PROFILE_BYTES, MAX_RUNTIME_FILE_BYTES,
};

use crate::protected_path::{
    has_reparse_ancestor, read_bounded_regular_file, runtime_pointer_version, MAX_POINTER_BYTES,
};

const REQUIRED_RUNTIME_FILES: [&str; 6] = [
    "mactype-service.exe",
    "mactype-injector32.exe",
    "mactype-injector64.exe",
    "MacType.dll",
    "MacType64.dll",
    "MacType.ini",
];

pub const RUNTIME_PROFILE_ABSENT_CODE: &str = "runtime-profile-absent";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedRuntimeAssets {
    root: PathBuf,
    injector32: PathBuf,
    injector64: PathBuf,
    generation_id: String,
}

impl ProtectedRuntimeAssets {
    pub fn load(paths: MachinePaths) -> Result<Self, StructuredServiceError> {
        let pointer = paths.runtime_pointer();
        reject_reparse(pointer)?;
        let bytes = read_bounded_regular_file(pointer, MAX_POINTER_BYTES).map_err(|error| {
            if error.kind() == std::io::ErrorKind::InvalidData {
                service_error(
                    "active-runtime-invalid",
                    "the protected active runtime pointer is not a bounded regular file",
                    error.raw_os_error(),
                )
            } else {
                service_error(
                    "active-runtime-unavailable",
                    "the protected active runtime pointer could not be read",
                    error.raw_os_error(),
                )
            }
        })?;
        let version = runtime_pointer_version(&bytes).ok_or_else(|| {
            service_error(
                "active-runtime-invalid",
                "the protected active runtime pointer has an unsupported value",
                None,
            )
        })?;

        let root = paths.runtime_versions().join(version);
        reject_reparse(&root)?;
        if !root.is_dir() {
            return Err(service_error(
                "active-runtime-unavailable",
                "the protected active runtime generation is missing",
                None,
            ));
        }

        validate_runtime_file_set(&root)?;

        let mut immutable_files = BTreeMap::new();
        for name in REQUIRED_RUNTIME_FILES {
            let file = root.join(name);
            reject_reparse(&file)?;
            let maximum_bytes = if name == "MacType.ini" {
                MAX_PROFILE_BYTES
            } else {
                MAX_RUNTIME_FILE_BYTES
            };
            let bytes = read_bounded_regular_file(&file, maximum_bytes as u64).map_err(|error| {
                match error.kind() {
                    std::io::ErrorKind::NotFound => service_error(
                        "runtime-component-missing",
                        "a required protected runtime component is missing",
                        error.raw_os_error(),
                    ),
                    std::io::ErrorKind::InvalidData => service_error(
                        "runtime-component-invalid",
                        "a required protected runtime component is not a bounded regular non-empty file",
                        error.raw_os_error(),
                    ),
                    _ => service_error(
                        "runtime-component-inaccessible",
                        "a required protected runtime component could not be read",
                        error.raw_os_error(),
                    ),
                }
            })?;
            if IMMUTABLE_RUNTIME_FILES.contains(&name) {
                immutable_files.insert(name.to_owned(), bytes);
            }
        }
        let generation_id = runtime_generation_id(&immutable_files).map_err(|_| {
            service_error(
                "runtime-generation-invalid",
                "the protected runtime generation identity could not be calculated",
                None,
            )
        })?;

        Ok(Self {
            injector32: root.join("mactype-injector32.exe"),
            injector64: root.join("mactype-injector64.exe"),
            root,
            generation_id,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn injector32(&self) -> &Path {
        &self.injector32
    }

    pub fn injector64(&self) -> &Path {
        &self.injector64
    }

    pub fn generation_id(&self) -> &str {
        &self.generation_id
    }
}

pub(crate) fn validate_runtime_file_set(root: &Path) -> Result<(), StructuredServiceError> {
    let names = fs::read_dir(root)
        .map_err(|error| {
            service_error(
                "runtime-file-set-invalid",
                "the protected runtime file set could not be enumerated",
                error.raw_os_error(),
            )
        })?
        .map(|entry| {
            let entry = entry.map_err(|error| {
                service_error(
                    "runtime-file-set-invalid",
                    "the protected runtime file set could not be enumerated",
                    error.raw_os_error(),
                )
            })?;
            Ok(entry.file_name())
        });
    validate_runtime_file_names(names)
}

fn validate_runtime_file_names(
    names: impl IntoIterator<Item = Result<OsString, StructuredServiceError>>,
) -> Result<(), StructuredServiceError> {
    let mut count = 0;
    let mut actual = BTreeSet::new();
    for name in names
        .into_iter()
        .take(REQUIRED_RUNTIME_FILES.len().saturating_add(1))
    {
        count += 1;
        actual.insert(name?.into_string().map_err(|_| {
            service_error(
                "runtime-file-set-invalid",
                "the protected runtime contains a non-Unicode file name",
                None,
            )
        })?);
    }

    if count == REQUIRED_RUNTIME_FILES.len() - 1
        && actual.len() == REQUIRED_RUNTIME_FILES.len() - 1
        && REQUIRED_RUNTIME_FILES
            .iter()
            .filter(|name| **name != "MacType.ini")
            .all(|name| actual.contains(*name))
    {
        return Err(service_error(
            RUNTIME_PROFILE_ABSENT_CODE,
            "the protected runtime has no generated profile, so the service stays stopped until setup publishes one",
            None,
        ));
    }

    if count != REQUIRED_RUNTIME_FILES.len()
        || actual.len() != REQUIRED_RUNTIME_FILES.len()
        || REQUIRED_RUNTIME_FILES
            .iter()
            .any(|name| !actual.contains(*name))
    {
        return Err(service_error(
            "runtime-file-set-invalid",
            "the protected runtime must contain exactly the fixed service, helpers, DLLs, and generated profile",
            None,
        ));
    }
    Ok(())
}

fn reject_reparse(path: &Path) -> Result<(), StructuredServiceError> {
    if has_reparse_ancestor(path).map_err(|error| {
        service_error(
            "active-runtime-inaccessible",
            "the protected runtime path could not be inspected",
            error.raw_os_error(),
        )
    })? {
        return Err(service_error(
            "active-runtime-reparse",
            "reparse points are forbidden in the protected runtime path",
            None,
        ));
    }
    Ok(())
}

fn service_error(code: &str, message: &str, win32_error: Option<i32>) -> StructuredServiceError {
    StructuredServiceError {
        code: code.to_owned(),
        message: message.to_owned(),
        win32_error: win32_error.map(|code| code as u32),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{validate_runtime_file_names, REQUIRED_RUNTIME_FILES, RUNTIME_PROFILE_ABSENT_CODE};

    fn validate(names: &[&str]) -> Result<(), mactype_service_contract::StructuredServiceError> {
        validate_runtime_file_names(names.iter().map(|name| Ok(OsString::from(name))))
    }

    #[test]
    fn exact_non_profile_runtime_files_report_the_generated_profile_absent() {
        let error = validate(&REQUIRED_RUNTIME_FILES[..REQUIRED_RUNTIME_FILES.len() - 1])
            .expect_err("the supported stopped runtime has no generated profile");

        assert_eq!(error.code, RUNTIME_PROFILE_ABSENT_CODE);
    }

    #[test]
    fn non_profile_runtime_files_with_a_foreign_file_are_invalid() {
        let mut names = REQUIRED_RUNTIME_FILES[..REQUIRED_RUNTIME_FILES.len() - 1].to_vec();
        names.push("foreign.dll");

        let error = validate(&names).expect_err("a foreign runtime file must remain invalid");

        assert_eq!(error.code, "runtime-file-set-invalid");
    }

    #[test]
    fn runtime_files_missing_the_profile_and_a_dll_are_invalid() {
        let names = [
            "mactype-service.exe",
            "mactype-injector32.exe",
            "mactype-injector64.exe",
            "MacType.dll",
        ];

        let error = validate(&names).expect_err("two missing runtime files must remain invalid");

        assert_eq!(error.code, "runtime-file-set-invalid");
    }

    #[test]
    fn exact_required_runtime_files_pass_validation() {
        validate(&REQUIRED_RUNTIME_FILES).unwrap();
    }

    #[test]
    fn runtime_file_set_validation_stops_at_the_first_excess_entry() {
        let names = REQUIRED_RUNTIME_FILES
            .into_iter()
            .map(|name| Ok(OsString::from(name)))
            .chain(std::iter::once(Ok(OsString::from("foreign.dll"))))
            .chain(std::iter::once_with(|| {
                panic!("validation enumerated beyond the fixed file-count bound")
            }));

        let error = validate_runtime_file_names(names).unwrap_err();

        assert_eq!(error.code, "runtime-file-set-invalid");
    }

    #[test]
    fn runtime_file_set_validation_rejects_a_non_unicode_entry() {
        let names = REQUIRED_RUNTIME_FILES[..REQUIRED_RUNTIME_FILES.len() - 1]
            .iter()
            .map(|name| Ok(OsString::from(name)))
            .chain(std::iter::once(Ok(non_unicode_name())));

        let error = validate_runtime_file_names(names).unwrap_err();

        assert_eq!(error.code, "runtime-file-set-invalid");
        assert!(error.message.contains("non-Unicode"));
    }

    #[cfg(windows)]
    fn non_unicode_name() -> OsString {
        use std::os::windows::ffi::OsStringExt;

        OsString::from_wide(&[0xD800])
    }

    #[cfg(unix)]
    fn non_unicode_name() -> OsString {
        use std::os::unix::ffi::OsStringExt;

        OsString::from_vec(vec![0xFF])
    }
}
