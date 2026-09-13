mod recovery;
mod repair;
mod transition;

use mactype_service_contract::{
    valid_runtime_version_component, RuntimeActivationReceipt, RuntimeGenerationPointer,
    MAX_RUNTIME_ACTIVATION_RECEIPT_BYTES, MAX_RUNTIME_POINTER_BYTES,
};

use crate::storage::SetupError;

pub(super) type RuntimePointer = RuntimeGenerationPointer;
pub(super) const MAX_ACTIVATION_JOURNAL_BYTES: u64 = MAX_RUNTIME_ACTIVATION_RECEIPT_BYTES;
pub(super) const MAX_POINTER_BYTES: u64 = MAX_RUNTIME_POINTER_BYTES;

pub(super) fn validate_runtime_pointer(bytes: &[u8]) -> Result<RuntimePointer, SetupError> {
    RuntimePointer::parse(bytes)
        .map_err(|_| SetupError::Runtime("active runtime pointer is invalid".to_owned()))
}

fn activation_receipt_bytes(receipt: &RuntimeActivationReceipt) -> Result<Vec<u8>, SetupError> {
    receipt
        .to_bytes()
        .map_err(|_| SetupError::Runtime("runtime activation journal is invalid".to_owned()))
}

pub(super) fn safe_version_component(version: &str) -> bool {
    valid_runtime_version_component(version)
}
