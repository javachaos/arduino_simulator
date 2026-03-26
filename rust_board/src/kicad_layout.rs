use std::fs;
use std::path::Path;

use crate::dsl::DslError;
use crate::layout::{
    BoardLayout, Bounds, CirclePrimitive, FootprintLayout, LinePrimitive, PadGeometry, Point,
    TextPrimitive, ViaGeometry, ZonePolygon,
};
use crate::sexpr::{parse_sexpr, SExpr};

fn node_name(node: &SExpr) -> Option<&str> {
    let list = node.as_list()?;
    list.first()?.as_atom()
}

fn iter_children<'a>(list: &'a [SExpr], child_name: &str) -> Vec<&'a [SExpr]> {
    list.iter()
        .skip(1)
        .filter_map(SExpr::as_list)
        .filter(|child| child.first().and_then(SExpr::as_atom) == Some(child_name))
        .collect()
}

fn first_child<'a>(list: &'a [SExpr], child_name: &str) -> Option<&'a [SExpr]> {
    iter_children(list, child_name).into_iter().next()
}

fn first_atom(node: Option<&[SExpr]>, index: usize) -> Option<&str> {
    let list = node?;
    list.get(index)?.as_atom()
}

fn parse_float(token: Option<&str>) -> Option<f64> {
    token?.parse::<f64>().ok()
}

fn parse_point(node: Option<&[SExpr]>, start_index: usize) -> Option<Point> {
    let x_mm = parse_float(first_atom(node, start_index))?;
    let y_mm = parse_float(first_atom(node, start_index + 1))?;
    Some(Point::new(x_mm, y_mm))
}

fn parse_position(node: Option<&[SExpr]>) -> Option<(Point, Option<f64>)> {
    let point = parse_point(node, 1)?;
    let rotation_deg = parse_float(first_atom(node, 3));
    Some((point, rotation_deg))
}

fn parse_size(node: Option<&[SExpr]>) -> Option<(f64, f64)> {
    let width_mm = parse_float(first_atom(node, 1))?;
    let height_mm = parse_float(first_atom(node, 2))?;
    Some((width_mm, height_mm))
}

fn parse_stroke_width(node: &[SExpr]) -> f64 {
    parse_float(
        first_child(node, "stroke")
            .and_then(|stroke| first_child(stroke, "width"))
            .and_then(|width| first_atom(Some(width), 1)),
    )
    .unwrap_or(0.15)
}

fn parse_stroke_type(node: &[SExpr]) -> Option<String> {
    first_child(node, "stroke")
        .and_then(|stroke| first_child(stroke, "type"))
        .and_then(|stroke_type| first_atom(Some(stroke_type), 1))
        .map(str::to_string)
}

fn parse_text_size(node: &[SExpr]) -> Option<(f64, f64)> {
    parse_size(
        first_child(node, "effects")
            .and_then(|effects| first_child(effects, "font"))
            .and_then(|font| first_child(font, "size")),
    )
}

fn parse_property_map(footprint: &[SExpr]) -> Vec<(String, String)> {
    footprint
        .iter()
        .skip(1)
        .filter_map(SExpr::as_list)
        .filter(|child| child.first().and_then(SExpr::as_atom) == Some("property"))
        .filter_map(|property_node| {
            Some((
                property_node.get(1)?.as_atom()?.to_string(),
                property_node.get(2)?.as_atom()?.to_string(),
            ))
        })
        .collect()
}

fn parse_polygon_points(node: Option<&[SExpr]>) -> Vec<Point> {
    let Some(polygon) = node else {
        return Vec::new();
    };
    let Some(pts_node) = first_child(polygon, "pts") else {
        return Vec::new();
    };
    pts_node
        .iter()
        .skip(1)
        .filter(|child| node_name(child) == Some("xy"))
        .filter_map(|child| parse_point(child.as_list(), 1))
        .collect()
}

fn rotate_point(point: &Point, rotation_deg: f64) -> Point {
    let radians = (-rotation_deg).to_radians();
    let cosine = radians.cos();
    let sine = radians.sin();
    Point::new(
        (point.x_mm * cosine) - (point.y_mm * sine),
        (point.x_mm * sine) + (point.y_mm * cosine),
    )
}

fn transform_local_point(
    local_point: &Point,
    origin: &Point,
    rotation_deg: f64,
    mirrored: bool,
) -> Point {
    let maybe_mirrored = if mirrored {
        Point::new(-local_point.x_mm, local_point.y_mm)
    } else {
        local_point.clone()
    };
    let rotated = rotate_point(&maybe_mirrored, rotation_deg);
    Point::new(origin.x_mm + rotated.x_mm, origin.y_mm + rotated.y_mm)
}

fn points_from_rect(start: &Point, end: &Point) -> Vec<(Point, Point)> {
    let top_left = Point::new(start.x_mm.min(end.x_mm), start.y_mm.min(end.y_mm));
    let bottom_right = Point::new(start.x_mm.max(end.x_mm), start.y_mm.max(end.y_mm));
    let top_right = Point::new(bottom_right.x_mm, top_left.y_mm);
    let bottom_left = Point::new(top_left.x_mm, bottom_right.y_mm);
    vec![
        (top_left.clone(), top_right.clone()),
        (top_right.clone(), bottom_right.clone()),
        (bottom_right.clone(), bottom_left.clone()),
        (bottom_left, top_left),
    ]
}

fn parse_line_primitive(
    node: &[SExpr],
    layer: String,
    width_mm: f64,
    owner: Option<String>,
    owner_kind: Option<String>,
    net_name: Option<String>,
) -> Option<LinePrimitive> {
    let start = parse_point(first_child(node, "start"), 1)?;
    let end = parse_point(first_child(node, "end"), 1)?;
    Some(LinePrimitive {
        start,
        end,
        layer,
        width_mm,
        owner,
        owner_kind,
        net_name,
        stroke_type: parse_stroke_type(node),
    })
}

fn transform_line(
    local_start: &Point,
    local_end: &Point,
    origin: &Point,
    rotation_deg: f64,
    mirrored: bool,
    layer: String,
    width_mm: f64,
    owner: String,
    owner_kind: String,
) -> LinePrimitive {
    LinePrimitive {
        start: transform_local_point(local_start, origin, rotation_deg, mirrored),
        end: transform_local_point(local_end, origin, rotation_deg, mirrored),
        layer,
        width_mm,
        owner: Some(owner),
        owner_kind: Some(owner_kind),
        net_name: None,
        stroke_type: Some("solid".to_string()),
    }
}

fn parse_footprint_graphics(
    footprint: &[SExpr],
    reference: &str,
    origin: &Point,
    rotation_deg: f64,
    mirrored: bool,
) -> Vec<LinePrimitive> {
    let mut graphics = Vec::new();
    for child in footprint.iter().skip(1).filter_map(SExpr::as_list) {
        let tag = child.first().and_then(SExpr::as_atom).unwrap_or_default();
        let Some(layer) = first_atom(first_child(child, "layer"), 1).map(str::to_string) else {
            continue;
        };
        let width_mm = parse_stroke_width(child);
        match tag {
            "fp_line" => {
                let Some(start) = parse_point(first_child(child, "start"), 1) else {
                    continue;
                };
                let Some(end) = parse_point(first_child(child, "end"), 1) else {
                    continue;
                };
                graphics.push(transform_line(
                    &start,
                    &end,
                    origin,
                    rotation_deg,
                    mirrored,
                    layer,
                    width_mm,
                    reference.to_string(),
                    "footprint_graphic".to_string(),
                ));
            }
            "fp_rect" => {
                let Some(start) = parse_point(first_child(child, "start"), 1) else {
                    continue;
                };
                let Some(end) = parse_point(first_child(child, "end"), 1) else {
                    continue;
                };
                for (rect_start, rect_end) in points_from_rect(&start, &end) {
                    graphics.push(transform_line(
                        &rect_start,
                        &rect_end,
                        origin,
                        rotation_deg,
                        mirrored,
                        layer.clone(),
                        width_mm,
                        reference.to_string(),
                        "footprint_graphic".to_string(),
                    ));
                }
            }
            _ => {}
        }
    }
    graphics
}

fn derive_pad_display_layer(footprint_layer: &str, pad_layers: &[String]) -> String {
    if footprint_layer.starts_with("B.") {
        return "B.Cu".to_string();
    }
    if footprint_layer.starts_with("F.") {
        return "F.Cu".to_string();
    }
    if pad_layers.iter().any(|layer| layer == "B.Cu")
        && !pad_layers.iter().any(|layer| layer == "F.Cu")
    {
        return "B.Cu".to_string();
    }
    "F.Cu".to_string()
}

fn parse_footprint_layout(footprint: &[SExpr]) -> Option<FootprintLayout> {
    let footprint_name = footprint.get(1).and_then(SExpr::as_atom)?.to_string();
    let layer = first_atom(first_child(footprint, "layer"), 1)?.to_string();
    let (origin, rotation_deg) = parse_position(first_child(footprint, "at"))?;
    let properties = parse_property_map(footprint);
    let reference = properties
        .iter()
        .find(|(name, _)| name == "Reference")
        .map(|(_, value)| value.clone())
        .unwrap_or_default();
    let mirrored = false;
    let graphics = parse_footprint_graphics(
        footprint,
        &reference,
        &origin,
        rotation_deg.unwrap_or(0.0),
        mirrored,
    );

    let mut pads = Vec::new();
    for pad_node in footprint
        .iter()
        .skip(1)
        .filter_map(SExpr::as_list)
        .filter(|child| child.first().and_then(SExpr::as_atom) == Some("pad"))
    {
        let number = pad_node
            .get(1)
            .and_then(SExpr::as_atom)
            .unwrap_or_default()
            .to_string();
        let pad_type = pad_node
            .get(2)
            .and_then(SExpr::as_atom)
            .unwrap_or_default()
            .to_string();
        let shape = pad_node
            .get(3)
            .and_then(SExpr::as_atom)
            .unwrap_or_default()
            .to_string();
        let Some(size_mm) = parse_size(first_child(pad_node, "size")) else {
            continue;
        };
        let (local_point, pad_rotation_deg) = parse_position(first_child(pad_node, "at"))
            .unwrap_or((Point::new(0.0, 0.0), Some(0.0)));
        let absolute_position =
            transform_local_point(&local_point, &origin, rotation_deg.unwrap_or(0.0), mirrored);
        let layers = first_child(pad_node, "layers")
            .map(|node| {
                node.iter()
                    .skip(1)
                    .filter_map(SExpr::as_atom)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let net_name = first_child(pad_node, "net").and_then(|node| {
            first_atom(Some(node), 2)
                .or_else(|| first_atom(Some(node), 1))
                .map(str::to_string)
        });
        let drill_mm = first_child(pad_node, "drill").and_then(|node| {
            let mut values = Vec::new();
            for value in node.iter().skip(1).filter_map(SExpr::as_atom) {
                let Ok(parsed) = value.parse::<f64>() else {
                    continue;
                };
                values.push(parsed);
            }
            if values.is_empty() {
                None
            } else {
                Some(values)
            }
        });
        let absolute_rotation = rotation_deg.unwrap_or(0.0) + pad_rotation_deg.unwrap_or(0.0);
        pads.push(PadGeometry {
            component: reference.clone(),
            number,
            shape,
            pad_type,
            position: absolute_position,
            size_mm,
            layers: layers.clone(),
            net_name,
            rotation_deg: Some(absolute_rotation),
            drill_mm,
            display_layer: Some(derive_pad_display_layer(&layer, &layers)),
        });
    }

    let label = Some(TextPrimitive {
        text: reference.clone(),
        position: origin.clone(),
        layer: if layer.contains(".Cu") {
            layer.replace(".Cu", ".SilkS")
        } else {
            layer.clone()
        },
        owner: Some(reference.clone()),
        size_mm: Some((1.0, 1.0)),
        rotation_deg,
    });

    Some(FootprintLayout {
        reference,
        footprint: footprint_name,
        layer,
        position: origin,
        pads,
        graphics,
        label,
        rotation_deg,
    })
}

fn circle_from_node(node: &[SExpr], layer: String, width_mm: f64) -> Option<CirclePrimitive> {
    let center = parse_point(first_child(node, "center"), 1)?;
    let end = parse_point(first_child(node, "end"), 1)?;
    let radius_mm = ((end.x_mm - center.x_mm).powi(2) + (end.y_mm - center.y_mm).powi(2)).sqrt();
    Some(CirclePrimitive {
        center,
        radius_mm,
        layer,
        width_mm,
        owner: None,
        owner_kind: None,
        fill: false,
    })
}

fn bounds_from_layout(
    footprints: &[FootprintLayout],
    edge_cuts: &[LinePrimitive],
    drawings: &[LinePrimitive],
    circles: &[CirclePrimitive],
    tracks: &[LinePrimitive],
    vias: &[ViaGeometry],
    zones: &[ZonePolygon],
    texts: &[TextPrimitive],
) -> Bounds {
    let mut xs = Vec::new();
    let mut ys = Vec::new();

    for primitive in edge_cuts.iter().chain(drawings.iter()).chain(tracks.iter()) {
        xs.push(primitive.start.x_mm);
        ys.push(primitive.start.y_mm);
        xs.push(primitive.end.x_mm);
        ys.push(primitive.end.y_mm);
    }

    for circle in circles {
        xs.push(circle.center.x_mm - circle.radius_mm);
        xs.push(circle.center.x_mm + circle.radius_mm);
        ys.push(circle.center.y_mm - circle.radius_mm);
        ys.push(circle.center.y_mm + circle.radius_mm);
    }

    for footprint in footprints {
        xs.push(footprint.position.x_mm);
        ys.push(footprint.position.y_mm);
        for pad in &footprint.pads {
            xs.push(pad.position.x_mm);
            ys.push(pad.position.y_mm);
            xs.push(pad.position.x_mm - (pad.size_mm.0 / 2.0));
            xs.push(pad.position.x_mm + (pad.size_mm.0 / 2.0));
            ys.push(pad.position.y_mm - (pad.size_mm.1 / 2.0));
            ys.push(pad.position.y_mm + (pad.size_mm.1 / 2.0));
        }
        for graphic in &footprint.graphics {
            xs.push(graphic.start.x_mm);
            ys.push(graphic.start.y_mm);
            xs.push(graphic.end.x_mm);
            ys.push(graphic.end.y_mm);
        }
    }

    for via in vias {
        xs.push(via.position.x_mm - (via.size_mm / 2.0));
        xs.push(via.position.x_mm + (via.size_mm / 2.0));
        ys.push(via.position.y_mm - (via.size_mm / 2.0));
        ys.push(via.position.y_mm + (via.size_mm / 2.0));
    }

    for zone in zones {
        for point in &zone.points {
            xs.push(point.x_mm);
            ys.push(point.y_mm);
        }
    }

    for text in texts {
        xs.push(text.position.x_mm);
        ys.push(text.position.y_mm);
    }

    if xs.is_empty() || ys.is_empty() {
        return Bounds::new(0.0, 0.0, 100.0, 100.0);
    }

    Bounds::new(
        xs.iter().copied().fold(f64::INFINITY, f64::min),
        ys.iter().copied().fold(f64::INFINITY, f64::min),
        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    )
    .expand(3.0)
}

pub fn layout_from_kicad_pcb(path: impl AsRef<Path>) -> Result<BoardLayout, DslError> {
    let path = path.as_ref().canonicalize().map_err(DslError::from)?;
    let text = fs::read_to_string(&path).map_err(DslError::from)?;
    let root = parse_sexpr(&text)?;
    let root_list = root
        .as_list()
        .ok_or_else(|| DslError::new(format!("{} is not a KiCad PCB file", path.display())))?;
    if root_list.first().and_then(SExpr::as_atom) != Some("kicad_pcb") {
        return Err(DslError::new(format!(
            "{} is not a KiCad PCB file",
            path.display()
        )));
    }

    let mut footprints = root_list
        .iter()
        .skip(1)
        .filter(|child| node_name(child) == Some("footprint"))
        .filter_map(SExpr::as_list)
        .filter_map(parse_footprint_layout)
        .collect::<Vec<_>>();
    footprints.sort_by(|left, right| left.reference.cmp(&right.reference));

    let mut edge_cuts = Vec::new();
    let mut drawings = Vec::new();
    let mut circles = Vec::new();
    let mut texts = Vec::new();
    let mut tracks = Vec::new();
    let mut vias = Vec::new();
    let mut zones = Vec::new();

    for child in root_list.iter().skip(1).filter_map(SExpr::as_list) {
        let tag = child.first().and_then(SExpr::as_atom).unwrap_or_default();
        match tag {
            "gr_line" | "gr_rect" => {
                let layer = first_atom(first_child(child, "layer"), 1)
                    .unwrap_or_default()
                    .to_string();
                let width_mm = parse_stroke_width(child);
                if tag == "gr_line" {
                    if let Some(primitive) =
                        parse_line_primitive(child, layer.clone(), width_mm, None, None, None)
                    {
                        if layer == "Edge.Cuts" {
                            edge_cuts.push(primitive);
                        } else {
                            drawings.push(primitive);
                        }
                    }
                } else {
                    let Some(start) = parse_point(first_child(child, "start"), 1) else {
                        continue;
                    };
                    let Some(end) = parse_point(first_child(child, "end"), 1) else {
                        continue;
                    };
                    let target = if layer == "Edge.Cuts" {
                        &mut edge_cuts
                    } else {
                        &mut drawings
                    };
                    for (rect_start, rect_end) in points_from_rect(&start, &end) {
                        target.push(LinePrimitive {
                            start: rect_start,
                            end: rect_end,
                            layer: layer.clone(),
                            width_mm,
                            owner: None,
                            owner_kind: None,
                            net_name: None,
                            stroke_type: parse_stroke_type(child),
                        });
                    }
                }
            }
            "gr_circle" => {
                let layer = first_atom(first_child(child, "layer"), 1)
                    .unwrap_or_default()
                    .to_string();
                if let Some(circle) = circle_from_node(child, layer, parse_stroke_width(child)) {
                    circles.push(circle);
                }
            }
            "gr_text" => {
                let Some((position, rotation_deg)) = parse_position(first_child(child, "at"))
                else {
                    continue;
                };
                let Some(text) = child.get(1).and_then(SExpr::as_atom).map(str::to_string) else {
                    continue;
                };
                let layer = first_atom(first_child(child, "layer"), 1)
                    .unwrap_or_default()
                    .to_string();
                texts.push(TextPrimitive {
                    text,
                    position,
                    layer,
                    owner: None,
                    size_mm: parse_text_size(child),
                    rotation_deg,
                });
            }
            "segment" => {
                let layer = first_atom(first_child(child, "layer"), 1)
                    .unwrap_or_default()
                    .to_string();
                let width_mm =
                    parse_float(first_atom(first_child(child, "width"), 1)).unwrap_or(0.2);
                let net_name = first_atom(first_child(child, "net"), 1).map(str::to_string);
                if let Some(primitive) = parse_line_primitive(
                    child,
                    layer,
                    width_mm,
                    None,
                    Some("track".to_string()),
                    net_name,
                ) {
                    tracks.push(primitive);
                }
            }
            "via" => {
                let Some(position) = parse_point(first_child(child, "at"), 1) else {
                    continue;
                };
                let Some(size_mm) = parse_float(first_atom(first_child(child, "size"), 1)) else {
                    continue;
                };
                let drill_mm = parse_float(first_atom(first_child(child, "drill"), 1));
                let layers = first_child(child, "layers")
                    .map(|node| {
                        node.iter()
                            .skip(1)
                            .filter_map(SExpr::as_atom)
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let net_name = first_atom(first_child(child, "net"), 1).map(str::to_string);
                vias.push(ViaGeometry {
                    position,
                    size_mm,
                    layers,
                    net_name,
                    drill_mm,
                });
            }
            "zone" => {
                let layer = first_atom(first_child(child, "layer"), 1)
                    .unwrap_or_default()
                    .to_string();
                let points = parse_polygon_points(first_child(child, "polygon"));
                if !points.is_empty() {
                    zones.push(ZonePolygon {
                        layer,
                        points,
                        name: first_atom(first_child(child, "name"), 1).map(str::to_string),
                        keepout: first_child(child, "keepout").is_some(),
                    });
                }
            }
            _ => {}
        }
    }

    let bounds = bounds_from_layout(
        &footprints,
        &edge_cuts,
        &drawings,
        &circles,
        &tracks,
        &vias,
        &zones,
        &texts,
    );

    Ok(BoardLayout {
        name: path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string(),
        source_path: path.display().to_string(),
        bounds,
        footprints,
        edge_cuts,
        drawings,
        circles,
        texts,
        tracks,
        vias,
        zones,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::layout_from_kicad_pcb;

    fn examples_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("examples/pcbs")
            .to_path_buf()
    }

    fn example_pcb_path(file_name: &str) -> PathBuf {
        examples_root().join(file_name)
    }

    #[test]
    fn imports_air_node_layout_geometry() {
        let layout = layout_from_kicad_pcb(example_pcb_path("air_node.kicad_pcb"))
            .expect("layout");
        assert_eq!(layout.name, "air_node");
        assert!(!layout.footprints.is_empty());
        assert!(!layout.edge_cuts.is_empty());
        assert!(!layout.tracks.is_empty());
        assert!(layout.bounds.width_mm() > 0.0);
    }

    #[test]
    fn imports_mega_sidecar_layout_geometry() {
        let layout = layout_from_kicad_pcb(example_pcb_path(
            "mega_r3_sidecar_controller_rev_a.kicad_pcb",
        ))
        .expect("layout");
        assert_eq!(layout.name, "mega_r3_sidecar_controller_rev_a");
        assert!(layout
            .footprints
            .iter()
            .any(|footprint| footprint.reference == "P_DIG"));
        assert!(layout.vias.len() > 10);
    }
}
