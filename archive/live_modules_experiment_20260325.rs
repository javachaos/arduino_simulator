use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use rust_behavior::{
    instantiate_behavior, load_built_in_behavior_definition, suggested_builtin_behavior_for_board_model,
    BehaviorInstance, Max31865Behavior, Mcp2515Behavior, PwmToVoltageBehavior, Sht31Behavior,
};
use rust_cpu::{Cpu, CpuConfig, CpuError, DataBus, StepOutcome};
use rust_mcu::atmega2560::{
    Atmega2560Bus, MegaBoard, NullMegaBoard, UBRR0H as MEGA_UBRR0H, UBRR0L as MEGA_UBRR0L,
    UCSR0A as MEGA_UCSR0A,
};
use rust_mcu::atmega328p::{
    Atmega328pBus, NanoBoard, NullNanoBoard, UBRR0H as NANO_UBRR0H, UBRR0L as NANO_UBRR0L,
    UCSR0A as NANO_UCSR0A,
};
use rust_mcu::{BoardPin, PinMode, SpiSettings};
use rust_project::{ModuleOverlay, SignalBinding};

const U2X0: u8 = 1 << 1;

const MCP_CMD_RESET: u8 = 0xC0;
const MCP_CMD_READ: u8 = 0x03;
const MCP_CMD_WRITE: u8 = 0x02;
const MCP_CMD_BIT_MODIFY: u8 = 0x05;
const MCP_CMD_READ_STATUS: u8 = 0xA0;
const MCP_CMD_RTS_TX0: u8 = 0x81;

const MCP_REG_CANSTAT: u8 = 0x0E;
const MCP_REG_CANCTRL: u8 = 0x0F;
const MCP_REG_CNF3: u8 = 0x28;
const MCP_REG_CNF2: u8 = 0x29;
const MCP_REG_CNF1: u8 = 0x2A;
const MCP_REG_CANINTF: u8 = 0x2C;
const MCP_REG_TXB0CTRL: u8 = 0x30;
const MCP_REG_TXB0SIDH: u8 = 0x31;
const MCP_REG_RXB0CTRL: u8 = 0x60;
const MCP_REG_RXB0SIDH: u8 = 0x61;
const MCP_MODE_MASK: u8 = 0xE0;
const MCP_MODE_NORMAL: u8 = 0x00;
const MCP_MODE_CONFIGURATION: u8 = 0x80;
const MCP_RX0IF_MASK: u8 = 0x01;
const MCP_TXREQ_MASK: u8 = 0x08;

const MAX_REG_CONFIG: u8 = 0x00;
const MAX_REG_RTD_MSB: u8 = 0x01;
const MAX_REG_RTD_LSB: u8 = 0x02;
const MAX_REG_FAULT_STATUS: u8 = 0x07;
const MAX_CFG_BIAS: u8 = 0x80;
const MAX_CFG_ONE_SHOT: u8 = 0x20;
const MAX_CFG_CLEAR_FAULT: u8 = 0x02;

#[derive(Debug, Clone, Default)]
pub struct SimulationEnvironment {
    pub controller_bindings: Vec<SignalBinding>,
    pub module_overlays: Vec<ModuleOverlay>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRuntimeSnapshot {
    pub summary_lines: Vec<String>,
    pub active_pcb_nets: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GuiNanoRuntime {
    pub cpu: Cpu<Atmega328pBus<LiveNanoBoard>>,
    serial_cursor: usize,
}

#[derive(Debug, Clone)]
pub struct GuiMegaRuntime {
    pub cpu: Cpu<Atmega2560Bus<LiveMegaBoard>>,
    serial_cursor: usize,
}

impl GuiNanoRuntime {
    pub fn new(environment: SimulationEnvironment) -> Self {
        Self {
            cpu: Cpu::new(
                CpuConfig::atmega328p(),
                Atmega328pBus::new(LiveNanoBoard::new(environment), 16_000_000),
            ),
            serial_cursor: 0,
        }
    }

    pub fn clear_serial_output(&mut self) {
        self.cpu.bus.serial0.clear_output();
        self.serial_cursor = 0;
    }

    pub fn serial_output_bytes(&self) -> &[u8] {
        &self.cpu.bus.serial0.tx_log
    }

    pub fn configured_serial_baud(&self) -> u32 {
        configured_baud(
            self.cpu.bus.clock_hz,
            self.cpu.data[NANO_UCSR0A],
            self.cpu.data[NANO_UBRR0L],
            self.cpu.data[NANO_UBRR0H],
        )
    }

    pub fn inject_serial_rx(&mut self, payload: &[u8]) {
        self.cpu.bus.serial0.inject_rx(payload);
    }

    pub fn run_chunk(
        &mut self,
        instruction_budget: usize,
        until_serial: bool,
    ) -> Result<(usize, Option<rust_runtime::RuntimeExit>), CpuError> {
        run_chunk(
            &mut self.cpu,
            &mut self.serial_cursor,
            instruction_budget,
            until_serial,
        )
    }

    pub fn apply_environment(&mut self, environment: SimulationEnvironment) {
        self.cpu.bus.board.reconfigure(environment);
    }

    pub fn module_snapshot(&self) -> ModuleRuntimeSnapshot {
        self.cpu.bus.board.module_snapshot()
    }
}

impl GuiMegaRuntime {
    pub fn new(environment: SimulationEnvironment) -> Self {
        Self {
            cpu: Cpu::new(
                CpuConfig::atmega2560(),
                Atmega2560Bus::new(LiveMegaBoard::new(environment), 16_000_000),
            ),
            serial_cursor: 0,
        }
    }

    pub fn clear_serial_output(&mut self) {
        self.cpu.bus.serial0.clear_output();
        self.serial_cursor = 0;
    }

    pub fn serial_output_bytes(&self) -> &[u8] {
        &self.cpu.bus.serial0.tx_log
    }

    pub fn configured_serial_baud(&self) -> u32 {
        configured_baud(
            self.cpu.bus.clock_hz,
            self.cpu.data[MEGA_UCSR0A],
            self.cpu.data[MEGA_UBRR0L],
            self.cpu.data[MEGA_UBRR0H],
        )
    }

    pub fn inject_serial_rx(&mut self, payload: &[u8]) {
        self.cpu.bus.serial0.inject_rx(payload);
    }

    pub fn run_chunk(
        &mut self,
        instruction_budget: usize,
        until_serial: bool,
    ) -> Result<(usize, Option<rust_runtime::RuntimeExit>), CpuError> {
        run_chunk(
            &mut self.cpu,
            &mut self.serial_cursor,
            instruction_budget,
            until_serial,
        )
    }

    pub fn apply_environment(&mut self, environment: SimulationEnvironment) {
        self.cpu.bus.board.reconfigure(environment);
    }

    pub fn module_snapshot(&self) -> ModuleRuntimeSnapshot {
        self.cpu.bus.board.module_snapshot()
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
) -> Result<(usize, Option<rust_runtime::RuntimeExit>), CpuError>
where
    B: DataBus + SerialTap,
{
    let mut executed = 0usize;
    while executed < instruction_budget {
        match cpu.step()? {
            StepOutcome::Executed => {
                executed += 1;
                if until_serial && cpu.bus.serial_bytes_len() > *serial_cursor {
                    return Ok((executed, Some(rust_runtime::RuntimeExit::UntilSerialSatisfied)));
                }
            }
            StepOutcome::BreakHit => {
                executed += 1;
                return Ok((executed, Some(rust_runtime::RuntimeExit::BreakHit)));
            }
            StepOutcome::Sleeping => {
                executed += 1;
                return Ok((executed, Some(rust_runtime::RuntimeExit::Sleeping)));
            }
        }
    }
    Ok((executed, Some(rust_runtime::RuntimeExit::MaxInstructionsReached)))
}

trait SerialTap {
    fn serial_bytes_len(&self) -> usize;
}

impl SerialTap for Atmega328pBus<LiveNanoBoard> {
    fn serial_bytes_len(&self) -> usize {
        self.serial0.tx_log.len()
    }
}

impl SerialTap for Atmega2560Bus<LiveMegaBoard> {
    fn serial_bytes_len(&self) -> usize {
        self.serial0.tx_log.len()
    }
}

#[derive(Debug, Clone)]
pub struct LiveNanoBoard {
    pin_modes: HashMap<BoardPin, PinMode>,
    pin_levels: HashMap<BoardPin, u8>,
    now_ms: f64,
    environment: SimulationEnvironment,
    sht31: Option<Sht31Device>,
    mcp2515: Option<Mcp2515Device>,
}

impl LiveNanoBoard {
    pub fn new(environment: SimulationEnvironment) -> Self {
        let mut board = Self {
            pin_modes: HashMap::new(),
            pin_levels: HashMap::new(),
            now_ms: 0.0,
            environment: SimulationEnvironment::default(),
            sht31: None,
            mcp2515: None,
        };
        board.reconfigure(environment);
        board
    }

    pub fn reconfigure(&mut self, environment: SimulationEnvironment) {
        self.environment = environment.clone();
        self.sht31 = environment
            .module_overlays
            .iter()
            .find(|module| module.model == "gy_sht31_d")
            .and_then(Sht31Device::from_overlay);
        self.mcp2515 = environment
            .module_overlays
            .iter()
            .find(|module| module.model == "mcp2515_tja1050_can_module")
            .and_then(Mcp2515Device::from_overlay);
    }

    pub fn module_snapshot(&self) -> ModuleRuntimeSnapshot {
        let mut summary_lines = Vec::new();
        let mut active = BTreeSet::new();
        if let Some(sensor) = &self.sht31 {
            summary_lines.extend(sensor.summary_lines());
            active.extend(sensor.active_nets(self.now_ms));
        }
        if let Some(can) = &self.mcp2515 {
            summary_lines.extend(can.summary_lines());
            active.extend(can.active_nets(self.now_ms));
        }
        ModuleRuntimeSnapshot {
            summary_lines,
            active_pcb_nets: active.into_iter().collect(),
        }
    }
}

impl NanoBoard for LiveNanoBoard {
    fn advance_time_ms(&mut self, elapsed_ms: f64) {
        self.now_ms += elapsed_ms.max(0.0);
        if let Some(sensor) = &mut self.sht31 {
            sensor.advance_time_ms(self.now_ms);
        }
        if let Some(can) = &mut self.mcp2515 {
            can.advance_time_ms(self.now_ms);
        }
    }

    fn read_pin(&self, pin: BoardPin) -> u8 {
        if pin == BoardPin::Digital(2) {
            if let Some(can) = &self.mcp2515 {
                if can.interrupt_level(self.now_ms) == 0 {
                    return 0;
                }
            }
        }
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

    fn begin_spi(&mut self, settings: SpiSettings) {
        if self.read_pin(BoardPin::Digital(10)) == 0 {
            if let Some(can) = &mut self.mcp2515 {
                can.begin_spi(settings, self.now_ms);
            }
        }
    }

    fn transfer_spi(&mut self, value: u8, settings: SpiSettings) -> u8 {
        if self.read_pin(BoardPin::Digital(10)) == 0 {
            if let Some(can) = &mut self.mcp2515 {
                return can.transfer_spi(value, settings, self.now_ms);
            }
        }
        0
    }

    fn end_spi(&mut self) {
        if let Some(can) = &mut self.mcp2515 {
            can.end_spi(self.now_ms);
        }
    }

    fn i2c_preview_write(&self, address: u8, payload: &[u8]) -> bool {
        self.sht31
            .as_ref()
            .map(|sensor| sensor.preview_write(address, payload))
            .unwrap_or(false)
    }

    fn i2c_write(&mut self, address: u8, payload: &[u8]) -> bool {
        self.sht31
            .as_mut()
            .map(|sensor| sensor.write(address, payload, self.now_ms))
            .unwrap_or(false)
    }

    fn i2c_read(&mut self, address: u8, length: usize) -> Option<Vec<u8>> {
        self.sht31
            .as_mut()
            .and_then(|sensor| sensor.read(address, length, self.now_ms))
    }
}

#[derive(Debug, Clone)]
pub struct LiveMegaBoard {
    inner: NullMegaBoard,
    now_ms: f64,
    environment: SimulationEnvironment,
    mcp2515: Option<Mcp2515Device>,
    max31865: Option<Max31865Device>,
    pwm_to_voltage: Option<PwmToVoltageDevice>,
}

impl LiveMegaBoard {
    pub fn new(environment: SimulationEnvironment) -> Self {
        let mut board = Self {
            inner: NullMegaBoard::default(),
            now_ms: 0.0,
            environment: SimulationEnvironment::default(),
            mcp2515: None,
            max31865: None,
            pwm_to_voltage: None,
        };
        board.reconfigure(environment);
        board
    }

    pub fn reconfigure(&mut self, environment: SimulationEnvironment) {
        self.environment = environment.clone();
        self.mcp2515 = environment
            .module_overlays
            .iter()
            .find(|module| module.model == "mcp2515_tja1050_can_module")
            .and_then(Mcp2515Device::from_overlay);
        self.max31865 = environment
            .module_overlays
            .iter()
            .find(|module| module.model == "max31865_breakout")
            .and_then(Max31865Device::from_overlay);
        self.pwm_to_voltage = environment
            .module_overlays
            .iter()
            .find(|module| module.model == "lc_lm358_pwm_to_0_10v")
            .and_then(PwmToVoltageDevice::from_overlay);
    }

    pub fn module_snapshot(&self) -> ModuleRuntimeSnapshot {
        let mut summary_lines = Vec::new();
        let mut active = BTreeSet::new();
        if let Some(can) = &self.mcp2515 {
            summary_lines.extend(can.summary_lines());
            active.extend(can.active_nets(self.now_ms));
        }
        if let Some(rtd) = &self.max31865 {
            summary_lines.extend(rtd.summary_lines());
            active.extend(rtd.active_nets(self.now_ms));
        }
        if let Some(pwm) = &self.pwm_to_voltage {
            summary_lines.extend(pwm.summary_lines());
            active.extend(pwm.active_nets(self.now_ms));
        }
        ModuleRuntimeSnapshot {
            summary_lines,
            active_pcb_nets: active.into_iter().collect(),
        }
    }
}

impl MegaBoard for LiveMegaBoard {
    fn advance_time_ms(&mut self, elapsed_ms: f64) {
        self.now_ms += elapsed_ms.max(0.0);
        self.inner.advance_time_ms(elapsed_ms);
        if let Some(can) = &mut self.mcp2515 {
            can.advance_time_ms(self.now_ms);
        }
        if let Some(rtd) = &mut self.max31865 {
            rtd.advance_time_ms(self.now_ms);
        }
        if let Some(pwm) = &mut self.pwm_to_voltage {
            pwm.advance_time_ms(self.now_ms);
        }
    }

    fn read_pin(&self, pin: BoardPin) -> u8 {
        if pin == BoardPin::Digital(28) {
            if let Some(can) = &self.mcp2515 {
                if can.interrupt_level(self.now_ms) == 0 {
                    return 0;
                }
            }
        }
        self.inner.read_pin(pin)
    }

    fn set_pin_mode(&mut self, pin: BoardPin, mode: PinMode) {
        self.inner.set_pin_mode(pin, mode);
    }

    fn write_pin(&mut self, pin: BoardPin, level: u8) {
        self.inner.write_pin(pin, level);
    }

    fn set_pwm_duty(&mut self, pin: BoardPin, duty: u8) {
        if pin == BoardPin::Digital(44) {
            if let Some(pwm) = &mut self.pwm_to_voltage {
                pwm.set_pwm_duty(duty, self.now_ms);
            }
        }
    }

    fn begin_spi_can(&mut self, settings: SpiSettings) {
        if let Some(can) = &mut self.mcp2515 {
            can.begin_spi(settings, self.now_ms);
        }
    }

    fn transfer_spi_can(&mut self, value: u8, settings: SpiSettings) -> u8 {
        self.mcp2515
            .as_mut()
            .map(|device| device.transfer_spi(value, settings, self.now_ms))
            .unwrap_or(0)
    }

    fn end_spi_can(&mut self) {
        if let Some(can) = &mut self.mcp2515 {
            can.end_spi(self.now_ms);
        }
    }

    fn begin_spi_rtd(&mut self, settings: SpiSettings) {
        if let Some(rtd) = &mut self.max31865 {
            rtd.begin_spi(settings, self.now_ms);
        }
    }

    fn transfer_spi_rtd(&mut self, value: u8, settings: SpiSettings) -> u8 {
        self.max31865
            .as_mut()
            .map(|device| device.transfer_spi(value, settings, self.now_ms))
            .unwrap_or(0)
    }

    fn end_spi_rtd(&mut self) {
        if let Some(rtd) = &mut self.max31865 {
            rtd.end_spi(self.now_ms);
        }
    }

    fn analog_input_counts(&self, pin: BoardPin) -> u16 {
        if let BoardPin::Analog(0) = pin {
            return 1023;
        }
        if let Some(pwm) = &self.pwm_to_voltage {
            if let Some(feedback_counts) =
                pwm.analog_feedback_counts(pin, &self.environment.controller_bindings)
            {
                return feedback_counts;
            }
        }
        0
    }
}

#[derive(Debug, Clone)]
struct Sht31Device {
    name: String,
    bindings: BTreeMap<String, String>,
    behavior: Sht31Behavior,
    serial_number: u32,
    last_command: Option<u16>,
    ready_at_ms: f64,
    active_until_ms: f64,
}

impl Sht31Device {
    fn from_overlay(overlay: &ModuleOverlay) -> Option<Self> {
        let behavior_name = suggested_builtin_behavior_for_board_model(&overlay.model)?;
        let definition = load_built_in_behavior_definition(behavior_name).ok()?;
        let BehaviorInstance::Sht31(behavior) = instantiate_behavior(&definition).ok()? else {
            return None;
        };
        Some(Self {
            name: overlay.name.clone(),
            bindings: overlay
                .bindings
                .iter()
                .map(|binding| (binding.module_signal.clone(), binding.pcb_net.clone()))
                .collect(),
            behavior,
            serial_number: stable_serial_number(&overlay.name),
            last_command: None,
            ready_at_ms: 0.0,
            active_until_ms: 0.0,
        })
    }

    fn advance_time_ms(&mut self, _now_ms: f64) {}

    fn preview_write(&self, address: u8, payload: &[u8]) -> bool {
        if address != self.behavior.address {
            return false;
        }
        payload.is_empty()
            || payload == [0x24].as_slice()
            || payload == [0x36].as_slice()
            || payload == [0x24, 0x00].as_slice()
            || payload == [0x36, 0x82].as_slice()
    }

    fn write(&mut self, address: u8, payload: &[u8], now_ms: f64) -> bool {
        if address != self.behavior.address || payload.len() != 2 {
            return false;
        }
        let command = u16::from(payload[0]) << 8 | u16::from(payload[1]);
        match command {
            0x2400 | 0x3682 => {
                self.last_command = Some(command);
                self.ready_at_ms = now_ms
                    + if command == 0x2400 {
                        self.behavior.measurement_delay_ms as f64
                    } else {
                        1.0
                    };
                self.active_until_ms = now_ms + 45.0;
                true
            }
            _ => false,
        }
    }

    fn read(&mut self, address: u8, length: usize, now_ms: f64) -> Option<Vec<u8>> {
        if address != self.behavior.address || length != 6 || now_ms < self.ready_at_ms {
            return None;
        }
        self.active_until_ms = now_ms + 45.0;
        match self.last_command {
            Some(0x3682) => Some(encode_sht31_serial(self.serial_number)),
            Some(0x2400) => Some(encode_sht31_measurement(
                self.behavior.ambient_temp_c,
                self.behavior.relative_humidity_percent,
            )),
            _ => None,
        }
    }

    fn summary_lines(&self) -> Vec<String> {
        vec![
            format!("[{}] SHT31 0x{:02X}", self.name, self.behavior.address),
            format!(
                "[{}] {:.2}C {:.2}%RH",
                self.name, self.behavior.ambient_temp_c, self.behavior.relative_humidity_percent
            ),
        ]
    }

    fn active_nets(&self, now_ms: f64) -> BTreeSet<String> {
        if now_ms > self.active_until_ms {
            return BTreeSet::new();
        }
        [self.bindings.get("SDA"), self.bindings.get("SCL")]
            .into_iter()
            .flatten()
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, Copy)]
struct CanFrame {
    id: u16,
    length: u8,
    data: [u8; 8],
}

#[derive(Debug, Clone)]
struct Mcp2515Device {
    name: String,
    bindings: BTreeMap<String, String>,
    behavior: Mcp2515Behavior,
    registers: [u8; 128],
    rx_queue: VecDeque<CanFrame>,
    tx_log: Vec<CanFrame>,
    spi_state: Mcp2515SpiState,
    spi_active_until_ms: f64,
    can_active_until_ms: f64,
}

#[derive(Debug, Clone)]
enum Mcp2515SpiState {
    Idle,
    GotCommand(u8),
    Read { address: u8 },
    WriteAddress,
    WriteData { address: u8 },
    BitModifyAddress,
    BitModifyMask { address: u8 },
    BitModifyData { address: u8, mask: u8 },
    ReadStatus,
}

impl Mcp2515Device {
    fn from_overlay(overlay: &ModuleOverlay) -> Option<Self> {
        let behavior_name = suggested_builtin_behavior_for_board_model(&overlay.model)?;
        let definition = load_built_in_behavior_definition(behavior_name).ok()?;
        let BehaviorInstance::Mcp2515(behavior) = instantiate_behavior(&definition).ok()? else {
            return None;
        };
        let mut device = Self {
            name: overlay.name.clone(),
            bindings: overlay
                .bindings
                .iter()
                .map(|binding| (binding.module_signal.clone(), binding.pcb_net.clone()))
                .collect(),
            behavior,
            registers: [0u8; 128],
            rx_queue: VecDeque::new(),
            tx_log: Vec::new(),
            spi_state: Mcp2515SpiState::Idle,
            spi_active_until_ms: 0.0,
            can_active_until_ms: 0.0,
        };
        device.reset();
        Some(device)
    }

    fn reset(&mut self) {
        self.registers = [0u8; 128];
        self.registers[MCP_REG_CANSTAT as usize] = MCP_MODE_CONFIGURATION;
        self.registers[MCP_REG_CANCTRL as usize] = MCP_MODE_CONFIGURATION;
        self.registers[MCP_REG_RXB0CTRL as usize] = 0x64;
        self.update_receive_buffer();
        self.behavior.interrupt_asserted = !self.rx_queue.is_empty();
        self.behavior.tx_pending_frames = 0;
        self.behavior.can_bus_active = false;
    }

    fn advance_time_ms(&mut self, now_ms: f64) {
        if now_ms > self.can_active_until_ms {
            self.behavior.can_bus_active = false;
            self.behavior.tx_pending_frames = 0;
        }
    }

    fn begin_spi(&mut self, _settings: SpiSettings, now_ms: f64) {
        self.spi_state = Mcp2515SpiState::Idle;
        self.spi_active_until_ms = now_ms + 35.0;
    }

    fn transfer_spi(&mut self, value: u8, _settings: SpiSettings, now_ms: f64) -> u8 {
        self.spi_active_until_ms = now_ms + 35.0;
        let state = self.spi_state.clone();
        match state {
            Mcp2515SpiState::Idle => match value {
                MCP_CMD_RESET => {
                    self.reset();
                    0
                }
                MCP_CMD_READ => {
                    self.spi_state = Mcp2515SpiState::Read { address: 0 };
                    0
                }
                MCP_CMD_WRITE => {
                    self.spi_state = Mcp2515SpiState::WriteAddress;
                    0
                }
                MCP_CMD_BIT_MODIFY => {
                    self.spi_state = Mcp2515SpiState::BitModifyAddress;
                    0
                }
                MCP_CMD_READ_STATUS => {
                    self.spi_state = Mcp2515SpiState::ReadStatus;
                    0
                }
                MCP_CMD_RTS_TX0 => {
                    self.process_rts_tx0(now_ms);
                    0
                }
                other => {
                    self.spi_state = Mcp2515SpiState::GotCommand(other);
                    0
                }
            },
            Mcp2515SpiState::Read { address } if address == 0 => {
                self.spi_state = Mcp2515SpiState::Read { address: value };
                0
            }
            Mcp2515SpiState::Read { address } => {
                let result = self.read_register(address);
                self.spi_state = Mcp2515SpiState::Read {
                    address: address.wrapping_add(1),
                };
                result
            }
            Mcp2515SpiState::WriteAddress => {
                self.spi_state = Mcp2515SpiState::WriteData { address: value };
                0
            }
            Mcp2515SpiState::WriteData { address } => {
                self.write_register(address, value);
                self.spi_state = Mcp2515SpiState::WriteData {
                    address: address.wrapping_add(1),
                };
                0
            }
            Mcp2515SpiState::BitModifyAddress => {
                self.spi_state = Mcp2515SpiState::BitModifyMask { address: value };
                0
            }
            Mcp2515SpiState::BitModifyMask { address } => {
                self.spi_state = Mcp2515SpiState::BitModifyData { address, mask: value };
                0
            }
            Mcp2515SpiState::BitModifyData { address, mask } => {
                let current = self.read_register(address);
                self.write_register(address, (current & !mask) | (value & mask));
                self.spi_state = Mcp2515SpiState::Idle;
                0
            }
            Mcp2515SpiState::ReadStatus => self.read_status(),
            Mcp2515SpiState::GotCommand(_) => 0,
        }
    }

    fn end_spi(&mut self, _now_ms: f64) {
        self.spi_state = Mcp2515SpiState::Idle;
    }

    fn read_register(&self, address: u8) -> u8 {
        self.registers[address as usize]
    }

    fn write_register(&mut self, address: u8, value: u8) {
        self.registers[address as usize] = value;
        match address {
            MCP_REG_CANCTRL => {
                let mode = value & MCP_MODE_MASK;
                self.registers[MCP_REG_CANSTAT as usize] =
                    (self.registers[MCP_REG_CANSTAT as usize] & !MCP_MODE_MASK) | mode;
            }
            MCP_REG_CANINTF => {
                self.behavior.interrupt_asserted =
                    (self.registers[MCP_REG_CANINTF as usize] & MCP_RX0IF_MASK) != 0;
            }
            _ => {}
        }
    }

    fn process_rts_tx0(&mut self, now_ms: f64) {
        let sid_h = self.registers[MCP_REG_TXB0SIDH as usize];
        let sid_l = self.registers[(MCP_REG_TXB0SIDH + 1) as usize];
        let length = self.registers[(MCP_REG_TXB0SIDH + 4) as usize] & 0x0F;
        let mut frame = CanFrame {
            id: (u16::from(sid_h) << 3) | u16::from(sid_l >> 5),
            length,
            data: [0u8; 8],
        };
        for index in 0..usize::from(length.min(8)) {
            frame.data[index] = self.registers[(MCP_REG_TXB0SIDH + 5 + index as u8) as usize];
        }
        self.tx_log.push(frame);
        self.behavior.tx_pending_frames = 1;
        self.behavior.can_bus_active = true;
        self.can_active_until_ms = now_ms + 55.0;
        self.registers[MCP_REG_TXB0CTRL as usize] &= !MCP_TXREQ_MASK;
    }

    fn read_status(&self) -> u8 {
        let mut status = 0u8;
        if (self.registers[MCP_REG_CANINTF as usize] & MCP_RX0IF_MASK) != 0 {
            status |= 0x01;
        }
        if (self.registers[MCP_REG_TXB0CTRL as usize] & MCP_TXREQ_MASK) != 0 {
            status |= 0x04;
        }
        status
    }

    fn update_receive_buffer(&mut self) {
        if let Some(frame) = self.rx_queue.front().copied() {
            self.registers[MCP_REG_CANINTF as usize] |= MCP_RX0IF_MASK;
            self.registers[MCP_REG_RXB0SIDH as usize] = ((frame.id >> 3) & 0xFF) as u8;
            self.registers[(MCP_REG_RXB0SIDH + 1) as usize] = ((frame.id & 0x07) << 5) as u8;
            self.registers[(MCP_REG_RXB0SIDH + 4) as usize] = frame.length & 0x0F;
            for index in 0..8 {
                self.registers[(MCP_REG_RXB0SIDH + 5 + index as u8) as usize] = frame.data[index];
            }
        } else {
            self.registers[MCP_REG_CANINTF as usize] &= !MCP_RX0IF_MASK;
        }
        self.behavior.interrupt_asserted =
            (self.registers[MCP_REG_CANINTF as usize] & MCP_RX0IF_MASK) != 0;
    }

    fn interrupt_level(&self, now_ms: f64) -> u8 {
        if self.behavior.interrupt_asserted || now_ms <= self.can_active_until_ms && !self.rx_queue.is_empty()
        {
            0
        } else {
            1
        }
    }

    fn summary_lines(&self) -> Vec<String> {
        vec![
            format!(
                "[{}] MCP2515 tx={} rx={} int={}",
                self.name,
                self.tx_log.len(),
                self.rx_queue.len(),
                self.behavior.interrupt_asserted
            ),
            format!(
                "[{}] bus_active={} osc={}Hz",
                self.name, self.behavior.can_bus_active, self.behavior.oscillator_hz
            ),
        ]
    }

    fn active_nets(&self, now_ms: f64) -> BTreeSet<String> {
        let mut nets = BTreeSet::new();
        if now_ms <= self.spi_active_until_ms {
            for signal in ["CS", "SCK", "SI", "SO"] {
                if let Some(net) = self.bindings.get(signal) {
                    nets.insert(net.clone());
                }
            }
        }
        if now_ms <= self.can_active_until_ms || self.behavior.can_bus_active {
            for signal in ["CANH", "CANL"] {
                if let Some(net) = self.bindings.get(signal) {
                    nets.insert(net.clone());
                }
            }
        }
        if self.behavior.interrupt_asserted {
            if let Some(net) = self.bindings.get("INT") {
                nets.insert(net.clone());
            }
        }
        nets
    }
}

#[derive(Debug, Clone)]
struct Max31865Device {
    name: String,
    bindings: BTreeMap<String, String>,
    behavior: Max31865Behavior,
    registers: [u8; 8],
    spi_state: Max31865SpiState,
    spi_active_until_ms: f64,
    conversion_ready_at_ms: f64,
}

#[derive(Debug, Clone)]
enum Max31865SpiState {
    Idle,
    Address(u8),
    Read { register: u8 },
    Write { register: u8 },
}

impl Max31865Device {
    fn from_overlay(overlay: &ModuleOverlay) -> Option<Self> {
        let behavior_name = suggested_builtin_behavior_for_board_model(&overlay.model)?;
        let definition = load_built_in_behavior_definition(behavior_name).ok()?;
        let BehaviorInstance::Max31865(behavior) = instantiate_behavior(&definition).ok()? else {
            return None;
        };
        Some(Self {
            name: overlay.name.clone(),
            bindings: overlay
                .bindings
                .iter()
                .map(|binding| (binding.module_signal.clone(), binding.pcb_net.clone()))
                .collect(),
            behavior,
            registers: [0u8; 8],
            spi_state: Max31865SpiState::Idle,
            spi_active_until_ms: 0.0,
            conversion_ready_at_ms: 0.0,
        })
    }

    fn advance_time_ms(&mut self, now_ms: f64) {
        if self.conversion_ready_at_ms != 0.0 && now_ms >= self.conversion_ready_at_ms {
            let raw_code = ((self.behavior.resistance_ohms() / self.behavior.reference_resistor_ohms)
                * 32768.0)
                .round()
                .clamp(0.0, 32767.0) as u16;
            let raw_register = (raw_code << 1) | u16::from(self.behavior.fault_status != 0);
            self.registers[MAX_REG_RTD_MSB as usize] = ((raw_register >> 8) & 0xFF) as u8;
            self.registers[MAX_REG_RTD_LSB as usize] = (raw_register & 0xFF) as u8;
            self.registers[MAX_REG_FAULT_STATUS as usize] = self.behavior.fault_status;
            self.registers[MAX_REG_CONFIG as usize] &= !MAX_CFG_ONE_SHOT;
            self.conversion_ready_at_ms = 0.0;
        }
    }

    fn begin_spi(&mut self, _settings: SpiSettings, now_ms: f64) {
        self.spi_state = Max31865SpiState::Idle;
        self.spi_active_until_ms = now_ms + 35.0;
    }

    fn transfer_spi(&mut self, value: u8, _settings: SpiSettings, now_ms: f64) -> u8 {
        self.spi_active_until_ms = now_ms + 35.0;
        let state = self.spi_state.clone();
        match state {
            Max31865SpiState::Idle => {
                self.spi_state = Max31865SpiState::Address(value);
                0
            }
            Max31865SpiState::Address(address) => {
                let register = address & 0x7F;
                if (address & 0x80) != 0 {
                    self.write_register(register, value, now_ms);
                    self.spi_state = Max31865SpiState::Write {
                        register: register.wrapping_add(1),
                    };
                    0
                } else {
                    let response = self.read_register(register);
                    self.spi_state = Max31865SpiState::Read {
                        register: register.wrapping_add(1),
                    };
                    response
                }
            }
            Max31865SpiState::Read { register } => {
                let response = self.read_register(register);
                self.spi_state = Max31865SpiState::Read {
                    register: register.wrapping_add(1),
                };
                response
            }
            Max31865SpiState::Write { register } => {
                self.write_register(register, value, now_ms);
                self.spi_state = Max31865SpiState::Write {
                    register: register.wrapping_add(1),
                };
                0
            }
        }
    }

    fn end_spi(&mut self, _now_ms: f64) {
        self.spi_state = Max31865SpiState::Idle;
    }

    fn read_register(&self, register: u8) -> u8 {
        self.registers[usize::from(register.min(7))]
    }

    fn write_register(&mut self, register: u8, value: u8, now_ms: f64) {
        let index = usize::from(register.min(7));
        self.registers[index] = value;
        if register == MAX_REG_CONFIG {
            if (value & MAX_CFG_CLEAR_FAULT) != 0 {
                self.registers[MAX_REG_FAULT_STATUS as usize] = 0;
            }
            if (value & MAX_CFG_ONE_SHOT) != 0 && (value & MAX_CFG_BIAS) != 0 {
                self.conversion_ready_at_ms = now_ms + 65.0;
            }
        }
    }

    fn summary_lines(&self) -> Vec<String> {
        vec![
            format!(
                "[{}] MAX31865 {:.2}C {:.2}ohm",
                self.name,
                self.behavior.temperature_c,
                self.behavior.resistance_ohms()
            ),
            format!(
                "[{}] fault=0x{:02X} pending_conversion={}",
                self.name,
                self.behavior.fault_status,
                self.conversion_ready_at_ms > 0.0
            ),
        ]
    }

    fn active_nets(&self, now_ms: f64) -> BTreeSet<String> {
        if now_ms > self.spi_active_until_ms {
            return BTreeSet::new();
        }
        ["CS", "CLK", "SDI", "SDO"]
            .into_iter()
            .filter_map(|signal| self.bindings.get(signal).cloned())
            .collect()
    }
}

#[derive(Debug, Clone)]
struct PwmToVoltageDevice {
    name: String,
    bindings: BTreeMap<String, String>,
    behavior: PwmToVoltageBehavior,
    active_until_ms: f64,
}

impl PwmToVoltageDevice {
    fn from_overlay(overlay: &ModuleOverlay) -> Option<Self> {
        let behavior_name = suggested_builtin_behavior_for_board_model(&overlay.model)?;
        let definition = load_built_in_behavior_definition(behavior_name).ok()?;
        let BehaviorInstance::PwmToVoltage(behavior) = instantiate_behavior(&definition).ok()? else {
            return None;
        };
        Some(Self {
            name: overlay.name.clone(),
            bindings: overlay
                .bindings
                .iter()
                .map(|binding| (binding.module_signal.clone(), binding.pcb_net.clone()))
                .collect(),
            behavior,
            active_until_ms: 0.0,
        })
    }

    fn advance_time_ms(&mut self, _now_ms: f64) {}

    fn set_pwm_duty(&mut self, duty: u8, now_ms: f64) {
        self.behavior.pwm_duty = (f64::from(duty) / 255.0).clamp(0.0, 1.0);
        self.active_until_ms = now_ms + 75.0;
    }

    fn analog_feedback_counts(
        &self,
        pin: BoardPin,
        bindings: &[SignalBinding],
    ) -> Option<u16> {
        let BoardPin::Analog(channel) = pin else {
            return None;
        };
        let target_signal = format!("A{channel}");
        let bound_net = bindings
            .iter()
            .find(|binding| binding.board_signal == target_signal)
            .map(|binding| binding.pcb_net.as_str())?;
        let output_net = self.bindings.get("VOUT")?;
        if bound_net != output_net {
            return None;
        }
        let mut sensed_voltage = self.behavior.output_voltage();
        if bound_net.to_ascii_uppercase().contains("ACT_U") || bound_net.to_ascii_uppercase().contains("_U_") {
            sensed_voltage *= 0.5;
        }
        let counts = ((sensed_voltage.clamp(0.0, 5.0) / 5.0) * 1023.0).round() as u16;
        Some(counts.min(1023))
    }

    fn summary_lines(&self) -> Vec<String> {
        vec![
            format!(
                "[{}] PWM duty={:.3}",
                self.name, self.behavior.pwm_duty
            ),
            format!(
                "[{}] Vout={:.2}V",
                self.name,
                self.behavior.output_voltage()
            ),
        ]
    }

    fn active_nets(&self, now_ms: f64) -> BTreeSet<String> {
        if now_ms > self.active_until_ms {
            return BTreeSet::new();
        }
        ["PWM", "VOUT"]
            .into_iter()
            .filter_map(|signal| self.bindings.get(signal).cloned())
            .collect()
    }
}

fn encode_sht31_serial(serial_number: u32) -> Vec<u8> {
    let first = [((serial_number >> 24) & 0xFF) as u8, ((serial_number >> 16) & 0xFF) as u8];
    let second = [((serial_number >> 8) & 0xFF) as u8, (serial_number & 0xFF) as u8];
    vec![
        first[0],
        first[1],
        sht31_crc(&first),
        second[0],
        second[1],
        sht31_crc(&second),
    ]
}

fn encode_sht31_measurement(temp_c: f64, rh_percent: f64) -> Vec<u8> {
    let raw_temp = (((temp_c + 45.0) / 175.0) * 65535.0)
        .round()
        .clamp(0.0, 65535.0) as u16;
    let raw_rh = ((rh_percent.clamp(0.0, 100.0) / 100.0) * 65535.0)
        .round()
        .clamp(0.0, 65535.0) as u16;
    let temp = [(raw_temp >> 8) as u8, raw_temp as u8];
    let rh = [(raw_rh >> 8) as u8, raw_rh as u8];
    vec![temp[0], temp[1], sht31_crc(&temp), rh[0], rh[1], sht31_crc(&rh)]
}

fn sht31_crc(bytes: &[u8; 2]) -> u8 {
    let mut crc = 0xFFu8;
    for value in bytes {
        crc ^= *value;
        for _ in 0..8 {
            crc = if (crc & 0x80) != 0 {
                (crc << 1) ^ 0x31
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn stable_serial_number(name: &str) -> u32 {
    let mut hash = 0xA531_0001u32;
    for byte in name.bytes() {
        hash = hash.rotate_left(5) ^ u32::from(byte);
    }
    if hash == 0 { 0x5331_0001 } else { hash }
}

#[cfg(test)]
mod tests {
    use super::{
        auto, encode_sht31_measurement, encode_sht31_serial, stable_serial_number, GuiMegaRuntime,
        GuiNanoRuntime, LiveMegaBoard, LiveNanoBoard, ModuleRuntimeSnapshot, SimulationEnvironment,
    };
    use rust_project::{BindingMode, ModuleOverlay, ModuleSignalBinding, SignalBinding};

    #[test]
    fn sht31_payloads_have_expected_length() {
        assert_eq!(encode_sht31_serial(0x1234_5678).len(), 6);
        assert_eq!(encode_sht31_measurement(21.5, 50.0).len(), 6);
        assert_ne!(stable_serial_number("air_1"), 0);
    }

    #[test]
    fn nano_board_reports_live_module_activity() {
        let env = SimulationEnvironment {
            controller_bindings: vec![
                SignalBinding { board_signal: "A4_SDA".to_string(), pcb_net: "/A4{slash}SDA".to_string(), mode: BindingMode::Bus, note: None },
                SignalBinding { board_signal: "A5_SCL".to_string(), pcb_net: "/A5{slash}SCL".to_string(), mode: BindingMode::Bus, note: None },
            ],
            module_overlays: vec![ModuleOverlay {
                name: "sensor_1".to_string(),
                model: "gy_sht31_d".to_string(),
                bindings: vec![
                    ModuleSignalBinding { module_signal: "SDA".to_string(), pcb_net: "/A4{slash}SDA".to_string(), mode: BindingMode::Bus, note: None },
                    ModuleSignalBinding { module_signal: "SCL".to_string(), pcb_net: "/A5{slash}SCL".to_string(), mode: BindingMode::Bus, note: None },
                ],
            }],
        };
        let board = LiveNanoBoard::new(env);
        let snap = board.module_snapshot();
        assert!(!snap.summary_lines.is_empty());
    }

    #[test]
    fn mega_pwm_module_feeds_analog_input() {
        let env = SimulationEnvironment {
            controller_bindings: vec![SignalBinding {
                board_signal: "A10".to_string(),
                pcb_net: "/ACT_U_RAW".to_string(),
                mode: BindingMode::Analog,
                note: None,
            }],
            module_overlays: vec![ModuleOverlay {
                name: "pwm_1".to_string(),
                model: "lc_lm358_pwm_to_0_10v".to_string(),
                bindings: vec![
                    ModuleSignalBinding {
                        module_signal: "VOUT".to_string(),
                        pcb_net: "/ACT_U_RAW".to_string(),
                        mode: BindingMode::Analog,
                        note: None,
                    },
                ],
            }],
        };
        let mut board = LiveMegaBoard::new(env);
        board.set_pwm_duty(BoardPin::Digital(44), 255);
        let counts = board.analog_input_counts(BoardPin::Analog(10));
        assert!(counts > 500);
    }

    #[test]
    fn runtimes_can_apply_environment() {
        let env = SimulationEnvironment::default();
        let mut nano = GuiNanoRuntime::new(env.clone());
        nano.apply_environment(env.clone());
        let _ = nano.module_snapshot();
        let mut mega = GuiMegaRuntime::new(env);
        mega.apply_environment(SimulationEnvironment::default());
        let ModuleRuntimeSnapshot { .. } = mega.module_snapshot();
    }
}
