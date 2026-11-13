//! The bytes the MacType runtime reads as its profile.
//!
//! IniParser decodes a profile by its byte order mark, and a profile without
//! one in the ANSI code page of the process that reads it. A BOM-less profile
//! with non-ASCII text therefore means one thing in a process on the system
//! code page and another in a process whose manifest selects UTF-8 (LibreOffice
//! does), and Hangul font names in its substitution rules stop matching there.
//! The runtime always gets a profile it decodes the same way everywhere: bytes
//! with a BOM or pure ASCII pass through unchanged, so their generation digest
//! stays the same, and anything else is re-encoded as UTF-8 with a BOM. A
//! BOM-less profile that is not UTF-8 is read in the system ANSI code page,
//! the code page every ordinary process already decoded it with. The user's
//! own file keeps its encoding.

use super::codec::decode;
use encoding_rs::Encoding;

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

fn decodes_everywhere(bytes: &[u8]) -> bool {
    bytes.is_ascii()
        || bytes.starts_with(&UTF8_BOM)
        || bytes.starts_with(&[0xFF, 0xFE])
        || bytes.starts_with(&[0xFE, 0xFF])
}

fn utf8_with_bom(text: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(UTF8_BOM.len() + text.len());
    bytes.extend_from_slice(&UTF8_BOM);
    bytes.extend_from_slice(text.as_bytes());
    if bytes.len() > mactype_service_contract::MAX_PROFILE_BYTES {
        return Err(format!(
            "runtime profile exceeds its {}-byte limit once encoded as UTF-8",
            mactype_service_contract::MAX_PROFILE_BYTES
        ));
    }
    Ok(bytes)
}

fn code_page_encoding(code_page: u32) -> Option<&'static Encoding> {
    let label = match code_page {
        932 => "shift_jis".to_owned(),
        936 => "gbk".to_owned(),
        949 => "euc-kr".to_owned(),
        950 => "big5".to_owned(),
        874 | 1250..=1258 => format!("windows-{code_page}"),
        _ => return None,
    };
    Encoding::for_label(label.as_bytes())
}

fn runtime_bytes_in(bytes: Vec<u8>, code_page: Option<u32>) -> Result<Vec<u8>, String> {
    if decodes_everywhere(&bytes) {
        return Ok(bytes);
    }
    if let Ok(text) = std::str::from_utf8(&bytes) {
        return utf8_with_bom(text);
    }
    if let Some(text) = code_page
        .and_then(code_page_encoding)
        .and_then(|codec| codec.decode_without_bom_handling_and_without_replacement(&bytes))
    {
        return utf8_with_bom(&text);
    }
    let (text, _, _) = decode(&bytes)?;
    utf8_with_bom(&text)
}

#[cfg(windows)]
fn system_code_page() -> Option<u32> {
    mactype_service_platform::system_ansi_code_page().ok()
}

#[cfg(not(windows))]
fn system_code_page() -> Option<u32> {
    None
}

/// The bytes the runtime reads for a profile whose bytes on disk are `bytes`.
pub(crate) fn runtime_profile_bytes(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    runtime_bytes_in(bytes, system_code_page())
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding_rs::{EUC_KR, SHIFT_JIS};

    const PROFILE: &str = "[FontSubstitutes]
맑은 고딕=Pretendard
";

    fn euc_kr() -> Vec<u8> {
        let (legacy, _, errors) = EUC_KR.encode(PROFILE);
        assert!(!errors);
        legacy.into_owned()
    }

    #[test]
    fn ascii_profiles_pass_through_unchanged() {
        let bytes = b"[General]
NormalWeight=0
"
        .to_vec();
        assert_eq!(runtime_bytes_in(bytes.clone(), Some(949)).unwrap(), bytes);
    }

    #[test]
    fn profiles_with_a_bom_pass_through_unchanged() {
        let mut utf8 = UTF8_BOM.to_vec();
        utf8.extend_from_slice(PROFILE.as_bytes());
        assert_eq!(runtime_bytes_in(utf8.clone(), Some(949)).unwrap(), utf8);

        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend(PROFILE.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(runtime_bytes_in(utf16.clone(), Some(949)).unwrap(), utf16);
    }

    #[test]
    fn bom_less_utf8_gains_a_bom() {
        let converted = runtime_bytes_in(PROFILE.as_bytes().to_vec(), Some(949)).unwrap();
        assert_eq!(&converted[..3], &UTF8_BOM);
        assert_eq!(&converted[3..], PROFILE.as_bytes());
    }

    #[test]
    fn legacy_profiles_are_read_in_the_system_code_page() {
        let converted = runtime_bytes_in(euc_kr(), Some(949)).unwrap();
        assert_eq!(&converted[..3], &UTF8_BOM);
        assert_eq!(std::str::from_utf8(&converted[3..]).unwrap(), PROFILE);

        let japanese = "[FontSubstitutes]
ＭＳ ゴシック=Pretendard
";
        let (legacy, _, _) = SHIFT_JIS.encode(japanese);
        let converted = runtime_bytes_in(legacy.into_owned(), Some(932)).unwrap();
        assert_eq!(std::str::from_utf8(&converted[3..]).unwrap(), japanese);
    }

    #[test]
    fn bytes_the_code_page_rejects_fall_back_to_detection() {
        let converted = runtime_bytes_in(euc_kr(), Some(437)).unwrap();
        assert_eq!(&converted[..3], &UTF8_BOM);
        assert!(std::str::from_utf8(&converted[3..]).is_ok());
    }

    #[test]
    fn conversion_is_idempotent() {
        let once = runtime_bytes_in(euc_kr(), Some(949)).unwrap();
        assert_eq!(runtime_bytes_in(once.clone(), Some(949)).unwrap(), once);
    }
}
