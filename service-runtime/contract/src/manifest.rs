use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const RUNTIME_MANIFEST_SCHEMA: u32 = 1;
pub const MAX_RUNTIME_FILE_BYTES: usize = 32 * 1024 * 1024;

pub const IMMUTABLE_RUNTIME_FILES: [&str; 5] = [
    "mactype-service.exe",
    "mactype-injector32.exe",
    "mactype-injector64.exe",
    "MacType.dll",
    "MacType64.dll",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeManifest {
    schema: u32,
    version: String,
    files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedRuntimeManifest {
    version: String,
    files: Vec<String>,
}

impl VerifiedRuntimeManifest {
    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn files(&self) -> &[String] {
        &self.files
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestError {
    InvalidJson,
    UnsupportedSchema,
    InvalidVersion,
    InvalidFileSet,
    InvalidHash,
    HashMismatch,
    FileEmpty,
    FileTooLarge,
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "runtime manifest verification failed: {self:?}")
    }
}

impl std::error::Error for ManifestError {}

pub fn sha256_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(71);
    encoded.push_str("sha256:");
    for byte in digest {
        use fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing to a string cannot fail");
    }
    encoded
}

pub fn runtime_generation_id(files: &BTreeMap<String, Vec<u8>>) -> Result<String, ManifestError> {
    if !exact_runtime_file_set(files) {
        return Err(ManifestError::InvalidFileSet);
    }

    let mut identity_material = Vec::with_capacity(IMMUTABLE_RUNTIME_FILES.len() * 96);
    for name in IMMUTABLE_RUNTIME_FILES {
        let bytes = files.get(name).ok_or(ManifestError::InvalidFileSet)?;
        if bytes.is_empty() {
            return Err(ManifestError::FileEmpty);
        }
        if bytes.len() > MAX_RUNTIME_FILE_BYTES {
            return Err(ManifestError::FileTooLarge);
        }
        identity_material.extend_from_slice(name.as_bytes());
        identity_material.extend_from_slice(sha256_digest(bytes).as_bytes());
    }
    Ok(sha256_digest(&identity_material)[7..].to_owned())
}

pub fn verify_runtime_manifest(
    manifest_json: &[u8],
    package_files: &BTreeMap<String, Vec<u8>>,
) -> Result<VerifiedRuntimeManifest, ManifestError> {
    let manifest: RuntimeManifest =
        serde_json::from_slice(manifest_json).map_err(|_| ManifestError::InvalidJson)?;

    if manifest.schema != RUNTIME_MANIFEST_SCHEMA {
        return Err(ManifestError::UnsupportedSchema);
    }
    if !valid_version(&manifest.version) {
        return Err(ManifestError::InvalidVersion);
    }
    if manifest.files.len() != IMMUTABLE_RUNTIME_FILES.len()
        || IMMUTABLE_RUNTIME_FILES
            .iter()
            .any(|name| !manifest.files.contains_key(*name))
        || manifest.files.keys().any(|name| !is_allowed_name(name))
        || package_files.len() != manifest.files.len()
        || package_files
            .keys()
            .any(|name| !manifest.files.contains_key(name))
    {
        return Err(ManifestError::InvalidFileSet);
    }

    for (name, expected_hash) in &manifest.files {
        if !canonical_sha256(expected_hash) {
            return Err(ManifestError::InvalidHash);
        }
        let bytes = package_files
            .get(name)
            .ok_or(ManifestError::InvalidFileSet)?;
        if bytes.is_empty() {
            return Err(ManifestError::FileEmpty);
        }
        if bytes.len() > MAX_RUNTIME_FILE_BYTES {
            return Err(ManifestError::FileTooLarge);
        }
        if sha256_digest(bytes) != *expected_hash {
            return Err(ManifestError::HashMismatch);
        }
    }

    Ok(VerifiedRuntimeManifest {
        version: manifest.version,
        files: manifest.files.into_keys().collect(),
    })
}

fn is_allowed_name(name: &str) -> bool {
    IMMUTABLE_RUNTIME_FILES.contains(&name)
}

pub(crate) fn exact_runtime_file_set<T>(files: &BTreeMap<String, T>) -> bool {
    files.len() == IMMUTABLE_RUNTIME_FILES.len()
        && IMMUTABLE_RUNTIME_FILES
            .iter()
            .all(|name| files.contains_key(*name))
        && files.keys().all(|name| is_allowed_name(name))
}

pub(crate) fn canonical_sha256(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn valid_version(version: &str) -> bool {
    if version.is_empty()
        || version.len() > 64
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
    {
        return false;
    }

    let core = version.split(['-', '+']).next().unwrap_or_default();
    let components = core.split('.').collect::<Vec<_>>();
    components.len() == 3
        && components.iter().all(|component| {
            !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
        })
}
