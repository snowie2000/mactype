//! File-system calls the standard library does not expose in the form the
//! service needs: attribute reads without following reparse points,
//! write-through replacement, reboot-deferred deletion, and deleting a file
//! through the handle that already verified its contents.

use std::fmt;
use std::fs::{self, File};
use std::io;
use std::os::windows::io::AsRawHandle;
use std::path::{Component, Path, PathBuf};
use std::ptr::null;

use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_INFO, FileDispositionInfo, GetFileAttributesW,
    INVALID_FILE_ATTRIBUTES, MOVEFILE_DELAY_UNTIL_REBOOT, MOVEFILE_REPLACE_EXISTING,
    MOVEFILE_WRITE_THROUGH, MoveFileExW, REPLACEFILE_WRITE_THROUGH, ReplaceFileW,
    SetFileInformationByHandle,
};

use crate::wide::wide_path;

/// The raw attribute bits of `path`, read without following reparse points.
pub fn file_attributes(path: &Path) -> io::Result<u32> {
    let wide = wide_path(path);
    // SAFETY: `wide` is NUL-terminated and outlives the call.
    let attributes = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if attributes == INVALID_FILE_ATTRIBUTES {
        return Err(io::Error::last_os_error());
    }
    Ok(attributes)
}

/// Whether `path` itself is a reparse point (symlink, junction, mount point).
pub fn is_reparse_point(path: &Path) -> io::Result<bool> {
    Ok(file_attributes(path)? & FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

/// Why a candidate path failed component-by-component validation.
#[derive(Debug)]
pub enum PathChainError {
    ReparsePoint(PathBuf),
    UnsafeComponent(PathBuf),
    EscapedRoot { root: PathBuf, candidate: PathBuf },
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for PathChainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReparsePoint(path) => {
                write!(
                    formatter,
                    "path contains a reparse point: {}",
                    path.display()
                )
            }
            Self::UnsafeComponent(path) => {
                write!(
                    formatter,
                    "path contains an unsafe component: {}",
                    path.display()
                )
            }
            Self::EscapedRoot { root, candidate } => write!(
                formatter,
                "path {} escaped trusted root {}",
                candidate.display(),
                root.display()
            ),
            Self::Io { path, source } => {
                write!(formatter, "could not inspect {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for PathChainError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Validates a path chain using a caller-supplied non-following entry probe.
pub fn validate_path_chain_with(
    candidate: &Path,
    trusted_root: Option<&Path>,
    mut probe: impl FnMut(&Path) -> io::Result<Option<bool>>,
) -> Result<(), PathChainError> {
    let paths = if let Some(root) = trusted_root {
        let relative = candidate
            .strip_prefix(root)
            .map_err(|_| PathChainError::EscapedRoot {
                root: root.to_path_buf(),
                candidate: candidate.to_path_buf(),
            })?;
        let mut current = root.to_path_buf();
        let mut paths = vec![current.clone()];
        for component in relative.components() {
            let Component::Normal(component) = component else {
                return Err(PathChainError::UnsafeComponent(candidate.to_path_buf()));
            };
            current.push(component);
            paths.push(current.clone());
        }
        paths
    } else {
        if !candidate.is_absolute() {
            return Err(PathChainError::UnsafeComponent(candidate.to_path_buf()));
        }
        let mut paths = candidate
            .ancestors()
            .filter(|path| !path.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .collect::<Vec<_>>();
        paths.reverse();
        paths
    };

    for path in paths {
        match probe(&path) {
            Ok(Some(true)) => return Err(PathChainError::ReparsePoint(path)),
            Ok(Some(false) | None) => {}
            Err(source) => return Err(PathChainError::Io { path, source }),
        }
    }
    Ok(())
}

/// Rejects every reparse point from the trusted or filesystem root through the candidate.
pub fn validate_path_chain(
    candidate: &Path,
    trusted_root: Option<&Path>,
) -> Result<(), PathChainError> {
    validate_path_chain_with(candidate, trusted_root, |path| {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        use std::os::windows::fs::MetadataExt;
        Ok(Some(
            metadata.file_type().is_symlink()
                || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0,
        ))
    })
}

/// Moves `source` over `destination` with `MoveFileExW`, replacing it and
/// committing the move to disk before returning. Unlike
/// [`replace_file_preserving_attributes`], this does not merge the replaced
/// file's identity, attributes, and ACL into the replacement.
pub fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    let source = wide_path(source);
    let destination = wide_path(destination);
    // SAFETY: both strings are NUL-terminated and outlive the call.
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Replaces `replaced` with `replacement` while retaining the replaced file's
/// identity, attributes, and ACL. The replacement path no longer exists after
/// success; with `backup` the previous contents survive there, moved by the
/// same call. [`replace_file`] uses `MoveFileExW` and does not provide this
/// metadata-preserving contract.
pub fn replace_file_preserving_attributes(
    replaced: &Path,
    replacement: &Path,
    backup: Option<&Path>,
) -> io::Result<()> {
    let replaced = wide_path(replaced);
    let replacement = wide_path(replacement);
    let backup = backup.map(wide_path);
    // SAFETY: every path is NUL-terminated and outlives the call; a null
    // backup selects no backup, and the exclude and reserved arguments are
    // null as the API requires.
    if unsafe {
        ReplaceFileW(
            replaced.as_ptr(),
            replacement.as_ptr(),
            backup.as_ref().map_or(null(), |path| path.as_ptr()),
            REPLACEFILE_WRITE_THROUGH,
            null(),
            null(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Schedules `path` for deletion at the next reboot.
pub fn delay_delete_until_reboot(path: &Path) -> io::Result<()> {
    let wide = wide_path(path);
    // SAFETY: the source is NUL-terminated and outlives the call; a null
    // destination with this flag means "delete".
    if unsafe { MoveFileExW(wide.as_ptr(), null(), MOVEFILE_DELAY_UNTIL_REBOOT) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Marks the file behind `file` for deletion when its last handle closes. The
/// handle that read and verified the contents is the one that deletes, so no
/// other file can be substituted between the check and the removal. The
/// handle must have been opened with `DELETE` access.
pub fn mark_open_file_for_deletion(file: &File) -> io::Result<()> {
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: the handle comes from a live `File`; the buffer pointer and
    // length describe exactly the local disposition structure.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            (&disposition as *const FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        PathChainError, file_attributes, is_reparse_point, mark_open_file_for_deletion,
        replace_file, replace_file_preserving_attributes, validate_path_chain,
        validate_path_chain_with,
    };
    use std::{
        io,
        path::{Path, PathBuf},
    };

    #[test]
    fn path_chain_core_handles_absence_and_reparse_entries() {
        struct Case {
            name: &'static str,
            reparse_suffix: Option<&'static str>,
            expected_reparse_suffix: Option<&'static str>,
        }
        for case in [
            Case {
                name: "absent ancestor",
                reparse_suffix: None,
                expected_reparse_suffix: None,
            },
            Case {
                name: "dangling symlink reported by probe",
                reparse_suffix: Some("dangling"),
                expected_reparse_suffix: Some("dangling"),
            },
            Case {
                name: "junction in the middle",
                reparse_suffix: Some("junction"),
                expected_reparse_suffix: Some("junction"),
            },
        ] {
            let root = Path::new(r"C:\trusted");
            let candidate = match case.name {
                "junction in the middle" => root.join("junction").join("child"),
                "dangling symlink reported by probe" => root.join("dangling").join("child"),
                _ => root.join("absent").join("child"),
            };
            let result = validate_path_chain_with(&candidate, Some(root), |path| {
                if case
                    .reparse_suffix
                    .is_some_and(|suffix| path.ends_with(suffix))
                {
                    Ok(Some(true))
                } else {
                    Ok(None)
                }
            });
            match case.expected_reparse_suffix {
                Some(suffix) => assert!(matches!(
                    result,
                    Err(PathChainError::ReparsePoint(path)) if path.ends_with(suffix)
                )),
                None => assert!(result.is_ok()),
            }
        }
    }

    #[test]
    fn path_chain_core_rejects_unsafe_and_escaped_candidates() {
        let root = Path::new(r"C:\trusted");
        assert!(matches!(
            validate_path_chain_with(
                &root.join("safe").join("..").join("escape"),
                Some(root),
                |_| Ok(Some(false))
            ),
            Err(PathChainError::UnsafeComponent(_))
        ));
        assert!(matches!(
            validate_path_chain_with(Path::new(r"C:\outside\file"), Some(root), |_| {
                Ok(Some(false))
            }),
            Err(PathChainError::EscapedRoot { .. })
        ));
        assert!(matches!(
            validate_path_chain_with(Path::new(r"relative\file"), None, |_| Ok(Some(false))),
            Err(PathChainError::UnsafeComponent(_))
        ));
    }

    #[test]
    fn path_chain_without_a_trusted_root_probes_root_to_candidate() {
        let candidate = Path::new(r"C:\trusted\child");
        let mut probed = Vec::new();
        validate_path_chain_with(candidate, None, |path| {
            probed.push(path.to_path_buf());
            Ok(Some(false))
        })
        .unwrap();

        assert_eq!(
            probed.first().map(PathBuf::as_path),
            Some(Path::new(r"C:\"))
        );
        assert_eq!(probed.last().map(PathBuf::as_path), Some(candidate));
    }

    #[test]
    fn path_chain_error_display_identifies_the_path_and_io_detail() {
        let path = PathBuf::from(r"C:\trusted\file");
        let error = PathChainError::Io {
            path: path.clone(),
            source: io::Error::new(io::ErrorKind::PermissionDenied, "denied"),
        };
        let rendered = error.to_string();

        assert!(rendered.contains(&path.display().to_string()));
        assert!(rendered.contains("denied"));
    }

    #[test]
    fn path_chain_core_preserves_probe_io_errors() {
        let candidate = Path::new(r"C:\trusted\file");
        let error = validate_path_chain_with(candidate, None, |path| {
            if path == candidate {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
            } else {
                Ok(Some(false))
            }
        })
        .unwrap_err();
        assert!(matches!(
            error,
            PathChainError::Io { path, source }
                if path == candidate && source.kind() == io::ErrorKind::PermissionDenied
        ));
    }

    #[test]
    fn real_directory_chain_skips_symlink_assertion_when_creation_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("one").join("two");
        std::fs::create_dir_all(&nested).unwrap();
        validate_path_chain(&nested, Some(directory.path())).unwrap();

        let dangling = directory.path().join("dangling");
        if std::os::windows::fs::symlink_dir(directory.path().join("missing"), &dangling).is_err() {
            return;
        }
        assert!(matches!(
            validate_path_chain(&dangling.join("child"), Some(directory.path())),
            Err(PathChainError::ReparsePoint(path)) if path == dangling
        ));
    }

    #[test]
    fn preserving_replacement_keeps_the_destination_path() {
        let directory = tempfile::tempdir().unwrap();
        let replaced = directory.path().join("replaced.txt");
        let replacement = directory.path().join("replacement.txt");
        std::fs::write(&replaced, b"old").unwrap();
        std::fs::write(&replacement, b"new").unwrap();

        replace_file_preserving_attributes(&replaced, &replacement, None).unwrap();
        assert_eq!(std::fs::read(&replaced).unwrap(), b"new");
        assert!(!replacement.exists());

        let backup = directory.path().join("replaced.bak");
        std::fs::write(&replacement, b"newer").unwrap();
        replace_file_preserving_attributes(&replaced, &replacement, Some(&backup)).unwrap();
        assert_eq!(std::fs::read(&replaced).unwrap(), b"newer");
        assert_eq!(std::fs::read(&backup).unwrap(), b"new");
        assert!(!replacement.exists());
    }

    #[test]
    fn attributes_replacement_and_handle_deletion_act_on_real_files() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.txt");
        let second = directory.path().join("second.txt");
        std::fs::write(&first, b"first").unwrap();
        std::fs::write(&second, b"second").unwrap();

        assert!(file_attributes(&first).unwrap() != 0);
        assert!(!is_reparse_point(&first).unwrap());
        assert!(file_attributes(&directory.path().join("missing")).is_err());

        replace_file(&first, &second).unwrap();
        assert!(!first.exists());
        assert_eq!(std::fs::read(&second).unwrap(), b"first");

        use std::os::windows::fs::OpenOptionsExt;
        const GENERIC_READ: u32 = 0x8000_0000;
        const DELETE: u32 = 0x0001_0000;
        let file = std::fs::OpenOptions::new()
            .access_mode(GENERIC_READ | DELETE)
            .open(&second)
            .unwrap();
        mark_open_file_for_deletion(&file).unwrap();
        drop(file);
        assert!(!second.exists());
    }
}
