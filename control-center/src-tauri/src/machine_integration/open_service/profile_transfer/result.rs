use super::{
    super::{
        decode_broker_result_frame, encode_broker_result_frame, BrokerResultMessage,
        ProfileTransferToken, BROKER_RESULT_HEADER_BYTES, BROKER_RESULT_MAGIC,
        BROKER_RESULT_VERSION, MAX_BROKER_RESULT_BYTES, PROFILE_TRANSFER_NONCE_BYTES,
    },
    shared::*,
};
use mactype_service_platform::{
    OverlappedPipeClient, OverlappedPipeServer, PipeAccess, PipeDirection, PipeError, Process,
    SecurityDescriptor,
};
use std::time::{Duration, Instant};

pub(in crate::machine_integration::open_service) struct BrokerResultPipeServer {
    pipe: OverlappedPipeServer,
    token: ProfileTransferToken,
}

impl BrokerResultPipeServer {
    pub(in crate::machine_integration::open_service) fn create() -> Result<Self, String> {
        Self::create_with_nonce(random_profile_transfer_nonce()?)
    }

    pub(in crate::machine_integration::open_service) fn create_with_nonce(
        nonce: [u8; PROFILE_TRANSFER_NONCE_BYTES],
    ) -> Result<Self, String> {
        let token = ProfileTransferToken {
            server_pid: std::process::id(),
            nonce,
        };
        let descriptor = SecurityDescriptor::from_sddl(PROFILE_PIPE_SDDL).map_err(|error| {
            format!(
                "creating the local broker result pipe ACL failed with {}",
                error.raw_os_error().unwrap_or(0)
            )
        })?;
        let buffer_bytes = u32::try_from(BROKER_RESULT_HEADER_BYTES + MAX_BROKER_RESULT_BYTES)
            .expect("broker result pipe buffer size fits in u32");
        let pipe = OverlappedPipeServer::create_single_instance(
            &result_pipe_name(&token),
            PipeDirection::Inbound,
            &descriptor,
            0,
            buffer_bytes,
            PROFILE_PIPE_TIMEOUT,
        )
        .map_err(|error| {
            format!(
                "creating the first broker result pipe instance failed with {}",
                error.raw_os_error().unwrap_or(0)
            )
        })?;
        Ok(Self { pipe, token })
    }

    pub(in crate::machine_integration::open_service) fn token(&self) -> &ProfileTransferToken {
        &self.token
    }

    pub(in crate::machine_integration::open_service) fn receive_from(
        self,
        expected_client_pid: u32,
        broker: Option<&Process>,
        timeout: Duration,
    ) -> Result<BrokerResultMessage, String> {
        if expected_client_pid == 0 || timeout.is_zero() {
            return Err("the broker result pipe peer or timeout is invalid".to_owned());
        }
        let deadline = Instant::now() + timeout;
        let wait = pipe_wait(deadline, broker);
        self.pipe.connect(&wait).map_err(|error| {
            map_pipe_error(
                error,
                "waiting for the broker result pipe client",
                "connecting the broker result pipe made no progress",
            )
        })?;
        let actual_client_pid = self.pipe.client_process_id().map_err(|error| {
            format!(
                "querying the broker result pipe client failed with {}",
                error.raw_os_error().unwrap_or(0)
            )
        })?;
        if actual_client_pid != expected_client_pid {
            return Err(
                "the first broker result pipe client is not the elevated broker".to_owned(),
            );
        }

        let maximum = BROKER_RESULT_HEADER_BYTES + MAX_BROKER_RESULT_BYTES;
        let mut frame = Vec::with_capacity(BROKER_RESULT_HEADER_BYTES);
        let mut expected_total = None;
        loop {
            let remaining = expected_total
                .map(|expected| expected - frame.len())
                .unwrap_or(maximum - frame.len());
            if remaining == 0 {
                return Err("the broker result frame exceeds the fixed size limit".to_owned());
            }
            let mut chunk = vec![0_u8; remaining.min(16 * 1024)];
            let read = self.pipe.read(&mut chunk, &wait).map_err(|error| {
                map_pipe_error(
                    error,
                    "receiving the broker result frame",
                    "reading the broker result pipe made no progress",
                )
            })?;
            if read == 0 {
                return Err(
                    "the broker result pipe closed before sending a complete frame".to_owned(),
                );
            }
            frame.extend_from_slice(&chunk[..read]);
            if expected_total.is_none() && frame.len() >= 12 {
                if &frame[..4] != BROKER_RESULT_MAGIC
                    || u16::from_le_bytes(frame[4..6].try_into().expect("fixed result version"))
                        != BROKER_RESULT_VERSION
                    || u16::from_le_bytes(frame[6..8].try_into().expect("fixed result reserved"))
                        != 0
                {
                    return Err("broker result frame header is invalid".to_owned());
                }
                let payload_len =
                    u32::from_le_bytes(frame[8..12].try_into().expect("fixed result length"))
                        as usize;
                if payload_len == 0 || payload_len > MAX_BROKER_RESULT_BYTES {
                    return Err("broker result frame length is invalid".to_owned());
                }
                expected_total = Some(BROKER_RESULT_HEADER_BYTES + payload_len);
            }
            if expected_total.is_some_and(|expected| frame.len() > expected) {
                return Err("broker result frame has trailing bytes".to_owned());
            }
            if expected_total == Some(frame.len()) {
                return decode_broker_result_frame(&frame, &self.token.nonce);
            }
        }
    }
}

pub(in crate::machine_integration::open_service) struct BrokerResultPipeWriter {
    pipe: OverlappedPipeClient,
    nonce: [u8; PROFILE_TRANSFER_NONCE_BYTES],
}

impl BrokerResultPipeWriter {
    pub(in crate::machine_integration::open_service) fn connect(
        token: &ProfileTransferToken,
        timeout: Duration,
    ) -> Result<Self, String> {
        if token.server_pid == 0 || timeout.is_zero() {
            return Err("the broker result pipe token or timeout is invalid".to_owned());
        }
        let deadline = Instant::now() + timeout;
        let pipe = OverlappedPipeClient::open(
            &result_pipe_name(token),
            PipeAccess::Write,
            deadline,
            PROFILE_PIPE_POLL,
        )
        .map_err(|error| match error {
            PipeError::TimedOut => "connecting to the broker result pipe timed out".to_owned(),
            PipeError::Io(error) => error.to_string(),
            other => map_pipe_error(
                other,
                "connecting to the broker result pipe",
                "connecting to the broker result pipe made no progress",
            ),
        })?;
        let actual_server_pid = pipe.server_process_id().map_err(|error| {
            format!(
                "querying the broker result pipe server failed with {}",
                error.raw_os_error().unwrap_or(0)
            )
        })?;
        if actual_server_pid != token.server_pid {
            return Err(
                "the broker result pipe server PID does not match the broker token".to_owned(),
            );
        }
        Ok(Self {
            pipe,
            nonce: token.nonce,
        })
    }

    pub(in crate::machine_integration::open_service) fn send(
        self,
        message: &BrokerResultMessage,
        timeout: Duration,
    ) -> Result<(), String> {
        if timeout.is_zero() {
            return Err("the broker result pipe timeout is invalid".to_owned());
        }
        let frame = encode_broker_result_frame(message, &self.nonce)?;
        let wait = pipe_wait(Instant::now() + timeout, None);
        self.pipe.write_all(&frame, &wait).map_err(|error| {
            map_pipe_error(
                error,
                "sending the broker result frame",
                "writing the broker result pipe made no progress",
            )
        })
    }
}

fn result_pipe_name(token: &ProfileTransferToken) -> String {
    format!(
        r"\\.\pipe\MacTypeControlCenter.broker-result.v1.{}.{}",
        token.server_pid,
        profile_transfer_nonce_text(&token.nonce)
    )
}
