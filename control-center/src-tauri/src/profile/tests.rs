use super::*;
use encoding_rs::{Encoding, BIG5, GB18030, SHIFT_JIS, WINDOWS_1252};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_profile(bytes: &[u8]) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("mactype-profile-{unique}.ini"));
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn legacy_alternative_file_can_be_parsed_from_already_verified_bytes() {
    let bytes = b"[General]\r\nAlternativeFile=ini\\Community.ini\r\n";

    assert_eq!(
        legacy_alternative_file_bytes(bytes).unwrap(),
        Some(PathBuf::from(r"ini\Community.ini"))
    );
}

#[test]
fn unchanged_utf8_profile_round_trips_byte_for_byte() {
    let bytes = b"\xEF\xBB\xBF; keep\r\n[General]\r\nUnknown = 7\r\nNormalWeight = 2  \r\n";
    let path = temp_profile(bytes);
    let document = ProfileDocument::open(&path).unwrap();
    assert_eq!(document.encoded().unwrap(), bytes);
    let _ = fs::remove_file(path);
}

#[test]
fn opening_a_profile_rejects_files_larger_than_the_service_contract() {
    let oversized = vec![b';'; mactype_service_contract::MAX_PROFILE_BYTES + 1];
    let path = temp_profile(&oversized);

    let error = ProfileDocument::open(&path).unwrap_err();

    assert!(error.contains("byte limit"), "unexpected error: {error}");
    let _ = fs::remove_file(path);
}

#[test]
fn default_profile_payload_rejects_an_oversized_profile() {
    let oversized = vec![b';'; mactype_service_contract::MAX_PROFILE_BYTES + 1];
    let path = temp_profile(&oversized);

    let error = legacy::default_profile_payload_from(path.clone()).unwrap_err();

    assert!(error.contains("byte limit"), "unexpected error: {error}");
    let _ = fs::remove_file(path);
}

#[test]
fn profile_listing_rejects_more_entries_than_the_ui_contract() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("mactype-profile-list-{unique}"));
    let profile_root = root.join("ini");
    fs::create_dir_all(&profile_root).unwrap();
    for index in 0..513 {
        fs::write(
            profile_root.join(format!("profile-{index}.ini")),
            b"[General]\nA=1\n",
        )
        .unwrap();
    }

    let error = commands::list_profile_paths_from(&root, None).unwrap_err();

    assert!(error.contains("512 entries"), "unexpected error: {error}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn default_profile_discovery_stops_at_the_directory_contract() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("mactype-default-list-{unique}"));
    let profile_root = root.join("ini");
    fs::create_dir_all(&profile_root).unwrap();
    for index in 0..513 {
        fs::write(profile_root.join(format!("entry-{index}.txt")), b"ignored").unwrap();
    }

    let error = legacy::find_default_profile_at(&root).unwrap_err();

    assert!(error.contains("512 entries"), "unexpected error: {error}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn default_profile_discovery_is_deterministic_without_a_bundled_default() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("mactype-default-order-{unique}"));
    let profile_root = root.join("ini");
    fs::create_dir_all(&profile_root).unwrap();
    fs::write(profile_root.join("zeta.ini"), b"[General]\nA=1\n").unwrap();
    fs::write(profile_root.join("notes.txt"), b"ignored").unwrap();
    fs::write(profile_root.join("Alpha.ini"), b"[General]\nA=1\n").unwrap();
    fs::write(root.join("MacType.ini"), b"[General]\nA=1\n").unwrap();

    let picked = legacy::find_default_profile_at(&root).unwrap().unwrap();
    assert_eq!(picked, profile_root.join("Alpha.ini"));

    fs::write(profile_root.join("Default.ini"), b"[General]\nA=1\n").unwrap();
    let picked = legacy::find_default_profile_at(&root).unwrap().unwrap();
    assert_eq!(picked, profile_root.join("Default.ini"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn changing_one_key_preserves_comments_order_and_unknown_lines() {
    let bytes = b"; keep\r\n[General]\r\nUnknown = 7\r\nNormalWeight = 2  \r\n# tail\r\n";
    let path = temp_profile(bytes);
    let mut document = ProfileDocument::open(&path).unwrap();
    document.set_value("normal_weight", 4.0).unwrap();
    let rendered = String::from_utf8(document.encoded().unwrap()).unwrap();
    assert_eq!(
        rendered,
        "; keep\r\n[General]\r\nUnknown = 7\r\nNormalWeight = 4  \r\n# tail\r\n"
    );
    let _ = fs::remove_file(path);
}

#[test]
fn detects_external_change_before_save() {
    let path = temp_profile(b"[General]\nNormalWeight=0\n");
    let mut document = ProfileDocument::open(&path).unwrap();
    document.set_value("normal_weight", 3.0).unwrap();
    fs::write(&path, b"[General]\nNormalWeight=9\n").unwrap();
    assert!(document.save().unwrap_err().contains("changed on disk"));
    let _ = fs::remove_file(path);
}

#[test]
fn profile_apply_rejects_unsaved_edits() {
    let path = temp_profile(b"[General]\nNormalWeight=0\n");
    let state = ProfileState::default();
    state.set(ProfileDocument::open(&path).unwrap()).unwrap();
    commands::update_profile_setting("normal_weight".to_owned(), 3.0, &state).unwrap();

    let error = state.active_payload().unwrap_err();

    assert!(
        error.contains("save profile changes"),
        "unexpected error: {error}"
    );
    let _ = fs::remove_file(path);
}

#[test]
fn external_profile_save_requires_import_or_save_as() {
    let path = temp_profile(b"[General]\nNormalWeight=0\n");
    let state = ProfileState::default();
    state.set(ProfileDocument::open(&path).unwrap()).unwrap();
    commands::update_profile_setting("normal_weight".to_owned(), 3.0, &state).unwrap();

    let error = match commands::save_profile(&state) {
        Ok(_) => panic!("an external profile must not be saved in place"),
        Err(error) => error,
    };

    assert!(
        error.contains("imported or saved as"),
        "unexpected error: {error}"
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "[General]\nNormalWeight=0\n"
    );
    let _ = fs::remove_file(path);
}

#[test]
fn save_rejects_an_oversized_external_replacement_before_hashing_it() {
    let path = temp_profile(b"[General]\nNormalWeight=0\n");
    let mut document = ProfileDocument::open(&path).unwrap();
    document.set_value("normal_weight", 3.0).unwrap();
    fs::write(
        &path,
        vec![b';'; mactype_service_contract::MAX_PROFILE_BYTES + 1],
    )
    .unwrap();

    let error = document.save().unwrap_err();

    assert!(error.contains("byte limit"), "unexpected error: {error}");
    let _ = fs::remove_file(path);
}

#[test]
fn edited_profile_cannot_serialize_past_the_read_contract() {
    let path = temp_profile(b"[General]\nNormalWeight=0\n");
    let mut document = ProfileDocument::open(&path).unwrap();
    let suffix = "x".repeat(1024);
    let entries = (0..5000)
        .map(|index| format!("font-{index}-{suffix}"))
        .collect::<Vec<_>>();
    document.set_list("excludeFonts", entries).unwrap();

    match document.encoded() {
        Err(error) => assert!(error.contains("byte limit"), "unexpected error: {error}"),
        Ok(_) => panic!("oversized edited profile serialized successfully"),
    }
    let _ = fs::remove_file(path);
}

#[test]
fn profile_history_undoes_redoes_and_clears_after_save() {
    let path = temp_profile(b"[General]\nNormalWeight=0\n");
    let mut document = ProfileDocument::open(&path).unwrap();

    document.update_value("normal_weight", 3.0).unwrap();
    let changed = document.snapshot();
    assert_eq!(changed.values.get("normal_weight"), Some(&3.0));
    assert!(changed.can_undo);
    assert!(!changed.can_redo);

    assert!(document.undo());
    let undone = document.snapshot();
    assert_eq!(undone.values.get("normal_weight"), Some(&0.0));
    assert!(undone.dirty_keys.is_empty());
    assert!(undone.can_redo);

    assert!(document.redo());
    assert_eq!(document.snapshot().values.get("normal_weight"), Some(&3.0));
    document.save().unwrap();
    let saved = document.snapshot();
    assert!(!saved.can_undo);
    assert!(!saved.can_redo);
    assert!(saved.dirty_keys.is_empty());
    let _ = fs::remove_file(path);
}

#[test]
fn export_includes_unsaved_edits_without_clearing_them() {
    let path = temp_profile(b"[General]\nNormalWeight=0\n");
    let mut document = ProfileDocument::open(&path).unwrap();
    document.update_value("normal_weight", 6.0).unwrap();
    let destination = path.with_extension("export.ini");

    document.export_to(&destination).unwrap();

    assert!(String::from_utf8(fs::read(&destination).unwrap())
        .unwrap()
        .contains("NormalWeight=6"));
    let snapshot = document.snapshot();
    assert_eq!(snapshot.path, path.to_string_lossy());
    assert!(snapshot.dirty_keys.contains(&"normal_weight".to_owned()));
    assert!(snapshot.can_undo);
    let _ = fs::remove_file(destination);
    let _ = fs::remove_file(path);
}

#[test]
fn preserves_utf16le_bom_and_line_endings() {
    let text = "[General]\r\nGammaValue=1.2\r\n";
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let path = temp_profile(&bytes);
    let mut document = ProfileDocument::open(&path).unwrap();
    document.set_value("gamma_value", 1.4).unwrap();
    let encoded = document.encoded().unwrap();
    assert!(encoded.starts_with(&[0xFF, 0xFE]));
    let _ = fs::remove_file(path);
}

#[test]
fn new_section_is_inserted_before_its_first_key() {
    let path = temp_profile(b"; empty profile\n");
    let mut document = ProfileDocument::open(&path).unwrap();
    document.set_value("normal_weight", 5.0).unwrap();
    document.set_value("normal_weight", 6.0).unwrap();
    let rendered = String::from_utf8(document.encoded().unwrap()).unwrap();
    assert_eq!(rendered, "; empty profile\n[General]\nNormalWeight=6\n");
    let _ = fs::remove_file(path);
}

#[test]
fn snapshot_reports_saved_values_until_save_refreshes_them() {
    let path = temp_profile(b"[General]\nNormalWeight=3\n");
    let mut document = ProfileDocument::open(&path).unwrap();
    document.update_value("normal_weight", 7.0).unwrap();

    let snapshot = document.snapshot();
    assert_eq!(snapshot.values["normal_weight"], 7.0);
    assert_eq!(snapshot.saved_values["normal_weight"], 3.0);

    document.save().unwrap();
    let snapshot = document.snapshot();
    assert_eq!(snapshot.saved_values["normal_weight"], 7.0);
    let _ = fs::remove_file(path.with_extension("ini.bak"));
    let _ = fs::remove_file(path);
}

#[test]
fn reset_settings_to_defaults_writes_factory_values_as_one_undoable_revision() {
    let path = temp_profile(b"[General]\n; note\nNormalWeight=3\nGammaMode=-1\nItalicSlant=7\n");
    let mut document = ProfileDocument::open(&path).unwrap();
    document.reset_settings_to_defaults().unwrap();

    let rendered = String::from_utf8(document.encoded().unwrap()).unwrap();
    assert!(rendered.contains("; note"));
    assert!(rendered.contains("NormalWeight=16"));
    assert!(rendered.contains("GammaMode=0"));
    assert!(rendered.contains("GammaValue=1.5"));
    assert!(rendered.contains("HintingMode=1"));
    assert!(rendered.contains("ItalicSlant=0"));
    let snapshot = document.snapshot();
    assert_eq!(snapshot.values["normal_weight"], 16.0);
    assert_eq!(snapshot.values["gamma_mode"], 0.0);
    assert_eq!(snapshot.values["contrast"], 1.7);
    assert!(snapshot.dirty_keys.contains(&"normal_weight".to_owned()));

    assert!(document.undo());
    let reverted = String::from_utf8(document.encoded().unwrap()).unwrap();
    assert!(reverted.contains("NormalWeight=3"));
    assert!(!reverted.contains("HintingMode"));
    assert!(!document.undo());
    let _ = fs::remove_file(path);
}

#[test]
fn reset_settings_to_defaults_twice_is_a_no_op_the_second_time() {
    let path = temp_profile(b"[Exclude]\nTahoma\n");
    let mut document = ProfileDocument::open(&path).unwrap();
    document.reset_settings_to_defaults().unwrap();
    document.reset_settings_to_defaults().unwrap();
    assert_eq!(document.snapshot().values["hinting_mode"], 1.0);
    assert!(document.undo());
    assert!(!document.undo());
    let _ = fs::remove_file(path);
}

#[test]
fn edits_individual_fonts_and_lists_without_dropping_comments() {
    let path =
        temp_profile(b"[Individual]\n; keep\nSegoe UI=1,2,3,4,5,1\n[Exclude]\n; fonts\nTahoma\n");
    let mut document = ProfileDocument::open(&path).unwrap();
    document
        .set_individuals(vec![IndividualSetting {
            font_face: "Malgun Gothic".to_owned(),
            values: vec![Some(1), Some(2), None, Some(4), None, Some(1)],
        }])
        .unwrap();
    document
        .set_list("excludeFonts", vec!["Arial".to_owned(), "Arial".to_owned()])
        .unwrap();
    let rendered = String::from_utf8(document.encoded().unwrap()).unwrap();
    assert!(rendered.contains("; keep\nMalgun Gothic=1,2,,4,,1\n"));
    assert!(rendered.contains("; fonts\nArial\n"));
    assert!(!rendered.contains("Tahoma"));
    let _ = fs::remove_file(path);
}

#[test]
fn duplicate_preserves_encoded_profile_and_refuses_overwrite() {
    let path = temp_profile(b"[General]\nNormalWeight=2\n");
    let document = ProfileDocument::open(&path).unwrap();
    let name = format!(
        "mactype-copy-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let parent = path.parent().unwrap();
    let mut copy = document.duplicate_in(parent, &name).unwrap();
    assert_eq!(copy.encoded().unwrap(), document.encoded().unwrap());
    assert!(document
        .duplicate_in(parent, &name)
        .unwrap_err()
        .contains("already exists"));
    copy.set_value("normal_weight", 7.0).unwrap();
    copy.save().unwrap();
    let reopened = ProfileDocument::open(&copy.path).unwrap();
    assert_eq!(reopened.snapshot().values.get("normal_weight"), Some(&7.0));
    let _ = fs::remove_file(copy.path);
    let _ = fs::remove_file(path);
}

fn legacy_round_trip(codec: &'static Encoding, expected: TextEncoding, comment: &str) {
    let source = format!("; {comment}\r\n[General]\r\nNormalWeight=2\r\nUnknown=유지\r\n")
        .replace("Unknown=유지\r\n", "Unknown=keep\r\n");
    let (bytes, _, had_errors) = codec.encode(&source);
    assert!(!had_errors);
    let path = temp_profile(bytes.as_ref());
    let mut document = ProfileDocument::open(&path).unwrap();
    assert_eq!(document.encoding, expected, "decoded text: {source}");
    assert_eq!(document.encoded().unwrap(), bytes.as_ref());
    document.set_value("normal_weight", 7.0).unwrap();
    let changed = document.encoded().unwrap();
    let (decoded, _, decode_errors) = codec.decode(&changed);
    assert!(!decode_errors);
    assert!(decoded.contains(comment));
    assert!(decoded.contains("NormalWeight=7"));
    fs::write(&path, &changed).unwrap();
    let reopened = ProfileDocument::open(&path).unwrap();
    assert_eq!(reopened.encoding, expected);
    assert_eq!(reopened.encoded().unwrap(), changed);
    let _ = fs::remove_file(path);
}

#[test]
fn gb18030_profile_round_trips_after_edit() {
    legacy_round_trip(GB18030, TextEncoding::Gb18030, "简体中文配置与字体设置");
}

#[test]
fn big5_profile_round_trips_after_edit() {
    legacy_round_trip(BIG5, TextEncoding::Big5, "繁體中文設定與字型調整");
}

#[test]
fn shift_jis_profile_round_trips_after_edit() {
    legacy_round_trip(SHIFT_JIS, TextEncoding::ShiftJis, "日本語プロファイル設定");
}

#[test]
fn windows_1252_profile_round_trips_after_edit() {
    legacy_round_trip(
        WINDOWS_1252,
        TextEncoding::Windows1252,
        "Profil français: qualité élevée",
    );
}

#[test]
fn discovers_the_profile_selected_by_legacy_mactype() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("mactype-discovery-{unique}"));
    let ini = root.join("ini");
    fs::create_dir_all(&ini).unwrap();
    fs::write(
        root.join("MacType.ini"),
        b"[General]\r\nAlternativeFile=ini\\Community.ini\r\n",
    )
    .unwrap();
    fs::write(
        ini.join("Community.ini"),
        b"[General]\r\nNormalWeight=2\r\n",
    )
    .unwrap();

    let candidate = discover_legacy_profile_at(&root).unwrap().unwrap();
    assert_eq!(candidate.name, "Community");
    assert_eq!(PathBuf::from(candidate.path), ini.join("Community.ini"));
    assert_eq!(candidate.source, "alternative-file");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discovers_mactype_ini_when_no_alternative_profile_is_selected() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("mactype-primary-discovery-{unique}"));
    fs::create_dir_all(&root).unwrap();
    let configuration = root.join("MacType.ini");
    fs::write(
        &configuration,
        b"[General]\r\nNormalWeight=2\r\nHookChildProcesses=1\r\n",
    )
    .unwrap();

    let candidate = discover_legacy_profile_at(&root).unwrap().unwrap();
    assert_eq!(candidate.name, "MacType");
    assert_eq!(PathBuf::from(candidate.path), configuration);
    assert_eq!(candidate.source, "primary-file");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn installs_a_managed_system_profile_without_losing_the_source_identity() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("mactype-system-profile-{unique}"));
    fs::create_dir_all(root.join("ini")).unwrap();
    fs::write(
        root.join("MacType.ini"),
        b"; keep this comment\r\n[General]\r\nAlternativeFile=ini\\Default.ini\r\nLoadType=1\r\n",
    )
    .unwrap();
    let source = Path::new(r"C:\Users\Test\profiles\Pretendard forever.ini");
    let bytes = b"[General]\r\nNormalWeight=7\r\n";

    install_system_profile_at(&root, source, bytes).unwrap();

    assert_eq!(
        fs::read(root.join("ini").join("ControlCenter.ini")).unwrap(),
        bytes
    );
    let configuration = ProfileDocument::open(root.join("MacType.ini")).unwrap();
    assert_eq!(
        configuration.raw_value("General", "AlternativeFile"),
        Some(r"ini\ControlCenter.ini")
    );
    assert_eq!(
        configuration.raw_value("General", "ControlCenterSourceProfile"),
        Some(source.to_string_lossy().as_ref())
    );
    assert_eq!(configuration.raw_value("General", "LoadType"), Some("1"));
    let candidate = discover_legacy_profile_at(&root).unwrap().unwrap();
    assert_eq!(candidate.name, "Pretendard forever");
    assert_eq!(
        PathBuf::from(candidate.path),
        root.join("ini").join("ControlCenter.ini")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn imported_profile_preserves_bytes_and_avoids_name_collisions() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("mactype-import-{unique}"));
    let source_root = root.join("source");
    let destination = root.join("profiles");
    fs::create_dir_all(&source_root).unwrap();
    let source = source_root.join("Community.ini");
    let bytes = b"; untouched\r\n[General]\r\nNormalWeight=2  \r\n";
    fs::write(&source, bytes).unwrap();

    let first = import_profile_to(&source, &destination).unwrap();
    let second = import_profile_to(&source, &destination).unwrap();
    assert_eq!(first.path(), destination.join("Community.ini"));
    assert_eq!(second.path(), destination.join("Community (2).ini"));
    assert_eq!(fs::read(first.path()).unwrap(), bytes);
    assert_eq!(fs::read(second.path()).unwrap(), bytes);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn import_rejects_a_profile_larger_than_the_service_contract() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("mactype-import-limit-{unique}"));
    let source = root.join("Oversized.ini");
    let destination = root.join("profiles");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        &source,
        vec![b';'; mactype_service_contract::MAX_PROFILE_BYTES + 1],
    )
    .unwrap();

    let error = import_profile_to(&source, &destination).unwrap_err();

    assert!(error.contains("byte limit"), "unexpected error: {error}");
    assert!(
        !destination.exists(),
        "an oversized import must fail before creating a destination"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn supported_advanced_edits_preserve_unsupported_profile_entries() {
    let source = b"[FreeType]\r\nNormalWeight=3\r\nLcdFilterWeight=8,77,86,77,8\r\nPixelLayout=-21,0,0,0,21,0\r\n[General]\r\nNormalWeight=2\r\nShadow=1,2,4,112233,5,AABBCC\r\nDisplayAffinity=0,2\r\n[FontSubstitutes]\r\nArial=Segoe UI\r\n[Infinality]\r\nINFINALITY_FT_GAMMA_CORRECTION=0 100\r\nINFINALITY_FT_FILTER_PARAMS=11 22 38 22 11\r\n";
    let path = temp_profile(source);
    let mut document = ProfileDocument::open(&path).unwrap();
    let snapshot = document.snapshot();
    assert_eq!(snapshot.values.get("normal_weight"), Some(&3.0));
    assert_eq!(
        snapshot.advanced.lcd_filter_weight,
        Some(vec![8, 77, 86, 77, 8])
    );
    assert_eq!(
        snapshot.advanced.pixel_layout,
        Some(vec![-21, 0, 0, 0, 21, 0])
    );
    assert_eq!(snapshot.advanced.font_substitutes, vec!["Arial=Segoe UI"]);
    document.set_value("normal_weight", 7.0).unwrap();
    document
        .set_advanced(AdvancedProfile {
            shadow: Some(ShadowSetting {
                offset_x: -2,
                offset_y: 3,
                dark_alpha: 6,
                dark_color: 0x010203,
                light_alpha: 7,
                light_color: 0xA0B0C0,
            }),
            lcd_filter_weight: Some(vec![1, 2, 3, 4, 5]),
            pixel_layout: Some(vec![-20, 0, 0, 0, 20, 0]),
            font_substitutes: vec!["Tahoma=Segoe UI".to_owned()],
        })
        .unwrap();
    document
        .set_list("unloadDlls", vec!["example.dll".to_owned()])
        .unwrap();
    document
        .set_list("excludeSubstitutionModules", vec!["legacy.exe".to_owned()])
        .unwrap();
    let rendered = String::from_utf8(document.encoded().unwrap()).unwrap();
    assert!(rendered.contains("[FreeType]\r\nNormalWeight=7"));
    assert!(rendered.contains("NormalWeight=2"));
    assert!(rendered.contains("Shadow=-2,3,6,010203,7,A0B0C0"));
    assert!(rendered.contains("LcdFilterWeight=1,2,3,4,5"));
    assert!(rendered.contains("PixelLayout=-20,0,0,0,20,0"));
    assert!(rendered.contains("DisplayAffinity=0,2"));
    assert!(rendered.contains("Tahoma=Segoe UI"));
    assert!(rendered.contains("INFINALITY_FT_GAMMA_CORRECTION=0 100"));
    assert!(rendered.contains("INFINALITY_FT_FILTER_PARAMS=11 22 38 22 11"));
    assert!(rendered.contains("[UnloadDLL]\r\nexample.dll"));
    assert!(rendered.contains("[ExcludeSub]\r\nlegacy.exe"));
    let _ = fs::remove_file(path);
}

#[test]
fn whitespace_delimited_shadow_is_parsed_without_panicking() {
    let path = temp_profile(b"[General]\r\nShadow=1 2 3 010203 4 112233\r\n");
    let document = ProfileDocument::open(&path).unwrap();
    let shadow = document.snapshot().advanced.shadow.unwrap();

    assert_eq!(shadow.offset_x, 1);
    assert_eq!(shadow.offset_y, 2);
    assert_eq!(shadow.dark_alpha, 3);
    assert_eq!(shadow.dark_color, 0x010203);
    assert_eq!(shadow.light_alpha, 4);
    assert_eq!(shadow.light_color, 0x112233);
    let _ = fs::remove_file(path);
}

#[test]
fn rejected_advanced_edit_is_transactional() {
    let path = temp_profile(b"[General]\r\nShadow=1,2,3,010203,4,A0B0C0\r\n");
    let mut document = ProfileDocument::open(&path).unwrap();
    let before = document.encoded().unwrap();
    let mut advanced = document.snapshot().advanced;
    advanced.shadow = None;
    advanced.font_substitutes = vec!["missing separator".to_owned()];
    assert!(document.set_advanced(advanced).is_err());
    assert_eq!(document.encoded().unwrap(), before);
    let _ = fs::remove_file(path);
}

#[test]
fn rejected_advanced_list_entry_is_transactional() {
    let path = temp_profile(b"[General]\r\nShadow=1,2,3,010203,4,A0B0C0\r\n");
    let mut document = ProfileDocument::open(&path).unwrap();
    let before = document.encoded().unwrap();
    let mut advanced = document.snapshot().advanced;
    advanced.shadow = None;
    advanced.font_substitutes = vec![";invalid=Segoe UI".to_owned()];

    assert!(document.set_advanced(advanced).is_err());
    assert_eq!(document.encoded().unwrap(), before);
    let _ = fs::remove_file(path);
}

#[test]
fn rejected_trimmed_advanced_list_entry_is_transactional() {
    let path = temp_profile(b"[General]\r\nShadow=1,2,3,010203,4,A0B0C0\r\n");
    let mut document = ProfileDocument::open(&path).unwrap();
    let before = document.encoded().unwrap();
    let mut advanced = document.snapshot().advanced;
    advanced.shadow = None;
    advanced.font_substitutes = vec!["  ;invalid=Segoe UI".to_owned()];

    assert!(document.set_advanced(advanced).is_err());
    assert_eq!(document.encoded().unwrap(), before);
    let _ = fs::remove_file(path);
}
