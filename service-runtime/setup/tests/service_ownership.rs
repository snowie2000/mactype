#![cfg(all(windows, feature = "ci-test-adapter"))]

use std::fs;

use mactype_service_setup::{
    service_configuration_drift, service_configuration_matches_owned_contract,
    service_identity_matches_owned_contract, service_image_matches_protected_contract,
    ObservedServiceConfiguration,
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
    assert!(service_identity_matches_owned_contract(&root, &exact));
    assert!(service_configuration_drift(&exact).is_empty());
    assert_eq!(
        service_configuration_matches_owned_contract(&root, &exact),
        service_identity_matches_owned_contract(&root, &exact)
            && service_configuration_drift(&exact).is_empty()
    );

    let demand_start = ObservedServiceConfiguration {
        start_type: 3,
        ..exact
    };
    assert!(service_identity_matches_owned_contract(
        &root,
        &demand_start
    ));
    assert_eq!(service_configuration_drift(&demand_start), ["start-type"]);
    assert!(!service_configuration_matches_owned_contract(
        &root,
        &demand_start
    ));

    let display_drift = ObservedServiceConfiguration {
        display_name: "Foreign Display",
        ..exact
    };
    assert!(service_identity_matches_owned_contract(
        &root,
        &display_drift
    ));
    assert_eq!(
        service_configuration_drift(&display_drift),
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
        service_configuration_drift(&all_drift),
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
        assert!(!service_identity_matches_owned_contract(&root, &foreign));
    }
}
