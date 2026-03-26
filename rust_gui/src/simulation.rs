use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rust_cpu::{Cpu, DataBus, DecodedInstruction};
use rust_mcu::BoardPinLevel;
use rust_project::HostBoard;
use rust_runtime::{load_hex_file, MegaRuntime, NanoRuntime, RuntimeExit};

use crate::arduino::compile_ino;

const RUN_CHUNK_SIZE: usize = 20_000;
const SNAPSHOT_INTERVAL_MS: u64 = 33;
const IDLE_POLL_MS: u64 = 10;
const SERIAL_TAIL_BYTES: usize = 64 * 1024;
const UART_BITS_PER_FRAME: u64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulatorStatus {
    Idle,
    Compiling,
    Ready,
    Running,
    Paused,
    Break,
    Sleep,
    Done,
    Error,
}

impl SimulatorStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Compiling => "Compiling",
            Self::Ready => "Ready",
            Self::Running => "Running",
            Self::Paused => "Paused",
            Self::Break => "Break",
            Self::Sleep => "Sleep",
            Self::Done => "Done",
            Self::Error => "Error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationSnapshot {
    pub board: HostBoard,
    pub status: SimulatorStatus,
    pub source_path: Option<PathBuf>,
    pub firmware_path: Option<PathBuf>,
    pub status_message: String,
    pub compile_log: String,
    pub serial_text: String,
    pub serial_bytes: usize,
    pub serial_configured_baud: u32,
    pub serial_rx_queued: usize,
    pub pc: u32,
    pub sp: u16,
    pub cycles: u64,
    pub synced_cycles: u64,
    pub sreg: u8,
    pub next_instruction: String,
    pub registers: [u8; 32],
    pub extra_lines: Vec<String>,
    pub host_pin_levels: Vec<BoardPinLevel>,
}

impl Default for SimulationSnapshot {
    fn default() -> Self {
        Self {
            board: HostBoard::Mega2560Rev3,
            status: SimulatorStatus::Idle,
            source_path: None,
            firmware_path: None,
            status_message: "Load a .hex file or compile an .ino sketch to begin.".to_string(),
            compile_log: String::new(),
            serial_text: String::new(),
            serial_bytes: 0,
            serial_configured_baud: 0,
            serial_rx_queued: 0,
            pc: 0,
            sp: 0,
            cycles: 0,
            synced_cycles: 0,
            sreg: 0,
            next_instruction: String::new(),
            registers: [0u8; 32],
            extra_lines: Vec::new(),
            host_pin_levels: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SharedSimulationState {
    pub sequence: u64,
    pub snapshot: SimulationSnapshot,
}

impl Default for SharedSimulationState {
    fn default() -> Self {
        Self {
            sequence: 1,
            snapshot: SimulationSnapshot::default(),
        }
    }
}

#[derive(Debug)]
enum WorkerCommand {
    LoadHex { path: PathBuf, board: HostBoard },
    CompileAndLoad { path: PathBuf, board: HostBoard },
    Run,
    Pause,
    Step,
    Reset,
    ClearSerial,
    InjectSerial { payload: Vec<u8>, baud: u32 },
    Shutdown,
}

pub struct SimulationController {
    tx: Sender<WorkerCommand>,
    shared: Arc<Mutex<SharedSimulationState>>,
    worker: Option<JoinHandle<()>>,
}

impl SimulationController {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Mutex::new(SharedSimulationState::default()));
        let worker_shared = Arc::clone(&shared);
        let worker = thread::spawn(move || worker_loop(rx, worker_shared));
        Self {
            tx,
            shared,
            worker: Some(worker),
        }
    }

    pub fn latest_snapshot(&self) -> SharedSimulationState {
        match self.shared.lock() {
            Ok(state) => state.clone(),
            Err(poisoned) => {
                let mut state = poisoned.into_inner().clone();
                state.sequence = state.sequence.wrapping_add(1);
                state.snapshot.status = SimulatorStatus::Error;
                state.snapshot.status_message =
                    "Simulation state became poisoned; the worker likely crashed.".to_string();
                state
            }
        }
    }

    pub fn load_hex(&self, path: PathBuf, board: HostBoard) {
        self.send_command(
            WorkerCommand::LoadHex { path, board },
            "loading a HEX image",
        );
    }

    pub fn compile_and_load(&self, path: PathBuf, board: HostBoard) {
        self.send_command(
            WorkerCommand::CompileAndLoad { path, board },
            "compiling and loading firmware",
        );
    }

    pub fn run(&self) {
        self.send_command(WorkerCommand::Run, "starting execution");
    }

    pub fn pause(&self) {
        self.send_command(WorkerCommand::Pause, "pausing execution");
    }

    pub fn step(&self) {
        self.send_command(WorkerCommand::Step, "stepping execution");
    }

    pub fn reset(&self) {
        self.send_command(WorkerCommand::Reset, "resetting execution");
    }

    pub fn clear_serial(&self) {
        self.send_command(WorkerCommand::ClearSerial, "clearing serial output");
    }

    pub fn inject_serial(&self, payload: Vec<u8>, baud: u32) {
        self.send_command(
            WorkerCommand::InjectSerial { payload, baud },
            "injecting serial input",
        );
    }

    fn send_command(&self, command: WorkerCommand, action: &str) {
        if self.tx.send(command).is_err() {
            self.mark_worker_unavailable(action);
        }
    }

    fn mark_worker_unavailable(&self, action: &str) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.sequence = shared.sequence.wrapping_add(1);
            shared.snapshot.status = SimulatorStatus::Error;
            shared.snapshot.status_message =
                format!("Simulation worker unavailable while {action}.");
        }
    }
}

impl Default for SimulationController {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SimulationController {
    fn drop(&mut self) {
        let _ = self.tx.send(WorkerCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct WorkerState {
    runtime: Option<RuntimeController>,
    loaded_board: HostBoard,
    source_path: Option<PathBuf>,
    firmware_path: Option<PathBuf>,
    compile_log: String,
    status_message: String,
    status: SimulatorStatus,
    pending_serial: VecDeque<PendingSerialInjection>,
}

impl WorkerState {
    fn new() -> Self {
        Self {
            runtime: None,
            loaded_board: HostBoard::Mega2560Rev3,
            source_path: None,
            firmware_path: None,
            compile_log: String::new(),
            status_message: "Load a .hex file or compile an .ino sketch to begin.".to_string(),
            status: SimulatorStatus::Idle,
            pending_serial: VecDeque::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct PendingSerialInjection {
    remaining: VecDeque<u8>,
    cycles_per_byte: u64,
    next_cycle: u64,
}

fn worker_loop(rx: Receiver<WorkerCommand>, shared: Arc<Mutex<SharedSimulationState>>) {
    let mut state = WorkerState::new();
    let mut dirty = true;
    let snapshot_interval = Duration::from_millis(SNAPSHOT_INTERVAL_MS);
    let mut last_publish = Instant::now() - snapshot_interval;

    loop {
        while let Ok(command) = rx.try_recv() {
            match command {
                WorkerCommand::LoadHex { path, board } => {
                    load_runtime_from_hex(&mut state, board, None, &path);
                    dirty = true;
                }
                WorkerCommand::CompileAndLoad { path, board } => {
                    state.loaded_board = board;
                    state.source_path = Some(path.clone());
                    state.status = SimulatorStatus::Compiling;
                    state.status_message =
                        format!("Compiling {} for {}", path.display(), board.label());
                    publish_if_needed(&state, &shared, &mut last_publish, true);

                    match compile_ino(&path, board) {
                        Ok(artifact) => {
                            state.compile_log = format_compile_log(&artifact);
                            load_runtime_from_hex(
                                &mut state,
                                artifact.board,
                                Some(path),
                                &artifact.hex_path,
                            );
                        }
                        Err(error) => {
                            state.runtime = None;
                            state.status = SimulatorStatus::Error;
                            state.status_message = error.to_string();
                        }
                    }
                    dirty = true;
                }
                WorkerCommand::Run => {
                    if state.runtime.is_some()
                        && matches!(
                            state.status,
                            SimulatorStatus::Ready
                                | SimulatorStatus::Paused
                                | SimulatorStatus::Break
                                | SimulatorStatus::Sleep
                                | SimulatorStatus::Done
                        )
                    {
                        state.status = SimulatorStatus::Running;
                        state.status_message = "Running firmware".to_string();
                        dirty = true;
                    }
                }
                WorkerCommand::Pause => {
                    if state.runtime.is_some() && state.status == SimulatorStatus::Running {
                        state.status = SimulatorStatus::Paused;
                        state.status_message = "Execution paused".to_string();
                        dirty = true;
                    }
                }
                WorkerCommand::Step => {
                    if let Some(runtime) = state.runtime.as_mut() {
                        match runtime.run_chunk(1) {
                            Ok(exit) => {
                                state.status = exit.unwrap_or(SimulatorStatus::Paused);
                                if state.status == SimulatorStatus::Running {
                                    state.status = SimulatorStatus::Paused;
                                }
                                state.status_message = "Stepped one instruction".to_string();
                            }
                            Err(error) => {
                                state.status = SimulatorStatus::Error;
                                state.status_message = error;
                            }
                        }
                        dirty = true;
                    }
                }
                WorkerCommand::Reset => {
                    if let Some(firmware_path) = state.firmware_path.clone() {
                        let source_path = state.source_path.clone();
                        let board = state.loaded_board;
                        load_runtime_from_hex(&mut state, board, source_path, &firmware_path);
                        dirty = true;
                    }
                }
                WorkerCommand::ClearSerial => {
                    if let Some(runtime) = state.runtime.as_mut() {
                        runtime.clear_serial_output();
                        state.status_message = "Cleared serial output".to_string();
                        dirty = true;
                    }
                }
                WorkerCommand::InjectSerial { payload, baud } => {
                    if payload.is_empty() {
                        continue;
                    }
                    if let Some(runtime) = state.runtime.as_ref() {
                        let clock_hz = runtime.clock_hz();
                        let cycles_per_byte = ((clock_hz as u64) * UART_BITS_PER_FRAME)
                            .checked_div((baud.max(1)) as u64)
                            .unwrap_or(1)
                            .max(1);
                        state.pending_serial.push_back(PendingSerialInjection {
                            remaining: payload.into_iter().collect(),
                            cycles_per_byte,
                            next_cycle: runtime.cycles(),
                        });
                        state.status_message = format!("Queued serial input at {baud} baud");
                        dirty = true;
                    }
                }
                WorkerCommand::Shutdown => {
                    publish_if_needed(&state, &shared, &mut last_publish, true);
                    return;
                }
            }
        }

        service_pending_serial(&mut state);

        if state.status == SimulatorStatus::Running {
            if let Some(runtime) = state.runtime.as_mut() {
                match runtime.run_chunk(RUN_CHUNK_SIZE) {
                    Ok(exit) => {
                        if let Some(exit_status) = exit {
                            state.status = exit_status;
                            state.status_message = match exit_status {
                                SimulatorStatus::Break => "Break instruction hit".to_string(),
                                SimulatorStatus::Sleep => "CPU entered sleep".to_string(),
                                SimulatorStatus::Done => "Simulation completed".to_string(),
                                _ => "Execution updated".to_string(),
                            };
                        }
                    }
                    Err(error) => {
                        state.status = SimulatorStatus::Error;
                        state.status_message = error;
                    }
                }
                dirty = true;
            } else {
                state.status = SimulatorStatus::Idle;
                state.status_message = "No firmware loaded".to_string();
                dirty = true;
            }
        } else {
            thread::sleep(Duration::from_millis(IDLE_POLL_MS));
        }

        publish_if_needed(&state, &shared, &mut last_publish, dirty);
        if dirty && last_publish.elapsed() >= snapshot_interval {
            dirty = false;
        }
    }
}

fn service_pending_serial(state: &mut WorkerState) {
    let Some(runtime) = state.runtime.as_mut() else {
        return;
    };
    let current_cycle = runtime.cycles();
    while let Some(front) = state.pending_serial.front_mut() {
        if front.next_cycle > current_cycle {
            break;
        }
        let Some(byte) = front.remaining.pop_front() else {
            state.pending_serial.pop_front();
            continue;
        };
        runtime.inject_serial_rx(&[byte]);
        front.next_cycle = front.next_cycle.saturating_add(front.cycles_per_byte);
        if front.remaining.is_empty() {
            state.pending_serial.pop_front();
        }
    }
}

fn load_runtime_from_hex(
    state: &mut WorkerState,
    board: HostBoard,
    source_path: Option<PathBuf>,
    firmware_path: &Path,
) {
    match RuntimeController::load(board, firmware_path) {
        Ok(runtime) => {
            state.runtime = Some(runtime);
            state.loaded_board = board;
            state.source_path = source_path;
            state.firmware_path = Some(firmware_path.to_path_buf());
            state.status = SimulatorStatus::Ready;
            state.status_message =
                format!("Loaded {} for {}", firmware_path.display(), board.label());
        }
        Err(error) => {
            state.runtime = None;
            state.loaded_board = board;
            state.source_path = source_path;
            state.firmware_path = Some(firmware_path.to_path_buf());
            state.status = SimulatorStatus::Error;
            state.status_message = error;
        }
    }
}

fn publish_if_needed(
    state: &WorkerState,
    shared: &Arc<Mutex<SharedSimulationState>>,
    last_publish: &mut Instant,
    dirty: bool,
) {
    if !dirty && last_publish.elapsed() < Duration::from_millis(SNAPSHOT_INTERVAL_MS) {
        return;
    }

    if let Ok(mut shared_state) = shared.lock() {
        shared_state.sequence = shared_state.sequence.wrapping_add(1);
        shared_state.snapshot = capture_snapshot(state);
        *last_publish = Instant::now();
    }
}

fn capture_snapshot(state: &WorkerState) -> SimulationSnapshot {
    if let Some(runtime) = state.runtime.as_ref() {
        runtime.snapshot(
            state.loaded_board,
            state.source_path.clone(),
            state.firmware_path.clone(),
            state.status,
            state.status_message.clone(),
            state.compile_log.clone(),
        )
    } else {
        let mut snapshot = SimulationSnapshot::default();
        snapshot.board = state.loaded_board;
        snapshot.source_path = state.source_path.clone();
        snapshot.firmware_path = state.firmware_path.clone();
        snapshot.status = state.status;
        snapshot.status_message = state.status_message.clone();
        snapshot.compile_log = state.compile_log.clone();
        snapshot
    }
}

enum RuntimeController {
    Nano(NanoRuntime),
    Mega(MegaRuntime),
}

impl RuntimeController {
    fn load(board: HostBoard, firmware_path: &Path) -> Result<Self, String> {
        match board {
            HostBoard::NanoV3 => {
                let mut runtime = NanoRuntime::new();
                load_hex_file(&mut runtime.cpu, firmware_path)
                    .map_err(|error| error.to_string())?;
                Ok(Self::Nano(runtime))
            }
            HostBoard::Mega2560Rev3 => {
                let mut runtime = MegaRuntime::new();
                load_hex_file(&mut runtime.cpu, firmware_path)
                    .map_err(|error| error.to_string())?;
                Ok(Self::Mega(runtime))
            }
        }
    }

    fn run_chunk(&mut self, instruction_budget: usize) -> Result<Option<SimulatorStatus>, String> {
        let exit = match self {
            Self::Nano(runtime) => runtime
                .run_chunk(instruction_budget, false)
                .map(|(_, exit)| exit)
                .map_err(|error| error.to_string())?,
            Self::Mega(runtime) => runtime
                .run_chunk(instruction_budget, false)
                .map(|(_, exit)| exit)
                .map_err(|error| error.to_string())?,
        };
        Ok(map_runtime_exit(exit))
    }

    fn clear_serial_output(&mut self) {
        match self {
            Self::Nano(runtime) => runtime.clear_serial_output(),
            Self::Mega(runtime) => runtime.clear_serial_output(),
        }
    }

    fn inject_serial_rx(&mut self, payload: &[u8]) {
        match self {
            Self::Nano(runtime) => runtime.inject_serial_rx(payload),
            Self::Mega(runtime) => runtime.inject_serial_rx(payload),
        }
    }

    fn cycles(&self) -> u64 {
        match self {
            Self::Nano(runtime) => runtime.cpu.cycles,
            Self::Mega(runtime) => runtime.cpu.cycles,
        }
    }

    fn clock_hz(&self) -> u32 {
        match self {
            Self::Nano(runtime) => runtime.cpu.bus.clock_hz,
            Self::Mega(runtime) => runtime.cpu.bus.clock_hz,
        }
    }

    fn snapshot(
        &self,
        board: HostBoard,
        source_path: Option<PathBuf>,
        firmware_path: Option<PathBuf>,
        status: SimulatorStatus,
        status_message: String,
        compile_log: String,
    ) -> SimulationSnapshot {
        match self {
            Self::Nano(runtime) => capture_runtime_snapshot(
                board,
                source_path,
                firmware_path,
                status,
                status_message,
                compile_log,
                &runtime.cpu,
                runtime.serial_output_bytes(),
                runtime.configured_serial_baud(),
                runtime.cpu.bus.serial0.rx_queue.len(),
                vec![
                    format!(
                        "Timer0: pending={} rem={} | USART0: tx_busy={} tx_cycles={} rx_queue={}",
                        runtime.cpu.bus.timer0.interrupt_pending,
                        runtime.cpu.bus.timer0.cycle_remainder,
                        runtime.cpu.bus.serial0.tx_busy_byte.is_some(),
                        runtime.cpu.bus.serial0.tx_cycles_remaining,
                        runtime.cpu.bus.serial0.rx_queue.len()
                    ),
                    format!(
                        "SPI: active={} | TWI: active={} irq={} addr={} mode={}",
                        runtime.cpu.bus.spi_transaction_active,
                        runtime.cpu.bus.twi_bus_active,
                        runtime.cpu.bus.twi_interrupt_pending,
                        runtime
                            .cpu
                            .bus
                            .twi_address
                            .map(|value| format!("0x{value:02X}"))
                            .unwrap_or_else(|| "--".to_string()),
                        if runtime.cpu.bus.twi_read_mode {
                            "read"
                        } else {
                            "write"
                        }
                    ),
                ],
                runtime.cpu.bus.synced_cycles,
                runtime.cpu.bus.host_pin_levels(),
            ),
            Self::Mega(runtime) => capture_runtime_snapshot(
                board,
                source_path,
                firmware_path,
                status,
                status_message,
                compile_log,
                &runtime.cpu,
                runtime.serial_output_bytes(),
                runtime.configured_serial_baud(),
                runtime.cpu.bus.serial0.rx_queue.len(),
                vec![
                    format!(
                        "Timer0: pending={} rem={} | USART0: tx_busy={} tx_cycles={} rx_queue={}",
                        runtime.cpu.bus.timer0.interrupt_pending,
                        runtime.cpu.bus.timer0.cycle_remainder,
                        runtime.cpu.bus.serial0.tx_busy_byte.is_some(),
                        runtime.cpu.bus.serial0.tx_cycles_remaining,
                        runtime.cpu.bus.serial0.rx_queue.len()
                    ),
                    format!(
                        "ADC: pending={} rem={} | SPI CAN: {} | SPI RTD: {} | EEPROM: {} bytes",
                        runtime.cpu.bus.adc.interrupt_pending,
                        runtime.cpu.bus.adc.cycles_remaining,
                        runtime.cpu.bus.spi_transaction_active_can,
                        runtime.cpu.bus.spi_transaction_active_rtd,
                        runtime.cpu.bus.eeprom.len()
                    ),
                ],
                runtime.cpu.bus.synced_cycles,
                runtime.cpu.bus.host_pin_levels(),
            ),
        }
    }
}

fn map_runtime_exit(exit: Option<RuntimeExit>) -> Option<SimulatorStatus> {
    match exit {
        Some(RuntimeExit::BreakHit) => Some(SimulatorStatus::Break),
        Some(RuntimeExit::Sleeping) => Some(SimulatorStatus::Sleep),
        Some(RuntimeExit::UntilSerialSatisfied) => Some(SimulatorStatus::Done),
        Some(RuntimeExit::MaxInstructionsReached) | None => None,
    }
}

fn capture_runtime_snapshot<B: DataBus>(
    board: HostBoard,
    source_path: Option<PathBuf>,
    firmware_path: Option<PathBuf>,
    status: SimulatorStatus,
    status_message: String,
    compile_log: String,
    cpu: &Cpu<B>,
    serial_bytes: &[u8],
    serial_configured_baud: u32,
    serial_rx_queued: usize,
    extra_lines: Vec<String>,
    synced_cycles: u64,
    host_pin_levels: Vec<BoardPinLevel>,
) -> SimulationSnapshot {
    let mut registers = [0u8; 32];
    registers.copy_from_slice(&cpu.data[0..32]);
    let next_instruction = match cpu.decode_at(cpu.pc) {
        Ok(decoded) => format_instruction(&decoded),
        Err(error) => format!("<decode error: {error}>"),
    };

    SimulationSnapshot {
        board,
        status,
        source_path,
        firmware_path,
        status_message,
        compile_log,
        serial_text: serial_text(serial_bytes),
        serial_bytes: serial_bytes.len(),
        serial_configured_baud,
        serial_rx_queued,
        pc: cpu.pc,
        sp: cpu.sp(),
        cycles: cpu.cycles,
        synced_cycles,
        sreg: cpu.data[cpu.config.sreg_address],
        next_instruction,
        registers,
        extra_lines,
        host_pin_levels,
    }
}

fn serial_text(serial_bytes: &[u8]) -> String {
    let start = serial_bytes.len().saturating_sub(SERIAL_TAIL_BYTES);
    String::from_utf8_lossy(&serial_bytes[start..]).replace('\r', "")
}

fn format_instruction(instruction: &DecodedInstruction) -> String {
    let mut rendered = format!(
        "0x{:06X}: {:?} (opcode=0x{:04X}",
        instruction.address, instruction.mnemonic, instruction.opcode
    );
    if let Some(next_word) = instruction.next_word {
        let _ = write!(rendered, " next=0x{next_word:04X}");
    }
    rendered.push(')');

    let operand_text = format_operands(instruction);
    if !operand_text.is_empty() {
        rendered.push(' ');
        rendered.push_str(&operand_text);
    }
    rendered
}

fn format_operands(instruction: &DecodedInstruction) -> String {
    let operands = &instruction.operands;
    let mut parts = Vec::new();
    if let Some(pointer) = operands.pointer {
        let pointer_text = match (pointer, operands.mode) {
            (rust_cpu::PointerRegister::X, Some(rust_cpu::PointerMode::PostIncrement)) => {
                "X+".to_string()
            }
            (rust_cpu::PointerRegister::Y, Some(rust_cpu::PointerMode::PostIncrement)) => {
                "Y+".to_string()
            }
            (rust_cpu::PointerRegister::Z, Some(rust_cpu::PointerMode::PostIncrement)) => {
                "Z+".to_string()
            }
            (rust_cpu::PointerRegister::X, Some(rust_cpu::PointerMode::PreDecrement)) => {
                "-X".to_string()
            }
            (rust_cpu::PointerRegister::Y, Some(rust_cpu::PointerMode::PreDecrement)) => {
                "-Y".to_string()
            }
            (rust_cpu::PointerRegister::Z, Some(rust_cpu::PointerMode::PreDecrement)) => {
                "-Z".to_string()
            }
            (rust_cpu::PointerRegister::X, _) => "X".to_string(),
            (rust_cpu::PointerRegister::Y, _) => "Y".to_string(),
            (rust_cpu::PointerRegister::Z, _) => "Z".to_string(),
        };
        parts.push(pointer_text);
    }
    if let Some(d) = operands.d {
        parts.push(format!("d=r{d}"));
    }
    if let Some(r) = operands.r {
        parts.push(format!("r=r{r}"));
    }
    if let Some(a) = operands.a {
        parts.push(format!("a=0x{a:02X}"));
    }
    if let Some(b) = operands.b {
        parts.push(format!("b={b}"));
    }
    if let Some(s) = operands.s {
        parts.push(format!("s={s}"));
    }
    if let Some(k) = operands.k {
        parts.push(format!("k={k}"));
    }
    if let Some(q) = operands.q {
        parts.push(format!("q={q}"));
    }
    parts.join(" ")
}

fn format_compile_log(artifact: &crate::arduino::CompileArtifact) -> String {
    let mut rendered = format!(
        "$ arduino-cli compile --fqbn {} --build-path {} {}\n",
        artifact.board.fqbn(),
        artifact.build_path.display(),
        artifact.sketch_path.display()
    );
    if !artifact.stdout.trim().is_empty() {
        rendered.push_str("--- stdout ---\n");
        rendered.push_str(&artifact.stdout);
        if !artifact.stdout.ends_with('\n') {
            rendered.push('\n');
        }
    }
    if !artifact.stderr.trim().is_empty() {
        rendered.push_str("--- stderr ---\n");
        rendered.push_str(&artifact.stderr);
        if !artifact.stderr.ends_with('\n') {
            rendered.push('\n');
        }
    }
    rendered.push_str(&format!("Loaded HEX: {}\n", artifact.hex_path.display()));
    rendered
}

#[cfg(test)]
mod tests {
    use super::{serial_text, SimulationSnapshot, SimulatorStatus};

    #[test]
    fn serial_text_normalizes_carriage_returns() {
        let text = serial_text(b"hello\r\nworld\r\n");
        assert_eq!(text, "hello\nworld\n");
    }

    #[test]
    fn default_snapshot_starts_idle() {
        let snapshot = SimulationSnapshot::default();
        assert_eq!(snapshot.status, SimulatorStatus::Idle);
        assert!(snapshot.status_message.contains("Load"));
    }
}
