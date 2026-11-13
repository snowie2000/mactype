use std::path::{Path, PathBuf};

use mactype_service_contract::{
    MachinePaths, RuntimeGenerationPointer, StructuredServiceError, valid_runtime_version_component,
};

use crate::protected_path::{MAX_POINTER_BYTES, has_reparse_ancestor, read_bounded_regular_file};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActiveRuntimeGeneration {
    root: PathBuf,
    version: String,
    pointer: RuntimeGenerationPointer,
}

impl ActiveRuntimeGeneration {
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn version(&self) -> &str {
        &self.version
    }

    pub(crate) const fn pointer(&self) -> &RuntimeGenerationPointer {
        &self.pointer
    }
}

pub(crate) fn resolve(
    paths: &MachinePaths,
) -> Result<ActiveRuntimeGeneration, StructuredServiceError> {
    let pointer_path = paths.runtime_pointer();
    reject_reparse(pointer_path)?;
    let bytes = read_bounded_regular_file(pointer_path, MAX_POINTER_BYTES).map_err(|error| {
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
    let pointer = RuntimeGenerationPointer::parse(&bytes).map_err(|_| {
        service_error(
            "active-runtime-invalid",
            "the protected active runtime pointer has an unsupported value",
            None,
        )
    })?;
    let version = pointer.version().to_owned();
    if !valid_runtime_version_component(&version) {
        return Err(service_error(
            "active-runtime-invalid",
            "the protected active runtime pointer has an unsupported value",
            None,
        ));
    }

    let root = paths.runtime_versions().join(&version);
    reject_reparse(&root)?;
    if !root.is_dir() {
        return Err(service_error(
            "active-runtime-unavailable",
            "the protected active runtime generation is missing",
            None,
        ));
    }

    Ok(ActiveRuntimeGeneration {
        root,
        version,
        pointer,
    })
}

pub(crate) fn reject_reparse(path: &Path) -> Result<(), StructuredServiceError> {
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
    use std::fs;
    use std::path::Path;

    use mactype_service_contract::MachinePaths;

    use super::resolve;

    #[derive(Clone, Copy, Debug)]
    enum ResolveCase {
        MissingPointer,
        ReparsePointer,
        InvalidVersion,
        MissingDirectory,
        Valid,
    }

    #[test]
    #[cfg_attr(
        all(miri, windows),
        ignore = "Windows Miri does not implement CreateDirectoryW"
    )]
    fn active_runtime_resolution_maps_pointer_and_directory_outcomes() {
        let cases = [
            (
                ResolveCase::MissingPointer,
                Some("active-runtime-unavailable"),
            ),
            (ResolveCase::ReparsePointer, Some("active-runtime-reparse")),
            (ResolveCase::InvalidVersion, Some("active-runtime-invalid")),
            (
                ResolveCase::MissingDirectory,
                Some("active-runtime-unavailable"),
            ),
            (ResolveCase::Valid, None),
        ];

        for (case, expected_code) in cases {
            let fixture = Fixture::new();
            fixture.arrange(case);
            let result = resolve(&fixture.paths);

            match expected_code {
                Some(expected_code) => {
                    let error = result.unwrap_err();
                    assert_eq!(error.code, expected_code, "{case:?}");
                    if matches!(case, ResolveCase::MissingPointer) {
                        assert!(error.win32_error.is_some(), "{case:?}");
                    }
                }
                None => {
                    let generation = result.unwrap();
                    assert_eq!(
                        generation.root(),
                        fixture.paths.runtime_versions().join("0.2.0")
                    );
                    assert_eq!(generation.version(), "0.2.0");
                    assert_eq!(generation.pointer().version(), "0.2.0");
                }
            }
        }
    }

    struct Fixture {
        _root: tempfile::TempDir,
        paths: MachinePaths,
    }

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
            let program_files = root.path().join("Program Files");
            let program_data = root.path().join("ProgramData");
            fs::create_dir_all(&program_files).unwrap();
            fs::create_dir_all(&program_data).unwrap();
            let paths = MachinePaths::from_trusted_os_roots(&program_files, &program_data).unwrap();
            Self { _root: root, paths }
        }

        fn arrange(&self, case: ResolveCase) {
            match case {
                ResolveCase::MissingPointer => {}
                ResolveCase::ReparsePointer => {
                    let target = self._root.path().join("reparse-target");
                    fs::create_dir_all(&target).unwrap();
                    fs::write(
                        target.join("current.json"),
                        br#"{"schema":1,"version":"0.2.0"}"#,
                    )
                    .unwrap();
                    fs::create_dir_all(self.paths.service_root().parent().unwrap()).unwrap();
                    create_directory_link(&target, self.paths.service_root());
                }
                ResolveCase::InvalidVersion => self.write_pointer(".."),
                ResolveCase::MissingDirectory => self.write_pointer("0.2.0"),
                ResolveCase::Valid => {
                    self.write_pointer("0.2.0");
                    fs::create_dir_all(self.paths.runtime_versions().join("0.2.0")).unwrap();
                }
            }
        }

        fn write_pointer(&self, version: &str) {
            fs::create_dir_all(self.paths.runtime_pointer().parent().unwrap()).unwrap();
            fs::write(
                self.paths.runtime_pointer(),
                format!(r#"{{"schema":1,"version":"{version}"}}"#),
            )
            .unwrap();
        }
    }

    #[cfg(windows)]
    fn create_directory_link(target: &Path, link: &Path) {
        let status = std::process::Command::new("cmd")
            .arg("/c")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[cfg(unix)]
    fn create_directory_link(target: &Path, link: &Path) {
        std::os::unix::fs::symlink(target, link).unwrap();
    }
}
