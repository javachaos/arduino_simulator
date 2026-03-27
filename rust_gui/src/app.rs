use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::pcb_view::{render_pcb, LoadedPcb};
use crate::simulation::{
    SharedSimulationState, SimulationController, SimulationSnapshot, SimulatorStatus,
};
use eframe::egui;
use eframe::egui::{Color32, RichText};
use rust_board::{built_in_board_model_names, load_built_in_board_model};
use rust_mcu::BoardPin;
use rust_project::{
    BindingMode, FirmwareSource, FirmwareSourceKind, HostBoard, ModuleOverlay, ModuleSignalBinding,
    PcbSource, ProbeSpec, SignalBinding, SimulationProject, StimulusSpec,
};

const PROJECT_FILE_SUFFIX: &str = ".avrsim.json";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct SignalActivity {
    is_high: bool,
    is_flashing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ControllerConnection {
    controller_pin: String,
    pcb_net: String,
    mode: BindingMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NetSuggestion {
    net_name: String,
    score: i32,
    reason: &'static str,
}

impl NetSuggestion {
    fn confidence_label(&self) -> &'static str {
        match self.score {
            900.. => "exact",
            450.. => "high",
            250.. => "medium",
            _ => "low",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModuleSignalSuggestion {
    module_signal: String,
    suggestion: NetSuggestion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum NumericHintKind {
    Unknown,
    Digital,
    Analog,
}

pub struct AvrSimGuiApp {
    controller: SimulationController,
    selected_board: HostBoard,
    project_name: String,
    project_path: String,
    project_description: String,
    source_path: String,
    pcb_path: String,
    project_notice: String,
    loaded_pcb: Option<LoadedPcb>,
    bindings: BTreeMap<String, SignalBinding>,
    module_overlays: Vec<ModuleOverlay>,
    project_probes: Vec<ProbeSpec>,
    project_stimuli: Vec<StimulusSpec>,
    mega_board_preview: Option<LoadedPcb>,
    nano_board_preview: Option<LoadedPcb>,
    wiring_open: bool,
    compile_log_open: bool,
    controller_pins_open: bool,
    serial_console_open: bool,
    serial_terminal_baud: u32,
    serial_input: String,
    serial_append_line_ending: bool,
    pending_module_model: String,
    last_sequence: u64,
    snapshot: SimulationSnapshot,
    host_signal_levels: BTreeMap<String, u8>,
    host_signal_flash_until: BTreeMap<String, Instant>,
}

impl Default for AvrSimGuiApp {
    fn default() -> Self {
        let controller = SimulationController::new();
        let initial = controller.latest_snapshot();
        Self {
            controller,
            selected_board: HostBoard::Mega2560Rev3,
            project_name: "Untitled Simulation".to_string(),
            project_path: String::new(),
            project_description: String::new(),
            source_path: String::new(),
            pcb_path: String::new(),
            project_notice: String::new(),
            loaded_pcb: None,
            bindings: BTreeMap::new(),
            module_overlays: Vec::new(),
            project_probes: Vec::new(),
            project_stimuli: Vec::new(),
            mega_board_preview: None,
            nano_board_preview: None,
            wiring_open: false,
            compile_log_open: false,
            controller_pins_open: false,
            serial_console_open: false,
            serial_terminal_baud: 115_200,
            serial_input: String::new(),
            serial_append_line_ending: true,
            pending_module_model: "gy_sht31_d".to_string(),
            last_sequence: initial.sequence,
            snapshot: initial.snapshot,
            host_signal_levels: BTreeMap::new(),
            host_signal_flash_until: BTreeMap::new(),
        }
    }
}

impl eframe::App for AvrSimGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.style_mut(|style| {
            style.interaction.show_tooltips_only_when_still = false;
            style.interaction.tooltip_delay = 0.05;
            style.interaction.tooltip_grace_time = 0.4;
        });
        self.refresh_snapshot();
        ctx.request_repaint_after(Duration::from_millis(16));

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.heading("arduino_simulator");
                ui.label("Load a PCB, auto-wire the selected Arduino CPU to its nets, overlay attached modules, and watch the simulation live on the board drawing.");
                ui.add_space(4.0);

                ui.horizontal_wrapped(|ui| {
                    ui.label("Project:");
                    ui.add_sized(
                        [220.0, 24.0],
                        egui::TextEdit::singleline(&mut self.project_name)
                            .hint_text("Simulation project name"),
                    );
                    if ui.button("Open Project").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("avrsim project", &["json"])
                            .pick_file()
                        {
                            self.load_project_file(&path);
                        }
                    }
                    if ui.button("Save Project").clicked() {
                        self.save_project_dialog();
                    }
                    if ui
                        .button(if self.wiring_open {
                            "Hide Wiring"
                        } else {
                            "Wiring"
                        })
                        .clicked()
                    {
                        self.wiring_open = !self.wiring_open;
                    }
                    if ui
                        .button(if self.controller_pins_open {
                            "Hide Controller Pins"
                        } else {
                            "Controller Pins"
                        })
                        .clicked()
                    {
                        self.controller_pins_open = !self.controller_pins_open;
                    }
                    if ui
                        .button(if self.compile_log_open {
                            "Hide Compile Log"
                        } else {
                            "Compile Log"
                        })
                        .clicked()
                    {
                        self.compile_log_open = !self.compile_log_open;
                    }
                    if ui.button("Serial Console").clicked() {
                        self.serial_console_open = true;
                    }
                    if !self.project_path.trim().is_empty() {
                        ui.separator();
                        ui.label(format!("File: {}", self.project_path));
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    let previous_board = self.selected_board;
                    ui.label("Target:");
                    egui::ComboBox::from_id_salt("board_target")
                        .selected_text(self.selected_board.label())
                        .show_ui(ui, |ui| {
                            for target in HostBoard::ALL {
                                ui.selectable_value(&mut self.selected_board, target, target.label());
                            }
                        });
                    if self.selected_board != previous_board {
                        self.prune_bindings_for_selected_board();
                        self.reload_source_for_selected_board_if_unwired();
                    }

                    ui.label("Source:");
                    ui.add_sized(
                        [420.0, 24.0],
                        egui::TextEdit::singleline(&mut self.source_path)
                            .hint_text("Select a .ino sketch or .hex image"),
                    );
                    if ui.button("Browse").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("AVR firmware", &["ino", "hex"])
                            .pick_file()
                        {
                            self.source_path = path.display().to_string();
                            if let Some(hinted_board) = inferred_host_board_from_source(&self.source_path)
                            {
                                if hinted_board != self.selected_board {
                                    self.selected_board = hinted_board;
                                    self.prune_bindings_for_selected_board();
                                    self.project_notice = format!(
                                        "Selected target changed to {} based on the source file name.",
                                        hinted_board.label()
                                    );
                                }
                            }
                        }
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label("PCB:");
                    ui.add_sized(
                        [420.0, 24.0],
                        egui::TextEdit::singleline(&mut self.pcb_path)
                            .hint_text("Select a .kicad_pcb file"),
                    );
                    if ui.button("Browse PCB").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("KiCad PCB", &["kicad_pcb"])
                            .pick_file()
                        {
                            self.pcb_path = path.display().to_string();
                            self.bindings.clear();
                            self.load_pcb_path(&path, true);
                        }
                    }

                    let action = classify_source(&self.source_path);
                    let action_enabled = action != SourceAction::None;
                    let action_label = match action {
                        SourceAction::Compile => "Compile && Load",
                        SourceAction::LoadHex => "Load HEX",
                        SourceAction::None => "Load",
                    };
                    if ui
                        .add_enabled(action_enabled, egui::Button::new(action_label))
                        .clicked()
                    {
                        let path = PathBuf::from(self.source_path.trim());
                        match action {
                            SourceAction::Compile => {
                                self.controller.compile_and_load(path, self.selected_board)
                            }
                            SourceAction::LoadHex => {
                                self.controller.load_hex(path, self.selected_board)
                            }
                            SourceAction::None => {}
                        }
                    }

                    let can_control = self.snapshot.firmware_path.is_some();
                    let running = self.snapshot.status == SimulatorStatus::Running;
                    if ui
                        .add_enabled(
                            can_control,
                            egui::Button::new(if running { "Pause" } else { "Run" }),
                        )
                        .clicked()
                    {
                        if running {
                            self.controller.pause();
                        } else {
                            self.controller.run();
                        }
                    }
                    if ui
                        .add_enabled(
                            can_control && self.snapshot.status != SimulatorStatus::Running,
                            egui::Button::new("Step"),
                        )
                        .clicked()
                    {
                        self.controller.step();
                    }
                    if ui
                        .add_enabled(can_control, egui::Button::new("Reset"))
                        .clicked()
                    {
                        self.controller.reset();
                    }
                    if ui
                        .add_enabled(can_control, egui::Button::new("Clear Serial"))
                        .clicked()
                    {
                        self.controller.clear_serial();
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    ui.label("Status:");
                    ui.colored_label(status_color(self.snapshot.status), self.snapshot.status.label());
                    ui.separator();
                    ui.label(&self.snapshot.status_message);
                    if target_runtime_board_mismatch(self.selected_board, &self.snapshot) {
                        ui.separator();
                        ui.colored_label(
                            Color32::from_rgb(220, 180, 60),
                            format!(
                                "Target is {} but loaded runtime is {}. Reload or recompile to switch boards.",
                                self.selected_board.label(),
                                self.snapshot.board.label()
                            ),
                        );
                    }
                    if !self.project_notice.is_empty() {
                        ui.separator();
                        ui.label(&self.project_notice);
                    }
                    if let Some(path) = &self.snapshot.firmware_path {
                        ui.separator();
                        ui.label(format!("HEX: {}", path.display()));
                    }
                });
            });
        });

        egui::SidePanel::left("cpu_panel")
            .resizable(true)
            .default_width(360.0)
            .show(ctx, |ui| {
                let displayed_board = displayed_host_board(self.selected_board, &self.snapshot);
                ui.heading("CPU");
                ui.add_space(4.0);
                egui::Grid::new("cpu_summary")
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        summary_row(ui, "Target", self.selected_board.label());
                        if self.snapshot.firmware_path.is_some() {
                            summary_row(ui, "Runtime", displayed_board.label());
                        }
                        summary_row(ui, "PC", &format!("0x{:06X}", self.snapshot.pc));
                        summary_row(ui, "SP", &format!("0x{:04X}", self.snapshot.sp));
                        summary_row(ui, "Cycles", &self.snapshot.cycles.to_string());
                        summary_row(ui, "Synced", &self.snapshot.synced_cycles.to_string());
                        summary_row(
                            ui,
                            "Serial",
                            &format!("{} bytes", self.snapshot.serial_bytes),
                        );
                    });

                ui.add_space(8.0);
                self.draw_host_board_card(ui, displayed_board);
                ui.add_space(8.0);
                ui.label(RichText::new("Next Instruction").strong());
                code_block(ui, &self.snapshot.next_instruction);
                ui.add_space(8.0);
                ui.label(RichText::new("SREG").strong());
                code_block(ui, &format_sreg(self.snapshot.sreg));

                if !self.snapshot.extra_lines.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Peripherals").strong());
                    for line in &self.snapshot.extra_lines {
                        code_block(ui, line);
                    }
                }

                ui.add_space(8.0);
                ui.label(RichText::new("Registers").strong());
                egui::ScrollArea::vertical()
                    .id_salt("cpu_registers_scroll")
                    .max_height(340.0)
                    .show(ui, |ui| {
                        egui::Grid::new("register_grid")
                            .num_columns(4)
                            .spacing([12.0, 2.0])
                            .show(ui, |ui| {
                                for row in 0..8 {
                                    for column in 0..4 {
                                        let index = row * 4 + column;
                                        ui.monospace(format!(
                                            "R{index:02} = 0x{:02X}",
                                            self.snapshot.registers[index]
                                        ));
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("PCB View");
            ui.add_space(4.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                if let Some(loaded_pcb) = &self.loaded_pcb {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("PCB: {}", loaded_pcb.board.name));
                        ui.separator();
                        ui.label(format!("Components: {}", loaded_pcb.board.components.len()));
                        ui.separator();
                        ui.label(format!("PCB nets: {}", loaded_pcb.net_names.len()));
                        ui.separator();
                        ui.label(format!("Controller pins: {}", self.bindings.len()));
                        ui.separator();
                        ui.label(format!("Modules: {}", self.module_overlays.len()));
                    });
                    ui.add_space(6.0);
                    let board_height = ui.available_height().max(1.0);
                    let side_width = 320.0;
                    let gap = 10.0;
                    let render_width = (ui.available_width() - side_width - gap).max(320.0);
                    ui.horizontal_top(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(render_width, board_height),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                render_pcb(
                                    ui,
                                    loaded_pcb,
                                    &self.binding_list(),
                                    &self.module_overlays,
                                    &self.module_highlight_nets(),
                                    &self.active_highlight_nets(),
                                );
                            },
                        );
                        ui.add_space(gap);
                        ui.allocate_ui_with_layout(
                            egui::vec2(side_width, board_height),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                egui::ScrollArea::vertical()
                                    .id_salt("assembly_side_panel_scroll")
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        self.draw_module_summary_block(ui);
                                    });
                            },
                        );
                    });
                } else {
                    let board_height = ui.available_height().max(1.0);
                    ui.set_min_height(board_height);
                    ui.centered_and_justified(|ui| {
                        ui.label("Load a .kicad_pcb file to auto-wire the selected Arduino and inspect live PCB activity.");
                    });
                }
            });
        });

        self.show_wiring_window(ctx);
        self.show_compile_log_window(ctx);
        self.show_controller_pins_window(ctx);
        self.show_serial_console(ctx);
    }
}

impl AvrSimGuiApp {
    fn refresh_snapshot(&mut self) {
        let latest: SharedSimulationState = self.controller.latest_snapshot();
        if latest.sequence != self.last_sequence {
            self.last_sequence = latest.sequence;
            self.update_host_signal_activity(&latest.snapshot);
            self.snapshot = latest.snapshot;
        }
    }

    fn reload_source_for_selected_board_if_unwired(&mut self) {
        if !can_hot_swap_target_without_pcb(
            self.loaded_pcb.is_some(),
            &self.source_path,
            &self.snapshot,
        ) {
            return;
        }

        let path = PathBuf::from(self.source_path.trim());
        match classify_source(&self.source_path) {
            SourceAction::Compile => {
                self.controller
                    .compile_and_load(path.clone(), self.selected_board);
                self.project_notice = format!(
                    "Switched target to {} and recompiling {}.",
                    self.selected_board.label(),
                    path.display()
                );
            }
            SourceAction::LoadHex => {
                self.controller.load_hex(path.clone(), self.selected_board);
                self.project_notice = format!(
                    "Switched target to {} and reloading {}.",
                    self.selected_board.label(),
                    path.display()
                );
            }
            SourceAction::None => {}
        }
    }

    fn draw_host_board_card(&mut self, ui: &mut egui::Ui, board: HostBoard) {
        let context_note = if self.snapshot.firmware_path.is_some() {
            "Loaded runtime board"
        } else {
            "Selected target board"
        };
        let module_nets = BTreeSet::new();
        let preview = self.board_preview(board).cloned();
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(RichText::new("Current Board").strong());
            ui.small(format!("{context_note}: {}", board.label()));
            if target_runtime_board_mismatch(self.selected_board, &self.snapshot) {
                ui.small(format!(
                    "Target selection is {}. Reload or recompile to apply it.",
                    self.selected_board.label()
                ));
            }
            ui.add_space(6.0);
            if let Some(preview) = &preview {
                let bindings = self.host_board_preview_bindings(board, preview);
                let active_nets = self.active_host_preview_nets(board, preview);
                let height = 190.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        render_pcb(ui, preview, &bindings, &[], &module_nets, &active_nets);
                    },
                );
                if !bindings.is_empty() {
                    ui.small(format!(
                        "Wired controller pins highlighted: {}",
                        bindings.len()
                    ));
                }
                if !active_nets.is_empty() {
                    ui.small(format!("Active host nets: {}", active_nets.len()));
                }
                if bindings.is_empty() && active_nets.is_empty() {
                    ui.small("Host nets will highlight here as wiring and activity appear.");
                }
            } else {
                ui.small("Board preview canvas is unavailable.");
            }
        });
    }

    fn board_preview(&mut self, board: HostBoard) -> Option<&LoadedPcb> {
        let slot = match board {
            HostBoard::Mega2560Rev3 => &mut self.mega_board_preview,
            HostBoard::NanoV3 => &mut self.nano_board_preview,
        };
        if slot.is_none() {
            *slot = load_host_board_preview(board);
        }
        slot.as_ref()
    }

    fn host_board_preview_bindings(
        &self,
        board: HostBoard,
        preview: &LoadedPcb,
    ) -> Vec<SignalBinding> {
        let valid = host_signals_for_board(board)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let preview_nets = preview.net_names.iter().cloned().collect::<BTreeSet<_>>();
        let mut bindings = self
            .bindings
            .values()
            .filter(|binding| valid.contains(&binding.board_signal))
            .flat_map(|binding| {
                preview_net_aliases(board, &binding.board_signal, &preview_nets)
                    .into_iter()
                    .map(|pcb_net| SignalBinding {
                        board_signal: binding.board_signal.clone(),
                        pcb_net,
                        mode: binding.mode,
                        note: binding.note.clone(),
                    })
            })
            .collect::<Vec<_>>();
        bindings.sort_by(|left, right| {
            left.board_signal
                .cmp(&right.board_signal)
                .then_with(|| left.pcb_net.cmp(&right.pcb_net))
        });
        bindings.dedup_by(|left, right| {
            left.board_signal == right.board_signal && left.pcb_net == right.pcb_net
        });
        bindings
    }

    fn active_host_preview_nets(&self, board: HostBoard, preview: &LoadedPcb) -> BTreeSet<String> {
        let now = Instant::now();
        let preview_nets = preview.net_names.iter().cloned().collect::<BTreeSet<_>>();
        host_signals_for_board(board)
            .into_iter()
            .filter(|signal| {
                self.host_signal_levels.get(signal).copied().unwrap_or(0) != 0
                    || self
                        .host_signal_flash_until
                        .get(signal)
                        .map(|deadline| *deadline > now)
                        .unwrap_or(false)
            })
            .flat_map(|signal| preview_net_aliases(board, &signal, &preview_nets))
            .collect()
    }

    fn update_host_signal_activity(&mut self, snapshot: &SimulationSnapshot) {
        let now = Instant::now();
        let next_levels = host_signal_levels_for_snapshot(snapshot);
        for (signal, level) in &next_levels {
            if self.host_signal_levels.get(signal).copied().unwrap_or(0) != *level {
                self.host_signal_flash_until
                    .insert(signal.clone(), now + Duration::from_millis(240));
            }
        }
        self.host_signal_flash_until
            .retain(|_, deadline| *deadline > now);
        self.host_signal_levels = next_levels;
    }

    fn load_pcb_path(&mut self, path: &Path, auto_wire: bool) {
        if path.as_os_str().is_empty() {
            self.project_notice = "Select a .kicad_pcb file first.".to_string();
            return;
        }
        match LoadedPcb::load(path) {
            Ok(loaded) => {
                self.pcb_path = path.display().to_string();
                self.loaded_pcb = Some(loaded);
                if auto_wire {
                    let controller_bound = self.auto_bind_common_aliases(true);
                    let module_bound = self.auto_wire_all_modules();
                    self.project_notice = format!(
                        "Loaded PCB {} and auto-wired {controller_bound} controller pin(s) plus {module_bound} module signal(s).",
                        path.display()
                    );
                } else {
                    let (rebound, dropped) = self.reconcile_controller_bindings();
                    let module_bound = self.auto_wire_all_modules();
                    if rebound > 0 || dropped > 0 {
                        self.project_notice = format!(
                            "Loaded PCB {} and refreshed {} controller binding(s) after dropping {dropped} stale net reference(s); rewired {module_bound} module signal(s).",
                            path.display(),
                            rebound,
                        );
                    } else {
                        self.project_notice = format!(
                            "Loaded PCB {} with {} controller binding(s) and rewired {module_bound} module signal(s).",
                            path.display(),
                            self.bindings.len(),
                        );
                    }
                }
            }
            Err(error) => {
                self.loaded_pcb = None;
                self.project_notice = format!("PCB load failed: {error}");
            }
        }
    }

    fn draw_wiring_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Wiring");
        ui.add_space(4.0);

        let host_signals = self.host_signals();
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Host signals: {}", host_signals.len()));
            ui.separator();
            ui.label(format!("Controller wired: {}", self.bindings.len()));
            ui.separator();
            ui.label(format!("Modules: {}", self.module_overlays.len()));
            if let Some(loaded) = &self.loaded_pcb {
                ui.separator();
                ui.label(format!("PCB nets: {}", loaded.net_names.len()));
            }
        });

        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.loaded_pcb.is_some(),
                    egui::Button::new("Auto-Wire Controller"),
                )
                .clicked()
            {
                let bound = self.auto_bind_common_aliases(true);
                self.project_notice =
                    format!("Auto-wired {bound} controller signal(s) to the loaded PCB.");
            }
            if ui
                .add_enabled(
                    !self.bindings.is_empty(),
                    egui::Button::new("Clear Controller Wiring"),
                )
                .clicked()
            {
                self.bindings.clear();
                self.project_notice = "Cleared all controller-to-PCB wiring.".to_string();
            }
            if ui
                .add_enabled(
                    self.loaded_pcb.is_some(),
                    egui::Button::new("Auto-Wire Modules"),
                )
                .clicked()
            {
                let wired = self.auto_wire_all_modules();
                self.project_notice =
                    format!("Auto-wired {wired} module signal(s) onto the loaded PCB.");
            }
        });

        ui.add_space(6.0);
        ui.label(RichText::new("Attached Modules").strong());
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("pending_module_model")
                .selected_text(module_model_title(&self.pending_module_model))
                .show_ui(ui, |ui| {
                    for model in available_module_models() {
                        ui.selectable_value(
                            &mut self.pending_module_model,
                            model.to_string(),
                            module_model_title(model),
                        );
                    }
                });
            if ui
                .add_enabled(self.loaded_pcb.is_some(), egui::Button::new("Add Module"))
                .clicked()
            {
                self.add_pending_module_overlay();
            }
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .id_salt("module_overlay_scroll")
            .max_height(240.0)
            .show(ui, |ui| {
                if self.module_overlays.is_empty() {
                    ui.small("No overlay modules added yet.");
                }
                let mut remove_index = None;
                for (index, module) in self.module_overlays.iter_mut().enumerate() {
                    let suggestions = suggested_module_bindings(module, self.loaded_pcb.as_ref());
                    ui.group(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(&module.name).strong());
                            ui.small(module_model_title(&module.model));
                            if ui.button("Rewire").clicked() {
                                let wired =
                                    auto_wire_module_overlay(module, self.loaded_pcb.as_ref());
                                self.project_notice = format!(
                                    "Rewired {} signal(s) for module {}.",
                                    wired, module.name
                                );
                            }
                            if ui.button("Remove").clicked() {
                                remove_index = Some(index);
                            }
                        });
                        if module.bindings.is_empty() {
                            if suggestions.is_empty() {
                                ui.small("No PCB nets matched yet for this module.");
                            } else {
                                ui.small("Likely matches:");
                            }
                        } else {
                            for binding in &module.bindings {
                                ui.small(format!(
                                    "{} -> {} ({})",
                                    binding.module_signal,
                                    binding.pcb_net,
                                    binding_mode_label(binding.mode)
                                ));
                            }
                        }
                        for suggestion in suggestions.iter().take(4) {
                            ui.small(format!(
                                "{} -> {} ({}, {})",
                                suggestion.module_signal,
                                suggestion.suggestion.net_name,
                                suggestion.suggestion.confidence_label(),
                                suggestion.suggestion.reason
                            ));
                        }
                    });
                    ui.add_space(4.0);
                }
                if let Some(index) = remove_index {
                    let removed = self.module_overlays.remove(index);
                    self.project_notice = format!("Removed module overlay {}.", removed.name);
                }
            });

        ui.add_space(8.0);
        egui::CollapsingHeader::new("Advanced Controller Wiring")
            .default_open(self.loaded_pcb.is_some() && self.bindings.is_empty())
            .show(ui, |ui| {
                self.draw_advanced_controller_binding_matrix(ui);
            });
    }

    fn show_wiring_window(&mut self, ctx: &egui::Context) {
        if !self.wiring_open {
            return;
        }
        let title = format!(
            "Wiring ({} controller, {} modules)",
            self.bindings.len(),
            self.module_overlays.len()
        );
        let mut open = self.wiring_open;
        egui::Window::new(title)
            .open(&mut open)
            .default_size(egui::vec2(380.0, 620.0))
            .show(ctx, |ui| {
                self.draw_wiring_panel(ui);
            });
        self.wiring_open = open;
    }

    fn host_signals(&self) -> Vec<String> {
        host_signals_for_board(self.selected_board)
    }

    fn draw_advanced_controller_binding_matrix(&mut self, ui: &mut egui::Ui) {
        let Some(loaded_pcb) = &self.loaded_pcb else {
            ui.small("Load a PCB to edit controller-to-net mappings.");
            return;
        };
        let target_names = loaded_pcb.net_names.clone();
        let available_nets = target_names.iter().cloned().collect::<BTreeSet<_>>();
        egui::ScrollArea::vertical()
            .id_salt("advanced_host_bindings_scroll")
            .max_height(320.0)
            .show(ui, |ui| {
                for signal in self.host_signals() {
                    let existing = self.bindings.get(&signal).cloned();
                    let mut selected_net = existing
                        .as_ref()
                        .map(|binding| binding.pcb_net.clone())
                        .unwrap_or_default();
                    let mode = existing
                        .as_ref()
                        .map(|binding| binding.mode)
                        .unwrap_or_else(|| infer_binding_mode(&signal));
                    let suggestions = controller_signal_suggestions(
                        self.selected_board,
                        &signal,
                        &available_nets,
                    );
                    let display_signal =
                        standard_controller_signal_label(self.selected_board, &signal);

                    ui.group(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.monospace(display_signal);
                            ui.separator();
                            ui.label(binding_mode_label(mode));
                        });
                        if let Some(best) = suggestions.first() {
                            ui.horizontal_wrapped(|ui| {
                                ui.small(format!(
                                    "Suggested: {} ({}, {})",
                                    best.net_name,
                                    best.confidence_label(),
                                    best.reason
                                ));
                                if selected_net != best.net_name
                                    && ui.button("Use Suggested").clicked()
                                {
                                    selected_net = best.net_name.clone();
                                }
                            });
                            let alternatives = suggestions
                                .iter()
                                .filter(|suggestion| suggestion.net_name != best.net_name)
                                .take(2)
                                .collect::<Vec<_>>();
                            if !alternatives.is_empty() {
                                ui.horizontal_wrapped(|ui| {
                                    ui.small("Other likely nets:");
                                    for suggestion in alternatives {
                                        if ui
                                            .small_button(format!(
                                                "{} ({})",
                                                suggestion.net_name,
                                                suggestion.confidence_label()
                                            ))
                                            .clicked()
                                        {
                                            selected_net = suggestion.net_name.clone();
                                        }
                                    }
                                });
                            }
                        }
                        egui::ComboBox::from_id_salt(format!("binding_{signal}"))
                            .width(250.0)
                            .selected_text(if selected_net.is_empty() {
                                "-- Unbound --".to_string()
                            } else {
                                selected_net.clone()
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut selected_net,
                                    String::new(),
                                    "-- Unbound --",
                                );
                                for target_name in &target_names {
                                    ui.selectable_value(
                                        &mut selected_net,
                                        target_name.clone(),
                                        target_name,
                                    );
                                }
                            });
                    });

                    if selected_net.trim().is_empty() {
                        self.bindings.remove(&signal);
                    } else {
                        let note = existing.and_then(|binding| {
                            if binding.pcb_net == selected_net {
                                binding.note
                            } else {
                                None
                            }
                        });
                        self.bindings.insert(
                            signal.clone(),
                            SignalBinding {
                                board_signal: signal.clone(),
                                pcb_net: selected_net,
                                mode,
                                note,
                            },
                        );
                    }
                    ui.add_space(4.0);
                }
            });
    }

    fn show_controller_pins_window(&mut self, ctx: &egui::Context) {
        if !self.controller_pins_open {
            return;
        }
        let title = {
            let count = self.controller_connections().len();
            if count == 0 {
                "Controller Pins".to_string()
            } else {
                format!("Controller Pins ({count})")
            }
        };
        let mut open = self.controller_pins_open;
        egui::Window::new(title)
            .open(&mut open)
            .default_size(egui::vec2(360.0, 440.0))
            .show(ctx, |ui| {
                self.draw_controller_pin_contents(ui);
            });
        self.controller_pins_open = open;
    }

    fn draw_controller_pin_contents(&self, ui: &mut egui::Ui) {
        let pulse_time = ui.input(|input| input.time) as f32;
        let connections = self.controller_connections();
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.small("Auto-wired Arduino pins on the selected CPU, with live simulator state.");
            ui.add_space(6.0);
            if connections.is_empty() {
                ui.small("No controller pins are wired to the currently loaded PCB.");
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("board_view_controller_pins_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for connection in &connections {
                        let activity = self.controller_signal_activity(&connection.controller_pin);
                        let indicator_color = connectable_pin_indicator_color(activity, pulse_time);
                        let display_pin = standard_controller_signal_label(
                            self.selected_board,
                            &connection.controller_pin,
                        );
                        ui.group(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.colored_label(indicator_color, "●");
                                ui.monospace(display_pin);
                                ui.small("->");
                                ui.label(&connection.pcb_net);
                            });
                            ui.small(format!("Mode: {}", binding_mode_label(connection.mode)));
                            ui.small(format!(
                                "Pin status: {}",
                                self.controller_signal_status_text(&connection.controller_pin)
                            ));
                        });
                        ui.add_space(4.0);
                    }
                });
        });
    }

    fn show_compile_log_window(&mut self, ctx: &egui::Context) {
        if !self.compile_log_open {
            return;
        }
        let mut open = self.compile_log_open;
        egui::Window::new("Compile Log")
            .open(&mut open)
            .default_size(egui::vec2(560.0, 320.0))
            .show(ctx, |ui| {
                let compile_log = if self.snapshot.compile_log.is_empty() {
                    "<no compile log yet>".to_string()
                } else {
                    self.snapshot.compile_log.clone()
                };
                let height = ui.available_height().max(220.0);
                scrolled_text_block(ui, "compile_log_window", compile_log, height, false);
            });
        self.compile_log_open = open;
    }

    fn draw_module_summary_block(&self, ui: &mut egui::Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(RichText::new("Module Overlays").strong());
            ui.small("Built-in modules attached to this PCB-centric simulation.");
            ui.add_space(6.0);
            if self.module_overlays.is_empty() {
                ui.small("No modules are currently attached.");
                return;
            }
            for module in &self.module_overlays {
                ui.group(|ui| {
                    ui.label(RichText::new(&module.name).strong());
                    ui.small(module_model_title(&module.model));
                    if module.bindings.is_empty() {
                        ui.small("Unwired");
                    } else {
                        ui.small(format!("{} connected signal(s)", module.bindings.len()));
                        for binding in &module.bindings {
                            ui.small(format!("{} -> {}", binding.module_signal, binding.pcb_net));
                        }
                    }
                });
                ui.add_space(4.0);
            }
        });
    }

    fn controller_signal_activity(&self, signal: &str) -> SignalActivity {
        let now = Instant::now();
        SignalActivity {
            is_high: self.host_signal_levels.get(signal).copied().unwrap_or(0) != 0,
            is_flashing: self
                .host_signal_flash_until
                .get(signal)
                .map(|deadline| *deadline > now)
                .unwrap_or(false),
        }
    }

    fn controller_signal_status_text(&self, signal: &str) -> String {
        if signal == "GND" {
            return "ground".to_string();
        }
        if matches!(signal, "+5V" | "+3V3" | "VIN" | "IOREF" | "AREF") {
            return "power rail".to_string();
        }
        let activity = self.controller_signal_activity(signal);
        if activity.is_flashing {
            "activity pulse".to_string()
        } else if activity.is_high {
            "high".to_string()
        } else if self.host_signal_levels.contains_key(signal) {
            "low".to_string()
        } else {
            "unresolved".to_string()
        }
    }

    fn controller_connections(&self) -> Vec<ControllerConnection> {
        let mut connections = self
            .bindings
            .values()
            .map(|binding| ControllerConnection {
                controller_pin: binding.board_signal.clone(),
                pcb_net: binding.pcb_net.clone(),
                mode: binding.mode,
            })
            .collect::<Vec<_>>();
        connections.sort_by(|left, right| {
            left.controller_pin
                .cmp(&right.controller_pin)
                .then_with(|| left.pcb_net.cmp(&right.pcb_net))
        });
        connections
    }

    fn module_highlight_nets(&self) -> BTreeSet<String> {
        self.module_overlays
            .iter()
            .flat_map(|module| {
                module
                    .bindings
                    .iter()
                    .map(|binding| binding.pcb_net.clone())
            })
            .collect()
    }

    fn active_highlight_nets(&self) -> BTreeSet<String> {
        let now = Instant::now();
        self.bindings
            .values()
            .filter(|binding| {
                let signal = binding.board_signal.as_str();
                self.host_signal_levels.get(signal).copied().unwrap_or(0) != 0
                    || self
                        .host_signal_flash_until
                        .get(signal)
                        .map(|deadline| *deadline > now)
                        .unwrap_or(false)
            })
            .map(|binding| binding.pcb_net.clone())
            .collect()
    }

    fn prune_bindings_for_selected_board(&mut self) {
        let valid = self.host_signals().into_iter().collect::<BTreeSet<_>>();
        self.bindings.retain(|signal, _| valid.contains(signal));
        if self.loaded_pcb.is_some() {
            let bound = self.auto_bind_common_aliases(true);
            self.project_notice = format!(
                "Switched host board to {} and auto-wired {bound} signal(s) to the loaded PCB.",
                self.selected_board.label()
            );
        } else {
            self.project_notice = format!(
                "Switched host board to {}; wiring will be available after you load a PCB.",
                self.selected_board.label()
            );
        }
    }

    fn auto_bind_common_aliases(&mut self, clear_existing: bool) -> usize {
        let available = self
            .loaded_pcb
            .as_ref()
            .map(|loaded| loaded.net_names.iter().cloned().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        if available.is_empty() {
            return 0;
        }
        if clear_existing {
            self.bindings.clear();
        }
        let mut bound = 0usize;
        for signal in self.host_signals() {
            if self.bindings.contains_key(&signal) {
                continue;
            }
            let suggestions =
                controller_signal_suggestions(self.selected_board, &signal, &available);
            if !should_auto_apply_suggestion(&suggestions) {
                continue;
            }
            let Some(best) = suggestions.first() else {
                continue;
            };
            self.bindings.insert(
                signal.clone(),
                SignalBinding {
                    board_signal: signal.clone(),
                    pcb_net: best.net_name.clone(),
                    mode: infer_binding_mode(&signal),
                    note: Some(format!(
                        "Auto-bound by arduino_simulator ({}, {})",
                        best.confidence_label(),
                        best.reason
                    )),
                },
            );
            bound += 1;
        }
        bound
    }

    fn reconcile_controller_bindings(&mut self) -> (usize, usize) {
        let available = self
            .loaded_pcb
            .as_ref()
            .map(|loaded| loaded.net_names.iter().cloned().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        if available.is_empty() {
            return (0, 0);
        }

        let stale_signals = self
            .bindings
            .iter()
            .filter_map(|(signal, binding)| {
                (!available.contains(&binding.pcb_net)).then_some(signal.clone())
            })
            .collect::<Vec<_>>();
        let dropped = stale_signals.len();
        for signal in &stale_signals {
            self.bindings.remove(signal.as_str());
        }

        let mut rebound = 0usize;
        for signal in stale_signals {
            let suggestions =
                controller_signal_suggestions(self.selected_board, &signal, &available);
            if !should_auto_apply_suggestion(&suggestions) {
                continue;
            }
            let Some(best) = suggestions.first() else {
                continue;
            };
            self.bindings.insert(
                signal.clone(),
                SignalBinding {
                    board_signal: signal.clone(),
                    pcb_net: best.net_name.clone(),
                    mode: infer_binding_mode(&signal),
                    note: Some(format!(
                        "Rebound by arduino_simulator after PCB net refresh ({}, {})",
                        best.confidence_label(),
                        best.reason
                    )),
                },
            );
            rebound += 1;
        }

        (rebound, dropped)
    }

    fn binding_list(&self) -> Vec<SignalBinding> {
        self.bindings.values().cloned().collect()
    }

    fn add_pending_module_overlay(&mut self) {
        let model = self.pending_module_model.clone();
        let name = next_module_overlay_name(&model, &self.module_overlays);
        let mut module = ModuleOverlay {
            name: name.clone(),
            model,
            bindings: Vec::new(),
        };
        let wired = auto_wire_module_overlay(&mut module, self.loaded_pcb.as_ref());
        self.module_overlays.push(module);
        self.project_notice =
            format!("Added module overlay {name} and wired {wired} signal(s) to the current PCB.");
    }

    fn auto_wire_all_modules(&mut self) -> usize {
        let mut total = 0usize;
        for module in &mut self.module_overlays {
            total += auto_wire_module_overlay(module, self.loaded_pcb.as_ref());
        }
        total
    }

    fn build_project(&self) -> Result<SimulationProject, String> {
        let source_path = self.source_path.trim();
        let pcb_path = self.pcb_path.trim();
        let firmware_kind = match classify_source(source_path) {
            SourceAction::Compile => FirmwareSourceKind::Ino,
            SourceAction::LoadHex => FirmwareSourceKind::Hex,
            SourceAction::None => {
                return Err("Select a valid .ino sketch or .hex image first.".to_string())
            }
        };

        let project_name = if self.project_name.trim().is_empty() {
            default_project_name(source_path, pcb_path)
        } else {
            self.project_name.trim().to_string()
        };

        if pcb_path.is_empty() {
            return Err("Select a valid .kicad_pcb file first.".to_string());
        }

        let firmware = FirmwareSource {
            kind: firmware_kind,
            path: PathBuf::from(source_path),
            compiled_hex_path: if firmware_kind == FirmwareSourceKind::Ino {
                self.snapshot.firmware_path.clone()
            } else {
                None
            },
        };
        let pcb = PcbSource {
            path: PathBuf::from(pcb_path),
            board_name_hint: Path::new(pcb_path)
                .file_stem()
                .and_then(|value| value.to_str())
                .map(|value| value.to_string()),
        };
        let mut project = SimulationProject::new(project_name, self.selected_board, firmware, pcb);
        if !self.project_description.trim().is_empty() {
            project.description = Some(self.project_description.trim().to_string());
        }
        project.module_overlays = self.module_overlays.clone();
        project.bindings = self.binding_list();
        project.probes = self.project_probes.clone();
        project.stimuli = self.project_stimuli.clone();
        project.validate().map_err(|error| error.to_string())?;
        Ok(project)
    }

    fn save_project_dialog(&mut self) {
        match self.build_project() {
            Ok(project) => {
                let default_name = format!("{}.avrsim.json", sanitize_project_name(&project.name));
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(&default_name)
                    .save_file()
                {
                    match project.save_json(&path) {
                        Ok(()) => {
                            self.project_path = path.display().to_string();
                            self.project_name = project.name;
                            self.project_notice = format!("Saved project to {}", path.display());
                        }
                        Err(error) => {
                            self.project_notice = format!("Save failed: {error}");
                        }
                    }
                }
            }
            Err(error) => {
                self.project_notice = format!("Save blocked: {error}");
            }
        }
    }

    fn load_project_file(&mut self, path: &Path) {
        match SimulationProject::load_json(path) {
            Ok(project) => {
                if let Err(error) = project.validate() {
                    self.project_notice = format!("Project is invalid: {error}");
                    return;
                }
                self.apply_project(project, path);
            }
            Err(error) => {
                self.project_notice = format!("Open failed: {error}");
            }
        }
    }

    fn apply_project(&mut self, project: SimulationProject, path: &Path) {
        self.selected_board = project.host_board;
        self.project_name = project.name;
        self.project_path = path.display().to_string();
        self.project_description = project.description.unwrap_or_default();
        self.source_path = project.firmware.path.display().to_string();
        self.pcb_path = project.pcb.path.display().to_string();
        self.module_overlays = project.module_overlays;
        self.bindings = project
            .bindings
            .into_iter()
            .map(|binding| (binding.board_signal.clone(), binding))
            .collect();
        self.project_probes = project.probes;
        self.project_stimuli = project.stimuli;
        self.project_notice = format!(
            "Loaded project with {} binding(s), {} probe(s), and {} stimulus/stimuli.",
            self.bindings.len(),
            self.project_probes.len(),
            self.project_stimuli.len()
        );
        if project.pcb.path.is_file() {
            self.load_pcb_path(&project.pcb.path, false);
        }
    }

    fn show_serial_console(&mut self, ctx: &egui::Context) {
        let mut open = self.serial_console_open;
        egui::Window::new("Serial Console")
            .open(&mut open)
            .default_size([760.0, 520.0])
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Host Baud:");
                    egui::ComboBox::from_id_salt("serial_terminal_baud")
                        .selected_text(self.serial_terminal_baud.to_string())
                        .show_ui(ui, |ui| {
                            for baud in common_baud_rates() {
                                ui.selectable_value(
                                    &mut self.serial_terminal_baud,
                                    *baud,
                                    baud.to_string(),
                                );
                            }
                        });
                    ui.separator();
                    ui.label(format!(
                        "MCU UART: {}",
                        if self.snapshot.serial_configured_baud == 0 {
                            "not configured".to_string()
                        } else {
                            format!("{} baud", self.snapshot.serial_configured_baud)
                        }
                    ));
                    ui.separator();
                    ui.label(format!("RX queued: {}", self.snapshot.serial_rx_queued));
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let response = ui.add_sized(
                        [ui.available_width() - 180.0, 24.0],
                        egui::TextEdit::singleline(&mut self.serial_input)
                            .hint_text("Enter bytes/text to send to the simulated UART"),
                    );
                    let enter_pressed = response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if ui.button("Send").clicked() || enter_pressed {
                        self.send_serial_input();
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut self.serial_append_line_ending, "Append LF");
                    if ui.button("Clear Input").clicked() {
                        self.serial_input.clear();
                    }
                    if ui.button("Clear Output").clicked() {
                        self.controller.clear_serial();
                    }
                });

                ui.add_space(8.0);
                let serial = if self.snapshot.serial_text.is_empty() {
                    "<no serial output yet>".to_string()
                } else {
                    self.snapshot.serial_text.clone()
                };
                scrolled_text_block(
                    ui,
                    "serial_console_output",
                    serial,
                    ui.available_height().max(240.0),
                    true,
                );
            });
        self.serial_console_open = open;
    }

    fn send_serial_input(&mut self) {
        let trimmed = self.serial_input.clone();
        if trimmed.is_empty() {
            return;
        }
        let mut payload = trimmed.into_bytes();
        if self.serial_append_line_ending {
            payload.push(b'\n');
        }
        self.controller
            .inject_serial(payload, self.serial_terminal_baud);
        self.project_notice = format!("Sent serial input at {} baud.", self.serial_terminal_baud);
        self.serial_input.clear();
    }
}

fn scrolled_text_block(
    ui: &mut egui::Ui,
    id: &str,
    text: String,
    height: f32,
    stick_to_bottom: bool,
) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(id)
            .auto_shrink([false, false])
            .stick_to_bottom(stick_to_bottom)
            .max_height(height)
            .show(ui, |ui| {
                ui.set_min_height(height);
                ui.add(
                    egui::Label::new(RichText::new(text).monospace())
                        .wrap_mode(egui::TextWrapMode::Extend),
                );
            });
    });
}

fn code_block(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).monospace());
}

fn summary_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).strong());
    ui.monospace(value);
    ui.end_row();
}

fn displayed_host_board(selected_board: HostBoard, snapshot: &SimulationSnapshot) -> HostBoard {
    if snapshot.firmware_path.is_some() {
        snapshot.board
    } else {
        selected_board
    }
}

fn can_hot_swap_target_without_pcb(
    has_loaded_pcb: bool,
    source_path: &str,
    snapshot: &SimulationSnapshot,
) -> bool {
    !has_loaded_pcb
        && snapshot.firmware_path.is_some()
        && matches!(
            classify_source(source_path),
            SourceAction::Compile | SourceAction::LoadHex
        )
}

fn target_runtime_board_mismatch(selected_board: HostBoard, snapshot: &SimulationSnapshot) -> bool {
    snapshot.firmware_path.is_some() && snapshot.board != selected_board
}

fn inferred_host_board_from_source(path: &str) -> Option<HostBoard> {
    let normalized = path.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    if normalized.contains("atmega328") || normalized.contains("328p") {
        return Some(HostBoard::NanoV3);
    }
    if normalized.contains("atmega2560") || normalized.contains("2560") {
        return Some(HostBoard::Mega2560Rev3);
    }

    let looks_like_nano = normalized.contains("nano");
    let looks_like_mega = normalized.contains("/mega")
        || normalized.contains("_mega")
        || normalized.contains("mega_");

    match (looks_like_nano, looks_like_mega) {
        (true, false) => Some(HostBoard::NanoV3),
        (false, true) => Some(HostBoard::Mega2560Rev3),
        _ => None,
    }
}

fn host_signals_for_board(board: HostBoard) -> Vec<String> {
    load_built_in_board_model(board.builtin_board_model())
        .map(|board| board.nets.into_iter().map(|net| net.name).collect())
        .unwrap_or_default()
}

fn load_host_board_preview(board: HostBoard) -> Option<LoadedPcb> {
    match board {
        HostBoard::Mega2560Rev3 => {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../examples/pcbs/arduino_mega_2560_rev3e.kicad_pcb");
            LoadedPcb::load(&path)
                .ok()
                .map(LoadedPcb::simplified_preview)
                .or_else(|| {
                    load_built_in_board_model(board.builtin_board_model())
                        .ok()
                        .map(LoadedPcb::preview)
                })
        }
        HostBoard::NanoV3 => {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../examples/pcbs/arduino_nano_v3_3.kicad_pcb");
            LoadedPcb::load(&path)
                .ok()
                .map(LoadedPcb::simplified_preview)
                .or_else(|| {
                    load_built_in_board_model(board.builtin_board_model())
                        .ok()
                        .map(LoadedPcb::preview)
                })
        }
    }
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

fn status_color(status: SimulatorStatus) -> Color32 {
    match status {
        SimulatorStatus::Idle => Color32::LIGHT_GRAY,
        SimulatorStatus::Compiling => Color32::YELLOW,
        SimulatorStatus::Ready | SimulatorStatus::Paused => Color32::from_rgb(120, 180, 255),
        SimulatorStatus::Running => Color32::GREEN,
        SimulatorStatus::Break | SimulatorStatus::Sleep | SimulatorStatus::Done => {
            Color32::from_rgb(255, 180, 80)
        }
        SimulatorStatus::Error => Color32::RED,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceAction {
    Compile,
    LoadHex,
    None,
}

fn classify_source(path: &str) -> SourceAction {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return SourceAction::None;
    }

    let path = Path::new(trimmed);
    if path.is_dir() {
        return SourceAction::Compile;
    }

    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
    {
        Some(extension) if extension == "ino" => SourceAction::Compile,
        Some(extension) if extension == "hex" => SourceAction::LoadHex,
        _ => SourceAction::None,
    }
}

fn default_project_name(source_path: &str, pcb_path: &str) -> String {
    for candidate in [source_path, pcb_path] {
        if let Some(stem) = display_stem_for_path(candidate) {
            return stem;
        }
    }
    "Untitled Simulation".to_string()
}

fn display_stem_for_path(path: &str) -> Option<String> {
    let candidate = Path::new(path);
    let file_name = candidate.file_name().and_then(|value| value.to_str())?;
    let stem = strip_avrsim_suffix(file_name);
    if stem != file_name && !stem.trim().is_empty() {
        return Some(stem.to_string());
    }
    candidate
        .file_stem()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string())
}

fn strip_avrsim_suffix(name: &str) -> &str {
    name.strip_suffix(".board.avrsim.json")
        .or_else(|| name.strip_suffix(PROJECT_FILE_SUFFIX))
        .or_else(|| name.strip_suffix(".board.avrsim"))
        .or_else(|| name.strip_suffix(".avrsim"))
        .or_else(|| name.strip_suffix(".json"))
        .unwrap_or(name)
}

fn sanitize_project_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.trim_matches('-').is_empty() {
        "simulation-project".to_string()
    } else {
        sanitized
    }
}

fn common_baud_rates() -> &'static [u32] {
    &[
        300, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400,
    ]
}

fn infer_binding_mode(signal: &str) -> BindingMode {
    if signal.starts_with("A") || signal.contains("_PWM") || signal == "AREF" {
        return BindingMode::Analog;
    }
    if signal.contains("_SDA")
        || signal.contains("_SCL")
        || signal.contains("_MISO")
        || signal.contains("_MOSI")
        || signal.contains("_SCK")
        || signal.ends_with("_SS")
        || signal.ends_with("_RX")
        || signal.ends_with("_TX")
    {
        return BindingMode::Bus;
    }
    if signal == "GND"
        || signal == "+5V"
        || signal == "+3V3"
        || signal == "VIN"
        || signal == "IOREF"
        || signal == "RESET"
    {
        return BindingMode::Power;
    }
    BindingMode::Digital
}

fn binding_mode_label(mode: BindingMode) -> &'static str {
    match mode {
        BindingMode::Auto => "auto",
        BindingMode::Digital => "digital",
        BindingMode::Analog => "analog",
        BindingMode::Power => "power",
        BindingMode::Bus => "bus",
    }
}

fn candidate_pcb_nets(board: HostBoard, signal: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut push_unique = |candidate: String| {
        if !candidates.iter().any(|existing| existing == &candidate) {
            candidates.push(candidate);
        }
    };
    push_unique(signal.to_string());
    if let Some(number) = signal
        .strip_prefix('D')
        .and_then(|rest| rest.split('_').next())
        .filter(|value| {
            !value.is_empty() && value.chars().all(|character| character.is_ascii_digit())
        })
    {
        push_unique(format!("/{number}"));
        push_unique(format!("/*{number}"));
    }
    if let Some(number) = signal.strip_prefix('A').filter(|value| {
        !value.is_empty() && value.chars().all(|character| character.is_ascii_digit())
    }) {
        push_unique(format!("A{number}"));
        push_unique(format!("/A{number}"));
    }
    for alias in controller_signal_aliases(board, signal) {
        push_unique(alias);
    }
    candidates
}

fn preview_net_aliases(
    board: HostBoard,
    signal: &str,
    preview_nets: &BTreeSet<String>,
) -> Vec<String> {
    let mut aliases = Vec::new();
    let mut push_if_present = |candidate: String| {
        if preview_nets.contains(&candidate)
            && !aliases.iter().any(|existing| existing == &candidate)
        {
            aliases.push(candidate);
        }
    };

    push_if_present(signal.to_string());
    for alias in controller_signal_aliases(board, signal) {
        push_if_present(alias);
    }

    if aliases.is_empty() {
        aliases.push(signal.to_string());
    }

    aliases
}

fn controller_signal_aliases(board: HostBoard, signal: &str) -> Vec<String> {
    let mut aliases = controller_port_aliases(board, signal);
    aliases.extend(controller_board_label_aliases(board, signal));
    if let Some(adc_alias) = analog_channel_alias(signal) {
        aliases.push(adc_alias.clone());
        aliases.push(format!("/{adc_alias}"));
    }
    aliases
}

fn controller_board_label_aliases(board: HostBoard, signal: &str) -> Vec<String> {
    match board {
        HostBoard::Mega2560Rev3 => Vec::new(),
        HostBoard::NanoV3 => nano_controller_board_label_aliases(signal),
    }
}

fn controller_port_aliases(board: HostBoard, signal: &str) -> Vec<String> {
    match board {
        HostBoard::Mega2560Rev3 => mega_controller_port_aliases(signal),
        HostBoard::NanoV3 => nano_controller_port_aliases(signal),
    }
}

fn analog_channel_alias(signal: &str) -> Option<String> {
    parse_board_signal_number(signal, 'A').map(|number| format!("ADC{number}"))
}

fn standard_controller_signal_label(board: HostBoard, signal: &str) -> String {
    match board {
        HostBoard::Mega2560Rev3 => standard_mega_signal_label(signal),
        HostBoard::NanoV3 => standard_nano_signal_label(signal),
    }
}

fn standard_mega_signal_label(signal: &str) -> String {
    if let Some(adc_alias) = analog_channel_alias(signal) {
        return adc_alias;
    }
    controller_port_aliases(HostBoard::Mega2560Rev3, signal)
        .into_iter()
        .find(|alias| !alias.starts_with('/'))
        .unwrap_or_else(|| signal.to_string())
}

fn standard_nano_signal_label(signal: &str) -> String {
    match signal {
        "D0_RX" | "D0" => "D0/RX".to_string(),
        "D1_TX" | "D1" => "D1/TX".to_string(),
        "D10_SS" | "D10" => "D10/SS".to_string(),
        "D11_MOSI" | "D11" => "D11/MOSI".to_string(),
        "D12_MISO" | "D12" => "D12/MISO".to_string(),
        "D13_SCK" | "D13" => "D13/SCK".to_string(),
        "A4_SDA" | "A4" => "A4/SDA".to_string(),
        "A5_SCL" | "A5" => "A5/SCL".to_string(),
        _ => signal.to_string(),
    }
}

fn parse_board_signal_number(signal: &str, prefix: char) -> Option<u8> {
    let rest = signal.strip_prefix(prefix)?;
    let digits = rest.split('_').next()?;
    if digits.is_empty() || !digits.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn push_port_alias(aliases: &mut Vec<String>, port: &str) {
    aliases.push(port.to_string());
    aliases.push(format!("/{port}"));
}

fn mega_controller_port_aliases(signal: &str) -> Vec<String> {
    let mut aliases = Vec::new();

    if let Some(number) = parse_board_signal_number(signal, 'D') {
        let port = match number {
            0 => Some("PE0"),
            1 => Some("PE1"),
            2 => Some("PE4"),
            3 => Some("PE5"),
            4 => Some("PG5"),
            5 => Some("PE3"),
            6 => Some("PH3"),
            7 => Some("PH4"),
            8 => Some("PH5"),
            9 => Some("PH6"),
            10 => Some("PB4"),
            11 => Some("PB5"),
            12 => Some("PB6"),
            13 => Some("PB7"),
            14 => Some("PJ1"),
            15 => Some("PJ0"),
            16 => Some("PH1"),
            17 => Some("PH0"),
            18 => Some("PD3"),
            19 => Some("PD2"),
            20 => Some("PD1"),
            21 => Some("PD0"),
            22 => Some("PA0"),
            23 => Some("PA1"),
            24 => Some("PA2"),
            25 => Some("PA3"),
            26 => Some("PA4"),
            27 => Some("PA5"),
            28 => Some("PA6"),
            29 => Some("PA7"),
            30 => Some("PC7"),
            31 => Some("PC6"),
            32 => Some("PC5"),
            33 => Some("PC4"),
            34 => Some("PC3"),
            35 => Some("PC2"),
            36 => Some("PC1"),
            37 => Some("PC0"),
            38 => Some("PD7"),
            39 => Some("PG2"),
            40 => Some("PG1"),
            41 => Some("PG0"),
            42 => Some("PL7"),
            43 => Some("PL6"),
            44 => Some("PL5"),
            45 => Some("PL4"),
            46 => Some("PL3"),
            47 => Some("PL2"),
            48 => Some("PL1"),
            49 => Some("PL0"),
            50 => Some("PB3"),
            51 => Some("PB2"),
            52 => Some("PB1"),
            53 => Some("PB0"),
            _ => None,
        };
        if let Some(port) = port {
            push_port_alias(&mut aliases, port);
        }
    }

    if let Some(number) = parse_board_signal_number(signal, 'A') {
        let port = match number {
            0 => Some("PF0"),
            1 => Some("PF1"),
            2 => Some("PF2"),
            3 => Some("PF3"),
            4 => Some("PF4"),
            5 => Some("PF5"),
            6 => Some("PF6"),
            7 => Some("PF7"),
            8 => Some("PK0"),
            9 => Some("PK1"),
            10 => Some("PK2"),
            11 => Some("PK3"),
            12 => Some("PK4"),
            13 => Some("PK5"),
            14 => Some("PK6"),
            15 => Some("PK7"),
            _ => None,
        };
        if let Some(port) = port {
            push_port_alias(&mut aliases, port);
        }
    }

    aliases
}

fn nano_controller_port_aliases(signal: &str) -> Vec<String> {
    let mut aliases = Vec::new();

    if let Some(number) = parse_board_signal_number(signal, 'D') {
        let port = match number {
            0 => Some("PD0"),
            1 => Some("PD1"),
            2 => Some("PD2"),
            3 => Some("PD3"),
            4 => Some("PD4"),
            5 => Some("PD5"),
            6 => Some("PD6"),
            7 => Some("PD7"),
            8 => Some("PB0"),
            9 => Some("PB1"),
            10 => Some("PB2"),
            11 => Some("PB3"),
            12 => Some("PB4"),
            13 => Some("PB5"),
            _ => None,
        };
        if let Some(port) = port {
            push_port_alias(&mut aliases, port);
        }
    }

    if let Some(number) = parse_board_signal_number(signal, 'A') {
        let port = match number {
            0 => Some("PC0"),
            1 => Some("PC1"),
            2 => Some("PC2"),
            3 => Some("PC3"),
            4 => Some("PC4"),
            5 => Some("PC5"),
            _ => None,
        };
        if let Some(port) = port {
            push_port_alias(&mut aliases, port);
        }
    }

    aliases
}

fn nano_controller_board_label_aliases(signal: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    let mut push_label_aliases = |label: &str| {
        aliases.push(label.to_string());
        aliases.push(format!("/{label}"));
        aliases.push(format!("/{}", label.replace('/', "{slash}")));
    };

    match signal {
        "D0_RX" | "D0" => push_label_aliases("D0/RX"),
        "D1_TX" | "D1" => push_label_aliases("D1/TX"),
        "D10_SS" | "D10" => push_label_aliases("D10/SS"),
        "D11_MOSI" | "D11" => push_label_aliases("D11/MOSI"),
        "D12_MISO" | "D12" => push_label_aliases("D12/MISO"),
        "D13_SCK" | "D13" => push_label_aliases("D13/SCK"),
        "A4_SDA" | "A4" => push_label_aliases("A4/SDA"),
        "A5_SCL" | "A5" => push_label_aliases("A5/SCL"),
        _ => {}
    }

    aliases
}

fn canonical_signal_name(value: &str) -> String {
    let upper = value.to_ascii_uppercase().replace("{SLASH}", "_");
    let mut normalized = String::with_capacity(upper.len());
    let mut last_was_sep = false;
    for character in upper.chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character);
            last_was_sep = false;
        } else if !last_was_sep {
            normalized.push('_');
            last_was_sep = true;
        }
    }
    normalized.trim_matches('_').to_string()
}

fn suggest_pcb_nets(
    signal: &str,
    candidates: &[String],
    available_nets: &BTreeSet<String>,
) -> Vec<NetSuggestion> {
    let exact_candidates = candidates
        .iter()
        .map(|candidate| candidate.to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    let canonical_candidates = candidates
        .iter()
        .map(|candidate| canonical_signal_name(candidate))
        .collect::<BTreeSet<_>>();
    let candidate_tokens = collect_match_tokens(candidates);
    let candidate_numbers = collect_number_hints(signal, candidates);
    let signal_mode = infer_binding_mode(signal);

    let mut suggestions = available_nets
        .iter()
        .filter_map(|net| {
            let mut score = 0i32;
            let mut strongest_reason = "";
            let mut strongest_reason_score = 0i32;
            let mut bump = |points: i32, reason: &'static str| {
                if points <= 0 {
                    return;
                }
                score += points;
                if points > strongest_reason_score {
                    strongest_reason_score = points;
                    strongest_reason = reason;
                }
            };

            let net_upper = net.to_ascii_uppercase();
            let net_canonical = canonical_signal_name(net);
            if exact_candidates.contains(&net_upper) {
                bump(1200, "exact alias");
            }
            if canonical_candidates.contains(&net_canonical) {
                bump(950, "normalized name");
            }
            if canonical_candidates.iter().any(|candidate| {
                candidate.len() > 2
                    && !candidate
                        .chars()
                        .all(|character| character.is_ascii_digit())
                    && net_canonical.contains(candidate)
            }) {
                bump(180, "shared name");
            }

            let net_tokens = match_tokens(net);
            let role_overlap = candidate_tokens
                .iter()
                .filter(|token| {
                    !token.chars().all(|character| character.is_ascii_digit())
                        && net_tokens.contains(*token)
                })
                .count();
            if role_overlap > 0 {
                bump((role_overlap.min(2) as i32) * 140, "shared signal role");
            }

            let net_numbers = extract_number_hints(net);
            let mut best_number_points = 0i32;
            for (candidate_number, candidate_kind) in &candidate_numbers {
                for (net_number, net_kind) in &net_numbers {
                    if candidate_number != net_number {
                        continue;
                    }
                    let points = match (candidate_kind, net_kind) {
                        (left, right) if left == right && *left != NumericHintKind::Unknown => 320,
                        (NumericHintKind::Unknown, NumericHintKind::Unknown) => 220,
                        (NumericHintKind::Unknown, _) | (_, NumericHintKind::Unknown) => 260,
                        _ => 0,
                    };
                    best_number_points = best_number_points.max(points);
                }
            }
            if best_number_points > 0 {
                bump(best_number_points, "pin number");
            }

            let net_mode = infer_net_binding_mode(net);
            if signal_mode == net_mode {
                let points = match signal_mode {
                    BindingMode::Power => 120,
                    BindingMode::Bus => 100,
                    BindingMode::Analog => 90,
                    BindingMode::Digital => 60,
                    BindingMode::Auto => 0,
                };
                bump(points, "signal class");
            }

            if score > 0 {
                Some(NetSuggestion {
                    net_name: net.clone(),
                    score,
                    reason: if strongest_reason.is_empty() {
                        "heuristic match"
                    } else {
                        strongest_reason
                    },
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    suggestions.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.net_name.cmp(&right.net_name))
    });
    suggestions
}

fn controller_signal_suggestions(
    board: HostBoard,
    signal: &str,
    available_nets: &BTreeSet<String>,
) -> Vec<NetSuggestion> {
    suggest_pcb_nets(signal, &candidate_pcb_nets(board, signal), available_nets)
}

fn module_signal_suggestions(
    model: &str,
    signal: &str,
    available_nets: &BTreeSet<String>,
) -> Vec<NetSuggestion> {
    suggest_pcb_nets(
        signal,
        &module_signal_aliases(model, signal),
        available_nets,
    )
}

fn suggested_module_bindings(
    module: &ModuleOverlay,
    loaded_pcb: Option<&LoadedPcb>,
) -> Vec<ModuleSignalSuggestion> {
    let Some(loaded_pcb) = loaded_pcb else {
        return Vec::new();
    };
    let available_nets = loaded_pcb
        .net_names
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let bound_signals = module
        .bindings
        .iter()
        .map(|binding| binding.module_signal.clone())
        .collect::<BTreeSet<_>>();
    let mut suggestions = Vec::new();

    for signal in module_signal_names(&module.model) {
        if bound_signals.contains(&signal) {
            continue;
        }
        let Some(best) = module_signal_suggestions(&module.model, &signal, &available_nets)
            .into_iter()
            .next()
        else {
            continue;
        };
        if best.score < 200 {
            continue;
        }
        suggestions.push(ModuleSignalSuggestion {
            module_signal: signal,
            suggestion: best,
        });
    }

    suggestions.sort_by(|left, right| {
        right
            .suggestion
            .score
            .cmp(&left.suggestion.score)
            .then_with(|| left.module_signal.cmp(&right.module_signal))
    });
    suggestions
}

fn should_auto_apply_suggestion(suggestions: &[NetSuggestion]) -> bool {
    let Some(best) = suggestions.first() else {
        return false;
    };
    let second_score = suggestions
        .get(1)
        .map(|suggestion| suggestion.score)
        .unwrap_or(0);
    best.score >= 900
        || (best.score >= 450 && (best.score - second_score) >= 80)
        || (best.score >= 320 && (best.score - second_score) >= 140)
}

fn split_match_tokens(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut current_is_alpha: Option<bool> = None;

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            let is_alpha = character.is_ascii_alphabetic();
            if current_is_alpha.is_some_and(|previous| previous != is_alpha) && !current.is_empty()
            {
                tokens.push(std::mem::take(&mut current));
            }
            current.push(character.to_ascii_uppercase());
            current_is_alpha = Some(is_alpha);
        } else {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            current_is_alpha = None;
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn normalize_match_token(token: &str) -> String {
    match token {
        "SS" => "CS".to_string(),
        "CLK" => "SCK".to_string(),
        "SDI" | "SI" => "MOSI".to_string(),
        "SDO" | "SO" => "MISO".to_string(),
        "IRQ" => "INT".to_string(),
        "GROUND" | "GRND" | "VSS" => "GND".to_string(),
        "VDD" => "VCC".to_string(),
        _ => token.to_string(),
    }
}

fn is_noise_match_token(token: &str) -> bool {
    matches!(
        token,
        "D" | "A" | "GPIO" | "IO" | "PIN" | "PAD" | "NET" | "SIG" | "SIGNAL" | "PORT"
    )
}

fn match_tokens(value: &str) -> BTreeSet<String> {
    split_match_tokens(value)
        .into_iter()
        .map(|token| normalize_match_token(&token))
        .filter(|token| !token.is_empty() && !is_noise_match_token(token))
        .collect()
}

fn collect_match_tokens(candidates: &[String]) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    for candidate in candidates {
        tokens.extend(match_tokens(candidate));
    }
    tokens
}

fn extract_number_hints(value: &str) -> Vec<(String, NumericHintKind)> {
    let tokens = split_match_tokens(value);
    let mut hints = Vec::new();
    let mut seen = BTreeSet::new();

    for (index, token) in tokens.iter().enumerate() {
        if !token.chars().all(|character| character.is_ascii_digit()) {
            continue;
        }
        let previous = index
            .checked_sub(1)
            .map(|offset| normalize_match_token(&tokens[offset]));
        let kind = match previous.as_deref() {
            Some("A") | Some("ADC") | Some("AN") => NumericHintKind::Analog,
            Some("D") | Some("GPIO") | Some("IO") | Some("PIN") => NumericHintKind::Digital,
            _ => NumericHintKind::Unknown,
        };
        if seen.insert((token.clone(), kind)) {
            hints.push((token.clone(), kind));
        }
    }

    hints
}

fn collect_number_hints(signal: &str, candidates: &[String]) -> Vec<(String, NumericHintKind)> {
    let mut hints = extract_number_hints(signal);
    let mut seen = hints.iter().cloned().collect::<BTreeSet<_>>();
    for candidate in candidates {
        for hint in extract_number_hints(candidate) {
            if seen.insert(hint.clone()) {
                hints.push(hint);
            }
        }
    }
    hints
}

fn infer_net_binding_mode(name: &str) -> BindingMode {
    let upper = name.trim_start_matches('/').to_ascii_uppercase();
    if upper == "GND" || upper.contains("GROUND") {
        return BindingMode::Power;
    }
    if upper.starts_with('+')
        || upper.contains("VCC")
        || upper.contains("VDD")
        || upper.contains("VIN")
        || upper.contains("24V")
        || upper.contains("12V")
        || upper.contains("5V")
        || upper.contains("3V3")
        || upper.contains("IOREF")
    {
        return BindingMode::Power;
    }
    if upper.starts_with('A') || upper.contains("ADC") || upper.contains("_RAW") {
        return BindingMode::Analog;
    }
    if upper.contains("SDA")
        || upper.contains("SCL")
        || upper.contains("MISO")
        || upper.contains("MOSI")
        || upper.contains("SCK")
        || upper.contains("CLK")
        || upper.contains("SPI")
        || upper.contains("I2C")
        || upper.contains("UART")
        || upper.contains("CAN")
        || upper.contains("IRQ")
        || upper.contains("INT")
        || upper.contains("CS")
        || upper.ends_with("_RX")
        || upper.ends_with("_TX")
    {
        return BindingMode::Bus;
    }
    BindingMode::Digital
}

fn available_module_models() -> Vec<&'static str> {
    built_in_board_model_names()
        .into_iter()
        .filter(|model| !model.starts_with("arduino_"))
        .collect()
}

fn module_model_title(model: &str) -> String {
    load_built_in_board_model(model)
        .ok()
        .and_then(|board| board.title.or(Some(board.name)))
        .unwrap_or_else(|| model.to_string())
}

fn next_module_overlay_name(model: &str, existing: &[ModuleOverlay]) -> String {
    let base = model
        .strip_suffix("_module")
        .or_else(|| model.strip_suffix("_board"))
        .unwrap_or(model)
        .to_string();
    let mut index = 1usize;
    loop {
        let candidate = format!("{base}_{index}");
        if existing.iter().all(|overlay| overlay.name != candidate) {
            return candidate;
        }
        index += 1;
    }
}

fn module_signal_names(model: &str) -> Vec<String> {
    load_built_in_board_model(model)
        .map(|board| {
            let mut names = board
                .nets
                .into_iter()
                .map(|net| net.name)
                .collect::<Vec<_>>();
            names.sort();
            names
        })
        .unwrap_or_default()
}

fn module_signal_aliases(model: &str, signal: &str) -> Vec<String> {
    let mut aliases = vec![signal.to_string()];
    let upper = signal.to_ascii_uppercase();
    let push_unique = |values: &mut Vec<String>, value: &str| {
        if !values
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(value))
        {
            values.push(value.to_string());
        }
    };

    match upper.as_str() {
        "VCC" => {
            for alias in ["+5V", "5V", "VCC", "+3V3", "3V3"] {
                push_unique(&mut aliases, alias);
            }
        }
        "GND" => {
            push_unique(&mut aliases, "GND");
        }
        "SDA" => {
            for alias in ["A4_SDA", "A4", "D20_SDA", "D20", "/A4", "/20", "/*20"] {
                push_unique(&mut aliases, alias);
            }
        }
        "SCL" => {
            for alias in ["A5_SCL", "A5", "D21_SCL", "D21", "/A5", "/21", "/*21"] {
                push_unique(&mut aliases, alias);
            }
        }
        "SCK" | "CLK" => {
            for alias in [
                "D13_SCK", "D13", "D52_SCK", "D52", "/13", "/*13", "/52", "/*52",
            ] {
                push_unique(&mut aliases, alias);
            }
        }
        "SI" | "SDI" | "MOSI" => {
            for alias in [
                "D11_MOSI", "D11", "D51_MOSI", "D51", "/11", "/*11", "/51", "/*51",
            ] {
                push_unique(&mut aliases, alias);
            }
        }
        "SO" | "SDO" | "MISO" => {
            for alias in [
                "D12_MISO", "D12", "D50_MISO", "D50", "/12", "/*12", "/50", "/*50",
            ] {
                push_unique(&mut aliases, alias);
            }
        }
        "CS" => {
            for alias in [
                "D10_SS", "D10", "D53_SS", "D53", "D27", "D26", "/10", "/*10", "/53", "/*53",
                "/27", "/*27", "/26", "/*26",
            ] {
                push_unique(&mut aliases, alias);
            }
        }
        "INT" => {
            for alias in ["D2", "D28", "/2", "/*2", "/28", "/*28", "INT"] {
                push_unique(&mut aliases, alias);
            }
        }
        "CANH" => {
            for alias in ["CANH", "CAN_H"] {
                push_unique(&mut aliases, alias);
            }
        }
        "CANL" => {
            for alias in ["CANL", "CAN_L"] {
                push_unique(&mut aliases, alias);
            }
        }
        "PWM" => {
            for alias in ["D44_PWM", "D44", "PWM", "/44", "/*44"] {
                push_unique(&mut aliases, alias);
            }
        }
        "VOUT" => {
            for alias in ["VOUT", "Y", "ACT_Y", "/ACT_Y"] {
                push_unique(&mut aliases, alias);
            }
        }
        _ => {}
    }

    if model == "mcp2515_tja1050_can_module" && upper == "CS" {
        for alias in ["MCP2515_CS", "/MCP2515_CS"] {
            push_unique(&mut aliases, alias);
        }
    }
    if model == "max31865_breakout" && upper == "CS" {
        for alias in ["MAX31865_CS", "/MAX31865_CS"] {
            push_unique(&mut aliases, alias);
        }
    }

    aliases
}

fn auto_wire_module_overlay(module: &mut ModuleOverlay, loaded_pcb: Option<&LoadedPcb>) -> usize {
    let Some(loaded_pcb) = loaded_pcb else {
        module.bindings.clear();
        return 0;
    };
    let available_nets = loaded_pcb
        .net_names
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let existing_by_signal = module
        .bindings
        .iter()
        .map(|binding| (binding.module_signal.clone(), binding.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut wired = 0usize;
    let mut next_bindings = Vec::new();

    for signal in module_signal_names(&module.model) {
        let mode = infer_binding_mode(&signal);
        let suggestions = module_signal_suggestions(&module.model, &signal, &available_nets);
        let existing = existing_by_signal.get(&signal);
        let auto_note = suggestions.first().and_then(|best| {
            should_auto_apply_suggestion(&suggestions).then(|| {
                format!(
                    "Auto-wired by arduino_simulator ({}, {})",
                    best.confidence_label(),
                    best.reason
                )
            })
        });
        let matched_net = auto_note
            .as_ref()
            .and_then(|_| {
                suggestions
                    .first()
                    .map(|suggestion| suggestion.net_name.clone())
            })
            .or_else(|| existing.map(|binding| binding.pcb_net.clone()));
        let Some(pcb_net) = matched_net else {
            continue;
        };
        next_bindings.push(ModuleSignalBinding {
            module_signal: signal,
            pcb_net,
            mode,
            note: auto_note.or_else(|| existing.and_then(|binding| binding.note.clone())),
        });
        wired += 1;
    }

    module.bindings = next_bindings;
    wired
}

fn host_signal_levels_for_snapshot(snapshot: &SimulationSnapshot) -> BTreeMap<String, u8> {
    let mut levels = BTreeMap::new();
    for entry in &snapshot.host_pin_levels {
        for signal in host_signal_names(snapshot.board, entry.pin) {
            levels.insert(signal.to_string(), entry.level);
        }
    }
    levels
}

fn host_signal_names(board: HostBoard, pin: BoardPin) -> &'static [&'static str] {
    match board {
        HostBoard::NanoV3 => nano_host_signal_names(pin),
        HostBoard::Mega2560Rev3 => mega_host_signal_names(pin),
    }
}

fn nano_host_signal_names(pin: BoardPin) -> &'static [&'static str] {
    match pin {
        BoardPin::Digital(0) => &["D0_RX", "D0"],
        BoardPin::Digital(1) => &["D1_TX", "D1"],
        BoardPin::Digital(2) => &["D2"],
        BoardPin::Digital(3) => &["D3_PWM", "D3"],
        BoardPin::Digital(4) => &["D4"],
        BoardPin::Digital(5) => &["D5_PWM", "D5"],
        BoardPin::Digital(6) => &["D6_PWM", "D6"],
        BoardPin::Digital(7) => &["D7"],
        BoardPin::Digital(8) => &["D8"],
        BoardPin::Digital(9) => &["D9_PWM", "D9"],
        BoardPin::Digital(10) => &["D10_SS", "D10"],
        BoardPin::Digital(11) => &["D11_MOSI", "D11"],
        BoardPin::Digital(12) => &["D12_MISO", "D12"],
        BoardPin::Digital(13) => &["D13_SCK", "D13"],
        BoardPin::Analog(0) => &["A0"],
        BoardPin::Analog(1) => &["A1"],
        BoardPin::Analog(2) => &["A2"],
        BoardPin::Analog(3) => &["A3"],
        BoardPin::Analog(4) => &["A4_SDA", "A4"],
        BoardPin::Analog(5) => &["A5_SCL", "A5"],
        BoardPin::Analog(6) => &["A6"],
        BoardPin::Analog(7) => &["A7"],
        _ => &[],
    }
}

fn mega_host_signal_names(pin: BoardPin) -> &'static [&'static str] {
    match pin {
        BoardPin::Digital(0) => &["D0_RX0", "D0"],
        BoardPin::Digital(1) => &["D1_TX0", "D1"],
        BoardPin::Digital(2) => &["D2"],
        BoardPin::Digital(3) => &["D3_PWM", "D3"],
        BoardPin::Digital(4) => &["D4"],
        BoardPin::Digital(5) => &["D5_PWM", "D5"],
        BoardPin::Digital(6) => &["D6_PWM", "D6"],
        BoardPin::Digital(7) => &["D7"],
        BoardPin::Digital(8) => &["D8"],
        BoardPin::Digital(9) => &["D9_PWM", "D9"],
        BoardPin::Digital(10) => &["D10_PWM", "D10"],
        BoardPin::Digital(11) => &["D11_PWM", "D11"],
        BoardPin::Digital(12) => &["D12"],
        BoardPin::Digital(13) => &["D13"],
        BoardPin::Digital(14) => &["D14_TX3", "D14"],
        BoardPin::Digital(15) => &["D15_RX3", "D15"],
        BoardPin::Digital(16) => &["D16_TX2", "D16"],
        BoardPin::Digital(17) => &["D17_RX2", "D17"],
        BoardPin::Digital(18) => &["D18_TX1", "D18"],
        BoardPin::Digital(19) => &["D19_RX1", "D19"],
        BoardPin::Digital(20) => &["D20_SDA", "D20"],
        BoardPin::Digital(21) => &["D21_SCL", "D21"],
        BoardPin::Digital(22) => &["D22"],
        BoardPin::Digital(23) => &["D23"],
        BoardPin::Digital(24) => &["D24"],
        BoardPin::Digital(25) => &["D25"],
        BoardPin::Digital(26) => &["D26"],
        BoardPin::Digital(27) => &["D27"],
        BoardPin::Digital(28) => &["D28"],
        BoardPin::Digital(29) => &["D29"],
        BoardPin::Digital(30) => &["D30"],
        BoardPin::Digital(31) => &["D31"],
        BoardPin::Digital(32) => &["D32"],
        BoardPin::Digital(33) => &["D33"],
        BoardPin::Digital(34) => &["D34"],
        BoardPin::Digital(35) => &["D35"],
        BoardPin::Digital(36) => &["D36"],
        BoardPin::Digital(37) => &["D37"],
        BoardPin::Digital(38) => &["D38"],
        BoardPin::Digital(39) => &["D39"],
        BoardPin::Digital(40) => &["D40"],
        BoardPin::Digital(41) => &["D41"],
        BoardPin::Digital(42) => &["D42"],
        BoardPin::Digital(43) => &["D43"],
        BoardPin::Digital(44) => &["D44_PWM", "D44"],
        BoardPin::Digital(45) => &["D45_PWM", "D45"],
        BoardPin::Digital(46) => &["D46_PWM", "D46"],
        BoardPin::Digital(47) => &["D47"],
        BoardPin::Digital(48) => &["D48"],
        BoardPin::Digital(49) => &["D49"],
        BoardPin::Digital(50) => &["D50_MISO", "D50"],
        BoardPin::Digital(51) => &["D51_MOSI", "D51"],
        BoardPin::Digital(52) => &["D52_SCK", "D52"],
        BoardPin::Digital(53) => &["D53_SS", "D53"],
        BoardPin::Analog(0) => &["A0"],
        BoardPin::Analog(1) => &["A1"],
        BoardPin::Analog(2) => &["A2"],
        BoardPin::Analog(3) => &["A3"],
        BoardPin::Analog(4) => &["A4"],
        BoardPin::Analog(5) => &["A5"],
        BoardPin::Analog(6) => &["A6"],
        BoardPin::Analog(7) => &["A7"],
        BoardPin::Analog(8) => &["A8"],
        BoardPin::Analog(9) => &["A9"],
        BoardPin::Analog(10) => &["A10"],
        BoardPin::Analog(11) => &["A11"],
        BoardPin::Analog(12) => &["A12"],
        BoardPin::Analog(13) => &["A13"],
        BoardPin::Analog(14) => &["A14"],
        BoardPin::Analog(15) => &["A15"],
        _ => &[],
    }
}

fn connectable_pin_indicator_color(activity: SignalActivity, pulse_time: f32) -> Color32 {
    if activity.is_flashing {
        let pulse = (((pulse_time * 7.5).sin() + 1.0) * 0.5).clamp(0.0, 1.0);
        let low = egui::Rgba::from_rgb(0.65, 0.08, 0.08);
        let high = egui::Rgba::from_rgb(1.0, 0.2, 0.2);
        return Color32::from(egui::lerp(low..=high, pulse));
    }
    if activity.is_high {
        return Color32::from_rgb(220, 72, 72);
    }
    Color32::from_rgb(96, 210, 120)
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, path::PathBuf};

    use rust_board::load_built_in_board_model;
    use rust_mcu::{BoardPin, BoardPinLevel};
    use rust_project::{BindingMode, HostBoard, ModuleOverlay};

    use super::{
        auto_wire_module_overlay, available_module_models, can_hot_swap_target_without_pcb,
        candidate_pcb_nets, classify_source, common_baud_rates, connectable_pin_indicator_color,
        controller_signal_suggestions, default_project_name, display_stem_for_path,
        displayed_host_board, host_signal_levels_for_snapshot, host_signal_names,
        infer_binding_mode, inferred_host_board_from_source, module_model_title,
        module_signal_aliases, module_signal_suggestions, next_module_overlay_name,
        sanitize_project_name, should_auto_apply_suggestion, standard_controller_signal_label,
        target_runtime_board_mismatch, AvrSimGuiApp, ControllerConnection, SignalActivity,
        SourceAction,
    };
    use crate::simulation::{SimulationSnapshot, SimulatorStatus};

    #[test]
    fn classify_source_detects_sketches_and_hex_files() {
        assert_eq!(classify_source(""), SourceAction::None);
        assert_eq!(classify_source("/tmp/hello.ino"), SourceAction::Compile);
        assert_eq!(classify_source("/tmp/hello.hex"), SourceAction::LoadHex);
        assert_eq!(classify_source("/tmp/hello.txt"), SourceAction::None);
    }

    #[test]
    fn project_name_helpers_are_stable() {
        assert_eq!(default_project_name("/tmp/dewpoint.ino", ""), "dewpoint");
        assert_eq!(default_project_name("", "/tmp/board.kicad_pcb"), "board");
        assert_eq!(
            default_project_name("", "/tmp/main-controller.board.avrsim.json"),
            "main-controller"
        );
        assert_eq!(
            display_stem_for_path("/tmp/controller.board.avrsim"),
            Some("controller".to_string())
        );
        assert_eq!(
            sanitize_project_name("Main Controller Rev A"),
            "Main-Controller-Rev-A"
        );
    }

    #[test]
    fn displayed_host_board_prefers_selected_target_until_firmware_is_loaded() {
        let snapshot = SimulationSnapshot::default();
        assert_eq!(
            displayed_host_board(HostBoard::NanoV3, &snapshot),
            HostBoard::NanoV3
        );

        let mut loaded = SimulationSnapshot::default();
        loaded.board = HostBoard::Mega2560Rev3;
        loaded.firmware_path = Some(PathBuf::from("/tmp/blink.hex"));
        assert_eq!(
            displayed_host_board(HostBoard::NanoV3, &loaded),
            HostBoard::Mega2560Rev3
        );
    }

    #[test]
    fn inferred_host_board_detects_common_source_name_hints() {
        assert_eq!(
            inferred_host_board_from_source("/tmp/nano_pin_sweep.ino"),
            Some(HostBoard::NanoV3)
        );
        assert_eq!(
            inferred_host_board_from_source("/tmp/mega_pin_sweep.ino"),
            Some(HostBoard::Mega2560Rev3)
        );
        assert_eq!(
            inferred_host_board_from_source("/tmp/atmega328p_blink.hex"),
            Some(HostBoard::NanoV3)
        );
        assert_eq!(
            inferred_host_board_from_source("/tmp/atmega2560_blink.hex"),
            Some(HostBoard::Mega2560Rev3)
        );
        assert_eq!(inferred_host_board_from_source("/tmp/controller.ino"), None);
    }

    #[test]
    fn target_runtime_mismatch_and_hot_swap_rules_are_explicit() {
        let snapshot = SimulationSnapshot::default();
        assert!(!target_runtime_board_mismatch(HostBoard::NanoV3, &snapshot));
        assert!(!can_hot_swap_target_without_pcb(
            false,
            "/tmp/nano.ino",
            &snapshot
        ));

        let mut loaded = SimulationSnapshot::default();
        loaded.board = HostBoard::Mega2560Rev3;
        loaded.firmware_path = Some(PathBuf::from("/tmp/blink.hex"));

        assert!(target_runtime_board_mismatch(HostBoard::NanoV3, &loaded));
        assert!(can_hot_swap_target_without_pcb(
            false,
            "/tmp/nano.ino",
            &loaded
        ));
        assert!(!can_hot_swap_target_without_pcb(
            true,
            "/tmp/nano.ino",
            &loaded
        ));
        assert!(!can_hot_swap_target_without_pcb(
            false,
            "/tmp/readme.txt",
            &loaded
        ));
    }

    #[test]
    fn pcb_alias_candidates_cover_common_header_nets() {
        assert_eq!(
            candidate_pcb_nets(HostBoard::Mega2560Rev3, "D27"),
            vec!["D27", "/27", "/*27", "PA5", "/PA5"]
        );
        assert_eq!(
            candidate_pcb_nets(HostBoard::Mega2560Rev3, "A10"),
            vec!["A10", "/A10", "PK2", "/PK2", "ADC10", "/ADC10"]
        );
        assert_eq!(
            candidate_pcb_nets(HostBoard::NanoV3, "D13_SCK"),
            vec![
                "D13_SCK",
                "/13",
                "/*13",
                "PB5",
                "/PB5",
                "D13/SCK",
                "/D13/SCK",
                "/D13{slash}SCK"
            ]
        );
    }

    #[test]
    fn controller_suggestions_can_match_generic_gpio_nets() {
        let available = ["GPIO27".to_string(), "GPIO28".to_string()]
            .into_iter()
            .collect::<BTreeSet<_>>();
        let suggestions = controller_signal_suggestions(HostBoard::Mega2560Rev3, "D27", &available);
        assert_eq!(
            suggestions
                .first()
                .map(|suggestion| suggestion.net_name.as_str()),
            Some("GPIO27")
        );
        assert!(should_auto_apply_suggestion(&suggestions));
    }

    #[test]
    fn controller_suggestions_can_match_generic_adc_nets() {
        let available = ["ADC10".to_string(), "ADC11".to_string()]
            .into_iter()
            .collect::<BTreeSet<_>>();
        let suggestions = controller_signal_suggestions(HostBoard::Mega2560Rev3, "A10", &available);
        assert_eq!(
            suggestions
                .first()
                .map(|suggestion| suggestion.net_name.as_str()),
            Some("ADC10")
        );
        assert!(should_auto_apply_suggestion(&suggestions));
    }

    #[test]
    fn controller_suggestions_can_match_official_mega_port_net_names() {
        let available = ["/PA5".to_string(), "/PK2".to_string()]
            .into_iter()
            .collect::<BTreeSet<_>>();
        let digital = controller_signal_suggestions(HostBoard::Mega2560Rev3, "D27", &available);
        assert_eq!(
            digital
                .first()
                .map(|suggestion| suggestion.net_name.as_str()),
            Some("/PA5")
        );
        let analog = controller_signal_suggestions(HostBoard::Mega2560Rev3, "A10", &available);
        assert_eq!(
            analog
                .first()
                .map(|suggestion| suggestion.net_name.as_str()),
            Some("/PK2")
        );
    }

    #[test]
    fn standard_controller_signal_labels_follow_board_preview_conventions() {
        assert_eq!(
            standard_controller_signal_label(HostBoard::Mega2560Rev3, "A11"),
            "ADC11"
        );
        assert_eq!(
            standard_controller_signal_label(HostBoard::Mega2560Rev3, "D30"),
            "PC7"
        );
        assert_eq!(
            standard_controller_signal_label(HostBoard::NanoV3, "D13_SCK"),
            "D13/SCK"
        );
    }

    #[test]
    fn module_suggestions_can_match_protocol_role_names() {
        let available = ["I2C_SDA".to_string(), "I2C_SCL".to_string()]
            .into_iter()
            .collect::<BTreeSet<_>>();
        let suggestions = module_signal_suggestions("gy_sht31_d", "SDA", &available);
        assert_eq!(
            suggestions
                .first()
                .map(|suggestion| suggestion.net_name.as_str()),
            Some("I2C_SDA")
        );
    }

    #[test]
    fn binding_modes_are_inferred_sensibly() {
        assert_eq!(infer_binding_mode("D27"), BindingMode::Digital);
        assert_eq!(infer_binding_mode("D44_PWM"), BindingMode::Analog);
        assert_eq!(infer_binding_mode("D50_MISO"), BindingMode::Bus);
        assert_eq!(infer_binding_mode("+5V"), BindingMode::Power);
    }

    #[test]
    fn baud_rate_list_is_sorted_and_contains_common_terminal_speed() {
        let rates = common_baud_rates();
        assert!(rates.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(rates.contains(&115_200));
    }

    #[test]
    fn module_model_helpers_expose_non_controller_modules() {
        let models = available_module_models();
        assert!(models.contains(&"gy_sht31_d"));
        assert!(models.contains(&"mcp2515_tja1050_can_module"));
        assert!(!models.contains(&"arduino_nano_v3"));
        assert!(module_model_title("gy_sht31_d").contains("SHT31"));
    }

    #[test]
    fn host_signal_names_cover_controller_aliases() {
        assert_eq!(
            host_signal_names(HostBoard::NanoV3, BoardPin::Digital(13)),
            &["D13_SCK", "D13"]
        );
        assert_eq!(
            host_signal_names(HostBoard::NanoV3, BoardPin::Analog(4)),
            &["A4_SDA", "A4"]
        );
        assert_eq!(
            host_signal_names(HostBoard::Mega2560Rev3, BoardPin::Digital(44)),
            &["D44_PWM", "D44"]
        );
        assert_eq!(
            host_signal_names(HostBoard::Mega2560Rev3, BoardPin::Digital(50)),
            &["D50_MISO", "D50"]
        );
    }

    #[test]
    fn snapshot_host_pin_levels_expand_to_named_host_signals() {
        let mut snapshot = SimulationSnapshot::default();
        snapshot.board = HostBoard::NanoV3;
        snapshot.status = SimulatorStatus::Running;
        snapshot.host_pin_levels = vec![
            BoardPinLevel {
                pin: BoardPin::Digital(13),
                level: 1,
            },
            BoardPinLevel {
                pin: BoardPin::Analog(4),
                level: 1,
            },
        ];
        let levels = host_signal_levels_for_snapshot(&snapshot);
        assert_eq!(levels.get("D13_SCK"), Some(&1));
        assert_eq!(levels.get("D13"), Some(&1));
        assert_eq!(levels.get("A4_SDA"), Some(&1));
        assert_eq!(levels.get("A4"), Some(&1));
    }

    #[test]
    fn connectable_pin_indicator_prefers_flash_over_steady_high() {
        let flashing = connectable_pin_indicator_color(
            SignalActivity {
                is_high: true,
                is_flashing: true,
            },
            0.25,
        );
        let steady = connectable_pin_indicator_color(
            SignalActivity {
                is_high: true,
                is_flashing: false,
            },
            0.25,
        );
        assert_ne!(flashing, steady);
    }

    #[test]
    fn module_overlay_names_increment_cleanly() {
        let existing = vec![ModuleOverlay {
            name: "gy_sht31_d_1".to_string(),
            model: "gy_sht31_d".to_string(),
            bindings: Vec::new(),
        }];
        assert_eq!(
            next_module_overlay_name("gy_sht31_d", &existing),
            "gy_sht31_d_2"
        );
    }

    #[test]
    fn controller_connections_are_derived_from_controller_bindings() {
        let mut app = AvrSimGuiApp::default();
        app.bindings.insert(
            "D10_SS".to_string(),
            rust_project::SignalBinding {
                board_signal: "D10_SS".to_string(),
                pcb_net: "/10".to_string(),
                mode: BindingMode::Bus,
                note: None,
            },
        );

        assert_eq!(
            app.controller_connections(),
            vec![ControllerConnection {
                controller_pin: "D10_SS".to_string(),
                pcb_net: "/10".to_string(),
                mode: BindingMode::Bus,
            }]
        );
    }

    #[test]
    fn host_board_preview_uses_preview_net_aliases_for_real_mega_board() {
        let mut app = AvrSimGuiApp::default();
        let preview_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/pcbs/arduino_mega_2560_rev3e.kicad_pcb");
        let preview = crate::pcb_view::LoadedPcb::load(&preview_path)
            .expect("mega preview")
            .simplified_preview();
        app.bindings.insert(
            "D31".to_string(),
            rust_project::SignalBinding {
                board_signal: "D31".to_string(),
                pcb_net: "D31".to_string(),
                mode: BindingMode::Digital,
                note: None,
            },
        );
        app.host_signal_levels.insert("D31".to_string(), 1);

        let bindings = app.host_board_preview_bindings(HostBoard::Mega2560Rev3, &preview);
        assert!(bindings
            .iter()
            .any(|binding| binding.board_signal == "D31" && binding.pcb_net == "PC6"));

        let active = app.active_host_preview_nets(HostBoard::Mega2560Rev3, &preview);
        assert!(active.contains("PC6"));
    }

    #[test]
    fn host_board_preview_uses_preview_net_aliases_for_real_nano_board() {
        let mut app = AvrSimGuiApp::default();
        let preview_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/pcbs/arduino_nano_v3_3.kicad_pcb");
        let preview = crate::pcb_view::LoadedPcb::load(&preview_path)
            .expect("nano preview")
            .simplified_preview();
        app.bindings.insert(
            "D13_SCK".to_string(),
            rust_project::SignalBinding {
                board_signal: "D13_SCK".to_string(),
                pcb_net: "/D13{slash}SCK".to_string(),
                mode: BindingMode::Bus,
                note: None,
            },
        );
        app.host_signal_levels.insert("D13_SCK".to_string(), 1);

        let bindings = app.host_board_preview_bindings(HostBoard::NanoV3, &preview);
        assert!(bindings
            .iter()
            .any(|binding| binding.board_signal == "D13_SCK" && binding.pcb_net == "D13/SCK"));

        let active = app.active_host_preview_nets(HostBoard::NanoV3, &preview);
        assert!(active.contains("D13/SCK"));
    }

    #[test]
    fn reconcile_controller_bindings_rebinds_stale_project_nets() {
        let mut app = AvrSimGuiApp::default();
        let board = load_built_in_board_model(HostBoard::Mega2560Rev3.builtin_board_model())
            .expect("mega board");
        app.loaded_pcb = Some(crate::pcb_view::LoadedPcb::preview(board));
        app.bindings.insert(
            "D22".to_string(),
            rust_project::SignalBinding {
                board_signal: "D22".to_string(),
                pcb_net: "/22".to_string(),
                mode: BindingMode::Digital,
                note: Some("legacy".to_string()),
            },
        );
        app.bindings.insert(
            "A8".to_string(),
            rust_project::SignalBinding {
                board_signal: "A8".to_string(),
                pcb_net: "/A8".to_string(),
                mode: BindingMode::Analog,
                note: Some("legacy".to_string()),
            },
        );

        let (rebound, dropped) = app.reconcile_controller_bindings();
        assert_eq!((rebound, dropped), (2, 2));
        assert_eq!(
            app.bindings
                .get("D22")
                .map(|binding| binding.pcb_net.as_str()),
            Some("D22")
        );
        assert_eq!(
            app.bindings
                .get("A8")
                .map(|binding| binding.pcb_net.as_str()),
            Some("A8")
        );
    }

    #[test]
    fn module_aliases_cover_common_spi_and_can_nets() {
        let aliases = module_signal_aliases("mcp2515_tja1050_can_module", "CS");
        assert!(aliases.contains(&"/10".to_string()));
        assert!(aliases.contains(&"/27".to_string()));
        let canh = module_signal_aliases("mcp2515_tja1050_can_module", "CANH");
        assert!(canh.contains(&"CAN_H".to_string()));
    }

    #[test]
    fn module_autowire_uses_loaded_pcb_nets() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/pcbs/air_node.kicad_pcb");
        let loaded = crate::pcb_view::LoadedPcb::load(&path).expect("pcb");
        let mut module = ModuleOverlay {
            name: "can_1".to_string(),
            model: "mcp2515_tja1050_can_module".to_string(),
            bindings: Vec::new(),
        };
        let wired = auto_wire_module_overlay(&mut module, Some(&loaded));
        assert!(wired >= 5);
        assert!(module
            .bindings
            .iter()
            .any(|binding| binding.module_signal == "CANH"));
        assert!(module
            .bindings
            .iter()
            .any(|binding| binding.module_signal == "CANL"));
    }

    #[test]
    fn controller_signal_status_text_reports_signal_level() {
        let mut app = AvrSimGuiApp::default();
        app.host_signal_levels.insert("D10_SS".to_string(), 1);
        assert_eq!(app.controller_signal_status_text("D10_SS"), "high");
        app.host_signal_levels.insert("D10_SS".to_string(), 0);
        assert_eq!(app.controller_signal_status_text("D10_SS"), "low");
        assert_eq!(app.controller_signal_status_text("+5V"), "power rail");
        assert_eq!(app.controller_signal_status_text("GND"), "ground");
    }
}
