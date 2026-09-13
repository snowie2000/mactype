use super::{
    super::*,
    common::{
        expected_mactray_path, open_for, query_configuration, query_runtime, trusted_mactray_path,
        win32_code,
    },
};
use mactype_service_platform::{
    ServiceAccess, ServiceControlManager, ServiceCreation, ServiceManagerAccess,
};
use std::{thread, time::Duration};
use windows_sys::Win32::{
    Foundation::{
        ERROR_DUPLICATE_SERVICE_NAME, ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SERVICE_EXISTS,
        ERROR_SERVICE_MARKED_FOR_DELETE,
    },
    System::Services::SERVICE_DISABLED,
};

fn inaccessible(code: u32, trusted: bool, registry: bool) -> LegacyServiceStatus {
    with_capabilities(
        ServicePresence::Inaccessible,
        ServiceRuntimeState::Unknown,
        None,
        Some(code),
        trusted,
        registry,
    )
}

pub(super) fn query(registry_conflict: bool) -> LegacyServiceStatus {
    let expected = expected_mactray_path();
    let trusted_available = trusted_mactray_path().is_some();
    let manager = match ServiceControlManager::connect(ServiceManagerAccess::Connect) {
        Ok(manager) => manager,
        Err(error) => {
            return inaccessible(win32_code(&error), trusted_available, registry_conflict)
        }
    };
    let service = match manager.open("MacType", ServiceAccess::QueryStatusAndConfig) {
        Ok(Some(service)) => service,
        Ok(None) => {
            return with_capabilities(
                ServicePresence::Absent,
                ServiceRuntimeState::Unknown,
                None,
                None,
                trusted_available,
                registry_conflict,
            )
        }
        Err(error) => {
            let code = win32_code(&error);
            let presence = if code == ERROR_SERVICE_MARKED_FOR_DELETE {
                ServicePresence::DeletePending
            } else {
                ServicePresence::Inaccessible
            };
            return with_capabilities(
                presence,
                ServiceRuntimeState::Unknown,
                None,
                (presence == ServicePresence::Inaccessible).then_some(code),
                trusted_available,
                registry_conflict,
            );
        }
    };
    let state = match query_runtime(&service) {
        Ok(state) => state,
        Err(code) => return inaccessible(code, trusted_available, registry_conflict),
    };
    let configuration = match query_configuration(&service) {
        Ok(configuration) => configuration,
        Err(code) => return inaccessible(code, trusted_available, registry_conflict),
    };
    status_from_configuration(
        &configuration,
        state,
        expected.as_deref(),
        trusted_available,
        registry_conflict,
    )
}

fn wait_for(target: ServiceRuntimeState) -> Result<(), String> {
    for _ in 0..120 {
        let status = query(false);
        if status.state == target
            || (target == ServiceRuntimeState::Stopped
                && status.presence == ServicePresence::Absent)
        {
            return Ok(());
        }
        if matches!(
            status.presence,
            ServicePresence::Foreign | ServicePresence::Inaccessible
        ) {
            return Err("legacy service changed to an unsafe state".to_owned());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err("legacy service operation timed out after 30 seconds".to_owned())
}

fn wait_until_absent() -> Result<(), String> {
    for _ in 0..120 {
        let status = query(crate::machine_integration::registry_conflict_detected());
        match status.presence {
            ServicePresence::Absent => return Ok(()),
            ServicePresence::Owned
            | ServicePresence::CompatibleUnquoted
            | ServicePresence::DeletePending => {}
            ServicePresence::Foreign | ServicePresence::Inaccessible => {
                return Err("legacy service changed to an unsafe state".to_owned());
            }
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err("legacy service removal timed out after 30 seconds".to_owned())
}

pub(super) fn start() -> Result<(), String> {
    let service = open_for(ServiceAccess::Start)
        .map_err(|code| format!("OpenServiceW failed with {code}"))?;
    service
        .start()
        .map_err(|error| format!("StartServiceW failed with {}", win32_code(&error)))?;
    drop(service);
    wait_for(ServiceRuntimeState::Running)
}

pub(super) fn stop() -> Result<(), String> {
    let service =
        open_for(ServiceAccess::Stop).map_err(|code| format!("OpenServiceW failed with {code}"))?;
    service
        .stop()
        .map_err(|error| format!("ControlService failed with {}", win32_code(&error)))?;
    drop(service);
    wait_for(ServiceRuntimeState::Stopped)
}

pub(super) fn create_service_configuration(
    configuration: &ServiceConfiguration,
) -> Result<(), String> {
    let manager = ServiceControlManager::connect(ServiceManagerAccess::ConnectAndCreate).map_err(
        |error| {
            format!(
                "OpenSCManagerW for service creation failed with {}",
                win32_code(&error)
            )
        },
    )?;
    let creation = ServiceCreation {
        display_name: &configuration.display_name,
        service_type: configuration.service_type,
        start_type: configuration.start_type,
        error_control: configuration.error_control,
        image_path: &configuration.binary_path,
        load_order_group: configuration.load_order_group.as_deref(),
        dependencies: &configuration.dependencies,
        account: &configuration.account,
    };
    // Start is the closest fixed platform rights set to the legacy
    // QUERY_CONFIG | QUERY_STATUS | START | STOP creation handle. The handle is
    // immediately dropped, so the missing STOP right is never exercised.
    let service = match manager.create("MacType", &creation, ServiceAccess::Start) {
        Ok(service) => service,
        Err(error) => {
            let code = win32_code(&error);
            if matches!(code, ERROR_SERVICE_EXISTS | ERROR_DUPLICATE_SERVICE_NAME)
                && matches!(
                    query(false).presence,
                    ServicePresence::Owned | ServicePresence::CompatibleUnquoted
                )
            {
                return Ok(());
            }
            return Err(format!("CreateServiceW failed with {code}"));
        }
    };
    drop(service);
    let created = query(false);
    if matches!(
        created.presence,
        ServicePresence::Owned | ServicePresence::CompatibleUnquoted
    ) {
        Ok(())
    } else {
        Err("Windows created a MacType service with unexpected configuration".to_owned())
    }
}

fn delete_owned_service() -> Result<(), String> {
    let service = match open_for(ServiceAccess::Delete) {
        Ok(service) => service,
        Err(ERROR_SERVICE_DOES_NOT_EXIST | ERROR_SERVICE_MARKED_FOR_DELETE) => {
            return wait_until_absent()
        }
        Err(code) => return Err(format!("OpenServiceW for deletion failed with {code}")),
    };
    if let Err(error) = service.delete() {
        let code = win32_code(&error);
        if !matches!(
            code,
            ERROR_SERVICE_DOES_NOT_EXIST | ERROR_SERVICE_MARKED_FOR_DELETE
        ) {
            return Err(format!("DeleteService failed with {code}"));
        }
    }
    drop(service);
    wait_until_absent()
}

// Change only the start type of the owned legacy service, leaving every other
// field untouched (SERVICE_NO_CHANGE). Used to park the legacy service disabled
// between migration and its funeral, and to re-enable it on restore.
fn set_start_type(start_type: u32) -> Result<(), String> {
    let service = open_for(ServiceAccess::ChangeStartType)
        .map_err(|code| format!("OpenServiceW for start-type change failed with {code}"))?;
    service.set_start_type(start_type).map_err(|error| {
        format!(
            "ChangeServiceConfigW(start type {start_type}) failed with {}",
            win32_code(&error)
        )
    })
}

pub(super) fn migration_stop() -> Result<(), String> {
    let status = query(false);
    if !matches!(
        status.presence,
        ServicePresence::Owned | ServicePresence::CompatibleUnquoted
    ) {
        return Err("only an owned legacy service can be stopped for migration".to_owned());
    }
    require_stable_migration_state(status.state)?;
    if status.state == ServiceRuntimeState::Running {
        stop()?;
    }
    // Park the legacy service disabled so a reboot between the migration and the
    // funeral cannot auto-start it alongside the new service (double injection).
    // The original start type is preserved in the migration backup receipt and
    // is put back by restore_service_configuration and migration_restore_running_state.
    set_start_type(SERVICE_DISABLED)
}

pub(super) fn migration_remove() -> Result<(), String> {
    let status = query(false);
    if !matches!(
        status.presence,
        ServicePresence::Owned | ServicePresence::CompatibleUnquoted
    ) || status.state != ServiceRuntimeState::Stopped
    {
        return Err("legacy service must be owned and stopped before removal".to_owned());
    }
    delete_owned_service()
}

pub(super) fn migration_restore_running_state(snapshot: &LegacyScmSnapshot) -> Result<(), String> {
    require_stable_migration_state(snapshot.state)?;
    let current = query(false);
    if !matches!(
        current.presence,
        ServicePresence::Owned | ServicePresence::CompatibleUnquoted
    ) {
        return Err("only an owned legacy service can have its runtime state restored".to_owned());
    }
    require_stable_migration_state(current.state)?;
    // Undo migration_stop's disable before touching the runtime state; a disabled
    // service cannot be started. This is a no-op when the full configuration
    // restore already put the original start type back.
    set_start_type(snapshot.configuration.start_type)?;
    match (snapshot.state, current.state) {
        (ServiceRuntimeState::Running, ServiceRuntimeState::Stopped) => start(),
        (ServiceRuntimeState::Stopped, ServiceRuntimeState::Running) => stop(),
        (ServiceRuntimeState::Running, ServiceRuntimeState::Running)
        | (ServiceRuntimeState::Stopped, ServiceRuntimeState::Stopped) => Ok(()),
        _ => unreachable!(),
    }
}
