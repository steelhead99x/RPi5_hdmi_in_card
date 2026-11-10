use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::{err, Result};

const EMBEDDED: &[(&str, &str)] = &[
    ("720p60edid", include_str!("../hdmi2csi2card/720p60edid")),
    ("1080p25edid", include_str!("../hdmi2csi2card/1080p25edid")),
    ("1080p30edid", include_str!("../hdmi2csi2card/1080p30edid")),
    ("1080p50edid", include_str!("../hdmi2csi2card/1080p50edid")),
    ("1080p60edid", include_str!("../hdmi2csi2card/1080p60edid")),
];

#[derive(Debug, Clone)]
pub struct Edid {
    pub name: String,
    pub source: String,
    pub bytes: Vec<u8>,
}

pub fn nominal_size(name: &str) -> Option<(u32, u32)> {
    let base = Path::new(name)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(name);
    match base {
        "720p60edid" => Some((1280, 720)),
        "1080p25edid" | "1080p30edid" | "1080p50edid" | "1080p60edid" => Some((1920, 1080)),
        _ => None,
    }
}

const MAX_EDID_FILE: u64 = 16 * 1024;

pub fn load(name: &str) -> Result<Edid> {
    let path = Path::new(name);
    if name.contains('/') || path.is_file() {
        let bytes = read_edid_file(path)?;
        let parsed = parse_bytes(&bytes).map_err(|error| err(format!("{name}: {error}")))?;
        return Ok(Edid {
            name: path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(name)
                .to_string(),
            source: path_label(path),
            bytes: parsed,
        });
    }
    if let Some((_, text)) = EMBEDDED.iter().find(|(key, _)| *key == name) {
        let parsed = parse_hex(text)?;
        return Ok(Edid {
            name: name.to_string(),
            source: "built-in".to_string(),
            bytes: parsed,
        });
    }
    let known = EMBEDDED
        .iter()
        .map(|(key, _)| *key)
        .collect::<Vec<_>>()
        .join(", ");
    Err(err(format!(
        "Unknown EDID {name}. Built-in names: {known}. Pass a file path for another EDID."
    )))
}

fn read_edid_file(path: &Path) -> Result<Vec<u8>> {
    let followed = std::fs::metadata(path)
        .map_err(|error| err(format!("Cannot read {}: {error}", path.display())))?;
    if !followed.is_file() {
        return Err(err(format!("{} is not a regular file.", path.display())));
    }
    if followed.len() > MAX_EDID_FILE {
        return Err(err(format!(
            "{} is {} bytes. An EDID file must be {} bytes or smaller.",
            path.display(),
            followed.len(),
            MAX_EDID_FILE
        )));
    }
    let mut file = std::fs::File::open(path)
        .map_err(|error| err(format!("Cannot read {}: {error}", path.display())))?;
    let mut bytes = Vec::new();
    std::io::Read::take(&mut file, MAX_EDID_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| err(format!("Cannot read {}: {error}", path.display())))?;
    if bytes.len() as u64 > MAX_EDID_FILE {
        return Err(err(format!(
            "{} is larger than {} bytes.",
            path.display(),
            MAX_EDID_FILE
        )));
    }
    Ok(bytes)
}

fn path_label(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| PathBuf::from(path))
        .display()
        .to_string()
}

pub fn parse_hex(text: &str) -> Result<Vec<u8>> {
    if text.len() > MAX_EDID_FILE as usize {
        return Err(err("EDID text is too long."));
    }
    let mut digits = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_whitespace() {
            continue;
        }
        if ch == '0' && matches!(chars.peek(), Some('x' | 'X')) {
            chars.next();
            continue;
        }
        if !ch.is_ascii_hexdigit() {
            return Err(err(format!("EDID text contains {ch:?}, which is not hex")));
        }
        digits.push(ch);
    }
    if !digits.len().is_multiple_of(2) {
        return Err(err("EDID text has an odd number of hex digits"));
    }
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    let digit_bytes = digits.as_bytes();
    for index in (0..digit_bytes.len()).step_by(2) {
        let pair =
            std::str::from_utf8(&digit_bytes[index..index + 2]).expect("hex digits are ascii");
        bytes.push(u8::from_str_radix(pair, 16).expect("hex pair"));
    }
    parse_bytes(&bytes)
}

pub fn parse_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.is_empty() || !bytes.len().is_multiple_of(128) || bytes.len() > 512 {
        return Err(err(format!(
            "EDID length is {} bytes. It must be 128, 256, or another multiple of 128 up to 512.",
            bytes.len()
        )));
    }
    let header = [0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00];
    if bytes.len() < 8 || bytes[..8] != header {
        return Err(err("EDID header is not 00 ff ff ff ff ff ff 00"));
    }
    for (index, block) in bytes.chunks(128).enumerate() {
        let sum = block
            .iter()
            .fold(0u8, |total, byte| total.wrapping_add(*byte));
        if sum != 0 {
            let expected = block[127].wrapping_sub(sum);
            return Err(err(format!(
                "EDID block {index} checksum is 0x{:02x}, expected 0x{expected:02x}",
                block[127]
            )));
        }
    }
    Ok(bytes.to_vec())
}

pub fn preferred_timing(bytes: &[u8]) -> Result<(u32, u32, f64)> {
    if bytes.len() < 72 {
        return Err(err("EDID is too short for a preferred timing"));
    }
    timing(&bytes[54..72])
}

pub fn timing(block: &[u8]) -> Result<(u32, u32, f64)> {
    if block.len() < 18 {
        return Err(err("Detailed timing block is short"));
    }
    let clock = (u32::from(block[1]) << 8 | u32::from(block[0])) * 10_000;
    if clock == 0 {
        return Err(err("Preferred descriptor is not a detailed timing"));
    }
    let width = u32::from(block[2]) | (u32::from(block[4] & 0xf0) << 4);
    let h_blank = u32::from(block[3]) | (u32::from(block[4] & 0x0f) << 8);
    let height = u32::from(block[5]) | (u32::from(block[7] & 0xf0) << 4);
    let v_blank = u32::from(block[6]) | (u32::from(block[7] & 0x0f) << 8);
    let total = (width + h_blank) * (height + v_blank);
    if total == 0 {
        return Err(err("Detailed timing has a zero total size"));
    }
    let hertz = f64::from(clock) / f64::from(total);
    Ok((width, height, hertz))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_modes_match_their_names() {
        let expected = [
            ("720p60edid", 1280, 720, 60.0),
            ("1080p25edid", 1920, 1080, 25.0),
            ("1080p30edid", 1920, 1080, 30.0),
            ("1080p50edid", 1920, 1080, 50.0),
            ("1080p60edid", 1920, 1080, 60.0),
        ];
        for (name, width, height, refresh) in expected {
            let edid = load(name).unwrap();
            assert_eq!(edid.bytes.len(), 256, "{name}");
            assert_eq!(edid.source, "built-in");
            let (got_w, got_h, got_hz) = preferred_timing(&edid.bytes).unwrap();
            assert_eq!((got_w, got_h), (width, height), "{name}");
            assert!((got_hz - refresh).abs() < 0.01, "{name} is {got_hz}");
            let extension = &edid.bytes[128..];
            assert_eq!(extension[0], 0x02);
            assert_eq!(extension[3] & 0x30, 0, "{name} must not ask for YCbCr");
            assert_eq!(extension[3] & 0x40, 0x40, "{name} should offer basic audio");
        }
    }

    #[test]
    fn embedded_bytes_match_the_files() {
        for (name, text) in EMBEDDED {
            let from_file = std::fs::read_to_string(format!("hdmi2csi2card/{name}")).unwrap();
            assert_eq!(*text, from_file);
            assert_eq!(parse_hex(text).unwrap(), parse_hex(&from_file).unwrap());
        }
    }

    #[test]
    fn rejects_a_bad_checksum_and_a_bad_header() {
        let mut bytes = load("720p60edid").unwrap().bytes;
        bytes[127] ^= 0xff;
        assert!(parse_bytes(&bytes)
            .unwrap_err()
            .to_string()
            .contains("checksum"));
        bytes = load("720p60edid").unwrap().bytes;
        bytes[1] = 0x00;
        assert!(parse_bytes(&bytes)
            .unwrap_err()
            .to_string()
            .contains("header"));
    }

    #[test]
    fn rejects_oversized_text_and_non_files() {
        let text = "00".repeat(20_000);
        assert!(parse_hex(&text)
            .unwrap_err()
            .to_string()
            .contains("too long"));
        let error = load("/dev/null").unwrap_err().to_string();
        assert!(error.contains("regular file"), "{error}");
    }

    #[test]
    fn hex_parser_accepts_spaces_and_0x() {
        let bytes = parse_hex("00 ff ff ff ff ff ff 00\n0x11 22").unwrap_err();
        assert!(bytes.to_string().contains("length") || bytes.to_string().contains("header"));
        let err = parse_hex("zz").unwrap_err();
        assert!(err.to_string().contains("not hex"));
    }
}
