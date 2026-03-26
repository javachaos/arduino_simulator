use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PinMode {
    Input,
    Output,
    InputPullup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoardPin {
    Digital(u8),
    Analog(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardPinLevel {
    pub pin: BoardPin,
    pub level: u8,
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
        for _ in 0..tick_count {
            let (next, overflowed) = tcnt0.overflowing_add(1);
            *tcnt0 = next;
            if overflowed {
                *tifr0 |= tov_mask;
                if (timsk0 & toie_mask) != 0 {
                    self.interrupt_pending = true;
                }
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
