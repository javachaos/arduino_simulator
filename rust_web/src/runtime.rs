use rust_cpu::{
    Cpu, CpuConfig, CpuError, DataBus, DecodedInstruction, Mnemonic, OperandSet, PointerMode,
    PointerRegister, StepOutcome,
};
use rust_mcu::atmega2560::{
    Atmega2560Bus, NullMegaBoard, UBRR0H as MEGA_UBRR0H, UBRR0L as MEGA_UBRR0L,
    UCSR0A as MEGA_UCSR0A,
};
use rust_mcu::atmega328p::{
    Atmega328pBus, NullNanoBoard, UBRR0H as NANO_UBRR0H, UBRR0L as NANO_UBRR0L,
    UCSR0A as NANO_UCSR0A,
};
use rust_mcu::BoardPinLevel;

use crate::firmware::{load_hex_into_cpu, HexLoadError};

const U2X0: u8 = 1 << 1;
const CLOCK_HZ: u32 = 16_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationTarget {
    Nano,
    Mega,
}

impl SimulationTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::Nano => "Arduino Nano",
            Self::Mega => "Arduino Mega 2560",
        }
    }
}

impl Default for SimulationTarget {
    fn default() -> Self {
        Self::Nano
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeExit {
    BreakHit,
    Sleeping,
    MaxInstructionsReached,
}

#[derive(Debug, Clone)]
pub struct CpuSnapshot {
    pub target: SimulationTarget,
    pub pc: u32,
    pub sp: u16,
    pub cycles: u64,
    pub synced_cycles: u64,
    pub serial_bytes: usize,
    pub serial_rx_queued: usize,
    pub sreg: u8,
    pub registers: [u8; 32],
    pub next_instruction: String,
    pub extra_lines: Vec<String>,
}

pub enum SimulationRuntime {
    Nano {
        cpu: Cpu<Atmega328pBus<NullNanoBoard>>,
        serial_cursor: usize,
    },
    Mega {
        cpu: Cpu<Atmega2560Bus<NullMegaBoard>>,
        serial_cursor: usize,
    },
}

impl SimulationRuntime {
    pub fn new(target: SimulationTarget) -> Self {
        match target {
            SimulationTarget::Nano => Self::Nano {
                cpu: Cpu::new(
                    CpuConfig::atmega328p(),
                    Atmega328pBus::new(NullNanoBoard::default(), CLOCK_HZ),
                ),
                serial_cursor: 0,
            },
            SimulationTarget::Mega => Self::Mega {
                cpu: Cpu::new(
                    CpuConfig::atmega2560(),
                    Atmega2560Bus::new(NullMegaBoard::default(), CLOCK_HZ),
                ),
                serial_cursor: 0,
            },
        }
    }

    pub fn target(&self) -> SimulationTarget {
        match self {
            Self::Nano { .. } => SimulationTarget::Nano,
            Self::Mega { .. } => SimulationTarget::Mega,
        }
    }

    pub fn reset(&mut self) {
        match self {
            Self::Nano { cpu, serial_cursor } => {
                cpu.reset(false);
                *serial_cursor = 0;
            }
            Self::Mega { cpu, serial_cursor } => {
                cpu.reset(false);
                *serial_cursor = 0;
            }
        }
    }

    pub fn load_hex(&mut self, hex: &str) -> Result<(), HexLoadError> {
        match self {
            Self::Nano { cpu, serial_cursor } => {
                cpu.reset(true);
                *serial_cursor = 0;
                load_hex_into_cpu(cpu, hex)?;
            }
            Self::Mega { cpu, serial_cursor } => {
                cpu.reset(true);
                *serial_cursor = 0;
                load_hex_into_cpu(cpu, hex)?;
            }
        }
        Ok(())
    }

    pub fn clear_serial_output(&mut self) {
        match self {
            Self::Nano { cpu, serial_cursor } => {
                cpu.bus.serial0.clear_output();
                *serial_cursor = 0;
            }
            Self::Mega { cpu, serial_cursor } => {
                cpu.bus.serial0.clear_output();
                *serial_cursor = 0;
            }
        }
    }

    pub fn run_chunk(&mut self, instruction_budget: usize) -> Result<RuntimeExit, CpuError> {
        match self {
            Self::Nano { cpu, .. } => run_chunk(cpu, instruction_budget),
            Self::Mega { cpu, .. } => run_chunk(cpu, instruction_budget),
        }
    }

    pub fn step_once(&mut self) -> Result<RuntimeExit, CpuError> {
        self.run_chunk(1)
    }

    pub fn take_new_serial_bytes(&mut self) -> Vec<u8> {
        match self {
            Self::Nano { cpu, serial_cursor } => {
                let start = *serial_cursor;
                *serial_cursor = cpu.bus.serial0.tx_log.len();
                cpu.bus.serial0.tx_log[start..].to_vec()
            }
            Self::Mega { cpu, serial_cursor } => {
                let start = *serial_cursor;
                *serial_cursor = cpu.bus.serial0.tx_log.len();
                cpu.bus.serial0.tx_log[start..].to_vec()
            }
        }
    }

    pub fn serial_output_bytes(&self) -> &[u8] {
        match self {
            Self::Nano { cpu, .. } => &cpu.bus.serial0.tx_log,
            Self::Mega { cpu, .. } => &cpu.bus.serial0.tx_log,
        }
    }

    pub fn host_pin_levels(&self) -> Vec<BoardPinLevel> {
        match self {
            Self::Nano { cpu, .. } => cpu.bus.host_pin_levels(),
            Self::Mega { cpu, .. } => cpu.bus.host_pin_levels(),
        }
    }

    pub fn configured_serial_baud(&self) -> u32 {
        match self {
            Self::Nano { cpu, .. } => configured_baud(
                cpu.bus.clock_hz,
                cpu.data[NANO_UCSR0A],
                cpu.data[NANO_UBRR0L],
                cpu.data[NANO_UBRR0H],
            ),
            Self::Mega { cpu, .. } => configured_baud(
                cpu.bus.clock_hz,
                cpu.data[MEGA_UCSR0A],
                cpu.data[MEGA_UBRR0L],
                cpu.data[MEGA_UBRR0H],
            ),
        }
    }

    pub fn pc(&self) -> u32 {
        match self {
            Self::Nano { cpu, .. } => cpu.pc,
            Self::Mega { cpu, .. } => cpu.pc,
        }
    }

    pub fn cycles(&self) -> u64 {
        match self {
            Self::Nano { cpu, .. } => cpu.cycles,
            Self::Mega { cpu, .. } => cpu.cycles,
        }
    }

    pub fn cpu_snapshot(&self) -> CpuSnapshot {
        match self {
            Self::Nano { cpu, .. } => CpuSnapshot {
                target: SimulationTarget::Nano,
                pc: cpu.pc,
                sp: cpu.sp(),
                cycles: cpu.cycles,
                synced_cycles: cpu.bus.synced_cycles,
                serial_bytes: cpu.bus.serial0.tx_log.len(),
                serial_rx_queued: cpu.bus.serial0.rx_queue.len(),
                sreg: cpu.data[cpu.config.sreg_address],
                registers: register_snapshot(cpu),
                next_instruction: format_next_instruction(cpu),
                extra_lines: vec![
                    format!(
                        "Timer0: pending={} rem={} | USART0: tx_busy={} tx_cycles={} rx_queue={}",
                        cpu.bus.timer0.interrupt_pending,
                        cpu.bus.timer0.cycle_remainder,
                        cpu.bus.serial0.tx_busy_byte.is_some(),
                        cpu.bus.serial0.tx_cycles_remaining,
                        cpu.bus.serial0.rx_queue.len()
                    ),
                    format!(
                        "SPI: active={} | TWI: active={} irq={} addr={} mode={}",
                        cpu.bus.spi_transaction_active,
                        cpu.bus.twi_bus_active,
                        cpu.bus.twi_interrupt_pending,
                        cpu.bus
                            .twi_address
                            .map(|value| format!("0x{value:02X}"))
                            .unwrap_or_else(|| "--".to_owned()),
                        if cpu.bus.twi_read_mode { "read" } else { "write" }
                    ),
                ],
            },
            Self::Mega { cpu, .. } => CpuSnapshot {
                target: SimulationTarget::Mega,
                pc: cpu.pc,
                sp: cpu.sp(),
                cycles: cpu.cycles,
                synced_cycles: cpu.bus.synced_cycles,
                serial_bytes: cpu.bus.serial0.tx_log.len(),
                serial_rx_queued: cpu.bus.serial0.rx_queue.len(),
                sreg: cpu.data[cpu.config.sreg_address],
                registers: register_snapshot(cpu),
                next_instruction: format_next_instruction(cpu),
                extra_lines: vec![
                    format!(
                        "Timer0: pending={} rem={} | USART0: tx_busy={} tx_cycles={} rx_queue={}",
                        cpu.bus.timer0.interrupt_pending,
                        cpu.bus.timer0.cycle_remainder,
                        cpu.bus.serial0.tx_busy_byte.is_some(),
                        cpu.bus.serial0.tx_cycles_remaining,
                        cpu.bus.serial0.rx_queue.len()
                    ),
                    format!(
                        "ADC: pending={} rem={} | SPI CAN: {} | SPI RTD: {} | EEPROM: {} bytes",
                        cpu.bus.adc.interrupt_pending,
                        cpu.bus.adc.cycles_remaining,
                        cpu.bus.spi_transaction_active_can,
                        cpu.bus.spi_transaction_active_rtd,
                        cpu.bus.eeprom.len()
                    ),
                ],
            },
        }
    }
}

fn register_snapshot<B: DataBus>(cpu: &Cpu<B>) -> [u8; 32] {
    let mut registers = [0u8; 32];
    registers.copy_from_slice(&cpu.data[..32]);
    registers
}

fn format_next_instruction<B: DataBus>(cpu: &Cpu<B>) -> String {
    match cpu.decode_at(cpu.pc) {
        Ok(decoded) => format_instruction(&decoded),
        Err(error) => format!("<decode error: {error}>"),
    }
}

fn format_instruction(instruction: &DecodedInstruction) -> String {
    let mnemonic = mnemonic_name(instruction.mnemonic);
    let operand_text = format_operands(instruction.mnemonic, &instruction.operands);

    if operand_text.is_empty() {
        format!(
            "0x{:06X}: {} ; opcode=0x{:04X}",
            instruction.address, mnemonic, instruction.opcode
        )
    } else {
        format!(
            "0x{:06X}: {} {} ; opcode=0x{:04X}",
            instruction.address, mnemonic, operand_text, instruction.opcode
        )
    }
}

fn format_operands(mnemonic: Mnemonic, operands: &OperandSet) -> String {
    let format_reg = |value: u8| format!("r{value}");
    let format_io = |value: u8| format!("0x{value:02X}");
    let format_data = |value: i32| format!("0x{:04X}", value as u16);

    match mnemonic {
        Mnemonic::Nop
        | Mnemonic::Break
        | Mnemonic::Sleep
        | Mnemonic::Wdr
        | Mnemonic::Ret
        | Mnemonic::Reti
        | Mnemonic::Ijmp
        | Mnemonic::Icall
        | Mnemonic::Eijmp
        | Mnemonic::Eicall
        | Mnemonic::Unsupported => String::new(),
        Mnemonic::Jmp | Mnemonic::Call => operands
            .k
            .map(|k| format!("0x{:06X}", k as u32))
            .unwrap_or_default(),
        Mnemonic::Lds => match (operands.d, operands.k) {
            (Some(d), Some(k)) => format!("{}, {}", format_reg(d), format_data(k)),
            _ => String::new(),
        },
        Mnemonic::Sts => match (operands.r, operands.k) {
            (Some(r), Some(k)) => format!("{}, {}", format_data(k), format_reg(r)),
            _ => String::new(),
        },
        Mnemonic::LdPtr | Mnemonic::Lpm => match (operands.d, operands.pointer, operands.mode) {
            (Some(d), Some(pointer), Some(mode)) => {
                format!("{}, {}", format_reg(d), format_pointer(pointer, mode))
            }
            (Some(d), _, _) => format_reg(d),
            _ => String::new(),
        },
        Mnemonic::StPtr => match (operands.r, operands.pointer, operands.mode) {
            (Some(r), Some(pointer), Some(mode)) => {
                format!("{}, {}", format_pointer(pointer, mode), format_reg(r))
            }
            _ => String::new(),
        },
        Mnemonic::LdDisp => match (operands.d, operands.pointer, operands.q) {
            (Some(d), Some(pointer), Some(q)) => {
                format!("{}, {}+{}", format_reg(d), format_pointer_base(pointer), q)
            }
            _ => String::new(),
        },
        Mnemonic::StDisp => match (operands.r, operands.pointer, operands.q) {
            (Some(r), Some(pointer), Some(q)) => {
                format!("{}+{}, {}", format_pointer_base(pointer), q, format_reg(r))
            }
            _ => String::new(),
        },
        Mnemonic::Des => operands
            .k
            .map(|k| k.to_string())
            .unwrap_or_default(),
        Mnemonic::Xch | Mnemonic::Lac | Mnemonic::Las | Mnemonic::Lat => operands
            .d
            .map(format_reg)
            .unwrap_or_default(),
        Mnemonic::Bset | Mnemonic::Bclr => operands
            .s
            .map(|s| s.to_string())
            .unwrap_or_default(),
        Mnemonic::Cbi | Mnemonic::Sbi | Mnemonic::Sbic | Mnemonic::Sbis => {
            match (operands.a, operands.b) {
                (Some(a), Some(b)) => format!("{}, {}", format_io(a), b),
                _ => String::new(),
            }
        }
        Mnemonic::Pop
        | Mnemonic::Push
        | Mnemonic::Com
        | Mnemonic::Neg
        | Mnemonic::Swap
        | Mnemonic::Inc
        | Mnemonic::Dec
        | Mnemonic::Asr
        | Mnemonic::Lsr
        | Mnemonic::Ror
        | Mnemonic::Bld
        | Mnemonic::Bst => operands
            .d
            .or(operands.r)
            .map(format_reg)
            .unwrap_or_default(),
        Mnemonic::Adiw | Mnemonic::Sbiw => match (operands.d, operands.k) {
            (Some(d), Some(k)) => format!("{}, {}", format_reg(d), k),
            _ => String::new(),
        },
        Mnemonic::Mov
        | Mnemonic::Movw
        | Mnemonic::Add
        | Mnemonic::Adc
        | Mnemonic::Sub
        | Mnemonic::Sbc
        | Mnemonic::Cp
        | Mnemonic::Cpc
        | Mnemonic::Cpse
        | Mnemonic::And
        | Mnemonic::Or
        | Mnemonic::Eor
        | Mnemonic::Mul
        | Mnemonic::Muls
        | Mnemonic::Mulsu
        | Mnemonic::Fmul
        | Mnemonic::Fmuls
        | Mnemonic::Fmulsu => match (operands.d, operands.r) {
            (Some(d), Some(r)) => format!("{}, {}", format_reg(d), format_reg(r)),
            _ => String::new(),
        },
        Mnemonic::Cpi | Mnemonic::Sbci | Mnemonic::Subi | Mnemonic::Ori | Mnemonic::Andi | Mnemonic::Ldi => {
            match (operands.d, operands.k) {
                (Some(d), Some(k)) => format!("{}, 0x{:02X}", format_reg(d), (k as u8)),
                _ => String::new(),
            }
        }
        Mnemonic::Rjmp | Mnemonic::Rcall | Mnemonic::Brbs | Mnemonic::Brbc => operands
            .k
            .map(|k| format!("{:+}", k))
            .unwrap_or_default(),
        Mnemonic::Sbrc | Mnemonic::Sbrs => match (operands.r, operands.b) {
            (Some(r), Some(b)) => format!("{}, {}", format_reg(r), b),
            _ => String::new(),
        },
        Mnemonic::In => match (operands.d, operands.a) {
            (Some(d), Some(a)) => format!("{}, {}", format_reg(d), format_io(a)),
            _ => String::new(),
        },
        Mnemonic::Out => match (operands.a, operands.r) {
            (Some(a), Some(r)) => format!("{}, {}", format_io(a), format_reg(r)),
            _ => String::new(),
        },
    }
}

fn format_pointer(pointer: PointerRegister, mode: PointerMode) -> String {
    match mode {
        PointerMode::Direct => format_pointer_base(pointer).to_owned(),
        PointerMode::PostIncrement => format!("{}+", format_pointer_base(pointer)),
        PointerMode::PreDecrement => format!("-{}", format_pointer_base(pointer)),
    }
}

fn format_pointer_base(pointer: PointerRegister) -> &'static str {
    match pointer {
        PointerRegister::X => "X",
        PointerRegister::Y => "Y",
        PointerRegister::Z => "Z",
    }
}

fn mnemonic_name(mnemonic: Mnemonic) -> &'static str {
    match mnemonic {
        Mnemonic::Nop => "nop",
        Mnemonic::Break => "break",
        Mnemonic::Sleep => "sleep",
        Mnemonic::Wdr => "wdr",
        Mnemonic::Ret => "ret",
        Mnemonic::Reti => "reti",
        Mnemonic::Ijmp => "ijmp",
        Mnemonic::Icall => "icall",
        Mnemonic::Eijmp => "eijmp",
        Mnemonic::Eicall => "eicall",
        Mnemonic::Jmp => "jmp",
        Mnemonic::Call => "call",
        Mnemonic::LdPtr => "ld",
        Mnemonic::StPtr => "st",
        Mnemonic::LdDisp => "ldd",
        Mnemonic::StDisp => "std",
        Mnemonic::Lpm => "lpm",
        Mnemonic::Des => "des",
        Mnemonic::Xch => "xch",
        Mnemonic::Lac => "lac",
        Mnemonic::Las => "las",
        Mnemonic::Lat => "lat",
        Mnemonic::Lds => "lds",
        Mnemonic::Sts => "sts",
        Mnemonic::Bset => "bset",
        Mnemonic::Bclr => "bclr",
        Mnemonic::Cbi => "cbi",
        Mnemonic::Sbi => "sbi",
        Mnemonic::Sbic => "sbic",
        Mnemonic::Sbis => "sbis",
        Mnemonic::Pop => "pop",
        Mnemonic::Push => "push",
        Mnemonic::Com => "com",
        Mnemonic::Neg => "neg",
        Mnemonic::Swap => "swap",
        Mnemonic::Inc => "inc",
        Mnemonic::Dec => "dec",
        Mnemonic::Asr => "asr",
        Mnemonic::Lsr => "lsr",
        Mnemonic::Ror => "ror",
        Mnemonic::Adiw => "adiw",
        Mnemonic::Sbiw => "sbiw",
        Mnemonic::Mov => "mov",
        Mnemonic::Movw => "movw",
        Mnemonic::Add => "add",
        Mnemonic::Adc => "adc",
        Mnemonic::Sub => "sub",
        Mnemonic::Sbc => "sbc",
        Mnemonic::Cp => "cp",
        Mnemonic::Cpc => "cpc",
        Mnemonic::Cpse => "cpse",
        Mnemonic::And => "and",
        Mnemonic::Or => "or",
        Mnemonic::Eor => "eor",
        Mnemonic::Cpi => "cpi",
        Mnemonic::Sbci => "sbci",
        Mnemonic::Subi => "subi",
        Mnemonic::Ori => "ori",
        Mnemonic::Andi => "andi",
        Mnemonic::Ldi => "ldi",
        Mnemonic::Rjmp => "rjmp",
        Mnemonic::Rcall => "rcall",
        Mnemonic::Brbs => "brbs",
        Mnemonic::Brbc => "brbc",
        Mnemonic::Bld => "bld",
        Mnemonic::Bst => "bst",
        Mnemonic::Sbrc => "sbrc",
        Mnemonic::Sbrs => "sbrs",
        Mnemonic::In => "in",
        Mnemonic::Out => "out",
        Mnemonic::Mul => "mul",
        Mnemonic::Muls => "muls",
        Mnemonic::Mulsu => "mulsu",
        Mnemonic::Fmul => "fmul",
        Mnemonic::Fmuls => "fmuls",
        Mnemonic::Fmulsu => "fmulsu",
        Mnemonic::Unsupported => "unsupported",
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
    instruction_budget: usize,
) -> Result<RuntimeExit, CpuError>
where
    B: DataBus,
{
    let mut executed = 0usize;
    while executed < instruction_budget {
        match cpu.step()? {
            StepOutcome::Executed => {
                executed += 1;
            }
            StepOutcome::BreakHit => {
                return Ok(RuntimeExit::BreakHit);
            }
            StepOutcome::Sleeping => {
                return Ok(RuntimeExit::Sleeping);
            }
        }
    }
    Ok(RuntimeExit::MaxInstructionsReached)
}

#[cfg(test)]
mod tests {
    use super::{RuntimeExit, SimulationRuntime, SimulationTarget};
    use crate::example_firmware::{MEGA_PIN_SWEEP, NANO_PIN_SWEEP};

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

    fn run_until_halt(runtime: &mut SimulationRuntime, instruction_budget: usize) {
        loop {
            let exit = runtime.run_chunk(instruction_budget).expect("run chunk");
            if matches!(exit, RuntimeExit::BreakHit | RuntimeExit::Sleeping) {
                break;
            }
        }
    }

    #[test]
    fn take_new_serial_bytes_preserves_output_from_previous_chunks() {
        let mut runtime = SimulationRuntime::new(SimulationTarget::Nano);
        let program = program_with_serial_tail(vec![
            ldi(16, 0x00),
            sts(16, rust_mcu::atmega328p::UBRR0L).0,
            sts(16, rust_mcu::atmega328p::UBRR0L).1,
            ldi(16, 1 << 3),
            sts(16, rust_mcu::atmega328p::UCSR0B).0,
            sts(16, rust_mcu::atmega328p::UCSR0B).1,
            ldi(16, b'W'),
            sts(16, rust_mcu::atmega328p::UDR0).0,
            sts(16, rust_mcu::atmega328p::UDR0).1,
        ]);

        runtime.load_hex(&make_hex(&program)).expect("load hex");
        run_until_halt(&mut runtime, 64);

        assert_eq!(runtime.serial_output_bytes(), b"W");
        assert_eq!(runtime.take_new_serial_bytes(), vec![b'W']);
        assert!(runtime.take_new_serial_bytes().is_empty());
    }

    #[test]
    fn bundled_examples_emit_serial_output() {
        for example in [NANO_PIN_SWEEP, MEGA_PIN_SWEEP] {
            let mut runtime = SimulationRuntime::new(example.target);
            runtime.load_hex(example.hex).expect("load example hex");

            let mut emitted_serial = false;
            for _ in 0..400 {
                runtime.run_chunk(20_000).expect("run example");
                if !runtime.serial_output_bytes().is_empty() {
                    emitted_serial = true;
                    break;
                }
            }

            assert!(
                emitted_serial,
                "{} should emit serial output early in execution",
                example.label
            );
        }
    }
}
