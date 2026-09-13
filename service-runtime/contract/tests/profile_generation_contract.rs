use mactype_service_contract::{
    GenerationPointer, ProfileCatalog, ProfileError, SourceMetadata, MAX_PROFILE_BYTES,
};

fn profile(gamma: &str) -> Vec<u8> {
    format!("[General]\r\nGammaValue={gamma}\r\n").into_bytes()
}

#[test]
fn generation_pointer_contains_only_schema_and_digest() {
    let generation = mactype_service_contract::GenerationId::parse(
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )
    .unwrap();
    let pointer = GenerationPointer::new(generation.clone());
    assert_eq!(
        serde_json::to_value(&pointer).unwrap(),
        serde_json::json!({"schema": 1, "generation": generation.as_str()})
    );
    assert!(
        serde_json::from_value::<GenerationPointer>(serde_json::json!({
            "schema": 1,
            "generation": generation.as_str(),
            "profilePath": r"C:\Users\person\profile.ini"
        }))
        .is_err()
    );
}

fn metadata(name: &str) -> SourceMetadata {
    SourceMetadata {
        display_name: name.to_owned(),
    }
}

#[test]
fn profile_generations_publish_activate_and_rollback_by_content_digest() {
    let mut catalog = ProfileCatalog::new();
    let first = catalog
        .publish_machine_profile(&profile("1.0"), metadata("first"))
        .unwrap();
    let duplicate = catalog
        .publish_machine_profile(&profile("1.0"), metadata("same bytes"))
        .unwrap();
    assert_eq!(duplicate, first);
    assert_eq!(first.as_str().len(), 71);

    catalog.activate_machine_generation(&first).unwrap();
    assert_eq!(catalog.active(), Some(&first));
    assert_eq!(catalog.previous(), None);

    let second = catalog
        .publish_machine_profile(&profile("1.2"), metadata("second"))
        .unwrap();
    catalog.activate_machine_generation(&second).unwrap();
    assert_eq!(catalog.active(), Some(&second));
    assert_eq!(catalog.previous(), Some(&first));

    let rolled_back = catalog.rollback_machine_generation().unwrap();
    assert_eq!(rolled_back, first);
    assert_eq!(catalog.active(), Some(&first));
    assert_eq!(catalog.previous(), Some(&second));
}

#[test]
fn failed_activation_and_rollback_leave_the_current_generation_unchanged() {
    let mut catalog = ProfileCatalog::new();
    let active = catalog
        .publish_machine_profile(&profile("1.0"), metadata("active"))
        .unwrap();
    catalog.activate_machine_generation(&active).unwrap();

    let unknown = mactype_service_contract::GenerationId::parse(
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )
    .unwrap();
    assert_eq!(
        catalog.activate_machine_generation(&unknown),
        Err(ProfileError::UnknownGeneration)
    );
    assert_eq!(catalog.active(), Some(&active));
    assert_eq!(catalog.previous(), None);

    assert_eq!(
        catalog.rollback_machine_generation(),
        Err(ProfileError::NoPreviousGeneration)
    );
    assert_eq!(catalog.active(), Some(&active));
}

#[test]
fn invalid_or_oversized_profiles_are_rejected_before_publication() {
    let mut catalog = ProfileCatalog::new();
    assert_eq!(
        catalog.publish_machine_profile(&[], metadata("empty")),
        Err(ProfileError::InvalidSize)
    );
    assert_eq!(
        catalog.publish_machine_profile(b"not an ini", metadata("bad")),
        Err(ProfileError::InvalidIni)
    );
    assert_eq!(
        catalog.publish_machine_profile(&vec![b'x'; MAX_PROFILE_BYTES + 1], metadata("oversized")),
        Err(ProfileError::InvalidSize)
    );
    assert!(catalog.active().is_none());
}

#[test]
fn legacy_east_asian_profile_bytes_remain_opaque_when_ini_structure_is_valid() {
    let mut bytes = b"[General]\r\nFontSubstitutes=".to_vec();
    bytes.extend_from_slice(&[0x81, 0x40, 0xa4, 0x40, 0xb0, 0xa1]);
    bytes.extend_from_slice(b"\r\n");

    let mut catalog = ProfileCatalog::new();
    let generation = catalog
        .publish_machine_profile(&bytes, metadata("legacy"))
        .unwrap();
    assert_eq!(catalog.profile_bytes(&generation).unwrap(), bytes);
}

#[test]
fn mactype_list_sections_accept_bare_entries_without_weakening_other_sections() {
    for section in [
        "Exclude",
        "Include",
        "ExcludeModule",
        "IncludeModule",
        "UnloadDLL",
        "ExcludeSub",
    ] {
        let profile = format!("[General]\r\nGammaValue=1.0\r\n[{section}]\r\nfontview.exe\r\n");
        let mut catalog = ProfileCatalog::new();
        catalog
            .publish_machine_profile(profile.as_bytes(), metadata(section))
            .unwrap();
    }

    for profile in [
        b"[General]\r\nGammaValue=1.0\r\nfontview.exe\r\n".as_slice(),
        b"[General]\r\nGammaValue=1.0\r\n[Unknown]\r\nfontview.exe\r\n".as_slice(),
    ] {
        let mut catalog = ProfileCatalog::new();
        assert_eq!(
            catalog.publish_machine_profile(profile, metadata("invalid bare entry")),
            Err(ProfileError::InvalidIni)
        );
    }
}

#[test]
fn bundled_default_profile_satisfies_the_service_profile_contract() {
    let profile = include_bytes!("../../../distribution/ini/Default.ini");
    let mut catalog = ProfileCatalog::new();
    catalog
        .publish_machine_profile(profile, metadata("bundled default"))
        .unwrap();
}

#[test]
fn utf8_bom_is_ignored_for_structure_but_preserved_in_generation_bytes() {
    let plain = profile("1.25");
    let mut with_bom = vec![0xef, 0xbb, 0xbf];
    with_bom.extend_from_slice(&plain);
    let mut catalog = ProfileCatalog::new();

    let generation = catalog
        .publish_machine_profile(&with_bom, metadata("UTF-8 BOM"))
        .unwrap();
    let plain_generation = catalog
        .publish_machine_profile(&plain, metadata("plain UTF-8"))
        .unwrap();

    assert_ne!(generation, plain_generation);
    assert_eq!(catalog.profile_bytes(&generation).unwrap(), with_bom);
}
