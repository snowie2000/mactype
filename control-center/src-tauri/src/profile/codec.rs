use super::{BomKind, LineEnding, TextEncoding};
use encoding_rs::{Encoding, BIG5, EUC_KR, GB18030, SHIFT_JIS, WINDOWS_1252};

pub(super) type OriginalLegacyLines = Vec<(String, Vec<u8>)>;

pub(super) fn decode(bytes: &[u8]) -> Result<(String, TextEncoding, BomKind), String> {
    if let Some(body) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(body.to_vec())
            .map(|text| (text, TextEncoding::Utf8, BomKind::Utf8))
            .map_err(|error| error.to_string());
    }
    if let Some(body) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        if body.len() % 2 != 0 {
            return Err("UTF-16LE profile has an odd byte length".to_owned());
        }
        let units = body
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16(&units)
            .map(|text| (text, TextEncoding::Utf16Le, BomKind::Utf16Le))
            .map_err(|error| error.to_string());
    }
    if let Some(body) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        if body.len() % 2 != 0 {
            return Err("UTF-16BE profile has an odd byte length".to_owned());
        }
        let units = body
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        return String::from_utf16(&units)
            .map(|text| (text, TextEncoding::Utf16Be, BomKind::Utf16Be))
            .map_err(|error| error.to_string());
    }
    if let Ok(text) = String::from_utf8(bytes.to_vec()) {
        return Ok((text, TextEncoding::Utf8, BomKind::None));
    }
    let candidates = [
        (TextEncoding::Gb18030, GB18030),
        (TextEncoding::Big5, BIG5),
        (TextEncoding::ShiftJis, SHIFT_JIS),
        (TextEncoding::EucKr, EUC_KR),
        (TextEncoding::Windows1252, WINDOWS_1252),
    ];
    candidates
        .into_iter()
        .filter_map(|(encoding, codec)| {
            let text = codec
                .decode_without_bom_handling_and_without_replacement(bytes)?
                .into_owned();
            let (round_trip, _, encode_errors) = codec.encode(&text);
            if encode_errors || round_trip.as_ref() != bytes {
                return None;
            }
            let score = legacy_score(&text, encoding);
            Some((score, text, encoding))
        })
        .max_by_key(|candidate| candidate.0)
        .map(|(_, text, encoding)| (text, encoding, BomKind::None))
        .ok_or_else(|| {
            "profile is not valid UTF-8, UTF-16, GB18030, Big5, Shift-JIS, EUC-KR, or Windows-1252"
                .to_owned()
        })
}

fn legacy_score(text: &str, encoding: TextEncoding) -> i64 {
    let sections = text
        .lines()
        .filter(|line| {
            let line = line.trim();
            line.starts_with('[') && line.ends_with(']')
        })
        .count() as i64;
    let assignments = text.lines().filter(|line| line.contains('=')).count() as i64;
    let mut hangul = 0_i64;
    let mut kana = 0_i64;
    let mut han = 0_i64;
    let mut non_ascii = 0_i64;
    let mut controls = 0_i64;
    let mut simplified = 0_i64;
    let mut traditional = 0_i64;
    const SIMPLIFIED: &str = "简体汉语配置设置默认启用关闭字体进程缓存试验权重过滤阴影";
    const TRADITIONAL: &str = "簡體漢語設定預設啟用關閉字型程序快取試驗權重過濾陰影";
    // Let GB18030 win ambiguous ties: it covers legacy GBK/CP936, while Big5
    // accepts some of the same byte streams through permissive mappings.
    const GB18030_AMBIGUITY_BIAS: i64 = 75;
    for character in text.chars() {
        if !character.is_ascii() {
            non_ascii += 1;
        }
        match character {
            '\u{ac00}'..='\u{d7af}' => hangul += 1,
            '\u{3040}'..='\u{30ff}' => kana += 1,
            '\u{3400}'..='\u{9fff}' => han += 1,
            character if character.is_control() && !matches!(character, '\r' | '\n' | '\t') => {
                controls += 1
            }
            _ => {}
        }
        if SIMPLIFIED.contains(character) {
            simplified += 1;
        }
        if TRADITIONAL.contains(character) {
            traditional += 1;
        }
    }
    let structure = sections * 100 + assignments * 12 - controls * 200;
    structure
        + match encoding {
            TextEncoding::Gb18030 => {
                han * 3 + simplified * 18 - traditional * 3 - hangul * 12 - kana * 12
                    + i64::from(han > 0) * 10
                    + GB18030_AMBIGUITY_BIAS
            }
            TextEncoding::Big5 => {
                han * 3 + traditional * 18 - simplified * 3 - hangul * 12 - kana * 12
                    + i64::from(han > 0) * 10
            }
            TextEncoding::ShiftJis => kana * 20 + han * 2 - hangul * 12 + i64::from(kana > 0) * 10,
            TextEncoding::EucKr => hangul * 20 + han - kana * 12 + i64::from(hangul > 0) * 10,
            // Never reward ASCII as Windows-1252 evidence: every candidate
            // shares it, while Windows-1252 turns CJK bytes into mojibake.
            TextEncoding::Windows1252 => -non_ascii * 8 - (hangul + kana + han) * 8,
            TextEncoding::Utf8 | TextEncoding::Utf16Le | TextEncoding::Utf16Be => 0,
        }
}

fn legacy_codec(encoding: TextEncoding) -> Option<&'static Encoding> {
    match encoding {
        TextEncoding::EucKr => Some(EUC_KR),
        TextEncoding::Gb18030 => Some(GB18030),
        TextEncoding::Big5 => Some(BIG5),
        TextEncoding::ShiftJis => Some(SHIFT_JIS),
        TextEncoding::Windows1252 => Some(WINDOWS_1252),
        TextEncoding::Utf8 | TextEncoding::Utf16Le | TextEncoding::Utf16Be => None,
    }
}

pub(super) fn encode(text: &str, encoding: TextEncoding, bom: BomKind) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    match encoding {
        TextEncoding::Utf8 => {
            if matches!(bom, BomKind::Utf8) {
                output.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            }
            output.extend_from_slice(text.as_bytes());
        }
        TextEncoding::Utf16Le | TextEncoding::Utf16Be => {
            if matches!(bom, BomKind::Utf16Le) {
                output.extend_from_slice(&[0xFF, 0xFE]);
            } else if matches!(bom, BomKind::Utf16Be) {
                output.extend_from_slice(&[0xFE, 0xFF]);
            }
            for unit in text.encode_utf16() {
                let bytes = if matches!(encoding, TextEncoding::Utf16Le) {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                };
                output.extend_from_slice(&bytes);
            }
        }
        TextEncoding::EucKr
        | TextEncoding::Gb18030
        | TextEncoding::Big5
        | TextEncoding::ShiftJis
        | TextEncoding::Windows1252 => {
            let codec = legacy_codec(encoding).expect("legacy encoding must have a codec");
            let (encoded, _, had_errors) = codec.encode(text);
            if had_errors {
                return Err(
                    "profile contains text that cannot be represented in its original encoding"
                        .to_owned(),
                );
            }
            output.extend_from_slice(&encoded);
        }
    }
    Ok(output)
}

pub(super) fn detect_line_ending(text: &str) -> LineEnding {
    if text.contains("\r\n") {
        LineEnding::CrLf
    } else if text.contains('\n') {
        LineEnding::Lf
    } else {
        LineEnding::Cr
    }
}

pub(super) fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, character) in text.char_indices() {
        if character == '\n' || (character == '\r' && !text[index..].starts_with("\r\n")) {
            lines.push(&text[start..=index]);
            start = index + 1;
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

fn split_byte_lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            index += usize::from(bytes.get(index + 1) == Some(&b'\n'));
            lines.push(&bytes[start..=index]);
            start = index + 1;
        } else if bytes[index] == b'\n' {
            lines.push(&bytes[start..=index]);
            start = index + 1;
        }
        index += 1;
    }
    if start < bytes.len() {
        lines.push(&bytes[start..]);
    }
    lines
}

pub(super) fn original_legacy_lines(
    bytes: &[u8],
    text: &str,
    encoding: TextEncoding,
) -> Option<OriginalLegacyLines> {
    legacy_codec(encoding)?;
    let text_lines = split_lines(text);
    let byte_lines = split_byte_lines(bytes);
    (text_lines.len() == byte_lines.len()).then(|| {
        text_lines
            .into_iter()
            .zip(byte_lines)
            .map(|(line, bytes)| (line.to_owned(), bytes.to_vec()))
            .collect()
    })
}

pub(super) fn encode_preserving_legacy_lines<'a>(
    lines: impl IntoIterator<Item = &'a str>,
    original_lines: &OriginalLegacyLines,
    encoding: TextEncoding,
) -> Result<Vec<u8>, String> {
    let codec = legacy_codec(encoding).expect("legacy lines require a legacy codec");
    let mut used = vec![false; original_lines.len()];
    let mut output = Vec::new();
    for raw in lines {
        if let Some((index, (_, bytes))) = original_lines
            .iter()
            .enumerate()
            .find(|(index, (line, _))| !used[*index] && line == raw)
        {
            used[index] = true;
            output.extend_from_slice(bytes);
            continue;
        }
        let (encoded, _, had_errors) = codec.encode(raw);
        if had_errors {
            return Err(
                "profile contains text that cannot be represented in its original encoding"
                    .to_owned(),
            );
        }
        output.extend_from_slice(&encoded);
    }
    Ok(output)
}
