use mactype_service_contract::appinit_mactype_conflict;

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[test]
fn appinit_conflicts_only_when_enabled_value_names_a_mactype_dll() {
    assert!(!appinit_mactype_conflict(true, Some(&wide(r"C:\Other\OtherHook.dll"))).unwrap());
    assert!(!appinit_mactype_conflict(false, Some(&wide(r"C:\MacType\MacType64.dll"))).unwrap());
    assert!(appinit_mactype_conflict(
        true,
        Some(&wide(
            r#""C:\Program Files\MacType\MacType64.dll",Other.dll"#
        )),
    )
    .unwrap());
    assert!(!appinit_mactype_conflict(true, None).unwrap());
    assert!(!appinit_mactype_conflict(true, Some(&[0])).unwrap());
}

#[test]
fn enabled_malformed_appinit_value_fails_closed() {
    assert!(appinit_mactype_conflict(true, Some(&wide("Other.dll\0MacType.dll"))).is_err());
    assert!(appinit_mactype_conflict(true, Some(&[b'M' as u16, b'a' as u16])).is_err());
    assert!(appinit_mactype_conflict(true, Some(&[0xD800, 0])).is_err());
}
