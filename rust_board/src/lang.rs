use std::fmt::Write as _;

use crate::dsl::{Board, Component, Net, Pad, Position};

fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn line(indent: usize, text: &str) -> String {
    format!("{}{}", "  ".repeat(indent), text)
}

fn render_scalar(name: &str, value: Option<&str>, indent: usize, lines: &mut Vec<String>) {
    match value {
        Some(value) => lines.push(line(indent, &format!("({name} {})", quote(value)))),
        None => lines.push(line(indent, &format!("({name} null)"))),
    }
}

fn render_position(
    name: &str,
    position: Option<&Position>,
    indent: usize,
    lines: &mut Vec<String>,
) {
    match position {
        Some(position) => {
            if let Some(rotation) = position.rotation_deg {
                lines.push(line(
                    indent,
                    &format!(
                        "({name} {} {} {})",
                        trim_float(position.x_mm),
                        trim_float(position.y_mm),
                        trim_float(rotation)
                    ),
                ));
            } else {
                lines.push(line(
                    indent,
                    &format!(
                        "({name} {} {})",
                        trim_float(position.x_mm),
                        trim_float(position.y_mm)
                    ),
                ));
            }
        }
        None => lines.push(line(indent, &format!("({name} null)"))),
    }
}

fn render_string_list(name: &str, values: &[String], indent: usize, lines: &mut Vec<String>) {
    if values.is_empty() {
        lines.push(line(indent, &format!("({name})")));
        return;
    }
    let joined = values
        .iter()
        .map(|value| quote(value))
        .collect::<Vec<_>>()
        .join(" ");
    lines.push(line(indent, &format!("({name} {joined})")));
}

fn render_float_tuple(name: &str, values: Option<&[f64]>, indent: usize, lines: &mut Vec<String>) {
    match values {
        Some(values) => {
            let joined = values
                .iter()
                .map(|value| trim_float(*value))
                .collect::<Vec<_>>()
                .join(" ");
            lines.push(line(indent, &format!("({name} {joined})")));
        }
        None => lines.push(line(indent, &format!("({name} null)"))),
    }
}

fn render_pad(pad: &Pad, indent: usize, lines: &mut Vec<String>) {
    lines.push(line(indent, "(pad"));
    render_scalar("number", Some(&pad.number), indent + 1, lines);
    render_scalar("pad_type", Some(&pad.pad_type), indent + 1, lines);
    render_scalar("shape", Some(&pad.shape), indent + 1, lines);
    render_string_list("layers", &pad.layers, indent + 1, lines);
    render_scalar("net", pad.net_name.as_deref(), indent + 1, lines);
    match pad.net_code {
        Some(value) => lines.push(line(indent + 1, &format!("(net_code {value})"))),
        None => lines.push(line(indent + 1, "(net_code null)")),
    }
    render_position("position", pad.position.as_ref(), indent + 1, lines);
    render_float_tuple(
        "size_mm",
        pad.size_mm
            .as_ref()
            .map(|(x, y)| [*x, *y])
            .as_ref()
            .map(|value| value.as_slice()),
        indent + 1,
        lines,
    );
    render_float_tuple("drill_mm", pad.drill_mm.as_deref(), indent + 1, lines);
    render_scalar("uuid", pad.uuid.as_deref(), indent + 1, lines);
    lines.push(line(indent, ")"));
}

fn render_component(component: &Component, indent: usize, lines: &mut Vec<String>) {
    lines.push(line(indent, "(component"));
    render_scalar("reference", Some(&component.reference), indent + 1, lines);
    render_scalar("value", component.value.as_deref(), indent + 1, lines);
    render_scalar("kind", Some(&component.kind), indent + 1, lines);
    render_scalar("footprint", Some(&component.footprint), indent + 1, lines);
    render_scalar("layer", Some(&component.layer), indent + 1, lines);
    render_position("position", component.position.as_ref(), indent + 1, lines);
    render_scalar("uuid", component.uuid.as_deref(), indent + 1, lines);
    lines.push(line(indent + 1, "(properties"));
    for (key, value) in &component.properties {
        lines.push(line(
            indent + 2,
            &format!("(property {} {})", quote(key), quote(value)),
        ));
    }
    lines.push(line(indent + 1, ")"));
    for pad in &component.pads {
        render_pad(pad, indent + 1, lines);
    }
    lines.push(line(indent, ")"));
}

fn render_net(net: &Net, indent: usize, lines: &mut Vec<String>) {
    lines.push(line(indent, "(net"));
    render_scalar("name", Some(&net.name), indent + 1, lines);
    match net.code {
        Some(value) => lines.push(line(indent + 1, &format!("(code {value})"))),
        None => lines.push(line(indent + 1, "(code null)")),
    }
    for connection in &net.connections {
        lines.push(line(
            indent + 1,
            &format!(
                "(connect {} {} {})",
                quote(&connection.component),
                quote(&connection.pad),
                quote(connection.component_kind.as_deref().unwrap_or(""))
            ),
        ));
    }
    lines.push(line(indent, ")"));
}

pub fn dump_board_dsl(board: &Board) -> String {
    let mut lines = vec!["(board".to_string()];
    render_scalar("name", Some(&board.name), 1, &mut lines);
    render_scalar("title", board.title.as_deref(), 1, &mut lines);
    render_scalar("source_format", Some(&board.source_format), 1, &mut lines);
    render_scalar("source_path", Some(&board.source_path), 1, &mut lines);
    render_scalar("generator", board.generator.as_deref(), 1, &mut lines);
    render_scalar(
        "generator_version",
        board.generator_version.as_deref(),
        1,
        &mut lines,
    );
    render_scalar(
        "board_version",
        board.board_version.as_deref(),
        1,
        &mut lines,
    );
    render_scalar("paper", board.paper.as_deref(), 1, &mut lines);
    render_string_list("layers", &board.layers, 1, &mut lines);
    for component in &board.components {
        render_component(component, 1, &mut lines);
    }
    for net in &board.nets {
        render_net(net, 1, &mut lines);
    }
    lines.push(")".to_string());
    let mut output = String::new();
    for line in lines {
        let _ = writeln!(output, "{line}");
    }
    output
}

fn trim_float(value: f64) -> String {
    let mut rendered = format!("{value}");
    if rendered.contains('.') {
        while rendered.ends_with('0') {
            rendered.pop();
        }
        if rendered.ends_with('.') {
            rendered.pop();
        }
    }
    rendered
}
