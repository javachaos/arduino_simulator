use std::collections::HashMap;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use rust_mcu::{BoardPin, BoardPinLevel};

use crate::runtime::SimulationTarget;

#[derive(Clone, Copy)]
enum PinSide {
    Left,
    Right,
}

struct PinSlot {
    pin: BoardPin,
    label: String,
    side: PinSide,
    order: usize,
    group: usize,
}

pub fn show_board(
    ui: &mut egui::Ui,
    target: SimulationTarget,
    host_pin_levels: &[BoardPinLevel],
) {
    let slots = board_slots(target);
    let canvas_size = board_canvas_size(target, &slots);
    let (rect, _) = ui.allocate_exact_size(canvas_size, Sense::hover());
    let painter = ui.painter_at(rect);
    let pin_map = pin_level_map(host_pin_levels);

    let surface = Color32::from_rgb(10, 18, 29);
    let panel_shadow = Color32::from_rgba_unmultiplied(91, 148, 255, 28);
    painter.rect_filled(rect, 24.0, surface);
    painter.rect_stroke(
        rect.shrink(1.0),
        24.0,
        Stroke::new(1.0, panel_shadow),
        StrokeKind::Outside,
    );

    let board_rect = Rect::from_center_size(
        rect.center() + egui::vec2(0.0, 20.0),
        Vec2::new(
            if target == SimulationTarget::Mega {
                260.0
            } else {
                210.0
            },
            rect.height() - 120.0,
        ),
    );
    let board_fill = match target {
        SimulationTarget::Nano => Color32::from_rgb(18, 84, 97),
        SimulationTarget::Mega => Color32::from_rgb(22, 96, 78),
    };
    let board_outline = Color32::from_rgb(114, 210, 192);
    painter.rect_filled(board_rect, 28.0, board_fill);
    painter.rect_stroke(
        board_rect,
        28.0,
        Stroke::new(2.0, board_outline),
        StrokeKind::Outside,
    );

    let title_font = FontId::proportional(28.0);
    let subtitle_font = FontId::proportional(15.0);
    painter.text(
        Pos2::new(rect.center().x, rect.top() + 34.0),
        Align2::CENTER_CENTER,
        target.label(),
        title_font,
        Color32::from_rgb(241, 248, 255),
    );
    painter.text(
        Pos2::new(rect.center().x, rect.top() + 62.0),
        Align2::CENTER_CENTER,
        "Live pin activity and board wiring",
        subtitle_font,
        Color32::from_rgb(141, 167, 194),
    );

    paint_chips(&painter, board_rect, target);

    let active_count = host_pin_levels.iter().filter(|entry| entry.level != 0).count();
    painter.text(
        Pos2::new(board_rect.center().x, board_rect.top() + 26.0),
        Align2::CENTER_CENTER,
        format!("{active_count} pins HIGH"),
        FontId::proportional(18.0),
        Color32::from_rgb(255, 214, 124),
    );

    let slot_spacing = 22.0;
    let group_gap = 16.0;
    let board_top = board_rect.top() + 54.0;
    let left_label_x = rect.left() + 34.0;
    let right_label_x = rect.right() - 34.0;

    for slot in slots {
        let y = board_top + (slot.order as f32 * slot_spacing) + (slot.group as f32 * group_gap);
        let (pad_center, elbow, label_pos, align) = match slot.side {
            PinSide::Left => (
                Pos2::new(board_rect.left(), y),
                Pos2::new(board_rect.left() - 26.0, y),
                Pos2::new(left_label_x, y),
                Align2::LEFT_CENTER,
            ),
            PinSide::Right => (
                Pos2::new(board_rect.right(), y),
                Pos2::new(board_rect.right() + 26.0, y),
                Pos2::new(right_label_x, y),
                Align2::RIGHT_CENTER,
            ),
        };

        let level = pin_map.get(&slot.pin).copied().unwrap_or(0);
        let active = level != 0;
        let trace = if active {
            Color32::from_rgb(255, 177, 73)
        } else {
            Color32::from_rgb(86, 110, 131)
        };
        let label_color = if active {
            Color32::from_rgb(255, 239, 204)
        } else {
            Color32::from_rgb(196, 210, 223)
        };

        painter.line_segment([pad_center, elbow], Stroke::new(3.0, trace));
        painter.line_segment([elbow, label_pos], Stroke::new(1.5, trace.gamma_multiply(0.8)));
        painter.circle_filled(
            pad_center,
            6.0,
            if active {
                Color32::from_rgb(255, 188, 92)
            } else {
                Color32::from_rgb(48, 65, 83)
            },
        );
        painter.circle_stroke(
            pad_center,
            8.5,
            Stroke::new(1.0, Color32::from_rgb(220, 242, 250)),
        );
        painter.text(
            label_pos,
            align,
            slot.label,
            FontId::monospace(13.5),
            label_color,
        );
    }
}

pub fn show_board_preview(
    ui: &mut egui::Ui,
    target: SimulationTarget,
    host_pin_levels: &[BoardPinLevel],
) {
    let desired_size = egui::vec2(ui.available_width().max(220.0), 196.0);
    let (rect, _) = ui.allocate_exact_size(desired_size, Sense::hover());
    let painter = ui.painter_at(rect);
    let pin_map = pin_level_map(host_pin_levels);
    let slots = board_slots(target);

    painter.rect_filled(rect, 18.0, Color32::from_rgb(12, 22, 34));
    painter.rect_stroke(
        rect.shrink(0.5),
        18.0,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(124, 181, 214, 48)),
        StrokeKind::Outside,
    );

    let board_rect = Rect::from_center_size(
        rect.center() + egui::vec2(0.0, 8.0),
        Vec2::new(
            if target == SimulationTarget::Mega {
                rect.width() * 0.72
            } else {
                rect.width() * 0.64
            },
            rect.height() * 0.62,
        ),
    );
    let pcb_fill = match target {
        SimulationTarget::Nano => Color32::from_rgb(20, 96, 110),
        SimulationTarget::Mega => Color32::from_rgb(26, 106, 84),
    };
    painter.rect_filled(board_rect, 22.0, pcb_fill);
    painter.rect_stroke(
        board_rect,
        22.0,
        Stroke::new(1.5, Color32::from_rgb(148, 221, 207)),
        StrokeKind::Outside,
    );

    painter.text(
        Pos2::new(rect.center().x, rect.top() + 22.0),
        Align2::CENTER_CENTER,
        target.label(),
        FontId::proportional(16.0),
        Color32::from_rgb(239, 245, 250),
    );
    painter.text(
        Pos2::new(rect.center().x, rect.top() + 42.0),
        Align2::CENTER_CENTER,
        format!("{} pins HIGH", active_pin_count(host_pin_levels)),
        FontId::proportional(12.0),
        Color32::from_rgb(255, 214, 124),
    );

    paint_chips(&painter, board_rect, target);
    paint_preview_traces(&painter, board_rect);
    paint_preview_pin_glow(&painter, board_rect, &slots, &pin_map);
}

fn paint_chips(painter: &egui::Painter, board_rect: Rect, target: SimulationTarget) {
    let chip_fill = Color32::from_rgb(16, 24, 32);
    let chip_outline = Color32::from_rgb(96, 117, 139);

    let main_chip = Rect::from_center_size(
        board_rect.center() + egui::vec2(0.0, 10.0),
        Vec2::new(
            if target == SimulationTarget::Mega {
                108.0
            } else {
                86.0
            },
            if target == SimulationTarget::Mega {
                236.0
            } else {
                160.0
            },
        ),
    );
    painter.rect_filled(main_chip, 10.0, chip_fill);
    painter.rect_stroke(
        main_chip,
        10.0,
        Stroke::new(1.0, chip_outline),
        StrokeKind::Outside,
    );

    let usb_chip = Rect::from_center_size(
        Pos2::new(board_rect.center().x, board_rect.top() + 108.0),
        Vec2::new(74.0, 38.0),
    );
    painter.rect_filled(usb_chip, 8.0, chip_fill.gamma_multiply(1.15));
    painter.rect_stroke(
        usb_chip,
        8.0,
        Stroke::new(1.0, chip_outline),
        StrokeKind::Outside,
    );
}

fn pin_level_map(levels: &[BoardPinLevel]) -> HashMap<BoardPin, u8> {
    levels.iter().map(|entry| (entry.pin, entry.level)).collect()
}

pub fn active_pin_count(levels: &[BoardPinLevel]) -> usize {
    levels.iter().filter(|entry| entry.level != 0).count()
}

fn board_canvas_size(target: SimulationTarget, slots: &[PinSlot]) -> Vec2 {
    let max_slot_index = slots
        .iter()
        .map(|slot| slot.order + slot.group)
        .max()
        .unwrap_or(0) as f32;
    let height = 170.0
        + if target == SimulationTarget::Mega {
            32.0
        } else {
            0.0
        }
        + (max_slot_index * 24.0);

    Vec2::new(
        if target == SimulationTarget::Mega {
            840.0
        } else {
            720.0
        },
        height.max(560.0),
    )
}

fn board_slots(target: SimulationTarget) -> Vec<PinSlot> {
    match target {
        SimulationTarget::Nano => nano_slots(),
        SimulationTarget::Mega => mega_slots(),
    }
}

fn paint_preview_traces(painter: &egui::Painter, board_rect: Rect) {
    let trace = Color32::from_rgba_unmultiplied(207, 235, 199, 84);
    let trace_stroke = Stroke::new(1.6, trace);
    let x0 = board_rect.left() + board_rect.width() * 0.18;
    let x1 = board_rect.right() - board_rect.width() * 0.18;
    let y0 = board_rect.top() + board_rect.height() * 0.24;
    let y1 = board_rect.bottom() - board_rect.height() * 0.22;

    painter.line_segment(
        [Pos2::new(x0, y0), Pos2::new(board_rect.center().x, y0)],
        trace_stroke,
    );
    painter.line_segment(
        [
            Pos2::new(board_rect.center().x, y0),
            Pos2::new(board_rect.center().x, board_rect.center().y),
        ],
        trace_stroke,
    );
    painter.line_segment(
        [
            Pos2::new(board_rect.center().x, board_rect.center().y),
            Pos2::new(x1, board_rect.center().y),
        ],
        trace_stroke,
    );
    painter.line_segment(
        [Pos2::new(x0, y1), Pos2::new(x1, y1)],
        trace_stroke,
    );
    painter.line_segment(
        [Pos2::new(x1, y1), Pos2::new(x1, y0 + 12.0)],
        trace_stroke,
    );

    for point in [
        Pos2::new(x0, y0),
        Pos2::new(board_rect.center().x, y0),
        Pos2::new(board_rect.center().x, board_rect.center().y),
        Pos2::new(x1, board_rect.center().y),
        Pos2::new(x0, y1),
        Pos2::new(x1, y1),
    ] {
        painter.circle_filled(point, 2.8, Color32::from_rgb(228, 244, 211));
    }
}

fn paint_preview_pin_glow(
    painter: &egui::Painter,
    board_rect: Rect,
    slots: &[PinSlot],
    pin_map: &HashMap<BoardPin, u8>,
) {
    let left_slots = slots
        .iter()
        .filter(|slot| matches!(slot.side, PinSide::Left))
        .collect::<Vec<_>>();
    let right_slots = slots
        .iter()
        .filter(|slot| matches!(slot.side, PinSide::Right))
        .collect::<Vec<_>>();

    paint_preview_pin_side(painter, board_rect, &left_slots, pin_map, true);
    paint_preview_pin_side(painter, board_rect, &right_slots, pin_map, false);
}

fn paint_preview_pin_side(
    painter: &egui::Painter,
    board_rect: Rect,
    slots: &[&PinSlot],
    pin_map: &HashMap<BoardPin, u8>,
    left_side: bool,
) {
    if slots.is_empty() {
        return;
    }

    let top = board_rect.top() + 18.0;
    let bottom = board_rect.bottom() - 18.0;
    let span = (bottom - top).max(1.0);

    for (index, slot) in slots.iter().enumerate() {
        let t = if slots.len() == 1 {
            0.5
        } else {
            index as f32 / (slots.len() - 1) as f32
        };
        let y = top + (span * t);
        let x = if left_side {
            board_rect.left() - 8.0
        } else {
            board_rect.right() + 8.0
        };
        let active = pin_map.get(&slot.pin).copied().unwrap_or(0) != 0;
        let trace_end = if left_side {
            Pos2::new(board_rect.left() + 12.0, y)
        } else {
            Pos2::new(board_rect.right() - 12.0, y)
        };
        let pad = Pos2::new(x, y);

        painter.line_segment(
            [pad, trace_end],
            Stroke::new(
                if active { 1.6 } else { 1.0 },
                if active {
                    Color32::from_rgb(255, 183, 82)
                } else {
                    Color32::from_rgba_unmultiplied(214, 228, 199, 52)
                },
            ),
        );
        painter.circle_filled(
            pad,
            if active { 3.6 } else { 2.4 },
            if active {
                Color32::from_rgb(255, 203, 117)
            } else {
                Color32::from_rgb(86, 120, 110)
            },
        );
    }
}

fn nano_slots() -> Vec<PinSlot> {
    let mut slots = vec![PinSlot {
        pin: BoardPin::Digital(13),
        label: "D13".to_owned(),
        side: PinSide::Left,
        order: 0,
        group: 0,
    }];
    for analog in 0u8..=7 {
        slots.push(PinSlot {
            pin: BoardPin::Analog(analog),
            label: format!("A{analog}"),
            side: PinSide::Left,
            order: analog as usize + 1,
            group: 0,
        });
    }
    for digital in (0u8..=12).rev() {
        slots.push(PinSlot {
            pin: BoardPin::Digital(digital),
            label: format!("D{digital}"),
            side: PinSide::Right,
            order: (12 - digital) as usize,
            group: 0,
        });
    }
    slots
}

fn mega_slots() -> Vec<PinSlot> {
    let mut slots = Vec::new();
    for (order, digital) in (0u8..=21).rev().enumerate() {
        slots.push(PinSlot {
            pin: BoardPin::Digital(digital),
            label: format!("D{digital}"),
            side: PinSide::Left,
            order,
            group: 0,
        });
    }
    for (index, analog) in (0u8..=15).rev().enumerate() {
        slots.push(PinSlot {
            pin: BoardPin::Analog(analog),
            label: format!("A{analog}"),
            side: PinSide::Left,
            order: index,
            group: 1,
        });
    }
    for (order, digital) in (22u8..=53).rev().enumerate() {
        slots.push(PinSlot {
            pin: BoardPin::Digital(digital),
            label: format!("D{digital}"),
            side: PinSide::Right,
            order,
            group: 0,
        });
    }
    slots
}
