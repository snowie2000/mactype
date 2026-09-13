use super::{
    super::{encode_profile_transfer_frame, ProfileTransferToken, PROFILE_TRANSFER_NONCE_BYTES},
    shared::*,
};
use mactype_service_platform::{OverlappedPipeServer, PipeDirection, Process, SecurityDescriptor};
use std::time::{Duration, Instant};

pub(in crate::machine_integration::open_service) struct ProfilePipeServer {
    pipe: OverlappedPipeServer,
    #[cfg(test)]
    token: ProfileTransferToken,
    frame: Vec<u8>,
}

impl ProfilePipeServer {
    #[cfg(test)]
    pub(in crate::machine_integration::open_service) fn create(
        profile: &[u8],
    ) -> Result<Self, String> {
        Self::create_with_nonce(profile, random_profile_transfer_nonce()?)
    }

    pub(in crate::machine_integration::open_service) fn create_with_nonce(
        profile: &[u8],
        nonce: [u8; PROFILE_TRANSFER_NONCE_BYTES],
    ) -> Result<Self, String> {
        let token = ProfileTransferToken {
            server_pid: std::process::id(),
            nonce,
        };
        let frame = encode_profile_transfer_frame(profile, &token.nonce)?;
        let descriptor = SecurityDescriptor::from_sddl(PROFILE_PIPE_SDDL).map_err(|error| {
            format!(
                "creating the local profile pipe ACL failed with {}",
                error.raw_os_error().unwrap_or(0)
            )
        })?;
        let buffer_bytes = u32::try_from(frame.len().min(PROFILE_PIPE_BUFFER_BYTES as usize))
            .expect("profile pipe buffer size is bounded to u32");
        let pipe = OverlappedPipeServer::create_single_instance(
            &profile_pipe_name(&token),
            PipeDirection::Outbound,
            &descriptor,
            buffer_bytes,
            0,
            PROFILE_PIPE_TIMEOUT,
        )
        .map_err(|error| {
            format!(
                "creating the first profile pipe instance failed with {}",
                error.raw_os_error().unwrap_or(0)
            )
        })?;
        Ok(Self {
            pipe,
            #[cfg(test)]
            token,
            frame,
        })
    }

    #[cfg(test)]
    pub(in crate::machine_integration::open_service) fn token(&self) -> &ProfileTransferToken {
        &self.token
    }

    pub(in crate::machine_integration::open_service) fn send_to(
        self,
        expected_client_pid: u32,
        broker: Option<&Process>,
        timeout: Duration,
    ) -> Result<(), String> {
        if expected_client_pid == 0 || timeout.is_zero() {
            return Err("the profile pipe peer or timeout is invalid".to_owned());
        }
        let deadline = Instant::now() + timeout;
        let wait = pipe_wait(deadline, broker);
        self.pipe.connect(&wait).map_err(|error| {
            map_pipe_error(
                error,
                "waiting for the profile pipe client",
                "connecting the profile pipe made no progress",
            )
        })?;

        let actual_client_pid = self.pipe.client_process_id().map_err(|error| {
            format!(
                "querying the profile pipe client failed with {}",
                error.raw_os_error().unwrap_or(0)
            )
        })?;
        if actual_client_pid != expected_client_pid {
            return Err("the first profile pipe client is not the elevated broker".to_owned());
        }
        self.pipe.write_all(&self.frame, &wait).map_err(|error| {
            map_pipe_error(
                error,
                "sending the profile pipe frame",
                "writing the profile pipe made no progress",
            )
        })
    }
}
