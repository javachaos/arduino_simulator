use std::collections::BTreeSet;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use eframe::egui::{
    self, CentralPanel, Color32, ComboBox, Context, Frame, Grid, RichText, ScrollArea, SidePanel,
    TextEdit, TopBottomPanel,
};
use rust_mcu::{BoardPin, BoardPinLevel};

use crate::board_view;
use crate::example_firmware::{self, ExampleFirmware};
use crate::firmware::{decode_hex_bytes, HexLoadError};
use crate::pcb_view::LoadedPcb;
use crate::runtime::{CpuSnapshot, RuntimeExit, SimulationRuntime, SimulationTarget};

const RUN_CHUNK_SIZE: usize = 20_000;

pub struct RustWebApp {
    target: SimulationTarget,
    runtime: SimulationRuntime,
    firmware_name: Option<String>,
    firmware_hex: Option<String>,
    serial_text: String,
    status_text: String,
    last_exit: Option<RuntimeExit>,
    running: bool,
    mega_board_preview: Option<LoadedPcb>,
    nano_board_preview: Option<LoadedPcb>,
    pending_file_rx: Option<Receiver<Result<Option<LoadedFile>, String>>>,
}

struct LoadedFile {
    name: String,
    bytes: Vec<u8>,
}

#[derive(Clone, Copy)]
enum WebStatus {
    Idle,
    Ready,
    Running,
    Break,
    Sleep,
    Error,
}

impl WebStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Ready => "Ready",
            Self::Running => "Running",
            Self::Break => "Break",
            Self::Sleep => "Sleep",
            Self::Error => "Error",
        }
    }

    fn color(self) -> Color32 {
        match self {
            Self::Idle => Color32::from_rgb(145, 168, 188),
            Self::Ready => Color32::from_rgb(120, 204, 160),
            Self::Running => Color32::from_rgb(112, 196, 255),
            Self::Break => Color32::from_rgb(255, 188, 92),
            Self::Sleep => Color32::from_rgb(173, 149, 255),
            Self::Error => Color32::from_rgb(255, 129, 129),
        }
    }
}

impl Default for RustWebApp {
    fn default() -> Self {
        Self {
            target: SimulationTarget::default(),
            runtime: SimulationRuntime::new(SimulationTarget::default()),
            firmware_name: None,
            firmware_hex: None,
            serial_text: String::new(),
            status_text:
                "Drop a firmware.hex file, open one from the toolbar, or load a built-in example."
                    .to_owned(),
            last_exit: None,
            running: false,
            mega_board_preview: None,
            nano_board_preview: None,
            pending_file_rx: None,
        }
    }
}

impl RustWebApp {
    fn set_target(&mut self, new_target: SimulationTarget) {
        if new_target == self.runtime.target() {
            self.target = new_target;
            return;
        }

        self.target = new_target;
        self.runtime = SimulationRuntime::new(new_target);
        self.running = false;
        self.last_exit = None;
        self.serial_text.clear();

        if let Some(hex) = self.firmware_hex.as_deref() {
            match self.runtime.load_hex(hex) {
                Ok(()) => {
                    self.status_text = format!("Reloaded firmware for {}.", self.target.label());
                }
                Err(error) => {
                    self.status_text = format!(
                        "Failed to reload {} for {}: {error}",
                        self.firmware_name
                            .as_deref()
                            .unwrap_or("firmware.hex"),
                        self.target.label()
                    );
                }
            }
        } else {
            self.status_text = format!("Target changed to {}.", self.target.label());
        }
    }

    fn reset_runtime(&mut self) {
        self.runtime.reset();
        self.running = false;
        self.last_exit = None;
        self.serial_text.clear();
        self.status_text = format!("Reset {}.", self.target.label());
    }

    fn clear_serial(&mut self) {
        self.runtime.clear_serial_output();
        self.serial_text.clear();
        self.status_text = "Cleared serial output.".to_owned();
    }

    fn load_hex_text(&mut self, name: String, hex: String) {
        self.runtime = SimulationRuntime::new(self.target);
        self.running = false;
        self.last_exit = None;
        self.serial_text.clear();

        match self.runtime.load_hex(&hex) {
            Ok(()) => {
                self.firmware_name = Some(name.clone());
                self.firmware_hex = Some(hex);
                self.status_text =
                    format!("Loaded {name} for {}.", self.runtime.target().label());
            }
            Err(error) => self.status_text = format!("Failed to load {name}: {error}"),
        }
    }

    fn load_hex_file(&mut self, file: LoadedFile) {
        match decode_hex_bytes(&file.bytes) {
            Ok(hex) => self.load_hex_text(file.name, hex),
            Err(HexLoadError::InvalidEncoding) => {
                self.status_text = format!("{} is not valid Intel HEX text.", file.name);
            }
            Err(error) => {
                self.status_text = format!("Failed to decode {}: {error}", file.name);
            }
        }
    }

    fn load_builtin_example(&mut self, example: ExampleFirmware) {
        self.target = example.target;
        self.load_hex_text(example.file_name.to_owned(), example.hex.to_owned());
        self.status_text = format!(
            "Loaded example {} for {}.",
            example.label,
            example.target.label()
        );
    }

    fn has_firmware(&self) -> bool {
        self.firmware_hex.is_some()
    }

    fn current_status(&self) -> WebStatus {
        if status_text_is_error(&self.status_text) {
            return WebStatus::Error;
        }
        if self.running {
            return WebStatus::Running;
        }
        match self.last_exit {
            Some(RuntimeExit::BreakHit) => WebStatus::Break,
            Some(RuntimeExit::Sleeping) => WebStatus::Sleep,
            Some(RuntimeExit::MaxInstructionsReached) | None => {
                if self.has_firmware() {
                    WebStatus::Ready
                } else {
                    WebStatus::Idle
                }
            }
        }
    }

    fn step_once(&mut self) {
        if !self.has_firmware() {
            self.status_text = "Load a firmware.hex file first.".to_owned();
            return;
        }
        self.running = false;
        self.last_exit = None;

        match self.runtime.step_once() {
            Ok(exit) => {
                self.append_new_serial_output();
                self.handle_exit(exit, false);
            }
            Err(error) => {
                self.status_text = format!("Execution failed: {error}");
            }
        }
    }

    fn append_new_serial_output(&mut self) {
        let new_bytes = self.runtime.take_new_serial_bytes();
        if !new_bytes.is_empty() {
            self.serial_text
                .push_str(String::from_utf8_lossy(&new_bytes).as_ref());
        }
    }

    fn handle_exit(&mut self, exit: RuntimeExit, from_run_loop: bool) {
        self.last_exit = Some(exit);
        match exit {
            RuntimeExit::MaxInstructionsReached if from_run_loop => {}
            RuntimeExit::MaxInstructionsReached => {
                self.status_text = "Executed one instruction.".to_owned();
            }
            RuntimeExit::BreakHit => {
                self.running = false;
                self.status_text = "Execution stopped on BREAK.".to_owned();
            }
            RuntimeExit::Sleeping => {
                self.running = false;
                self.status_text = "Execution stopped because the CPU is sleeping.".to_owned();
            }
        }
    }

    fn run_frame(&mut self) {
        if !self.running {
            return;
        }

        match self.runtime.run_chunk(RUN_CHUNK_SIZE) {
            Ok(exit) => {
                self.append_new_serial_output();
                self.handle_exit(exit, true);
            }
            Err(error) => {
                self.running = false;
                self.status_text = format!("Execution failed: {error}");
            }
        }
    }

    fn poll_async_file_dialog(&mut self) {
        let Some(receiver) = self.pending_file_rx.as_ref() else {
            return;
        };

        match receiver.try_recv() {
            Ok(Ok(Some(file))) => {
                self.pending_file_rx = None;
                self.load_hex_file(file);
            }
            Ok(Ok(None)) => {
                self.pending_file_rx = None;
            }
            Ok(Err(error)) => {
                self.pending_file_rx = None;
                self.status_text = error;
            }
            Err(TryRecvError::Disconnected) => {
                self.pending_file_rx = None;
                self.status_text = "File dialog closed unexpectedly.".to_owned();
            }
            Err(TryRecvError::Empty) => {}
        }
    }

    fn handle_dropped_files(&mut self, ctx: &Context) {
        let dropped_files = ctx.input(|input| input.raw.dropped_files.clone());
        if dropped_files.is_empty() {
            return;
        }

        if let Some(file) = dropped_files.into_iter().last() {
            if let Some(bytes) = file.bytes {
                self.load_hex_file(LoadedFile {
                    name: file.name,
                    bytes: bytes.to_vec(),
                });
                return;
            }

            #[cfg(not(target_arch = "wasm32"))]
            if let Some(path) = file.path {
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        self.load_hex_file(LoadedFile {
                            name: path
                                .file_name()
                                .and_then(|value| value.to_str())
                                .unwrap_or("firmware.hex")
                                .to_owned(),
                            bytes,
                        });
                    }
                    Err(error) => {
                        self.status_text =
                            format!("Failed to read dropped file {}: {error}", path.display());
                    }
                }
            }
        }
    }

    fn open_firmware_dialog(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Intel HEX", &["hex"])
                .pick_file()
            {
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        self.load_hex_file(LoadedFile {
                            name: path
                                .file_name()
                                .and_then(|value| value.to_str())
                                .unwrap_or("firmware.hex")
                                .to_owned(),
                            bytes,
                        });
                    }
                    Err(error) => {
                        self.status_text = format!("Failed to read {}: {error}", path.display());
                    }
                }
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            self.pending_file_rx = Some(rx);
            self.status_text = "Waiting for firmware.hex...".to_owned();
            wasm_bindgen_futures::spawn_local(async move {
                let result = match rfd::AsyncFileDialog::new()
                    .add_filter("Intel HEX", &["hex"])
                    .pick_file()
                    .await
                {
                    Some(handle) => Ok(Some(LoadedFile {
                        name: handle.file_name(),
                        bytes: handle.read().await,
                    })),
                    None => Ok(None),
                };
                let _ = tx.send(result);
            });
        }
    }

    fn render_toolbar(&mut self, ctx: &Context) {
        TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.heading("arduino_simulator");
                ui.label(
                    "Load a firmware.hex file, choose a target Arduino, and watch board activity and serial output live.",
                );
                ui.add_space(4.0);

                ui.horizontal_wrapped(|ui| {
                    if ui.button("Open firmware.hex").clicked() {
                        self.open_firmware_dialog();
                    }

                    let mut example_to_load = None;
                    ui.menu_button("Examples", |ui| {
                        for example in example_firmware::ALL {
                            if ui.button(example.label).clicked() {
                                example_to_load = Some(example);
                                ui.close_menu();
                            }
                        }
                    });

                    if let Some(example) = example_to_load {
                        self.load_builtin_example(example);
                    }

                    if let Some(name) = self.firmware_name.as_deref() {
                        ui.separator();
                        ui.label(format!("Loaded: {name}"));
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    let mut selected_target = self.target;
                    ui.label("Target:");
                    ComboBox::from_id_salt("board_target")
                        .selected_text(selected_target.label())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut selected_target,
                                SimulationTarget::Nano,
                                SimulationTarget::Nano.label(),
                            );
                            ui.selectable_value(
                                &mut selected_target,
                                SimulationTarget::Mega,
                                SimulationTarget::Mega.label(),
                            );
                        });
                    if selected_target != self.target {
                        self.set_target(selected_target);
                    }

                    ui.label("Source:");
                    let mut firmware_text = self.firmware_name.clone().unwrap_or_default();
                    ui.add_sized(
                        [420.0, 24.0],
                        TextEdit::singleline(&mut firmware_text)
                            .hint_text("Select a .hex image or built-in example")
                            .interactive(false),
                    );
                    if ui.button("Load HEX").clicked() {
                        self.open_firmware_dialog();
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    let controls_enabled = self.has_firmware();
                    if ui
                        .add_enabled(!self.running && controls_enabled, egui::Button::new("Run"))
                        .clicked()
                    {
                        self.running = true;
                        self.last_exit = None;
                        self.status_text = format!("Running {}...", self.target.label());
                    }
                    if ui
                        .add_enabled(self.running, egui::Button::new("Pause"))
                        .clicked()
                    {
                        self.running = false;
                        self.status_text = "Execution paused.".to_owned();
                    }
                    if ui
                        .add_enabled(controls_enabled, egui::Button::new("Step"))
                        .clicked()
                    {
                        self.step_once();
                    }
                    if ui
                        .add_enabled(controls_enabled, egui::Button::new("Reset"))
                        .clicked()
                    {
                        self.reset_runtime();
                    }
                    if ui
                        .add_enabled(controls_enabled, egui::Button::new("Clear Serial"))
                        .clicked()
                    {
                        self.clear_serial();
                    }

                    ui.separator();
                    ui.label(format!("PC: 0x{:06X}", self.runtime.pc()));
                    ui.separator();
                    ui.label(format!("Cycles: {}", self.runtime.cycles()));
                    ui.separator();
                    ui.label(format!("Serial: {} baud", self.runtime.configured_serial_baud()));
                });

                ui.horizontal_wrapped(|ui| {
                    let status = self.current_status();
                    ui.label("Status:");
                    ui.colored_label(status.color(), status.label());
                    ui.separator();
                    ui.label(&self.status_text);
                    if let Some(exit) = self.last_exit {
                        if !self.running {
                            ui.separator();
                            ui.label(format!("Last stop: {}", exit_label(exit)));
                        }
                    }
                });
            });
        });
    }

    fn render_cpu_panel(&mut self, ctx: &Context, snapshot: &CpuSnapshot) {
        SidePanel::left("cpu_panel")
            .resizable(true)
            .default_width(360.0)
            .show(ctx, |ui| {
                ui.heading("CPU");
                ui.add_space(4.0);

                Grid::new("cpu_summary")
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        summary_row(ui, "Target", self.target.label());
                        summary_row(ui, "Runtime", snapshot.target.label());
                        summary_row(ui, "PC", &format!("0x{:06X}", snapshot.pc));
                        summary_row(ui, "SP", &format!("0x{:04X}", snapshot.sp));
                        summary_row(ui, "Cycles", &snapshot.cycles.to_string());
                        summary_row(ui, "Synced", &snapshot.synced_cycles.to_string());
                        summary_row(ui, "Serial", &format!("{} bytes", snapshot.serial_bytes));
                        summary_row(ui, "RX queued", &snapshot.serial_rx_queued.to_string());
                    });

                ui.label(RichText::new("Next Instruction").strong());
                code_block(ui, &snapshot.next_instruction);

                ui.add_space(8.0);
                ui.label(RichText::new("SREG").strong());
                code_block(ui, &format_sreg(snapshot.sreg));

                if !snapshot.extra_lines.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Peripherals").strong());
                    for line in &snapshot.extra_lines {
                        code_block(ui, line);
                    }
                }

                ui.add_space(8.0);
                ui.label(RichText::new("Registers").strong());
                ScrollArea::vertical()
                    .id_salt("cpu_registers_scroll")
                    .max_height(340.0)
                    .show(ui, |ui| {
                        Grid::new("register_grid")
                            .num_columns(4)
                            .spacing([12.0, 2.0])
                            .show(ui, |ui| {
                                for row in 0..8 {
                                    for column in 0..4 {
                                        let index = row * 4 + column;
                                        ui.monospace(format!(
                                            "R{index:02} = 0x{:02X}",
                                            snapshot.registers[index]
                                        ));
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            });
    }

    fn draw_host_board_card(
        &mut self,
        ui: &mut egui::Ui,
        board: SimulationTarget,
        host_pin_levels: &[BoardPinLevel],
    ) {
        Frame::group(ui.style()).show(ui, |ui| {
            ui.label(RichText::new("Current Board").strong());
            ui.small(format!("Loaded runtime board: {}", board.label()));
            ui.add_space(6.0);
            let active_nets = active_preview_nets(board, host_pin_levels);
            if let Some(preview) = self.board_preview(board) {
                crate::pcb_view::render_pcb_preview(ui, preview, &active_nets);
                let active = board_view::active_pin_count(host_pin_levels);
                if active > 0 {
                    ui.small(format!("Active host nets highlighted: {active}"));
                } else {
                    ui.small("Host nets will highlight here as board activity appears.");
                }
            } else {
                board_view::show_board_preview(ui, board, host_pin_levels);
                ui.small("KiCad board preview unavailable; showing simplified board card.");
            }
        });
    }

    fn board_preview(&mut self, board: SimulationTarget) -> Option<&LoadedPcb> {
        let slot = match board {
            SimulationTarget::Mega => &mut self.mega_board_preview,
            SimulationTarget::Nano => &mut self.nano_board_preview,
        };
        if slot.is_none() {
            *slot = load_host_board_preview(board);
        }
        slot.as_ref()
    }

    fn render_serial_panel(
        &mut self,
        ctx: &Context,
        snapshot: &CpuSnapshot,
        host_pin_levels: &[BoardPinLevel],
    ) {
        CentralPanel::default().show(ctx, |ui| {
            self.draw_host_board_card(ui, snapshot.target, host_pin_levels);
            ui.add_space(10.0);

            ui.heading("Serial Console");
            ui.add_space(4.0);
            Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Configured Baud:");
                    ui.monospace(self.runtime.configured_serial_baud().to_string());
                    ui.separator();
                    ui.label(format!("Captured: {} bytes", snapshot.serial_bytes));
                    ui.separator();
                    ui.label(format!("RX queued: {}", snapshot.serial_rx_queued));
                    if let Some(exit) = self.last_exit {
                        ui.separator();
                        ui.label(format!("Last stop: {}", exit_label(exit)));
                    }
                    if let Some(name) = self.firmware_name.as_deref() {
                        ui.separator();
                        ui.label(format!("Firmware: {name}"));
                    }
                    ui.separator();
                    ui.label(format!("Target: {}", self.runtime.target().label()));
                    if ui.button("Clear Serial").clicked() {
                        self.clear_serial();
                    }
                });
                ui.add_space(6.0);

                if self.serial_text.is_empty() {
                    ui.small("Run a firmware image to see UART output here.");
                } else {
                    ScrollArea::vertical()
                        .id_salt("serial_console_scroll")
                        .stick_to_bottom(true)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add_sized(
                                [ui.available_width(), ui.available_height().max(220.0)],
                                TextEdit::multiline(&mut self.serial_text)
                                    .code_editor()
                                    .desired_rows(18)
                                    .interactive(false)
                                    .lock_focus(true)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                }
            });
        });
    }
}

impl eframe::App for RustWebApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        configure_visuals(ctx);
        self.poll_async_file_dialog();
        self.handle_dropped_files(ctx);
        self.run_frame();

        if self.running || self.pending_file_rx.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        self.render_toolbar(ctx);
        let host_pin_levels = self.runtime.host_pin_levels();
        let snapshot = self.runtime.cpu_snapshot();
        self.render_cpu_panel(ctx, &snapshot);
        self.render_serial_panel(ctx, &snapshot, &host_pin_levels);
    }
}

fn load_host_board_preview(board: SimulationTarget) -> Option<LoadedPcb> {
    match board {
        SimulationTarget::Mega => LoadedPcb::from_embedded_kicad(
            "arduino_mega_2560_rev3e",
            "embedded://arduino_mega_2560_rev3e.kicad_pcb",
            include_str!("../../examples/pcbs/arduino_mega_2560_rev3e.kicad_pcb"),
        )
        .ok()
        .map(LoadedPcb::simplified_preview),
        SimulationTarget::Nano => LoadedPcb::from_embedded_kicad(
            "arduino_nano_v3_3",
            "embedded://arduino_nano_v3_3.kicad_pcb",
            include_str!("../../examples/pcbs/arduino_nano_v3_3.kicad_pcb"),
        )
        .ok()
        .map(LoadedPcb::simplified_preview),
    }
}

fn active_preview_nets(target: SimulationTarget, host_pin_levels: &[BoardPinLevel]) -> BTreeSet<String> {
    host_pin_levels
        .iter()
        .filter(|entry| entry.level != 0)
        .flat_map(|entry| preview_net_aliases(target, entry.pin))
        .collect()
}

fn preview_net_aliases(target: SimulationTarget, pin: BoardPin) -> Vec<String> {
    match (target, pin) {
        (SimulationTarget::Nano, BoardPin::Analog(index)) => vec![format!("A{index}")],
        (SimulationTarget::Mega, BoardPin::Analog(index)) => {
            vec![format!("A{index}"), format!("ADC{index}")]
        }
        (SimulationTarget::Nano, BoardPin::Digital(0)) => vec!["RX".to_owned(), "D0".to_owned()],
        (SimulationTarget::Nano, BoardPin::Digital(1)) => vec!["TX".to_owned(), "D1".to_owned()],
        (SimulationTarget::Nano, BoardPin::Digital(11)) => {
            vec!["D11".to_owned(), "MOSI".to_owned()]
        }
        (SimulationTarget::Nano, BoardPin::Digital(12)) => {
            vec!["D12".to_owned(), "MISO".to_owned()]
        }
        (SimulationTarget::Nano, BoardPin::Digital(13)) => {
            vec!["D13".to_owned(), "SCK".to_owned()]
        }
        (SimulationTarget::Nano, BoardPin::Digital(index)) => vec![format!("D{index}")],
        (SimulationTarget::Mega, BoardPin::Digital(index)) => mega_preview_pin_aliases(index),
    }
}

fn mega_preview_pin_aliases(index: u8) -> Vec<String> {
    match index {
        0 => vec![
            "D0".to_owned(),
            "RX".to_owned(),
            "RX0".to_owned(),
            "RXL".to_owned(),
            "M8RXD".to_owned(),
            "PE0".to_owned(),
        ],
        1 => vec![
            "D1".to_owned(),
            "TX".to_owned(),
            "TX0".to_owned(),
            "TXL".to_owned(),
            "M8TXD".to_owned(),
            "PE1".to_owned(),
        ],
        2 => vec!["D2".to_owned(), "PE4".to_owned()],
        3 => vec!["D3".to_owned(), "PE5".to_owned()],
        4 => vec!["D4".to_owned(), "PG5".to_owned()],
        5 => vec!["D5".to_owned(), "PE3".to_owned()],
        6 => vec!["D6".to_owned(), "PH3".to_owned()],
        7 => vec!["D7".to_owned(), "PH4".to_owned()],
        8 => vec!["D8".to_owned(), "PH5".to_owned()],
        9 => vec!["D9".to_owned(), "PH6".to_owned()],
        10 => vec!["D10".to_owned(), "PB4".to_owned()],
        11 => vec!["D11".to_owned(), "PB5".to_owned()],
        12 => vec!["D12".to_owned(), "PB6".to_owned()],
        13 => vec!["D13".to_owned(), "PB7".to_owned()],
        14 => vec!["D14".to_owned(), "TX3".to_owned(), "TXD3".to_owned()],
        15 => vec!["D15".to_owned(), "RX3".to_owned(), "RXD3".to_owned()],
        16 => vec!["D16".to_owned(), "TX2".to_owned(), "TXD2".to_owned()],
        17 => vec!["D17".to_owned(), "RX2".to_owned(), "RXD2".to_owned()],
        18 => vec!["D18".to_owned(), "TX1".to_owned(), "TXD1".to_owned()],
        19 => vec!["D19".to_owned(), "RX1".to_owned(), "RXD1".to_owned()],
        20 => vec!["D20".to_owned(), "SDA".to_owned()],
        21 => vec!["D21".to_owned(), "SCL".to_owned()],
        22 => vec!["D22".to_owned(), "PA0".to_owned()],
        23 => vec!["D23".to_owned(), "PA1".to_owned()],
        24 => vec!["D24".to_owned(), "PA2".to_owned()],
        25 => vec!["D25".to_owned(), "PA3".to_owned()],
        26 => vec!["D26".to_owned(), "PA4".to_owned()],
        27 => vec!["D27".to_owned(), "PA5".to_owned()],
        28 => vec!["D28".to_owned(), "PA6".to_owned()],
        29 => vec!["D29".to_owned(), "PA7".to_owned()],
        30 => vec!["D30".to_owned(), "PC7".to_owned()],
        31 => vec!["D31".to_owned(), "PC6".to_owned()],
        32 => vec!["D32".to_owned(), "PC5".to_owned()],
        33 => vec!["D33".to_owned(), "PC4".to_owned()],
        34 => vec!["D34".to_owned(), "PC3".to_owned()],
        35 => vec!["D35".to_owned(), "PC2".to_owned()],
        36 => vec!["D36".to_owned(), "PC1".to_owned()],
        37 => vec!["D37".to_owned(), "PC0".to_owned()],
        38 => vec!["D38".to_owned(), "PD7".to_owned()],
        39 => vec!["D39".to_owned(), "PG2".to_owned()],
        40 => vec!["D40".to_owned(), "PG1".to_owned()],
        41 => vec!["D41".to_owned(), "PG0".to_owned()],
        42 => vec!["D42".to_owned(), "PL7".to_owned()],
        43 => vec!["D43".to_owned(), "PL6".to_owned()],
        44 => vec!["D44".to_owned(), "PL5".to_owned()],
        45 => vec!["D45".to_owned(), "PL4".to_owned()],
        46 => vec!["D46".to_owned(), "PL3".to_owned()],
        47 => vec!["D47".to_owned(), "PL2".to_owned()],
        48 => vec!["D48".to_owned(), "PL1".to_owned()],
        49 => vec!["D49".to_owned(), "PL0".to_owned()],
        50 => vec![
            "D50".to_owned(),
            "PB3".to_owned(),
            "MISO".to_owned(),
            "MISO2".to_owned(),
        ],
        51 => vec![
            "D51".to_owned(),
            "PB2".to_owned(),
            "MOSI".to_owned(),
            "MOSI2".to_owned(),
        ],
        52 => vec![
            "D52".to_owned(),
            "PB1".to_owned(),
            "SCK".to_owned(),
            "SCK2".to_owned(),
        ],
        53 => vec!["D53".to_owned(), "PB0".to_owned()],
        _ => vec![format!("D{index}")],
    }
}

fn configure_visuals(ctx: &Context) {
    ctx.style_mut(|style| {
        style.interaction.show_tooltips_only_when_still = false;
        style.interaction.tooltip_delay = 0.05;
        style.interaction.tooltip_grace_time = 0.4;
    });

    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = Color32::from_rgb(7, 14, 24);
    visuals.panel_fill = Color32::from_rgb(10, 18, 29);
    visuals.extreme_bg_color = Color32::from_rgb(12, 23, 38);
    visuals.faint_bg_color = Color32::from_rgb(16, 28, 44);
    visuals.override_text_color = Some(Color32::from_rgb(233, 241, 248));
    visuals.selection.bg_fill = Color32::from_rgb(54, 121, 255);
    visuals.widgets.active.bg_fill = Color32::from_rgb(36, 92, 182);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(28, 71, 138);
    ctx.set_visuals(visuals);
}

fn status_text_is_error(text: &str) -> bool {
    text.starts_with("Failed")
        || text.starts_with("Execution failed")
        || text.starts_with("File dialog closed unexpectedly")
}

fn exit_label(exit: RuntimeExit) -> &'static str {
    match exit {
        RuntimeExit::BreakHit => "BREAK",
        RuntimeExit::Sleeping => "Sleeping",
        RuntimeExit::MaxInstructionsReached => "Running",
    }
}

fn code_block(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).monospace());
}

fn summary_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).strong());
    ui.monospace(value);
    ui.end_row();
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use rust_mcu::BoardPin;

    use crate::{
        example_firmware::MEGA_PIN_SWEEP,
        runtime::{SimulationRuntime, SimulationTarget},
    };

    use super::{active_preview_nets, load_host_board_preview, preview_net_aliases};

    #[test]
    fn mega_preview_aliases_cover_port_named_header_nets() {
        assert!(preview_net_aliases(SimulationTarget::Mega, BoardPin::Digital(22)).contains(&"PA0".to_owned()));
        assert!(preview_net_aliases(SimulationTarget::Mega, BoardPin::Digital(44)).contains(&"PL5".to_owned()));
        assert!(preview_net_aliases(SimulationTarget::Mega, BoardPin::Digital(53)).contains(&"PB0".to_owned()));
    }

    #[test]
    fn mega_preview_aliases_cover_serial_and_spi_header_nets() {
        let d0 = preview_net_aliases(SimulationTarget::Mega, BoardPin::Digital(0));
        let d1 = preview_net_aliases(SimulationTarget::Mega, BoardPin::Digital(1));
        let d50 = preview_net_aliases(SimulationTarget::Mega, BoardPin::Digital(50));
        let d52 = preview_net_aliases(SimulationTarget::Mega, BoardPin::Digital(52));

        assert!(d0.contains(&"RXL".to_owned()));
        assert!(d1.contains(&"TXL".to_owned()));
        assert!(d50.contains(&"MISO2".to_owned()));
        assert!(d52.contains(&"SCK2".to_owned()));
    }

    #[test]
    fn mega_preview_aliases_cover_analog_header_nets() {
        let a15 = preview_net_aliases(SimulationTarget::Mega, BoardPin::Analog(15));
        let a0 = preview_net_aliases(SimulationTarget::Mega, BoardPin::Analog(0));

        assert!(a15.contains(&"A15".to_owned()));
        assert!(a15.contains(&"ADC15".to_owned()));
        assert!(a0.contains(&"A0".to_owned()));
        assert!(a0.contains(&"ADC0".to_owned()));
    }

    #[test]
    fn mega_pin_sweep_example_highlights_embedded_preview_nets() {
        let preview = load_host_board_preview(SimulationTarget::Mega).expect("mega preview");
        let preview_nets = preview.net_names.into_iter().collect::<BTreeSet<_>>();
        let mut runtime = SimulationRuntime::new(SimulationTarget::Mega);
        runtime
            .load_hex(MEGA_PIN_SWEEP.hex)
            .expect("load mega example");

        let mut highlighted = BTreeSet::new();
        for _ in 0..600 {
            runtime.run_chunk(20_000).expect("run mega example");
            let host_pin_levels = runtime.host_pin_levels();
            let active_nets = active_preview_nets(SimulationTarget::Mega, &host_pin_levels);
            highlighted.extend(active_nets.intersection(&preview_nets).cloned());
            if !highlighted.is_empty() {
                break;
            }
        }

        assert!(
            !highlighted.is_empty(),
            "mega example should drive at least one net that exists in the embedded KiCad preview"
        );
    }
}
