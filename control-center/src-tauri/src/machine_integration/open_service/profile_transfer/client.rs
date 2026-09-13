use super::{
    super::{
        decode_profile_transfer_frame, ProfileTransferToken, PROFILE_TRANSFER_HEADER_BYTES,
        PROFILE_TRANSFER_MAGIC, PROFILE_TRANSFER_VERSION,
    },
    shared::*,
};
use mactype_service_platform::{OverlappedPipeClient, PipeAccess, PipeError};
use std::time::{Duration, Instant};

pub(in crate::machine_integration::open_service) fn receive_profile_from_pipe_bounded(
    token: &ProfileTransferToken,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    if token.server_pid == 0 || timeout.is_zero() {
        return Err("the profile pipe token or timeout is invalid".to_owned());
    }
    let deadline = Instant::now() + timeout;
    let pipe = OverlappedPipeClient::open(
        &profile_pipe_name(token),
        PipeAccess::Read,
        deadline,
        PROFILE_PIPE_POLL,
    )
    .map_err(|error| match error {
        PipeError::TimedOut => "connecting to the profile pipe timed out".to_owned(),
        PipeError::Io(error) => error.to_string(),
        other => map_pipe_error(
            other,
            "connecting to the profile pipe",
            "connecting to the profile pipe made no progress",
        ),
    })?;
    let actual_server_pid = pipe.server_process_id().map_err(|error| {
        format!(
            "querying the profile pipe server failed with {}",
            error.raw_os_error().unwrap_or(0)
        )
    })?;
    if actual_server_pid != token.server_pid {
        return Err("the profile pipe server PID does not match the broker token".to_owned());
    }
    let maximum = PROFILE_TRANSFER_HEADER_BYTES + mactype_service_contract::MAX_PROFILE_BYTES;
    let mut frame = Vec::with_capacity(PROFILE_TRANSFER_HEADER_BYTES);
    let mut expected_total = None;
    loop {
        let remaining = expected_total
            .map(|expected| expected - frame.len())
            .unwrap_or(maximum - frame.len());
        if remaining == 0 {
            return Err("the profile pipe frame exceeds the fixed size limit".to_owned());
        }
        let mut chunk = vec![0_u8; remaining.min(64 * 1024)];
        let read = pipe
            .read(&mut chunk, &pipe_wait(deadline, None))
            .map_err(|error| {
                map_pipe_error(
                    error,
                    "receiving the profile pipe frame",
                    "reading the profile pipe made no progress",
                )
            })?;
        if read == 0 {
            return Err("the profile pipe closed before sending a complete frame".to_owned());
        }
        frame.extend_from_slice(&chunk[..read]);
        if expected_total.is_none() && frame.len() >= 12 {
            if &frame[..4] != PROFILE_TRANSFER_MAGIC
                || u16::from_le_bytes(frame[4..6].try_into().expect("fixed frame version prefix"))
                    != PROFILE_TRANSFER_VERSION
                || u16::from_le_bytes(frame[6..8].try_into().expect("fixed reserved prefix")) != 0
            {
                return Err("profile transfer frame header is invalid".to_owned());
            }
            let payload_len =
                u32::from_le_bytes(frame[8..12].try_into().expect("fixed frame length prefix"))
                    as usize;
            if payload_len == 0 || payload_len > mactype_service_contract::MAX_PROFILE_BYTES {
                return Err("profile transfer frame length is invalid".to_owned());
            }
            expected_total = Some(PROFILE_TRANSFER_HEADER_BYTES + payload_len);
        }
        if expected_total.is_some_and(|expected| frame.len() > expected) {
            return Err("profile transfer frame has trailing bytes".to_owned());
        }
        if expected_total == Some(frame.len()) {
            return decode_profile_transfer_frame(&frame, &token.nonce);
        }
    }
}
