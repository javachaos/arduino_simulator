use std::collections::HashMap;

use rust_cpu::{CpuConfig, DataBus, DecodedInstruction};

use crate::common::{BoardPin, BoardPinLevel, PinMode, SerialState, SpiSettings, Timer0State};

pub const PINB: usize = 0x23;
pub const DDRB: usize = 0x24;
pub const PORTB: usize = 0x25;
pub const PINC: usize = 0x26;
pub const DDRC: usize = 0x27;
pub const PORTC: usize = 0x28;
pub const PIND: usize = 0x29;
pub const DDRD: usize = 0x2A;
pub const PORTD: usize = 0x2B;
pub const TIFR0: usize = 0x35;
pub const TCCR0A: usize = 0x44;
pub const TCCR0B: usize = 0x45;
pub const TCNT0: usize = 0x46;
pub const SPCR: usize = 0x4C;
pub const SPSR: usize = 0x4D;
pub const SPDR: usize = 0x4E;
pub const TIMSK0: usize = 0x6E;
pub const TWBR: usize = 0xB8;
pub const TWSR: usize = 0xB9;
pub const TWAR: usize = 0xBA;
pub const TWDR: usize = 0xBB;
pub const TWCR: usize = 0xBC;
pub const TWAMR: usize = 0xBD;
pub const UCSR0A: usize = 0xC0;
pub const UCSR0B: usize = 0xC1;
pub const UCSR0C: usize = 0xC2;
pub const UBRR0L: usize = 0xC4;
pub const UBRR0H: usize = 0xC5;
pub const UDR0: usize = 0xC6;

pub const TIMER0_OVF_VECTOR: u8 = 16;
pub const USART_RX_VECTOR: u8 = 18;
pub const USART_UDRE_VECTOR: u8 = 19;
pub const USART_TX_VECTOR: u8 = 20;
pub const TWI_VECTOR: u8 = 24;

const TOIE0: u8 = 1 << 0;
const TOV0: u8 = 1 << 0;
const SPI2X: u8 = 1 << 0;
const WCOL: u8 = 1 << 6;
const SPIF: u8 = 1 << 7;
const MSTR: u8 = 1 << 4;
const SPE: u8 = 1 << 6;
const U2X0: u8 = 1 << 1;
const UDRE0: u8 = 1 << 5;
const TXC0: u8 = 1 << 6;
const RXC0: u8 = 1 << 7;
const TXEN0: u8 = 1 << 3;
const RXEN0: u8 = 1 << 4;
const UDRIE0: u8 = 1 << 5;
const TXCIE0: u8 = 1 << 6;
const RXCIE0: u8 = 1 << 7;
const TWIE: u8 = 1 << 0;
const TWEN: u8 = 1 << 2;
const TWSTO: u8 = 1 << 4;
const TWSTA: u8 = 1 << 5;
const TWEA: u8 = 1 << 6;
const TWINT: u8 = 1 << 7;
const TW_NO_INFO: u8 = 0xF8;
const TW_START: u8 = 0x08;
const TW_REP_START: u8 = 0x10;
const TW_MT_SLA_ACK: u8 = 0x18;
const TW_MT_SLA_NACK: u8 = 0x20;
const TW_MT_DATA_ACK: u8 = 0x28;
const TW_MT_DATA_NACK: u8 = 0x30;
const TW_MR_SLA_ACK: u8 = 0x40;
const TW_MR_SLA_NACK: u8 = 0x48;
const TW_MR_DATA_ACK: u8 = 0x50;
const TW_MR_DATA_NACK: u8 = 0x58;

pub trait NanoBoard {
    fn advance_time_ms(&mut self, elapsed_ms: f64);
    fn read_pin(&self, pin: BoardPin) -> u8;
    fn set_pin_mode(&mut self, pin: BoardPin, mode: PinMode);
    fn write_pin(&mut self, pin: BoardPin, level: u8);
    fn begin_spi(&mut self, _settings: SpiSettings) {}
    fn transfer_spi(&mut self, _value: u8, _settings: SpiSettings) -> u8 {
        0
    }
    fn end_spi(&mut self) {}
    fn i2c_preview_write(&self, _address: u8, _payload: &[u8]) -> bool {
        false
    }
    fn i2c_write(&mut self, _address: u8, _payload: &[u8]) -> bool {
        false
    }
    fn i2c_read(&mut self, _address: u8, _length: usize) -> Option<Vec<u8>> {
        None
    }
}

#[derive(Debug, Default, Clone)]
pub struct NullNanoBoard {
    pin_modes: HashMap<BoardPin, PinMode>,
    pin_levels: HashMap<BoardPin, u8>,
}

impl NanoBoard for NullNanoBoard {
    fn advance_time_ms(&mut self, _elapsed_ms: f64) {}

    fn read_pin(&self, pin: BoardPin) -> u8 {
        if let Some(level) = self.pin_levels.get(&pin) {
            return *level;
        }
        match self.pin_modes.get(&pin).copied() {
            Some(PinMode::InputPullup) => 1,
            _ => 0,
        }
    }

    fn set_pin_mode(&mut self, pin: BoardPin, mode: PinMode) {
        self.pin_modes.insert(pin, mode);
    }

    fn write_pin(&mut self, pin: BoardPin, level: u8) {
        self.pin_levels.insert(pin, level);
    }
}

impl NullNanoBoard {
    pub fn set_input_level(&mut self, pin: BoardPin, level: u8) {
        self.pin_levels.insert(pin, u8::from(level != 0));
    }

    pub fn clear_input_level(&mut self, pin: BoardPin) {
        self.pin_levels.remove(&pin);
    }
}

pub struct Atmega328pBus<B: NanoBoard> {
    pub board: B,
    pub clock_hz: u32,
    pub synced_cycles: u64,
    pub timer0: Timer0State,
    pub serial0: SerialState,
    pub spi_transaction_active: bool,
    pub twi_interrupt_pending: bool,
    pub twi_bus_active: bool,
    pub twi_address: Option<u8>,
    pub twi_read_mode: bool,
    pub twi_write_buffer: Vec<u8>,
    pub twi_read_buffer: Vec<u8>,
    pub twi_read_index: usize,
}

impl<B: NanoBoard> Atmega328pBus<B> {
    pub fn new(board: B, clock_hz: u32) -> Self {
        Self {
            board,
            clock_hz,
            synced_cycles: 0,
            timer0: Timer0State::default(),
            serial0: SerialState::default(),
            spi_transaction_active: false,
            twi_interrupt_pending: false,
            twi_bus_active: false,
            twi_address: None,
            twi_read_mode: false,
            twi_write_buffer: Vec::new(),
            twi_read_buffer: Vec::new(),
            twi_read_index: 0,
        }
    }

    fn sync_to_cycle(&mut self, data: &mut [u8], target_cycles: u64) {
        if target_cycles <= self.synced_cycles {
            return;
        }
        let delta = target_cycles - self.synced_cycles;
        let elapsed_ms = (delta as f64) * 1000.0 / (self.clock_hz as f64);
        self.board.advance_time_ms(elapsed_ms);
        let prescaler = timer0_prescaler(data[TCCR0B] & 0x07);
        let mut tcnt0 = data[TCNT0];
        let mut tifr0 = data[TIFR0];
        let timsk0 = data[TIMSK0];
        self.timer0.advance(
            delta as u32,
            prescaler,
            &mut tcnt0,
            &mut tifr0,
            timsk0,
            TOV0,
            TOIE0,
        );
        data[TCNT0] = tcnt0;
        data[TIFR0] = tifr0;
        let mut ucsra = data[UCSR0A];
        let ucsrb = data[UCSR0B];
        let mut udr = data[UDR0];
        self.serial0.advance(
            delta as u32,
            &mut ucsra,
            ucsrb,
            &mut udr,
            UDRE0,
            TXC0,
            RXC0,
            RXEN0,
        );
        data[UCSR0A] = ucsra;
        data[UDR0] = udr;
        self.synced_cycles = target_cycles;
    }

    fn sync_port(
        &mut self,
        data: &mut [u8],
        pin_address: usize,
        ddr_address: usize,
        port_address: usize,
    ) {
        let ddr_value = data[ddr_address];
        let port_value = data[port_address];
        for bit_index in 0u8..8 {
            let Some(pin) = port_pin(pin_address, bit_index) else {
                continue;
            };
            if (ddr_value & (1u8 << bit_index)) != 0 {
                self.board.set_pin_mode(pin, PinMode::Output);
                self.board.write_pin(
                    pin,
                    if (port_value & (1u8 << bit_index)) != 0 {
                        1
                    } else {
                        0
                    },
                );
            } else if (port_value & (1u8 << bit_index)) != 0 {
                self.board.set_pin_mode(pin, PinMode::InputPullup);
            } else {
                self.board.set_pin_mode(pin, PinMode::Input);
            }
        }
    }

    fn read_pin_register(&self, pin_address: usize) -> u8 {
        let mut value = 0u8;
        for bit_index in 0u8..8 {
            let Some(pin) = port_pin(pin_address, bit_index) else {
                continue;
            };
            if self.board.read_pin(pin) != 0 {
                value |= 1u8 << bit_index;
            }
        }
        value
    }

    pub fn host_pin_levels(&self) -> Vec<BoardPinLevel> {
        let mut levels = Vec::with_capacity(22);
        for pin in 0u8..=13 {
            levels.push(BoardPinLevel {
                pin: BoardPin::Digital(pin),
                level: self.board.read_pin(BoardPin::Digital(pin)),
            });
        }
        for pin in 0u8..=7 {
            levels.push(BoardPinLevel {
                pin: BoardPin::Analog(pin),
                level: self.board.read_pin(BoardPin::Analog(pin)),
            });
        }
        levels
    }

    fn write_udr0(&mut self, data: &mut [u8], value: u8) {
        data[UDR0] = value;
        let ubrr = data[UBRR0L] as u16 | ((data[UBRR0H] as u16 & 0x0F) << 8);
        self.serial0.write_udr(
            value,
            self.clock_hz,
            ubrr,
            (data[UCSR0A] & U2X0) != 0,
            (data[UCSR0B] & TXEN0) != 0,
            &mut data[UCSR0A],
            UDRE0,
            TXC0,
        );
    }

    fn write_twcr(&mut self, data: &mut [u8], value: u8) {
        data[TWCR] = value;
        self.twi_interrupt_pending = false;
        if (value & TWINT) != 0 {
            data[TWCR] &= !TWINT;
        }
        if (value & TWEN) == 0 {
            self.twi_bus_active = false;
            self.twi_address = None;
            self.twi_read_mode = false;
            self.twi_write_buffer.clear();
            self.twi_read_buffer.clear();
            self.twi_read_index = 0;
            data[TWSR] = (data[TWSR] & 0x03) | TW_NO_INFO;
            return;
        }
        if (value & TWSTO) != 0 {
            if let Some(address) = self.twi_address {
                let _ = self.board.i2c_write(address, &self.twi_write_buffer);
            }
            data[TWCR] &= !TWSTO;
            self.twi_bus_active = false;
            self.twi_address = None;
            self.twi_read_mode = false;
            self.twi_write_buffer.clear();
            self.twi_read_buffer.clear();
            self.twi_read_index = 0;
            return;
        }
        if (value & TWSTA) != 0 {
            let status = if self.twi_bus_active {
                TW_REP_START
            } else {
                TW_START
            };
            self.twi_bus_active = true;
            data[TWSR] = (data[TWSR] & 0x03) | status;
            data[TWCR] |= TWINT;
            self.twi_interrupt_pending = (data[TWCR] & TWIE) != 0;
            return;
        }

        match data[TWSR] & 0xF8 {
            TW_START | TW_REP_START => {
                let slarw = data[TWDR];
                let address = (slarw >> 1) & 0x7F;
                self.twi_address = Some(address);
                self.twi_read_mode = (slarw & 0x01) != 0;
                self.twi_write_buffer.clear();
                self.twi_read_buffer.clear();
                self.twi_read_index = 0;
                if self.twi_read_mode {
                    if let Some(payload) = self.board.i2c_read(address, 6) {
                        self.twi_read_buffer = payload;
                        data[TWSR] = (data[TWSR] & 0x03) | TW_MR_SLA_ACK;
                    } else {
                        data[TWSR] = (data[TWSR] & 0x03) | TW_MR_SLA_NACK;
                    }
                } else if self.board.i2c_preview_write(address, &[]) {
                    data[TWSR] = (data[TWSR] & 0x03) | TW_MT_SLA_ACK;
                } else {
                    data[TWSR] = (data[TWSR] & 0x03) | TW_MT_SLA_NACK;
                }
                data[TWCR] |= TWINT;
                self.twi_interrupt_pending = (data[TWCR] & TWIE) != 0;
            }
            TW_MT_SLA_ACK | TW_MT_DATA_ACK => {
                let Some(address) = self.twi_address else {
                    return;
                };
                let mut preview = self.twi_write_buffer.clone();
                preview.push(data[TWDR]);
                if self.board.i2c_preview_write(address, &preview) {
                    self.twi_write_buffer = preview;
                    data[TWSR] = (data[TWSR] & 0x03) | TW_MT_DATA_ACK;
                } else {
                    data[TWSR] = (data[TWSR] & 0x03) | TW_MT_DATA_NACK;
                }
                data[TWCR] |= TWINT;
                self.twi_interrupt_pending = (data[TWCR] & TWIE) != 0;
            }
            TW_MR_SLA_ACK | TW_MR_DATA_ACK => {
                if self.twi_read_index < self.twi_read_buffer.len() {
                    data[TWDR] = self.twi_read_buffer[self.twi_read_index];
                    self.twi_read_index += 1;
                    data[TWSR] = (data[TWSR] & 0x03)
                        | if (value & TWEA) != 0 {
                            TW_MR_DATA_ACK
                        } else {
                            TW_MR_DATA_NACK
                        };
                    data[TWCR] |= TWINT;
                    self.twi_interrupt_pending = (data[TWCR] & TWIE) != 0;
                }
            }
            _ => {}
        }
    }
}

impl<B: NanoBoard> DataBus for Atmega328pBus<B> {
    fn reset(&mut self, _config: &CpuConfig, data: &mut [u8]) {
        self.synced_cycles = 0;
        self.timer0.reset();
        self.serial0.reset();
        self.spi_transaction_active = false;
        self.twi_interrupt_pending = false;
        self.twi_bus_active = false;
        self.twi_address = None;
        self.twi_read_mode = false;
        self.twi_write_buffer.clear();
        self.twi_read_buffer.clear();
        self.twi_read_index = 0;
        data[TIFR0] = 0;
        data[TWSR] = TW_NO_INFO;
        data[UCSR0A] = UDRE0 | TXC0;
        data[UCSR0B] = 0;
        data[UCSR0C] = 0;
        data[UBRR0L] = 0;
        data[UBRR0H] = 0;
        data[UDR0] = 0;
        self.sync_port(data, PINB, DDRB, PORTB);
        self.sync_port(data, PINC, DDRC, PORTC);
        self.sync_port(data, PIND, DDRD, PORTD);
    }

    fn read_data(
        &mut self,
        _config: &CpuConfig,
        data: &mut [u8],
        cycles: u64,
        address: usize,
    ) -> Option<u8> {
        self.sync_to_cycle(data, cycles);
        match address {
            PINB | PINC | PIND => Some(self.read_pin_register(address)),
            UDR0 => {
                let value = data[UDR0];
                let mut ucsra = data[UCSR0A] & !RXC0;
                let ucsrb = data[UCSR0B];
                let mut udr = data[UDR0];
                self.serial0
                    .service_rx_latch(&mut ucsra, ucsrb, &mut udr, RXC0, RXEN0);
                data[UCSR0A] = ucsra;
                data[UDR0] = udr;
                Some(value)
            }
            SPDR => {
                let value = data[SPDR];
                data[SPSR] &= !(SPIF | WCOL);
                Some(value)
            }
            DDRB | PORTB | DDRC | PORTC | DDRD | PORTD | TIFR0 | TCCR0A | TCCR0B | TCNT0 | SPCR
            | SPSR | TIMSK0 | UCSR0A | UCSR0B | UCSR0C | UBRR0L | UBRR0H | TWBR | TWSR | TWAR
            | TWDR | TWCR | TWAMR => Some(data[address]),
            _ => None,
        }
    }

    fn write_data(
        &mut self,
        _config: &CpuConfig,
        data: &mut [u8],
        cycles: u64,
        address: usize,
        value: u8,
    ) -> bool {
        self.sync_to_cycle(data, cycles);
        match address {
            DDRB | PORTB => {
                data[address] = value;
                self.sync_port(data, PINB, DDRB, PORTB);
                true
            }
            DDRC | PORTC => {
                data[address] = value;
                self.sync_port(data, PINC, DDRC, PORTC);
                true
            }
            DDRD | PORTD => {
                data[address] = value;
                self.sync_port(data, PIND, DDRD, PORTD);
                true
            }
            PINB => {
                data[PORTB] ^= value;
                self.sync_port(data, PINB, DDRB, PORTB);
                true
            }
            PINC => {
                data[PORTC] ^= value;
                self.sync_port(data, PINC, DDRC, PORTC);
                true
            }
            PIND => {
                data[PORTD] ^= value;
                self.sync_port(data, PIND, DDRD, PORTD);
                true
            }
            TIFR0 => {
                data[TIFR0] &= !value;
                if (data[TIFR0] & TOV0) == 0 {
                    self.timer0.interrupt_pending = false;
                }
                true
            }
            TCCR0A | TCCR0B | TCNT0 | TIMSK0 | SPCR | TWBR | TWAR | TWDR | TWAMR | UBRR0L
            | UBRR0H | UCSR0C => {
                data[address] = value;
                true
            }
            UCSR0A => {
                let mut current = data[UCSR0A];
                current = (current & !(U2X0 | TXC0)) | (value & U2X0);
                if (value & TXC0) == 0 {
                    current |= data[UCSR0A] & TXC0;
                }
                data[UCSR0A] = current;
                true
            }
            UCSR0B => {
                data[UCSR0B] = value;
                if (value & RXEN0) == 0 {
                    data[UCSR0A] &= !RXC0;
                } else {
                    let mut ucsra = data[UCSR0A];
                    let ucsrb = data[UCSR0B];
                    let mut udr = data[UDR0];
                    self.serial0
                        .service_rx_latch(&mut ucsra, ucsrb, &mut udr, RXC0, RXEN0);
                    data[UCSR0A] = ucsra;
                    data[UDR0] = udr;
                }
                true
            }
            SPSR => {
                data[SPSR] = (data[SPSR] & (SPIF | WCOL)) | (value & SPI2X);
                true
            }
            UDR0 => {
                self.write_udr0(data, value);
                true
            }
            SPDR => {
                data[SPDR] = value;
                if (data[SPCR] & (SPE | MSTR)) == (SPE | MSTR) {
                    let settings = SpiSettings {
                        spcr: data[SPCR],
                        spsr: data[SPSR],
                    };
                    if !self.spi_transaction_active {
                        self.board.begin_spi(settings);
                        self.spi_transaction_active = true;
                    }
                    let response = self.board.transfer_spi(value, settings);
                    data[SPDR] = response;
                    data[SPSR] = (data[SPSR] & SPI2X) | SPIF;
                }
                true
            }
            TWSR => {
                data[TWSR] = (data[TWSR] & 0xF8) | (value & 0x03);
                true
            }
            TWCR => {
                self.write_twcr(data, value);
                true
            }
            _ => false,
        }
    }

    fn after_step(
        &mut self,
        _config: &CpuConfig,
        data: &mut [u8],
        pc: u32,
        cycles: u64,
        _instruction: &DecodedInstruction,
        _step_cycles: u8,
    ) {
        self.sync_to_cycle(data, cycles);
        let _ = pc;
    }

    fn pending_interrupt(
        &mut self,
        _config: &CpuConfig,
        data: &mut [u8],
        _pc: u32,
        cycles: u64,
    ) -> Option<u8> {
        self.sync_to_cycle(data, cycles);
        if self.timer0.interrupt_pending && (data[TIMSK0] & TOIE0) != 0 {
            self.timer0.interrupt_pending = false;
            return Some(TIMER0_OVF_VECTOR);
        }
        if (data[UCSR0A] & RXC0) != 0 && (data[UCSR0B] & RXCIE0) != 0 && (data[UCSR0B] & RXEN0) != 0
        {
            return Some(USART_RX_VECTOR);
        }
        if (data[UCSR0A] & UDRE0) != 0
            && (data[UCSR0B] & UDRIE0) != 0
            && (data[UCSR0B] & TXEN0) != 0
        {
            return Some(USART_UDRE_VECTOR);
        }
        if (data[UCSR0A] & TXC0) != 0 && (data[UCSR0B] & TXCIE0) != 0 && (data[UCSR0B] & TXEN0) != 0
        {
            return Some(USART_TX_VECTOR);
        }
        if self.twi_interrupt_pending && (data[TWCR] & TWIE) != 0 && (data[TWCR] & TWINT) != 0 {
            self.twi_interrupt_pending = false;
            return Some(TWI_VECTOR);
        }
        None
    }

    fn on_interrupt(
        &mut self,
        _config: &CpuConfig,
        data: &mut [u8],
        _pc: u32,
        cycles: u64,
        _vector_number: u8,
        latency_cycles: u8,
    ) {
        self.sync_to_cycle(data, cycles + latency_cycles as u64);
    }
}

fn timer0_prescaler(value: u8) -> Option<u32> {
    match value & 0x07 {
        1 => Some(1),
        2 => Some(8),
        3 => Some(64),
        4 => Some(256),
        5 => Some(1024),
        _ => None,
    }
}

fn port_pin(pin_address: usize, bit_index: u8) -> Option<BoardPin> {
    match (pin_address, bit_index) {
        (PINB, 0) => Some(BoardPin::Digital(8)),
        (PINB, 1) => Some(BoardPin::Digital(9)),
        (PINB, 2) => Some(BoardPin::Digital(10)),
        (PINB, 3) => Some(BoardPin::Digital(11)),
        (PINB, 4) => Some(BoardPin::Digital(12)),
        (PINB, 5) => Some(BoardPin::Digital(13)),
        (PINC, 0) => Some(BoardPin::Analog(0)),
        (PINC, 1) => Some(BoardPin::Analog(1)),
        (PINC, 2) => Some(BoardPin::Analog(2)),
        (PINC, 3) => Some(BoardPin::Analog(3)),
        (PINC, 4) => Some(BoardPin::Analog(4)),
        (PINC, 5) => Some(BoardPin::Analog(5)),
        (PIND, 0) => Some(BoardPin::Digital(0)),
        (PIND, 1) => Some(BoardPin::Digital(1)),
        (PIND, 2) => Some(BoardPin::Digital(2)),
        (PIND, 3) => Some(BoardPin::Digital(3)),
        (PIND, 4) => Some(BoardPin::Digital(4)),
        (PIND, 5) => Some(BoardPin::Digital(5)),
        (PIND, 6) => Some(BoardPin::Digital(6)),
        (PIND, 7) => Some(BoardPin::Digital(7)),
        _ => None,
    }
}
