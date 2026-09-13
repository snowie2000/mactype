use super::{
    super::*,
    common::{open_for, query_configuration, query_runtime},
    control::query,
};
use mactype_service_platform::{ServiceAccess, ServiceHandle};
use std::io;

pub(super) const MAX_SECURITY_DESCRIPTOR_BYTES: usize = 64 * 1024;
pub(super) const MAX_FAILURE_ACTIONS: usize = 64;
pub(super) const MAX_REQUIRED_PRIVILEGES: usize = 64;

fn query_error(error: io::Error) -> String {
    format!("QueryServiceConfig2W failed: {error}")
}

pub(super) fn service_has_triggers(service: &ServiceHandle) -> Result<bool, String> {
    let trigger = service.trigger_info().map_err(query_error)?;
    Ok(trigger.count != 0 || trigger.has_trigger_data || trigger.has_reserved_data)
}

pub(super) fn query_extended_configuration(
    service: &ServiceHandle,
) -> Result<ServiceExtendedConfiguration, String> {
    let description = service.description().map_err(query_error)?;
    let failure_actions = service.failure_actions().map_err(|error| {
        if error.kind() == io::ErrorKind::InvalidData
            && error.to_string().contains("unsupported failure action")
        {
            "legacy service has an unsupported failure action".to_owned()
        } else {
            query_error(error)
        }
    })?;
    let failure_actions_on_non_crash = service
        .failure_actions_on_non_crash_failures()
        .map_err(query_error)?;
    let delayed_auto_start = service.delayed_auto_start().map_err(query_error)?;
    let service_sid_type = service.service_sid_type().map_err(query_error)?;
    if !matches!(service_sid_type, 0 | 1 | 3) {
        return Err("legacy service has an unsupported service SID type".to_owned());
    }
    let required_privileges = service.required_privileges().map_err(query_error)?;
    let preshutdown_timeout_ms = service.preshutdown_timeout_ms().map_err(query_error)?;
    let trigger = service.trigger_info().map_err(query_error)?;
    let security_descriptor = service.object_security().map_err(query_error)?;

    Ok(ServiceExtendedConfiguration {
        description,
        failure_actions: FailureActionsConfiguration {
            reset_period_seconds: failure_actions.reset_period_seconds,
            reboot_message: failure_actions.reboot_message,
            command: failure_actions.command,
            actions: failure_actions
                .actions
                .into_iter()
                .map(|action| FailureAction {
                    action_type: action.kind.as_raw(),
                    delay_ms: action.delay_ms,
                })
                .collect(),
        },
        failure_actions_on_non_crash,
        delayed_auto_start,
        service_sid_type,
        required_privileges,
        preshutdown_timeout_ms,
        triggers: snapshot_trigger_configuration(
            trigger.count,
            trigger.has_trigger_data,
            trigger.has_reserved_data,
        )?,
        security_descriptor: SecurityDescriptorSnapshot {
            self_relative: security_descriptor.as_bytes().to_vec(),
        },
    })
}

pub(super) fn migration_snapshot(registry_conflict: bool) -> Result<LegacyScmSnapshot, String> {
    let status = query(registry_conflict);
    if status.registry_conflict
        || !matches!(
            status.presence,
            ServicePresence::Owned | ServicePresence::CompatibleUnquoted
        )
    {
        return Err("legacy SCM service is not exactly owned and migration-safe".to_owned());
    }
    require_stable_migration_state(status.state)?;
    let service = open_for(ServiceAccess::Inspect)
        .map_err(|code| format!("OpenServiceW for migration snapshot failed with {code}"))?;
    let configuration = query_configuration(&service)
        .map_err(|code| format!("QueryServiceConfigW for migration snapshot failed with {code}"))?;
    let extended = query_extended_configuration(&service)?;
    let final_state = query_runtime(&service)
        .map_err(|code| format!("QueryServiceStatusEx for snapshot failed with {code}"))?;
    require_stable_migration_state(final_state)?;
    if final_state != status.state {
        return Err("legacy SCM service state changed while taking its snapshot".to_owned());
    }
    Ok(LegacyScmSnapshot {
        presence: status.presence,
        state: status.state,
        configuration,
        extended,
    })
}
