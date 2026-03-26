pub mod atmega2560;
pub mod atmega328p;
pub mod common;

pub use atmega2560::{Atmega2560Bus, MegaBoard, NullMegaBoard};
pub use atmega328p::{Atmega328pBus, NanoBoard, NullNanoBoard};
pub use common::{BoardPin, BoardPinLevel, PinMode, SpiSettings};

#[cfg(test)]
mod tests {
    use rust_cpu::{Cpu, CpuConfig, DataBus};

    use super::atmega2560::{
        Atmega2560Bus, NullMegaBoard, ADCH, ADCL, ADCSRA, ADCSRB, DDRL, PINL, PORTL,
    };
    use super::atmega328p::{
        Atmega328pBus, NullNanoBoard, DDRB, PINB, PORTB, UBRR0L, UCSR0B, UDR0,
    };
    use super::common::BoardPin;

    const TXEN0: u8 = 1 << 3;
    const ADEN: u8 = 1 << 7;
    const ADSC: u8 = 1 << 6;
    const MUX5: u8 = 1 << 3;

    #[test]
    fn nano_bus_captures_serial_output() {
        let mut bus = Atmega328pBus::new(NullNanoBoard::default(), 16_000_000);
        let config = CpuConfig::atmega328p();
        let mut data = vec![0u8; config.data_size_bytes];
        bus.reset(&config, &mut data);

        assert!(bus.write_data(&config, &mut data, 0, UBRR0L, 0));
        assert!(bus.write_data(&config, &mut data, 0, UCSR0B, TXEN0));
        assert!(bus.write_data(&config, &mut data, 0, UDR0, b'A'));
        bus.after_step(
            &config,
            &mut data,
            0,
            200,
            &rust_cpu::DecodedInstruction {
                address: 0,
                opcode: 0,
                next_word: None,
                mnemonic: rust_cpu::Mnemonic::Nop,
                word_length: 1,
                operands: rust_cpu::OperandSet::default(),
            },
            1,
        );

        assert_eq!(bus.serial0.tx_log, vec![b'A']);
    }

    #[test]
    fn mega_cpu_can_complete_adc_conversion() {
        let bus = Atmega2560Bus::new(NullMegaBoard::default(), 16_000_000);
        let mut cpu = Cpu::new(CpuConfig::atmega2560(), bus);

        cpu.write_data(ADCSRB, MUX5).unwrap();
        cpu.write_data(ADCSRA, ADEN | ADSC | 0x07).unwrap();
        cpu.cycles += 1700;
        {
            let config = cpu.config.clone();
            let pc = cpu.pc;
            let cycles = cpu.cycles;
            let instruction = rust_cpu::DecodedInstruction {
                address: 0,
                opcode: 0,
                next_word: None,
                mnemonic: rust_cpu::Mnemonic::Nop,
                word_length: 1,
                operands: rust_cpu::OperandSet::default(),
            };
            let bus = &mut cpu.bus;
            let data = &mut cpu.data;
            bus.after_step(&config, data, pc, cycles, &instruction, 1);
        }

        let value = cpu.data[ADCL] as u16 | ((cpu.data[ADCH] as u16) << 8);
        assert_eq!(value, 0);
        assert_eq!(cpu.data[ADCSRA] & ADSC, 0);
    }

    #[test]
    fn nano_bus_reports_host_pin_levels() {
        let mut bus = Atmega328pBus::new(NullNanoBoard::default(), 16_000_000);
        let config = CpuConfig::atmega328p();
        let mut data = vec![0u8; config.data_size_bytes];
        bus.reset(&config, &mut data);

        assert!(bus.write_data(&config, &mut data, 0, DDRB, 1 << 5));
        assert!(bus.write_data(&config, &mut data, 0, PORTB, 1 << 5));

        let levels = bus.host_pin_levels();
        let d13 = levels
            .into_iter()
            .find(|entry| entry.pin == BoardPin::Digital(13))
            .expect("D13 snapshot");
        assert_eq!(d13.level, 1);
        assert_eq!(bus.read_data(&config, &mut data, 0, PINB), Some(1 << 5));
    }

    #[test]
    fn mega_bus_reports_host_pin_levels() {
        let mut bus = Atmega2560Bus::new(NullMegaBoard::default(), 16_000_000);
        let config = CpuConfig::atmega2560();
        let mut data = vec![0u8; config.data_size_bytes];
        bus.reset(&config, &mut data);

        assert!(bus.write_data(&config, &mut data, 0, DDRL, 1 << 5));
        assert!(bus.write_data(&config, &mut data, 0, PORTL, 1 << 5));

        let levels = bus.host_pin_levels();
        let d44 = levels
            .into_iter()
            .find(|entry| entry.pin == BoardPin::Digital(44))
            .expect("D44 snapshot");
        assert_eq!(d44.level, 1);
        assert_eq!(bus.read_data(&config, &mut data, 0, PINL), Some(1 << 5));
    }
}
