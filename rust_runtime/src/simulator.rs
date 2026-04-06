use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use rust_cpu::CpuError;
use rust_mcu::{BoardPin, BoardPinLevel};
use serde::{Deserialize, Serialize};

use crate::{load_hex_into_cpu, HexLoadError, MegaRuntime, NanoRuntime, RuntimeExit};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulatorBoard {
    NanoV3,
    Mega2560Rev3,
}

impl SimulatorBoard {
    pub const ALL: [SimulatorBoard; 2] =
        [SimulatorBoard::Mega2560Rev3, SimulatorBoard::NanoV3];

    pub fn label(self) -> &'static str {
        match self {
            Self::NanoV3 => "Arduino Nano (ATmega328P)",
            Self::Mega2560Rev3 => "Arduino Mega 2560",
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            Self::NanoV3 => "nano",
            Self::Mega2560Rev3 => "mega",
        }
    }
}

impl Default for SimulatorBoard {
    fn default() -> Self {
        Self::Mega2560Rev3
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulatorConfig {
    pub board: SimulatorBoard,
    pub firmware_path: Option<PathBuf>,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            board: SimulatorBoard::default(),
            firmware_path: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationSnapshot {
    pub board: SimulatorBoard,
    pub firmware_path: Option<PathBuf>,
    pub firmware_loaded: bool,
    pub last_exit: Option<RuntimeExit>,
    pub serial_text: String,
    pub serial_bytes: usize,
    pub serial_configured_baud: u32,
    pub serial_rx_queued: usize,
    pub pc: u32,
    pub sp: u16,
    pub cycles: u64,
    pub synced_cycles: u64,
    pub sreg: u8,
    pub registers: [u8; 32],
    pub host_pin_levels: Vec<BoardPinLevel>,
}

#[derive(Debug)]
pub enum SimulatorError {
    FirmwareNotLoaded,
    AnalogInputUnsupported(SimulatorBoard),
    Hex(HexLoadError),
    Cpu(CpuError),
}

impl fmt::Display for SimulatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FirmwareNotLoaded => write!(f, "no firmware image is loaded"),
            Self::AnalogInputUnsupported(board) => write!(
                f,
                "analog input injection is not yet supported for {}",
                board.label()
            ),
            Self::Hex(error) => write!(f, "{error}"),
            Self::Cpu(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SimulatorError {}

impl From<HexLoadError> for SimulatorError {
    fn from(value: HexLoadError) -> Self {
        Self::Hex(value)
    }
}

impl From<CpuError> for SimulatorError {
    fn from(value: CpuError) -> Self {
        Self::Cpu(value)
    }
}

pub struct SimulatorCore {
    board: SimulatorBoard,
    runtime: RuntimeVariant,
    firmware_hex: Option<String>,
    firmware_path: Option<PathBuf>,
    last_exit: Option<RuntimeExit>,
}

impl SimulatorCore {
    pub fn new(board: SimulatorBoard) -> Self {
        Self {
            board,
            runtime: RuntimeVariant::new(board),
            firmware_hex: None,
            firmware_path: None,
            last_exit: None,
        }
    }

    pub fn board(&self) -> SimulatorBoard {
        self.board
    }

    pub fn config(&self) -> SimulatorConfig {
        SimulatorConfig {
            board: self.board,
            firmware_path: self.firmware_path.clone(),
        }
    }

    pub fn apply_config(&mut self, config: &SimulatorConfig) -> Result<(), SimulatorError> {
        self.board = config.board;
        self.runtime = RuntimeVariant::new(config.board);
        self.last_exit = None;
        self.firmware_hex = None;
        self.firmware_path = None;

        if let Some(path) = &config.firmware_path {
            self.load_hex_path(path)?;
        }

        Ok(())
    }

    pub fn load_hex_path(&mut self, path: impl AsRef<Path>) -> Result<(), SimulatorError> {
        let path = path.as_ref();
        let hex = fs::read_to_string(path).map_err(HexLoadError::Io)?;
        self.load_hex_string(Some(path.to_path_buf()), hex)
    }

    pub fn load_hex_string(
        &mut self,
        source_path: Option<PathBuf>,
        hex: impl Into<String>,
    ) -> Result<(), SimulatorError> {
        let hex = hex.into();
        self.runtime = RuntimeVariant::new(self.board);
        self.runtime.load_hex(&hex)?;
        self.firmware_hex = Some(hex);
        self.firmware_path = source_path;
        self.last_exit = None;
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), SimulatorError> {
        self.runtime = RuntimeVariant::new(self.board);
        self.last_exit = None;
        if let Some(hex) = self.firmware_hex.clone() {
            self.runtime.load_hex(&hex)?;
        }
        Ok(())
    }

    pub fn run_chunk(
        &mut self,
        instruction_budget: usize,
        until_serial: bool,
    ) -> Result<(usize, Option<RuntimeExit>), SimulatorError> {
        if self.firmware_hex.is_none() {
            return Err(SimulatorError::FirmwareNotLoaded);
        }
        let result = self.runtime.run_chunk(instruction_budget, until_serial)?;
        self.last_exit = result.1;
        Ok(result)
    }

    pub fn step(&mut self) -> Result<(usize, Option<RuntimeExit>), SimulatorError> {
        self.run_chunk(1, false)
    }

    pub fn inject_serial_rx(&mut self, payload: &[u8]) {
        self.runtime.inject_serial_rx(payload);
    }

    pub fn take_new_serial_bytes(&mut self) -> Vec<u8> {
        self.runtime.take_new_serial_bytes().to_vec()
    }

    pub fn take_new_serial_text(&mut self) -> String {
        String::from_utf8_lossy(&self.take_new_serial_bytes()).into_owned()
    }

    pub fn clear_serial_output(&mut self) {
        self.runtime.clear_serial_output();
    }

    pub fn set_input_pin_level(&mut self, pin: BoardPin, level: u8) {
        self.runtime.set_input_pin_level(pin, level);
    }

    pub fn clear_input_pin_level(&mut self, pin: BoardPin) {
        self.runtime.clear_input_pin_level(pin);
    }

    pub fn set_analog_input_level(
        &mut self,
        pin: BoardPin,
        counts: u16,
    ) -> Result<(), SimulatorError> {
        self.runtime
            .set_analog_input_level(pin, counts)
            .ok_or(SimulatorError::AnalogInputUnsupported(self.board))
    }

    pub fn clear_analog_input_level(&mut self, pin: BoardPin) -> Result<(), SimulatorError> {
        self.runtime
            .clear_analog_input_level(pin)
            .ok_or(SimulatorError::AnalogInputUnsupported(self.board))
    }

    pub fn snapshot(&self) -> SimulationSnapshot {
        SimulationSnapshot {
            board: self.board,
            firmware_path: self.firmware_path.clone(),
            firmware_loaded: self.firmware_hex.is_some(),
            last_exit: self.last_exit,
            serial_text: String::from_utf8_lossy(self.runtime.serial_output_bytes()).into_owned(),
            serial_bytes: self.runtime.serial_output_bytes().len(),
            serial_configured_baud: self.runtime.configured_serial_baud(),
            serial_rx_queued: self.runtime.serial_rx_queued(),
            pc: self.runtime.pc(),
            sp: self.runtime.sp(),
            cycles: self.runtime.cycles(),
            synced_cycles: self.runtime.synced_cycles(),
            sreg: self.runtime.sreg(),
            registers: self.runtime.registers(),
            host_pin_levels: self.runtime.host_pin_levels(),
        }
    }
}

impl Default for SimulatorCore {
    fn default() -> Self {
        Self::new(SimulatorBoard::default())
    }
}

enum RuntimeVariant {
    Nano(NanoRuntime),
    Mega(MegaRuntime),
}

impl RuntimeVariant {
    fn new(board: SimulatorBoard) -> Self {
        match board {
            SimulatorBoard::NanoV3 => Self::Nano(NanoRuntime::new()),
            SimulatorBoard::Mega2560Rev3 => Self::Mega(MegaRuntime::new()),
        }
    }

    fn load_hex(&mut self, hex: &str) -> Result<(), HexLoadError> {
        match self {
            Self::Nano(runtime) => load_hex_into_cpu(&mut runtime.cpu, hex),
            Self::Mega(runtime) => load_hex_into_cpu(&mut runtime.cpu, hex),
        }
    }

    fn run_chunk(
        &mut self,
        instruction_budget: usize,
        until_serial: bool,
    ) -> Result<(usize, Option<RuntimeExit>), CpuError> {
        match self {
            Self::Nano(runtime) => runtime.run_chunk(instruction_budget, until_serial),
            Self::Mega(runtime) => runtime.run_chunk(instruction_budget, until_serial),
        }
    }

    fn serial_output_bytes(&self) -> &[u8] {
        match self {
            Self::Nano(runtime) => runtime.serial_output_bytes(),
            Self::Mega(runtime) => runtime.serial_output_bytes(),
        }
    }

    fn take_new_serial_bytes(&mut self) -> &[u8] {
        match self {
            Self::Nano(runtime) => runtime.take_new_serial_bytes(),
            Self::Mega(runtime) => runtime.take_new_serial_bytes(),
        }
    }

    fn clear_serial_output(&mut self) {
        match self {
            Self::Nano(runtime) => runtime.clear_serial_output(),
            Self::Mega(runtime) => runtime.clear_serial_output(),
        }
    }

    fn inject_serial_rx(&mut self, payload: &[u8]) {
        match self {
            Self::Nano(runtime) => runtime.inject_serial_rx(payload),
            Self::Mega(runtime) => runtime.inject_serial_rx(payload),
        }
    }

    fn configured_serial_baud(&self) -> u32 {
        match self {
            Self::Nano(runtime) => runtime.configured_serial_baud(),
            Self::Mega(runtime) => runtime.configured_serial_baud(),
        }
    }

    fn serial_rx_queued(&self) -> usize {
        match self {
            Self::Nano(runtime) => runtime.cpu.bus.serial0.rx_queue.len(),
            Self::Mega(runtime) => runtime.cpu.bus.serial0.rx_queue.len(),
        }
    }

    fn pc(&self) -> u32 {
        match self {
            Self::Nano(runtime) => runtime.cpu.pc as u32,
            Self::Mega(runtime) => runtime.cpu.pc as u32,
        }
    }

    fn sp(&self) -> u16 {
        match self {
            Self::Nano(runtime) => runtime.cpu.sp(),
            Self::Mega(runtime) => runtime.cpu.sp(),
        }
    }

    fn cycles(&self) -> u64 {
        match self {
            Self::Nano(runtime) => runtime.cpu.cycles,
            Self::Mega(runtime) => runtime.cpu.cycles,
        }
    }

    fn synced_cycles(&self) -> u64 {
        match self {
            Self::Nano(runtime) => runtime.cpu.bus.synced_cycles,
            Self::Mega(runtime) => runtime.cpu.bus.synced_cycles,
        }
    }

    fn sreg(&self) -> u8 {
        match self {
            Self::Nano(runtime) => runtime.cpu.data[runtime.cpu.config.sreg_address],
            Self::Mega(runtime) => runtime.cpu.data[runtime.cpu.config.sreg_address],
        }
    }

    fn registers(&self) -> [u8; 32] {
        let mut registers = [0u8; 32];
        match self {
            Self::Nano(runtime) => registers.copy_from_slice(&runtime.cpu.data[..32]),
            Self::Mega(runtime) => registers.copy_from_slice(&runtime.cpu.data[..32]),
        }
        registers
    }

    fn host_pin_levels(&self) -> Vec<BoardPinLevel> {
        match self {
            Self::Nano(runtime) => runtime.cpu.bus.host_pin_levels(),
            Self::Mega(runtime) => runtime.cpu.bus.host_pin_levels(),
        }
    }

    fn set_input_pin_level(&mut self, pin: BoardPin, level: u8) {
        match self {
            Self::Nano(runtime) => runtime.cpu.bus.board.set_input_level(pin, level),
            Self::Mega(runtime) => runtime.cpu.bus.board.set_input_level(pin, level),
        }
    }

    fn clear_input_pin_level(&mut self, pin: BoardPin) {
        match self {
            Self::Nano(runtime) => runtime.cpu.bus.board.clear_input_level(pin),
            Self::Mega(runtime) => runtime.cpu.bus.board.clear_input_level(pin),
        }
    }

    fn set_analog_input_level(&mut self, pin: BoardPin, counts: u16) -> Option<()> {
        match self {
            Self::Mega(runtime) => {
                runtime.cpu.bus.board.set_analog_input_level(pin, counts);
                Some(())
            }
            Self::Nano(_) => None,
        }
    }

    fn clear_analog_input_level(&mut self, pin: BoardPin) -> Option<()> {
        match self {
            Self::Mega(runtime) => {
                runtime.cpu.bus.board.clear_analog_input_level(pin);
                Some(())
            }
            Self::Nano(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use rust_mcu::{atmega2560, atmega328p};

    use super::{SimulatorBoard, SimulatorCore};
    use crate::RuntimeExit;

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

    #[test]
    fn simulator_core_runs_loaded_firmware_and_captures_serial() {
        let mut simulator = SimulatorCore::new(SimulatorBoard::Mega2560Rev3);
        let program = program_with_serial_tail(vec![
            ldi(16, 0x00),
            sts(16, atmega2560::UBRR0L).0,
            sts(16, atmega2560::UBRR0L).1,
            ldi(16, 1 << 3),
            sts(16, atmega2560::UCSR0B).0,
            sts(16, atmega2560::UCSR0B).1,
            ldi(16, b'R'),
            sts(16, atmega2560::UDR0).0,
            sts(16, atmega2560::UDR0).1,
        ]);

        simulator
            .load_hex_string(None, make_hex(&program))
            .expect("load hex");

        loop {
            let (_, exit) = simulator.run_chunk(64, false).expect("run chunk");
            match exit {
                Some(RuntimeExit::BreakHit | RuntimeExit::Sleeping) => break,
                _ => {}
            }
        }

        let snapshot = simulator.snapshot();
        assert_eq!(snapshot.serial_text, "R");
        assert!(snapshot.firmware_loaded);
    }

    #[test]
    fn simulator_core_reset_restores_loaded_firmware() {
        let mut simulator = SimulatorCore::new(SimulatorBoard::NanoV3);
        let program = program_with_serial_tail(vec![
            ldi(16, 0x00),
            sts(16, atmega328p::UBRR0L).0,
            sts(16, atmega328p::UBRR0L).1,
            ldi(16, 1 << 3),
            sts(16, atmega328p::UCSR0B).0,
            sts(16, atmega328p::UCSR0B).1,
            ldi(16, b'N'),
            sts(16, atmega328p::UDR0).0,
            sts(16, atmega328p::UDR0).1,
        ]);

        simulator
            .load_hex_string(None, make_hex(&program))
            .expect("load hex");
        let _ = simulator.step().expect("step");
        simulator.reset().expect("reset");

        let snapshot = simulator.snapshot();
        assert_eq!(snapshot.pc, 0);
        assert_eq!(snapshot.serial_text, "");
        assert!(snapshot.firmware_loaded);
    }

    #[test]
    fn simulator_core_reports_pin_input_overrides() {
        let mut simulator = SimulatorCore::new(SimulatorBoard::NanoV3);
        simulator.set_input_pin_level(rust_mcu::BoardPin::Digital(2), 1);

        let snapshot = simulator.snapshot();
        let d2 = snapshot
            .host_pin_levels
            .into_iter()
            .find(|entry| entry.pin == rust_mcu::BoardPin::Digital(2))
            .expect("D2 snapshot");
        assert_eq!(d2.level, 1);
    }
}
