use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use eframe::egui;
use rust_behavior::suggested_builtin_behavior_for_board_model;
use rust_board::{built_in_board_model_names, load_built_in_board_model};
use rust_project::{
    AssemblyBundle, AssemblyExport, AssemblyMember, AssemblyMemberKind, AttachmentBinding,
    AttachmentEndpoint, BindingMode, DefinitionReference, DefinitionReferenceKind,
    DefinitionSource, DefinitionSourceKind, FirmwareSource, FirmwareSourceKind, HostBoard,
    PortClass, PortDefinition, PortDirection,
};

use crate::pcb_view::{render_pcb, LoadedPcb};

const PRIMARY_MEMBER_ID: &str = "primary";
const BOARD_FILE_SUFFIX: &str = ".board.avrsim.json";
const COLLAPSED_PORTS_PER_ROW: usize = 16;
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PortCanvasKey {
    instance_id: Option<String>,
    port: String,
}

impl PortCanvasKey {
    fn from_endpoint(endpoint: &AttachmentEndpoint) -> Self {
        Self {
            instance_id: endpoint.instance_id.clone(),
            port: endpoint.port.clone(),
        }
    }
}

pub struct BoardEditorState {
    pub open: bool,
    bundle: AssemblyBundle,
    bundle_path: String,
    selected_member_id: String,
    notice: String,
    loaded_pcbs: BTreeMap<String, LoadedPcb>,
    member_positions: BTreeMap<String, egui::Pos2>,
    collapsed_member_ids: BTreeSet<String>,
    expose_pin_mode: bool,
    wire_drag_from: Option<AttachmentEndpoint>,
    selected_attachment_index: Option<usize>,
    preview_member_id: Option<String>,
}

impl Default for BoardEditorState {
    fn default() -> Self {
        let mut primary = AssemblyMember::new(
            PRIMARY_MEMBER_ID,
            AssemblyMemberKind::Board,
            "controller",
            DefinitionSource::builtin_board_model(HostBoard::Mega2560Rev3.builtin_board_model()),
        );
        primary.embedded_host_board = Some(HostBoard::Mega2560Rev3);
        primary.ports = built_in_ports(HostBoard::Mega2560Rev3);
        Self {
            open: false,
            bundle: AssemblyBundle::new("Untitled Board Assembly", primary),
            bundle_path: String::new(),
            selected_member_id: PRIMARY_MEMBER_ID.to_string(),
            notice: "Choose a controller, add modules, then wire them together.".to_string(),
            loaded_pcbs: BTreeMap::new(),
            member_positions: BTreeMap::new(),
            collapsed_member_ids: BTreeSet::new(),
            expose_pin_mode: false,
            wire_drag_from: None,
            selected_attachment_index: None,
            preview_member_id: None,
        }
    }
}

impl BoardEditorState {
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }

        let viewport_id = egui::ViewportId::from_hash_of("avrsim_board_editor");
        let builder = egui::ViewportBuilder::default()
            .with_title("Board Editor")
            .with_inner_size([1600.0, 980.0])
            .with_min_inner_size([980.0, 720.0])
            .with_resizable(true);

        ctx.show_viewport_immediate(viewport_id, builder, |ctx, class| {
            if ctx.input(|input| input.viewport().close_requested()) {
                self.open = false;
                self.preview_member_id = None;
                return;
            }

            match class {
                egui::ViewportClass::Embedded => {
                    let mut open = self.open;
                    egui::Window::new("Board Editor")
                        .open(&mut open)
                        .default_size([1480.0, 900.0])
                        .resizable(true)
                        .show(ctx, |ui| self.draw_root_contents(ui));
                    self.open = open;
                }
                _ => {
                    egui::CentralPanel::default().show(ctx, |ui| self.draw_root_contents(ui));
                }
            }

            self.draw_preview_window(ctx);
        });
    }

    fn draw_root_contents(&mut self, ui: &mut egui::Ui) {
        self.draw_toolbar(ui);
        ui.add_space(8.0);
        let available_height = ui.available_height();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(300.0, available_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.draw_module_sidebar(ui),
            );
            ui.separator();
            let wiring_size = ui.available_size();
            ui.allocate_ui_with_layout(
                wiring_size,
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.draw_wiring_editor(ui),
            );
        });
    }

    fn draw_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Board Name:");
            ui.add_sized(
                [220.0, 24.0],
                egui::TextEdit::singleline(&mut self.bundle.name).hint_text("Board name"),
            );
            if ui.button("Open Board").clicked() {
                self.open_bundle_dialog();
            }
            if ui.button("Save Board").clicked() {
                self.save_bundle_dialog();
            }
            if !self.bundle_path.trim().is_empty() {
                ui.separator();
                ui.small(self.bundle_path.clone());
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Main Controller:");
            let mut controller = self
                .bundle
                .primary
                .embedded_host_board
                .unwrap_or(HostBoard::Mega2560Rev3);
            egui::ComboBox::from_id_salt("board_editor_main_controller")
                .selected_text(controller.label())
                .show_ui(ui, |ui| {
                    for board in HostBoard::ALL {
                        ui.selectable_value(&mut controller, board, board.label());
                    }
                });
            self.apply_primary_controller(controller);
        });

        self.draw_primary_firmware_toolbar(ui);

        ui.horizontal_wrapped(|ui| {
            ui.label("Status:");
            ui.small(&self.notice);
        });
    }

    fn apply_primary_controller(&mut self, host_board: HostBoard) {
        if self.bundle.primary.embedded_host_board == Some(host_board)
            && self.bundle.primary.source.builtin_name.as_deref()
                == Some(host_board.builtin_board_model())
        {
            return;
        }

        self.bundle.primary.embedded_host_board = Some(host_board);
        self.bundle.primary.source =
            DefinitionSource::builtin_board_model(host_board.builtin_board_model());
        self.bundle.primary.name = host_board.short_name().to_string();
        self.bundle.primary.label = Some(host_board.label().to_string());
        self.bundle.primary.ports = built_in_ports(host_board);
        self.reseed_member_positions();
    }

    fn draw_primary_firmware_toolbar(&mut self, ui: &mut egui::Ui) {
        let existing = self
            .bundle
            .primary
            .firmware
            .clone()
            .unwrap_or(FirmwareSource {
                kind: FirmwareSourceKind::Hex,
                path: PathBuf::new(),
                compiled_hex_path: None,
            });
        let mut firmware = existing;
        let mut clear = false;
        let mut browse_path = None;

        ui.horizontal_wrapped(|ui| {
            ui.label("Firmware:");
            let mut firmware_text = firmware.path.display().to_string();
            ui.add_sized(
                [360.0, 24.0],
                egui::TextEdit::singleline(&mut firmware_text)
                    .hint_text("Select a .ino sketch or .hex image"),
            );
            if ui.button("Browse").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("AVR firmware", &["ino", "hex"])
                    .pick_file()
                {
                    firmware_text = path.display().to_string();
                    browse_path = Some(path);
                }
            }
            if ui.button("Clear").clicked() {
                clear = true;
            }
            firmware.path = PathBuf::from(firmware_text.trim());
            firmware.kind = infer_firmware_kind(&firmware.path);
            ui.small(match firmware.kind {
                FirmwareSourceKind::Ino => ".ino",
                FirmwareSourceKind::Hex => ".hex",
            });
        });

        if let Some(path) = browse_path {
            firmware.path = path;
            firmware.kind = infer_firmware_kind(&firmware.path);
        }

        if clear {
            self.bundle.primary.firmware = None;
        } else if !firmware.path.as_os_str().is_empty() {
            self.bundle.primary.firmware = Some(firmware);
        }
    }

    fn draw_preview_window(&mut self, ctx: &egui::Context) {
        let Some(member_id) = self.preview_member_id.clone() else {
            return;
        };
        let Some(loaded) = self.loaded_pcbs.get(&member_id) else {
            self.preview_member_id = None;
            return;
        };

        let mut open = true;
        egui::Window::new(format!("PCB Preview: {member_id}"))
            .open(&mut open)
            .default_size([900.0, 720.0])
            .show(ctx, |ui| {
                let bindings = Vec::new();
                render_pcb(
                    ui,
                    loaded,
                    &bindings,
                    &[],
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                );
            });
        if !open {
            self.preview_member_id = None;
        }
    }

    fn draw_module_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("board_editor_module_sidebar_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.draw_member_list(ui);
                ui.add_space(10.0);
                self.draw_member_detail(ui);
            });
    }

    fn draw_member_list(&mut self, ui: &mut egui::Ui) {
        ui.heading("Modules");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Add Module").clicked() {
                self.add_child_member(AssemblyMemberKind::Module);
            }
            if self.selected_member_id != PRIMARY_MEMBER_ID
                && ui.button("Remove Selected").clicked()
            {
                self.remove_selected_child();
            }
        });
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .id_salt("board_editor_member_list_scroll")
            .show(ui, |ui| {
                for child in &self.bundle.children {
                    let selected = self.selected_member_id == child.id;
                    if ui
                        .selectable_label(selected, display_member_label(child))
                        .clicked()
                    {
                        self.selected_member_id = child.id.clone();
                    }
                    ui.small(member_subtitle(child));
                    ui.add_space(4.0);
                }
            });
    }

    fn draw_member_detail(&mut self, ui: &mut egui::Ui) {
        if self.selected_member_id == PRIMARY_MEMBER_ID {
            ui.heading("Module Setup");
            ui.small("Add a module on the left to configure its source and ports.");
            return;
        }

        let Some(snapshot) = self.selected_member_snapshot() else {
            ui.heading("Module Setup");
            ui.label("Select a module on the left.");
            return;
        };

        ui.heading("Selected Module");
        ui.add_space(4.0);

        let selected_id = self.selected_member_id.clone();
        self.draw_module_identity_editor(ui, &selected_id);
        ui.add_space(8.0);
        self.draw_module_source_editor(ui, &selected_id);
        ui.add_space(8.0);
        self.draw_member_port_editor(ui, &selected_id);

        if snapshot.source.kind == DefinitionSourceKind::KicadPcb {
            ui.add_space(8.0);
            if ui.button("Open PCB Preview").clicked() {
                self.preview_member_id = Some(selected_id);
            }
        }
    }

    fn draw_module_identity_editor(&mut self, ui: &mut egui::Ui, selected_id: &str) {
        let Some(member) = self.find_member_mut(selected_id) else {
            self.handle_missing_selected_member(selected_id);
            ui.small("Selected module is no longer available.");
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.label("ID:");
            ui.monospace(&member.id);
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Module Name:");
            ui.add_sized(
                [260.0, 24.0],
                egui::TextEdit::singleline(&mut member.name).hint_text("Module name"),
            );
        });
    }

    fn draw_module_source_editor(&mut self, ui: &mut egui::Ui, selected_id: &str) {
        if self.find_member(selected_id).is_none() {
            self.handle_missing_selected_member(selected_id);
            ui.small("Selected module is no longer available.");
            return;
        }
        ui.label(egui::RichText::new("Module Source").strong());
        let mut source_kind = self
            .find_member(selected_id)
            .map(|member| member.source.kind)
            .unwrap_or(DefinitionSourceKind::BuiltinBoardModel);
        egui::ComboBox::from_id_salt(format!("member_source_kind_{selected_id}"))
            .selected_text(match source_kind {
                DefinitionSourceKind::KicadPcb => "PCB File",
                DefinitionSourceKind::BuiltinBoardModel => "Built-in Module",
                DefinitionSourceKind::Virtual => "Built-in Module",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut source_kind, DefinitionSourceKind::KicadPcb, "PCB File");
                ui.selectable_value(
                    &mut source_kind,
                    DefinitionSourceKind::BuiltinBoardModel,
                    "Built-in Module",
                );
            });

        {
            let Some(member) = self.find_member_mut(selected_id) else {
                self.handle_missing_selected_member(selected_id);
                ui.small("Selected module is no longer available.");
                return;
            };
            if member.source.kind != source_kind {
                match source_kind {
                    DefinitionSourceKind::KicadPcb => {
                        member.source.kind = DefinitionSourceKind::KicadPcb;
                        if member.source.path.is_none() {
                            member.source.path = Some(PathBuf::new());
                        }
                        member.source.builtin_name = None;
                    }
                    DefinitionSourceKind::BuiltinBoardModel => {
                        member.source.kind = DefinitionSourceKind::BuiltinBoardModel;
                        member.source.path = None;
                        member.source.builtin_name =
                            Some(default_builtin_module_name().to_string());
                    }
                    DefinitionSourceKind::Virtual => {
                        member.source =
                            DefinitionSource::builtin_board_model(default_builtin_module_name());
                    }
                }
            }
        }
        if source_kind != DefinitionSourceKind::KicadPcb {
            self.loaded_pcbs.remove(selected_id);
        }

        match source_kind {
            DefinitionSourceKind::KicadPcb => {
                let path_text = self
                    .find_member(selected_id)
                    .and_then(|member| member.source.path.as_ref())
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
                let mut edited = path_text;
                let mut browse_path = None;
                let mut load_clicked = false;
                ui.horizontal_wrapped(|ui| {
                    ui.label("PCB File:");
                    ui.add_sized(
                        [320.0, 24.0],
                        egui::TextEdit::singleline(&mut edited)
                            .hint_text("Select a .kicad_pcb file"),
                    );
                    if ui.button("Browse PCB").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("KiCad PCB", &["kicad_pcb"])
                            .pick_file()
                        {
                            edited = path.display().to_string();
                            browse_path = Some(path);
                        }
                    }
                    if ui.button("Load").clicked() {
                        load_clicked = true;
                    }
                });
                {
                    let Some(member) = self.find_member_mut(selected_id) else {
                        self.handle_missing_selected_member(selected_id);
                        ui.small("Selected module is no longer available.");
                        return;
                    };
                    member.source.path = Some(PathBuf::from(edited.trim()));
                }
                if let Some(path) = browse_path {
                    self.load_member_pcb(selected_id, &path);
                } else if load_clicked {
                    let path = PathBuf::from(edited.trim());
                    self.load_member_pcb(selected_id, &path);
                }
            }
            DefinitionSourceKind::BuiltinBoardModel => {
                let mut builtin = self
                    .find_member(selected_id)
                    .and_then(|member| member.source.builtin_name.clone())
                    .unwrap_or_else(|| default_builtin_module_name().to_string());
                egui::ComboBox::from_id_salt(format!("builtin_board_{selected_id}"))
                    .selected_text(builtin.clone())
                    .show_ui(ui, |ui| {
                        for name in built_in_module_model_names() {
                            ui.selectable_value(&mut builtin, name.to_string(), name);
                        }
                    });
                let source = DefinitionSource::builtin_board_model(&builtin);
                let temp_member = AssemblyMember::new(
                    selected_id.to_string(),
                    AssemblyMemberKind::Module,
                    selected_id.to_string(),
                    source.clone(),
                );
                let ports = self.derive_member_ports(selected_id, &temp_member);
                let Some(member) = self.find_member_mut(selected_id) else {
                    self.handle_missing_selected_member(selected_id);
                    ui.small("Selected module is no longer available.");
                    return;
                };
                member.source = source;
                member.ports = ports;
                member.behavior = suggested_behavior_reference_for_member(member);
            }
            DefinitionSourceKind::Virtual => {
                ui.small("Built-in modules expose their ports automatically.");
            }
        }
    }

    fn draw_member_port_editor(&mut self, ui: &mut egui::Ui, selected_id: &str) {
        ui.label(egui::RichText::new("Ports").strong());
        let Some(snapshot) = self.find_member(selected_id).cloned() else {
            self.handle_missing_selected_member(selected_id);
            ui.small("Selected module is no longer available.");
            return;
        };
        let mut derive_ports = false;
        let mut clear_ports = false;
        let mut add_port = false;
        ui.horizontal_wrapped(|ui| {
            if ui.button("Auto Derive Ports").clicked() {
                derive_ports = true;
            }
        });

        if derive_ports {
            let ports = self.derive_member_ports(selected_id, &snapshot);
            let count = ports.len();
            let Some(member) = self.find_member_mut(selected_id) else {
                self.handle_missing_selected_member(selected_id);
                ui.small("Selected module is no longer available.");
                return;
            };
            member.ports = ports;
            self.notice = format!("Derived {count} port(s) for {}", member.id);
        }

        ui.small(format!(
            "{} port(s) available for visual wiring.",
            self.find_member(selected_id)
                .map(|member| member.ports.len())
                .unwrap_or(0)
        ));

        egui::CollapsingHeader::new("Advanced Port Details")
            .default_open(false)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Add Port").clicked() {
                        add_port = true;
                    }
                    if !snapshot.ports.is_empty() && ui.button("Clear Ports").clicked() {
                        clear_ports = true;
                    }
                });
                ui.add_space(6.0);
                let Some(member) = self.find_member_mut(selected_id) else {
                    self.handle_missing_selected_member(selected_id);
                    ui.small("Selected module is no longer available.");
                    return;
                };
                egui::ScrollArea::vertical()
                    .id_salt(format!("board_editor_ports_scroll_{selected_id}"))
                    .max_height(220.0)
                    .show(ui, |ui| {
                        let mut remove_index = None;
                        for (index, port) in member.ports.iter_mut().enumerate() {
                            ui.group(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Name:");
                                    ui.add_sized(
                                        [160.0, 24.0],
                                        egui::TextEdit::singleline(&mut port.name),
                                    );
                                    ui.label("Class:");
                                    port_class_combo(
                                        ui,
                                        &format!("port_class_{selected_id}_{index}"),
                                        &mut port.class,
                                    );
                                    ui.label("Dir:");
                                    port_direction_combo(
                                        ui,
                                        &format!("port_direction_{selected_id}_{index}"),
                                        &mut port.direction,
                                    );
                                    if ui.button("Remove").clicked() {
                                        remove_index = Some(index);
                                    }
                                });
                                let mut aliases = port.aliases.join(", ");
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Aliases:");
                                    ui.add_sized(
                                        [300.0, 24.0],
                                        egui::TextEdit::singleline(&mut aliases)
                                            .hint_text("Comma-separated aliases"),
                                    );
                                });
                                port.aliases = split_csv(&aliases);
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Note:");
                                    ui.add_sized(
                                        [360.0, 24.0],
                                        egui::TextEdit::singleline(
                                            port.note.get_or_insert_with(String::new),
                                        )
                                        .hint_text("Optional note"),
                                    );
                                });
                            });
                            ui.add_space(4.0);
                        }
                        if let Some(index) = remove_index {
                            member.ports.remove(index);
                        }
                    });
            });

        if add_port {
            let Some(member) = self.find_member_mut(selected_id) else {
                self.handle_missing_selected_member(selected_id);
                ui.small("Selected module is no longer available.");
                return;
            };
            member.ports.push(PortDefinition::new(
                format!("port_{}", member.ports.len() + 1),
                PortClass::Passive,
                PortDirection::Passive,
            ));
        }
        if clear_ports {
            let Some(member) = self.find_member_mut(selected_id) else {
                self.handle_missing_selected_member(selected_id);
                ui.small("Selected module is no longer available.");
                return;
            };
            member.ports.clear();
        }
    }

    fn draw_wiring_editor(&mut self, ui: &mut egui::Ui) {
        ui.set_min_size(ui.available_size());
        ui.heading("Wiring");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Auto Layout").clicked() {
                self.reseed_member_positions();
                self.notice = "Re-laid out the visual board canvas.".to_string();
            }
            if ui.button("Import Wiring").clicked() {
                let imported = self.import_wiring_from_ports();
                if imported == 0 {
                    self.notice = "No safe controller-to-module wires were inferred.".to_string();
                } else {
                    self.notice = format!("Imported {imported} controller-to-module wire(s).");
                }
            }
            if ui.button("Clear Wires").clicked() {
                self.bundle.attachments.clear();
                self.wire_drag_from = None;
                self.selected_attachment_index = None;
                self.notice = "Cleared all visual wires from the assembly.".to_string();
            }
            if ui
                .selectable_label(self.expose_pin_mode, "Expose Pins")
                .clicked()
            {
                self.expose_pin_mode = !self.expose_pin_mode;
                self.wire_drag_from = None;
                self.selected_attachment_index = None;
                self.notice = if self.expose_pin_mode {
                    "Expose Pins mode is on. Click a port to toggle it as connectable from outside the board."
                        .to_string()
                } else {
                    "Expose Pins mode is off. Ports now behave normally for wiring.".to_string()
                };
            }
            if ui
                .add_enabled(
                    self.selected_attachment_index.is_some(),
                    egui::Button::new("Delete Selected Wire"),
                )
                .clicked()
            {
                self.delete_selected_attachment();
            }
            if ui.button("Validate").clicked() {
                self.notice = match self.bundle.validate() {
                    Ok(()) => "Assembly bundle validates cleanly.".to_string(),
                    Err(error) => format!("Validation failed: {error}"),
                };
            }
        });
        if !self.bundle.exports.is_empty() {
            ui.add_space(4.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(egui::RichText::new("Connectable Pins").strong());
                let mut remove_export_index = None;
                egui::ScrollArea::vertical()
                    .id_salt("board_editor_exports_scroll")
                    .max_height(92.0)
                    .show(ui, |ui| {
                        for (index, export) in self.bundle.exports.iter().enumerate() {
                            ui.horizontal_wrapped(|ui| {
                                ui.monospace(&export.name);
                                ui.small(endpoint_display(&export.source));
                                if ui.small_button("x").clicked() {
                                    remove_export_index = Some(index);
                                }
                            });
                        }
                    });
                if let Some(index) = remove_export_index {
                    let removed = self.bundle.exports.remove(index);
                    self.notice = format!(
                        "Removed connectable pin {} ({})",
                        removed.name,
                        endpoint_display(&removed.source)
                    );
                }
            });
        }
        if ui.input(|input| {
            input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace)
        }) {
            self.delete_selected_attachment();
        }
        if self.expose_pin_mode {
            ui.small("Expose Pins mode: click a port to toggle whether it can be connected from outside this board.");
        } else if let Some(from) = self.wire_drag_from.as_ref() {
            ui.small(format!(
                "Creating wire from {}. Click or drag onto another port to finish.",
                endpoint_display(from)
            ));
        } else if let Some(index) = self.selected_attachment_index {
            if let Some(attachment) = self.bundle.attachments.get(index) {
                ui.small(format!(
                    "Selected wire: {} -> {}. Press Delete or use the button above to remove it.",
                    endpoint_display(&attachment.from),
                    endpoint_display(&attachment.to)
                ));
            } else {
                ui.small("Drag from one port to another to create a wire. Drag a card by its body to reposition it.");
            }
        } else {
            ui.small("Drag from one port to another to create a wire. Click a wire to select it. Drag a card by its body to reposition it.");
        }

        ui.add_space(8.0);
        let remaining = ui.available_size();
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_min_size(remaining);
            egui::ScrollArea::both()
                .id_salt("board_editor_wiring_canvas_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    self.draw_visual_wiring_canvas(ui);
                });
        });
    }

    fn draw_visual_wiring_canvas(&mut self, ui: &mut egui::Ui) {
        self.ensure_member_positions();
        let desired_size = self.visual_canvas_size(ui.available_width());
        let (canvas_rect, canvas_response) =
            ui.allocate_exact_size(desired_size, egui::Sense::click());
        let painter = ui.painter_at(canvas_rect);
        painter.rect_filled(canvas_rect, 10.0, ui.visuals().faint_bg_color);

        let members = self.member_snapshots();
        let mut anchors = BTreeMap::<PortCanvasKey, egui::Pos2>::new();
        let pointer_pos = ui.ctx().pointer_latest_pos();
        let pointer_released = ui.input(|input| input.pointer.any_released());
        let pointer_clicked = ui.input(|input| input.pointer.primary_clicked());
        let mut hovered_endpoint: Option<AttachmentEndpoint> = None;
        let mut any_member_clicked = false;
        let mut any_port_clicked = false;
        let selected_attachment_endpoints = self
            .selected_attachment_index
            .and_then(|index| self.bundle.attachments.get(index))
            .map(|attachment| {
                (
                    PortCanvasKey::from_endpoint(&attachment.from),
                    PortCanvasKey::from_endpoint(&attachment.to),
                )
            });
        let exported_endpoints = self
            .bundle
            .exports
            .iter()
            .map(|export| PortCanvasKey::from_endpoint(&export.source))
            .collect::<BTreeSet<_>>();

        for (index, member) in members.iter().enumerate() {
            let collapsed = self.collapsed_member_ids.contains(&member.id);
            let local_pos = *self
                .member_positions
                .entry(member.id.clone())
                .or_insert_with(|| default_member_canvas_position(index));
            let card_size = member_card_size(member, collapsed);
            let card_rect =
                egui::Rect::from_min_size(canvas_rect.min + local_pos.to_vec2(), card_size);
            let card_id = ui.make_persistent_id(format!("wiring_card_{}", member.id));
            let card_response = ui.interact(card_rect, card_id, egui::Sense::click_and_drag());

            let mut pointer_over_port = false;
            let header_rect =
                egui::Rect::from_min_size(card_rect.min, egui::vec2(card_rect.width(), 42.0));
            let selected = self.selected_member_id == member.id;
            let header_fill = if selected {
                ui.visuals().selection.bg_fill
            } else {
                egui::Color32::from_rgb(46, 58, 74)
            };
            painter.rect_filled(
                card_rect,
                8.0,
                if selected {
                    egui::Color32::from_rgb(41, 47, 58)
                } else {
                    ui.visuals().panel_fill
                },
            );
            painter.rect_filled(header_rect, 8.0, header_fill);
            painter.text(
                header_rect.left_top() + egui::vec2(12.0, 9.0),
                egui::Align2::LEFT_TOP,
                display_member_label(member),
                egui::FontId::proportional(17.0),
                egui::Color32::WHITE,
            );
            painter.text(
                header_rect.left_bottom() + egui::vec2(12.0, -7.0),
                egui::Align2::LEFT_BOTTOM,
                if collapsed {
                    format!("{} ports hidden", member.ports.len())
                } else {
                    member_subtitle(member)
                },
                egui::FontId::proportional(11.0),
                egui::Color32::from_gray(220),
            );
            let collapse_rect = egui::Rect::from_center_size(
                egui::pos2(header_rect.right() - 18.0, header_rect.center().y),
                egui::vec2(24.0, 24.0),
            );
            let collapse_id = ui.make_persistent_id(format!("wiring_collapse_{}", member.id));
            let collapse_response = ui.interact(collapse_rect, collapse_id, egui::Sense::click());
            if collapse_response.clicked() {
                if collapsed {
                    self.collapsed_member_ids.remove(&member.id);
                } else {
                    self.collapsed_member_ids.insert(member.id.clone());
                }
                self.notice = format!(
                    "{} {}.",
                    display_member_label(member),
                    if collapsed { "expanded" } else { "collapsed" }
                );
            }
            let collapse_fill = if collapse_response.hovered() {
                egui::Color32::from_rgb(96, 126, 164)
            } else {
                egui::Color32::from_rgb(68, 86, 108)
            };
            painter.rect_filled(collapse_rect, 6.0, collapse_fill);
            painter.rect_stroke(
                collapse_rect,
                6.0,
                egui::Stroke::new(1.0, egui::Color32::from_gray(220)),
                egui::StrokeKind::Outside,
            );
            painter.text(
                collapse_rect.center(),
                egui::Align2::CENTER_CENTER,
                if collapsed { "+" } else { "-" },
                egui::FontId::proportional(16.0),
                egui::Color32::WHITE,
            );
            painter.rect_stroke(
                card_rect,
                8.0,
                egui::Stroke::new(
                    if selected { 2.0 } else { 1.0 },
                    if selected {
                        ui.visuals().selection.stroke.color
                    } else {
                        egui::Color32::from_gray(90)
                    },
                ),
                egui::StrokeKind::Outside,
            );

            if card_response.clicked() {
                self.selected_member_id = member.id.clone();
                any_member_clicked = true;
            }

            for (port_index, port) in member.ports.iter().enumerate() {
                let endpoint = member_endpoint(member, &port.name);
                let port_key = PortCanvasKey::from_endpoint(&endpoint);
                let (anchor_center, hit_rect) = if collapsed {
                    let row = port_index / COLLAPSED_PORTS_PER_ROW;
                    let column = port_index % COLLAPSED_PORTS_PER_ROW;
                    let left = card_rect.left() + 12.0;
                    let right = card_rect.right() - 12.0;
                    let columns = COLLAPSED_PORTS_PER_ROW.max(1) as f32;
                    let x = if columns <= 1.0 {
                        (left + right) * 0.5
                    } else {
                        left + ((right - left) * (column as f32 / (columns - 1.0)))
                    };
                    let y = card_rect.top() + 50.0 + (row as f32 * 12.0);
                    let center = egui::pos2(x, y);
                    (
                        center,
                        egui::Rect::from_center_size(center, egui::vec2(12.0, 12.0)),
                    )
                } else {
                    let y = card_rect.top() + 52.0 + (port_index as f32 * 24.0);
                    (
                        egui::pos2(card_rect.left() + 14.0, y + 8.0),
                        egui::Rect::from_min_size(
                            egui::pos2(card_rect.left() + 8.0, y),
                            egui::vec2(card_rect.width() - 16.0, 20.0),
                        ),
                    )
                };
                anchors.insert(port_key.clone(), anchor_center);

                let port_id =
                    ui.make_persistent_id(format!("wiring_port_{}_{}", member.id, port.name));
                let port_response = ui.interact(hit_rect, port_id, egui::Sense::click_and_drag());
                if collapsed {
                    port_response.clone().on_hover_text(&port.name);
                }
                if port_response.hovered() {
                    hovered_endpoint = Some(endpoint.clone());
                    pointer_over_port = true;
                }
                if port_response.clicked() {
                    any_port_clicked = true;
                    if self.expose_pin_mode {
                        self.toggle_export_for_endpoint(endpoint.clone());
                    } else {
                        match self.wire_drag_from.clone() {
                            Some(from) if from == endpoint => {
                                self.wire_drag_from = None;
                            }
                            Some(from) => {
                                self.add_attachment_if_new(from, endpoint.clone());
                                self.wire_drag_from = None;
                            }
                            None => {
                                self.wire_drag_from = Some(endpoint.clone());
                            }
                        }
                    }
                }
                if port_response.drag_started() && !self.expose_pin_mode {
                    self.wire_drag_from = Some(endpoint.clone());
                }

                let is_drag_origin = self
                    .wire_drag_from
                    .as_ref()
                    .map(|from| *from == endpoint)
                    .unwrap_or(false);
                let is_selected_wire_port = selected_attachment_endpoints
                    .as_ref()
                    .map(|(from, to)| *from == port_key || *to == port_key)
                    .unwrap_or(false);
                let is_exported_port = exported_endpoints.contains(&port_key);
                let port_fill = if is_drag_origin {
                    egui::Color32::from_rgb(255, 196, 64)
                } else if port_response.hovered() {
                    egui::Color32::from_rgb(124, 188, 255)
                } else if is_exported_port {
                    egui::Color32::from_rgb(96, 210, 120)
                } else if is_selected_wire_port {
                    egui::Color32::from_rgb(255, 150, 92)
                } else {
                    ui.visuals().widgets.inactive.fg_stroke.color
                };
                if is_drag_origin
                    || port_response.hovered()
                    || is_selected_wire_port
                    || is_exported_port
                {
                    painter.circle_stroke(
                        anchor_center,
                        8.0,
                        egui::Stroke::new(2.0, port_fill.gamma_multiply(0.9)),
                    );
                }
                painter.circle_filled(anchor_center, 5.5, port_fill);
                if collapsed {
                    painter.circle_filled(anchor_center, 2.5, port_fill);
                } else {
                    painter.text(
                        egui::pos2(anchor_center.x + 12.0, anchor_center.y - 7.0),
                        egui::Align2::LEFT_TOP,
                        &port.name,
                        egui::FontId::proportional(13.0),
                        ui.visuals().text_color(),
                    );
                }
            }

            if card_response.dragged() && !pointer_over_port {
                let delta = ui.input(|input| input.pointer.delta());
                if delta != egui::Vec2::ZERO {
                    if let Some(position) = self.member_positions.get_mut(&member.id) {
                        let next = *position + delta;
                        position.x = next
                            .x
                            .clamp(8.0, (canvas_rect.width() - card_size.x - 8.0).max(8.0));
                        position.y = next
                            .y
                            .clamp(8.0, (canvas_rect.height() - card_size.y - 8.0).max(8.0));
                    }
                    ui.ctx().request_repaint();
                }
            }
        }

        let mut clicked_attachment = None;
        for (index, attachment) in self.bundle.attachments.iter().enumerate() {
            let from = anchors.get(&PortCanvasKey::from_endpoint(&attachment.from));
            let to = anchors.get(&PortCanvasKey::from_endpoint(&attachment.to));
            if let (Some(from), Some(to)) = (from, to) {
                let selected = self.selected_attachment_index == Some(index);
                let wire_color = if selected {
                    egui::Color32::from_rgb(255, 170, 72)
                } else {
                    ui.visuals().selection.stroke.color
                };
                let wire_stroke = egui::Stroke::new(if selected { 4.0 } else { 2.0 }, wire_color);
                painter.line_segment([*from, *to], wire_stroke);
                if selected {
                    painter.circle_filled(*from, 4.0, wire_color);
                    painter.circle_filled(*to, 4.0, wire_color);
                }

                if pointer_clicked {
                    if let Some(pointer) = pointer_pos {
                        let hit_rect = egui::Rect::from_two_pos(*from, *to).expand(8.0);
                        if hit_rect.contains(pointer)
                            && distance_to_segment(pointer, *from, *to) <= 8.0
                        {
                            clicked_attachment = Some(index);
                        }
                    }
                }
            }
        }

        if let Some(from) = self.wire_drag_from.as_ref() {
            if let Some(start) = anchors.get(&PortCanvasKey::from_endpoint(from)) {
                let target = hovered_endpoint
                    .as_ref()
                    .and_then(|endpoint| anchors.get(&PortCanvasKey::from_endpoint(endpoint)))
                    .copied()
                    .or(pointer_pos);
                if let Some(target) = target {
                    painter.line_segment(
                        [*start, target],
                        egui::Stroke::new(3.0, egui::Color32::from_rgb(255, 210, 92)),
                    );
                    painter.circle_filled(*start, 6.0, egui::Color32::from_rgb(255, 210, 92));
                    painter.circle_filled(target, 6.0, egui::Color32::from_rgb(255, 210, 92));
                }
            }
        }

        if let Some(index) = clicked_attachment {
            self.selected_attachment_index = Some(index);
            self.wire_drag_from = None;
        } else if canvas_response.clicked() && !any_member_clicked && !any_port_clicked {
            self.selected_attachment_index = None;
            if self.wire_drag_from.is_none() {
                self.selected_member_id = PRIMARY_MEMBER_ID.to_string();
            }
        }

        if pointer_released {
            if let Some(from) = self.wire_drag_from.take() {
                if let Some(target) = hovered_endpoint {
                    if from != target {
                        self.add_attachment_if_new(from, target);
                    }
                }
            }
        }
    }

    fn member_snapshots(&self) -> Vec<AssemblyMember> {
        std::iter::once(self.bundle.primary.clone())
            .chain(self.bundle.children.clone())
            .collect()
    }

    fn ensure_member_positions(&mut self) {
        let members = self.member_snapshots();
        for (index, member) in members.iter().enumerate() {
            self.member_positions
                .entry(member.id.clone())
                .or_insert_with(|| default_member_canvas_position(index));
        }
        let active_ids = members
            .into_iter()
            .map(|member| member.id)
            .collect::<BTreeSet<_>>();
        self.member_positions
            .retain(|member_id, _| active_ids.contains(member_id));
        self.collapsed_member_ids
            .retain(|member_id| active_ids.contains(member_id));
    }

    fn reseed_member_positions(&mut self) {
        self.member_positions.clear();
        self.ensure_member_positions();
    }

    fn visual_canvas_size(&self, available_width: f32) -> egui::Vec2 {
        let mut width = available_width.max(1200.0);
        let mut height = 900.0f32;
        for member in self.member_snapshots() {
            let position = self
                .member_positions
                .get(&member.id)
                .copied()
                .unwrap_or_else(|| default_member_canvas_position(0));
            let card = member_card_size(&member, self.collapsed_member_ids.contains(&member.id));
            width = width.max(position.x + card.x + 120.0);
            height = height.max(position.y + card.y + 120.0);
        }
        egui::vec2(width, height)
    }

    fn import_wiring_from_ports(&mut self) -> usize {
        let primary_ports = self.bundle.primary.ports.clone();
        let child_port_sets = self
            .bundle
            .children
            .iter()
            .map(|child| (child.id.clone(), child.ports.clone()))
            .collect::<Vec<_>>();

        let mut planned = Vec::<(AttachmentEndpoint, AttachmentEndpoint)>::new();
        for (child_id, ports) in child_port_sets {
            for child_port in ports {
                let Some(primary_port) = best_primary_port_match(&primary_ports, &child_port)
                else {
                    continue;
                };
                planned.push((
                    AttachmentEndpoint::primary(primary_port.name.clone()),
                    AttachmentEndpoint::child(child_id.clone(), child_port.name.clone()),
                ));
            }
        }

        let mut added = 0usize;
        for (from, to) in planned {
            if self.attachment_exists(&from, &to) {
                continue;
            }
            self.bundle.attachments.push(AttachmentBinding {
                from,
                to,
                mode: BindingMode::Bus,
                note: Some("Imported from controller/module port matching".to_string()),
            });
            added += 1;
        }
        if added > 0 {
            self.selected_attachment_index = Some(self.bundle.attachments.len() - 1);
        }
        added
    }

    fn attachment_exists(&self, from: &AttachmentEndpoint, to: &AttachmentEndpoint) -> bool {
        self.bundle.attachments.iter().any(|attachment| {
            (attachment.from == *from && attachment.to == *to)
                || (attachment.from == *to && attachment.to == *from)
        })
    }

    fn add_attachment_if_new(&mut self, from: AttachmentEndpoint, to: AttachmentEndpoint) {
        if from == to {
            return;
        }
        if self.attachment_exists(&from, &to) {
            self.notice = "That wire already exists.".to_string();
            return;
        }
        self.bundle.attachments.push(AttachmentBinding {
            from: from.clone(),
            to: to.clone(),
            mode: BindingMode::Bus,
            note: None,
        });
        self.selected_attachment_index = Some(self.bundle.attachments.len() - 1);
        self.notice = format!(
            "Connected {} -> {}.",
            endpoint_display(&from),
            endpoint_display(&to)
        );
    }

    fn delete_selected_attachment(&mut self) {
        let Some(index) = self.selected_attachment_index else {
            return;
        };
        if index >= self.bundle.attachments.len() {
            self.selected_attachment_index = None;
            return;
        }
        let attachment = self.bundle.attachments.remove(index);
        self.selected_attachment_index = None;
        self.wire_drag_from = None;
        self.notice = format!(
            "Deleted wire {} -> {}.",
            endpoint_display(&attachment.from),
            endpoint_display(&attachment.to)
        );
    }

    fn toggle_export_for_endpoint(&mut self, endpoint: AttachmentEndpoint) {
        if let Some(index) = self
            .bundle
            .exports
            .iter()
            .position(|export| export.source == endpoint)
        {
            let removed = self.bundle.exports.remove(index);
            self.notice = format!(
                "Removed connectable pin {} ({})",
                removed.name,
                endpoint_display(&removed.source)
            );
            return;
        }

        let export_name = self.make_unique_export_name(default_export_name_for_endpoint(&endpoint));
        self.bundle.exports.push(AssemblyExport {
            name: export_name.clone(),
            source: endpoint.clone(),
            aliases: Vec::new(),
            note: Some("Selected from wiring canvas".to_string()),
        });
        self.notice = format!(
            "Added connectable pin {} ({})",
            export_name,
            endpoint_display(&endpoint)
        );
    }

    fn make_unique_export_name(&self, base_name: String) -> String {
        let trimmed = base_name.trim();
        let root = if trimmed.is_empty() { "pin" } else { trimmed };
        let existing = self
            .bundle
            .exports
            .iter()
            .map(|export| export.name.clone())
            .collect::<BTreeSet<_>>();
        if !existing.contains(root) {
            return root.to_string();
        }
        for suffix in 2.. {
            let candidate = format!("{root}_{suffix}");
            if !existing.contains(&candidate) {
                return candidate;
            }
        }
        unreachable!("finite export name space exhaustion is not realistic");
    }

    fn selected_member_snapshot(&self) -> Option<AssemblyMember> {
        self.find_member(&self.selected_member_id).cloned()
    }

    fn handle_missing_selected_member(&mut self, missing_id: &str) {
        if self.selected_member_id == missing_id {
            self.selected_member_id = PRIMARY_MEMBER_ID.to_string();
        }
        if self.preview_member_id.as_deref() == Some(missing_id) {
            self.preview_member_id = None;
        }
        self.notice = format!("Selected module {missing_id} is no longer available.");
    }

    fn find_member(&self, id: &str) -> Option<&AssemblyMember> {
        if self.bundle.primary.id == id {
            Some(&self.bundle.primary)
        } else {
            self.bundle.children.iter().find(|child| child.id == id)
        }
    }

    fn find_member_mut(&mut self, id: &str) -> Option<&mut AssemblyMember> {
        if self.bundle.primary.id == id {
            Some(&mut self.bundle.primary)
        } else {
            self.bundle.children.iter_mut().find(|child| child.id == id)
        }
    }

    fn add_child_member(&mut self, kind: AssemblyMemberKind) {
        let id = next_member_id(&self.bundle, kind);
        let source = DefinitionSource::builtin_board_model(default_builtin_module_name());
        let mut member = AssemblyMember::new(id.clone(), kind, id.clone(), source.clone());
        member.label = Some("Module".to_string());
        member.ports = self.derive_member_ports(&id, &member);
        member.behavior = suggested_behavior_reference_for_member(&member);
        self.bundle.children.push(member);
        self.member_positions.insert(
            id.clone(),
            default_member_canvas_position(self.bundle.children.len()),
        );
        self.collapsed_member_ids.remove(&id);
        self.selected_member_id = id.clone();
        self.notice = format!("Added {id} to the assembly.");
    }

    fn remove_selected_child(&mut self) {
        if self.selected_member_id == PRIMARY_MEMBER_ID {
            return;
        }
        let removed_id = self.selected_member_id.clone();
        self.bundle.children.retain(|child| child.id != removed_id);
        self.bundle.attachments.retain(|attachment| {
            attachment.from.instance_id.as_deref() != Some(&removed_id)
                && attachment.to.instance_id.as_deref() != Some(&removed_id)
        });
        self.bundle
            .exports
            .retain(|export| export.source.instance_id.as_deref() != Some(&removed_id));
        self.loaded_pcbs.remove(&removed_id);
        self.collapsed_member_ids.remove(&removed_id);
        self.selected_member_id = PRIMARY_MEMBER_ID.to_string();
        self.notice = format!("Removed {removed_id} and its wiring.");
    }

    fn load_member_pcb(&mut self, member_id: &str, path: &Path) {
        if path.as_os_str().is_empty() {
            self.notice = "Select a PCB file first.".to_string();
            return;
        }
        match LoadedPcb::load(path) {
            Ok(loaded) => {
                self.loaded_pcbs.insert(member_id.to_string(), loaded);
                let loaded_snapshot = self.loaded_pcbs.get(member_id).cloned();
                if let Some(member) = self.find_member_mut(member_id) {
                    member.source = DefinitionSource::kicad_pcb(path);
                    if member.name.trim().is_empty() {
                        member.name = path
                            .file_stem()
                            .and_then(|value| value.to_str())
                            .unwrap_or("board")
                            .to_string();
                    }
                    if let Some(loaded) = loaded_snapshot.as_ref() {
                        member.ports = derive_ports_from_loaded_pcb(loaded);
                    }
                    member.behavior = None;
                }
                self.notice = format!("Loaded PCB {} for {}.", path.display(), member_id);
            }
            Err(error) => {
                self.loaded_pcbs.remove(member_id);
                self.notice = format!("PCB load failed: {error}");
            }
        }
    }

    fn derive_member_ports(&self, member_id: &str, member: &AssemblyMember) -> Vec<PortDefinition> {
        match member.source.kind {
            DefinitionSourceKind::KicadPcb => self
                .loaded_pcbs
                .get(member_id)
                .map(derive_ports_from_loaded_pcb)
                .unwrap_or_default(),
            DefinitionSourceKind::BuiltinBoardModel => member
                .source
                .builtin_name
                .as_deref()
                .and_then(|name| load_built_in_board_model(name).ok())
                .map(|board| {
                    derive_ports_from_net_names(board.nets.into_iter().map(|net| net.name))
                })
                .unwrap_or_default(),
            DefinitionSourceKind::Virtual => Vec::new(),
        }
    }

    fn open_bundle_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("avrsim board", &["json", "avrsim"])
            .pick_file()
        {
            self.load_bundle_file(&path);
        }
    }

    fn save_bundle_dialog(&mut self) {
        let default_name = default_board_file_name(&self.bundle.name);
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("avrsim board", &["json", "avrsim"])
            .set_file_name(&default_name)
            .save_file()
        {
            let path = normalize_board_bundle_path(&path, &self.bundle.name);
            match self.bundle.validate() {
                Ok(()) => match self.bundle.save_json(&path) {
                    Ok(()) => {
                        self.bundle_path = path.display().to_string();
                        self.notice = format!("Saved board assembly to {}", path.display());
                    }
                    Err(error) => {
                        self.notice = format!("Save failed: {error}");
                    }
                },
                Err(error) => {
                    self.notice = format!("Save blocked: {error}");
                }
            }
        }
    }

    fn load_bundle_file(&mut self, path: &Path) {
        match AssemblyBundle::load_json(path) {
            Ok(bundle) => {
                if let Err(error) = bundle.validate() {
                    self.notice = format!("Board file is invalid: {error}");
                    return;
                }
                self.bundle = bundle;
                self.bundle_path = path.display().to_string();
                self.selected_member_id = self.bundle.primary.id.clone();
                self.loaded_pcbs.clear();
                self.collapsed_member_ids.clear();
                self.expose_pin_mode = false;
                let mut paths_to_load = Vec::new();
                if let Some(path) = self.bundle.primary.source.path.clone() {
                    paths_to_load.push((self.bundle.primary.id.clone(), path));
                }
                for child in &self.bundle.children {
                    if let Some(path) = child.source.path.clone() {
                        paths_to_load.push((child.id.clone(), path));
                    }
                }
                for (member_id, pcb_path) in paths_to_load {
                    let _ = LoadedPcb::load(&pcb_path).map(|loaded| {
                        self.loaded_pcbs.insert(member_id, loaded);
                    });
                }
                self.notice = format!(
                    "Loaded board assembly with {} child member(s).",
                    self.bundle.children.len()
                );
            }
            Err(error) => {
                self.notice = format!("Open failed: {error}");
            }
        }
    }
}

fn next_member_id(bundle: &AssemblyBundle, kind: AssemblyMemberKind) -> String {
    let prefix = match kind {
        AssemblyMemberKind::Board => "board",
        AssemblyMemberKind::Module => "module",
    };
    let used = bundle
        .children
        .iter()
        .map(|member| member.id.clone())
        .chain(std::iter::once(bundle.primary.id.clone()))
        .collect::<BTreeSet<_>>();
    for index in 1.. {
        let candidate = format!("{prefix}_{index}");
        if !used.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("infinite iterator returns");
}

fn display_member_label(member: &AssemblyMember) -> String {
    member
        .label
        .clone()
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| member.name.clone())
}

fn member_subtitle(member: &AssemblyMember) -> String {
    let source = match member.source.kind {
        DefinitionSourceKind::KicadPcb => "pcb".to_string(),
        DefinitionSourceKind::BuiltinBoardModel => member
            .source
            .builtin_name
            .clone()
            .unwrap_or_else(|| "built-in".to_string()),
        DefinitionSourceKind::Virtual => "virtual".to_string(),
    };
    let firmware = if member.firmware.is_some() { " fw" } else { "" };
    let behavior = if member.behavior.is_some() {
        " behavior"
    } else {
        ""
    };
    format!(
        "{source} • {} ports{firmware}{behavior}",
        member.ports.len()
    )
}

fn endpoint_display(endpoint: &AttachmentEndpoint) -> String {
    match endpoint.instance_id.as_deref() {
        None => format!("Primary:{}", endpoint.port),
        Some(instance_id) => format!("{instance_id}:{}", endpoint.port),
    }
}

fn default_export_name_for_endpoint(endpoint: &AttachmentEndpoint) -> String {
    let raw = match endpoint.instance_id.as_deref() {
        None => endpoint.port.clone(),
        Some(instance_id) => format!("{instance_id}_{}", endpoint.port),
    };
    raw.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn member_endpoint(member: &AssemblyMember, port_name: &str) -> AttachmentEndpoint {
    if member.id == PRIMARY_MEMBER_ID {
        AttachmentEndpoint::primary(port_name.to_string())
    } else {
        AttachmentEndpoint::child(member.id.clone(), port_name.to_string())
    }
}

fn member_card_size(member: &AssemblyMember, collapsed: bool) -> egui::Vec2 {
    let width = 240.0;
    if collapsed {
        let rows = member.ports.len().div_ceil(COLLAPSED_PORTS_PER_ROW);
        let dot_grid_height = if rows == 0 {
            0.0
        } else {
            8.0 + (rows as f32 * 12.0)
        };
        egui::vec2(width, 42.0 + dot_grid_height)
    } else {
        let port_rows = member.ports.len().max(1) as f32;
        egui::vec2(width, 52.0 + (port_rows * 24.0))
    }
}

fn default_member_canvas_position(index: usize) -> egui::Pos2 {
    let column = index % 2;
    let row = index / 2;
    egui::pos2(20.0 + (column as f32 * 270.0), 20.0 + (row as f32 * 190.0))
}

fn distance_to_segment(point: egui::Pos2, start: egui::Pos2, end: egui::Pos2) -> f32 {
    let segment = end - start;
    let length_sq = segment.length_sq();
    if length_sq <= f32::EPSILON {
        return point.distance(start);
    }

    let t = ((point - start).dot(segment) / length_sq).clamp(0.0, 1.0);
    let projection = start + segment * t;
    point.distance(projection)
}

fn port_class_combo(ui: &mut egui::Ui, id: &str, value: &mut PortClass) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(match value {
            PortClass::Digital => "digital",
            PortClass::Analog => "analog",
            PortClass::Power => "power",
            PortClass::Bus => "bus",
            PortClass::Passive => "passive",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(value, PortClass::Digital, "digital");
            ui.selectable_value(value, PortClass::Analog, "analog");
            ui.selectable_value(value, PortClass::Power, "power");
            ui.selectable_value(value, PortClass::Bus, "bus");
            ui.selectable_value(value, PortClass::Passive, "passive");
        });
}

fn port_direction_combo(ui: &mut egui::Ui, id: &str, value: &mut PortDirection) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(match value {
            PortDirection::Input => "input",
            PortDirection::Output => "output",
            PortDirection::Bidirectional => "bidirectional",
            PortDirection::Passive => "passive",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(value, PortDirection::Input, "input");
            ui.selectable_value(value, PortDirection::Output, "output");
            ui.selectable_value(value, PortDirection::Bidirectional, "bidirectional");
            ui.selectable_value(value, PortDirection::Passive, "passive");
        });
}

fn infer_firmware_kind(path: &Path) -> FirmwareSourceKind {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("ino") => FirmwareSourceKind::Ino,
        _ => FirmwareSourceKind::Hex,
    }
}

fn default_board_file_name(name: &str) -> String {
    format!("{}{}", sanitize_name(name), BOARD_FILE_SUFFIX)
}

fn normalize_board_bundle_path(path: &Path, board_name: &str) -> PathBuf {
    let typed_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let stem = strip_board_file_suffix(typed_name);
    let normalized_stem = if stem.trim().is_empty() {
        sanitize_name(board_name)
    } else {
        sanitize_name(stem)
    };

    let mut normalized = path.to_path_buf();
    normalized.set_file_name(format!("{normalized_stem}{BOARD_FILE_SUFFIX}"));
    normalized
}

fn strip_board_file_suffix(name: &str) -> &str {
    name.strip_suffix(BOARD_FILE_SUFFIX)
        .or_else(|| name.strip_suffix(".board.avrsim"))
        .or_else(|| name.strip_suffix(".avrsim.json"))
        .or_else(|| name.strip_suffix(".avrsim"))
        .or_else(|| name.strip_suffix(".json"))
        .unwrap_or(name)
}

fn sanitize_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if sanitized.trim_matches('-').is_empty() {
        "board-assembly".to_string()
    } else {
        sanitized
    }
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

fn built_in_module_model_names() -> Vec<&'static str> {
    built_in_board_model_names()
        .into_iter()
        .filter(|name| !name.starts_with("arduino_"))
        .collect()
}

fn default_builtin_module_name() -> &'static str {
    built_in_module_model_names()
        .first()
        .copied()
        .unwrap_or("gy_sht31_d")
}

fn suggested_behavior_reference_for_member(member: &AssemblyMember) -> Option<DefinitionReference> {
    let builtin_name = member.source.builtin_name.as_deref()?;
    let behavior_name = suggested_builtin_behavior_for_board_model(builtin_name)?;
    Some(DefinitionReference::builtin(
        DefinitionReferenceKind::BehaviorDefinition,
        behavior_name,
    ))
}

fn built_in_ports(host_board: HostBoard) -> Vec<PortDefinition> {
    load_built_in_board_model(host_board.builtin_board_model())
        .map(|board| derive_ports_from_net_names(board.nets.into_iter().map(|net| net.name)))
        .unwrap_or_default()
}

fn best_primary_port_match<'a>(
    primary_ports: &'a [PortDefinition],
    child_port: &PortDefinition,
) -> Option<&'a PortDefinition> {
    let mut best: Option<(&PortDefinition, i32)> = None;
    let mut second_best_score = i32::MIN;

    for primary_port in primary_ports {
        let score = port_match_score(primary_port, child_port);
        if score <= 0 {
            continue;
        }
        match best {
            None => best = Some((primary_port, score)),
            Some((_, best_score)) if score > best_score => {
                second_best_score = best_score;
                best = Some((primary_port, score));
            }
            Some((_, best_score)) if score > second_best_score => {
                second_best_score = score;
                if score == best_score {
                    second_best_score = best_score;
                }
            }
            _ => {}
        }
    }

    let (port, best_score) = best?;
    if best_score < 70 || second_best_score >= best_score {
        None
    } else {
        Some(port)
    }
}

fn port_match_score(primary_port: &PortDefinition, child_port: &PortDefinition) -> i32 {
    let primary_forms = port_match_forms(primary_port);
    let child_forms = port_match_forms(child_port);

    let mut best = 0;
    if !primary_forms.is_disjoint(&child_forms) {
        best = 100;
    }

    if shared_prefixed_form(&primary_forms, &child_forms, 'D') {
        best = best.max(96);
    }
    if shared_prefixed_form(&primary_forms, &child_forms, 'A') {
        best = best.max(96);
    }
    if primary_forms.contains("SCK") && child_forms.contains("SCK") {
        best = best.max(90);
    }
    if primary_forms.contains("MOSI") && child_forms.contains("MOSI") {
        best = best.max(90);
    }
    if primary_forms.contains("MISO") && child_forms.contains("MISO") {
        best = best.max(90);
    }
    if primary_forms.contains("SDA") && child_forms.contains("SDA") {
        best = best.max(90);
    }
    if primary_forms.contains("SCL") && child_forms.contains("SCL") {
        best = best.max(90);
    }
    if primary_forms.contains("CS") && child_forms.contains("CS") {
        best = best.max(88);
    }
    if primary_forms.contains("GND") && child_forms.contains("GND") {
        best = best.max(92);
    }
    if child_forms.contains("VCC") && primary_forms.contains("5V") {
        best = best.max(86);
    }
    if child_forms.contains("VCC") && primary_forms.contains("3V3") {
        best = best.max(72);
    }
    if child_forms.contains("VIN") && primary_forms.contains("VIN") {
        best = best.max(86);
    }

    best
}

fn shared_prefixed_form(
    primary_forms: &BTreeSet<String>,
    child_forms: &BTreeSet<String>,
    prefix: char,
) -> bool {
    primary_forms
        .iter()
        .filter(|form| form.starts_with(prefix))
        .any(|form| child_forms.contains(form))
}

fn port_match_forms(port: &PortDefinition) -> BTreeSet<String> {
    let mut forms = BTreeSet::new();
    collect_name_forms(&port.name, &mut forms);
    for alias in &port.aliases {
        collect_name_forms(alias, &mut forms);
    }
    forms
}

fn collect_name_forms(raw: &str, forms: &mut BTreeSet<String>) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }

    let mut normalized = trimmed
        .trim_start_matches('/')
        .trim_start_matches('*')
        .to_ascii_uppercase();
    normalized = normalized.replace(['-', '.', ':'], "_");
    if normalized.starts_with('+') {
        let voltage = normalized.trim_start_matches('+').replace('_', "");
        if !voltage.is_empty() {
            forms.insert(voltage);
        }
    }
    if !normalized.is_empty() {
        forms.insert(normalized.clone());
    }
    if let Some(mapped_signal) = example_header_alias_signal(&normalized) {
        collect_name_forms(&mapped_signal, forms);
        if let Some(rest) = mapped_signal.strip_prefix('A').filter(|value| {
            !value.is_empty() && value.chars().all(|character| character.is_ascii_digit())
        }) {
            forms.insert(format!("ADC{rest}"));
        }
        return;
    }

    for token in normalized
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
    {
        forms.insert(token.to_string());
        if token.chars().all(|character| character.is_ascii_digit()) {
            forms.insert(format!("D{token}"));
        }
        if let Some(rest) = token.strip_prefix('D') {
            if !rest.is_empty() && rest.chars().all(|character| character.is_ascii_digit()) {
                forms.insert(format!("D{rest}"));
            }
        }
        if let Some(rest) = token.strip_prefix('A') {
            if !rest.is_empty() && rest.chars().all(|character| character.is_ascii_digit()) {
                forms.insert(format!("A{rest}"));
            }
        }

        match token {
            "SS" | "CS" | "NSS" => {
                forms.insert("SS".to_string());
                forms.insert("CS".to_string());
            }
            "SI" | "SDI" | "MOSI" => {
                forms.insert("MOSI".to_string());
            }
            "SO" | "SDO" | "MISO" => {
                forms.insert("MISO".to_string());
            }
            "CLK" | "SCLK" | "SCK" => {
                forms.insert("SCK".to_string());
            }
            "CANH" | "CAN_H" => {
                forms.insert("CANH".to_string());
            }
            "CANL" | "CAN_L" => {
                forms.insert("CANL".to_string());
            }
            "GROUND" => {
                forms.insert("GND".to_string());
            }
            "VDD" => {
                forms.insert("VCC".to_string());
            }
            _ => {}
        }
    }
}

fn example_header_alias_signal(normalized: &str) -> Option<String> {
    let (reference, pad_text) = normalized.rsplit_once('_')?;
    let pad = pad_text.parse::<u8>().ok()?;
    match reference {
        "P_AUX" if (1..=8).contains(&pad) => Some(format!("A{}", pad + 7)),
        "P_DIG" => match pad {
            1 | 2 => Some("GND".to_string()),
            3..=34 => {
                let pair_index = (pad - 3) / 2;
                let base = 52u8.saturating_sub(pair_index * 2);
                let signal = if pad % 2 == 1 { base } else { base + 1 };
                Some(format!("D{signal}"))
            }
            35 | 36 => Some("+5V".to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn derive_ports_from_loaded_pcb(loaded: &LoadedPcb) -> Vec<PortDefinition> {
    let connector_refs = loaded
        .board
        .components
        .iter()
        .filter(|component| component.kind == "connector")
        .map(|component| component.reference.clone())
        .collect::<BTreeSet<_>>();

    let mut aliases_by_net = BTreeMap::<String, BTreeSet<String>>::new();
    for component in &loaded.board.components {
        if !connector_refs.contains(&component.reference) {
            continue;
        }
        for pad in &component.pads {
            let Some(net_name) = &pad.net_name else {
                continue;
            };
            aliases_by_net
                .entry(net_name.clone())
                .or_default()
                .insert(format!("{}:{}", component.reference, pad.number));
        }
    }

    if aliases_by_net.is_empty() {
        return derive_ports_from_net_names(loaded.net_names.iter().cloned());
    }

    aliases_by_net
        .into_iter()
        .map(|(net_name, aliases)| {
            let (class, direction) = classify_port(&net_name);
            let mut port = PortDefinition::new(net_name.clone(), class, direction);
            port.aliases = aliases.into_iter().collect();
            port
        })
        .collect()
}

fn derive_ports_from_net_names<I>(names: I) -> Vec<PortDefinition>
where
    I: IntoIterator<Item = String>,
{
    let mut unique = BTreeSet::new();
    names
        .into_iter()
        .filter(|name| !name.trim().is_empty())
        .filter(|name| unique.insert(name.clone()))
        .map(|name| {
            let (class, direction) = classify_port(&name);
            PortDefinition::new(name, class, direction)
        })
        .collect()
}

fn classify_port(name: &str) -> (PortClass, PortDirection) {
    let upper = name.trim_start_matches('/').to_ascii_uppercase();
    if upper == "GND" || upper.contains("GROUND") {
        return (PortClass::Power, PortDirection::Passive);
    }
    if upper.starts_with('+')
        || upper.contains("VCC")
        || upper.contains("VIN")
        || upper.contains("24V")
        || upper.contains("5V")
        || upper.contains("3V3")
        || upper.contains("IOREF")
    {
        return (PortClass::Power, PortDirection::Passive);
    }
    if upper.contains("CAN")
        || upper.contains("SDA")
        || upper.contains("SCL")
        || upper.contains("MISO")
        || upper.contains("MOSI")
        || upper.contains("SCK")
        || upper.ends_with("_TX")
        || upper.ends_with("_RX")
        || upper.contains("UART")
        || upper.contains("SPI")
        || upper.contains("I2C")
        || upper.contains("MODBUS")
        || upper.contains("RS485")
    {
        return (PortClass::Bus, PortDirection::Bidirectional);
    }
    if upper.starts_with('A')
        || upper.contains("ADC")
        || upper.contains("_RAW")
        || upper.ends_with("_U")
        || upper.ends_with("_Y")
        || upper.contains("ANALOG")
    {
        return (PortClass::Analog, PortDirection::Bidirectional);
    }
    (PortClass::Digital, PortDirection::Bidirectional)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use eframe::egui;
    use rust_project::{
        AssemblyMemberKind, DefinitionReferenceKind, DefinitionSource, DefinitionSourceKind,
        HostBoard,
    };

    use super::{
        best_primary_port_match, classify_port, default_board_file_name,
        default_export_name_for_endpoint, derive_ports_from_loaded_pcb, distance_to_segment,
        next_member_id, normalize_board_bundle_path, port_match_forms,
        suggested_behavior_reference_for_member, BoardEditorState, PRIMARY_MEMBER_ID,
    };
    use crate::pcb_view::LoadedPcb;

    fn air_node_pcb_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/pcbs/air_node.kicad_pcb")
    }

    #[test]
    fn next_member_id_skips_existing_children() {
        let mut editor = BoardEditorState::default();
        editor.add_child_member(rust_project::AssemblyMemberKind::Board);
        editor.add_child_member(rust_project::AssemblyMemberKind::Board);
        assert_eq!(
            next_member_id(&editor.bundle, rust_project::AssemblyMemberKind::Board),
            "board_3"
        );
    }

    #[test]
    fn derived_ports_prefer_connector_nets() {
        let loaded = LoadedPcb::load(&air_node_pcb_path()).expect("load air node");
        let ports = derive_ports_from_loaded_pcb(&loaded);
        let names = ports.into_iter().map(|port| port.name).collect::<Vec<_>>();
        assert!(names.contains(&"CAN_H".to_string()));
        assert!(names.contains(&"CAN_L".to_string()));
        assert!(names.contains(&"+24V".to_string()));
        assert!(names.contains(&"GND".to_string()));
    }

    #[test]
    fn port_classification_handles_power_bus_and_analog_names() {
        assert_eq!(classify_port("GND").0, rust_project::PortClass::Power);
        assert_eq!(classify_port("CAN_H").0, rust_project::PortClass::Bus);
        assert_eq!(
            classify_port("/ACT_U_RAW").0,
            rust_project::PortClass::Analog
        );
    }

    #[test]
    fn default_editor_starts_with_main_controller_model() {
        let editor = BoardEditorState::default();
        assert_eq!(editor.bundle.primary.id, PRIMARY_MEMBER_ID);
        assert_eq!(
            editor.bundle.primary.source.kind,
            DefinitionSourceKind::BuiltinBoardModel
        );
        assert_eq!(
            editor.bundle.primary.source.builtin_name.as_deref(),
            Some("arduino_mega_2560_rev3")
        );
        assert_eq!(
            editor.bundle.primary.embedded_host_board,
            Some(rust_project::HostBoard::Mega2560Rev3)
        );
        assert!(!editor.bundle.primary.ports.is_empty());
        assert_eq!(editor.selected_member_id, PRIMARY_MEMBER_ID);
    }

    #[test]
    fn loading_member_pcb_updates_member_source() {
        let mut editor = BoardEditorState::default();
        let path = air_node_pcb_path();
        editor.load_member_pcb(PRIMARY_MEMBER_ID, &path);
        let primary = editor.bundle.primary.clone();
        assert_eq!(primary.source.kind, DefinitionSourceKind::KicadPcb);
        assert_eq!(primary.source.path, Some(path));
    }

    #[test]
    fn built_in_sensor_models_can_derive_ports_for_wiring() {
        let mut editor = BoardEditorState::default();
        editor.bundle.primary.source =
            rust_project::DefinitionSource::builtin_board_model("gy_sht31_d");
        let snapshot = editor.bundle.primary.clone();
        let ports = editor.derive_member_ports(PRIMARY_MEMBER_ID, &snapshot);
        let names = ports.into_iter().map(|port| port.name).collect::<Vec<_>>();
        assert_eq!(names, vec!["GND", "SCL", "SDA", "VCC"]);

        editor.bundle.primary.source =
            rust_project::DefinitionSource::builtin_board_model("bme280_breakout");
        let snapshot = editor.bundle.primary.clone();
        let ports = editor.derive_member_ports(PRIMARY_MEMBER_ID, &snapshot);
        let names = ports.into_iter().map(|port| port.name).collect::<Vec<_>>();
        assert_eq!(names, vec!["ADDR", "GND", "SCL", "SDA", "VCC"]);
    }

    #[test]
    fn builtin_module_models_suggest_matching_builtin_behaviors() {
        let mut editor = BoardEditorState::default();
        editor.bundle.primary.source =
            rust_project::DefinitionSource::builtin_board_model("mcp2515_tja1050_can_module");
        let suggestion =
            suggested_behavior_reference_for_member(&editor.bundle.primary).expect("suggestion");
        assert_eq!(suggestion.kind, DefinitionReferenceKind::BehaviorDefinition);
        assert_eq!(
            suggestion.builtin_name.as_deref(),
            Some("mcp2515_tja1050_can_module_behavior")
        );

        editor.bundle.primary.source =
            rust_project::DefinitionSource::builtin_board_model("bme280_breakout");
        let suggestion =
            suggested_behavior_reference_for_member(&editor.bundle.primary).expect("suggestion");
        assert_eq!(
            suggestion.builtin_name.as_deref(),
            Some("bme280_breakout_behavior")
        );
    }

    #[test]
    fn distance_to_segment_handles_inline_offset_and_endpoint_cases() {
        let start = egui::pos2(0.0, 0.0);
        let end = egui::pos2(10.0, 0.0);
        assert_eq!(distance_to_segment(egui::pos2(5.0, 0.0), start, end), 0.0);
        assert_eq!(distance_to_segment(egui::pos2(5.0, 3.0), start, end), 3.0);
        assert_eq!(distance_to_segment(egui::pos2(14.0, 0.0), start, end), 4.0);
    }

    #[test]
    fn import_wiring_connects_common_nano_to_sht31_signals() {
        let mut editor = BoardEditorState::default();
        editor.apply_primary_controller(HostBoard::NanoV3);
        editor.add_child_member(AssemblyMemberKind::Module);
        let child_id = editor.bundle.children[0].id.clone();
        {
            let snapshot = {
                let child = editor.find_member_mut(&child_id).expect("child");
                child.source = DefinitionSource::builtin_board_model("gy_sht31_d");
                child.clone()
            };
            let ports = editor.derive_member_ports(&child_id, &snapshot);
            let child = editor.find_member_mut(&child_id).expect("child");
            child.ports = ports;
        }

        let imported = editor.import_wiring_from_ports();
        assert_eq!(imported, 4);
        assert!(editor.attachment_exists(
            &rust_project::AttachmentEndpoint::primary("A4_SDA"),
            &rust_project::AttachmentEndpoint::child(&child_id, "SDA"),
        ));
        assert!(editor.attachment_exists(
            &rust_project::AttachmentEndpoint::primary("A5_SCL"),
            &rust_project::AttachmentEndpoint::child(&child_id, "SCL"),
        ));
        assert!(editor.attachment_exists(
            &rust_project::AttachmentEndpoint::primary("+5V"),
            &rust_project::AttachmentEndpoint::child(&child_id, "VCC"),
        ));
        assert!(editor.attachment_exists(
            &rust_project::AttachmentEndpoint::primary("GND"),
            &rust_project::AttachmentEndpoint::child(&child_id, "GND"),
        ));
    }

    #[test]
    fn import_wiring_matches_pcb_style_numeric_net_names_to_controller_pins() {
        let mut editor = BoardEditorState::default();
        editor.apply_primary_controller(HostBoard::NanoV3);
        editor.add_child_member(AssemblyMemberKind::Module);
        let child_id = editor.bundle.children[0].id.clone();
        let child = editor.find_member_mut(&child_id).expect("child");
        child.ports = vec![
            rust_project::PortDefinition::new(
                "/10",
                rust_project::PortClass::Digital,
                rust_project::PortDirection::Bidirectional,
            ),
            rust_project::PortDefinition::new(
                "/13",
                rust_project::PortClass::Digital,
                rust_project::PortDirection::Bidirectional,
            ),
            rust_project::PortDefinition::new(
                "GND",
                rust_project::PortClass::Power,
                rust_project::PortDirection::Passive,
            ),
            rust_project::PortDefinition::new(
                "+5V",
                rust_project::PortClass::Power,
                rust_project::PortDirection::Passive,
            ),
        ];

        let imported = editor.import_wiring_from_ports();
        assert_eq!(imported, 4);
        assert!(editor.attachment_exists(
            &rust_project::AttachmentEndpoint::primary("D10_SS"),
            &rust_project::AttachmentEndpoint::child(&child_id, "/10"),
        ));
        assert!(editor.attachment_exists(
            &rust_project::AttachmentEndpoint::primary("D13_SCK"),
            &rust_project::AttachmentEndpoint::child(&child_id, "/13"),
        ));
    }

    #[test]
    fn port_match_forms_expand_common_spi_and_numeric_aliases() {
        let mut port = rust_project::PortDefinition::new(
            "D10_SS",
            rust_project::PortClass::Digital,
            rust_project::PortDirection::Bidirectional,
        );
        port.aliases.push("/10".to_string());
        let forms = port_match_forms(&port);
        assert!(forms.contains("D10"));
        assert!(forms.contains("SS"));
        assert!(forms.contains("CS"));
        assert!(forms.contains("10"));
    }

    #[test]
    fn port_match_forms_expand_sidecar_header_aliases_to_mega_pins() {
        let mut digital = rust_project::PortDefinition::new(
            "DIGITAL_BUS",
            rust_project::PortClass::Digital,
            rust_project::PortDirection::Bidirectional,
        );
        digital.aliases.push("P_DIG:33".to_string());
        let digital_forms = port_match_forms(&digital);
        assert!(digital_forms.contains("P_DIG_33"));
        assert!(digital_forms.contains("D22"));
        assert!(!digital_forms.contains("D33"));

        let mut analog = rust_project::PortDefinition::new(
            "ANALOG_BUS",
            rust_project::PortClass::Analog,
            rust_project::PortDirection::Bidirectional,
        );
        analog.aliases.push("P_AUX:1".to_string());
        let analog_forms = port_match_forms(&analog);
        assert!(analog_forms.contains("P_AUX_1"));
        assert!(analog_forms.contains("A8"));
        assert!(analog_forms.contains("ADC8"));
        assert!(!analog_forms.contains("D1"));
    }

    #[test]
    fn best_primary_port_match_prefers_unique_controller_function_match() {
        let primary_ports = vec![
            rust_project::PortDefinition::new(
                "D13_SCK",
                rust_project::PortClass::Bus,
                rust_project::PortDirection::Bidirectional,
            ),
            rust_project::PortDefinition::new(
                "D11_MOSI",
                rust_project::PortClass::Bus,
                rust_project::PortDirection::Bidirectional,
            ),
            rust_project::PortDefinition::new(
                "D12_MISO",
                rust_project::PortClass::Bus,
                rust_project::PortDirection::Bidirectional,
            ),
        ];
        let child = rust_project::PortDefinition::new(
            "SDI",
            rust_project::PortClass::Bus,
            rust_project::PortDirection::Bidirectional,
        );
        let matched = best_primary_port_match(&primary_ports, &child).expect("match");
        assert_eq!(matched.name, "D11_MOSI");
    }

    #[test]
    fn import_wiring_maps_sidecar_example_headers_to_mega_ports() {
        let mut editor = BoardEditorState::default();
        editor.add_child_member(AssemblyMemberKind::Board);
        let child_id = editor.bundle.children[0].id.clone();
        let child = editor.find_member_mut(&child_id).expect("child");
        let mut digital = rust_project::PortDefinition::new(
            "/22",
            rust_project::PortClass::Digital,
            rust_project::PortDirection::Bidirectional,
        );
        digital.aliases.push("P_DIG:33".to_string());
        let mut analog = rust_project::PortDefinition::new(
            "/A8",
            rust_project::PortClass::Analog,
            rust_project::PortDirection::Bidirectional,
        );
        analog.aliases.push("P_AUX:1".to_string());
        let mut ground = rust_project::PortDefinition::new(
            "GND",
            rust_project::PortClass::Power,
            rust_project::PortDirection::Passive,
        );
        ground.aliases.push("P_DIG:1".to_string());
        let mut supply = rust_project::PortDefinition::new(
            "MEGA_5V",
            rust_project::PortClass::Power,
            rust_project::PortDirection::Passive,
        );
        supply.aliases.push("P_DIG:35".to_string());
        child.ports = vec![digital, analog, ground, supply];

        let imported = editor.import_wiring_from_ports();
        assert_eq!(imported, 4);
        assert!(editor.attachment_exists(
            &rust_project::AttachmentEndpoint::primary("D22"),
            &rust_project::AttachmentEndpoint::child(&child_id, "/22"),
        ));
        assert!(editor.attachment_exists(
            &rust_project::AttachmentEndpoint::primary("A8"),
            &rust_project::AttachmentEndpoint::child(&child_id, "/A8"),
        ));
        assert!(editor.attachment_exists(
            &rust_project::AttachmentEndpoint::primary("GND"),
            &rust_project::AttachmentEndpoint::child(&child_id, "GND"),
        ));
        assert!(editor.attachment_exists(
            &rust_project::AttachmentEndpoint::primary("+5V"),
            &rust_project::AttachmentEndpoint::child(&child_id, "MEGA_5V"),
        ));
    }

    #[test]
    fn board_file_name_defaults_to_canonical_suffix() {
        assert_eq!(
            default_board_file_name("Main Controller Stack"),
            "Main-Controller-Stack.board.avrsim.json"
        );
    }

    #[test]
    fn board_bundle_save_path_normalizes_legacy_and_partial_suffixes() {
        assert_eq!(
            normalize_board_bundle_path(&PathBuf::from("/tmp/air_node.board.avrsim"), "ignored"),
            PathBuf::from("/tmp/air_node.board.avrsim.json")
        );
        assert_eq!(
            normalize_board_bundle_path(&PathBuf::from("/tmp/air_node"), "Air Node"),
            PathBuf::from("/tmp/air_node.board.avrsim.json")
        );
    }

    #[test]
    fn export_name_defaults_follow_endpoint_identity() {
        assert_eq!(
            default_export_name_for_endpoint(&rust_project::AttachmentEndpoint::primary("A4/SDA")),
            "A4_SDA"
        );
        assert_eq!(
            default_export_name_for_endpoint(&rust_project::AttachmentEndpoint::child(
                "module_1", "CAN-H",
            )),
            "module_1_CAN-H"
        );
    }

    #[test]
    fn toggling_export_adds_and_removes_connectable_pin() {
        let mut editor = BoardEditorState::default();
        let endpoint = rust_project::AttachmentEndpoint::primary("A4_SDA");
        editor.toggle_export_for_endpoint(endpoint.clone());
        assert_eq!(editor.bundle.exports.len(), 1);
        assert_eq!(editor.bundle.exports[0].name, "A4_SDA");
        assert_eq!(editor.bundle.exports[0].source, endpoint);

        editor.toggle_export_for_endpoint(rust_project::AttachmentEndpoint::primary("A4_SDA"));
        assert!(editor.bundle.exports.is_empty());
    }
}
