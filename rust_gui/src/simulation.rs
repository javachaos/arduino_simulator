use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rust_cpu::{Cpu, DataBus, DecodedInstruction};
use rust_mcu::{BoardPin, BoardPinLevel};
use rust_project::HostBoard;
use rust_runtime::{load_hex_file, MegaRuntime, NanoRuntime, RuntimeExit};

use crate::arduino::compile_ino;

const RUN_CHUNK_SIZE: usize = 20_000;
const SNAPSHOT_INTERVAL_MS: u64 = 33;
const HOST_PIN_ACTIVITY_HOLD_MS: u64 = 240;
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
    pub host_pin_recent_activity: Vec<BoardPinLevel>,
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
            host_pin_recent_activity: Vec::new(),
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
    last_host_pin_levels: Vec<BoardPinLevel>,
    host_pin_activity_until: HashMap<BoardPin, Instant>,
    run_pacing_anchor: Option<RunPacingAnchor>,
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
            last_host_pin_levels: Vec::new(),
            host_pin_activity_until: HashMap::new(),
            run_pacing_anchor: None,
        }
    }

    fn recent_host_pin_activity(&self) -> Vec<BoardPinLevel> {
        let now = Instant::now();
        self.last_host_pin_levels
            .iter()
            .filter(|entry| {
                self.host_pin_activity_until
                    .get(&entry.pin)
                    .map(|deadline| *deadline > now)
                    .unwrap_or(false)
            })
            .map(|entry| BoardPinLevel {
                pin: entry.pin,
                level: 1,
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
struct PendingSerialInjection {
    remaining: VecDeque<u8>,
    cycles_per_byte: u64,
    next_cycle: u64,
}

#[derive(Debug, Clone, Copy)]
struct RunPacingAnchor {
    wall_started_at: Instant,
    synced_cycles_started: u64,
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
                    state.run_pacing_anchor = None;
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
                    state.run_pacing_anchor = None;
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
                        state.run_pacing_anchor = None;
                        dirty = true;
                    }
                }
                WorkerCommand::Pause => {
                    if state.runtime.is_some() && state.status == SimulatorStatus::Running {
                        state.status = SimulatorStatus::Paused;
                        state.status_message = "Execution paused".to_string();
                        state.run_pacing_anchor = None;
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
                        state.run_pacing_anchor = None;
                        dirty = true;
                    }
                }
                WorkerCommand::Reset => {
                    if let Some(firmware_path) = state.firmware_path.clone() {
                        let source_path = state.source_path.clone();
                        let board = state.loaded_board;
                        load_runtime_from_hex(&mut state, board, source_path, &firmware_path);
                        state.run_pacing_anchor = None;
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
            pace_running_runtime(&mut state);
        } else {
            state.run_pacing_anchor = None;
            thread::sleep(Duration::from_millis(IDLE_POLL_MS));
        }

        refresh_host_pin_activity(&mut state);
        publish_if_needed(&state, &shared, &mut last_publish, dirty);
        if dirty && last_publish.elapsed() >= snapshot_interval {
            dirty = false;
        }
    }
}

fn refresh_host_pin_activity(state: &mut WorkerState) {
    let Some(runtime) = state.runtime.as_ref() else {
        state.last_host_pin_levels.clear();
        state.host_pin_activity_until.clear();
        return;
    };

    let now = Instant::now();
    let next_levels = runtime.host_pin_levels();
    let previous_levels = state
        .last_host_pin_levels
        .iter()
        .map(|entry| (entry.pin, entry.level))
        .collect::<HashMap<_, _>>();

    for entry in &next_levels {
        let changed = previous_levels
            .get(&entry.pin)
            .map(|previous| *previous != entry.level)
            .unwrap_or(entry.level != 0);
        if changed || entry.level != 0 {
            state.host_pin_activity_until.insert(
                entry.pin,
                now + Duration::from_millis(HOST_PIN_ACTIVITY_HOLD_MS),
            );
        }
    }

    let current_levels = next_levels
        .iter()
        .map(|entry| (entry.pin, entry.level))
        .collect::<HashMap<_, _>>();
    state.host_pin_activity_until.retain(|pin, deadline| {
        current_levels.get(pin).copied().unwrap_or(0) != 0 || *deadline > now
    });
    state.last_host_pin_levels = next_levels;
}

fn pace_running_runtime(state: &mut WorkerState) {
    let Some(runtime) = state.runtime.as_ref() else {
        state.run_pacing_anchor = None;
        return;
    };
    if state.status != SimulatorStatus::Running {
        state.run_pacing_anchor = None;
        return;
    }

    let anchor = state.run_pacing_anchor.get_or_insert_with(|| RunPacingAnchor {
        wall_started_at: Instant::now(),
        synced_cycles_started: runtime.synced_cycles(),
    });
    let simulated_elapsed = runtime
        .synced_cycles()
        .saturating_sub(anchor.synced_cycles_started);
    let target_elapsed = duration_for_cycles(simulated_elapsed, runtime.clock_hz());
    let actual_elapsed = anchor.wall_started_at.elapsed();
    if let Some(remaining) = target_elapsed.checked_sub(actual_elapsed) {
        thread::sleep(remaining.min(Duration::from_millis(8)));
    }
}

fn duration_for_cycles(cycles: u64, clock_hz: u32) -> Duration {
    if cycles == 0 || clock_hz == 0 {
        return Duration::default();
    }

    let hz = u64::from(clock_hz);
    let seconds = cycles / hz;
    let nanos = ((cycles % hz) * 1_000_000_000u64) / hz;
    Duration::from_secs(seconds) + Duration::from_nanos(nanos)
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
            state.recent_host_pin_activity(),
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

    fn host_pin_levels(&self) -> Vec<BoardPinLevel> {
        match self {
            Self::Nano(runtime) => runtime.cpu.bus.host_pin_levels(),
            Self::Mega(runtime) => runtime.cpu.bus.host_pin_levels(),
        }
    }

    fn synced_cycles(&self) -> u64 {
        match self {
            Self::Nano(runtime) => runtime.cpu.bus.synced_cycles,
            Self::Mega(runtime) => runtime.cpu.bus.synced_cycles,
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
        recent_host_pin_activity: Vec<BoardPinLevel>,
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
                recent_host_pin_activity,
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
                recent_host_pin_activity,
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
    host_pin_recent_activity: Vec<BoardPinLevel>,
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
        host_pin_recent_activity,
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
    use std::fs;
    use std::path::PathBuf;
    use std::time::Duration;

    use tempfile::tempdir;

    use rust_cpu::{DecodedInstruction, Mnemonic, OperandSet};
    use rust_mcu::{BoardPin, NanoBoard};
    use rust_runtime::RuntimeExit;

    use super::{
        capture_snapshot, duration_for_cycles, format_compile_log, format_instruction,
        load_runtime_from_hex, map_runtime_exit, refresh_host_pin_activity, serial_text,
        service_pending_serial, PendingSerialInjection, RuntimeController, SharedSimulationState,
        SimulationSnapshot, SimulatorStatus, WorkerState, SERIAL_TAIL_BYTES,
    };
    use crate::arduino::CompileArtifact;

    fn ldi(d: u8, k: u8) -> u16 {
        assert!((16..=31).contains(&d));
        0xE000 | (((k as u16) & 0xF0) << 4) | (((d - 16) as u16) << 4) | ((k as u16) & 0x0F)
    }

    fn brk() -> u16 {
        0x9598
    }

    fn hex_record(address: u16, record_type: u8, payload: &[u8]) -> String {
        let mut body = Vec::with_capacity(payload.len() + 5);
        body.push(payload.len() as u8);
        body.push((address >> 8) as u8);
        body.push((address & 0xFF) as u8);
        body.push(record_type);
        body.extend_from_slice(payload);
        let checksum =
            (0u8).wrapping_sub(body.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte)));
        body.push(checksum);
        format!(
            ":{}",
            body.iter()
                .map(|byte| format!("{byte:02X}"))
                .collect::<String>()
        )
    }

    fn words_to_bytes(words: &[u16]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(words.len() * 2);
        for word in words {
            bytes.push((word & 0xFF) as u8);
            bytes.push((word >> 8) as u8);
        }
        bytes
    }

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

    #[test]
    fn simulator_status_labels_and_shared_defaults_are_stable() {
        let cases = [
            (SimulatorStatus::Idle, "Idle"),
            (SimulatorStatus::Compiling, "Compiling"),
            (SimulatorStatus::Ready, "Ready"),
            (SimulatorStatus::Running, "Running"),
            (SimulatorStatus::Paused, "Paused"),
            (SimulatorStatus::Break, "Break"),
            (SimulatorStatus::Sleep, "Sleep"),
            (SimulatorStatus::Done, "Done"),
            (SimulatorStatus::Error, "Error"),
        ];

        for (status, label) in cases {
            assert_eq!(status.label(), label);
        }

        let shared = SharedSimulationState::default();
        assert_eq!(shared.sequence, 1);
        assert_eq!(shared.snapshot.status, SimulatorStatus::Idle);
    }

    #[test]
    fn serial_text_keeps_only_the_recent_tail() {
        let mut bytes = vec![b'A'; 32];
        bytes.extend(vec![b'B'; SERIAL_TAIL_BYTES]);

        let text = serial_text(&bytes);
        assert_eq!(text.len(), SERIAL_TAIL_BYTES);
        assert!(text.starts_with('B'));
        assert!(!text.contains('A'));
    }

    #[test]
    fn capture_snapshot_without_runtime_preserves_worker_metadata() {
        let mut state = WorkerState::new();
        state.loaded_board = rust_project::HostBoard::NanoV3;
        state.source_path = Some(PathBuf::from("sketches/blink.ino"));
        state.firmware_path = Some(PathBuf::from("build/blink.hex"));
        state.compile_log = "compile ok".to_string();
        state.status = SimulatorStatus::Error;
        state.status_message = "compile failed".to_string();

        let snapshot = capture_snapshot(&state);
        assert_eq!(snapshot.board, rust_project::HostBoard::NanoV3);
        assert_eq!(snapshot.status, SimulatorStatus::Error);
        assert_eq!(
            snapshot.source_path,
            Some(PathBuf::from("sketches/blink.ino"))
        );
        assert_eq!(
            snapshot.firmware_path,
            Some(PathBuf::from("build/blink.hex"))
        );
        assert_eq!(snapshot.compile_log, "compile ok");
        assert_eq!(snapshot.status_message, "compile failed");
    }

    #[test]
    fn format_compile_log_includes_command_output_and_loaded_hex_path() {
        let artifact = CompileArtifact {
            board: rust_project::HostBoard::Mega2560Rev3,
            sketch_path: PathBuf::from("/tmp/blink"),
            build_path: PathBuf::from("/tmp/build"),
            hex_path: PathBuf::from("/tmp/build/blink.ino.hex"),
            stdout: "compiled".to_string(),
            stderr: "warnings".to_string(),
        };

        let rendered = format_compile_log(&artifact);
        assert!(rendered.contains("arduino-cli compile --fqbn arduino:avr:mega"));
        assert!(rendered.contains("--- stdout ---\ncompiled\n"));
        assert!(rendered.contains("--- stderr ---\nwarnings\n"));
        assert!(rendered.contains("Loaded HEX: /tmp/build/blink.ino.hex"));
    }

    #[test]
    fn format_instruction_renders_operands_and_next_word() {
        let instruction = DecodedInstruction {
            address: 0x40,
            opcode: 0x9002,
            next_word: Some(0xCAFE),
            mnemonic: Mnemonic::LdDisp,
            word_length: 2,
            operands: OperandSet {
                d: Some(18),
                r: Some(3),
                a: Some(0x20),
                b: Some(4),
                s: Some(6),
                k: Some(7),
                q: Some(2),
                pointer: Some(rust_cpu::PointerRegister::Y),
                mode: Some(rust_cpu::PointerMode::PreDecrement),
            },
        };

        let rendered = format_instruction(&instruction);
        assert!(rendered.contains("0x000040: LdDisp"));
        assert!(rendered.contains("opcode=0x9002"));
        assert!(rendered.contains("next=0xCAFE"));
        assert!(rendered.contains("-Y"));
        assert!(rendered.contains("d=r18"));
        assert!(rendered.contains("r=r3"));
        assert!(rendered.contains("a=0x20"));
        assert!(rendered.contains("b=4"));
        assert!(rendered.contains("s=6"));
        assert!(rendered.contains("k=7"));
        assert!(rendered.contains("q=2"));
    }

    #[test]
    fn runtime_exit_mapping_matches_simulator_statuses() {
        assert_eq!(
            map_runtime_exit(Some(RuntimeExit::BreakHit)),
            Some(SimulatorStatus::Break)
        );
        assert_eq!(
            map_runtime_exit(Some(RuntimeExit::Sleeping)),
            Some(SimulatorStatus::Sleep)
        );
        assert_eq!(
            map_runtime_exit(Some(RuntimeExit::UntilSerialSatisfied)),
            Some(SimulatorStatus::Done)
        );
        assert_eq!(
            map_runtime_exit(Some(RuntimeExit::MaxInstructionsReached)),
            None
        );
        assert_eq!(map_runtime_exit(None), None);
    }

    #[test]
    fn load_runtime_from_hex_sets_ready_state_for_valid_firmware() {
        let temp = tempdir().expect("tempdir");
        let source_path = temp.path().join("blink.ino");
        let firmware_path = temp.path().join("blink.hex");
        let hex = format!(
            "{}\n{}\n",
            hex_record(0x0000, 0x00, &words_to_bytes(&[ldi(16, 0x2A), brk()])),
            hex_record(0x0000, 0x01, &[])
        );
        fs::write(&firmware_path, hex).expect("write hex");

        let mut state = WorkerState::new();
        load_runtime_from_hex(
            &mut state,
            rust_project::HostBoard::NanoV3,
            Some(source_path.clone()),
            &firmware_path,
        );

        assert_eq!(state.status, SimulatorStatus::Ready);
        assert_eq!(state.source_path, Some(source_path));
        assert_eq!(state.firmware_path, Some(firmware_path.clone()));
        assert!(matches!(state.runtime, Some(RuntimeController::Nano(_))));

        let snapshot = capture_snapshot(&state);
        assert_eq!(snapshot.board, rust_project::HostBoard::NanoV3);
        assert_eq!(snapshot.status, SimulatorStatus::Ready);
        assert_eq!(snapshot.firmware_path, Some(firmware_path));
        assert!(!snapshot.next_instruction.is_empty());
    }

    #[test]
    fn load_runtime_from_hex_sets_error_state_for_invalid_firmware() {
        let temp = tempdir().expect("tempdir");
        let firmware_path = temp.path().join("broken.hex");
        fs::write(&firmware_path, "not intel hex").expect("write invalid hex");

        let mut state = WorkerState::new();
        load_runtime_from_hex(
            &mut state,
            rust_project::HostBoard::Mega2560Rev3,
            None,
            &firmware_path,
        );

        assert!(state.runtime.is_none());
        assert_eq!(state.status, SimulatorStatus::Error);
        assert_eq!(state.firmware_path, Some(firmware_path));
        assert!(!state.status_message.is_empty());
    }

    #[test]
    fn service_pending_serial_injects_bytes_when_cycles_advance() {
        let mut state = WorkerState::new();
        state.runtime = Some(RuntimeController::Nano(rust_runtime::NanoRuntime::new()));
        state.pending_serial.push_back(PendingSerialInjection {
            remaining: vec![b'O', b'K'].into_iter().collect(),
            cycles_per_byte: 10,
            next_cycle: 0,
        });

        service_pending_serial(&mut state);
        match state.runtime.as_ref().expect("runtime") {
            RuntimeController::Nano(runtime) => {
                assert_eq!(runtime.cpu.bus.serial0.rx_queue.len(), 1);
                assert_eq!(runtime.cpu.bus.serial0.rx_queue[0], b'O');
            }
            RuntimeController::Mega(_) => panic!("expected nano runtime"),
        }
        assert_eq!(state.pending_serial.len(), 1);

        match state.runtime.as_mut().expect("runtime") {
            RuntimeController::Nano(runtime) => runtime.cpu.cycles = 10,
            RuntimeController::Mega(_) => panic!("expected nano runtime"),
        }
        service_pending_serial(&mut state);

        match state.runtime.as_ref().expect("runtime") {
            RuntimeController::Nano(runtime) => {
                assert_eq!(runtime.cpu.bus.serial0.rx_queue.len(), 2);
                assert_eq!(runtime.cpu.bus.serial0.rx_queue[1], b'K');
            }
            RuntimeController::Mega(_) => panic!("expected nano runtime"),
        }
        assert!(state.pending_serial.is_empty());
    }

    #[test]
    fn duration_for_cycles_matches_the_simulated_16mhz_clock() {
        assert_eq!(duration_for_cycles(16_000_000, 16_000_000), Duration::from_secs(1));
        assert_eq!(duration_for_cycles(8_000_000, 16_000_000), Duration::from_millis(500));
    }

    #[test]
    fn recent_host_pin_activity_survives_fast_high_to_low_transitions() {
        let mut state = WorkerState::new();
        state.loaded_board = rust_project::HostBoard::NanoV3;
        state.runtime = Some(RuntimeController::Nano(rust_runtime::NanoRuntime::new()));

        match state.runtime.as_mut().expect("runtime") {
            RuntimeController::Nano(runtime) => {
                runtime.cpu.bus.board.write_pin(BoardPin::Digital(13), 1);
            }
            RuntimeController::Mega(_) => panic!("expected nano runtime"),
        }
        refresh_host_pin_activity(&mut state);

        match state.runtime.as_mut().expect("runtime") {
            RuntimeController::Nano(runtime) => {
                runtime.cpu.bus.board.write_pin(BoardPin::Digital(13), 0);
            }
            RuntimeController::Mega(_) => panic!("expected nano runtime"),
        }
        refresh_host_pin_activity(&mut state);

        assert!(state
            .recent_host_pin_activity()
            .iter()
            .any(|entry| entry.pin == BoardPin::Digital(13) && entry.level == 1));

        let snapshot = capture_snapshot(&state);
        assert_eq!(
            snapshot
                .host_pin_levels
                .iter()
                .find(|entry| entry.pin == BoardPin::Digital(13))
                .map(|entry| entry.level),
            Some(0)
        );
        assert!(snapshot
            .host_pin_recent_activity
            .iter()
            .any(|entry| entry.pin == BoardPin::Digital(13) && entry.level == 1));
    }
}
