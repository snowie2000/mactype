use super::{LegacyServiceStatus, ServiceConfiguration, ServicePresence, ServiceRuntimeState};
use std::path::Path;

const MACTYPE_SERVICE_TYPE: u32 = 0x10;
const MACTYPE_SERVICE_START: u32 = 2;
// The migration parks the owned service disabled between the stop and the funeral
// so a reboot cannot auto-start it. Start type is a runtime setting, not an
// identity signal (the binary path, account, type, and dependencies pin
// identity), so a disabled service with the owned shape is still ours.
const MACTYPE_SERVICE_START_DISABLED: u32 = 4;
const MACTYPE_SERVICE_ERROR: u32 = 1;

pub(super) fn service_command_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    path.strip_prefix(r"\\?\")
        .unwrap_or(path.as_ref())
        .to_owned()
}

#[cfg(test)]
pub(super) fn owned_service_configuration(path: &Path) -> ServiceConfiguration {
    ServiceConfiguration {
        display_name: "MacType".to_owned(),
        binary_path: format!("\"{}\" -service", service_command_path(path)),
        service_type: MACTYPE_SERVICE_TYPE,
        start_type: MACTYPE_SERVICE_START,
        error_control: MACTYPE_SERVICE_ERROR,
        load_order_group: None,
        tag_id: 0,
        account: "LocalSystem".to_owned(),
        dependencies: vec!["winmgmt".to_owned()],
    }
}

pub(super) fn classify_configuration(
    configuration: &ServiceConfiguration,
    trusted: &Path,
) -> ServicePresence {
    if configuration.service_type != MACTYPE_SERVICE_TYPE
        || !matches!(
            configuration.start_type,
            MACTYPE_SERVICE_START | MACTYPE_SERVICE_START_DISABLED
        )
        || configuration.error_control != MACTYPE_SERVICE_ERROR
        || configuration.display_name != "MacType"
        || configuration.load_order_group.is_some()
        || configuration.tag_id != 0
        || !configuration.account.eq_ignore_ascii_case("LocalSystem")
        || configuration.dependencies.len() != 1
        || !configuration.dependencies[0].eq_ignore_ascii_case("winmgmt")
    {
        return ServicePresence::Foreign;
    }
    // Strip canonicalize's `\\?\` prefix before comparing with SCM ImagePath,
    // which normally omits it for the same executable.
    let trusted = service_command_path(trusted);
    let quoted = format!("\"{trusted}\" -service");
    let unquoted = format!("{trusted} -service");
    let actual = configuration.binary_path.trim();
    if actual.eq_ignore_ascii_case(&quoted) {
        ServicePresence::Owned
    } else if actual.eq_ignore_ascii_case(&unquoted) {
        ServicePresence::CompatibleUnquoted
    } else {
        ServicePresence::Foreign
    }
}

pub(super) fn with_capabilities(
    presence: ServicePresence,
    state: ServiceRuntimeState,
    binary_path: Option<String>,
    win32_error: Option<u32>,
    trusted_binary_available: bool,
    registry_conflict: bool,
) -> LegacyServiceStatus {
    let owned = matches!(
        presence,
        ServicePresence::Owned | ServicePresence::CompatibleUnquoted
    );
    LegacyServiceStatus {
        presence,
        state,
        binary_path,
        win32_error,
        trusted_binary_available,
        registry_conflict,
        can_remove: owned && state == ServiceRuntimeState::Stopped && !registry_conflict,
        can_stop: owned
            && state == ServiceRuntimeState::Running
            && trusted_binary_available
            && !registry_conflict,
    }
}

pub(super) fn status_from_configuration(
    configuration: &ServiceConfiguration,
    state: ServiceRuntimeState,
    expected_binary: Option<&Path>,
    trusted_binary_available: bool,
    registry_conflict: bool,
) -> LegacyServiceStatus {
    let presence = expected_binary
        .map(|path| classify_configuration(configuration, path))
        .unwrap_or(ServicePresence::Foreign);
    with_capabilities(
        presence,
        state,
        Some(configuration.binary_path.clone()),
        None,
        trusted_binary_available,
        registry_conflict,
    )
}

pub(super) fn is_trusted_mactray_layout(program_files: &Path, candidate: &Path) -> bool {
    let Ok(relative) = candidate.strip_prefix(program_files) else {
        return false;
    };
    let mut components = relative.components();
    let Some(mactype) = components.next() else {
        return false;
    };
    let Some(mactray) = components.next() else {
        return false;
    };
    components.next().is_none()
        && mactype
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("MacType")
        && mactray
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("MacTray.exe")
}
