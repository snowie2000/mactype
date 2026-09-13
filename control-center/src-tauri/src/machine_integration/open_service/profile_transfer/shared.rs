use super::super::{ProfileTransferToken, PROFILE_TRANSFER_NONCE_BYTES};
#[cfg(test)]
use mactype_service_platform::cancelled_pipe_operations;
use mactype_service_platform::{system_random_bytes, PipeError, PipeWait, Process};
use std::time::{Duration, Instant};

pub(in crate::machine_integration::open_service) const PROFILE_PIPE_TIMEOUT: Duration =
    Duration::from_secs(60);
pub(super) const PROFILE_PIPE_POLL: Duration = Duration::from_millis(10);
pub(super) const PROFILE_PIPE_BUFFER_BYTES: u32 = 64 * 1024;
// Only the elevated broker (LocalSystem or Administrators) and the pipe owner
// (the unelevated Control Center that created it) ever read this pipe. Granting
// Authenticated Users read let any local process first-connect the single pipe
// instance during the UAC window and deterministically DoS every publish/
// migrate/remove-legacy operation, so that ACE is intentionally absent.
pub(in crate::machine_integration::open_service) const PROFILE_PIPE_SDDL: &str =
    "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;OW)";

#[cfg(test)]
thread_local! {
    static PROFILE_PIPE_REAP_BASELINE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(in crate::machine_integration::open_service) fn reset_profile_pipe_reap_count() {
    PROFILE_PIPE_REAP_BASELINE.set(cancelled_pipe_operations());
}

#[cfg(test)]
pub(in crate::machine_integration::open_service) fn profile_pipe_reap_count() -> u64 {
    cancelled_pipe_operations() - PROFILE_PIPE_REAP_BASELINE.get()
}

pub(super) fn pipe_wait<'a>(deadline: Instant, peer: Option<&'a Process>) -> PipeWait<'a> {
    PipeWait {
        deadline,
        poll: PROFILE_PIPE_POLL,
        peer,
    }
}

pub(super) fn map_pipe_error(error: PipeError, operation: &str, no_progress: &str) -> String {
    match error {
        PipeError::PeerExited => format!("the elevated broker exited while {operation}"),
        PipeError::TimedOut => format!("{operation} timed out"),
        PipeError::PeerCheck(error) => format!(
            "checking the elevated broker while {operation} failed with {}",
            error.raw_os_error().unwrap_or(0)
        ),
        PipeError::Io(error) => format!(
            "{operation} failed with {}",
            error.raw_os_error().unwrap_or(0)
        ),
        PipeError::NoProgress => no_progress.to_owned(),
        PipeError::Cancel(error) => format!(
            "cancelling the profile pipe operation failed with {}",
            error.raw_os_error().unwrap_or(0)
        ),
    }
}

pub(in crate::machine_integration::open_service) fn random_profile_transfer_nonce(
) -> Result<[u8; PROFILE_TRANSFER_NONCE_BYTES], String> {
    let mut nonce = [0_u8; PROFILE_TRANSFER_NONCE_BYTES];
    system_random_bytes(&mut nonce)
        .map_err(|error| format!("generating the profile transfer nonce failed: {error}"))?;
    Ok(nonce)
}

pub(in crate::machine_integration::open_service) fn profile_transfer_nonce_text(
    nonce: &[u8; PROFILE_TRANSFER_NONCE_BYTES],
) -> String {
    let mut text = String::with_capacity(PROFILE_TRANSFER_NONCE_BYTES * 2);
    for byte in nonce {
        use std::fmt::Write as _;
        write!(text, "{byte:02x}").expect("writing to a String cannot fail");
    }
    text
}

pub(super) fn profile_pipe_name(token: &ProfileTransferToken) -> String {
    format!(
        r"\\.\pipe\MacTypeControlCenter.profile.v1.{}.{}",
        token.server_pid,
        profile_transfer_nonce_text(&token.nonce)
    )
}
