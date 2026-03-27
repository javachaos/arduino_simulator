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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use rust_cpu::Mnemonic;

    use super::{decode_hex_record, load_hex_file, load_hex_into_cpu, HexLoadError};
    use crate::runtime::{MegaRuntime, NanoRuntime};

    fn ldi(d: u8, k: u8) -> u16 {
        assert!((16..=31).contains(&d));
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

    fn temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("arduino-simulator-firmware-{name}-{unique}.hex"))
    }

    #[test]
    fn load_hex_into_cpu_loads_data_records_and_skips_blank_lines() {
        let mut runtime = NanoRuntime::new();
        let hex = format!(
            "\n{}\n\n{}\n{}\n",
            hex_record(0x0000, 0x00, &words_to_bytes(&[ldi(16, 0x42)])),
            hex_record(0x0002, 0x00, &words_to_bytes(&[brk()])),
            hex_record(0x0000, 0x01, &[])
        );

        load_hex_into_cpu(&mut runtime.cpu, &hex).expect("load hex");

        assert_eq!(
            runtime.cpu.decode_at(0).expect("ldi").mnemonic,
            Mnemonic::Ldi
        );
        assert_eq!(
            runtime.cpu.decode_at(1).expect("brk").mnemonic,
            Mnemonic::Break
        );
    }

    #[test]
    fn load_hex_into_cpu_supports_segment_and_linear_address_bases() {
        let mut runtime = MegaRuntime::new();
        let hex = [
            hex_record(0x0000, 0x02, &[0x01, 0x00]),
            hex_record(0x0000, 0x00, &words_to_bytes(&[ldi(16, 0x2A)])),
            hex_record(0x0000, 0x03, &[0x00, 0x00, 0x00, 0x00]),
            hex_record(0x0000, 0x04, &[0x00, 0x01]),
            hex_record(0x0000, 0x05, &[0x00, 0x00, 0x00, 0x00]),
            hex_record(0x0000, 0x00, &words_to_bytes(&[brk()])),
            hex_record(0x0000, 0x01, &[]),
        ]
        .join("\n");

        load_hex_into_cpu(&mut runtime.cpu, &hex).expect("load hex");

        assert_eq!(
            runtime
                .cpu
                .decode_at(0x1000 / 2)
                .expect("segmented data")
                .mnemonic,
            Mnemonic::Ldi
        );
        assert_eq!(
            runtime
                .cpu
                .decode_at(0x10000 / 2)
                .expect("linear data")
                .mnemonic,
            Mnemonic::Break
        );
    }

    #[test]
    fn load_hex_file_reads_contents_from_disk() {
        let mut runtime = NanoRuntime::new();
        let path = temp_path("load-file");
        let hex = format!(
            "{}\n{}\n",
            hex_record(0x0000, 0x00, &words_to_bytes(&[ldi(16, 0x11)])),
            hex_record(0x0000, 0x01, &[])
        );
        fs::write(&path, hex).expect("write hex");

        load_hex_file(&mut runtime.cpu, &path).expect("load file");
        assert_eq!(
            runtime.cpu.decode_at(0).expect("ldi").mnemonic,
            Mnemonic::Ldi
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn decode_hex_record_rejects_odd_length_payloads_and_invalid_digits() {
        assert!(matches!(
            decode_hex_record(":123", 7),
            Err(HexLoadError::InvalidRecord(message))
                if message == "line 7: Intel HEX payload must contain an even number of hex digits"
        ));

        assert!(matches!(
            decode_hex_record(":00GG", 3),
            Err(HexLoadError::InvalidRecord(message))
                if message == "line 3: invalid hex digit"
        ));
    }

    #[test]
    fn load_hex_into_cpu_rejects_malformed_records() {
        let mut runtime = NanoRuntime::new();

        assert!(matches!(
            load_hex_into_cpu(&mut runtime.cpu, "00000001FF"),
            Err(HexLoadError::InvalidRecord(message))
                if message == "line 1: Intel HEX record must start with ':'"
        ));

        assert!(matches!(
            load_hex_into_cpu(&mut runtime.cpu, ":0000"),
            Err(HexLoadError::InvalidRecord(message))
                if message == "line 1: Intel HEX record too short"
        ));

        assert!(matches!(
            load_hex_into_cpu(&mut runtime.cpu, ":01000000FF"),
            Err(HexLoadError::InvalidRecord(message))
                if message == "line 1: byte count does not match payload length"
        ));

        assert!(matches!(
            load_hex_into_cpu(&mut runtime.cpu, ":0000000001"),
            Err(HexLoadError::InvalidRecord(message))
                if message == "line 1: checksum mismatch"
        ));
    }

    #[test]
    fn load_hex_into_cpu_rejects_invalid_extended_records_and_record_types() {
        let mut runtime = MegaRuntime::new();

        let bad_segment = format!(
            "{}\n{}\n",
            hex_record(0x0000, 0x02, &[0x12]),
            hex_record(0x0000, 0x01, &[])
        );
        assert!(matches!(
            load_hex_into_cpu(&mut runtime.cpu, &bad_segment),
            Err(HexLoadError::InvalidRecord(message))
                if message == "line 1: extended segment record must contain 2 bytes"
        ));

        let bad_linear = format!(
            "{}\n{}\n",
            hex_record(0x0000, 0x04, &[0x00]),
            hex_record(0x0000, 0x01, &[])
        );
        assert!(matches!(
            load_hex_into_cpu(&mut runtime.cpu, &bad_linear),
            Err(HexLoadError::InvalidRecord(message))
                if message == "line 1: extended linear record must contain 2 bytes"
        ));

        let unsupported = format!(
            "{}\n{}\n",
            hex_record(0x0000, 0x07, &[0x00]),
            hex_record(0x0000, 0x01, &[])
        );
        assert!(matches!(
            load_hex_into_cpu(&mut runtime.cpu, &unsupported),
            Err(HexLoadError::UnsupportedRecordType(0x07))
        ));
    }
}
