use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PinMode {
    Input,
    Output,
    InputPullup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BoardPin {
    Digital(u8),
    Analog(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardPinLevel {
    pub pin: BoardPin,
    pub level: u8,
}

const DIGITAL_PIN_SLOTS: usize = 54;
const ANALOG_PIN_SLOTS: usize = 16;
const PIN_SLOTS: usize = DIGITAL_PIN_SLOTS + ANALOG_PIN_SLOTS;

#[derive(Debug, Clone)]
pub(crate) struct PinMap<T: Copy> {
    values: Box<[Option<T>; PIN_SLOTS]>,
}

impl<T: Copy> Default for PinMap<T> {
    fn default() -> Self {
        Self {
            values: Box::new([None; PIN_SLOTS]),
        }
    }
}

impl<T: Copy> PinMap<T> {
    pub(crate) fn get(&self, pin: BoardPin) -> Option<T> {
        pin_slot(pin).and_then(|index| self.values[index])
    }

    pub(crate) fn insert(&mut self, pin: BoardPin, value: T) {
        if let Some(index) = pin_slot(pin) {
            self.values[index] = Some(value);
        }
    }

    pub(crate) fn remove(&mut self, pin: BoardPin) {
        if let Some(index) = pin_slot(pin) {
            self.values[index] = None;
        }
    }
}

fn pin_slot(pin: BoardPin) -> Option<usize> {
    match pin {
        BoardPin::Digital(index) if usize::from(index) < DIGITAL_PIN_SLOTS => {
            Some(usize::from(index))
        }
        BoardPin::Analog(index) if usize::from(index) < ANALOG_PIN_SLOTS => {
            Some(DIGITAL_PIN_SLOTS + usize::from(index))
        }
        _ => None,
    }
}

#[cfg(test)]
mod pin_map_tests {
    use super::{BoardPin, PinMap};

    #[test]
    fn fixed_pin_map_keeps_digital_and_analog_slots_distinct() {
        let mut pins = PinMap::default();
        pins.insert(BoardPin::Digital(0), 1u8);
        pins.insert(BoardPin::Analog(0), 2u8);

        assert_eq!(pins.get(BoardPin::Digital(0)), Some(1));
        assert_eq!(pins.get(BoardPin::Analog(0)), Some(2));

        pins.remove(BoardPin::Digital(0));
        assert_eq!(pins.get(BoardPin::Digital(0)), None);
        assert_eq!(pins.get(BoardPin::Analog(0)), Some(2));
    }

    #[test]
    fn fixed_pin_map_ignores_out_of_range_board_pins() {
        let mut pins = PinMap::default();
        pins.insert(BoardPin::Digital(54), 1u8);
        pins.insert(BoardPin::Analog(16), 1u8);

        assert_eq!(pins.get(BoardPin::Digital(54)), None);
        assert_eq!(pins.get(BoardPin::Analog(16)), None);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpiSettings {
    pub spcr: u8,
    pub spsr: u8,
}

#[derive(Debug, Clone, Default)]
pub struct Timer0State {
    pub cycle_remainder: u32,
    pub interrupt_pending: bool,
}

impl Timer0State {
    pub fn reset(&mut self) {
        self.cycle_remainder = 0;
        self.interrupt_pending = false;
    }

    pub fn advance(
        &mut self,
        cycles: u32,
        prescaler: Option<u32>,
        tcnt0: &mut u8,
        tifr0: &mut u8,
        timsk0: u8,
        tov_mask: u8,
        toie_mask: u8,
    ) {
        let Some(prescaler) = prescaler else {
            self.cycle_remainder = 0;
            return;
        };
        let total_cycles = self.cycle_remainder + cycles;
        let tick_count = total_cycles / prescaler;
        self.cycle_remainder = total_cycles % prescaler;
        if tick_count == 0 {
            return;
        }

        let counter_total = u64::from(*tcnt0) + u64::from(tick_count);
        *tcnt0 = counter_total as u8;
        if counter_total >= 256 {
            *tifr0 |= tov_mask;
            if (timsk0 & toie_mask) != 0 {
                self.interrupt_pending = true;
            }
        }
    }

    pub fn overflow_deadline_cycles(&self, prescaler: Option<u32>, tcnt0: u8) -> Option<u64> {
        let Some(prescaler) = prescaler else {
            return None;
        };
        let ticks_until_overflow = 256u32 - (tcnt0 as u32);
        let cycles_until = ticks_until_overflow
            .saturating_mul(prescaler)
            .saturating_sub(self.cycle_remainder);
        Some(cycles_until.max(1) as u64)
    }
}

#[cfg(test)]
mod timer_tests {
    use super::Timer0State;

    #[test]
    fn timer_advance_handles_many_ticks_without_iterating_each_tick() {
        let mut timer = Timer0State::default();
        let mut counter = 250u8;
        let mut flags = 0u8;

        timer.advance(600, Some(1), &mut counter, &mut flags, 0x04, 0x02, 0x04);

        assert_eq!(counter, 82);
        assert_eq!(flags & 0x02, 0x02);
        assert!(timer.interrupt_pending);
    }
}

#[derive(Debug, Clone, Default)]
pub struct SerialState {
    pub tx_log: Vec<u8>,
    pub rx_queue: VecDeque<u8>,
    pub tx_busy_byte: Option<u8>,
    pub tx_cycles_remaining: i64,
}

impl SerialState {
    pub fn reset(&mut self) {
        self.tx_log.clear();
        self.rx_queue.clear();
        self.tx_busy_byte = None;
        self.tx_cycles_remaining = 0;
    }

    pub fn clear_output(&mut self) {
        self.tx_log.clear();
    }

    pub fn inject_rx(&mut self, payload: &[u8]) {
        self.rx_queue.extend(payload.iter().copied());
    }

    pub fn frame_cycles(clock_hz: u32, ubrr: u16, double_speed: bool) -> u32 {
        let divisor = if double_speed { 8u32 } else { 16u32 };
        let baud = (clock_hz as f64) / ((divisor as f64) * ((ubrr as f64) + 1.0));
        if baud <= 0.0 {
            return 1;
        }
        ((clock_hz as f64 * 10.0) / baud).round().max(1.0) as u32
    }

    pub fn write_udr(
        &mut self,
        value: u8,
        clock_hz: u32,
        ubrr: u16,
        double_speed: bool,
        tx_enabled: bool,
        ucsra: &mut u8,
        udre_mask: u8,
        txc_mask: u8,
    ) {
        if !tx_enabled {
            return;
        }
        self.tx_busy_byte = Some(value);
        self.tx_cycles_remaining = Self::frame_cycles(clock_hz, ubrr, double_speed) as i64;
        *ucsra &= !(udre_mask | txc_mask);
    }

    pub fn advance(
        &mut self,
        cycles: u32,
        ucsra: &mut u8,
        ucsrb: u8,
        udr: &mut u8,
        udre_mask: u8,
        txc_mask: u8,
        rxc_mask: u8,
        rxen_mask: u8,
    ) {
        if let Some(byte) = self.tx_busy_byte {
            self.tx_cycles_remaining -= cycles as i64;
            if self.tx_cycles_remaining <= 0 {
                self.tx_log.push(byte);
                self.tx_busy_byte = None;
                self.tx_cycles_remaining = 0;
                *ucsra |= udre_mask | txc_mask;
            }
        }
        self.service_rx_latch(ucsra, ucsrb, udr, rxc_mask, rxen_mask);
    }

    pub fn service_rx_latch(
        &mut self,
        ucsra: &mut u8,
        ucsrb: u8,
        udr: &mut u8,
        rxc_mask: u8,
        rxen_mask: u8,
    ) {
        if (*ucsra & rxc_mask) != 0 || (ucsrb & rxen_mask) == 0 {
            return;
        }
        if let Some(byte) = self.rx_queue.pop_front() {
            *udr = byte;
            *ucsra |= rxc_mask;
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AdcState {
    pub cycles_remaining: i64,
    pub interrupt_pending: bool,
}

impl AdcState {
    pub fn reset(&mut self) {
        self.cycles_remaining = 0;
        self.interrupt_pending = false;
    }

    pub fn start(&mut self, cycles: u32) {
        self.cycles_remaining = cycles as i64;
        self.interrupt_pending = false;
    }

    pub fn clear(&mut self) {
        self.cycles_remaining = 0;
        self.interrupt_pending = false;
    }

    pub fn advance(&mut self, cycles: u32) -> bool {
        if self.cycles_remaining <= 0 {
            return false;
        }
        self.cycles_remaining -= cycles as i64;
        self.cycles_remaining <= 0
    }
}
