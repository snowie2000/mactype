use std::fs;

use mactype_service_contract::{
    FIXED_ACCOUNT, FIXED_ERROR_CONTROL, FIXED_SERVICE_TYPE, FIXED_START_TYPE,
    ObservedServiceConfiguration, SERVICE_DISPLAY_NAME, ServiceConfigurationDrift,
    owned_service_identity, service_configuration_drift, service_image_matches_protected_contract,
    service_image_matches_protected_layout,
};

struct TemporaryDirectory(std::path::PathBuf);

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn protected_service() -> (TemporaryDirectory, std::path::PathBuf, String) {
    let base = std::env::temp_dir().join(format!(
        "mactype-service-contract-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&base);
    let root = base.join("Service");
    let binary = root.join("bin").join("0.2.0").join("mactype-service.exe");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, b"service").unwrap();
    let image = format!(r#""{}" --service"#, binary.display());
    (TemporaryDirectory(base), root, image)
}

fn exact<'a>(image_path: &'a str) -> ObservedServiceConfiguration<'a> {
    ObservedServiceConfiguration {
        service_type: FIXED_SERVICE_TYPE,
        start_type: FIXED_START_TYPE,
        error_control: FIXED_ERROR_CONTROL,
        image_path,
        account: FIXED_ACCOUNT,
        display_name: SERVICE_DISPLAY_NAME,
        load_order_group: "",
        tag_id: 0,
        dependencies: &[],
    }
}

#[test]
fn ownership_requires_the_fixed_type_account_and_protected_image() {
    let (_base, root, image) = protected_service();
    let owned = exact(&image);
    assert!(owned_service_identity(&owned, &root));
    assert!(service_image_matches_protected_layout(&root, &image));
    assert!(service_image_matches_protected_contract(&root, &image));

    for foreign in [
        ObservedServiceConfiguration {
            service_type: 0x20,
            ..owned
        },
        ObservedServiceConfiguration {
            account: "LocalService",
            ..owned
        },
        ObservedServiceConfiguration {
            image_path: r#""C:\Windows\System32\cmd.exe" /c exit 0"#,
            ..owned
        },
    ] {
        assert!(!owned_service_identity(&foreign, &root));
    }

    for image_path in [
        format!(r#""{}" --service OtherName"#, root.display()),
        format!(r#""{}"  --service"#, root.display()),
        format!(r#""{}" --service "#, root.display()),
        format!(r#"{} --service"#, root.display()),
    ] {
        assert!(!service_image_matches_protected_layout(&root, &image_path));
        assert!(!service_image_matches_protected_contract(
            &root,
            &image_path,
        ));
    }
}

#[test]
fn missing_binary_keeps_lexical_identity_but_does_not_exist() {
    let base = std::env::temp_dir().join(format!(
        "mactype-service-contract-missing-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&base);
    let root = base.join("Service");
    let binary = root.join("bin").join("0.2.0").join("mactype-service.exe");
    let image = format!(r#""{}" --service"#, binary.display());
    let _cleanup = TemporaryDirectory(base);
    let observed = exact(&image);

    assert!(service_image_matches_protected_layout(&root, &image));
    assert!(owned_service_identity(&observed, &root));
    assert!(!service_image_matches_protected_contract(&root, &image));
}

#[test]
fn invalid_version_component_matches_neither_rule() {
    let base = std::env::temp_dir().join(format!(
        "mactype-service-contract-version-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&base);
    let root = base.join("Service");
    let binary = root
        .join("bin")
        .join("invalid_version")
        .join("mactype-service.exe");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, b"service").unwrap();
    let image = format!(r#""{}" --service"#, binary.display());
    let _cleanup = TemporaryDirectory(base);

    assert!(!service_image_matches_protected_layout(&root, &image));
    assert!(!owned_service_identity(&exact(&image), &root));
    assert!(!service_image_matches_protected_contract(&root, &image));
}

#[test]
fn canonicalization_escape_does_not_count_as_an_existing_protected_binary() {
    let base = std::env::temp_dir().join(format!(
        "mactype-service-contract-escape-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&base);
    let root = base.join("Service");
    let outside = base.join("outside-generation");
    let linked_generation = root.join("bin").join("0.2.0");
    fs::create_dir_all(linked_generation.parent().unwrap()).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("mactype-service.exe"), b"service").unwrap();
    create_directory_link(&outside, &linked_generation);
    let binary = linked_generation.join("mactype-service.exe");
    let image = format!(r#""{}" --service"#, binary.display());
    let _cleanup = TemporaryDirectory(base);

    assert!(service_image_matches_protected_layout(&root, &image));
    assert!(!service_image_matches_protected_contract(&root, &image));
}

#[cfg(windows)]
fn create_directory_link(target: &std::path::Path, link: &std::path::Path) {
    let status = std::process::Command::new("cmd")
        .arg("/c")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(unix)]
fn create_directory_link(target: &std::path::Path, link: &std::path::Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[test]
fn drift_reports_each_fixed_configuration_field() {
    let dependencies = ["RpcSs".to_owned()];
    let cases = [
        (
            ObservedServiceConfiguration {
                start_type: 3,
                ..exact("unused")
            },
            ServiceConfigurationDrift {
                start_type: true,
                ..ServiceConfigurationDrift::default()
            },
        ),
        (
            ObservedServiceConfiguration {
                error_control: 0,
                ..exact("unused")
            },
            ServiceConfigurationDrift {
                error_control: true,
                ..ServiceConfigurationDrift::default()
            },
        ),
        (
            ObservedServiceConfiguration {
                display_name: "Foreign Display",
                ..exact("unused")
            },
            ServiceConfigurationDrift {
                display_name: true,
                ..ServiceConfigurationDrift::default()
            },
        ),
        (
            ObservedServiceConfiguration {
                load_order_group: "group",
                ..exact("unused")
            },
            ServiceConfigurationDrift {
                load_order_group: true,
                ..ServiceConfigurationDrift::default()
            },
        ),
        (
            ObservedServiceConfiguration {
                load_order_group: "group",
                tag_id: 7,
                ..exact("unused")
            },
            ServiceConfigurationDrift {
                load_order_group: true,
                tag: true,
                ..ServiceConfigurationDrift::default()
            },
        ),
        (
            ObservedServiceConfiguration {
                dependencies: &dependencies,
                ..exact("unused")
            },
            ServiceConfigurationDrift {
                dependencies: true,
                ..ServiceConfigurationDrift::default()
            },
        ),
    ];

    assert!(!service_configuration_drift(&exact("unused")).is_drifted());
    for (observed, expected) in cases {
        let actual = service_configuration_drift(&observed);
        assert_eq!(actual, expected);
        assert!(actual.is_drifted());
    }
}

#[test]
fn a_tag_without_a_load_order_group_is_inert() {
    let observed = ObservedServiceConfiguration {
        tag_id: 7,
        ..exact("unused")
    };
    assert_eq!(
        service_configuration_drift(&observed),
        ServiceConfigurationDrift::default()
    );
}

#[test]
fn field_names_keep_the_setup_event_order() {
    let dependencies = ["RpcSs".to_owned()];
    let observed = ObservedServiceConfiguration {
        start_type: 3,
        error_control: 0,
        display_name: "Foreign Display",
        load_order_group: "group",
        tag_id: 7,
        dependencies: &dependencies,
        ..exact("unused")
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
