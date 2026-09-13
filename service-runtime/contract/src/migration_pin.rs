use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::manifest::{canonical_sha256, exact_runtime_file_set, valid_version};

pub const MIGRATION_RUNTIME_PIN_SCHEMA: u32 = 1;
pub const MAX_MIGRATION_RUNTIME_PIN_BYTES: u64 = 64 * 1024;
pub const MAX_PINNED_RUNTIMES: usize = 64;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationPinnedRuntime {
    version: String,
    files: BTreeMap<String, String>,
    generated_profile: Option<String>,
}

impl MigrationPinnedRuntime {
    pub fn new(
        version: String,
        files: BTreeMap<String, String>,
        generated_profile: Option<String>,
    ) -> Result<Self, MigrationPinError> {
        let runtime = Self {
            version,
            files,
            generated_profile,
        };
        runtime.validate()?;
        Ok(runtime)
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn files(&self) -> &BTreeMap<String, String> {
        &self.files
    }

    pub fn generated_profile(&self) -> Option<&str> {
        self.generated_profile.as_deref()
    }

    pub fn validate(&self) -> Result<(), MigrationPinError> {
        if !valid_version(&self.version)
            || !exact_runtime_file_set(&self.files)
            || self.files.values().any(|digest| !canonical_sha256(digest))
            || self
                .generated_profile
                .as_deref()
                .is_some_and(|digest| !canonical_sha256(digest))
        {
            return Err(MigrationPinError);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationRuntimePin {
    schema: u32,
    nonce: String,
    runtimes: Vec<MigrationPinnedRuntime>,
}

impl MigrationRuntimePin {
    pub fn new(
        nonce: String,
        runtimes: Vec<MigrationPinnedRuntime>,
    ) -> Result<Self, MigrationPinError> {
        let pin = Self {
            schema: MIGRATION_RUNTIME_PIN_SCHEMA,
            nonce,
            runtimes,
        };
        pin.validate()?;
        Ok(pin)
    }

    pub fn nonce(&self) -> &str {
        &self.nonce
    }

    pub fn runtimes(&self) -> &[MigrationPinnedRuntime] {
        &self.runtimes
    }

    pub fn validate(&self) -> Result<(), MigrationPinError> {
        if self.schema != MIGRATION_RUNTIME_PIN_SCHEMA
            || self.nonce.len() != 32
            || !self
                .nonce
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || self.runtimes.is_empty()
            || self.runtimes.len() > MAX_PINNED_RUNTIMES
            || self
                .runtimes
                .iter()
                .any(|runtime| runtime.validate().is_err())
        {
            return Err(MigrationPinError);
        }
        let versions = self
            .runtimes
            .iter()
            .map(MigrationPinnedRuntime::version)
            .collect::<BTreeSet<_>>();
        if versions.len() != self.runtimes.len() {
            return Err(MigrationPinError);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MigrationPinError;

impl fmt::Display for MigrationPinError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("migration runtime pin violates the fixed contract")
    }
}

impl std::error::Error for MigrationPinError {}
