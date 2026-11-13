use super::read_bounded_regular_file;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(super) const MAX_BUNDLED_MANIFEST_BYTES: u64 = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BundledRuntimeManifest {
    pub(super) schema: u32,
    pub(super) version: String,
    pub(super) files: BTreeMap<String, String>,
}

pub(super) fn parse_bundled_runtime_manifest(
    bytes: &[u8],
) -> Result<BundledRuntimeManifest, String> {
    let manifest: BundledRuntimeManifest = serde_json::from_slice(bytes)
        .map_err(|_| "bundled runtime manifest is invalid".to_owned())?;
    if manifest.schema != mactype_service_contract::RUNTIME_MANIFEST_SCHEMA
        || !safe_runtime_version(&manifest.version)
        || manifest.files.len() != mactype_service_contract::IMMUTABLE_RUNTIME_FILES.len()
        || mactype_service_contract::IMMUTABLE_RUNTIME_FILES
            .iter()
            .any(|name| !manifest.files.contains_key(*name))
        || manifest.files.iter().any(|(name, digest)| {
            !mactype_service_contract::IMMUTABLE_RUNTIME_FILES.contains(&name.as_str())
                || digest.len() != 71
                || !digest.starts_with("sha256:")
                || !digest[7..]
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
    {
        return Err("bundled runtime manifest violates the fixed schema".to_owned());
    }
    Ok(manifest)
}

pub(super) fn bundled_runtime_version(bytes: &[u8]) -> Result<String, String> {
    parse_bundled_runtime_manifest(bytes).map(|manifest| manifest.version)
}

pub(super) fn safe_runtime_version(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        || matches!(value, "." | "..")
    {
        return false;
    }
    let core = value.split(['-', '+']).next().unwrap_or_default();
    let components = core.split('.').collect::<Vec<_>>();
    components.len() == 3
        && components.iter().all(|component| {
            !component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit())
        })
}

pub(super) fn bundled_service_binary(service_root: &Path) -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let app_root = executable
        .parent()
        .ok_or_else(|| "Control Center executable has no parent".to_owned())?;
    let manifest_path = app_root
        .join("service-runtime")
        .join("payload")
        .join("manifest.json");
    let manifest = read_bounded_regular_file(
        &manifest_path,
        MAX_BUNDLED_MANIFEST_BYTES,
        "bundled runtime manifest",
    )?;
    let version = bundled_runtime_version(&manifest)?;
    Ok(service_root
        .join("bin")
        .join(version)
        .join("mactype-service.exe"))
}
