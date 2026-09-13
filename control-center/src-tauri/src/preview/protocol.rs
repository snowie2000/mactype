use std::io::{Read, Write};

const MAGIC: u32 = 0x4350_544D;
pub(super) const VERSION: u16 = 1;
const MAX_JSON: usize = 64 * 1024;
const MAX_BINARY: usize = 8 * 1024 * 1024;

pub(super) const HELLO: u16 = 1;
pub(super) const RENDER_PREVIEW: u16 = 3;
pub(super) const SHUTDOWN: u16 = 4;
pub(super) const SHOW_NATIVE_PREVIEW: u16 = 6;
pub(super) const HIDE_NATIVE_PREVIEW: u16 = 7;
pub(super) const HELLO_ACK: u16 = 101;
pub(super) const PREVIEW_RENDERED: u16 = 103;
pub(super) const NATIVE_PREVIEW_STATE: u16 = 105;
pub(super) const ERROR: u16 = 199;

#[derive(Debug)]
pub(super) struct Frame {
    pub(super) kind: u16,
    pub(super) request_id: u64,
    pub(super) json: Vec<u8>,
    pub(super) binary: Vec<u8>,
}

impl Frame {
    pub(super) fn json_text(&self) -> Result<&str, String> {
        std::str::from_utf8(&self.json).map_err(|error| error.to_string())
    }
}

pub(super) fn read_frame(reader: &mut impl Read) -> Result<Frame, String> {
    let mut header = [0_u8; 24];
    reader
        .read_exact(&mut header)
        .map_err(|error| error.to_string())?;
    let magic = u32::from_le_bytes(header[0..4].try_into().expect("fixed frame header"));
    let version = u16::from_le_bytes(header[4..6].try_into().expect("fixed frame header"));
    if magic != MAGIC || version != VERSION {
        return Err("preview helper returned an unsupported frame".to_owned());
    }
    let kind = u16::from_le_bytes(header[6..8].try_into().expect("fixed frame header"));
    let request_id = u64::from_le_bytes(header[8..16].try_into().expect("fixed frame header"));
    let json_length =
        u32::from_le_bytes(header[16..20].try_into().expect("fixed frame header")) as usize;
    let binary_length =
        u32::from_le_bytes(header[20..24].try_into().expect("fixed frame header")) as usize;
    if json_length > MAX_JSON || binary_length > MAX_BINARY {
        return Err("preview helper frame exceeds the size limit".to_owned());
    }
    let mut json = vec![0; json_length];
    let mut binary = vec![0; binary_length];
    reader
        .read_exact(&mut json)
        .map_err(|error| error.to_string())?;
    reader
        .read_exact(&mut binary)
        .map_err(|error| error.to_string())?;
    Ok(Frame {
        kind,
        request_id,
        json,
        binary,
    })
}

pub(super) fn write_frame(writer: &mut impl Write, frame: &Frame) -> Result<(), String> {
    if frame.json.len() > MAX_JSON || frame.binary.len() > MAX_BINARY {
        return Err("preview request exceeds the size limit".to_owned());
    }
    writer
        .write_all(&MAGIC.to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&VERSION.to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&frame.kind.to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&frame.request_id.to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&(frame.json.len() as u32).to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&(frame.binary.len() as u32).to_le_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&frame.json)
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&frame.binary)
        .map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_round_trip_preserves_binary_payload() {
        let original = Frame {
            kind: RENDER_PREVIEW,
            request_id: 41,
            json: br#"{"ok":true}"#.to_vec(),
            binary: vec![1, 2, 3, 4],
        };
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &original).unwrap();
        let decoded = read_frame(&mut bytes.as_slice()).unwrap();
        assert_eq!(decoded.kind, original.kind);
        assert_eq!(decoded.request_id, original.request_id);
        assert_eq!(decoded.json, original.json);
        assert_eq!(decoded.binary, original.binary);
    }

    #[test]
    fn oversized_frame_is_rejected_before_allocation() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MAGIC.to_le_bytes());
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&RENDER_PREVIEW.to_le_bytes());
        bytes.extend_from_slice(&1_u64.to_le_bytes());
        bytes.extend_from_slice(&((MAX_JSON + 1) as u32).to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        assert!(read_frame(&mut bytes.as_slice())
            .unwrap_err()
            .contains("size limit"));
    }
}
