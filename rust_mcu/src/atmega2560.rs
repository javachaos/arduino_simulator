use std::collections::HashMap;

use rust_cpu::{CpuConfig, DataBus, DecodedInstruction};

use crate::common::{
    AdcState, BoardPin, BoardPinLevel, PinMode, SerialState, SpiSettings, Timer0State,
};

pub const PINA: usize = 0x20;
pub const DDRA: usize = 0x21;
pub const PORTA: usize = 0x22;
pub const PINB: usize = 0x23;
pub const DDRB: usize = 0x24;
pub const PORTB: usize = 0x25;
pub const PINC: usize = 0x26;
pub const DDRC: usize = 0x27;
pub const PORTC: usize = 0x28;
pub const PIND: usize = 0x29;
pub const DDRD: usize = 0x2A;
pub const PORTD: usize = 0x2B;
pub const PINE: usize = 0x2C;
pub const DDRE: usize = 0x2D;
pub const PORTE: usize = 0x2E;
pub const PINF: usize = 0x2F;
pub const DDRF: usize = 0x30;
pub const PORTF: usize = 0x31;
pub const PING: usize = 0x32;
pub const DDRG: usize = 0x33;
pub const PORTG: usize = 0x34;
pub const TIFR0: usize = 0x35;
pub const EECR: usize = 0x3F;
pub const EEDR: usize = 0x40;
pub const EEARL: usize = 0x41;
pub const EEARH: usize = 0x42;
pub const TCCR0A: usize = 0x44;
pub const TCCR0B: usize = 0x45;
pub const TCNT0: usize = 0x46;
pub const OCR0A: usize = 0x47;
pub const OCR0B: usize = 0x48;
pub const SPCR: usize = 0x4C;
pub const SPSR: usize = 0x4D;
pub const SPDR: usize = 0x4E;
pub const TIMSK0: usize = 0x6E;
pub const ADCL: usize = 0x78;
pub const ADCH: usize = 0x79;
pub const ADCSRA: usize = 0x7A;
pub const ADCSRB: usize = 0x7B;
pub const ADMUX: usize = 0x7C;
pub const DIDR2: usize = 0x7D;
pub const DIDR0: usize = 0x7E;
pub const UCSR0A: usize = 0xC0;
pub const UCSR0B: usize = 0xC1;
pub const UCSR0C: usize = 0xC2;
pub const UBRR0L: usize = 0xC4;
pub const UBRR0H: usize = 0xC5;
pub const UDR0: usize = 0xC6;
pub const PINH: usize = 0x100;
pub const DDRH: usize = 0x101;
pub const PORTH: usize = 0x102;
pub const PINJ: usize = 0x103;
pub const DDRJ: usize = 0x104;
pub const PORTJ: usize = 0x105;
pub const PINK: usize = 0x106;
pub const DDRK: usize = 0x107;
pub const PORTK: usize = 0x108;
pub const PINL: usize = 0x109;
pub const DDRL: usize = 0x10A;
pub const PORTL: usize = 0x10B;
pub const OCR5CL: usize = 0x12C;
pub const OCR5CH: usize = 0x12D;
pub const OCR5AL: usize = 0x128;
pub const OCR5AH: usize = 0x129;
pub const OCR5BL: usize = 0x12A;
pub const OCR5BH: usize = 0x12B;

pub const TIMER0_OVF_VECTOR: u8 = 23;
pub const USART_RX_VECTOR: u8 = 25;
pub const USART_UDRE_VECTOR: u8 = 26;
pub const USART_TX_VECTOR: u8 = 27;
pub const ADC_VECTOR: u8 = 29;

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
const EERE: u8 = 1 << 0;
const EEPE: u8 = 1 << 1;
const EEMPE: u8 = 1 << 2;
const ADPS0: u8 = 1 << 0;
const ADPS1: u8 = 1 << 1;
const ADPS2: u8 = 1 << 2;
const ADIE: u8 = 1 << 3;
const ADIF: u8 = 1 << 4;
const ADSC: u8 = 1 << 6;
const ADEN: u8 = 1 << 7;
const MUX5: u8 = 1 << 3;

pub trait MegaBoard {
    fn advance_time_ms(&mut self, elapsed_ms: f64);
    fn read_pin(&self, pin: BoardPin) -> u8;
    fn set_pin_mode(&mut self, pin: BoardPin, mode: PinMode);
    fn write_pin(&mut self, pin: BoardPin, level: u8);
    fn set_pwm_duty(&mut self, _pin: BoardPin, _duty: u8) {}
    fn begin_spi_can(&mut self, _settings: SpiSettings) {}
    fn transfer_spi_can(&mut self, _value: u8, _settings: SpiSettings) -> u8 {
        0
    }
    fn end_spi_can(&mut self) {}
    fn begin_spi_rtd(&mut self, _settings: SpiSettings) {}
    fn transfer_spi_rtd(&mut self, _value: u8, _settings: SpiSettings) -> u8 {
        0
    }
    fn end_spi_rtd(&mut self) {}
    fn analog_input_counts(&self, _pin: BoardPin) -> u16 {
        0
    }
    fn eeprom_size(&self) -> usize {
        0x1000
    }
}

#[derive(Debug, Clone)]
pub struct NullMegaBoard {
    pin_modes: HashMap<BoardPin, PinMode>,
    pin_levels: HashMap<BoardPin, u8>,
    analog_input_levels: HashMap<BoardPin, u16>,
}

impl Default for NullMegaBoard {
    fn default() -> Self {
        Self {
            pin_modes: HashMap::new(),
            pin_levels: HashMap::new(),
            analog_input_levels: HashMap::new(),
        }
    }
}

impl NullMegaBoard {
    pub fn set_input_level(&mut self, pin: BoardPin, level: u8) {
        self.pin_levels.insert(pin, u8::from(level != 0));
    }

    pub fn clear_input_level(&mut self, pin: BoardPin) {
        self.pin_levels.remove(&pin);
    }

    pub fn set_analog_input_level(&mut self, pin: BoardPin, counts: u16) {
        self.analog_input_levels.insert(pin, counts.min(1023));
    }

    pub fn clear_analog_input_level(&mut self, pin: BoardPin) {
        self.analog_input_levels.remove(&pin);
    }
}

impl MegaBoard for NullMegaBoard {
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

    fn analog_input_counts(&self, pin: BoardPin) -> u16 {
        if let Some(level) = self.analog_input_levels.get(&pin) {
            return (*level).min(1023);
        }
        match pin {
            // The stock 1602 LCD keypad shield idles near full-scale when no key is pressed.
            BoardPin::Analog(0) => 1023,
            _ => 0,
        }
    }
}

pub struct Atmega2560Bus<B: MegaBoard> {
    pub board: B,
    pub clock_hz: u32,
    pub synced_cycles: u64,
    pub timer0: Timer0State,
    pub serial0: SerialState,
    pub adc: AdcState,
    pub spi_transaction_active_can: bool,
    pub spi_transaction_active_rtd: bool,
    pub eeprom: Vec<u8>,
    pub eeprom_master_write_enabled: bool,
}

impl<B: MegaBoard> Atmega2560Bus<B> {
    pub fn new(board: B, clock_hz: u32) -> Self {
        let eeprom_size = board.eeprom_size();
        Self {
            board,
            clock_hz,
            synced_cycles: 0,
            timer0: Timer0State::default(),
            serial0: SerialState::default(),
            adc: AdcState::default(),
            spi_transaction_active_can: false,
            spi_transaction_active_rtd: false,
            eeprom: vec![0xFF; eeprom_size],
            eeprom_master_write_enabled: false,
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
        if self.adc.advance(delta as u32) {
            let channel =
                (((data[ADCSRB] & MUX5) >> 3) as usize * 8) + (data[ADMUX] as usize & 0x07);
            let value = self
                .board
                .analog_input_counts(BoardPin::Analog(channel as u8));
            data[ADCL] = (value & 0x00FF) as u8;
            data[ADCH] = ((value >> 8) & 0x03) as u8;
            data[ADCSRA] &= !ADSC;
            data[ADCSRA] |= ADIF;
            if (data[ADCSRA] & ADIE) != 0 {
                self.adc.interrupt_pending = true;
            }
        }
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
        let mut levels = Vec::with_capacity(70);
        for pin in 0u8..=53 {
            levels.push(BoardPinLevel {
                pin: BoardPin::Digital(pin),
                level: self.board.read_pin(BoardPin::Digital(pin)),
            });
        }
        for pin in 0u8..=15 {
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

    fn write_adcsra(&mut self, data: &mut [u8], value: u8) {
        let preserved = data[ADCSRA] & ADIF;
        let mut updated = value | preserved;
        if (value & ADIF) != 0 {
            updated &= !ADIF;
            self.adc.interrupt_pending = false;
        }
        data[ADCSRA] = updated;
        if (data[ADCSRA] & ADEN) == 0 {
            data[ADCSRA] &= !(ADSC | ADIF);
            self.adc.clear();
            return;
        }
        if (value & ADSC) != 0 && self.adc.cycles_remaining <= 0 {
            let prescaler = adc_prescaler(data[ADCSRA] & (ADPS0 | ADPS1 | ADPS2));
            self.adc.start((13 * prescaler) as u32);
            data[ADCSRA] |= ADSC;
        }
    }

    fn write_eecr(&mut self, data: &mut [u8], value: u8) {
        data[EECR] = value & (EERE | EEPE | EEMPE);
        if (value & EERE) != 0 {
            let address = (data[EEARL] as usize) | (((data[EEARH] & 0x0F) as usize) << 8);
            data[EEDR] = *self.eeprom.get(address).unwrap_or(&0xFF);
            data[EECR] &= !EERE;
        }
        if (value & EEMPE) != 0 {
            self.eeprom_master_write_enabled = true;
        }
        if (value & EEPE) != 0 && self.eeprom_master_write_enabled {
            let address = (data[EEARL] as usize) | (((data[EEARH] & 0x0F) as usize) << 8);
            if let Some(slot) = self.eeprom.get_mut(address) {
                *slot = data[EEDR];
            }
            self.eeprom_master_write_enabled = false;
            data[EECR] &= !(EEPE | EEMPE);
        }
    }

    fn sync_pwm_output(&mut self, data: &[u8]) {
        self.board.set_pwm_duty(BoardPin::Digital(46), data[OCR5AL]);
        self.board.set_pwm_duty(BoardPin::Digital(45), data[OCR5BL]);
        self.board.set_pwm_duty(BoardPin::Digital(44), data[OCR5CL]);
    }
}

impl<B: MegaBoard> DataBus for Atmega2560Bus<B> {
    fn reset(&mut self, _config: &CpuConfig, data: &mut [u8]) {
        self.synced_cycles = 0;
        self.timer0.reset();
        self.serial0.reset();
        self.adc.reset();
        self.spi_transaction_active_can = false;
        self.spi_transaction_active_rtd = false;
        self.eeprom_master_write_enabled = false;
        data[TIFR0] = 0;
        data[UCSR0A] = UDRE0 | TXC0;
        data[UCSR0B] = 0;
        data[UCSR0C] = 0;
        data[UBRR0L] = 0;
        data[UBRR0H] = 0;
        data[UDR0] = 0;
        data[ADCSRA] = 0;
        data[ADCSRB] = 0;
        data[ADMUX] = 0;
        data[ADCL] = 0;
        data[ADCH] = 0;
        data[EECR] = 0;
        data[EEDR] = 0xFF;
        data[EEARL] = 0;
        data[EEARH] = 0;
        for (pin, ddr, port) in [
            (PINA, DDRA, PORTA),
            (PINB, DDRB, PORTB),
            (PINC, DDRC, PORTC),
            (PIND, DDRD, PORTD),
            (PINE, DDRE, PORTE),
            (PINF, DDRF, PORTF),
            (PING, DDRG, PORTG),
            (PINH, DDRH, PORTH),
            (PINJ, DDRJ, PORTJ),
            (PINK, DDRK, PORTK),
            (PINL, DDRL, PORTL),
        ] {
            self.sync_port(data, pin, ddr, port);
        }
        self.sync_pwm_output(data);
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
            PINA | PINB | PINC | PIND | PINE | PINF | PING | PINH | PINJ | PINK | PINL => {
                Some(self.read_pin_register(address))
            }
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
            DDRA | PORTA | DDRB | PORTB | DDRC | PORTC | DDRD | PORTD | DDRE | PORTE | DDRF
            | PORTF | DDRG | PORTG | DDRH | PORTH | DDRJ | PORTJ | DDRK | PORTK | DDRL | PORTL
            | TIFR0 | TCCR0A | TCCR0B | TCNT0 | OCR0A | OCR0B | SPCR | SPSR | TIMSK0 | UCSR0A
            | UCSR0B | UCSR0C | UBRR0L | UBRR0H | EECR | EEDR | EEARL | EEARH | ADCL | ADCH
            | ADCSRA | ADCSRB | ADMUX | DIDR0 | DIDR2 | OCR5AL | OCR5AH | OCR5BL | OCR5BH
            | OCR5CL | OCR5CH => Some(data[address]),
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
            DDRA | PORTA => {
                data[address] = value;
                self.sync_port(data, PINA, DDRA, PORTA);
                true
            }
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
            DDRE | PORTE => {
                data[address] = value;
                self.sync_port(data, PINE, DDRE, PORTE);
                true
            }
            DDRF | PORTF => {
                data[address] = value;
                self.sync_port(data, PINF, DDRF, PORTF);
                true
            }
            DDRG | PORTG => {
                data[address] = value;
                self.sync_port(data, PING, DDRG, PORTG);
                true
            }
            DDRH | PORTH => {
                data[address] = value;
                self.sync_port(data, PINH, DDRH, PORTH);
                true
            }
            DDRJ | PORTJ => {
                data[address] = value;
                self.sync_port(data, PINJ, DDRJ, PORTJ);
                true
            }
            DDRK | PORTK => {
                data[address] = value;
                self.sync_port(data, PINK, DDRK, PORTK);
                true
            }
            DDRL | PORTL => {
                data[address] = value;
                self.sync_port(data, PINL, DDRL, PORTL);
                true
            }
            TIFR0 => {
                data[TIFR0] &= !value;
                if (data[TIFR0] & TOV0) == 0 {
                    self.timer0.interrupt_pending = false;
                }
                true
            }
            UCSR0A => {
                let writable = U2X0;
                data[UCSR0A] = (data[UCSR0A] & !(writable | TXC0)) | (value & writable);
                if (value & TXC0) == 0 {
                    data[UCSR0A] |= TXC0 & data[UCSR0A];
                }
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
            UCSR0C | UBRR0L | UBRR0H | SPCR | OCR0A | OCR0B | TCCR0A | TCCR0B | TCNT0 | TIMSK0
            | ADCL | ADCH | ADMUX | ADCSRB | DIDR0 | DIDR2 | EEDR | EEARL | EEARH => {
                data[address] = value;
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
                let settings = SpiSettings {
                    spcr: data[SPCR],
                    spsr: data[SPSR],
                };
                if (data[SPCR] & (SPE | MSTR)) == (SPE | MSTR) {
                    let cs_can = self.board.read_pin(BoardPin::Digital(26)) == 0;
                    let cs_rtd = self.board.read_pin(BoardPin::Digital(27)) == 0;
                    if cs_can {
                        if !self.spi_transaction_active_can {
                            self.board.begin_spi_can(settings);
                            self.spi_transaction_active_can = true;
                        }
                        data[SPDR] = self.board.transfer_spi_can(value, settings);
                    } else if cs_rtd {
                        if !self.spi_transaction_active_rtd {
                            self.board.begin_spi_rtd(settings);
                            self.spi_transaction_active_rtd = true;
                        }
                        data[SPDR] = self.board.transfer_spi_rtd(value, settings);
                    } else {
                        data[SPDR] = 0;
                    }
                    data[SPSR] |= SPIF;
                }
                true
            }
            ADCSRA => {
                self.write_adcsra(data, value);
                true
            }
            EECR => {
                self.write_eecr(data, value);
                true
            }
            OCR5AL | OCR5AH | OCR5BL | OCR5BH | OCR5CL | OCR5CH => {
                data[address] = value;
                self.sync_pwm_output(data);
                true
            }
            _ => false,
        }
    }

    fn after_step(
        &mut self,
        _config: &CpuConfig,
        data: &mut [u8],
        _pc: u32,
        cycles: u64,
        _instruction: &DecodedInstruction,
        _step_cycles: u8,
    ) {
        self.sync_to_cycle(data, cycles);
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
        if self.adc.interrupt_pending && (data[ADCSRA] & ADIE) != 0 {
            self.adc.interrupt_pending = false;
            return Some(ADC_VECTOR);
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

fn adc_prescaler(value: u8) -> u32 {
    match value & (ADPS0 | ADPS1 | ADPS2) {
        0 | 1 => 2,
        2 => 4,
        3 => 8,
        4 => 16,
        5 => 32,
        6 => 64,
        _ => 128,
    }
}

fn port_pin(pin_address: usize, bit_index: u8) -> Option<BoardPin> {
    match (pin_address, bit_index) {
        (PINA, 0) => Some(BoardPin::Digital(22)),
        (PINA, 1) => Some(BoardPin::Digital(23)),
        (PINA, 2) => Some(BoardPin::Digital(24)),
        (PINA, 3) => Some(BoardPin::Digital(25)),
        (PINA, 4) => Some(BoardPin::Digital(26)),
        (PINA, 5) => Some(BoardPin::Digital(27)),
        (PINA, 6) => Some(BoardPin::Digital(28)),
        (PINA, 7) => Some(BoardPin::Digital(29)),
        (PINB, 0) => Some(BoardPin::Digital(53)),
        (PINB, 1) => Some(BoardPin::Digital(52)),
        (PINB, 2) => Some(BoardPin::Digital(51)),
        (PINB, 3) => Some(BoardPin::Digital(50)),
        (PINB, 4) => Some(BoardPin::Digital(10)),
        (PINB, 5) => Some(BoardPin::Digital(11)),
        (PINB, 6) => Some(BoardPin::Digital(12)),
        (PINB, 7) => Some(BoardPin::Digital(13)),
        (PINC, 0) => Some(BoardPin::Digital(37)),
        (PINC, 1) => Some(BoardPin::Digital(36)),
        (PINC, 2) => Some(BoardPin::Digital(35)),
        (PINC, 3) => Some(BoardPin::Digital(34)),
        (PINC, 4) => Some(BoardPin::Digital(33)),
        (PINC, 5) => Some(BoardPin::Digital(32)),
        (PINC, 6) => Some(BoardPin::Digital(31)),
        (PINC, 7) => Some(BoardPin::Digital(30)),
        (PIND, 0) => Some(BoardPin::Digital(21)),
        (PIND, 1) => Some(BoardPin::Digital(20)),
        (PIND, 2) => Some(BoardPin::Digital(19)),
        (PIND, 3) => Some(BoardPin::Digital(18)),
        (PIND, 7) => Some(BoardPin::Digital(38)),
        (PINE, 0) => Some(BoardPin::Digital(0)),
        (PINE, 1) => Some(BoardPin::Digital(1)),
        (PINE, 3) => Some(BoardPin::Digital(5)),
        (PINE, 4) => Some(BoardPin::Digital(2)),
        (PINE, 5) => Some(BoardPin::Digital(3)),
        (PINF, 0) => Some(BoardPin::Analog(0)),
        (PINF, 1) => Some(BoardPin::Analog(1)),
        (PINF, 2) => Some(BoardPin::Analog(2)),
        (PINF, 3) => Some(BoardPin::Analog(3)),
        (PINF, 4) => Some(BoardPin::Analog(4)),
        (PINF, 5) => Some(BoardPin::Analog(5)),
        (PINF, 6) => Some(BoardPin::Analog(6)),
        (PINF, 7) => Some(BoardPin::Analog(7)),
        (PING, 0) => Some(BoardPin::Digital(41)),
        (PING, 1) => Some(BoardPin::Digital(40)),
        (PING, 2) => Some(BoardPin::Digital(39)),
        (PING, 5) => Some(BoardPin::Digital(4)),
        (PINH, 0) => Some(BoardPin::Digital(17)),
        (PINH, 1) => Some(BoardPin::Digital(16)),
        (PINH, 3) => Some(BoardPin::Digital(6)),
        (PINH, 4) => Some(BoardPin::Digital(7)),
        (PINH, 5) => Some(BoardPin::Digital(8)),
        (PINH, 6) => Some(BoardPin::Digital(9)),
        (PINJ, 0) => Some(BoardPin::Digital(15)),
        (PINJ, 1) => Some(BoardPin::Digital(14)),
        (PINK, 0) => Some(BoardPin::Analog(8)),
        (PINK, 1) => Some(BoardPin::Analog(9)),
        (PINK, 2) => Some(BoardPin::Analog(10)),
        (PINK, 3) => Some(BoardPin::Analog(11)),
        (PINK, 4) => Some(BoardPin::Analog(12)),
        (PINK, 5) => Some(BoardPin::Analog(13)),
        (PINK, 6) => Some(BoardPin::Analog(14)),
        (PINK, 7) => Some(BoardPin::Analog(15)),
        (PINL, 0) => Some(BoardPin::Digital(49)),
        (PINL, 1) => Some(BoardPin::Digital(48)),
        (PINL, 2) => Some(BoardPin::Digital(47)),
        (PINL, 3) => Some(BoardPin::Digital(46)),
        (PINL, 4) => Some(BoardPin::Digital(45)),
        (PINL, 5) => Some(BoardPin::Digital(44)),
        (PINL, 6) => Some(BoardPin::Digital(43)),
        (PINL, 7) => Some(BoardPin::Digital(42)),
        _ => None,
    }
}
