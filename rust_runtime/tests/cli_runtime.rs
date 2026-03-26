use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use rust_runtime::{load_hex_into_cpu, MegaRuntime, NanoRuntime};

fn ldi(d: u8, k: u8) -> u16 {
    assert!((16..=31).contains(&d));
    0xE000 | (((k as u16) & 0xF0) << 4) | (((d - 16) as u16) << 4) | ((k as u16) & 0x0F)
}

fn sts(r: u8, address: usize) -> (u16, u16) {
    (
        0x9200 | (((r as u16) & 0x1F) << 4),
        (address & 0xFFFF) as u16,
    )
}

fn brk() -> u16 {
    0x9598
}

fn program_with_serial_tail(mut words: Vec<u16>) -> Vec<u16> {
    words.extend(std::iter::repeat_n(0x0000, 200));
    words.push(brk());
    words
}

fn make_hex(words: &[u16]) -> String {
    let mut program_bytes = Vec::with_capacity(words.len() * 2);
    for word in words {
        program_bytes.push((word & 0xFF) as u8);
        program_bytes.push((word >> 8) as u8);
    }

    let mut records = Vec::new();
    for (offset, chunk) in program_bytes.chunks(16).enumerate() {
        records.push(hex_record((offset * 16) as u16, 0x00, chunk));
    }
    records.push(hex_record(0x0000, 0x01, &[]));
    records.join("\n") + "\n"
}

fn hex_record(address: u16, record_type: u8, payload: &[u8]) -> String {
    let mut body = Vec::with_capacity(payload.len() + 5);
    body.push(payload.len() as u8);
    body.push((address >> 8) as u8);
    body.push((address & 0xFF) as u8);
    body.push(record_type);
    body.extend_from_slice(payload);
    let checksum = (0u8).wrapping_sub(body.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte)));
    body.push(checksum);
    format!(
        ":{}",
        body.iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>()
    )
}

fn temp_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("arduino-simulator-{name}-{unique}"))
}

fn runtime_cli_binary() -> PathBuf {
    let current = std::env::current_exe().expect("current test executable");
    current
        .parent()
        .and_then(|path| path.parent())
        .expect("target debug directory")
        .join("arduino-simulator")
}

#[test]
fn hex_loader_populates_runtime_flash() {
    let mut runtime = NanoRuntime::new();
    let hex = make_hex(&[ldi(16, 0x42), brk()]);

    load_hex_into_cpu(&mut runtime.cpu, &hex).unwrap();

    assert_eq!(
        runtime.cpu.decode_at(0).unwrap().mnemonic,
        rust_cpu::Mnemonic::Ldi
    );
    assert_eq!(
        runtime.cpu.decode_at(1).unwrap().mnemonic,
        rust_cpu::Mnemonic::Break
    );
}

#[test]
fn nano_runtime_executes_loaded_hex_and_collects_serial() {
    let mut runtime = NanoRuntime::new();
    let program = program_with_serial_tail(vec![
        ldi(16, 0x00),
        sts(16, rust_mcu::atmega328p::UBRR0L).0,
        sts(16, rust_mcu::atmega328p::UBRR0L).1,
        ldi(16, 1 << 3),
        sts(16, rust_mcu::atmega328p::UCSR0B).0,
        sts(16, rust_mcu::atmega328p::UCSR0B).1,
        ldi(16, b'N'),
        sts(16, rust_mcu::atmega328p::UDR0).0,
        sts(16, rust_mcu::atmega328p::UDR0).1,
    ]);
    let hex = make_hex(&program);
    load_hex_into_cpu(&mut runtime.cpu, &hex).unwrap();

    let mut total = 0usize;
    loop {
        let (executed, exit) = runtime.run_chunk(64, false).unwrap();
        total += executed;
        match exit {
            Some(rust_runtime::RuntimeExit::BreakHit | rust_runtime::RuntimeExit::Sleeping) => {
                break
            }
            _ => {}
        }
    }

    assert!(total > 0);
    assert_eq!(runtime.serial_output_bytes(), b"N");
}

#[test]
fn mega_runtime_executes_loaded_hex_and_collects_serial() {
    let mut runtime = MegaRuntime::new();
    let program = program_with_serial_tail(vec![
        ldi(16, 0x00),
        sts(16, rust_mcu::atmega2560::UBRR0L).0,
        sts(16, rust_mcu::atmega2560::UBRR0L).1,
        ldi(16, 1 << 3),
        sts(16, rust_mcu::atmega2560::UCSR0B).0,
        sts(16, rust_mcu::atmega2560::UCSR0B).1,
        ldi(16, b'M'),
        sts(16, rust_mcu::atmega2560::UDR0).0,
        sts(16, rust_mcu::atmega2560::UDR0).1,
    ]);
    let hex = make_hex(&program);
    load_hex_into_cpu(&mut runtime.cpu, &hex).unwrap();

    loop {
        let (_executed, exit) = runtime.run_chunk(64, false).unwrap();
        match exit {
            Some(rust_runtime::RuntimeExit::BreakHit | rust_runtime::RuntimeExit::Sleeping) => {
                break
            }
            _ => {}
        }
    }

    assert_eq!(runtime.serial_output_bytes(), b"M");
}

#[test]
fn nano_runtime_accepts_injected_serial_rx_bytes() {
    let mut runtime = NanoRuntime::new();
    runtime.inject_serial_rx(b"OK");
    assert_eq!(runtime.cpu.bus.serial0.rx_queue.len(), 2);
    assert_eq!(runtime.configured_serial_baud(), 1_000_000);
}

#[test]
fn mega_runtime_accepts_injected_serial_rx_bytes() {
    let mut runtime = MegaRuntime::new();
    runtime.inject_serial_rx(b"HI");
    assert_eq!(runtime.cpu.bus.serial0.rx_queue.len(), 2);
    assert_eq!(runtime.configured_serial_baud(), 1_000_000);
}

#[test]
fn cli_run_nano_writes_serial_output_to_file() {
    let hex_path = temp_path("nano.hex");
    let out_path = temp_path("nano.txt");
    let program = program_with_serial_tail(vec![
        ldi(16, 0x00),
        sts(16, rust_mcu::atmega328p::UBRR0L).0,
        sts(16, rust_mcu::atmega328p::UBRR0L).1,
        ldi(16, 1 << 3),
        sts(16, rust_mcu::atmega328p::UCSR0B).0,
        sts(16, rust_mcu::atmega328p::UCSR0B).1,
        ldi(16, b'Q'),
        sts(16, rust_mcu::atmega328p::UDR0).0,
        sts(16, rust_mcu::atmega328p::UDR0).1,
    ]);
    let hex = make_hex(&program);
    fs::write(&hex_path, hex).unwrap();

    let status = rust_runtime::run_cli([
        "arduino-simulator".to_string(),
        "run-nano".to_string(),
        hex_path.display().to_string(),
        "--out".to_string(),
        out_path.display().to_string(),
        "--max-instructions".to_string(),
        "512".to_string(),
    ])
    .unwrap();

    assert_eq!(status, 0);
    assert_eq!(fs::read(&out_path).unwrap(), b"Q");

    let _ = fs::remove_file(hex_path);
    let _ = fs::remove_file(out_path);
}

#[test]
fn cli_run_mega_prints_serial_output_to_stdout() {
    let hex_path = temp_path("mega.hex");
    let program = program_with_serial_tail(vec![
        ldi(16, 0x00),
        sts(16, rust_mcu::atmega2560::UBRR0L).0,
        sts(16, rust_mcu::atmega2560::UBRR0L).1,
        ldi(16, 1 << 3),
        sts(16, rust_mcu::atmega2560::UCSR0B).0,
        sts(16, rust_mcu::atmega2560::UCSR0B).1,
        ldi(16, b'Z'),
        sts(16, rust_mcu::atmega2560::UDR0).0,
        sts(16, rust_mcu::atmega2560::UDR0).1,
    ]);
    let hex = make_hex(&program);
    fs::write(&hex_path, hex).unwrap();

    let output = Command::new(runtime_cli_binary())
        .args([
            "run-mega",
            hex_path.to_str().unwrap(),
            "--until-serial",
            "--chunk-size",
            "16",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(output.stdout, b"Z");

    let _ = fs::remove_file(hex_path);
}

#[test]
fn cli_reports_missing_serial_output_when_requested() {
    let hex_path = temp_path("silent.hex");
    fs::write(&hex_path, make_hex(&[0x0000, 0x0000, brk()])).unwrap();

    let output = Command::new(runtime_cli_binary())
        .args([
            "run-nano",
            hex_path.to_str().unwrap(),
            "--until-serial",
            "--max-instructions",
            "8",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("no serial output captured"));

    let _ = fs::remove_file(hex_path);
}
