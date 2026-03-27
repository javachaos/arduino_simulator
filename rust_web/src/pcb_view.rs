use std::collections::BTreeSet;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use rust_board::{
    board_from_kicad_pcb_text, layout_from_kicad_pcb_text, Board, BoardLayout, Bounds,
    LinePrimitive, PadGeometry, Point, ViaGeometry,
};

#[derive(Debug, Clone)]
pub struct LoadedPcb {
    pub board: Board,
    pub layout: BoardLayout,
    pub net_names: Vec<String>,
}

impl LoadedPcb {
    pub fn from_embedded_kicad(
        board_name: &str,
        source_path: &str,
        text: &str,
    ) -> Result<Self, String> {
        let board = board_from_kicad_pcb_text(board_name, source_path, text)
            .map_err(|error| error.to_string())?;
        let layout = layout_from_kicad_pcb_text(board_name, source_path, text)
            .map_err(|error| error.to_string())?;
        let mut net_names = board
            .nets
            .iter()
            .map(|net| net.name.clone())
            .collect::<Vec<_>>();
        net_names.sort();
        Ok(Self {
            board,
            layout,
            net_names,
        })
    }

    pub fn simplified_preview(mut self) -> Self {
        self.layout
            .drawings
            .retain(|line| preview_graphics_layer(&line.layer));
        self.layout
            .tracks
            .retain(|line| preview_track_layer(&line.layer));
        self.layout.vias.clear();
        self.layout.zones.clear();
        self.layout.circles.clear();
        self.layout.texts.clear();
        for footprint in &mut self.layout.footprints {
            footprint
                .graphics
                .retain(|line| preview_graphics_layer(&line.layer));
            footprint.label = None;
        }
        self.layout.bounds = recompute_bounds(&self.layout);
        self
    }
}

pub fn render_pcb_preview(
    ui: &mut egui::Ui,
    loaded: &LoadedPcb,
    active_nets: &BTreeSet<String>,
) {
    let desired_width = ui.available_width().max(220.0);
    let desired_height = (desired_width * 0.5).clamp(240.0, 340.0);
    let desired = egui::vec2(desired_width, desired_height);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 10.0, Color32::from_rgb(20, 26, 30));

    let projector = Projector::new(rect, &loaded.layout.bounds);

    for zone in &loaded.layout.zones {
        let points = zone
            .points
            .iter()
            .map(|point| projector.point(point))
            .collect::<Vec<_>>();
        if points.len() >= 3 {
            let fill = if zone.keepout {
                Color32::from_rgba_unmultiplied(170, 45, 45, 24)
            } else {
                Color32::from_rgba_unmultiplied(90, 110, 90, 18)
            };
            painter.add(Shape::convex_polygon(points, fill, Stroke::NONE));
        }
    }

    for line in &loaded.layout.drawings {
        paint_line(&painter, &projector, line, active_nets, false);
    }
    for line in &loaded.layout.tracks {
        paint_line(&painter, &projector, line, active_nets, true);
    }
    for line in &loaded.layout.edge_cuts {
        paint_line(&painter, &projector, line, active_nets, false);
    }
    for footprint in &loaded.layout.footprints {
        for graphic in &footprint.graphics {
            paint_line(&painter, &projector, graphic, active_nets, false);
        }
        for pad in &footprint.pads {
            paint_pad(&painter, &projector, pad, active_nets);
        }
    }
    for via in &loaded.layout.vias {
        paint_via(&painter, &projector, via, active_nets);
    }

    let outline = if response.hovered() {
        Color32::from_rgb(90, 130, 170)
    } else {
        Color32::from_rgb(56, 78, 98)
    };
    painter.rect_stroke(
        rect.shrink(1.0),
        10.0,
        Stroke::new(1.0, outline),
        egui::StrokeKind::Outside,
    );
}

fn preview_track_layer(layer: &str) -> bool {
    matches!(layer, "F.Cu" | "B.Cu")
}

fn preview_graphics_layer(layer: &str) -> bool {
    matches!(layer, "F.SilkS" | "B.SilkS" | "Edge.Cuts")
}

fn recompute_bounds(layout: &BoardLayout) -> Bounds {
    let mut xs = Vec::new();
    let mut ys = Vec::new();

    let mut push_point = |point: &Point| {
        xs.push(point.x_mm);
        ys.push(point.y_mm);
    };

    for line in layout
        .edge_cuts
        .iter()
        .chain(layout.drawings.iter())
        .chain(layout.tracks.iter())
    {
        push_point(&line.start);
        push_point(&line.end);
    }

    for footprint in &layout.footprints {
        push_point(&footprint.position);
        for pad in &footprint.pads {
            let (width_mm, height_mm) = pad.size_mm;
            let half_width = width_mm * 0.5;
            let half_height = height_mm * 0.5;
            push_point(&Point::new(
                pad.position.x_mm - half_width,
                pad.position.y_mm - half_height,
            ));
            push_point(&Point::new(
                pad.position.x_mm + half_width,
                pad.position.y_mm + half_height,
            ));
        }
        for line in &footprint.graphics {
            push_point(&line.start);
            push_point(&line.end);
        }
    }

    if xs.is_empty() || ys.is_empty() {
        return layout.bounds.clone();
    }

    Bounds::new(
        xs.iter().copied().fold(f64::INFINITY, f64::min),
        ys.iter().copied().fold(f64::INFINITY, f64::min),
        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    )
    .expand(2.0)
}

struct Projector {
    bounds: Bounds,
    scale: f32,
    offset: Vec2,
}

impl Projector {
    fn new(rect: Rect, bounds: &Bounds) -> Self {
        let margin = 10.0f32;
        let available_width = (rect.width() - (margin * 2.0)).max(1.0);
        let available_height = (rect.height() - (margin * 2.0)).max(1.0);
        let width_mm = bounds.width_mm().max(1.0) as f32;
        let height_mm = bounds.height_mm().max(1.0) as f32;
        let scale = (available_width / width_mm).min(available_height / height_mm);
        let scaled_width = width_mm * scale;
        let scaled_height = height_mm * scale;
        let offset = Vec2::new(
            rect.left() + margin + ((available_width - scaled_width) * 0.5),
            rect.top() + margin + ((available_height - scaled_height) * 0.5),
        );
        Self {
            bounds: bounds.clone(),
            scale,
            offset,
        }
    }

    fn point(&self, point: &Point) -> Pos2 {
        Pos2::new(
            self.offset.x + ((point.x_mm - self.bounds.min_x_mm) as f32 * self.scale),
            self.offset.y + ((point.y_mm - self.bounds.min_y_mm) as f32 * self.scale),
        )
    }

    fn stroke_width(&self, width_mm: f64, minimum_px: f32) -> f32 {
        ((width_mm as f32) * self.scale).max(minimum_px)
    }
}

fn paint_line(
    painter: &egui::Painter,
    projector: &Projector,
    line: &LinePrimitive,
    active_nets: &BTreeSet<String>,
    is_track: bool,
) {
    let active = line
        .net_name
        .as_ref()
        .map(|net| active_nets.contains(net))
        .unwrap_or(false);
    let color = if active {
        Color32::from_rgb(255, 86, 86)
    } else if line.layer == "Edge.Cuts" {
        Color32::from_rgb(220, 220, 220)
    } else if is_track {
        if line.layer.starts_with('B') {
            Color32::from_rgb(45, 170, 220)
        } else {
            Color32::from_rgb(195, 110, 52)
        }
    } else if line.layer.contains("SilkS") {
        Color32::from_gray(175)
    } else {
        Color32::from_gray(110)
    };

    painter.line_segment(
        [projector.point(&line.start), projector.point(&line.end)],
        Stroke::new(projector.stroke_width(line.width_mm, 1.0), color),
    );
}

fn paint_pad(
    painter: &egui::Painter,
    projector: &Projector,
    pad: &PadGeometry,
    active_nets: &BTreeSet<String>,
) {
    let center = projector.point(&pad.position);
    let active = pad
        .net_name
        .as_ref()
        .map(|net| active_nets.contains(net))
        .unwrap_or(false);
    let fill = if active {
        Color32::from_rgb(255, 86, 86)
    } else if pad.display_layer.as_deref() == Some("B.Cu") {
        Color32::from_rgb(70, 140, 190)
    } else {
        Color32::from_rgb(210, 126, 58)
    };
    let stroke = Stroke::new(1.0, Color32::from_black_alpha(120));

    match pad.shape.as_str() {
        "round" | "circle" => {
            let radius =
                ((pad.size_mm.0.max(pad.size_mm.1) as f32) * projector.scale * 0.5).max(1.5);
            painter.circle_filled(center, radius, fill);
            painter.circle_stroke(center, radius, stroke);
        }
        _ => {
            let polygon = rotated_rect_points(
                center,
                pad.size_mm,
                pad.rotation_deg.unwrap_or(0.0),
                projector.scale,
            );
            painter.add(Shape::convex_polygon(polygon, fill, stroke));
        }
    }
}

fn paint_via(
    painter: &egui::Painter,
    projector: &Projector,
    via: &ViaGeometry,
    active_nets: &BTreeSet<String>,
) {
    let center = projector.point(&via.position);
    let active = via
        .net_name
        .as_ref()
        .map(|net| active_nets.contains(net))
        .unwrap_or(false);
    let radius = ((via.size_mm as f32) * projector.scale * 0.5).max(1.2);
    let color = if active {
        Color32::from_rgb(255, 86, 86)
    } else {
        Color32::from_rgb(128, 128, 150)
    };
    painter.circle_filled(center, radius, color);
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(1.0, Color32::from_black_alpha(110)),
    );
}

fn rotated_rect_points(
    center: Pos2,
    size_mm: (f64, f64),
    rotation_deg: f64,
    scale: f32,
) -> Vec<Pos2> {
    let half_width = size_mm.0 as f32 * scale * 0.5;
    let half_height = size_mm.1 as f32 * scale * 0.5;
    let radians = (-rotation_deg as f32).to_radians();
    let cosine = radians.cos();
    let sine = radians.sin();
    [
        (-half_width, -half_height),
        (half_width, -half_height),
        (half_width, half_height),
        (-half_width, half_height),
    ]
    .into_iter()
    .map(|(x, y)| {
        Pos2::new(
            center.x + (x * cosine) - (y * sine),
            center.y + (x * sine) + (y * cosine),
        )
    })
    .collect()
}
