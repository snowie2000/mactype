use std::collections::BTreeMap;

use mactype_service_contract::{
    MigrationPinnedRuntime, MigrationRuntimePin, IMMUTABLE_RUNTIME_FILES,
};

fn runtime(version: &str) -> MigrationPinnedRuntime {
    let files = IMMUTABLE_RUNTIME_FILES
        .iter()
        .map(|name| {
            (
                (*name).to_owned(),
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_owned(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    MigrationPinnedRuntime::new(
        version.to_owned(),
        files,
        Some("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned()),
    )
    .unwrap()
}

#[test]
fn migration_pin_is_nonce_bound_and_contains_only_exact_hashed_runtime_files() {
    let pin = MigrationRuntimePin::new(
        "00112233445566778899aabbccddeeff".to_owned(),
        vec![runtime("0.1.0"), runtime("0.2.0")],
    )
    .unwrap();
    assert_eq!(pin.runtimes().len(), 2);
    assert!(pin.validate().is_ok());

    let mut unknown: serde_json::Value =
        serde_json::from_slice(&serde_json::to_vec(&pin).unwrap()).unwrap();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<MigrationRuntimePin>(unknown).is_err());

    assert!(MigrationRuntimePin::new("UPPERCASE".to_owned(), vec![runtime("0.1.0")]).is_err());
    assert!(MigrationRuntimePin::new(
        "00112233445566778899aabbccddeeff".to_owned(),
        vec![runtime("0.1.0"), runtime("0.1.0")],
    )
    .is_err());

    let mut incomplete: serde_json::Value = serde_json::to_value(runtime("0.1.0")).unwrap();
    incomplete["files"]
        .as_object_mut()
        .unwrap()
        .remove("MacType64.dll");
    let incomplete: MigrationPinnedRuntime = serde_json::from_value(incomplete).unwrap();
    assert!(incomplete.validate().is_err());
}
