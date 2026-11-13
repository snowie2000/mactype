use mactype_service_contract::SourceMetadata;

use super::BrokerContext;
use crate::storage::create_protected_directory;
use crate::{ProfileStore, SetupError};

pub(super) fn publish(
    context: &BrokerContext,
    profile_input: Option<&[u8]>,
) -> Result<String, SetupError> {
    let input = profile_input
        .ok_or_else(|| SetupError::Runtime("publish-profile requires stdin bytes".to_owned()))?;
    let data_root = context
        .paths
        .active_profile()
        .parent()
        .ok_or_else(|| SetupError::Runtime("profile root is unavailable".to_owned()))?
        .to_owned();
    create_protected_directory(&data_root)?;
    super::super::acl::harden_machine_directory(&data_root)?;
    let generation = ProfileStore::new(context.paths.clone()).publish_and_activate(
        input,
        SourceMetadata {
            display_name: "MacType Control Center".to_owned(),
        },
    )?;
    super::super::acl::harden_machine_directory(&data_root)?;
    harden_runtime_if_installed(context)?;
    Ok(format!(
        "{{\"ok\":true,\"verb\":\"publish-profile\",\"generation\":\"{}\"}}",
        generation.as_str()
    ))
}

pub(super) fn rollback(context: &BrokerContext) -> Result<String, SetupError> {
    let generation = ProfileStore::new(context.paths.clone()).rollback()?;
    harden_runtime_if_installed(context)?;
    crate::event_log::rollback_completed("profile");
    Ok(match generation {
        Some(generation) => format!(
            "{{\"ok\":true,\"verb\":\"rollback\",\"generation\":\"{}\"}}",
            generation.as_str()
        ),
        None => "{\"ok\":true,\"verb\":\"rollback\",\"generation\":null}".to_owned(),
    })
}

fn harden_runtime_if_installed(context: &BrokerContext) -> Result<(), SetupError> {
    if context.paths.runtime_pointer().is_file() {
        super::super::acl::harden_machine_directory(context.paths.service_root())?;
    }
    Ok(())
}
