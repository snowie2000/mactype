use super::{
    super::*,
    common::{open_for, query_configuration, win32_code},
    control::{create_service_configuration, query, stop},
    snapshot::{
        query_extended_configuration, service_has_triggers, MAX_FAILURE_ACTIONS,
        MAX_REQUIRED_PRIVILEGES, MAX_SECURITY_DESCRIPTOR_BYTES,
    },
};
use mactype_service_platform::{
    FailureAction as PlatformFailureAction, FailureActionKind,
    FailureActions as PlatformFailureActions, PrivilegeGuard, SelfRelativeSecurityDescriptor,
    ServiceAccess, ServiceConfig, ServiceHandle,
};

use windows_sys::Win32::System::Services::{
    SERVICE_CONFIG_DELAYED_AUTO_START_INFO, SERVICE_CONFIG_DESCRIPTION,
    SERVICE_CONFIG_FAILURE_ACTIONS, SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
    SERVICE_CONFIG_PRESHUTDOWN_INFO, SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO,
    SERVICE_CONFIG_SERVICE_SID_INFO, SERVICE_CONFIG_TRIGGER_INFO,
};

fn validate_snapshot_string(name: &str, value: &str) -> Result<(), String> {
    if value.contains('\0') || value.encode_utf16().count() > 32_767 {
        Err(format!("legacy SCM {name} is not safely restorable"))
    } else {
        Ok(())
    }
}

fn validated_security_descriptor_snapshot(
    snapshot: &SecurityDescriptorSnapshot,
) -> Result<SelfRelativeSecurityDescriptor, String> {
    if snapshot.self_relative.is_empty()
        || snapshot.self_relative.len() > MAX_SECURITY_DESCRIPTOR_BYTES
    {
        return Err("legacy SCM security descriptor size is invalid".to_owned());
    }
    SelfRelativeSecurityDescriptor::from_bytes(
        snapshot.self_relative.clone(),
        MAX_SECURITY_DESCRIPTOR_BYTES,
    )
    .map_err(|_| "legacy SCM security descriptor is invalid".to_owned())
}

fn validate_security_descriptor_snapshot(
    snapshot: &SecurityDescriptorSnapshot,
) -> Result<(), String> {
    validated_security_descriptor_snapshot(snapshot).map(drop)
}

pub(super) fn validate_snapshot_for_restore(snapshot: &LegacyScmSnapshot) -> Result<(), String> {
    require_stable_migration_state(snapshot.state)?;
    let configuration = &snapshot.configuration;
    for (name, value) in [
        ("display name", configuration.display_name.as_str()),
        ("binary path", configuration.binary_path.as_str()),
        ("account", configuration.account.as_str()),
    ] {
        validate_snapshot_string(name, value)?;
    }
    if let Some(load_order_group) = &configuration.load_order_group {
        if load_order_group.is_empty() {
            return Err("legacy SCM load-order group must use None for an empty value".to_owned());
        }
        validate_snapshot_string("load-order group", load_order_group)?;
    }
    for dependency in &configuration.dependencies {
        validate_snapshot_string("dependency", dependency)?;
    }

    let extended = &snapshot.extended;
    if let Some(description) = &extended.description {
        if description.is_empty() {
            return Err("legacy SCM description must use None for an empty value".to_owned());
        }
        validate_snapshot_string("description", description)?;
    }
    for (name, value) in [
        (
            "failure reboot message",
            extended.failure_actions.reboot_message.as_deref(),
        ),
        (
            "failure command",
            extended.failure_actions.command.as_deref(),
        ),
    ] {
        if let Some(value) = value {
            validate_snapshot_string(name, value)?;
        }
    }
    if extended.failure_actions.actions.len() > MAX_FAILURE_ACTIONS
        || extended
            .failure_actions
            .actions
            .iter()
            .any(|action| FailureActionKind::from_raw(action.action_type).is_none())
    {
        return Err("legacy SCM failure actions are not safely restorable".to_owned());
    }
    if !matches!(extended.service_sid_type, 0 | 1 | 3)
        || extended.required_privileges.len() > MAX_REQUIRED_PRIVILEGES
    {
        return Err("legacy SCM SID or privilege configuration is invalid".to_owned());
    }
    for privilege in &extended.required_privileges {
        validate_snapshot_string("required privilege", privilege)?;
    }
    validate_security_descriptor_snapshot(&extended.security_descriptor)
}

fn config2_result(result: std::io::Result<()>, information_level: u32) -> Result<(), String> {
    result.map_err(|error| {
        format!(
            "ChangeServiceConfig2W({information_level}) failed with {}",
            win32_code(&error)
        )
    })
}

struct WindowsServiceConfigurationRestorer<'a> {
    service: &'a ServiceHandle,
    snapshot: &'a LegacyScmSnapshot,
}

impl ServiceConfigurationRestorer for WindowsServiceConfigurationRestorer<'_> {
    fn restore(&mut self, step: ServiceRestoreStep) -> Result<(), String> {
        let configuration = &self.snapshot.configuration;
        let extended = &self.snapshot.extended;
        match step {
            ServiceRestoreStep::Core => {
                let configuration = ServiceConfig {
                    service_type: configuration.service_type,
                    start_type: configuration.start_type,
                    error_control: configuration.error_control,
                    image_path: configuration.binary_path.clone(),
                    account: configuration.account.clone(),
                    display_name: configuration.display_name.clone(),
                    load_order_group: configuration.load_order_group.clone().unwrap_or_default(),
                    tag_id: configuration.tag_id,
                    dependencies: configuration.dependencies.clone(),
                };
                self.service.set_config(&configuration).map_err(|error| {
                    format!("ChangeServiceConfigW failed with {}", win32_code(&error))
                })
            }
            ServiceRestoreStep::Description => config2_result(
                self.service
                    .set_optional_description(extended.description.as_deref()),
                SERVICE_CONFIG_DESCRIPTION,
            ),
            ServiceRestoreStep::FailureActions => {
                let actions = extended
                    .failure_actions
                    .actions
                    .iter()
                    .map(|action| {
                        let kind =
                            FailureActionKind::from_raw(action.action_type).ok_or_else(|| {
                                "legacy service has an unsupported failure action".to_owned()
                            })?;
                        Ok(PlatformFailureAction {
                            kind,
                            delay_ms: action.delay_ms,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let actions = PlatformFailureActions {
                    reset_period_seconds: extended.failure_actions.reset_period_seconds,
                    reboot_message: extended.failure_actions.reboot_message.clone(),
                    command: extended.failure_actions.command.clone(),
                    actions,
                };
                config2_result(
                    self.service.set_failure_actions(&actions),
                    SERVICE_CONFIG_FAILURE_ACTIONS,
                )
            }
            ServiceRestoreStep::FailureActionsFlag => config2_result(
                self.service.set_failure_actions_on_non_crash_failures(
                    extended.failure_actions_on_non_crash,
                ),
                SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
            ),
            ServiceRestoreStep::DelayedAutoStart => config2_result(
                self.service
                    .set_delayed_auto_start(extended.delayed_auto_start),
                SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
            ),
            ServiceRestoreStep::ServiceSidType => config2_result(
                self.service.set_service_sid_type(extended.service_sid_type),
                SERVICE_CONFIG_SERVICE_SID_INFO,
            ),
            ServiceRestoreStep::RequiredPrivileges => config2_result(
                self.service
                    .set_required_privileges(&extended.required_privileges),
                SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO,
            ),
            ServiceRestoreStep::PreshutdownTimeout => config2_result(
                self.service
                    .set_preshutdown_timeout_ms(extended.preshutdown_timeout_ms),
                SERVICE_CONFIG_PRESHUTDOWN_INFO,
            ),
            ServiceRestoreStep::Triggers => {
                // Clearing triggers on a service that has none returns
                // ERROR_INVALID_PARAMETER (87) on Windows Server builds. The
                // snapshot model only ever represents "no triggers", so only
                // issue the clear when the live service actually has some.
                if service_has_triggers(self.service)? {
                    config2_result(self.service.clear_triggers(), SERVICE_CONFIG_TRIGGER_INFO)
                } else {
                    Ok(())
                }
            }
            ServiceRestoreStep::SecurityDescriptor => {
                let descriptor =
                    validated_security_descriptor_snapshot(&extended.security_descriptor)?;
                // Best-effort: if the owner is already a held SID the write
                // succeeds without the privilege, and the real error still
                // surfaces below when it is genuinely needed but unavailable.
                let guard = PrivilegeGuard::enable("SeRestorePrivilege").ok();
                let applied = self.service.set_object_security(&descriptor);
                drop(guard);
                applied.map_err(|error| {
                    format!(
                        "SetServiceObjectSecurity failed with {}",
                        win32_code(&error)
                    )
                })
            }
        }
    }
}

pub(super) fn restore_service_configuration(snapshot: &LegacyScmSnapshot) -> Result<(), String> {
    validate_snapshot_for_restore(snapshot)?;
    let status = query(false);
    match status.presence {
        ServicePresence::Absent => create_service_configuration(&snapshot.configuration)?,
        ServicePresence::Owned | ServicePresence::CompatibleUnquoted => {
            require_stable_migration_state(status.state)?;
            let current = open_for(ServiceAccess::Inspect)
                .map_err(|code| format!("OpenServiceW for restore preflight failed with {code}"))?;
            query_extended_configuration(&current)?;
            drop(current);
            if status.state != ServiceRuntimeState::Stopped {
                stop()?;
            }
        }
        ServicePresence::Foreign
        | ServicePresence::DeletePending
        | ServicePresence::Inaccessible => {
            return Err("refusing to overwrite an unsafe MacType SCM service".to_owned());
        }
    }
    let verified = query(false);
    if !matches!(
        verified.presence,
        ServicePresence::Owned | ServicePresence::CompatibleUnquoted
    ) || verified.state != ServiceRuntimeState::Stopped
    {
        return Err("legacy SCM service changed before configuration restore".to_owned());
    }
    let preflight = open_for(ServiceAccess::Inspect)
        .map_err(|code| format!("OpenServiceW for final restore preflight failed with {code}"))?;
    query_extended_configuration(&preflight)?;
    drop(preflight);
    let service = open_for(ServiceAccess::Restore)
        .map_err(|code| format!("OpenServiceW for restore failed with {code}"))?;
    let mut restorer = WindowsServiceConfigurationRestorer {
        service: &service,
        snapshot,
    };
    perform_service_configuration_restore(&mut restorer)?;
    let restored_configuration = query_configuration(&service).map_err(|code| {
        format!("QueryServiceConfigW after configuration restore failed with {code}")
    })?;
    let restored_extended = query_extended_configuration(&service)?;
    verify_restored_configuration(snapshot, &restored_configuration, &restored_extended)
}
