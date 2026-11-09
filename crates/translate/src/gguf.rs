//! Pure-Rust GGUF preflight validation.
//!
//! llama.cpp is the authority on tensor layouts, but it is native code parsing
//! an untrusted file. This module checks the container before llama.cpp sees
//! it: magic, version, metadata and tensor counts, string/array lengths,
//! offsets and the model architecture. It does not allocate tokenizer arrays;
//! large arrays are skipped with bounded reads.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use thiserror::Error;

const MAGIC: u32 = 0x4655_4747;
const MAX_METADATA: u64 = 200_000;
const MAX_TENSORS: u64 = 200_000;
const MAX_STRING: u64 = 16 * 1024 * 1024;
const MAX_METADATA_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ARRAY: u64 = 4_000_000;
const MAX_DIMS: u32 = 8;
const MAX_TENSOR_ELEMENTS: u64 = 1 << 40;
const MAX_ALIGNMENT: u32 = 4096;

/// Errors returned before a file reaches llama.cpp.
#[derive(Debug, Error)]
pub enum GgufError {
    #[error("model I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a GGUF file")]
    BadMagic,
    #[error("unsupported GGUF version {0}")]
    UnsupportedVersion(u32),
    #[error("{field} count {actual} exceeds the limit of {max}")]
    TooMany {
        field: &'static str,
        max: u64,
        actual: u64,
    },
    #[error("GGUF string is too long ({0} bytes)")]
    StringTooLong(u64),
    #[error("GGUF metadata is too large")]
    MetadataTooLarge,
    #[error("unknown GGUF metadata type {0}")]
    UnknownType(u32),
    #[error("invalid GGUF tensor dimensions")]
    BadDimensions,
    #[error("GGUF tensor elements exceed the limit")]
    TooManyElements,
    #[error("GGUF offset points past the end of the file")]
    OffsetOutOfBounds,
    #[error("GGUF file is truncated")]
    Truncated,
    #[error("GGUF metadata is not valid UTF-8")]
    InvalidUtf8,
    #[error("GGUF model is missing general.architecture")]
    MissingArchitecture,
    #[error("unsupported model architecture {0}; Jabu expects Hunyuan MT")]
    UnsupportedArchitecture(String),
}

/// The container facts Jabu needs before loading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufInfo {
    pub version: u32,
    pub tensor_count: u64,
    pub metadata_count: u64,
    pub architecture: String,
    pub tokenizer_model: Option<String>,
    pub alignment: u32,
    pub data_offset: u64,
}

/// Validate a GGUF file and return its container information.
///
/// # Errors
///
/// Returns an error for a non-GGUF file, an unsupported version, an oversized
/// or truncated structure, an unsupported architecture, or a tensor offset
/// outside the file.
pub fn preflight(path: &Path) -> Result<GgufInfo, GgufError> {
    let file = File::open(path)?;
    let length = file.metadata()?.len();
    let mut reader = BufReader::new(file);
    inspect(&mut reader, length)
}

fn inspect<R: Read + Seek>(reader: &mut R, file_length: u64) -> Result<GgufInfo, GgufError> {
    if read_u32(reader)? != MAGIC {
        return Err(GgufError::BadMagic);
    }
    let version = read_u32(reader)?;
    if !matches!(version, 2 | 3) {
        return Err(GgufError::UnsupportedVersion(version));
    }
    let tensor_count = read_u64(reader)?;
    let metadata_count = read_u64(reader)?;
    check_count("metadata", metadata_count, MAX_METADATA)?;
    check_count("tensor", tensor_count, MAX_TENSORS)?;
    if metadata_count.saturating_mul(32) > file_length {
        return Err(GgufError::Truncated);
    }

    let mut architecture = None;
    let mut tokenizer_model = None;
    let mut alignment = 32u32;
    let metadata_start = reader.stream_position()?;
    for _ in 0..metadata_count {
        let key = read_string(reader)?;
        let value_type = read_u32(reader)?;
        match value_type {
            0 | 1 | 7 => {
                skip(reader, 1)?;
            }
            2 | 3 => {
                skip(reader, 2)?;
            }
            4 | 5 | 6 => {
                let value = read_u32(reader)?;
                if key == "general.alignment" {
                    if value == 0 || value > MAX_ALIGNMENT {
                        return Err(GgufError::BadDimensions);
                    }
                    alignment = value;
                }
            }
            10 | 11 | 12 => {
                skip(reader, 8)?;
            }
            8 => {
                let value = read_string(reader)?;
                match key.as_str() {
                    "general.architecture" => architecture = Some(value),
                    "tokenizer.ggml.model" => tokenizer_model = Some(value),
                    _ => {}
                }
            }
            9 => {
                let element_type = read_u32(reader)?;
                let count = read_u64(reader)?;
                if count > MAX_ARRAY {
                    return Err(GgufError::TooMany {
                        field: "array",
                        max: MAX_ARRAY,
                        actual: count,
                    });
                }
                for _ in 0..count {
                    skip_value(reader, element_type)?;
                }
            }
            other => return Err(GgufError::UnknownType(other)),
        }
        let consumed = reader.stream_position()?.saturating_sub(metadata_start);
        if consumed > MAX_METADATA_BYTES {
            return Err(GgufError::MetadataTooLarge);
        }
    }

    let metadata_end = reader.stream_position()?;
    let mut max_offset = 0u64;
    for _ in 0..tensor_count {
        let _name = read_string(reader)?;
        let dimensions = read_u32(reader)?;
        if dimensions == 0 || dimensions > MAX_DIMS {
            return Err(GgufError::BadDimensions);
        }
        let mut elements = 1u64;
        for _ in 0..dimensions {
            let dimension = read_u64(reader)?;
            if dimension == 0 {
                return Err(GgufError::BadDimensions);
            }
            elements = elements
                .checked_mul(dimension)
                .ok_or(GgufError::TooManyElements)?;
            if elements > MAX_TENSOR_ELEMENTS {
                return Err(GgufError::TooManyElements);
            }
        }
        let _type = read_u32(reader)?;
        let offset = read_u64(reader)?;
        max_offset = max_offset.max(offset);
    }

    let info_end = reader.stream_position()?;
    let data_offset = align_up(info_end, u64::from(alignment))?;
    if data_offset > file_length {
        return Err(GgufError::Truncated);
    }
    if max_offset >= file_length.saturating_sub(data_offset) {
        return Err(GgufError::OffsetOutOfBounds);
    }
    if metadata_end > file_length {
        return Err(GgufError::Truncated);
    }

    let architecture = architecture.ok_or(GgufError::MissingArchitecture)?;
    if architecture != "hunyuan-dense" {
        return Err(GgufError::UnsupportedArchitecture(architecture));
    }
    Ok(GgufInfo {
        version,
        tensor_count,
        metadata_count,
        architecture,
        tokenizer_model,
        alignment,
        data_offset,
    })
}

fn check_count(field: &'static str, actual: u64, max: u64) -> Result<(), GgufError> {
    if actual > max {
        Err(GgufError::TooMany { field, max, actual })
    } else {
        Ok(())
    }
}

fn skip_value<R: Read>(reader: &mut R, value_type: u32) -> Result<(), GgufError> {
    match value_type {
        0 | 1 | 7 => skip(reader, 1),
        2 | 3 => skip(reader, 2),
        4 | 5 | 6 => skip(reader, 4),
        10 | 11 | 12 => skip(reader, 8),
        8 => {
            let length = read_u64(reader)?;
            if length > MAX_STRING {
                return Err(GgufError::StringTooLong(length));
            }
            skip(reader, length)
        }
        9 => {
            // Nested arrays are unusual; keep a small bound to avoid recursion.
            let element_type = read_u32(reader)?;
            let count = read_u64(reader)?;
            if count > MAX_ARRAY {
                return Err(GgufError::TooMany {
                    field: "nested array",
                    max: MAX_ARRAY,
                    actual: count,
                });
            }
            for _ in 0..count {
                skip_value(reader, element_type)?;
            }
            Ok(())
        }
        other => Err(GgufError::UnknownType(other)),
    }
}

fn read_string<R: Read>(reader: &mut R) -> Result<String, GgufError> {
    let length = read_u64(reader)?;
    if length > MAX_STRING {
        return Err(GgufError::StringTooLong(length));
    }
    let length = usize::try_from(length).map_err(|_| GgufError::StringTooLong(length))?;
    let mut bytes = vec![0u8; length];
    reader.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|_| GgufError::InvalidUtf8)
}

fn read_u32<R: Read>(reader: &mut R) -> Result<u32, GgufError> {
    let mut bytes = [0u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64<R: Read>(reader: &mut R) -> Result<u64, GgufError> {
    let mut bytes = [0u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn skip<R: Read>(reader: &mut R, bytes: u64) -> Result<(), GgufError> {
    let bytes = usize::try_from(bytes).unwrap_or(usize::MAX);
    let mut buffer = vec![0u8; bytes.min(8 * 1024)];
    let mut remaining = bytes;
    while remaining > 0 {
        let chunk = remaining.min(buffer.len());
        reader.read_exact(&mut buffer[..chunk])?;
        remaining -= chunk;
    }
    Ok(())
}

fn align_up(value: u64, alignment: u64) -> Result<u64, GgufError> {
    if alignment == 0 {
        return Ok(value);
    }
    let remainder = value % alignment;
    if remainder == 0 {
        Ok(value)
    } else {
        value
            .checked_add(alignment - remainder)
            .ok_or(GgufError::Truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn push_str(bytes: &mut Vec<u8>, value: &str) {
        bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }

    fn sample() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GGUF");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes());
        bytes.extend_from_slice(&3u64.to_le_bytes());
        for (key, value) in [
            ("general.architecture", Some("hunyuan-dense")),
            ("tokenizer.ggml.model", Some("gpt2")),
        ] {
            push_str(&mut bytes, key);
            bytes.extend_from_slice(&8u32.to_le_bytes());
            push_str(&mut bytes, value.unwrap());
        }
        push_str(&mut bytes, "general.alignment");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&32u32.to_le_bytes());
        push_str(&mut bytes, "blk.0.weight");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&4u64.to_le_bytes());
        bytes.extend_from_slice(&4u64.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        while bytes.len() % 32 != 0 {
            bytes.push(0);
        }
        bytes.extend_from_slice(&[0u8; 64]);
        bytes
    }

    #[test]
    fn accepts_a_minimal_hunyuan_file() {
        let bytes = sample();
        let info = inspect(&mut Cursor::new(&bytes), bytes.len() as u64).unwrap();
        assert_eq!(info.architecture, "hunyuan-dense");
        assert_eq!(info.tokenizer_model.as_deref(), Some("gpt2"));
        assert_eq!(info.tensor_count, 1);
        assert_eq!(info.alignment, 32);
    }

    #[test]
    fn rejects_bad_magic_and_version() {
        let mut bytes = sample();
        bytes[0] = b'X';
        assert!(matches!(
            inspect(&mut Cursor::new(&bytes), bytes.len() as u64),
            Err(GgufError::BadMagic)
        ));
        let mut bytes = sample();
        bytes[4..8].copy_from_slice(&9u32.to_le_bytes());
        assert!(matches!(
            inspect(&mut Cursor::new(&bytes), bytes.len() as u64),
            Err(GgufError::UnsupportedVersion(9))
        ));
    }

    #[test]
    fn rejects_oversized_counts_and_truncation() {
        let mut bytes = sample();
        bytes[8..16].copy_from_slice(&(MAX_TENSORS + 1).to_le_bytes());
        assert!(matches!(
            inspect(&mut Cursor::new(&bytes), bytes.len() as u64),
            Err(GgufError::TooMany { .. })
        ));
        let mut bytes = sample();
        bytes.truncate(20);
        assert!(matches!(
            inspect(&mut Cursor::new(&bytes), bytes.len() as u64),
            Err(GgufError::Truncated | GgufError::Io(_))
        ));
    }

    #[test]
    fn rejects_other_architectures() {
        let mut bytes = sample();
        let position = bytes
            .windows(b"hunyuan-dense".len())
            .position(|window| window == b"hunyuan-dense")
            .unwrap();
        bytes[position..position + b"hunyuan-dense".len()].copy_from_slice(b"llama-dense!!");
        assert!(matches!(
            inspect(&mut Cursor::new(&bytes), bytes.len() as u64),
            Err(GgufError::UnsupportedArchitecture(_))
        ));
    }
}
