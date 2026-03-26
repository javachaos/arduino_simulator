use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::dsl::{derive_nets, Board, Component, DslError, Pad, Position};
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

fn parse_int(token: Option<&str>) -> Option<i32> {
    token?.parse::<i32>().ok()
}

fn parse_position(node: Option<&[SExpr]>) -> Option<Position> {
    let x_mm = parse_float(first_atom(node, 1))?;
    let y_mm = parse_float(first_atom(node, 2))?;
    let rotation_deg = parse_float(first_atom(node, 3));
    Some(Position::new(x_mm, y_mm, rotation_deg))
}

fn parse_float_tuple(node: Option<&[SExpr]>, start_index: usize) -> Option<(f64, f64)> {
    let list = node?;
    let first = list.get(start_index)?.as_atom()?.parse::<f64>().ok()?;
    let second = list.get(start_index + 1)?.as_atom()?.parse::<f64>().ok()?;
    Some((first, second))
}

fn parse_float_vec(node: Option<&[SExpr]>, start_index: usize) -> Option<Vec<f64>> {
    let list = node?;
    let mut values = Vec::new();
    for value in list.iter().skip(start_index) {
        let Some(atom) = value.as_atom() else {
            break;
        };
        let Ok(parsed) = atom.parse::<f64>() else {
            break;
        };
        values.push(parsed);
    }
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

fn parse_property_map(footprint: &[SExpr]) -> BTreeMap<String, String> {
    let mut properties = BTreeMap::new();
    for property_node in footprint
        .iter()
        .skip(1)
        .filter_map(SExpr::as_list)
        .filter(|child| child.first().and_then(SExpr::as_atom) == Some("property"))
    {
        if let (Some(name), Some(value)) = (
            property_node.get(1).and_then(SExpr::as_atom),
            property_node.get(2).and_then(SExpr::as_atom),
        ) {
            properties.insert(name.to_string(), value.to_string());
        }
    }
    properties
}

fn infer_component_kind(reference: &str, footprint: &str) -> String {
    let prefix = reference
        .chars()
        .take_while(|character| character.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase();
    let footprint_upper = footprint.to_ascii_uppercase();
    if footprint_upper.contains("CONNECTOR")
        || footprint_upper.contains("PINHEADER")
        || footprint_upper.contains("RJ45")
        || prefix.starts_with("RJ")
        || matches!(prefix.as_str(), "J" | "K" | "P" | "CN")
    {
        return "connector".to_string();
    }
    match prefix.as_str() {
        "R" => "resistor",
        "C" => "capacitor",
        "D" => "diode",
        "F" => "fuse",
        "L" => "inductor",
        "Q" => "transistor",
        "U" | "IC" => "integrated_circuit",
        "SW" => "switch",
        "Y" | "X" => "crystal",
        "TP" => "testpoint",
        _ => "component",
    }
    .to_string()
}

fn parse_pad(pad_node: &[SExpr]) -> Pad {
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

    let layers = first_child(pad_node, "layers")
        .map(|node| {
            node.iter()
                .skip(1)
                .filter_map(SExpr::as_atom)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let net_node = first_child(pad_node, "net");
    let mut net_name = None;
    let mut net_code = None;
    if let Some(node) = net_node {
        if let Some(code) = parse_int(first_atom(Some(node), 1)) {
            net_code = Some(code);
            net_name = first_atom(Some(node), 2).map(str::to_string);
        } else {
            net_name = first_atom(Some(node), 1).map(str::to_string);
        }
    }

    Pad {
        number,
        pad_type,
        shape,
        layers,
        net_name,
        net_code,
        position: parse_position(first_child(pad_node, "at")),
        size_mm: parse_float_tuple(first_child(pad_node, "size"), 1),
        drill_mm: parse_float_vec(first_child(pad_node, "drill"), 1),
        uuid: first_atom(first_child(pad_node, "uuid"), 1).map(str::to_string),
    }
}

fn parse_component(footprint: &[SExpr]) -> Component {
    let footprint_name = footprint
        .get(1)
        .and_then(SExpr::as_atom)
        .unwrap_or_default()
        .to_string();
    let properties = parse_property_map(footprint);
    let reference = properties.get("Reference").cloned().unwrap_or_default();
    let value = properties.get("Value").cloned();
    let pads = footprint
        .iter()
        .skip(1)
        .filter_map(SExpr::as_list)
        .filter(|child| child.first().and_then(SExpr::as_atom) == Some("pad"))
        .map(parse_pad)
        .collect::<Vec<_>>();

    Component {
        reference: reference.clone(),
        value,
        kind: infer_component_kind(&reference, &footprint_name),
        footprint: footprint_name,
        layer: first_atom(first_child(footprint, "layer"), 1)
            .unwrap_or_default()
            .to_string(),
        position: parse_position(first_child(footprint, "at")),
        uuid: first_atom(first_child(footprint, "uuid"), 1).map(str::to_string),
        properties,
        pads,
    }
}

fn component_sort_key(component: &Component) -> (&str, &str) {
    (&component.reference, &component.footprint)
}

pub fn board_from_kicad_pcb(path: impl AsRef<Path>) -> Result<Board, DslError> {
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

    let title = first_child(root_list, "title_block")
        .and_then(|title_block| first_child(title_block, "title"))
        .and_then(|title_node| first_atom(Some(title_node), 1))
        .map(str::to_string);

    let layers = first_child(root_list, "layers")
        .map(|layers_node| {
            layers_node
                .iter()
                .skip(1)
                .filter_map(SExpr::as_list)
                .filter_map(|child| child.get(1).and_then(SExpr::as_atom))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut components = root_list
        .iter()
        .skip(1)
        .filter(|child| node_name(child) == Some("footprint"))
        .filter_map(SExpr::as_list)
        .map(parse_component)
        .collect::<Vec<_>>();
    components.sort_by(|left, right| component_sort_key(left).cmp(&component_sort_key(right)));

    Ok(Board {
        name: path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string(),
        title,
        source_format: "kicad_pcb".to_string(),
        source_path: path.display().to_string(),
        generator: first_atom(first_child(root_list, "generator"), 1).map(str::to_string),
        generator_version: first_atom(first_child(root_list, "generator_version"), 1)
            .map(str::to_string),
        board_version: first_atom(first_child(root_list, "version"), 1).map(str::to_string),
        paper: first_atom(first_child(root_list, "paper"), 1).map(str::to_string),
        layers,
        nets: derive_nets(&components),
        components,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::board_from_kicad_pcb;

    fn examples_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/pcbs")
    }

    fn example_pcb_path(file_name: &str) -> PathBuf {
        examples_root().join(file_name)
    }

    fn component_pad_net<'a>(
        board: &'a crate::dsl::Board,
        reference: &str,
        pad_number: &str,
    ) -> Option<&'a str> {
        board
            .components
            .iter()
            .find(|component| component.reference == reference)?
            .pads
            .iter()
            .find(|pad| pad.number == pad_number)?
            .net_name
            .as_deref()
    }

    #[test]
    fn imports_air_node_board() {
        let board = board_from_kicad_pcb(example_pcb_path("air_node.kicad_pcb")).expect("board");
        assert_eq!(board.name, "air_node");
        assert_eq!(board.generator.as_deref(), Some("pcbnew"));
        assert_eq!(component_pad_net(&board, "RJ45", "1"), Some("CAN_H"));
        assert_eq!(component_pad_net(&board, "RJ45", "8"), Some("GND"));
    }

    #[test]
    fn imports_mega_sidecar_board() {
        let board = board_from_kicad_pcb(example_pcb_path(
            "mega_r3_sidecar_controller_rev_a.kicad_pcb",
        ))
        .expect("board");
        assert_eq!(
            board.title.as_deref(),
            Some("Mega R3 Sidecar Controller Rev A")
        );
        assert_eq!(component_pad_net(&board, "K1", "1"), Some("CANH"));
        assert_eq!(component_pad_net(&board, "J101", "5"), Some("/27"));
    }
}
