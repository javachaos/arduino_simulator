use std::fmt;

use rust_cpu::{Cpu, CpuError, DataBus};

#[derive(Debug)]
pub enum HexLoadError {
    InvalidEncoding,
    InvalidRecord(String),
    UnsupportedRecordType(u8),
    Cpu(CpuError),
}

impl fmt::Display for HexLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HexLoadError::InvalidEncoding => write!(f, "firmware.hex must be valid UTF-8 text"),
            HexLoadError::InvalidRecord(message) => write!(f, "{message}"),
            HexLoadError::UnsupportedRecordType(record_type) => {
                write!(f, "unsupported Intel HEX record type 0x{record_type:02X}")
            }
            HexLoadError::Cpu(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for HexLoadError {}

impl From<CpuError> for HexLoadError {
    fn from(value: CpuError) -> Self {
        Self::Cpu(value)
    }
}

pub fn decode_hex_bytes(bytes: &[u8]) -> Result<String, HexLoadError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| HexLoadError::InvalidEncoding)
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

#[cfg(test)]
mod tests {
    use rust_cpu::{Cpu, CpuConfig, Mnemonic, NullBus};

    use super::{decode_hex_bytes, load_hex_into_cpu};

    fn ldi(d: u8, k: u8) -> u16 {
        0xE000 | (((k as u16) & 0xF0) << 4) | (((d - 16) as u16) << 4) | ((k as u16) & 0x0F)
    }

    fn brk() -> u16 {
        0x9598
    }

    fn hex_record(address: u16, record_type: u8, payload: &[u8]) -> String {
        let mut body = Vec::with_capacity(payload.len() + 5);
        body.push(payload.len() as u8);
        body.push((address >> 8) as u8);
        body.push((address & 0xFF) as u8);
        body.push(record_type);
        body.extend_from_slice(payload);
        let checksum =
            (0u8).wrapping_sub(body.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte)));
        body.push(checksum);
        format!(
            ":{}",
            body.iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<String>()
        )
    }

    fn words_to_bytes(words: &[u16]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(words.len() * 2);
        for word in words {
            bytes.push((word & 0xFF) as u8);
            bytes.push((word >> 8) as u8);
        }
        bytes
    }

    #[test]
    fn decode_hex_bytes_rejects_binary_payloads() {
        assert!(decode_hex_bytes(&[0xFF, 0x00]).is_err());
    }

    #[test]
    fn load_hex_into_cpu_loads_program_records() {
        let hex = format!(
            "{}\n{}\n{}",
            hex_record(0x0000, 0x00, &words_to_bytes(&[ldi(16, 0x42)])),
            hex_record(0x0002, 0x00, &words_to_bytes(&[brk()])),
            hex_record(0x0000, 0x01, &[])
        );
        let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
        cpu.reset(true);

        load_hex_into_cpu(&mut cpu, &hex).expect("load hex");

        assert_eq!(cpu.decode_at(0).expect("ldi").mnemonic, Mnemonic::Ldi);
        assert_eq!(cpu.decode_at(1).expect("brk").mnemonic, Mnemonic::Break);
    }
}
