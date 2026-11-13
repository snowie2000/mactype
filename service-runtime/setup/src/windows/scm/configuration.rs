use std::path::Path;

use mactype_service_contract::{
    ObservedServiceConfiguration, owned_service_identity, service_image_matches_protected_contract,
};
use mactype_service_platform::ServiceConfig;

use crate::SetupError;

mod metadata;

pub(super) use metadata::configure_metadata;

pub(super) fn observed_configuration(config: &ServiceConfig) -> ObservedServiceConfiguration<'_> {
    ObservedServiceConfiguration {
        service_type: config.service_type,
        start_type: config.start_type,
        error_control: config.error_control,
        image_path: &config.image_path,
        account: &config.account,
        display_name: &config.display_name,
        load_order_group: &config.load_order_group,
        tag_id: config.tag_id,
        dependencies: &config.dependencies,
    }
}

pub(super) fn quoted_image_path(service_binary: &Path) -> Result<String, SetupError> {
    let value = service_binary.to_string_lossy();
    if value.contains('"') {
        return Err(SetupError::Runtime(
            "service binary path contains a quote".to_owned(),
        ));
    }
    Ok(format!("\"{value}\" --service"))
}

pub(super) fn validate_service_binary(
    protected_root: &Path,
    path: &Path,
) -> Result<(), SetupError> {
    let image_path = quoted_image_path(path)?;
    if service_image_matches_protected_contract(protected_root, &image_path) {
        Ok(())
    } else {
        Err(SetupError::Runtime(
            "service binary does not match the protected fixed layout".to_owned(),
        ))
    }
}

pub(super) fn service_configuration_matches_owned_contract(
    protected_root: &Path,
    observed: &ObservedServiceConfiguration<'_>,
) -> bool {
    owned_service_identity(observed, protected_root)
        && !mactype_service_contract::service_configuration_drift(observed).is_drifted()
}

#[cfg(test)]
mod tests {
    use super::{ObservedServiceConfiguration, validate_service_binary};
    use mactype_service_contract::{
        FIXED_ERROR_CONTROL, FIXED_SERVICE_TYPE, FIXED_START_TYPE, service_configuration_drift,
    };
    use windows_sys::Win32::System::Services::{
        SERVICE_AUTO_START, SERVICE_ERROR_NORMAL, SERVICE_WIN32_OWN_PROCESS,
    };

    #[test]
    fn contract_service_constants_match_windows() {
        assert_eq!(FIXED_SERVICE_TYPE, SERVICE_WIN32_OWN_PROCESS);
        assert_eq!(FIXED_START_TYPE, SERVICE_AUTO_START);
        assert_eq!(FIXED_ERROR_CONTROL, SERVICE_ERROR_NORMAL);
    }

    #[test]
    fn service_binary_must_belong_to_the_exact_protected_generation_layout() {
        let base = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let protected_root = base.path().join("Service");
        let protected_binary = protected_root
            .join("bin")
            .join("0.2.0")
            .join("mactype-service.exe");
        let foreign_binary = base.path().join("outside").join("mactype-service.exe");
        std::fs::create_dir_all(protected_binary.parent().unwrap()).unwrap();
        std::fs::create_dir_all(foreign_binary.parent().unwrap()).unwrap();
        std::fs::write(&protected_binary, b"service").unwrap();
        std::fs::write(&foreign_binary, b"foreign").unwrap();

        assert!(validate_service_binary(&protected_root, &protected_binary).is_ok());
        assert!(validate_service_binary(&protected_root, &foreign_binary).is_err());
    }

    #[test]
    fn adapter_preserves_all_configuration_fields() {
        let dependencies = ["RpcSs".to_owned()];
        let observed = ObservedServiceConfiguration {
            service_type: FIXED_SERVICE_TYPE,
            start_type: 3,
            error_control: 0,
            image_path: "unused",
            account: "LocalSystem",
            display_name: "Foreign Display",
            load_order_group: "group",
            tag_id: 7,
            dependencies: &dependencies,
        };
        assert_eq!(
            service_configuration_drift(&observed).field_names(),
            [
                "start-type",
                "error-control",
                "display-name",
                "load-order-group",
                "tag",
                "dependencies",
            ]
        );
    }
}
