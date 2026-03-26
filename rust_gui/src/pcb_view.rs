use std::collections::BTreeSet;
use std::path::Path;

use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Shape, Stroke, Vec2};
use rust_board::{
    board_from_kicad_pcb, layout_from_kicad_pcb, Board, BoardLayout, Bounds, Component,
    FootprintLayout, LinePrimitive, PadGeometry, Point, TextPrimitive, ViaGeometry, ZonePolygon,
};
use rust_project::{ModuleOverlay, SignalBinding};

#[derive(Debug, Clone)]
pub struct LoadedPcb {
    pub board: Board,
    pub layout: BoardLayout,
    pub net_names: Vec<String>,
}

impl LoadedPcb {
    pub fn load(path: &Path) -> Result<Self, String> {
        let board = board_from_kicad_pcb(path).map_err(|error| error.to_string())?;
        let layout = layout_from_kicad_pcb(path).map_err(|error| error.to_string())?;
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

    pub fn preview(board: Board) -> Self {
        let layout = preview_layout_from_board(&board);
        let mut net_names = board
            .nets
            .iter()
            .map(|net| net.name.clone())
            .collect::<Vec<_>>();
        net_names.sort();
        Self {
            board,
            layout,
            net_names,
        }
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

fn preview_track_layer(layer: &str) -> bool {
    matches!(layer, "F.Cu" | "B.Cu")
}

fn preview_graphics_layer(layer: &str) -> bool {
    matches!(layer, "F.SilkS" | "B.SilkS" | "Edge.Cuts")
}

fn preview_layout_from_board(board: &Board) -> BoardLayout {
    let footprints = board
        .components
        .iter()
        .map(preview_footprint_from_component)
        .collect::<Vec<_>>();
    let content_bounds = preview_content_bounds(board);
    let bounds = content_bounds.expand(preview_margin_mm(board));

    BoardLayout {
        name: board.title.clone().unwrap_or_else(|| board.name.clone()),
        source_path: format!("builtin-preview://{}", board.name),
        bounds: bounds.clone(),
        footprints,
        edge_cuts: rectangle_lines(
            &bounds,
            "Edge.Cuts",
            0.25,
            Some(board.name.clone()),
            Some("board_preview".to_string()),
            None,
        ),
        drawings: Vec::new(),
        circles: Vec::new(),
        texts: Vec::new(),
        tracks: Vec::new(),
        vias: Vec::new(),
        zones: vec![ZonePolygon {
            layer: "F.Cu".to_string(),
            points: vec![
                Point::new(bounds.min_x_mm, bounds.min_y_mm),
                Point::new(bounds.max_x_mm, bounds.min_y_mm),
                Point::new(bounds.max_x_mm, bounds.max_y_mm),
                Point::new(bounds.min_x_mm, bounds.max_y_mm),
            ],
            name: board.title.clone().or_else(|| Some(board.name.clone())),
            keepout: false,
        }],
    }
}

fn preview_content_bounds(board: &Board) -> Bounds {
    let mut bounds = board
        .components
        .iter()
        .map(component_preview_bounds)
        .collect::<Vec<_>>();

    if bounds.is_empty() {
        return Bounds::new(0.0, 0.0, 10.0, 10.0);
    }

    let first = bounds.remove(0);
    bounds.into_iter().fold(first, merge_bounds)
}

fn preview_margin_mm(board: &Board) -> f64 {
    match board.name.as_str() {
        "arduino_mega_2560_rev3" => 8.0,
        "arduino_nano_v3" => 5.0,
        _ => 4.0,
    }
}

fn preview_footprint_from_component(component: &Component) -> FootprintLayout {
    let component_bounds = component_preview_bounds(component);
    let label_text = component
        .value
        .clone()
        .unwrap_or_else(|| component.reference.clone());

    FootprintLayout {
        reference: component.reference.clone(),
        footprint: component.footprint.clone(),
        layer: component.layer.clone(),
        position: component_origin(component),
        pads: component
            .pads
            .iter()
            .map(|pad| PadGeometry {
                component: component.reference.clone(),
                number: pad.number.clone(),
                shape: pad.shape.clone(),
                pad_type: pad.pad_type.clone(),
                position: absolute_pad_point(component, pad.position.as_ref()),
                size_mm: pad.size_mm.unwrap_or((1.2, 1.2)),
                layers: pad.layers.clone(),
                net_name: pad.net_name.clone(),
                rotation_deg: pad
                    .position
                    .as_ref()
                    .and_then(|position| position.rotation_deg)
                    .or_else(|| {
                        component
                            .position
                            .as_ref()
                            .and_then(|position| position.rotation_deg)
                    }),
                drill_mm: pad.drill_mm.clone(),
                display_layer: Some(component.layer.clone()),
            })
            .collect(),
        graphics: rectangle_lines(
            &component_bounds,
            "F.SilkS",
            match component.kind.as_str() {
                "mcu" => 0.35,
                _ => 0.2,
            },
            Some(component.reference.clone()),
            Some(component.kind.clone()),
            None,
        ),
        label: Some(TextPrimitive {
            text: label_text,
            position: Point::new(
                (component_bounds.min_x_mm + component_bounds.max_x_mm) * 0.5,
                component_bounds.min_y_mm - 1.6,
            ),
            layer: "F.SilkS".to_string(),
            owner: Some(component.reference.clone()),
            size_mm: Some((3.2, 1.6)),
            rotation_deg: None,
        }),
        rotation_deg: component
            .position
            .as_ref()
            .and_then(|position| position.rotation_deg),
    }
}

fn component_preview_bounds(component: &Component) -> Bounds {
    let origin = component_origin(component);
    let pad_points = component
        .pads
        .iter()
        .map(|pad| absolute_pad_point(component, pad.position.as_ref()))
        .collect::<Vec<_>>();
    let pad_bounds = points_bounds(&pad_points).unwrap_or_else(|| {
        Bounds::new(
            origin.x_mm - 1.0,
            origin.y_mm - 1.0,
            origin.x_mm + 1.0,
            origin.y_mm + 1.0,
        )
    });

    let body_bounds = match component.kind.as_str() {
        "connector" => {
            let width = 3.8;
            let horizontal = pad_bounds.width_mm() > pad_bounds.height_mm();
            if horizontal {
                Bounds::new(
                    pad_bounds.min_x_mm - 1.5,
                    pad_bounds.min_y_mm - (width * 0.5),
                    pad_bounds.max_x_mm + 1.5,
                    pad_bounds.max_y_mm + (width * 0.5),
                )
            } else {
                Bounds::new(
                    pad_bounds.min_x_mm - (width * 0.5),
                    pad_bounds.min_y_mm - 1.5,
                    pad_bounds.max_x_mm + (width * 0.5),
                    pad_bounds.max_y_mm + 1.5,
                )
            }
        }
        "mcu" => Bounds::new(
            origin.x_mm - 8.5,
            origin.y_mm - 8.5,
            origin.x_mm + 8.5,
            origin.y_mm + 8.5,
        ),
        "module" => Bounds::new(
            origin.x_mm - 6.5,
            origin.y_mm - 6.5,
            origin.x_mm + 6.5,
            origin.y_mm + 6.5,
        ),
        _ => pad_bounds.clone().expand(1.5),
    };

    merge_bounds(pad_bounds.expand(0.8), body_bounds)
}

fn component_origin(component: &Component) -> Point {
    component
        .position
        .as_ref()
        .map(|position| Point::new(position.x_mm, position.y_mm))
        .unwrap_or_else(|| Point::new(0.0, 0.0))
}

fn absolute_pad_point(component: &Component, position: Option<&rust_board::Position>) -> Point {
    let origin = component_origin(component);
    let local = position
        .map(|position| Point::new(position.x_mm, position.y_mm))
        .unwrap_or_else(|| Point::new(0.0, 0.0));
    Point::new(origin.x_mm + local.x_mm, origin.y_mm + local.y_mm)
}

fn points_bounds(points: &[Point]) -> Option<Bounds> {
    let mut iter = points.iter();
    let first = iter.next()?;
    let mut min_x = first.x_mm;
    let mut min_y = first.y_mm;
    let mut max_x = first.x_mm;
    let mut max_y = first.y_mm;

    for point in iter {
        min_x = min_x.min(point.x_mm);
        min_y = min_y.min(point.y_mm);
        max_x = max_x.max(point.x_mm);
        max_y = max_y.max(point.y_mm);
    }

    Some(Bounds::new(min_x, min_y, max_x, max_y))
}

fn merge_bounds(left: Bounds, right: Bounds) -> Bounds {
    Bounds::new(
        left.min_x_mm.min(right.min_x_mm),
        left.min_y_mm.min(right.min_y_mm),
        left.max_x_mm.max(right.max_x_mm),
        left.max_y_mm.max(right.max_y_mm),
    )
}

fn rectangle_lines(
    bounds: &Bounds,
    layer: &str,
    width_mm: f64,
    owner: Option<String>,
    owner_kind: Option<String>,
    net_name: Option<String>,
) -> Vec<LinePrimitive> {
    let top_left = Point::new(bounds.min_x_mm, bounds.min_y_mm);
    let top_right = Point::new(bounds.max_x_mm, bounds.min_y_mm);
    let bottom_right = Point::new(bounds.max_x_mm, bounds.max_y_mm);
    let bottom_left = Point::new(bounds.min_x_mm, bounds.max_y_mm);

    [
        (top_left.clone(), top_right.clone()),
        (top_right, bottom_right.clone()),
        (bottom_right, bottom_left.clone()),
        (bottom_left, top_left),
    ]
    .into_iter()
    .map(|(start, end)| LinePrimitive {
        start,
        end,
        layer: layer.to_string(),
        width_mm,
        owner: owner.clone(),
        owner_kind: owner_kind.clone(),
        net_name: net_name.clone(),
        stroke_type: None,
    })
    .collect()
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct NetElementCounts {
    tracks: usize,
    pads: usize,
    vias: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NetTooltipSummary {
    net_name: String,
    counts: NetElementCounts,
    controller_signals: Vec<String>,
    module_signals: Vec<String>,
    is_active: bool,
    is_controller_bound: bool,
    is_module_connected: bool,
}

struct Projector {
    bounds: Bounds,
    scale: f32,
    offset: Vec2,
}

impl Projector {
    fn new(rect: Rect, bounds: &Bounds) -> Self {
        let margin = 12.0f32;
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

pub fn render_pcb(
    ui: &mut egui::Ui,
    loaded: &LoadedPcb,
    bindings: &[SignalBinding],
    module_overlays: &[ModuleOverlay],
    module_nets: &BTreeSet<String>,
    active_nets: &BTreeSet<String>,
) {
    let desired = ui.available_size_before_wrap();
    let desired = Vec2::new(desired.x.max(240.0), desired.y.max(220.0));
    let (rect, response) = ui.allocate_exact_size(desired, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, Color32::from_rgb(20, 26, 30));

    let projector = Projector::new(rect, &loaded.layout.bounds);
    let highlighted_nets = bindings
        .iter()
        .map(|binding| binding.pcb_net.clone())
        .collect::<BTreeSet<_>>();

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
        paint_line(
            &painter,
            &projector,
            line,
            &highlighted_nets,
            module_nets,
            active_nets,
            false,
        );
    }
    for line in &loaded.layout.tracks {
        paint_line(
            &painter,
            &projector,
            line,
            &highlighted_nets,
            module_nets,
            active_nets,
            true,
        );
    }
    for line in &loaded.layout.edge_cuts {
        paint_line(
            &painter,
            &projector,
            line,
            &highlighted_nets,
            module_nets,
            active_nets,
            false,
        );
    }
    for footprint in &loaded.layout.footprints {
        for graphic in &footprint.graphics {
            paint_line(
                &painter,
                &projector,
                graphic,
                &highlighted_nets,
                module_nets,
                active_nets,
                false,
            );
        }
        for pad in &footprint.pads {
            paint_pad(
                &painter,
                &projector,
                pad,
                &highlighted_nets,
                module_nets,
                active_nets,
            );
        }
        if let Some(label) = &footprint.label {
            let position = projector.point(&label.position);
            painter.text(
                position,
                egui::Align2::CENTER_CENTER,
                &label.text,
                egui::FontId::monospace(8.0),
                Color32::from_gray(140),
            );
        }
    }
    for via in &loaded.layout.vias {
        paint_via(
            &painter,
            &projector,
            via,
            &highlighted_nets,
            module_nets,
            active_nets,
        );
    }

    if response.hovered() {
        painter.rect_stroke(
            rect.shrink(1.0),
            6.0,
            Stroke::new(1.0, Color32::from_rgb(90, 130, 170)),
            egui::StrokeKind::Outside,
        );
    }

    if let Some(hover_pos) = response.hover_pos() {
        if let Some(net_name) = hovered_net_name(loaded, &projector, hover_pos) {
            let summary =
                net_tooltip_summary(loaded, &net_name, bindings, module_overlays, active_nets);
            response.clone().on_hover_ui_at_pointer(|ui| {
                draw_net_tooltip(ui, &summary);
            });
        }
    }
}

fn paint_line(
    painter: &egui::Painter,
    projector: &Projector,
    line: &LinePrimitive,
    highlighted_nets: &BTreeSet<String>,
    module_nets: &BTreeSet<String>,
    active_nets: &BTreeSet<String>,
    is_track: bool,
) {
    let highlighted = line
        .net_name
        .as_ref()
        .map(|net| highlighted_nets.contains(net))
        .unwrap_or(false);
    let module_connected = line
        .net_name
        .as_ref()
        .map(|net| module_nets.contains(net))
        .unwrap_or(false);
    let active = line
        .net_name
        .as_ref()
        .map(|net| active_nets.contains(net))
        .unwrap_or(false);
    let color = if active {
        Color32::from_rgb(255, 72, 72)
    } else if highlighted {
        Color32::from_rgb(255, 194, 76)
    } else if module_connected {
        Color32::from_rgb(96, 210, 210)
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
    highlighted_nets: &BTreeSet<String>,
    module_nets: &BTreeSet<String>,
    active_nets: &BTreeSet<String>,
) {
    let center = projector.point(&pad.position);
    let highlighted = pad
        .net_name
        .as_ref()
        .map(|net| highlighted_nets.contains(net))
        .unwrap_or(false);
    let module_connected = pad
        .net_name
        .as_ref()
        .map(|net| module_nets.contains(net))
        .unwrap_or(false);
    let active = pad
        .net_name
        .as_ref()
        .map(|net| active_nets.contains(net))
        .unwrap_or(false);
    let fill = if active {
        Color32::from_rgb(255, 86, 86)
    } else if highlighted {
        Color32::from_rgb(255, 214, 86)
    } else if module_connected {
        Color32::from_rgb(96, 210, 210)
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
    highlighted_nets: &BTreeSet<String>,
    module_nets: &BTreeSet<String>,
    active_nets: &BTreeSet<String>,
) {
    let center = projector.point(&via.position);
    let highlighted = via
        .net_name
        .as_ref()
        .map(|net| highlighted_nets.contains(net))
        .unwrap_or(false);
    let module_connected = via
        .net_name
        .as_ref()
        .map(|net| module_nets.contains(net))
        .unwrap_or(false);
    let active = via
        .net_name
        .as_ref()
        .map(|net| active_nets.contains(net))
        .unwrap_or(false);
    let radius = ((via.size_mm as f32) * projector.scale * 0.5).max(1.2);
    let color = if active {
        Color32::from_rgb(255, 86, 86)
    } else if highlighted {
        Color32::from_rgb(255, 214, 86)
    } else if module_connected {
        Color32::from_rgb(96, 210, 210)
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

fn hovered_net_name(loaded: &LoadedPcb, projector: &Projector, hover_pos: Pos2) -> Option<String> {
    let mut best: Option<(f32, String)> = None;

    for line in &loaded.layout.tracks {
        let Some(net_name) = &line.net_name else {
            continue;
        };
        let start = projector.point(&line.start);
        let end = projector.point(&line.end);
        let distance = distance_to_segment(hover_pos, start, end);
        let threshold = (projector.stroke_width(line.width_mm, 1.0) * 0.5) + 5.0;
        if distance <= threshold {
            consider_hover_candidate(&mut best, distance, net_name);
        }
    }

    for footprint in &loaded.layout.footprints {
        for pad in &footprint.pads {
            let Some(net_name) = &pad.net_name else {
                continue;
            };
            let distance = distance_to_pad(hover_pos, projector, pad);
            if distance <= 5.0 {
                consider_hover_candidate(&mut best, distance, net_name);
            }
        }
    }

    for via in &loaded.layout.vias {
        let Some(net_name) = &via.net_name else {
            continue;
        };
        let center = projector.point(&via.position);
        let radius = ((via.size_mm as f32) * projector.scale * 0.5).max(1.2);
        let distance = hover_pos.distance(center) - radius;
        if distance <= 5.0 {
            consider_hover_candidate(&mut best, distance.max(0.0), net_name);
        }
    }

    best.map(|(_, net_name)| net_name)
}

fn consider_hover_candidate(best: &mut Option<(f32, String)>, distance: f32, net_name: &str) {
    match best {
        Some((best_distance, _)) if *best_distance <= distance => {}
        _ => *best = Some((distance, net_name.to_string())),
    }
}

fn distance_to_pad(hover_pos: Pos2, projector: &Projector, pad: &PadGeometry) -> f32 {
    let center = projector.point(&pad.position);
    match pad.shape.as_str() {
        "round" | "circle" => {
            let radius =
                ((pad.size_mm.0.max(pad.size_mm.1) as f32) * projector.scale * 0.5).max(1.5);
            (hover_pos.distance(center) - radius).max(0.0)
        }
        _ => {
            let polygon = rotated_rect_points(
                center,
                pad.size_mm,
                pad.rotation_deg.unwrap_or(0.0),
                projector.scale,
            );
            if point_in_polygon(hover_pos, &polygon) {
                0.0
            } else {
                distance_to_polygon_outline(hover_pos, &polygon)
            }
        }
    }
}

fn point_in_polygon(point: Pos2, polygon: &[Pos2]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut previous = polygon[polygon.len() - 1];
    for current in polygon {
        let intersects = ((current.y > point.y) != (previous.y > point.y))
            && (point.x
                < (previous.x - current.x) * (point.y - current.y) / (previous.y - current.y)
                    + current.x);
        if intersects {
            inside = !inside;
        }
        previous = *current;
    }
    inside
}

fn distance_to_polygon_outline(point: Pos2, polygon: &[Pos2]) -> f32 {
    polygon
        .iter()
        .copied()
        .zip(polygon.iter().copied().cycle().skip(1))
        .take(polygon.len())
        .map(|(start, end)| distance_to_segment(point, start, end))
        .fold(f32::INFINITY, f32::min)
}

fn distance_to_segment(point: Pos2, start: Pos2, end: Pos2) -> f32 {
    let delta = end - start;
    let length_squared = delta.length_sq();
    if length_squared <= f32::EPSILON {
        return point.distance(start);
    }
    let projection = ((point - start).dot(delta) / length_squared).clamp(0.0, 1.0);
    let projected = start + (delta * projection);
    point.distance(projected)
}

fn net_tooltip_summary(
    loaded: &LoadedPcb,
    net_name: &str,
    bindings: &[SignalBinding],
    module_overlays: &[ModuleOverlay],
    active_nets: &BTreeSet<String>,
) -> NetTooltipSummary {
    let mut counts = NetElementCounts::default();
    for line in &loaded.layout.tracks {
        if line.net_name.as_deref() == Some(net_name) {
            counts.tracks += 1;
        }
    }
    for footprint in &loaded.layout.footprints {
        for pad in &footprint.pads {
            if pad.net_name.as_deref() == Some(net_name) {
                counts.pads += 1;
            }
        }
    }
    for via in &loaded.layout.vias {
        if via.net_name.as_deref() == Some(net_name) {
            counts.vias += 1;
        }
    }

    let mut controller_signals = bindings
        .iter()
        .filter(|binding| binding.pcb_net == net_name)
        .map(|binding| binding.board_signal.clone())
        .collect::<Vec<_>>();
    controller_signals.sort();
    controller_signals.dedup();

    let mut module_signals = module_overlays
        .iter()
        .flat_map(|module| {
            module.bindings.iter().filter_map(|binding| {
                (binding.pcb_net == net_name)
                    .then(|| format!("{}:{}", module.name, binding.module_signal))
            })
        })
        .collect::<Vec<_>>();
    module_signals.sort();
    module_signals.dedup();

    NetTooltipSummary {
        net_name: net_name.to_string(),
        counts,
        is_active: active_nets.contains(net_name),
        is_controller_bound: !controller_signals.is_empty(),
        is_module_connected: !module_signals.is_empty(),
        controller_signals,
        module_signals,
    }
}

fn draw_net_tooltip(ui: &mut egui::Ui, summary: &NetTooltipSummary) {
    ui.set_min_width(240.0);
    ui.label(RichText::new(&summary.net_name).strong());
    ui.small(format!(
        "Tracks: {}  Pads: {}  Vias: {}",
        summary.counts.tracks, summary.counts.pads, summary.counts.vias
    ));

    let status = if summary.is_active {
        "Active"
    } else if summary.is_controller_bound && summary.is_module_connected {
        "Controller + module net"
    } else if summary.is_controller_bound {
        "Controller-connected net"
    } else if summary.is_module_connected {
        "Module-connected net"
    } else {
        "PCB-only net"
    };
    ui.small(format!("Status: {status}"));

    if summary.controller_signals.is_empty() {
        ui.small("Controller: none");
    } else {
        ui.small(format!(
            "Controller: {}",
            summary.controller_signals.join(", ")
        ));
    }

    if summary.module_signals.is_empty() {
        ui.small("Modules: none");
    } else {
        ui.small(format!("Modules: {}", summary.module_signals.join(", ")));
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use eframe::egui::pos2;
    use rust_board::load_built_in_board_model;
    use rust_project::{BindingMode, ModuleOverlay, ModuleSignalBinding, SignalBinding};

    use super::{distance_to_segment, net_tooltip_summary, point_in_polygon, LoadedPcb};

    fn examples_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/pcbs")
    }

    fn example_pcb_path(file_name: &str) -> PathBuf {
        examples_root().join(file_name)
    }

    #[test]
    fn loaded_pcb_exposes_sorted_net_names() {
        let loaded = LoadedPcb::load(&example_pcb_path("air_node.kicad_pcb")).expect("loaded pcb");
        assert!(!loaded.net_names.is_empty());
        assert_eq!(loaded.net_names.first().map(String::as_str), Some("+24V"));
    }

    #[test]
    fn built_in_board_preview_generates_renderable_layout() {
        let nano = LoadedPcb::preview(
            load_built_in_board_model("arduino_nano_v3").expect("nano built-in board"),
        );
        let mega = LoadedPcb::preview(
            load_built_in_board_model("arduino_mega_2560_rev3").expect("mega built-in board"),
        );

        assert!(!nano.layout.footprints.is_empty());
        assert!(!nano.layout.edge_cuts.is_empty());
        assert!(nano.layout.bounds.width_mm() > 0.0);
        assert!(nano.layout.bounds.height_mm() > 0.0);
        assert!(nano.net_names.contains(&"D13_SCK".to_string()));

        assert!(!mega.layout.footprints.is_empty());
        assert!(!mega.layout.edge_cuts.is_empty());
        assert!(mega.layout.bounds.width_mm() > nano.layout.bounds.width_mm());
        assert!(mega.net_names.contains(&"D53_SS".to_string()));
    }

    #[test]
    fn mega_kicad_preview_filter_keeps_readable_layers() {
        let mega = LoadedPcb::load(&example_pcb_path("arduino_mega_2560_rev3e.kicad_pcb"))
            .expect("mega preview pcb")
            .simplified_preview();

        assert!(!mega.layout.edge_cuts.is_empty());
        assert!(mega
            .layout
            .drawings
            .iter()
            .all(|line| { matches!(line.layer.as_str(), "F.SilkS" | "B.SilkS" | "Edge.Cuts") }));
        assert!(mega
            .layout
            .tracks
            .iter()
            .all(|line| matches!(line.layer.as_str(), "F.Cu" | "B.Cu")));
        assert!(mega
            .layout
            .footprints
            .iter()
            .all(|footprint| footprint.label.is_none()));
        assert!(mega.layout.bounds.width_mm() > mega.layout.bounds.height_mm());
    }

    #[test]
    fn nano_kicad_preview_filter_keeps_readable_layers() {
        let nano = LoadedPcb::load(&example_pcb_path("arduino_nano_v3_3.kicad_pcb"))
            .expect("nano preview pcb")
            .simplified_preview();

        assert!(!nano.layout.edge_cuts.is_empty());
        assert!(nano
            .layout
            .drawings
            .iter()
            .all(|line| { matches!(line.layer.as_str(), "F.SilkS" | "B.SilkS" | "Edge.Cuts") }));
        assert!(nano
            .layout
            .tracks
            .iter()
            .all(|line| matches!(line.layer.as_str(), "F.Cu" | "B.Cu")));
        assert!(nano
            .layout
            .footprints
            .iter()
            .all(|footprint| footprint.label.is_none()));
        assert!(nano.layout.bounds.width_mm() > 0.0);
        assert!(nano.layout.bounds.height_mm() > 0.0);
    }

    #[test]
    fn net_tooltip_summary_reports_bound_signals_and_geometry_counts() {
        let loaded = LoadedPcb::load(&example_pcb_path("air_node.kicad_pcb")).expect("loaded pcb");

        let summary = net_tooltip_summary(
            &loaded,
            "CAN_H",
            &[SignalBinding {
                board_signal: "D10".to_string(),
                pcb_net: "CAN_H".to_string(),
                mode: BindingMode::Bus,
                note: None,
            }],
            &[ModuleOverlay {
                name: "module_1".to_string(),
                model: "mcp2515_tja1050_can_module".to_string(),
                bindings: vec![ModuleSignalBinding {
                    module_signal: "CANH".to_string(),
                    pcb_net: "CAN_H".to_string(),
                    mode: BindingMode::Bus,
                    note: None,
                }],
            }],
            &["CAN_H".to_string()].into_iter().collect(),
        );

        assert_eq!(summary.net_name, "CAN_H");
        assert_eq!(summary.controller_signals, vec!["D10".to_string()]);
        assert_eq!(summary.module_signals, vec!["module_1:CANH".to_string()]);
        assert!(summary.is_active);
        assert!(summary.is_controller_bound);
        assert!(summary.is_module_connected);
        assert!(summary.counts.tracks > 0 || summary.counts.pads > 0 || summary.counts.vias > 0);
    }

    #[test]
    fn point_in_polygon_and_segment_distance_handle_basic_geometry() {
        let square = vec![
            pos2(0.0, 0.0),
            pos2(10.0, 0.0),
            pos2(10.0, 10.0),
            pos2(0.0, 10.0),
        ];
        assert!(point_in_polygon(pos2(5.0, 5.0), &square));
        assert!(!point_in_polygon(pos2(15.0, 5.0), &square));
        assert_eq!(
            distance_to_segment(pos2(5.0, 5.0), pos2(0.0, 0.0), pos2(10.0, 0.0)),
            5.0
        );
    }
}
