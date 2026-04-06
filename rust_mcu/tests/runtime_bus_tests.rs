use std::collections::{HashMap, VecDeque};

use rust_cpu::{Cpu, CpuConfig, DataBus, DecodedInstruction, Mnemonic, OperandSet};
use rust_mcu::atmega2560::{
    Atmega2560Bus, MegaBoard, NullMegaBoard, ADCH, ADCL, ADCSRA, ADCSRB, ADMUX, DDRA, DDRC, EEARH,
    EEARL, EECR, EEDR, OCR5BL, PORTA, PORTC, SPCR as MEGA_SPCR, SPDR as MEGA_SPDR,
    TCCR0B as MEGA_TCCR0B, TCNT0 as MEGA_TCNT0, TIMSK0 as MEGA_TIMSK0,
    UBRR0L as MEGA_UBRR0L, UCSR0A as MEGA_UCSR0A, UCSR0B as MEGA_UCSR0B, UDR0 as MEGA_UDR0,
};
use rust_mcu::atmega328p::{
    Atmega328pBus, NanoBoard, NullNanoBoard, DDRB, DDRD, PORTB, PORTD, SPCR as NANO_SPCR,
    SPDR as NANO_SPDR, TCCR0B as NANO_TCCR0B, TCNT0 as NANO_TCNT0, TIMSK0 as NANO_TIMSK0, TWCR,
    TWDR, TWSR, UBRR0L as NANO_UBRR0L, UCSR0A as NANO_UCSR0A, UCSR0B as NANO_UCSR0B,
    UDR0 as NANO_UDR0,
};
use rust_mcu::{BoardPin, PinMode, SpiSettings};

const TOIE0: u8 = 1 << 0;
const TXEN0: u8 = 1 << 3;
const RXEN0: u8 = 1 << 4;
const MSTR: u8 = 1 << 4;
const SPE: u8 = 1 << 6;
const UDRE0: u8 = 1 << 5;
const ADSC: u8 = 1 << 6;
const ADEN: u8 = 1 << 7;
const ADIF: u8 = 1 << 4;
const RXC0: u8 = 1 << 7;
const MUX5: u8 = 1 << 3;
const TWEN: u8 = 1 << 2;
const TWSTO: u8 = 1 << 4;
const TWSTA: u8 = 1 << 5;
const TWEA: u8 = 1 << 6;
const TWINT: u8 = 1 << 7;
const TW_START: u8 = 0x08;
const TW_MT_SLA_ACK: u8 = 0x18;
const TW_MT_DATA_ACK: u8 = 0x28;
const TW_MR_SLA_ACK: u8 = 0x40;
const TW_MR_DATA_ACK: u8 = 0x50;
const TW_MR_DATA_NACK: u8 = 0x58;

fn ldi(d: u8, k: u8) -> u16 {
    assert!((16..=31).contains(&d));
    0xE000 | (((k as u16) & 0xF0) << 4) | (((d - 16) as u16) << 4) | ((k as u16) & 0x0F)
}

fn out(a: u8, r: u8) -> u16 {
    0xB800 | (((a as u16) & 0x30) << 5) | (((r as u16) & 0x1F) << 4) | ((a as u16) & 0x0F)
}

fn sts(r: u8, address: usize) -> (u16, u16) {
    (
        0x9200 | (((r as u16) & 0x1F) << 4),
        (address & 0xFFFF) as u16,
    )
}

fn bset(bit_index: u8) -> u16 {
    0x9408 | (((bit_index as u16) & 0x07) << 4)
}

fn inc(d: u8) -> u16 {
    0x9403 | (((d as u16) & 0x1F) << 4)
}

fn reti() -> u16 {
    0x9518
}

fn brk() -> u16 {
    0x9598
}

fn advance_cycles<B: DataBus>(cpu: &mut Cpu<B>, extra_cycles: u64) {
    cpu.cycles += extra_cycles;
    let config = cpu.config.clone();
    let pc = cpu.pc;
    let cycles = cpu.cycles;
    let instruction = DecodedInstruction {
        address: pc,
        opcode: 0,
        next_word: None,
        mnemonic: Mnemonic::Nop,
        word_length: 1,
        operands: OperandSet::default(),
    };
    cpu.bus
        .after_step(&config, &mut cpu.data, pc, cycles, &instruction, 1);
}

fn encode_measurement_payload(temp_c: f64, rh_percent: f64) -> [u8; 6] {
    let raw_temp = (((temp_c + 45.0) / 175.0) * 65535.0).round() as u16;
    let raw_rh = ((rh_percent / 100.0) * 65535.0).round() as u16;
    [
        ((raw_temp >> 8) & 0xFF) as u8,
        (raw_temp & 0xFF) as u8,
        0,
        ((raw_rh >> 8) & 0xFF) as u8,
        (raw_rh & 0xFF) as u8,
        0,
    ]
}

fn decode_measurement_payload(payload: &[u8]) -> (f64, f64) {
    let raw_temp = ((payload[0] as u16) << 8) | (payload[1] as u16);
    let raw_rh = ((payload[3] as u16) << 8) | (payload[4] as u16);
    let temp_c = -45.0 + (175.0 * f64::from(raw_temp) / 65535.0);
    let rh_percent = 100.0 * f64::from(raw_rh) / 65535.0;
    (temp_c, rh_percent)
}

#[derive(Default)]
struct TestNanoBoard {
    pin_modes: HashMap<BoardPin, PinMode>,
    pin_levels: HashMap<BoardPin, u8>,
    spi_log: Vec<u8>,
    spi_responses: VecDeque<u8>,
    i2c_preview_ok: bool,
    i2c_reads: HashMap<u8, Vec<u8>>,
    i2c_writes: Vec<(u8, Vec<u8>)>,
}

impl TestNanoBoard {
    fn level(&self, pin: BoardPin) -> u8 {
        *self.pin_levels.get(&pin).unwrap_or(&0)
    }
}

impl NanoBoard for TestNanoBoard {
    fn advance_time_ms(&mut self, _elapsed_ms: f64) {}

    fn read_pin(&self, pin: BoardPin) -> u8 {
        self.level(pin)
    }

    fn set_pin_mode(&mut self, pin: BoardPin, mode: PinMode) {
        self.pin_modes.insert(pin, mode);
    }

    fn write_pin(&mut self, pin: BoardPin, level: u8) {
        self.pin_levels.insert(pin, level);
    }

    fn begin_spi(&mut self, _settings: SpiSettings) {}

    fn transfer_spi(&mut self, value: u8, _settings: SpiSettings) -> u8 {
        self.spi_log.push(value);
        self.spi_responses.pop_front().unwrap_or(0)
    }

    fn i2c_preview_write(&self, _address: u8, _payload: &[u8]) -> bool {
        self.i2c_preview_ok
    }

    fn i2c_write(&mut self, address: u8, payload: &[u8]) -> bool {
        self.i2c_writes.push((address, payload.to_vec()));
        self.i2c_preview_ok
    }

    fn i2c_read(&mut self, address: u8, _length: usize) -> Option<Vec<u8>> {
        self.i2c_reads.get(&address).cloned()
    }
}

#[derive(Default)]
struct TestMegaBoard {
    pin_modes: HashMap<BoardPin, PinMode>,
    pin_levels: HashMap<BoardPin, u8>,
    analog_counts: HashMap<BoardPin, u16>,
    can_log: Vec<u8>,
    can_responses: VecDeque<u8>,
    rtd_log: Vec<u8>,
    rtd_responses: VecDeque<u8>,
    pwm_duty: HashMap<BoardPin, u8>,
}

impl TestMegaBoard {
    fn level(&self, pin: BoardPin) -> u8 {
        *self.pin_levels.get(&pin).unwrap_or(&0)
    }

    fn sensor_status_leds(&self) -> (bool, bool, bool, bool) {
        (
            self.level(BoardPin::Digital(30)) != 0,
            self.level(BoardPin::Digital(31)) != 0,
            self.level(BoardPin::Digital(32)) != 0,
            self.level(BoardPin::Digital(33)) != 0,
        )
    }
}

impl MegaBoard for TestMegaBoard {
    fn advance_time_ms(&mut self, _elapsed_ms: f64) {}

    fn read_pin(&self, pin: BoardPin) -> u8 {
        self.level(pin)
    }

    fn set_pin_mode(&mut self, pin: BoardPin, mode: PinMode) {
        self.pin_modes.insert(pin, mode);
    }

    fn write_pin(&mut self, pin: BoardPin, level: u8) {
        self.pin_levels.insert(pin, level);
    }

    fn set_pwm_duty(&mut self, pin: BoardPin, duty: u8) {
        self.pwm_duty.insert(pin, duty);
    }

    fn begin_spi_can(&mut self, _settings: SpiSettings) {}

    fn transfer_spi_can(&mut self, value: u8, _settings: SpiSettings) -> u8 {
        self.can_log.push(value);
        self.can_responses.pop_front().unwrap_or(0)
    }

    fn begin_spi_rtd(&mut self, _settings: SpiSettings) {}

    fn transfer_spi_rtd(&mut self, value: u8, _settings: SpiSettings) -> u8 {
        self.rtd_log.push(value);
        self.rtd_responses.pop_front().unwrap_or(0)
    }

    fn analog_input_counts(&self, pin: BoardPin) -> u16 {
        *self.analog_counts.get(&pin).unwrap_or(&0)
    }
}

#[test]
fn nano_port_registers_drive_gpio_levels() {
    let board = TestNanoBoard::default();
    let bus = Atmega328pBus::new(board, 16_000_000);
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), bus);

    cpu.write_data(DDRB, 1 << 5).unwrap();
    cpu.write_data(PORTB, 1 << 5).unwrap();
    assert_eq!(cpu.bus.board.level(BoardPin::Digital(13)), 1);

    cpu.write_data(PORTB, 0x00).unwrap();
    assert_eq!(cpu.bus.board.level(BoardPin::Digital(13)), 0);
}

#[test]
fn nano_null_board_tracks_output_levels_and_pullups() {
    let bus = Atmega328pBus::new(NullNanoBoard::default(), 16_000_000);
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), bus);

    cpu.write_data(DDRB, 1 << 5).unwrap();
    cpu.write_data(PORTB, 1 << 5).unwrap();
    assert_eq!(cpu.bus.board.read_pin(BoardPin::Digital(13)), 1);

    cpu.write_data(DDRD, 0x00).unwrap();
    cpu.write_data(PORTD, 1 << 2).unwrap();
    assert_eq!(cpu.bus.board.read_pin(BoardPin::Digital(2)), 1);
}

#[test]
fn nano_spi_mmio_routes_bytes() {
    let mut board = TestNanoBoard::default();
    board.spi_responses = VecDeque::from([0xAB]);
    let bus = Atmega328pBus::new(board, 16_000_000);
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), bus);

    cpu.write_data(DDRB, (1 << 2) | (1 << 3) | (1 << 5))
        .unwrap();
    cpu.write_data(PORTB, 1 << 2).unwrap();
    cpu.write_data(NANO_SPCR, SPE | MSTR).unwrap();
    cpu.write_data(NANO_SPDR, 0x55).unwrap();

    assert_eq!(cpu.read_data(NANO_SPDR).unwrap(), 0xAB);
    assert_eq!(cpu.bus.board.spi_log, vec![0x55]);
}

#[test]
fn nano_twi_mmio_status_sequence() {
    let mut board = TestNanoBoard::default();
    board.i2c_preview_ok = true;
    board
        .i2c_reads
        .insert(0x44, encode_measurement_payload(23.45, 56.78).to_vec());
    let bus = Atmega328pBus::new(board, 16_000_000);
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), bus);

    cpu.write_data(TWCR, TWEN | TWEA).unwrap();
    cpu.write_data(TWCR, TWEN | TWEA | TWINT | TWSTA).unwrap();
    assert_eq!(cpu.read_data(TWSR).unwrap() & 0xF8, TW_START);

    cpu.write_data(TWDR, 0x44 << 1).unwrap();
    cpu.write_data(TWCR, TWEN | TWEA | TWINT).unwrap();
    assert_eq!(cpu.read_data(TWSR).unwrap() & 0xF8, TW_MT_SLA_ACK);

    cpu.write_data(TWDR, 0x24).unwrap();
    cpu.write_data(TWCR, TWEN | TWEA | TWINT).unwrap();
    assert_eq!(cpu.read_data(TWSR).unwrap() & 0xF8, TW_MT_DATA_ACK);

    cpu.write_data(TWDR, 0x00).unwrap();
    cpu.write_data(TWCR, TWEN | TWEA | TWINT).unwrap();
    assert_eq!(cpu.read_data(TWSR).unwrap() & 0xF8, TW_MT_DATA_ACK);

    cpu.write_data(TWCR, TWEN | TWEA | TWINT | TWSTO).unwrap();

    cpu.write_data(TWCR, TWEN | TWEA | TWINT | TWSTA).unwrap();
    cpu.write_data(TWDR, (0x44 << 1) | 0x01).unwrap();
    cpu.write_data(TWCR, TWEN | TWEA | TWINT).unwrap();
    assert_eq!(cpu.read_data(TWSR).unwrap() & 0xF8, TW_MR_SLA_ACK);

    let mut payload = Vec::new();
    for index in 0..6 {
        let mut control = TWEN | TWINT;
        if index < 5 {
            control |= TWEA;
        }
        cpu.write_data(TWCR, control).unwrap();
        let expected = if index < 5 {
            TW_MR_DATA_ACK
        } else {
            TW_MR_DATA_NACK
        };
        assert_eq!(cpu.read_data(TWSR).unwrap() & 0xF8, expected);
        payload.push(cpu.read_data(TWDR).unwrap());
    }

    let (temp_c, rh_percent) = decode_measurement_payload(&payload);
    assert!((temp_c - 23.45).abs() <= 0.03);
    assert!((rh_percent - 56.78).abs() <= 0.03);
}

#[test]
fn nano_timer0_overflow_interrupt_vectors() {
    let board = TestNanoBoard::default();
    let bus = Atmega328pBus::new(board, 16_000_000);
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), bus);

    let mut program = vec![
        ldi(16, TOIE0),
        sts(16, NANO_TIMSK0).0,
        sts(16, NANO_TIMSK0).1,
        ldi(16, 0xFF),
        out((NANO_TCNT0 - 0x20) as u8, 16),
        ldi(16, 0x01),
        out((NANO_TCCR0B - 0x20) as u8, 16),
        bset(7),
        0x0000,
        0x0000,
        brk(),
    ];
    while program.len() < (16 * 2) {
        program.push(0x0000);
    }
    program.extend([inc(17), reti()]);
    cpu.load_program_words(&program, 0).unwrap();

    cpu.run(Some(32)).unwrap();

    assert_eq!(cpu.read_register(17).unwrap(), 1);
    assert!(cpu.break_hit);
}

#[test]
fn nano_usart_tx_and_rx() {
    let board = TestNanoBoard::default();
    let bus = Atmega328pBus::new(board, 16_000_000);
    let mut cpu = Cpu::new(CpuConfig::atmega328p(), bus);

    cpu.write_data(NANO_UBRR0L, 0).unwrap();
    cpu.write_data(NANO_UCSR0B, TXEN0).unwrap();
    assert_ne!(cpu.read_data(NANO_UCSR0A).unwrap() & UDRE0, 0);

    cpu.write_data(NANO_UDR0, b'A').unwrap();
    assert_eq!(cpu.bus.serial0.tx_log, Vec::<u8>::new());
    advance_cycles(&mut cpu, 200);
    assert_eq!(cpu.bus.serial0.tx_log, vec![b'A']);

    cpu.write_data(NANO_UCSR0B, RXEN0).unwrap();
    cpu.bus.serial0.inject_rx(b"Z");
    advance_cycles(&mut cpu, 1);
    assert_ne!(cpu.read_data(NANO_UCSR0A).unwrap() & RXC0, 0);
    assert_eq!(cpu.read_data(NANO_UDR0).unwrap(), b'Z');
}

#[test]
fn mega_port_registers_drive_leds() {
    let board = TestMegaBoard::default();
    let bus = Atmega2560Bus::new(board, 16_000_000);
    let mut cpu = Cpu::new(CpuConfig::atmega2560(), bus);

    cpu.write_data(DDRC, 0xF0).unwrap();
    cpu.write_data(PORTC, 0xA0).unwrap();

    assert_eq!(
        cpu.bus.board.sensor_status_leds(),
        (true, false, true, false)
    );
}

#[test]
fn mega_spi_chip_select_routing_matches_runtime_bus_behavior() {
    let mut board = TestMegaBoard::default();
    board.can_responses = VecDeque::from([0xA1]);
    board.rtd_responses = VecDeque::from([0xB2]);
    let bus = Atmega2560Bus::new(board, 16_000_000);
    let mut cpu = Cpu::new(CpuConfig::atmega2560(), bus);

    cpu.write_data(DDRA, (1 << 4) | (1 << 5)).unwrap();
    cpu.write_data(PORTA, (1 << 4) | (1 << 5)).unwrap();
    cpu.write_data(MEGA_SPCR, SPE | MSTR).unwrap();

    cpu.write_data(PORTA, 1 << 5).unwrap();
    cpu.write_data(MEGA_SPDR, 0x11).unwrap();
    assert_eq!(cpu.read_data(MEGA_SPDR).unwrap(), 0xA1);
    assert_eq!(cpu.bus.board.can_log, vec![0x11]);

    cpu.write_data(PORTA, 1 << 4).unwrap();
    cpu.write_data(MEGA_SPDR, 0x22).unwrap();
    assert_eq!(cpu.read_data(MEGA_SPDR).unwrap(), 0xB2);
    assert_eq!(cpu.bus.board.rtd_log, vec![0x22]);
}

#[test]
fn mega_adc_reads_feedback_counts() {
    let mut board = TestMegaBoard::default();
    board.analog_counts.insert(BoardPin::Analog(10), 767);
    let bus = Atmega2560Bus::new(board, 16_000_000);
    let mut cpu = Cpu::new(CpuConfig::atmega2560(), bus);

    cpu.write_data(ADMUX, 0x02).unwrap();
    cpu.write_data(ADCSRB, MUX5).unwrap();
    cpu.write_data(ADCSRA, ADEN | ADSC | 0x07).unwrap();
    advance_cycles(&mut cpu, 1700);

    let adc_value =
        (cpu.read_data(ADCL).unwrap() as u16) | ((cpu.read_data(ADCH).unwrap() as u16) << 8);
    assert!((766..=768).contains(&adc_value));
    assert_eq!(cpu.read_data(ADCSRA).unwrap() & ADSC, 0);
    assert_ne!(cpu.read_data(ADCSRA).unwrap() & ADIF, 0);
}

#[test]
fn mega_null_board_reports_no_lcd_key_pressed_on_a0() {
    let bus = Atmega2560Bus::new(NullMegaBoard::default(), 16_000_000);
    let mut cpu = Cpu::new(CpuConfig::atmega2560(), bus);

    cpu.write_data(ADMUX, 0x00).unwrap();
    cpu.write_data(ADCSRB, 0x00).unwrap();
    cpu.write_data(ADCSRA, ADEN | ADSC | 0x07).unwrap();
    advance_cycles(&mut cpu, 1700);

    let adc_value =
        (cpu.read_data(ADCL).unwrap() as u16) | ((cpu.read_data(ADCH).unwrap() as u16) << 8);
    assert_eq!(adc_value, 1023);
}

#[test]
fn mega_null_board_tracks_output_levels_pullups_and_idle_spi_chip_selects() {
    let bus = Atmega2560Bus::new(NullMegaBoard::default(), 16_000_000);
    let mut cpu = Cpu::new(CpuConfig::atmega2560(), bus);

    cpu.write_data(DDRA, (1 << 4) | (1 << 5)).unwrap();
    cpu.write_data(PORTA, (1 << 4) | (1 << 5)).unwrap();
    assert_eq!(cpu.bus.board.read_pin(BoardPin::Digital(26)), 1);
    assert_eq!(cpu.bus.board.read_pin(BoardPin::Digital(27)), 1);

    cpu.write_data(MEGA_SPCR, SPE | MSTR).unwrap();
    cpu.write_data(MEGA_SPDR, 0x55).unwrap();
    assert!(!cpu.bus.spi_transaction_active_can);
    assert!(!cpu.bus.spi_transaction_active_rtd);

    cpu.write_data(PORTA, 1 << 5).unwrap();
    cpu.write_data(MEGA_SPDR, 0xAA).unwrap();
    assert!(cpu.bus.spi_transaction_active_can);
    assert!(!cpu.bus.spi_transaction_active_rtd);

    cpu.write_data(DDRA, 0x00).unwrap();
    cpu.write_data(PORTA, 1 << 2).unwrap();
    assert_eq!(cpu.bus.board.read_pin(BoardPin::Digital(24)), 1);
}

#[test]
fn mega_timer5b_pwm_maps_to_d45() {
    let board = TestMegaBoard::default();
    let bus = Atmega2560Bus::new(board, 16_000_000);
    let mut cpu = Cpu::new(CpuConfig::atmega2560(), bus);

    cpu.write_data(OCR5BL, 77).unwrap();

    assert_eq!(cpu.bus.board.pwm_duty.get(&BoardPin::Digital(45)).copied(), Some(77));
}

#[test]
fn mega_eeprom_round_trip() {
    let board = TestMegaBoard::default();
    let bus = Atmega2560Bus::new(board, 16_000_000);
    let mut cpu = Cpu::new(CpuConfig::atmega2560(), bus);
    const EERE: u8 = 1 << 0;
    const EEPE: u8 = 1 << 1;
    const EEMPE: u8 = 1 << 2;

    cpu.write_data(EEARL, 0x34).unwrap();
    cpu.write_data(EEARH, 0x02).unwrap();
    cpu.write_data(EEDR, 0x5A).unwrap();
    cpu.write_data(EECR, EEMPE).unwrap();
    cpu.write_data(EECR, EEPE).unwrap();

    cpu.write_data(EEDR, 0x00).unwrap();
    cpu.write_data(EECR, EERE).unwrap();
    assert_eq!(cpu.read_data(EEDR).unwrap(), 0x5A);
}

#[test]
fn mega_timer0_overflow_interrupt_vectors() {
    let board = TestMegaBoard::default();
    let bus = Atmega2560Bus::new(board, 16_000_000);
    let mut cpu = Cpu::new(CpuConfig::atmega2560(), bus);

    let mut program = vec![
        ldi(16, TOIE0),
        sts(16, MEGA_TIMSK0).0,
        sts(16, MEGA_TIMSK0).1,
        ldi(16, 0xFF),
        out((MEGA_TCNT0 - 0x20) as u8, 16),
        ldi(16, 0x01),
        out((MEGA_TCCR0B - 0x20) as u8, 16),
        bset(7),
        0x0000,
        0x0000,
        brk(),
    ];
    while program.len() < (23 * 2) {
        program.push(0x0000);
    }
    program.extend([inc(17), reti()]);
    cpu.load_program_words(&program, 0).unwrap();

    cpu.run(Some(32)).unwrap();

    assert_eq!(cpu.read_register(17).unwrap(), 1);
    assert!(cpu.break_hit);
}

#[test]
fn mega_usart_tx_and_rx() {
    let board = TestMegaBoard::default();
    let bus = Atmega2560Bus::new(board, 16_000_000);
    let mut cpu = Cpu::new(CpuConfig::atmega2560(), bus);

    cpu.write_data(MEGA_UBRR0L, 0).unwrap();
    cpu.write_data(MEGA_UCSR0B, TXEN0).unwrap();
    assert_ne!(cpu.read_data(MEGA_UCSR0A).unwrap() & UDRE0, 0);

    cpu.write_data(MEGA_UDR0, b'M').unwrap();
    assert_eq!(cpu.bus.serial0.tx_log, Vec::<u8>::new());
    advance_cycles(&mut cpu, 200);
    assert_eq!(cpu.bus.serial0.tx_log, vec![b'M']);

    cpu.write_data(MEGA_UCSR0B, RXEN0).unwrap();
    cpu.bus.serial0.inject_rx(b"Z");
    advance_cycles(&mut cpu, 1);
    assert_ne!(cpu.read_data(MEGA_UCSR0A).unwrap() & RXC0, 0);
    assert_eq!(cpu.read_data(MEGA_UDR0).unwrap(), b'Z');
}
