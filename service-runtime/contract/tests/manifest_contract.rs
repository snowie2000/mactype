use std::collections::BTreeMap;

use mactype_service_contract::{
    runtime_generation_id, sha256_digest, verify_runtime_manifest, ManifestError,
    IMMUTABLE_RUNTIME_FILES,
};

fn package() -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        ("mactype-service.exe".to_owned(), b"service binary".to_vec()),
        (
            "mactype-injector32.exe".to_owned(),
            b"32-bit injector".to_vec(),
        ),
        (
            "mactype-injector64.exe".to_owned(),
            b"64-bit injector".to_vec(),
        ),
        ("MacType.dll".to_owned(), b"32-bit core".to_vec()),
        ("MacType64.dll".to_owned(), b"64-bit core".to_vec()),
    ])
}

fn manifest_for(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let hashes = files
        .iter()
        .map(|(name, bytes)| (name.clone(), sha256_digest(bytes)))
        .collect::<BTreeMap<_, _>>();
    serde_json::to_vec(&serde_json::json!({
        "schema": 1,
        "version": "0.2.0",
        "files": hashes,
    }))
    .unwrap()
}

#[test]
fn manifest_accepts_only_allowlisted_files_with_matching_sha256() {
    let files = package();
    let verified = verify_runtime_manifest(&manifest_for(&files), &files).unwrap();

    assert_eq!(verified.version(), "0.2.0");
    assert_eq!(verified.files().len(), 5);
}

#[test]
fn manifest_rejects_an_incomplete_runtime_even_when_every_present_file_is_valid() {
    let mut incomplete = package();
    incomplete.remove("mactype-injector32.exe");

    assert!(verify_runtime_manifest(&manifest_for(&incomplete), &incomplete).is_err());
}

#[test]
fn manifest_rejects_every_empty_executable_and_dll_even_when_its_hash_matches() {
    for name in IMMUTABLE_RUNTIME_FILES {
        let mut files = package();
        files.insert(name.to_owned(), Vec::new());

        assert_eq!(
            verify_runtime_manifest(&manifest_for(&files), &files),
            Err(ManifestError::FileEmpty),
            "{name} must never be accepted as an empty fixed payload"
        );
    }
}

#[test]
fn manifest_rejects_hash_mismatch_missing_service_and_extra_files() {
    let files = package();
    let mut wrong_hash = manifest_for(&files);
    let text = String::from_utf8(wrong_hash.clone()).unwrap();
    wrong_hash = text
        .replace(&sha256_digest(b"service binary"), &sha256_digest(b"other"))
        .into_bytes();
    assert!(verify_runtime_manifest(&wrong_hash, &files).is_err());

    let helper_only = BTreeMap::from([("mactype-injector64.exe".to_owned(), b"helper".to_vec())]);
    assert!(verify_runtime_manifest(&manifest_for(&helper_only), &helper_only).is_err());

    let declared = package();
    let mut package_with_extra = declared.clone();
    package_with_extra.insert("unsigned.exe".to_owned(), b"extra".to_vec());
    assert!(verify_runtime_manifest(&manifest_for(&declared), &package_with_extra).is_err());
}

#[test]
fn manifest_rejects_unknown_names_schema_fields_and_noncanonical_hashes() {
    let unknown = BTreeMap::from([("arbitrary.dll".to_owned(), b"payload".to_vec())]);
    assert!(verify_runtime_manifest(&manifest_for(&unknown), &unknown).is_err());

    let files = package();
    let hash = sha256_digest(b"service binary").to_uppercase();
    let noncanonical = serde_json::to_vec(&serde_json::json!({
        "schema": 1,
        "version": "0.2.0",
        "files": {"mactype-service.exe": hash},
    }))
    .unwrap();
    assert!(verify_runtime_manifest(&noncanonical, &files).is_err());

    let unknown_field = serde_json::to_vec(&serde_json::json!({
        "schema": 1,
        "version": "0.2.0",
        "files": {"mactype-service.exe": sha256_digest(b"service binary")},
        "serviceName": "OtherService",
    }))
    .unwrap();
    assert!(verify_runtime_manifest(&unknown_field, &files).is_err());
}

#[test]
fn runtime_generation_id_has_one_canonical_five_file_contract() {
    assert_eq!(
        IMMUTABLE_RUNTIME_FILES,
        [
            "mactype-service.exe",
            "mactype-injector32.exe",
            "mactype-injector64.exe",
            "MacType.dll",
            "MacType64.dll",
        ]
    );
    let files = BTreeMap::from([
        ("mactype-service.exe".to_owned(), b"service".to_vec()),
        ("mactype-injector32.exe".to_owned(), b"injector32".to_vec()),
        ("mactype-injector64.exe".to_owned(), b"injector64".to_vec()),
        ("MacType.dll".to_owned(), b"dll32".to_vec()),
        ("MacType64.dll".to_owned(), b"dll64".to_vec()),
    ]);

    assert_eq!(
        runtime_generation_id(&files).unwrap(),
        "b3af6edc5954a4fcd814410720c32571eaa38673622979b92c9bd6cc7854e90e"
    );
    let mut changed_service = files.clone();
    changed_service.insert("mactype-service.exe".to_owned(), b"changed".to_vec());
    assert_ne!(
        runtime_generation_id(&changed_service).unwrap(),
        runtime_generation_id(&files).unwrap()
    );
    let mut extra = files;
    extra.insert("unsigned.dll".to_owned(), b"extra".to_vec());
    assert!(runtime_generation_id(&extra).is_err());
}
