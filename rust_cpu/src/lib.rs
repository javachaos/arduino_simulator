pub mod bus;
pub mod config;
pub mod cpu;
pub mod error;
pub mod instruction;

pub use bus::{DataBus, NullBus};
pub use config::CpuConfig;
pub use cpu::{Cpu, StepOutcome};
pub use error::CpuError;
pub use instruction::{DecodedInstruction, Mnemonic, OperandSet, PointerMode, PointerRegister};

#[cfg(test)]
mod tests {
    use super::{Cpu, CpuConfig, NullBus, StepOutcome};

    fn ldi(d: u8, k: u8) -> u16 {
        0xE000
            | (((k as u16) & 0xF0) << 4)
            | ((((d - 16) as u16) & 0x0F) << 4)
            | ((k as u16) & 0x0F)
    }

    #[test]
    fn cpu_can_execute_ldi_and_break() {
        let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
        cpu.load_program_words(&[ldi(16, 0x42), 0x9598], 0).unwrap();

        assert_eq!(cpu.run(Some(2)).unwrap(), 2);
        assert_eq!(cpu.read_register(16).unwrap(), 0x42);
        assert!(cpu.break_hit);
    }

    #[test]
    fn cpu_can_step_until_sleep() {
        let mut cpu = Cpu::new(CpuConfig::atmega328p(), NullBus);
        cpu.load_program_words(&[0x9588], 0).unwrap();

        let outcome = cpu.step().unwrap();
        assert_eq!(outcome, StepOutcome::Sleeping);
        assert!(cpu.sleeping);
    }
}
