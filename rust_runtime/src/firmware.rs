use std::fmt;
use std::fs;
use std::path::Path;

use rust_cpu::{Cpu, CpuError, DataBus};

#[derive(Debug)]
pub enum HexLoadError {
    Io(std::io::Error),
    InvalidRecord(String),
    UnsupportedRecordType(u8),
    Cpu(CpuError),
}

impl fmt::Display for HexLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HexLoadError::Io(error) => write!(f, "{error}"),
            HexLoadError::InvalidRecord(message) => write!(f, "{message}"),
            HexLoadError::UnsupportedRecordType(record_type) => {
                write!(f, "unsupported Intel HEX record type 0x{record_type:02X}")
            }
            HexLoadError::Cpu(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for HexLoadError {}

impl From<std::io::Error> for HexLoadError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<CpuError> for HexLoadError {
    fn from(value: CpuError) -> Self {
        Self::Cpu(value)
    }
}

pub fn load_hex_file<B: DataBus>(cpu: &mut Cpu<B>, path: &Path) -> Result<(), HexLoadError> {
    let contents = fs::read_to_string(path)?;
    load_hex_into_cpu(cpu, &contents)
}

pub fn load_hex_into_cpu<B: DataBus>(cpu: &mut Cpu<B>, hex: &str) -> Result<(), HexLoadError> {
    let mut upper_linear_base = 0u32;
    let mut upper_segment_base = 0u32;

    for (line_index, raw_line) in hex.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if !line.starts_with(':') {
            return Err(HexLoadError::InvalidRecord(format!(
                "line {}: Intel HEX record must start with ':'",
                line_index + 1
            )));
        }

        let bytes = decode_hex_record(line, line_index + 1)?;
        if bytes.len() < 5 {
            return Err(HexLoadError::InvalidRecord(format!(
                "line {}: Intel HEX record too short",
                line_index + 1
            )));
        }
        let data_len = bytes[0] as usize;
        if bytes.len() != data_len + 5 {
            return Err(HexLoadError::InvalidRecord(format!(
                "line {}: byte count does not match payload length",
                line_index + 1
            )));
        }
        let checksum: u8 = bytes.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte));
        if checksum != 0 {
            return Err(HexLoadError::InvalidRecord(format!(
                "line {}: checksum mismatch",
                line_index + 1
            )));
        }

        let address = u16::from(bytes[1]) << 8 | u16::from(bytes[2]);
        let record_type = bytes[3];
        let payload = &bytes[4..4 + data_len];

        match record_type {
            0x00 => {
                let base = if upper_linear_base != 0 {
                    upper_linear_base
                } else {
                    upper_segment_base
                };
                let absolute_address = base + u32::from(address);
                cpu.load_program_bytes(payload, absolute_address as usize)?;
            }
            0x01 => break,
            0x02 => {
                if payload.len() != 2 {
                    return Err(HexLoadError::InvalidRecord(format!(
                        "line {}: extended segment record must contain 2 bytes",
                        line_index + 1
                    )));
                }
                upper_segment_base = (u32::from(payload[0]) << 8 | u32::from(payload[1])) << 4;
                upper_linear_base = 0;
            }
            0x04 => {
                if payload.len() != 2 {
                    return Err(HexLoadError::InvalidRecord(format!(
                        "line {}: extended linear record must contain 2 bytes",
                        line_index + 1
                    )));
                }
                upper_linear_base = (u32::from(payload[0]) << 8 | u32::from(payload[1])) << 16;
                upper_segment_base = 0;
            }
            0x03 | 0x05 => {}
            other => return Err(HexLoadError::UnsupportedRecordType(other)),
        }
    }

    Ok(())
}

fn decode_hex_record(line: &str, line_number: usize) -> Result<Vec<u8>, HexLoadError> {
    let hex = &line[1..];
    if (hex.len() & 1) != 0 {
        return Err(HexLoadError::InvalidRecord(format!(
            "line {}: Intel HEX payload must contain an even number of hex digits",
            line_number
        )));
    }

    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let chars: Vec<char> = hex.chars().collect();
    for pair in chars.chunks(2) {
        let hi = pair[0].to_digit(16).ok_or_else(|| {
            HexLoadError::InvalidRecord(format!("line {}: invalid hex digit", line_number))
        })?;
        let lo = pair[1].to_digit(16).ok_or_else(|| {
            HexLoadError::InvalidRecord(format!("line {}: invalid hex digit", line_number))
        })?;
        bytes.push(((hi << 4) | lo) as u8);
    }
    Ok(bytes)
}
