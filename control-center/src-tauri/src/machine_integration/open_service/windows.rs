use super::*;
use mactype_service_contract::{
    runtime_generation_id, sha256_digest, InjectionArchitecture, MachinePaths,
    MigrationPinnedRuntime, MigrationRuntimePin, IMMUTABLE_RUNTIME_FILES,
    MAX_MIGRATION_RUNTIME_PIN_BYTES, MAX_PROFILE_BYTES, MAX_RUNTIME_FILE_BYTES,
};
use mactype_service_platform::{KnownFolder, MachineKind, Process, ProcessAccess};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    io::Read,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Child,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    System::SystemInformation::{
        IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64, IMAGE_FILE_MACHINE_I386,
        IMAGE_FILE_MACHINE_UNKNOWN,
    },
    System::Threading::CREATE_NO_WINDOW,
};

#[cfg(test)]
pub(super) use super::profile_transfer::{
    profile_pipe_reap_count, receive_profile_from_pipe_bounded, reset_profile_pipe_reap_count,
    KillOnCloseJob, ProfilePipeServer, PROFILE_PIPE_SDDL,
};
pub(super) use super::profile_transfer::{
    profile_transfer_nonce_text, random_profile_transfer_nonce,
};
#[cfg(test)]
pub(super) use super::profile_transfer::{BrokerResultPipeServer, BrokerResultPipeWriter};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RuntimePointer {
    pub(super) schema: u32,
    pub(super) version: String,
}

mod backend;
mod mutation;
mod rollback;
mod runtime_pin;
mod smoke;
mod snapshot;

pub(super) use backend::SystemMigrationBackend;
use mutation::*;
use runtime_pin::*;
use snapshot::*;

pub(super) fn machine_roots() -> Result<(PathBuf, PathBuf), String> {
    snapshot::machine_roots_impl()
}

use rollback::rollback_open_service_snapshot;
#[cfg(test)]
pub(super) use rollback::{
    plan_file_rollback, plan_generation_cleanup, remove_empty_directory, FileRollbackAction,
};

#[cfg(test)]
pub(super) use smoke::marker_x64_system_directory;
use smoke::system_injection_smoke;
pub(super) use smoke::system_removal_verification;

#[cfg(test)]
pub(super) use super::broker::{
    combine_broker_cleanup_error, terminate_broker_process_with, BrokerProcessControl,
    BrokerTermination,
};
use super::broker::{
    fixed_setup_path, reject_reparse_ancestors, run_restore_pinned_runtime, run_setup,
};
pub(super) use super::broker::{run_elevated, run_elevated_at, run_privileged};
use super::platform::{
    known_folder, read_health_for_scm_process, running_service_process_id, safe_version,
};
pub(super) use super::platform::{query, reveal_system_service};
