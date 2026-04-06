use rust_cpu::{Cpu, CpuConfig, CpuError, StepOutcome};
use rust_mcu::atmega2560::{
    Atmega2560Bus, NullMegaBoard, UBRR0H as MEGA_UBRR0H, UBRR0L as MEGA_UBRR0L,
    UCSR0A as MEGA_UCSR0A,
};
use rust_mcu::atmega328p::{
    Atmega328pBus, NullNanoBoard, UBRR0H as NANO_UBRR0H, UBRR0L as NANO_UBRR0L,
    UCSR0A as NANO_UCSR0A,
};
use serde::{Deserialize, Serialize};

const U2X0: u8 = 1 << 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeExit {
    BreakHit,
    Sleeping,
    MaxInstructionsReached,
    UntilSerialSatisfied,
}

pub struct NanoRuntime {
    pub cpu: Cpu<Atmega328pBus<NullNanoBoard>>,
    serial_cursor: usize,
}

pub struct MegaRuntime {
    pub cpu: Cpu<Atmega2560Bus<NullMegaBoard>>,
    serial_cursor: usize,
}

impl NanoRuntime {
    pub fn new() -> Self {
        Self {
            cpu: Cpu::new(
                CpuConfig::atmega328p(),
                Atmega328pBus::new(NullNanoBoard::default(), 16_000_000),
            ),
            serial_cursor: 0,
        }
    }

    pub fn serial_output_bytes(&self) -> &[u8] {
        &self.cpu.bus.serial0.tx_log
    }

    pub fn take_new_serial_bytes(&mut self) -> &[u8] {
        let start = self.serial_cursor;
        self.serial_cursor = self.cpu.bus.serial0.tx_log.len();
        &self.cpu.bus.serial0.tx_log[start..]
    }

    pub fn clear_serial_output(&mut self) {
        self.cpu.bus.serial0.clear_output();
        self.serial_cursor = 0;
    }

    pub fn inject_serial_rx(&mut self, payload: &[u8]) {
        self.cpu.bus.serial0.inject_rx(payload);
    }

    pub fn configured_serial_baud(&self) -> u32 {
        configured_baud(
            self.cpu.bus.clock_hz,
            self.cpu.data[NANO_UCSR0A],
            self.cpu.data[NANO_UBRR0L],
            self.cpu.data[NANO_UBRR0H],
        )
    }

    pub fn run_chunk(
        &mut self,
        instruction_budget: usize,
        until_serial: bool,
    ) -> Result<(usize, Option<RuntimeExit>), CpuError> {
        run_chunk(
            &mut self.cpu,
            &mut self.serial_cursor,
            instruction_budget,
            until_serial,
        )
    }
}

impl Default for NanoRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl MegaRuntime {
    pub fn new() -> Self {
        Self {
            cpu: Cpu::new(
                CpuConfig::atmega2560(),
                Atmega2560Bus::new(NullMegaBoard::default(), 16_000_000),
            ),
            serial_cursor: 0,
        }
    }

    pub fn serial_output_bytes(&self) -> &[u8] {
        &self.cpu.bus.serial0.tx_log
    }

    pub fn take_new_serial_bytes(&mut self) -> &[u8] {
        let start = self.serial_cursor;
        self.serial_cursor = self.cpu.bus.serial0.tx_log.len();
        &self.cpu.bus.serial0.tx_log[start..]
    }

    pub fn clear_serial_output(&mut self) {
        self.cpu.bus.serial0.clear_output();
        self.serial_cursor = 0;
    }

    pub fn inject_serial_rx(&mut self, payload: &[u8]) {
        self.cpu.bus.serial0.inject_rx(payload);
    }

    pub fn configured_serial_baud(&self) -> u32 {
        configured_baud(
            self.cpu.bus.clock_hz,
            self.cpu.data[MEGA_UCSR0A],
            self.cpu.data[MEGA_UBRR0L],
            self.cpu.data[MEGA_UBRR0H],
        )
    }

    pub fn run_chunk(
        &mut self,
        instruction_budget: usize,
        until_serial: bool,
    ) -> Result<(usize, Option<RuntimeExit>), CpuError> {
        run_chunk(
            &mut self.cpu,
            &mut self.serial_cursor,
            instruction_budget,
            until_serial,
        )
    }
}

impl Default for MegaRuntime {
    fn default() -> Self {
        Self::new()
    }
}

fn configured_baud(clock_hz: u32, ucsr0a: u8, ubrr0l: u8, ubrr0h: u8) -> u32 {
    let ubrr = (ubrr0l as u16) | (((ubrr0h as u16) & 0x0F) << 8);
    let divisor = if (ucsr0a & U2X0) != 0 { 8u32 } else { 16u32 };
    let denom = divisor.saturating_mul((ubrr as u32).saturating_add(1));
    if denom == 0 {
        return 0;
    }
    clock_hz / denom
}

fn run_chunk<B>(
    cpu: &mut Cpu<B>,
    serial_cursor: &mut usize,
    instruction_budget: usize,
    until_serial: bool,
) -> Result<(usize, Option<RuntimeExit>), CpuError>
where
    B: rust_cpu::DataBus + SerialTap,
{
    let mut executed = 0usize;
    while executed < instruction_budget {
        match cpu.step()? {
            StepOutcome::Executed => {
                executed += 1;
                if until_serial && cpu.bus.serial_bytes_len() > *serial_cursor {
                    return Ok((executed, Some(RuntimeExit::UntilSerialSatisfied)));
                }
            }
            StepOutcome::BreakHit => {
                executed += 1;
                return Ok((executed, Some(RuntimeExit::BreakHit)));
            }
            StepOutcome::Sleeping => {
                executed += 1;
                return Ok((executed, Some(RuntimeExit::Sleeping)));
            }
        }
    }
    Ok((executed, Some(RuntimeExit::MaxInstructionsReached)))
}

pub trait SerialTap {
    fn serial_bytes_len(&self) -> usize;
}

impl SerialTap for Atmega328pBus<NullNanoBoard> {
    fn serial_bytes_len(&self) -> usize {
        self.serial0.tx_log.len()
    }
}

impl SerialTap for Atmega2560Bus<NullMegaBoard> {
    fn serial_bytes_len(&self) -> usize {
        self.serial0.tx_log.len()
    }
}
