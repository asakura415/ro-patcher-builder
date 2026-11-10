use std::io::Write;

use crate::{GrufError, Result};
use encoding::label::encoding_from_whatwg_label;
use encoding::EncoderTrap;

pub struct GenericFileEntry {
    pub offset: u64,
    // Note(LinkZ): u32 limited by the GRF and THOR file formats
    pub size: u32,
    pub size_compressed: u32,
}

/// Serializes string into a NULL-terminated list of win1252 chars and write it
/// into writer.
///
/// Used in GRF archives
pub fn serialize_as_win1252_cstr_into<W: Write>(mut writer: W, string: &str) -> Result<()> {
    let mut vec = serialize_to_win1252(string)?;
    vec.push(0); // NUL char terminator
    writer.write_all(vec.as_slice())?;
    Ok(())
}

/// Serializes string into a list of win1252 chars and write it into writer.
///
// Used in THOR archives
pub fn serialize_as_win1252_str_into<W: Write>(mut writer: W, string: &str) -> Result<()> {
    let vec = serialize_to_win1252(string)?;
    writer.write_all(vec.as_slice())?;
    Ok(())
}

pub fn serialize_to_win1252(string: &str) -> Result<Vec<u8>> {
    if let Some(encoder) = encoding_from_whatwg_label("windows-1252") {
        if let Ok(bytes) = encoder.encode(string, EncoderTrap::Strict) {
            return Ok(bytes);
        }
    }

    // Fallback to Korean CP949 / EUC-KR ("windows-949") for Unicode Korean filenames (e.g. "유저인터페이스")
    if let Some(kr_encoder) = encoding_from_whatwg_label("windows-949") {
        if let Ok(bytes) = kr_encoder.encode(string, EncoderTrap::Strict) {
            return Ok(bytes);
        }
    }

    // Final fallback: preserve raw Latin-1 / C1 bytes (0x00..=0xFF) if all codepoints <= 0xFF
    if string.chars().all(|c| (c as u32) <= 0xFF) {
        return Ok(string.chars().map(|c| c as u8).collect());
    }

    Err(GrufError::serialization_error(
        "Encoding failed: filename contains characters not representable in Windows-1252 or Korean CP949",
    ))
}
