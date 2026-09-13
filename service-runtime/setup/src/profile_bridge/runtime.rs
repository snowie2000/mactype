use std::path::PathBuf;

use serde::Deserialize;

use super::ProfileRuntimeBridge;
use crate::storage::{read_bounded_regular_file, reject_reparse_ancestors, SetupError};

const MAX_POINTER_BYTES: u64 = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimePointer {
    schema: u32,
    version: String,
}

impl ProfileRuntimeBridge {
    pub(super) fn active_runtime_root(&self) -> Result<Option<PathBuf>, SetupError> {
        let pointer_path = self.paths.runtime_pointer();
        if !pointer_path.exists() {
            return Ok(None);
        }
        reject_reparse_ancestors(pointer_path)?;
        let bytes =
            read_bounded_regular_file(pointer_path, MAX_POINTER_BYTES, "active runtime pointer")?;
        let pointer: RuntimePointer = serde_json::from_slice(&bytes)
            .map_err(|_| SetupError::Runtime("active runtime pointer is invalid".to_owned()))?;
        if pointer.schema != 1 || !safe_version_component(&pointer.version) {
            return Err(SetupError::Runtime(
                "active runtime pointer has an unsupported value".to_owned(),
            ));
        }
        let root = self.paths.runtime_versions().join(pointer.version);
        reject_reparse_ancestors(&root)?;
        if !root.is_dir() {
            return Err(SetupError::Runtime(
                "active runtime generation is unavailable".to_owned(),
            ));
        }
        Ok(Some(root))
    }
}

fn safe_version_component(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 64
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        && !matches!(version, "." | "..")
}
