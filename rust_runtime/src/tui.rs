use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rust_cpu::{Cpu, CpuError, DataBus, DecodedInstruction};
use rust_mcu::atmega2560::{Atmega2560Bus, MegaBoard};
use rust_mcu::atmega328p::{Atmega328pBus, NanoBoard};

use crate::cli::CliError;
use crate::firmware::load_hex_file;
use crate::runtime::{MegaRuntime, NanoRuntime, RuntimeExit};

const DEFAULT_CHUNK_SIZE: usize = 10_000;
const DEFAULT_REFRESH_MS: u64 = 50;
const IDLE_POLL_MS: u64 = 10;
const UI_POLL_MS: u64 = 5;
const SERIAL_TAIL_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorConfig {
    pub firmware_path: PathBuf,
    pub max_instructions: Option<usize>,
    pub chunk_size: usize,
    pub refresh_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MonitorStatus {
    Run,
    Pause,
    Break,
    Sleep,
    Done,
    Error,
}

impl MonitorStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Run => "RUN",
            Self::Pause => "PAUSE",
            Self::Break => "BREAK",
            Self::Sleep => "SLEEP",
            Self::Done => "DONE",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CpuPanelState {
    title: String,
    status: MonitorStatus,
    pc: u32,
    sp: u16,
    cycles: u64,
    synced_cycles: u64,
    serial_bytes: usize,
    sreg: u8,
    registers: [u8; 32],
    next_instruction: String,
    extra_lines: Vec<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MonitorSnapshot {
    panel: CpuPanelState,
    serial_tail: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SharedMonitorState {
    sequence: u64,
    snapshot: MonitorSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MonitorCommand {
    TogglePause,
    Step,
    ClearSerial,
    Quit,
}

pub fn monitor_nano(config: MonitorConfig) -> Result<i32, CliError> {
    let mut runtime = NanoRuntime::new();
    load_hex_file(&mut runtime.cpu, &config.firmware_path)?;
    monitor_loop(runtime, config)
}

pub fn monitor_mega(config: MonitorConfig) -> Result<i32, CliError> {
    let mut runtime = MegaRuntime::new();
    load_hex_file(&mut runtime.cpu, &config.firmware_path)?;
    monitor_loop(runtime, config)
}

pub fn default_chunk_size() -> usize {
    DEFAULT_CHUNK_SIZE
}

pub fn default_refresh_ms() -> u64 {
    DEFAULT_REFRESH_MS
}

trait MonitorBusInfo {
    fn synced_cycles(&self) -> u64;
    fn extra_status_lines(&self) -> Vec<String>;
}

impl<B: NanoBoard> MonitorBusInfo for Atmega328pBus<B> {
    fn synced_cycles(&self) -> u64 {
        self.synced_cycles
    }

    fn extra_status_lines(&self) -> Vec<String> {
        vec![
            format!(
                "Timer0: pending={} rem={} | USART0: tx_busy={} tx_cycles={} rx_queue={}",
                self.timer0.interrupt_pending,
                self.timer0.cycle_remainder,
                self.serial0.tx_busy_byte.is_some(),
                self.serial0.tx_cycles_remaining,
                self.serial0.rx_queue.len()
            ),
            format!(
                "SPI: active={} | TWI: active={} irq={} addr={} mode={}",
                self.spi_transaction_active,
                self.twi_bus_active,
                self.twi_interrupt_pending,
                self.twi_address
                    .map(|value| format!("0x{value:02X}"))
                    .unwrap_or_else(|| "--".to_string()),
                if self.twi_read_mode { "read" } else { "write" }
            ),
        ]
    }
}

impl<B: MegaBoard> MonitorBusInfo for Atmega2560Bus<B> {
    fn synced_cycles(&self) -> u64 {
        self.synced_cycles
    }

    fn extra_status_lines(&self) -> Vec<String> {
        vec![
            format!(
                "Timer0: pending={} rem={} | USART0: tx_busy={} tx_cycles={} rx_queue={}",
                self.timer0.interrupt_pending,
                self.timer0.cycle_remainder,
                self.serial0.tx_busy_byte.is_some(),
                self.serial0.tx_cycles_remaining,
                self.serial0.rx_queue.len()
            ),
            format!(
                "ADC: pending={} rem={} | SPI CAN: {} | SPI RTD: {} | EEPROM: {} bytes",
                self.adc.interrupt_pending,
                self.adc.cycles_remaining,
                self.spi_transaction_active_can,
                self.spi_transaction_active_rtd,
                self.eeprom.len()
            ),
        ]
    }
}

trait MonitorRuntime: Send + 'static {
    type Bus: DataBus + MonitorBusInfo;

    fn title(&self) -> &'static str;
    fn cpu(&self) -> &Cpu<Self::Bus>;
    fn run_chunk(
        &mut self,
        instruction_budget: usize,
    ) -> Result<(usize, Option<RuntimeExit>), CpuError>;
    fn clear_serial_output(&mut self);
    fn serial_output_bytes(&self) -> &[u8];
}

impl MonitorRuntime for NanoRuntime {
    type Bus = Atmega328pBus<rust_mcu::atmega328p::NullNanoBoard>;

    fn title(&self) -> &'static str {
        "Nano Monitor"
    }

    fn cpu(&self) -> &Cpu<Self::Bus> {
        &self.cpu
    }

    fn run_chunk(
        &mut self,
        instruction_budget: usize,
    ) -> Result<(usize, Option<RuntimeExit>), CpuError> {
        NanoRuntime::run_chunk(self, instruction_budget, false)
    }

    fn clear_serial_output(&mut self) {
        NanoRuntime::clear_serial_output(self);
    }

    fn serial_output_bytes(&self) -> &[u8] {
        NanoRuntime::serial_output_bytes(self)
    }
}

impl MonitorRuntime for MegaRuntime {
    type Bus = Atmega2560Bus<rust_mcu::atmega2560::NullMegaBoard>;

    fn title(&self) -> &'static str {
        "Mega Monitor"
    }

    fn cpu(&self) -> &Cpu<Self::Bus> {
        &self.cpu
    }

    fn run_chunk(
        &mut self,
        instruction_budget: usize,
    ) -> Result<(usize, Option<RuntimeExit>), CpuError> {
        MegaRuntime::run_chunk(self, instruction_budget, false)
    }

    fn clear_serial_output(&mut self) {
        MegaRuntime::clear_serial_output(self);
    }

    fn serial_output_bytes(&self) -> &[u8] {
        MegaRuntime::serial_output_bytes(self)
    }
}

fn monitor_loop<R: MonitorRuntime>(runtime: R, config: MonitorConfig) -> Result<i32, CliError> {
    let _terminal = TerminalGuard::enter()?;
    let refresh = Duration::from_millis(config.refresh_ms.max(10));
    let input = spawn_input_thread();
    let initial_snapshot = capture_snapshot(&runtime, MonitorStatus::Run, None);
    let shared = Arc::new(Mutex::new(SharedMonitorState {
        sequence: 1,
        snapshot: initial_snapshot,
    }));
    let (command_tx, command_rx) = mpsc::channel();
    let shared_runtime = Arc::clone(&shared);
    let monitor_handle = thread::spawn(move || {
        monitor_runtime_loop(runtime, config, refresh, command_rx, shared_runtime);
    });

    let mut last_sequence = 0u64;
    let mut last_draw_at = Instant::now() - refresh;
    let mut last_lines: Vec<String> = Vec::new();
    let mut last_size = (0usize, 0usize);
    let mut quit_requested = false;

    loop {
        while let Ok(command) = input.try_recv() {
            if command_tx.send(command).is_err() {
                quit_requested = true;
                break;
            }
            if command == MonitorCommand::Quit {
                quit_requested = true;
            }
        }

        if last_draw_at.elapsed() >= refresh {
            let size = terminal_size().unwrap_or((100, 32));
            let state = {
                shared
                    .lock()
                    .map_err(|_| io::Error::other("monitor shared state poisoned"))?
                    .clone()
            };
            if state.sequence != last_sequence || size != last_size {
                let lines = render_screen_lines(
                    &state.snapshot.panel,
                    &state.snapshot.serial_tail,
                    size.0,
                    size.1,
                );
                if lines != last_lines {
                    draw_screen(&lines)?;
                    last_lines = lines;
                }
                last_sequence = state.sequence;
                last_size = size;
            }
            last_draw_at = Instant::now();
        }

        if quit_requested {
            break;
        }

        thread::sleep(Duration::from_millis(UI_POLL_MS));
    }

    drop(command_tx);
    monitor_handle
        .join()
        .map_err(|_| io::Error::other("monitor thread panicked"))?;
    Ok(0)
}

fn monitor_runtime_loop<R: MonitorRuntime>(
    mut runtime: R,
    config: MonitorConfig,
    refresh: Duration,
    commands: Receiver<MonitorCommand>,
    shared: Arc<Mutex<SharedMonitorState>>,
) {
    let mut status = MonitorStatus::Run;
    let mut paused = false;
    let mut remaining = config.max_instructions;
    let mut error_message: Option<String> = None;
    let mut last_serial_len = runtime.serial_output_bytes().len();
    let mut dirty = true;
    let mut force_publish = false;
    let snapshot_interval = refresh.max(Duration::from_millis(10));
    let mut last_snapshot_at = Instant::now() - snapshot_interval;

    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                MonitorCommand::TogglePause => {
                    if matches!(status, MonitorStatus::Run | MonitorStatus::Pause) {
                        paused = !paused;
                        status = if paused {
                            MonitorStatus::Pause
                        } else {
                            MonitorStatus::Run
                        };
                        dirty = true;
                        force_publish = true;
                    }
                }
                MonitorCommand::Step => {
                    if status == MonitorStatus::Pause {
                        match run_monitor_chunk(&mut runtime, 1, &mut remaining) {
                            Ok(chunk_status) => {
                                status = chunk_status.unwrap_or(MonitorStatus::Pause);
                                if status != MonitorStatus::Pause {
                                    paused = true;
                                }
                            }
                            Err(error) => {
                                error_message = Some(error.to_string());
                                status = MonitorStatus::Error;
                                paused = true;
                            }
                        }
                        dirty = true;
                        force_publish = true;
                    }
                }
                MonitorCommand::ClearSerial => {
                    runtime.clear_serial_output();
                    last_serial_len = 0;
                    dirty = true;
                    force_publish = true;
                }
                MonitorCommand::Quit => {
                    publish_snapshot(&runtime, status, error_message.clone(), &shared);
                    return;
                }
            }
        }

        if !paused && status == MonitorStatus::Run {
            let budget = remaining
                .map(|value| value.min(config.chunk_size.max(1)))
                .unwrap_or(config.chunk_size.max(1));
            if budget == 0 {
                status = MonitorStatus::Done;
                paused = true;
                dirty = true;
                force_publish = true;
            } else {
                match run_monitor_chunk(&mut runtime, budget, &mut remaining) {
                    Ok(chunk_status) => {
                        if let Some(next_status) = chunk_status {
                            status = next_status;
                            if status != MonitorStatus::Run {
                                paused = true;
                            }
                            dirty = true;
                            force_publish = true;
                        }
                    }
                    Err(error) => {
                        error_message = Some(error.to_string());
                        status = MonitorStatus::Error;
                        paused = true;
                        dirty = true;
                        force_publish = true;
                    }
                }
            }
        } else {
            thread::sleep(Duration::from_millis(IDLE_POLL_MS));
        }

        let serial_len = runtime.serial_output_bytes().len();
        if serial_len != last_serial_len {
            last_serial_len = serial_len;
            dirty = true;
        }

        if dirty && (force_publish || last_snapshot_at.elapsed() >= snapshot_interval) {
            publish_snapshot(&runtime, status, error_message.clone(), &shared);
            last_snapshot_at = Instant::now();
            dirty = false;
            force_publish = false;
        }
    }
}

fn run_monitor_chunk<R: MonitorRuntime>(
    runtime: &mut R,
    budget: usize,
    remaining: &mut Option<usize>,
) -> Result<Option<MonitorStatus>, CpuError> {
    let (executed, exit) = runtime.run_chunk(budget)?;
    if let Some(limit) = remaining.as_mut() {
        *limit = limit.saturating_sub(executed);
        if *limit == 0 {
            return Ok(Some(MonitorStatus::Done));
        }
    }

    Ok(match exit {
        Some(RuntimeExit::BreakHit) => Some(MonitorStatus::Break),
        Some(RuntimeExit::Sleeping) => Some(MonitorStatus::Sleep),
        Some(RuntimeExit::MaxInstructionsReached) | None => None,
        Some(RuntimeExit::UntilSerialSatisfied) => Some(MonitorStatus::Done),
    })
}

fn capture_snapshot<R: MonitorRuntime>(
    runtime: &R,
    status: MonitorStatus,
    error: Option<String>,
) -> MonitorSnapshot {
    let serial_bytes = runtime.serial_output_bytes();
    MonitorSnapshot {
        panel: build_cpu_panel(
            runtime.title(),
            status,
            runtime.cpu(),
            serial_bytes.len(),
            error,
        ),
        serial_tail: serial_tail(serial_bytes),
    }
}

fn publish_snapshot<R: MonitorRuntime>(
    runtime: &R,
    status: MonitorStatus,
    error: Option<String>,
    shared: &Arc<Mutex<SharedMonitorState>>,
) {
    let snapshot = capture_snapshot(runtime, status, error);
    if let Ok(mut state) = shared.lock() {
        state.sequence = state.sequence.wrapping_add(1);
        state.snapshot = snapshot;
    }
}

fn serial_tail(serial_bytes: &[u8]) -> Vec<u8> {
    let start = serial_bytes.len().saturating_sub(SERIAL_TAIL_BYTES);
    serial_bytes[start..].to_vec()
}

fn build_cpu_panel<B: DataBus + MonitorBusInfo>(
    title: &str,
    status: MonitorStatus,
    cpu: &Cpu<B>,
    serial_bytes: usize,
    error: Option<String>,
) -> CpuPanelState {
    let mut registers = [0u8; 32];
    registers.copy_from_slice(&cpu.data[0..32]);
    let next_instruction = match cpu.decode_at(cpu.pc) {
        Ok(instruction) => format_instruction(&instruction),
        Err(err) => format!("<decode error: {err}>"),
    };

    CpuPanelState {
        title: title.to_string(),
        status,
        pc: cpu.pc,
        sp: cpu.sp(),
        cycles: cpu.cycles,
        synced_cycles: cpu.bus.synced_cycles(),
        serial_bytes,
        sreg: cpu.data[cpu.config.sreg_address],
        registers,
        next_instruction,
        extra_lines: cpu.bus.extra_status_lines(),
        error,
    }
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

fn render_screen_lines(
    panel: &CpuPanelState,
    serial_bytes: &[u8],
    width: usize,
    height: usize,
) -> Vec<String> {
    let width = width.max(60);
    let height = height.max(16);
    let cpu_height = ((height as f32) * 0.55) as usize;
    let serial_height = height.saturating_sub(cpu_height + 1);
    let mut lines = Vec::new();
    lines.extend(format_cpu_panel(panel, width, cpu_height.max(8)));
    lines.push("-".repeat(width));
    lines.extend(format_serial_panel(
        serial_bytes,
        width,
        serial_height.max(3),
    ));
    while lines.len() < height {
        lines.push(" ".repeat(width));
    }
    lines.truncate(height);
    lines
}

fn draw_screen(lines: &[String]) -> io::Result<()> {
    let mut stdout = io::stdout();
    stdout.write_all(b"\x1b[H")?;
    for (index, line) in lines.iter().enumerate() {
        write!(stdout, "\x1b[{};1H{}", index + 1, line)?;
    }
    stdout.flush()
}

fn format_cpu_panel(panel: &CpuPanelState, width: usize, height: usize) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(fit_line(
        &format!("{} | {}", panel.title, panel.status.label()),
        width,
    ));
    let pause_label = if panel.status == MonitorStatus::Run {
        "Pause"
    } else {
        "Resume"
    };
    lines.push(fit_line(
        &format!("[{pause_label}: Space] [Step: S] [Clear Serial: C] [Quit: Q]"),
        width,
    ));
    lines.push(fit_line(
        &format!(
            "PC=0x{:06X} SP=0x{:04X} Cycles={} Sync={} Serial={}B",
            panel.pc, panel.sp, panel.cycles, panel.synced_cycles, panel.serial_bytes
        ),
        width,
    ));
    lines.push(fit_line(
        &format!("Next: {}", panel.next_instruction),
        width,
    ));
    lines.push(fit_line(
        &format!("SREG: {}", format_sreg(panel.sreg)),
        width,
    ));
    for row in 0..4 {
        let start = row * 8;
        let mut line = String::new();
        for index in start..(start + 8) {
            let _ = write!(line, "R{index:02}=0x{:02X} ", panel.registers[index]);
        }
        lines.push(fit_line(line.trim_end(), width));
    }
    for extra in &panel.extra_lines {
        lines.push(fit_line(extra, width));
    }
    if let Some(error) = &panel.error {
        lines.push(fit_line(&format!("Error: {error}"), width));
    }
    while lines.len() < height {
        lines.push(" ".repeat(width));
    }
    lines.truncate(height);
    lines
}

fn format_serial_panel(serial_bytes: &[u8], width: usize, height: usize) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(fit_line(
        &format!("Serial Output | {} bytes", serial_bytes.len()),
        width,
    ));
    if height <= 1 {
        return lines;
    }

    let text = String::from_utf8_lossy(serial_bytes);
    let normalized = text.replace('\r', "");
    let mut payload_lines: Vec<String> = normalized
        .lines()
        .map(|line| fit_line(line, width))
        .collect();
    if payload_lines.is_empty() {
        payload_lines.push(fit_line("<no serial output yet>", width));
    }
    let visible = height - 1;
    if payload_lines.len() > visible {
        payload_lines = payload_lines[payload_lines.len() - visible..].to_vec();
    }
    lines.extend(payload_lines);
    while lines.len() < height {
        lines.push(" ".repeat(width));
    }
    lines.truncate(height);
    lines
}

fn format_sreg(sreg: u8) -> String {
    let flags = [
        ('I', (sreg >> 7) & 1),
        ('T', (sreg >> 6) & 1),
        ('H', (sreg >> 5) & 1),
        ('S', (sreg >> 4) & 1),
        ('V', (sreg >> 3) & 1),
        ('N', (sreg >> 2) & 1),
        ('Z', (sreg >> 1) & 1),
        ('C', sreg & 1),
    ];
    flags
        .iter()
        .map(|(name, value)| format!("{name}{value}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn fit_line(input: &str, width: usize) -> String {
    let mut line = String::new();
    for character in input.chars().take(width) {
        line.push(character);
    }
    if line.len() < width {
        line.push_str(&" ".repeat(width - line.len()));
    }
    line
}

fn spawn_input_thread() -> Receiver<MonitorCommand> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let Ok(mut tty) = File::open("/dev/tty") else {
            return;
        };
        let mut buffer = [0u8; 1];
        loop {
            match tty.read(&mut buffer) {
                Ok(1) => {
                    let command = match buffer[0] {
                        b' ' => Some(MonitorCommand::TogglePause),
                        b's' | b'S' => Some(MonitorCommand::Step),
                        b'c' | b'C' => Some(MonitorCommand::ClearSerial),
                        b'q' | b'Q' | 0x03 => Some(MonitorCommand::Quit),
                        _ => None,
                    };
                    if let Some(command) = command {
                        if tx.send(command).is_err() {
                            break;
                        }
                    }
                }
                Ok(0) => thread::sleep(Duration::from_millis(IDLE_POLL_MS)),
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    rx
}

struct TerminalGuard {
    saved_mode: Option<String>,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        let saved_mode = stty_query(&["-g"]).ok();
        stty_apply(&["raw", "-echo"])?;
        let mut stdout = io::stdout();
        stdout.write_all(b"\x1b[?1049h\x1b[?25l\x1b[2J\x1b[H")?;
        stdout.flush()?;
        Ok(Self { saved_mode })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if let Some(saved_mode) = &self.saved_mode {
            let _ = stty_apply(&[saved_mode.as_str()]);
        } else {
            let _ = stty_apply(&["sane"]);
        }
        let mut stdout = io::stdout();
        let _ = stdout.write_all(b"\x1b[?25h\x1b[?1049l");
        let _ = stdout.flush();
    }
}

fn terminal_size() -> io::Result<(usize, usize)> {
    let output = stty_query(&["size"])?;
    let mut parts = output.split_whitespace();
    let rows = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid terminal rows"))?;
    let cols = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid terminal cols"))?;
    Ok((cols.max(60), rows.max(16)))
}

fn stty_apply(args: &[&str]) -> io::Result<()> {
    run_stty(args).map(|_| ())
}

fn stty_query(args: &[&str]) -> io::Result<String> {
    let output = run_stty(args)?;
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_string())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn run_stty(args: &[&str]) -> io::Result<std::process::Output> {
    let tty = File::open("/dev/tty")?;
    let output = Command::new("stty")
        .args(args)
        .stdin(Stdio::from(tty))
        .output()?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "stty failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        format_cpu_panel, format_serial_panel, render_screen_lines, CpuPanelState, MonitorStatus,
    };

    fn sample_panel(status: MonitorStatus) -> CpuPanelState {
        let mut registers = [0u8; 32];
        for (index, slot) in registers.iter_mut().enumerate() {
            *slot = index as u8;
        }
        CpuPanelState {
            title: "Nano Monitor".to_string(),
            status,
            pc: 0x1234,
            sp: 0x21FF,
            cycles: 42,
            synced_cycles: 40,
            serial_bytes: 5,
            sreg: 0b1000_0011,
            registers,
            next_instruction: "0x001234: Nop (opcode=0x0000)".to_string(),
            extra_lines: vec!["Timer0: pending=false rem=0".to_string()],
            error: None,
        }
    }

    #[test]
    fn cpu_panel_shows_running_controls() {
        let lines = format_cpu_panel(&sample_panel(MonitorStatus::Run), 80, 10);
        assert_eq!(lines[0].trim_end(), "Nano Monitor | RUN");
        assert!(lines[1].contains("[Pause: Space]"));
        assert!(lines[1].contains("[Step: S]"));
        assert!(lines[1].contains("[Quit: Q]"));
    }

    #[test]
    fn cpu_panel_shows_resume_when_paused() {
        let lines = format_cpu_panel(&sample_panel(MonitorStatus::Pause), 80, 10);
        assert_eq!(lines[0].trim_end(), "Nano Monitor | PAUSE");
        assert!(lines[1].contains("[Resume: Space]"));
    }

    #[test]
    fn serial_panel_uses_placeholder_when_empty() {
        let lines = format_serial_panel(&[], 80, 4);
        assert_eq!(lines[1].trim_end(), "<no serial output yet>");
    }

    #[test]
    fn render_screen_lines_fill_requested_geometry_without_newline_tricks() {
        let lines = render_screen_lines(&sample_panel(MonitorStatus::Run), b"hello", 72, 18);
        assert_eq!(lines.len(), 18);
        assert!(lines.iter().all(|line| line.len() == 72));
        assert_eq!(lines[0].trim_end(), "Nano Monitor | RUN");
    }
}
