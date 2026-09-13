mod client;
mod handle;
mod result;
mod server;
mod shared;

pub(super) use client::receive_profile_from_pipe_bounded;
pub(super) use handle::KillOnCloseJob;
pub(super) use result::{BrokerResultPipeServer, BrokerResultPipeWriter};
pub(super) use server::ProfilePipeServer;
#[cfg(test)]
pub(super) use shared::{
    profile_pipe_reap_count, reset_profile_pipe_reap_count, PROFILE_PIPE_SDDL,
};
pub(super) use shared::{
    profile_transfer_nonce_text, random_profile_transfer_nonce, PROFILE_PIPE_TIMEOUT,
};
