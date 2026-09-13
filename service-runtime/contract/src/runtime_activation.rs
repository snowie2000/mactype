use std::fmt;

use serde::{Deserialize, Serialize};

pub const RUNTIME_POINTER_SCHEMA: u32 = 1;
pub const LEGACY_RUNTIME_ACTIVATION_SCHEMA: u32 = 1;
pub const UNCOMMITTED_RUNTIME_ACTIVATION_SCHEMA: u32 = 2;
pub const RUNTIME_ACTIVATION_SCHEMA: u32 = 3;
pub const MAX_RUNTIME_ACTIVATION_RECEIPT_BYTES: u64 = 16 * 1024;
pub const MAX_RUNTIME_POINTER_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeGenerationPointer {
    schema: u32,
    version: String,
}

impl RuntimeGenerationPointer {
    pub fn new(version: impl Into<String>) -> Result<Self, RuntimeActivationContractError> {
        let pointer = Self {
            schema: RUNTIME_POINTER_SCHEMA,
            version: version.into(),
        };
        pointer.validate()?;
        Ok(pointer)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, RuntimeActivationContractError> {
        if bytes.is_empty() || bytes.len() as u64 > MAX_RUNTIME_POINTER_BYTES {
            return Err(RuntimeActivationContractError);
        }
        let pointer: Self =
            serde_json::from_slice(bytes).map_err(|_| RuntimeActivationContractError)?;
        pointer.validate()?;
        Ok(pointer)
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, RuntimeActivationContractError> {
        serde_json::to_vec(self).map_err(|_| RuntimeActivationContractError)
    }

    fn validate(&self) -> Result<(), RuntimeActivationContractError> {
        if self.schema != RUNTIME_POINTER_SCHEMA || !valid_runtime_version_component(&self.version)
        {
            return Err(RuntimeActivationContractError);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeActivationPhase {
    Candidate,
    Committed,
    RollbackRequired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeActivationReceipt {
    schema: u32,
    phase: RuntimeActivationPhase,
    previous: Option<RuntimeGenerationPointer>,
    activated: RuntimeGenerationPointer,
}

impl RuntimeActivationReceipt {
    pub fn candidate(
        previous: Option<RuntimeGenerationPointer>,
        activated: RuntimeGenerationPointer,
    ) -> Self {
        Self {
            schema: RUNTIME_ACTIVATION_SCHEMA,
            phase: RuntimeActivationPhase::Candidate,
            previous,
            activated,
        }
    }

    pub fn with_phase(&self, phase: RuntimeActivationPhase) -> Self {
        Self {
            schema: RUNTIME_ACTIVATION_SCHEMA,
            phase,
            previous: self.previous.clone(),
            activated: self.activated.clone(),
        }
    }

    pub const fn phase(&self) -> RuntimeActivationPhase {
        self.phase
    }

    pub fn previous(&self) -> Option<&RuntimeGenerationPointer> {
        self.previous.as_ref()
    }

    pub const fn activated(&self) -> &RuntimeGenerationPointer {
        &self.activated
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, RuntimeActivationContractError> {
        serde_json::to_vec(self).map_err(|_| RuntimeActivationContractError)
    }

    fn validate(&self) -> Result<(), RuntimeActivationContractError> {
        if self.schema != RUNTIME_ACTIVATION_SCHEMA
            || self.activated.validate().is_err()
            || self
                .previous
                .as_ref()
                .is_some_and(|pointer| pointer.validate().is_err())
        {
            return Err(RuntimeActivationContractError);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedRuntimeActivationReceipt {
    Legacy {
        previous: Option<RuntimeGenerationPointer>,
    },
    Uncommitted {
        previous: Option<RuntimeGenerationPointer>,
        activated: RuntimeGenerationPointer,
    },
    Current(RuntimeActivationReceipt),
}

impl ParsedRuntimeActivationReceipt {
    pub fn previous(&self) -> Option<&RuntimeGenerationPointer> {
        match self {
            Self::Legacy { previous } | Self::Uncommitted { previous, .. } => previous.as_ref(),
            Self::Current(receipt) => receipt.previous(),
        }
    }

    pub fn activated(&self) -> Option<&RuntimeGenerationPointer> {
        match self {
            Self::Legacy { .. } => None,
            Self::Uncommitted { activated, .. } => Some(activated),
            Self::Current(receipt) => Some(receipt.activated()),
        }
    }

    pub fn phase(&self) -> Option<RuntimeActivationPhase> {
        match self {
            Self::Current(receipt) => Some(receipt.phase()),
            Self::Legacy { .. } | Self::Uncommitted { .. } => None,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyRuntimeActivationReceipt {
    schema: u32,
    previous: Option<RuntimeGenerationPointer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UncommittedRuntimeActivationReceipt {
    schema: u32,
    previous: Option<RuntimeGenerationPointer>,
    activated: RuntimeGenerationPointer,
}

pub fn parse_runtime_activation_receipt(
    bytes: &[u8],
) -> Result<ParsedRuntimeActivationReceipt, RuntimeActivationContractError> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_RUNTIME_ACTIVATION_RECEIPT_BYTES {
        return Err(RuntimeActivationContractError);
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| RuntimeActivationContractError)?;
    let schema = value
        .get("schema")
        .and_then(serde_json::Value::as_u64)
        .ok_or(RuntimeActivationContractError)?;
    match schema as u32 {
        RUNTIME_ACTIVATION_SCHEMA if schema == u64::from(RUNTIME_ACTIVATION_SCHEMA) => {
            let receipt: RuntimeActivationReceipt =
                serde_json::from_value(value).map_err(|_| RuntimeActivationContractError)?;
            receipt.validate()?;
            Ok(ParsedRuntimeActivationReceipt::Current(receipt))
        }
        UNCOMMITTED_RUNTIME_ACTIVATION_SCHEMA
            if schema == u64::from(UNCOMMITTED_RUNTIME_ACTIVATION_SCHEMA) =>
        {
            let receipt: UncommittedRuntimeActivationReceipt =
                serde_json::from_value(value).map_err(|_| RuntimeActivationContractError)?;
            if receipt.schema != UNCOMMITTED_RUNTIME_ACTIVATION_SCHEMA
                || receipt.activated.validate().is_err()
                || receipt
                    .previous
                    .as_ref()
                    .is_some_and(|pointer| pointer.validate().is_err())
            {
                return Err(RuntimeActivationContractError);
            }
            Ok(ParsedRuntimeActivationReceipt::Uncommitted {
                previous: receipt.previous,
                activated: receipt.activated,
            })
        }
        LEGACY_RUNTIME_ACTIVATION_SCHEMA
            if schema == u64::from(LEGACY_RUNTIME_ACTIVATION_SCHEMA) =>
        {
            let receipt: LegacyRuntimeActivationReceipt =
                serde_json::from_value(value).map_err(|_| RuntimeActivationContractError)?;
            if receipt.schema != LEGACY_RUNTIME_ACTIVATION_SCHEMA
                || receipt
                    .previous
                    .as_ref()
                    .is_some_and(|pointer| pointer.validate().is_err())
            {
                return Err(RuntimeActivationContractError);
            }
            Ok(ParsedRuntimeActivationReceipt::Legacy {
                previous: receipt.previous,
            })
        }
        _ => Err(RuntimeActivationContractError),
    }
}

pub fn valid_runtime_version_component(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 64
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        && !matches!(version, "." | "..")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeActivationContractError;

impl fmt::Display for RuntimeActivationContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("unsupported or malformed runtime activation receipt")
    }
}

impl std::error::Error for RuntimeActivationContractError {}
