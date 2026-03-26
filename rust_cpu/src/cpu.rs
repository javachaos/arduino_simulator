use crate::bus::DataBus;
use crate::config::CpuConfig;
use crate::error::CpuError;
use crate::instruction::{DecodedInstruction, Mnemonic, OperandSet, PointerMode, PointerRegister};

pub const FLAG_C: u8 = 0;
pub const FLAG_Z: u8 = 1;
pub const FLAG_N: u8 = 2;
pub const FLAG_V: u8 = 3;
pub const FLAG_S: u8 = 4;
pub const FLAG_H: u8 = 5;
pub const FLAG_T: u8 = 6;
pub const FLAG_I: u8 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOutcome {
    Executed,
    BreakHit,
    Sleeping,
}

pub struct Cpu<B: DataBus> {
    pub config: CpuConfig,
    pub bus: B,
    pub program: Vec<u8>,
    pub data: Vec<u8>,
    pub pc: u32,
    pub cycles: u64,
    pub break_hit: bool,
    pub sleeping: bool,
    interrupt_delay_steps: u8,
}

impl<B: DataBus> Cpu<B> {
    pub fn new(config: CpuConfig, bus: B) -> Self {
        let mut cpu = Self {
            program: vec![0xFF; config.program_size_bytes],
            data: vec![0x00; config.data_size_bytes],
            pc: 0,
            cycles: 0,
            break_hit: false,
            sleeping: false,
            interrupt_delay_steps: 0,
            bus,
            config,
        };
        cpu.reset(false);
        cpu
    }

    pub fn reset(&mut self, clear_program: bool) {
        self.data.fill(0);
        if clear_program {
            self.program.fill(0xFF);
        }
        self.pc = 0;
        self.cycles = 0;
        self.break_hit = false;
        self.sleeping = false;
        self.interrupt_delay_steps = 0;
        self.set_sp(self.config.stack_reset_value());
        self.bus.reset(&self.config, &mut self.data);
    }

    pub fn load_program_bytes(
        &mut self,
        program_bytes: &[u8],
        start_byte: usize,
    ) -> Result<(), CpuError> {
        let end = start_byte
            .checked_add(program_bytes.len())
            .ok_or(CpuError::ProgramBounds)?;
        if end > self.program.len() {
            return Err(CpuError::ProgramBounds);
        }
        self.program[start_byte..end].copy_from_slice(program_bytes);
        Ok(())
    }

    pub fn load_program_words(&mut self, words: &[u16], start_word: usize) -> Result<(), CpuError> {
        let mut program_bytes = Vec::with_capacity(words.len() * 2);
        for word in words {
            program_bytes.push((word & 0x00FF) as u8);
            program_bytes.push((word >> 8) as u8);
        }
        self.load_program_bytes(&program_bytes, start_word * 2)
    }

    pub fn set_program_word(&mut self, address: usize, word: u16) -> Result<(), CpuError> {
        if address >= self.config.program_size_words() {
            return Err(CpuError::ProgramBounds);
        }
        let byte_address = address * 2;
        self.program[byte_address] = (word & 0x00FF) as u8;
        self.program[byte_address + 1] = (word >> 8) as u8;
        Ok(())
    }

    pub fn read_register(&self, index: usize) -> Result<u8, CpuError> {
        if index >= 32 {
            return Err(CpuError::InvalidRegister);
        }
        Ok(self.data[index])
    }

    pub fn write_register(&mut self, index: usize, value: u8) -> Result<(), CpuError> {
        if index >= 32 {
            return Err(CpuError::InvalidRegister);
        }
        self.data[index] = value;
        Ok(())
    }

    pub fn read_data(&mut self, address: usize) -> Result<u8, CpuError> {
        if address >= self.data.len() {
            return Err(CpuError::DataBounds);
        }
        if let Some(value) = self
            .bus
            .read_data(&self.config, &mut self.data, self.cycles, address)
        {
            return Ok(value);
        }
        Ok(self.data[address])
    }

    pub fn write_data(&mut self, address: usize, value: u8) -> Result<(), CpuError> {
        if address >= self.data.len() {
            return Err(CpuError::DataBounds);
        }
        if self
            .bus
            .write_data(&self.config, &mut self.data, self.cycles, address, value)
        {
            return Ok(());
        }
        self.data[address] = value;
        Ok(())
    }

    pub fn read_io(&mut self, address: usize) -> Result<u8, CpuError> {
        self.read_data(0x20 + address)
    }

    pub fn write_io(&mut self, address: usize, value: u8) -> Result<(), CpuError> {
        self.write_data(0x20 + address, value)
    }

    pub fn fetch_word(&self, address: u32) -> Result<u16, CpuError> {
        let word_address = address as usize;
        if word_address >= self.config.program_size_words() {
            return Err(CpuError::ProgramBounds);
        }
        let byte_address = word_address * 2;
        Ok(self.program[byte_address] as u16 | ((self.program[byte_address + 1] as u16) << 8))
    }

    pub fn read_program_byte(&self, byte_address: usize) -> u8 {
        self.program.get(byte_address).copied().unwrap_or(0xFF)
    }

    pub fn step(&mut self) -> Result<StepOutcome, CpuError> {
        if self.sleeping {
            return Err(CpuError::Sleeping);
        }
        let instruction = self.decode_at(self.pc)?;
        self.execute(&instruction)?;
        if self.break_hit {
            Ok(StepOutcome::BreakHit)
        } else if self.sleeping {
            Ok(StepOutcome::Sleeping)
        } else {
            Ok(StepOutcome::Executed)
        }
    }

    pub fn run(&mut self, max_instructions: Option<usize>) -> Result<usize, CpuError> {
        let mut executed = 0usize;
        while max_instructions
            .map(|limit| executed < limit)
            .unwrap_or(true)
        {
            match self.step()? {
                StepOutcome::Executed => executed += 1,
                StepOutcome::BreakHit | StepOutcome::Sleeping => {
                    executed += 1;
                    break;
                }
            }
        }
        Ok(executed)
    }

    pub fn take_interrupt(
        &mut self,
        vector_number: u8,
        latency_cycles: u8,
    ) -> Result<(), CpuError> {
        if vector_number == 0 {
            return Err(CpuError::UnsupportedMnemonic("interrupt vector 0"));
        }
        self.sleeping = false;
        self.push_pc(self.pc)?;
        self.set_flag(FLAG_I, false);
        self.pc = self.normalize_pc((vector_number as u32) * 2);
        self.cycles += latency_cycles as u64;
        self.bus.on_interrupt(
            &self.config,
            &mut self.data,
            self.pc,
            self.cycles,
            vector_number,
            latency_cycles,
        );
        Ok(())
    }

    pub fn get_flag(&self, bit: u8) -> bool {
        ((self.data[self.config.sreg_address] >> bit) & 0x01) != 0
    }

    pub fn set_flag(&mut self, bit: u8, enabled: bool) {
        let mask = 1u8 << bit;
        if enabled {
            self.data[self.config.sreg_address] |= mask;
        } else {
            self.data[self.config.sreg_address] &= !mask;
        }
    }

    pub fn sp(&self) -> u16 {
        self.data[self.config.spl_address] as u16
            | ((self.data[self.config.sph_address] as u16) << 8)
    }

    pub fn set_sp(&mut self, value: u16) {
        self.data[self.config.spl_address] = (value & 0x00FF) as u8;
        self.data[self.config.sph_address] = (value >> 8) as u8;
    }

    fn push_byte(&mut self, value: u8) -> Result<(), CpuError> {
        let sp = self.sp() as usize;
        self.write_data(sp, value)?;
        self.set_sp((sp as u16).wrapping_sub(1));
        Ok(())
    }

    fn pop_byte(&mut self) -> Result<u8, CpuError> {
        let new_sp = self.sp().wrapping_add(1);
        self.set_sp(new_sp);
        self.read_data(new_sp as usize)
    }

    fn push_pc(&mut self, pc: u32) -> Result<(), CpuError> {
        for shift in (0..self.config.return_address_bytes()).rev() {
            self.push_byte(((pc >> (shift * 8)) & 0xFF) as u8)?;
        }
        Ok(())
    }

    fn pop_return_address(&mut self) -> Result<u32, CpuError> {
        let mut value = 0u32;
        for shift in 0..self.config.return_address_bytes() {
            value |= (self.pop_byte()? as u32) << (shift * 8);
        }
        Ok(value & self.config.pc_mask())
    }

    fn normalize_pc(&self, value: u32) -> u32 {
        value & self.config.pc_mask()
    }

    pub fn decode_at(&self, address: u32) -> Result<DecodedInstruction, CpuError> {
        let opcode = self.fetch_word(address)?;
        let next_word = if matches!(opcode & 0xFE0F, 0x9000 | 0x9200)
            || (opcode & 0xFE0E) == 0x940C
            || (opcode & 0xFE0E) == 0x940E
        {
            Some(self.fetch_word(address + 1)?)
        } else {
            None
        };

        let single = |mnemonic: Mnemonic, operands: OperandSet| DecodedInstruction {
            address,
            opcode,
            next_word: None,
            mnemonic,
            word_length: 1,
            operands,
        };
        let double = |mnemonic: Mnemonic, operands: OperandSet| DecodedInstruction {
            address,
            opcode,
            next_word,
            mnemonic,
            word_length: 2,
            operands,
        };

        let register_d = ((opcode >> 4) & 0x1F) as u8;
        let register_r = ((opcode & 0x0F) | ((opcode >> 5) & 0x10)) as u8;
        let immediate = ((opcode & 0x0F) | ((opcode >> 4) & 0xF0)) as i32;
        let immediate_d = (16 + ((opcode >> 4) & 0x0F)) as u8;

        let instruction = if opcode == 0x0000 {
            single(Mnemonic::Nop, OperandSet::default())
        } else if opcode == 0x9598 {
            single(Mnemonic::Break, OperandSet::default())
        } else if opcode == 0x9588 {
            single(Mnemonic::Sleep, OperandSet::default())
        } else if opcode == 0x95A8 {
            single(Mnemonic::Wdr, OperandSet::default())
        } else if opcode == 0x9508 {
            single(Mnemonic::Ret, OperandSet::default())
        } else if opcode == 0x9518 {
            single(Mnemonic::Reti, OperandSet::default())
        } else if opcode == 0x9409 {
            single(Mnemonic::Ijmp, OperandSet::default())
        } else if opcode == 0x9509 {
            single(Mnemonic::Icall, OperandSet::default())
        } else if opcode == 0x9419 {
            single(Mnemonic::Eijmp, OperandSet::default())
        } else if opcode == 0x9519 {
            single(Mnemonic::Eicall, OperandSet::default())
        } else if (opcode & 0xFE0E) == 0x940C {
            let target = (((opcode & 0x01F0) as u32) << 13)
                | (((opcode & 0x0001) as u32) << 16)
                | next_word.unwrap_or(0) as u32;
            double(
                Mnemonic::Jmp,
                OperandSet {
                    k: Some(target as i32),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0E) == 0x940E {
            let target = (((opcode & 0x01F0) as u32) << 13)
                | (((opcode & 0x0001) as u32) << 16)
                | next_word.unwrap_or(0) as u32;
            double(
                Mnemonic::Call,
                OperandSet {
                    k: Some(target as i32),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9000 {
            double(
                Mnemonic::Lds,
                OperandSet {
                    d: Some(register_d),
                    k: Some(next_word.unwrap_or(0) as i32),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9200 {
            double(
                Mnemonic::Sts,
                OperandSet {
                    r: Some(register_d),
                    k: Some(next_word.unwrap_or(0) as i32),
                    ..OperandSet::default()
                },
            )
        } else if opcode == 0x95C8 {
            single(
                Mnemonic::Lpm,
                OperandSet {
                    d: Some(0),
                    pointer: Some(PointerRegister::Z),
                    mode: Some(PointerMode::Direct),
                    ..OperandSet::default()
                },
            )
        } else if opcode == 0x95D8 {
            single(
                Mnemonic::Lpm,
                OperandSet {
                    d: Some(0),
                    pointer: Some(PointerRegister::X),
                    mode: Some(PointerMode::Direct),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFF0F) == 0x940B {
            single(
                Mnemonic::Des,
                OperandSet {
                    k: Some(((opcode >> 4) & 0x0F) as i32),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9004 {
            single(
                Mnemonic::Lpm,
                OperandSet {
                    d: Some(register_d),
                    pointer: Some(PointerRegister::Z),
                    mode: Some(PointerMode::Direct),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9005 {
            single(
                Mnemonic::Lpm,
                OperandSet {
                    d: Some(register_d),
                    pointer: Some(PointerRegister::Z),
                    mode: Some(PointerMode::PostIncrement),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9006 {
            single(
                Mnemonic::Lpm,
                OperandSet {
                    d: Some(register_d),
                    pointer: Some(PointerRegister::X),
                    mode: Some(PointerMode::Direct),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9007 {
            single(
                Mnemonic::Lpm,
                OperandSet {
                    d: Some(register_d),
                    pointer: Some(PointerRegister::X),
                    mode: Some(PointerMode::PostIncrement),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9204 {
            single(
                Mnemonic::Xch,
                OperandSet {
                    d: Some(register_d),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9205 {
            single(
                Mnemonic::Las,
                OperandSet {
                    d: Some(register_d),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9206 {
            single(
                Mnemonic::Lac,
                OperandSet {
                    d: Some(register_d),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9207 {
            single(
                Mnemonic::Lat,
                OperandSet {
                    d: Some(register_d),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x900C {
            single(
                Mnemonic::LdPtr,
                OperandSet {
                    d: Some(register_d),
                    pointer: Some(PointerRegister::X),
                    mode: Some(PointerMode::Direct),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x900D {
            single(
                Mnemonic::LdPtr,
                OperandSet {
                    d: Some(register_d),
                    pointer: Some(PointerRegister::X),
                    mode: Some(PointerMode::PostIncrement),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x900E {
            single(
                Mnemonic::LdPtr,
                OperandSet {
                    d: Some(register_d),
                    pointer: Some(PointerRegister::X),
                    mode: Some(PointerMode::PreDecrement),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x920C {
            single(
                Mnemonic::StPtr,
                OperandSet {
                    r: Some(register_d),
                    pointer: Some(PointerRegister::X),
                    mode: Some(PointerMode::Direct),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x920D {
            single(
                Mnemonic::StPtr,
                OperandSet {
                    r: Some(register_d),
                    pointer: Some(PointerRegister::X),
                    mode: Some(PointerMode::PostIncrement),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x920E {
            single(
                Mnemonic::StPtr,
                OperandSet {
                    r: Some(register_d),
                    pointer: Some(PointerRegister::X),
                    mode: Some(PointerMode::PreDecrement),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9009 {
            single(
                Mnemonic::LdPtr,
                OperandSet {
                    d: Some(register_d),
                    pointer: Some(PointerRegister::Y),
                    mode: Some(PointerMode::PostIncrement),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x900A {
            single(
                Mnemonic::LdPtr,
                OperandSet {
                    d: Some(register_d),
                    pointer: Some(PointerRegister::Y),
                    mode: Some(PointerMode::PreDecrement),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9001 {
            single(
                Mnemonic::LdPtr,
                OperandSet {
                    d: Some(register_d),
                    pointer: Some(PointerRegister::Z),
                    mode: Some(PointerMode::PostIncrement),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9002 {
            single(
                Mnemonic::LdPtr,
                OperandSet {
                    d: Some(register_d),
                    pointer: Some(PointerRegister::Z),
                    mode: Some(PointerMode::PreDecrement),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9209 {
            single(
                Mnemonic::StPtr,
                OperandSet {
                    r: Some(register_d),
                    pointer: Some(PointerRegister::Y),
                    mode: Some(PointerMode::PostIncrement),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x920A {
            single(
                Mnemonic::StPtr,
                OperandSet {
                    r: Some(register_d),
                    pointer: Some(PointerRegister::Y),
                    mode: Some(PointerMode::PreDecrement),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9201 {
            single(
                Mnemonic::StPtr,
                OperandSet {
                    r: Some(register_d),
                    pointer: Some(PointerRegister::Z),
                    mode: Some(PointerMode::PostIncrement),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9202 {
            single(
                Mnemonic::StPtr,
                OperandSet {
                    r: Some(register_d),
                    pointer: Some(PointerRegister::Z),
                    mode: Some(PointerMode::PreDecrement),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFF8F) == 0x9408 {
            single(
                Mnemonic::Bset,
                OperandSet {
                    s: Some(((opcode >> 4) & 0x07) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFF8F) == 0x9488 {
            single(
                Mnemonic::Bclr,
                OperandSet {
                    s: Some(((opcode >> 4) & 0x07) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFF00) == 0x9800 {
            single(
                Mnemonic::Cbi,
                OperandSet {
                    a: Some(((opcode >> 3) & 0x1F) as u8),
                    b: Some((opcode & 0x07) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFF00) == 0x9A00 {
            single(
                Mnemonic::Sbi,
                OperandSet {
                    a: Some(((opcode >> 3) & 0x1F) as u8),
                    b: Some((opcode & 0x07) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFF00) == 0x9900 {
            single(
                Mnemonic::Sbic,
                OperandSet {
                    a: Some(((opcode >> 3) & 0x1F) as u8),
                    b: Some((opcode & 0x07) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFF00) == 0x9B00 {
            single(
                Mnemonic::Sbis,
                OperandSet {
                    a: Some(((opcode >> 3) & 0x1F) as u8),
                    b: Some((opcode & 0x07) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x900F {
            single(
                Mnemonic::Pop,
                OperandSet {
                    d: Some(register_d),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x920F {
            single(
                Mnemonic::Push,
                OperandSet {
                    r: Some(register_d),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9400 {
            single(
                Mnemonic::Com,
                OperandSet {
                    d: Some(register_d),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9401 {
            single(
                Mnemonic::Neg,
                OperandSet {
                    d: Some(register_d),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9402 {
            single(
                Mnemonic::Swap,
                OperandSet {
                    d: Some(register_d),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9403 {
            single(
                Mnemonic::Inc,
                OperandSet {
                    d: Some(register_d),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9405 {
            single(
                Mnemonic::Asr,
                OperandSet {
                    d: Some(register_d),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9406 {
            single(
                Mnemonic::Lsr,
                OperandSet {
                    d: Some(register_d),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x9407 {
            single(
                Mnemonic::Ror,
                OperandSet {
                    d: Some(register_d),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE0F) == 0x940A {
            single(
                Mnemonic::Dec,
                OperandSet {
                    d: Some(register_d),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFF00) == 0x9600 {
            single(
                Mnemonic::Adiw,
                OperandSet {
                    d: Some((24 + (((opcode >> 4) & 0x03) * 2)) as u8),
                    k: Some(((opcode & 0x0F) | ((opcode >> 2) & 0x30)) as i32),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFF00) == 0x9700 {
            single(
                Mnemonic::Sbiw,
                OperandSet {
                    d: Some((24 + (((opcode >> 4) & 0x03) * 2)) as u8),
                    k: Some(((opcode & 0x0F) | ((opcode >> 2) & 0x30)) as i32),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFF00) == 0x0100 {
            single(
                Mnemonic::Movw,
                OperandSet {
                    d: Some((((opcode >> 4) & 0x0F) * 2) as u8),
                    r: Some(((opcode & 0x0F) * 2) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFF88) == 0x0300 {
            single(
                Mnemonic::Mulsu,
                OperandSet {
                    d: Some((16 + ((opcode >> 4) & 0x07)) as u8),
                    r: Some((16 + (opcode & 0x07)) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFF88) == 0x0308 {
            single(
                Mnemonic::Fmul,
                OperandSet {
                    d: Some((16 + ((opcode >> 4) & 0x07)) as u8),
                    r: Some((16 + (opcode & 0x07)) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFF88) == 0x0380 {
            single(
                Mnemonic::Fmuls,
                OperandSet {
                    d: Some((16 + ((opcode >> 4) & 0x07)) as u8),
                    r: Some((16 + (opcode & 0x07)) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFF88) == 0x0388 {
            single(
                Mnemonic::Fmulsu,
                OperandSet {
                    d: Some((16 + ((opcode >> 4) & 0x07)) as u8),
                    r: Some((16 + (opcode & 0x07)) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFC00) == 0x9C00 {
            single(
                Mnemonic::Mul,
                OperandSet {
                    d: Some(register_d),
                    r: Some(register_r),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFF00) == 0x0200 {
            single(
                Mnemonic::Muls,
                OperandSet {
                    d: Some((16 + ((opcode >> 4) & 0x0F)) as u8),
                    r: Some((16 + (opcode & 0x0F)) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFC00) == 0x2C00 {
            single(
                Mnemonic::Mov,
                OperandSet {
                    d: Some(register_d),
                    r: Some(register_r),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFC00) == 0x0C00 {
            single(
                Mnemonic::Add,
                OperandSet {
                    d: Some(register_d),
                    r: Some(register_r),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFC00) == 0x1C00 {
            single(
                Mnemonic::Adc,
                OperandSet {
                    d: Some(register_d),
                    r: Some(register_r),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFC00) == 0x1800 {
            single(
                Mnemonic::Sub,
                OperandSet {
                    d: Some(register_d),
                    r: Some(register_r),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFC00) == 0x0800 {
            single(
                Mnemonic::Sbc,
                OperandSet {
                    d: Some(register_d),
                    r: Some(register_r),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFC00) == 0x1400 {
            single(
                Mnemonic::Cp,
                OperandSet {
                    d: Some(register_d),
                    r: Some(register_r),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFC00) == 0x0400 {
            single(
                Mnemonic::Cpc,
                OperandSet {
                    d: Some(register_d),
                    r: Some(register_r),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFC00) == 0x1000 {
            single(
                Mnemonic::Cpse,
                OperandSet {
                    d: Some(register_d),
                    r: Some(register_r),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFC00) == 0x2000 {
            single(
                Mnemonic::And,
                OperandSet {
                    d: Some(register_d),
                    r: Some(register_r),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFC00) == 0x2800 {
            single(
                Mnemonic::Or,
                OperandSet {
                    d: Some(register_d),
                    r: Some(register_r),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFC00) == 0x2400 {
            single(
                Mnemonic::Eor,
                OperandSet {
                    d: Some(register_d),
                    r: Some(register_r),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xF000) == 0x3000 {
            single(
                Mnemonic::Cpi,
                OperandSet {
                    d: Some(immediate_d),
                    k: Some(immediate),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xF000) == 0x4000 {
            single(
                Mnemonic::Sbci,
                OperandSet {
                    d: Some(immediate_d),
                    k: Some(immediate),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xF000) == 0x5000 {
            single(
                Mnemonic::Subi,
                OperandSet {
                    d: Some(immediate_d),
                    k: Some(immediate),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xF000) == 0x6000 {
            single(
                Mnemonic::Ori,
                OperandSet {
                    d: Some(immediate_d),
                    k: Some(immediate),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xF000) == 0x7000 {
            single(
                Mnemonic::Andi,
                OperandSet {
                    d: Some(immediate_d),
                    k: Some(immediate),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xF000) == 0xE000 {
            single(
                Mnemonic::Ldi,
                OperandSet {
                    d: Some(immediate_d),
                    k: Some(immediate),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xF000) == 0xC000 {
            single(
                Mnemonic::Rjmp,
                OperandSet {
                    k: Some(sign_extend((opcode & 0x0FFF) as u32, 12)),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xF000) == 0xD000 {
            single(
                Mnemonic::Rcall,
                OperandSet {
                    k: Some(sign_extend((opcode & 0x0FFF) as u32, 12)),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFC00) == 0xF000 {
            single(
                Mnemonic::Brbs,
                OperandSet {
                    s: Some((opcode & 0x07) as u8),
                    k: Some(sign_extend(((opcode >> 3) & 0x7F) as u32, 7)),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFC00) == 0xF400 {
            single(
                Mnemonic::Brbc,
                OperandSet {
                    s: Some((opcode & 0x07) as u8),
                    k: Some(sign_extend(((opcode >> 3) & 0x7F) as u32, 7)),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE08) == 0xF800 {
            single(
                Mnemonic::Bld,
                OperandSet {
                    d: Some(register_d),
                    b: Some((opcode & 0x07) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE08) == 0xFA00 {
            single(
                Mnemonic::Bst,
                OperandSet {
                    d: Some(register_d),
                    b: Some((opcode & 0x07) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE08) == 0xFC00 {
            single(
                Mnemonic::Sbrc,
                OperandSet {
                    r: Some(register_d),
                    b: Some((opcode & 0x07) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xFE08) == 0xFE00 {
            single(
                Mnemonic::Sbrs,
                OperandSet {
                    r: Some(register_d),
                    b: Some((opcode & 0x07) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xF800) == 0xB000 {
            single(
                Mnemonic::In,
                OperandSet {
                    d: Some(register_d),
                    a: Some(((opcode & 0x0F) | ((opcode >> 5) & 0x30)) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xF800) == 0xB800 {
            single(
                Mnemonic::Out,
                OperandSet {
                    r: Some(register_d),
                    a: Some(((opcode & 0x0F) | ((opcode >> 5) & 0x30)) as u8),
                    ..OperandSet::default()
                },
            )
        } else if (opcode & 0xC000) == 0x8000 {
            let displacement =
                (((opcode >> 8) & 0x20) | ((opcode >> 7) & 0x18) | (opcode & 0x07)) as u8;
            let pointer = if (opcode & 0x0008) != 0 {
                PointerRegister::Y
            } else {
                PointerRegister::Z
            };
            if (opcode & 0x0200) != 0 {
                single(
                    Mnemonic::StDisp,
                    OperandSet {
                        r: Some(register_d),
                        q: Some(displacement),
                        pointer: Some(pointer),
                        ..OperandSet::default()
                    },
                )
            } else {
                single(
                    Mnemonic::LdDisp,
                    OperandSet {
                        d: Some(register_d),
                        q: Some(displacement),
                        pointer: Some(pointer),
                        ..OperandSet::default()
                    },
                )
            }
        } else {
            DecodedInstruction {
                address,
                opcode,
                next_word,
                mnemonic: Mnemonic::Unsupported,
                word_length: if next_word.is_some() { 2 } else { 1 },
                operands: OperandSet::default(),
            }
        };
        Ok(instruction)
    }

    fn execute(&mut self, instruction: &DecodedInstruction) -> Result<(), CpuError> {
        let mut next_pc = instruction.address + instruction.word_length as u32;
        let mut cycles = 1u8;
        let op = &instruction.operands;

        match instruction.mnemonic {
            Mnemonic::Nop => {}
            Mnemonic::Break => self.break_hit = true,
            Mnemonic::Sleep => self.sleeping = true,
            Mnemonic::Wdr => {}
            Mnemonic::Ijmp => {
                next_pc = self.pointer_word_address(PointerRegister::Z, None)?;
                cycles = 2;
            }
            Mnemonic::Eijmp => {
                self.ensure_extended_pc("eijmp")?;
                next_pc =
                    self.pointer_word_address(PointerRegister::Z, self.config.eind_address)?;
                cycles = 2;
            }
            Mnemonic::Icall => {
                self.push_pc(next_pc)?;
                next_pc = self.pointer_word_address(PointerRegister::Z, None)?;
                cycles = if self.config.return_address_bytes() == 3 {
                    4
                } else {
                    3
                };
            }
            Mnemonic::Eicall => {
                self.ensure_extended_pc("eicall")?;
                self.push_pc(next_pc)?;
                next_pc =
                    self.pointer_word_address(PointerRegister::Z, self.config.eind_address)?;
                cycles = 4;
            }
            Mnemonic::Ldi => self.data[op.d.unwrap() as usize] = op.k.unwrap() as u8,
            Mnemonic::Mov => {
                self.data[op.d.unwrap() as usize] = self.data[op.r.unwrap() as usize];
            }
            Mnemonic::Movw => {
                let d = op.d.unwrap() as usize;
                let r = op.r.unwrap() as usize;
                self.data[d] = self.data[r];
                self.data[d + 1] = self.data[r + 1];
            }
            Mnemonic::Add => {
                let d = op.d.unwrap() as usize;
                let r = op.r.unwrap() as usize;
                let result = self.add8(self.data[d], self.data[r], false);
                self.data[d] = result;
            }
            Mnemonic::Adc => {
                let d = op.d.unwrap() as usize;
                let r = op.r.unwrap() as usize;
                let result = self.add8(self.data[d], self.data[r], self.get_flag(FLAG_C));
                self.data[d] = result;
            }
            Mnemonic::Sub => {
                let d = op.d.unwrap() as usize;
                let r = op.r.unwrap() as usize;
                let result = self.sub8(self.data[d], self.data[r], false, None);
                self.data[d] = result;
            }
            Mnemonic::Sbc => {
                let d = op.d.unwrap() as usize;
                let r = op.r.unwrap() as usize;
                let result = self.sub8(
                    self.data[d],
                    self.data[r],
                    self.get_flag(FLAG_C),
                    Some(self.get_flag(FLAG_Z)),
                );
                self.data[d] = result;
            }
            Mnemonic::Cp => {
                let d = op.d.unwrap() as usize;
                let r = op.r.unwrap() as usize;
                self.sub8(self.data[d], self.data[r], false, None);
            }
            Mnemonic::Cpc => {
                let d = op.d.unwrap() as usize;
                let r = op.r.unwrap() as usize;
                self.sub8(
                    self.data[d],
                    self.data[r],
                    self.get_flag(FLAG_C),
                    Some(self.get_flag(FLAG_Z)),
                );
            }
            Mnemonic::Cpse => {
                if self.data[op.d.unwrap() as usize] == self.data[op.r.unwrap() as usize] {
                    let skipped = self.decode_at(next_pc)?;
                    next_pc += skipped.word_length as u32;
                    cycles = if skipped.word_length == 2 { 3 } else { 2 };
                }
            }
            Mnemonic::Cpi => {
                self.sub8(
                    self.data[op.d.unwrap() as usize],
                    op.k.unwrap() as u8,
                    false,
                    None,
                );
            }
            Mnemonic::Subi => {
                let d = op.d.unwrap() as usize;
                let result = self.sub8(self.data[d], op.k.unwrap() as u8, false, None);
                self.data[d] = result;
            }
            Mnemonic::Sbci => {
                let d = op.d.unwrap() as usize;
                let result = self.sub8(
                    self.data[d],
                    op.k.unwrap() as u8,
                    self.get_flag(FLAG_C),
                    Some(self.get_flag(FLAG_Z)),
                );
                self.data[d] = result;
            }
            Mnemonic::And => {
                let d = op.d.unwrap() as usize;
                let result = self.data[d] & self.data[op.r.unwrap() as usize];
                self.data[d] = result;
                self.set_logic_flags(result);
            }
            Mnemonic::Or => {
                let d = op.d.unwrap() as usize;
                let result = self.data[d] | self.data[op.r.unwrap() as usize];
                self.data[d] = result;
                self.set_logic_flags(result);
            }
            Mnemonic::Eor => {
                let d = op.d.unwrap() as usize;
                let result = self.data[d] ^ self.data[op.r.unwrap() as usize];
                self.data[d] = result;
                self.set_logic_flags(result);
            }
            Mnemonic::Andi => {
                let d = op.d.unwrap() as usize;
                let result = self.data[d] & op.k.unwrap() as u8;
                self.data[d] = result;
                self.set_logic_flags(result);
            }
            Mnemonic::Ori => {
                let d = op.d.unwrap() as usize;
                let result = self.data[d] | op.k.unwrap() as u8;
                self.data[d] = result;
                self.set_logic_flags(result);
            }
            Mnemonic::Com => {
                let d = op.d.unwrap() as usize;
                let result = !self.data[d];
                self.data[d] = result;
                self.set_flag(FLAG_C, true);
                self.set_logic_flags(result);
            }
            Mnemonic::Neg => {
                let d = op.d.unwrap() as usize;
                let result = 0u8.wrapping_sub(self.data[d]);
                self.data[d] = result;
                self.set_flag(FLAG_C, result != 0);
                self.set_flag(FLAG_H, (result & 0x0F) != 0);
                self.set_flag(FLAG_V, result == 0x80);
                self.set_flag(FLAG_N, (result & 0x80) != 0);
                self.set_flag(FLAG_Z, result == 0);
                self.set_flag(FLAG_S, self.get_flag(FLAG_N) ^ self.get_flag(FLAG_V));
            }
            Mnemonic::Swap => {
                let d = op.d.unwrap() as usize;
                let value = self.data[d];
                self.data[d] = (value << 4) | (value >> 4);
            }
            Mnemonic::Inc => {
                let d = op.d.unwrap() as usize;
                let result = self.data[d].wrapping_add(1);
                self.data[d] = result;
                self.set_flag(FLAG_V, result == 0x80);
                self.set_flag(FLAG_N, (result & 0x80) != 0);
                self.set_flag(FLAG_Z, result == 0);
                self.set_flag(FLAG_S, self.get_flag(FLAG_N) ^ self.get_flag(FLAG_V));
            }
            Mnemonic::Dec => {
                let d = op.d.unwrap() as usize;
                let result = self.data[d].wrapping_sub(1);
                self.data[d] = result;
                self.set_flag(FLAG_V, result == 0x7F);
                self.set_flag(FLAG_N, (result & 0x80) != 0);
                self.set_flag(FLAG_Z, result == 0);
                self.set_flag(FLAG_S, self.get_flag(FLAG_N) ^ self.get_flag(FLAG_V));
            }
            Mnemonic::Asr => {
                let d = op.d.unwrap() as usize;
                let value = self.data[d];
                let result = (value >> 1) | (value & 0x80);
                self.data[d] = result;
                self.shift_flags(value, result);
            }
            Mnemonic::Lsr => {
                let d = op.d.unwrap() as usize;
                let value = self.data[d];
                let result = value >> 1;
                self.data[d] = result;
                self.shift_flags(value, result);
            }
            Mnemonic::Ror => {
                let d = op.d.unwrap() as usize;
                let value = self.data[d];
                let result = (value >> 1) | if self.get_flag(FLAG_C) { 0x80 } else { 0x00 };
                self.data[d] = result;
                self.shift_flags(value, result);
            }
            Mnemonic::Adiw => {
                let d = op.d.unwrap() as usize;
                let value = self.data[d] as u16 | ((self.data[d + 1] as u16) << 8);
                let result = value.wrapping_add(op.k.unwrap() as u16);
                self.data[d] = (result & 0x00FF) as u8;
                self.data[d + 1] = (result >> 8) as u8;
                self.set_flag(FLAG_V, (value & 0x8000) == 0 && (result & 0x8000) != 0);
                self.set_flag(FLAG_N, (result & 0x8000) != 0);
                self.set_flag(FLAG_Z, result == 0);
                self.set_flag(FLAG_C, (value & 0x8000) != 0 && (result & 0x8000) == 0);
                self.set_flag(FLAG_S, self.get_flag(FLAG_N) ^ self.get_flag(FLAG_V));
                cycles = 2;
            }
            Mnemonic::Sbiw => {
                let d = op.d.unwrap() as usize;
                let value = self.data[d] as u16 | ((self.data[d + 1] as u16) << 8);
                let result = value.wrapping_sub(op.k.unwrap() as u16);
                self.data[d] = (result & 0x00FF) as u8;
                self.data[d + 1] = (result >> 8) as u8;
                self.set_flag(FLAG_V, (value & 0x8000) != 0 && (result & 0x8000) == 0);
                self.set_flag(FLAG_N, (result & 0x8000) != 0);
                self.set_flag(FLAG_Z, result == 0);
                self.set_flag(FLAG_C, (value & 0x8000) == 0 && (result & 0x8000) != 0);
                self.set_flag(FLAG_S, self.get_flag(FLAG_N) ^ self.get_flag(FLAG_V));
                cycles = 2;
            }
            Mnemonic::In => {
                self.data[op.d.unwrap() as usize] = self.read_io(op.a.unwrap() as usize)?;
            }
            Mnemonic::Out => {
                let value = self.data[op.r.unwrap() as usize];
                self.write_io(op.a.unwrap() as usize, value)?;
            }
            Mnemonic::Cbi => {
                let address = 0x20 + op.a.unwrap() as usize;
                let value = self.read_data(address)? & !(1u8 << op.b.unwrap());
                self.write_data(address, value)?;
                cycles = 2;
            }
            Mnemonic::Sbi => {
                let address = 0x20 + op.a.unwrap() as usize;
                let value = self.read_data(address)? | (1u8 << op.b.unwrap());
                self.write_data(address, value)?;
                cycles = 2;
            }
            Mnemonic::Sbic | Mnemonic::Sbis => {
                let address = 0x20 + op.a.unwrap() as usize;
                let value = self.read_data(address)?;
                let bit_set = ((value >> op.b.unwrap()) & 0x01) != 0;
                let should_skip = if matches!(instruction.mnemonic, Mnemonic::Sbic) {
                    !bit_set
                } else {
                    bit_set
                };
                cycles = 1;
                if should_skip {
                    let skipped = self.decode_at(next_pc)?;
                    next_pc += skipped.word_length as u32;
                    cycles = if skipped.word_length == 2 { 3 } else { 2 };
                }
            }
            Mnemonic::Bset => {
                self.set_flag(op.s.unwrap(), true);
                if op.s.unwrap() == FLAG_I {
                    self.interrupt_delay_steps = self.interrupt_delay_steps.max(2);
                }
            }
            Mnemonic::Bclr => self.set_flag(op.s.unwrap(), false),
            Mnemonic::Bld => {
                let mask = 1u8 << op.b.unwrap();
                let d = op.d.unwrap() as usize;
                if self.get_flag(FLAG_T) {
                    self.data[d] |= mask;
                } else {
                    self.data[d] &= !mask;
                }
            }
            Mnemonic::Bst => {
                let d = op.d.unwrap() as usize;
                let bit = ((self.data[d] >> op.b.unwrap()) & 0x01) != 0;
                self.set_flag(FLAG_T, bit);
            }
            Mnemonic::Sbrc | Mnemonic::Sbrs => {
                let bit = ((self.data[op.r.unwrap() as usize] >> op.b.unwrap()) & 0x01) != 0;
                let should_skip = if matches!(instruction.mnemonic, Mnemonic::Sbrc) {
                    !bit
                } else {
                    bit
                };
                if should_skip {
                    let skipped = self.decode_at(next_pc)?;
                    next_pc += skipped.word_length as u32;
                    cycles = if skipped.word_length == 2 { 3 } else { 2 };
                }
            }
            Mnemonic::Rjmp => {
                next_pc =
                    self.normalize_pc(((instruction.address as i32) + 1 + op.k.unwrap()) as u32);
                cycles = 2;
            }
            Mnemonic::Rcall => {
                self.push_pc(next_pc)?;
                next_pc =
                    self.normalize_pc(((instruction.address as i32) + 1 + op.k.unwrap()) as u32);
                cycles = 3;
            }
            Mnemonic::Jmp => {
                next_pc = self.normalize_pc(op.k.unwrap() as u32);
                cycles = 3;
            }
            Mnemonic::Call => {
                self.push_pc(next_pc)?;
                next_pc = self.normalize_pc(op.k.unwrap() as u32);
                cycles = if self.config.return_address_bytes() == 3 {
                    5
                } else {
                    4
                };
            }
            Mnemonic::Ret => {
                next_pc = self.pop_return_address()?;
                cycles = if self.config.return_address_bytes() == 3 {
                    5
                } else {
                    4
                };
            }
            Mnemonic::Reti => {
                next_pc = self.pop_return_address()?;
                self.set_flag(FLAG_I, true);
                self.interrupt_delay_steps = self.interrupt_delay_steps.max(2);
                cycles = if self.config.return_address_bytes() == 3 {
                    5
                } else {
                    4
                };
            }
            Mnemonic::Lds => {
                let address = self.direct_data_address(op.k.unwrap() as u16)?;
                self.data[op.d.unwrap() as usize] = self.read_data(address)?;
                cycles = 2;
            }
            Mnemonic::Sts => {
                let address = self.direct_data_address(op.k.unwrap() as u16)?;
                self.write_data(address, self.data[op.r.unwrap() as usize])?;
                cycles = 2;
            }
            Mnemonic::Push => {
                self.push_byte(self.data[op.r.unwrap() as usize])?;
                cycles = 2;
            }
            Mnemonic::Pop => {
                self.data[op.d.unwrap() as usize] = self.pop_byte()?;
                cycles = 2;
            }
            Mnemonic::Brbs | Mnemonic::Brbc => {
                let bit_set = self.get_flag(op.s.unwrap());
                let take_branch = if matches!(instruction.mnemonic, Mnemonic::Brbs) {
                    bit_set
                } else {
                    !bit_set
                };
                if take_branch {
                    next_pc = self
                        .normalize_pc(((instruction.address as i32) + 1 + op.k.unwrap()) as u32);
                    cycles = 2;
                }
            }
            Mnemonic::Mul => {
                let result = (self.data[op.d.unwrap() as usize] as i32)
                    * (self.data[op.r.unwrap() as usize] as i32);
                self.write_product(result);
                cycles = 2;
            }
            Mnemonic::Muls => {
                let left = self.data[op.d.unwrap() as usize] as i8 as i16;
                let right = self.data[op.r.unwrap() as usize] as i8 as i16;
                self.write_product((left * right) as i32);
                cycles = 2;
            }
            Mnemonic::Mulsu => {
                let left = self.data[op.d.unwrap() as usize] as i8 as i16;
                let right = self.data[op.r.unwrap() as usize] as i16;
                self.write_product((left * right) as i32);
                cycles = 2;
            }
            Mnemonic::Fmul => {
                let raw = (self.data[op.d.unwrap() as usize] as i32)
                    * (self.data[op.r.unwrap() as usize] as i32);
                self.write_fractional_product(raw);
                cycles = 2;
            }
            Mnemonic::Fmuls => {
                let left = self.data[op.d.unwrap() as usize] as i8 as i16;
                let right = self.data[op.r.unwrap() as usize] as i8 as i16;
                self.write_fractional_product((left * right) as i32);
                cycles = 2;
            }
            Mnemonic::Fmulsu => {
                let left = self.data[op.d.unwrap() as usize] as i8 as i16;
                let right = self.data[op.r.unwrap() as usize] as i16;
                self.write_fractional_product((left * right) as i32);
                cycles = 2;
            }
            Mnemonic::LdPtr => {
                self.data[op.d.unwrap() as usize] =
                    self.read_pointer_mode(op.pointer.unwrap(), op.mode.unwrap())?;
                cycles = 2;
            }
            Mnemonic::StPtr => {
                self.write_pointer_mode(
                    op.pointer.unwrap(),
                    op.mode.unwrap(),
                    self.data[op.r.unwrap() as usize],
                )?;
                cycles = 2;
            }
            Mnemonic::LdDisp => {
                let address = self
                    .pointer_data_address(op.pointer.unwrap())?
                    .wrapping_add(op.q.unwrap() as u32);
                self.data[op.d.unwrap() as usize] = self.read_data(address as usize)?;
                cycles = 2;
            }
            Mnemonic::StDisp => {
                let address = self
                    .pointer_data_address(op.pointer.unwrap())?
                    .wrapping_add(op.q.unwrap() as u32);
                self.write_data(address as usize, self.data[op.r.unwrap() as usize])?;
                cycles = 2;
            }
            Mnemonic::Lpm => {
                let extended =
                    matches!(op.pointer.unwrap_or(PointerRegister::Z), PointerRegister::X);
                let post_increment = matches!(
                    op.mode.unwrap_or(PointerMode::Direct),
                    PointerMode::PostIncrement
                );
                let byte_address = self.program_pointer_byte_address(extended)?;
                self.data[op.d.unwrap() as usize] = self.read_program_byte(byte_address as usize);
                if post_increment {
                    self.increment_program_pointer(extended)?;
                }
                cycles = 3;
            }
            Mnemonic::Des => {
                return Err(if self.config.supports_des {
                    CpuError::UnsupportedMnemonic("des")
                } else {
                    CpuError::InstructionUnavailable {
                        instruction: "des",
                        device: self.config.name,
                    }
                });
            }
            Mnemonic::Xch => {
                let address = self.pointer_data_address(PointerRegister::Z)?;
                self.ensure_sram_operation_address(address, "xch")?;
                let memory_value = self.read_data(address as usize)?;
                let register_value = self.data[op.d.unwrap() as usize];
                self.write_data(address as usize, register_value)?;
                self.data[op.d.unwrap() as usize] = memory_value;
                cycles = 2;
            }
            Mnemonic::Lac => {
                let address = self.pointer_data_address(PointerRegister::Z)?;
                self.ensure_sram_operation_address(address, "lac")?;
                let memory_value = self.read_data(address as usize)?;
                let register_value = self.data[op.d.unwrap() as usize];
                self.data[op.d.unwrap() as usize] = memory_value;
                self.write_data(address as usize, (!register_value) & memory_value)?;
                cycles = 2;
            }
            Mnemonic::Las => {
                let address = self.pointer_data_address(PointerRegister::Z)?;
                self.ensure_sram_operation_address(address, "las")?;
                let memory_value = self.read_data(address as usize)?;
                let register_value = self.data[op.d.unwrap() as usize];
                self.data[op.d.unwrap() as usize] = memory_value;
                self.write_data(address as usize, register_value | memory_value)?;
                cycles = 2;
            }
            Mnemonic::Lat => {
                let address = self.pointer_data_address(PointerRegister::Z)?;
                self.ensure_sram_operation_address(address, "lat")?;
                let memory_value = self.read_data(address as usize)?;
                let register_value = self.data[op.d.unwrap() as usize];
                self.data[op.d.unwrap() as usize] = memory_value;
                self.write_data(address as usize, register_value ^ memory_value)?;
                cycles = 2;
            }
            _ => {
                return Err(match instruction.mnemonic {
                    Mnemonic::Unsupported => CpuError::UnsupportedInstruction {
                        opcode: instruction.opcode,
                        address: instruction.address,
                    },
                    other => CpuError::UnsupportedMnemonic(other_name(other)),
                });
            }
        }

        self.pc = self.normalize_pc(next_pc);
        self.cycles += cycles as u64;
        self.bus.after_step(
            &self.config,
            &mut self.data,
            self.pc,
            self.cycles,
            instruction,
            cycles,
        );
        if self.interrupt_delay_steps > 0 {
            self.interrupt_delay_steps -= 1;
        }
        if self.interrupt_delay_steps == 0 && self.get_flag(FLAG_I) {
            if let Some(vector_number) =
                self.bus
                    .pending_interrupt(&self.config, &mut self.data, self.pc, self.cycles)
            {
                self.take_interrupt(vector_number, 4)?;
            }
        }
        Ok(())
    }

    fn add8(&mut self, left: u8, right: u8, carry_in: bool) -> u8 {
        let carry = if carry_in { 1u16 } else { 0u16 };
        let result16 = left as u16 + right as u16 + carry;
        let result = result16 as u8;
        self.set_flag(
            FLAG_H,
            ((left & 0x0F) + (right & 0x0F) + carry as u8) > 0x0F,
        );
        self.set_flag(FLAG_V, ((left ^ !right) & (left ^ result) & 0x80) != 0);
        self.set_flag(FLAG_N, (result & 0x80) != 0);
        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_C, result16 > 0xFF);
        self.set_flag(FLAG_S, self.get_flag(FLAG_N) ^ self.get_flag(FLAG_V));
        result
    }

    fn sub8(&mut self, left: u8, right: u8, carry_in: bool, previous_z: Option<bool>) -> u8 {
        let carry = if carry_in { 1u16 } else { 0u16 };
        let result16 = (left as i16) - (right as i16) - (carry as i16);
        let result = result16 as u8;
        self.set_flag(
            FLAG_H,
            ((!left & right) | (right & result) | (result & !left)) & 0x08 != 0,
        );
        self.set_flag(
            FLAG_V,
            ((left & !right & !result) | (!left & right & result)) & 0x80 != 0,
        );
        self.set_flag(FLAG_N, (result & 0x80) != 0);
        self.set_flag(FLAG_C, result16 < 0);
        self.set_flag(
            FLAG_Z,
            match previous_z {
                Some(old) => old && result == 0,
                None => result == 0,
            },
        );
        self.set_flag(FLAG_S, self.get_flag(FLAG_N) ^ self.get_flag(FLAG_V));
        result
    }

    fn set_logic_flags(&mut self, result: u8) {
        self.set_flag(FLAG_V, false);
        self.set_flag(FLAG_N, (result & 0x80) != 0);
        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_S, self.get_flag(FLAG_N) ^ self.get_flag(FLAG_V));
    }

    fn shift_flags(&mut self, original: u8, result: u8) {
        self.set_flag(FLAG_C, (original & 0x01) != 0);
        self.set_flag(FLAG_N, (result & 0x80) != 0);
        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_V, self.get_flag(FLAG_N) ^ self.get_flag(FLAG_C));
        self.set_flag(FLAG_S, self.get_flag(FLAG_N) ^ self.get_flag(FLAG_V));
    }

    fn direct_data_address(&mut self, low_word_address: u16) -> Result<usize, CpuError> {
        let mut address = low_word_address as u32;
        if let Some(rampd) = self.config.rampd_address {
            address |= (self.read_data(rampd)? as u32) << 16;
        }
        usize::try_from(address).map_err(|_| CpuError::DataBounds)
    }

    fn pointer_data_address(&mut self, pointer: PointerRegister) -> Result<u32, CpuError> {
        let (low_reg, high_reg, ramp) = match pointer {
            PointerRegister::X => (26usize, 27usize, self.config.rampx_address),
            PointerRegister::Y => (28usize, 29usize, self.config.rampy_address),
            PointerRegister::Z => (30usize, 31usize, self.config.rampz_address),
        };
        let mut value =
            (self.read_register(low_reg)? as u32) | ((self.read_register(high_reg)? as u32) << 8);
        if let Some(ramp_address) = ramp {
            value |= (self.read_data(ramp_address)? as u32) << 16;
        }
        Ok(value)
    }

    fn write_pointer_data_address(
        &mut self,
        pointer: PointerRegister,
        value: u32,
    ) -> Result<(), CpuError> {
        let value = value & 0x00FF_FFFF;
        let (low_reg, high_reg, ramp) = match pointer {
            PointerRegister::X => (26usize, 27usize, self.config.rampx_address),
            PointerRegister::Y => (28usize, 29usize, self.config.rampy_address),
            PointerRegister::Z => (30usize, 31usize, self.config.rampz_address),
        };
        self.write_register(low_reg, (value & 0xFF) as u8)?;
        self.write_register(high_reg, ((value >> 8) & 0xFF) as u8)?;
        if let Some(ramp_address) = ramp {
            self.write_data(ramp_address, ((value >> 16) & 0xFF) as u8)?;
        }
        Ok(())
    }

    fn pointer_word_address(
        &mut self,
        pointer: PointerRegister,
        upper_address: Option<usize>,
    ) -> Result<u32, CpuError> {
        let (low_reg, high_reg) = match pointer {
            PointerRegister::X => (26usize, 27usize),
            PointerRegister::Y => (28usize, 29usize),
            PointerRegister::Z => (30usize, 31usize),
        };
        let mut value =
            (self.read_register(low_reg)? as u32) | ((self.read_register(high_reg)? as u32) << 8);
        if let Some(address) = upper_address {
            value |= (self.read_data(address)? as u32) << 16;
        }
        Ok(self.normalize_pc(value))
    }

    fn program_pointer_byte_address(&mut self, extended: bool) -> Result<u32, CpuError> {
        let mut value = (self.read_register(30)? as u32) | ((self.read_register(31)? as u32) << 8);
        if extended {
            if let Some(rampz) = self.config.rampz_address {
                value |= (self.read_data(rampz)? as u32) << 16;
            }
        }
        Ok(value)
    }

    fn increment_program_pointer(&mut self, extended: bool) -> Result<(), CpuError> {
        let value = (self.program_pointer_byte_address(extended)? + 1) & 0x00FF_FFFF;
        self.write_register(30, (value & 0xFF) as u8)?;
        self.write_register(31, ((value >> 8) & 0xFF) as u8)?;
        if extended {
            if let Some(rampz) = self.config.rampz_address {
                self.write_data(rampz, ((value >> 16) & 0xFF) as u8)?;
            }
        }
        Ok(())
    }

    fn read_pointer_mode(
        &mut self,
        pointer: PointerRegister,
        mode: PointerMode,
    ) -> Result<u8, CpuError> {
        let mut address = self.pointer_data_address(pointer)?;
        if matches!(mode, PointerMode::PreDecrement) {
            address = address.wrapping_sub(1) & 0x00FF_FFFF;
            self.write_pointer_data_address(pointer, address)?;
        }
        let value = self.read_data(address as usize)?;
        if matches!(mode, PointerMode::PostIncrement) {
            self.write_pointer_data_address(pointer, address.wrapping_add(1) & 0x00FF_FFFF)?;
        }
        Ok(value)
    }

    fn write_pointer_mode(
        &mut self,
        pointer: PointerRegister,
        mode: PointerMode,
        value: u8,
    ) -> Result<(), CpuError> {
        let mut address = self.pointer_data_address(pointer)?;
        if matches!(mode, PointerMode::PreDecrement) {
            address = address.wrapping_sub(1) & 0x00FF_FFFF;
            self.write_pointer_data_address(pointer, address)?;
        }
        self.write_data(address as usize, value)?;
        if matches!(mode, PointerMode::PostIncrement) {
            self.write_pointer_data_address(pointer, address.wrapping_add(1) & 0x00FF_FFFF)?;
        }
        Ok(())
    }

    fn ensure_sram_operation_address(
        &self,
        address: u32,
        instruction: &'static str,
    ) -> Result<(), CpuError> {
        if address < self.config.sram_start_address as u32 || address as usize >= self.data.len() {
            return Err(CpuError::InvalidSramOperation {
                instruction,
                address,
            });
        }
        Ok(())
    }

    fn ensure_extended_pc(&self, instruction: &'static str) -> Result<(), CpuError> {
        if self.config.return_address_bytes() != 3 {
            return Err(CpuError::ExtendedPcRequired(instruction));
        }
        Ok(())
    }

    fn write_product(&mut self, product: i32) {
        let result = (product as i64 & 0xFFFF) as u16;
        self.data[0] = (result & 0x00FF) as u8;
        self.data[1] = (result >> 8) as u8;
        self.set_flag(FLAG_Z, result == 0);
        self.set_flag(FLAG_C, (result & 0x8000) != 0);
    }

    fn write_fractional_product(&mut self, raw_product: i32) {
        let raw = (raw_product as i64 & 0xFFFF) as u16;
        self.set_flag(FLAG_C, (raw & 0x8000) != 0);
        let result = raw.wrapping_shl(1);
        self.data[0] = (result & 0x00FF) as u8;
        self.data[1] = (result >> 8) as u8;
        self.set_flag(FLAG_Z, result == 0);
    }
}

fn sign_extend(value: u32, bits: u8) -> i32 {
    let sign_bit = 1u32 << (bits - 1);
    let mask = (1u32 << bits) - 1;
    let value = value & mask;
    if (value & sign_bit) != 0 {
        (value as i32) - (1i32 << bits)
    } else {
        value as i32
    }
}

fn other_name(mnemonic: Mnemonic) -> &'static str {
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
        Mnemonic::Lds => "lds",
        Mnemonic::Sts => "sts",
        Mnemonic::LdPtr => "ld_ptr",
        Mnemonic::StPtr => "st_ptr",
        Mnemonic::LdDisp => "ld_disp",
        Mnemonic::StDisp => "st_disp",
        Mnemonic::Lpm => "lpm",
        Mnemonic::Des => "des",
        Mnemonic::Xch => "xch",
        Mnemonic::Lac => "lac",
        Mnemonic::Las => "las",
        Mnemonic::Lat => "lat",
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
