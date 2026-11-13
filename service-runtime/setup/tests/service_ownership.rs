#![cfg(all(windows, feature = "ci-test-adapter"))]

use std::fs;

use mactype_service_contract::{
    ObservedServiceConfiguration, owned_service_identity, service_configuration_drift,
    service_image_matches_protected_contract,
};

#[test]
fn only_the_fixed_service_binary_below_the_protected_runtime_is_owned() {
    let base = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let root = base
        .path()
        .join("Program Files")
        .join("MacType Control Center")
        .join("Service");
    let binary = root.join("bin").join("0.2.0").join("mactype-service.exe");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, b"service").unwrap();

    assert!(service_image_matches_protected_contract(
        &root,
        &format!(r#""{}" --service"#, binary.display())
    ));
    assert!(!service_image_matches_protected_contract(
        &root,
        &format!(r#""{}" --service OtherName"#, binary.display())
    ));
    assert!(!service_image_matches_protected_contract(
        &root,
        &format!(r#""{}"  --service"#, binary.display())
    ));
    assert!(!service_image_matches_protected_contract(
        &root,
        &format!(r#""{}" --service "#, binary.display())
    ));
    assert!(!service_image_matches_protected_contract(
        &root,
        &format!(r#"{} --service"#, binary.display())
    ));
    assert!(!service_image_matches_protected_contract(
        &root,
        r#""C:\Program Files\MacType\MacTray.exe" -service"#
    ));
    assert!(!service_image_matches_protected_contract(
        &root,
        r#""C:\Users\person\AppData\Local\mactype-service.exe" --service"#
    ));
}

#[test]
fn service_identity_is_separate_from_repairable_configuration_drift() {
    let base = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let root = base
        .path()
        .join("Program Files")
        .join("MacType Control Center")
        .join("Service");
    let binary = root.join("bin").join("0.2.0").join("mactype-service.exe");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, b"service").unwrap();
    let image = format!(r#""{}" --service"#, binary.display());
    let dependencies = ["winmgmt".to_owned()];
    let exact = ObservedServiceConfiguration {
        service_type: 0x10,
        start_type: 2,
        error_control: 1,
        image_path: &image,
        account: "LocalSystem",
        display_name: "MacType Control Center Service",
        load_order_group: "",
        tag_id: 0,
        dependencies: &[],
    };
    assert!(owned_service_identity(&exact, &root));
    assert!(!service_configuration_drift(&exact).is_drifted());

    let demand_start = ObservedServiceConfiguration {
        start_type: 3,
        ..exact
    };
    assert!(owned_service_identity(&demand_start, &root));
    assert_eq!(
        service_configuration_drift(&demand_start).field_names(),
        ["start-type"]
    );

    let display_drift = ObservedServiceConfiguration {
        display_name: "Foreign Display",
        ..exact
    };
    assert!(owned_service_identity(&display_drift, &root));
    assert_eq!(
        service_configuration_drift(&display_drift).field_names(),
        ["display-name"]
    );

    let all_drift = ObservedServiceConfiguration {
        start_type: 3,
        error_control: 0,
        display_name: "Foreign Display",
        load_order_group: "group",
        tag_id: 1,
        dependencies: &dependencies,
        ..exact
    };
    assert_eq!(
        service_configuration_drift(&all_drift).field_names(),
        [
            "start-type",
            "error-control",
            "display-name",
            "load-order-group",
            "tag",
            "dependencies"
        ]
    );

    for foreign in [
        ObservedServiceConfiguration {
            service_type: 0x20,
            ..exact
        },
        ObservedServiceConfiguration {
            account: "LocalService",
            ..exact
        },
        ObservedServiceConfiguration {
            image_path: r#""C:\Windows\System32\cmd.exe" /c exit 0"#,
            ..exact
        },
    ] {
        assert!(!owned_service_identity(&foreign, &root));
    }
}
