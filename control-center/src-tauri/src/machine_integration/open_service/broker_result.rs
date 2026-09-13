use serde::{Deserialize, Serialize};

use super::PROFILE_TRANSFER_NONCE_BYTES;

pub(super) const BROKER_RESULT_MAGIC: &[u8; 4] = b"MTBR";
pub(super) const BROKER_RESULT_VERSION: u16 = 1;
pub(super) const MAX_BROKER_RESULT_BYTES: usize = 32 * 1024;
pub(super) const BROKER_RESULT_HEADER_BYTES: usize =
    4 + 2 + 2 + 4 + PROFILE_TRANSFER_NONCE_BYTES + 32;
const MAX_OPERATION_BYTES: usize = 64;
const MAX_STAGE_BYTES: usize = 256;
const MAX_ERROR_CHAIN_BYTES: usize = 24 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum BrokerResultDisposition {
    Success,
    Blocked,
    Failure,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct BrokerResultMessage {
    pub(super) operation: String,
    pub(super) disposition: BrokerResultDisposition,
    pub(super) stage: String,
    pub(super) error_chain: String,
    pub(super) rollback: String,
    pub(super) final_state: String,
}

impl BrokerResultMessage {
    pub(super) fn success(operation: &str) -> Self {
        Self {
            operation: bounded_text(operation, MAX_OPERATION_BYTES),
            disposition: BrokerResultDisposition::Success,
            stage: String::new(),
            error_chain: String::new(),
            rollback: String::new(),
            final_state: String::new(),
        }
    }

    pub(super) fn failure(operation: &str, stage: &str, error_chain: &str) -> Self {
        Self {
            operation: bounded_text(operation, MAX_OPERATION_BYTES),
            disposition: BrokerResultDisposition::Failure,
            stage: bounded_text(stage, MAX_STAGE_BYTES),
            error_chain: bounded_text(error_chain, MAX_ERROR_CHAIN_BYTES),
            rollback: String::new(),
            final_state: String::new(),
        }
    }
}

fn bounded_text(value: &str, maximum_bytes: usize) -> String {
    if value.len() <= maximum_bytes {
        return value.to_owned();
    }
    let suffix = " [truncated]";
    let mut end = maximum_bytes.saturating_sub(suffix.len());
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    let mut bounded = value[..end].to_owned();
    bounded.push_str(suffix);
    bounded
}

pub(super) fn encode_broker_result_frame(
    message: &BrokerResultMessage,
    nonce: &[u8; PROFILE_TRANSFER_NONCE_BYTES],
) -> Result<Vec<u8>, String> {
    let payload = serde_json::to_vec(message).map_err(|error| error.to_string())?;
    if payload.is_empty() || payload.len() > MAX_BROKER_RESULT_BYTES {
        return Err("broker result exceeds the fixed size limit".to_owned());
    }
    let mut frame = Vec::with_capacity(BROKER_RESULT_HEADER_BYTES + payload.len());
    frame.extend_from_slice(BROKER_RESULT_MAGIC);
    frame.extend_from_slice(&BROKER_RESULT_VERSION.to_le_bytes());
    frame.extend_from_slice(&0_u16.to_le_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(nonce);
    frame.extend_from_slice(&digest_bytes(&payload)?);
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub(super) fn decode_broker_result_frame(
    frame: &[u8],
    expected_nonce: &[u8; PROFILE_TRANSFER_NONCE_BYTES],
) -> Result<BrokerResultMessage, String> {
    if frame.len() < BROKER_RESULT_HEADER_BYTES
        || &frame[..4] != BROKER_RESULT_MAGIC
        || u16::from_le_bytes(frame[4..6].try_into().expect("fixed result version"))
            != BROKER_RESULT_VERSION
        || u16::from_le_bytes(frame[6..8].try_into().expect("fixed result reserved")) != 0
        || &frame[12..28] != expected_nonce
    {
        return Err("broker result frame header is invalid".to_owned());
    }
    let payload_len =
        u32::from_le_bytes(frame[8..12].try_into().expect("fixed result length")) as usize;
    if payload_len == 0
        || payload_len > MAX_BROKER_RESULT_BYTES
        || frame.len() != BROKER_RESULT_HEADER_BYTES + payload_len
    {
        return Err("broker result frame length is invalid".to_owned());
    }
    let payload = &frame[BROKER_RESULT_HEADER_BYTES..];
    if frame[28..60] != digest_bytes(payload)? {
        return Err("broker result frame digest does not match".to_owned());
    }
    serde_json::from_slice(payload).map_err(|error| error.to_string())
}

fn digest_bytes(bytes: &[u8]) -> Result<[u8; 32], String> {
    let digest = mactype_service_contract::sha256_digest(bytes);
    let hex = digest
        .strip_prefix("sha256:")
        .ok_or_else(|| "broker result digest is invalid".to_owned())?;
    if hex.len() != 64 {
        return Err("broker result digest is invalid".to_owned());
    }
    let mut result = [0_u8; 32];
    for (index, output) in result.iter_mut().enumerate() {
        *output = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| "broker result digest is invalid".to_owned())?;
    }
    Ok(result)
}
