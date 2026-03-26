"""Native textual DSL rendering for avrsim boards."""

from __future__ import annotations

from .dsl import Board, Component, Net, Pad, Position


def _quote(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    return f'"{escaped}"'


def _line(indent: int, text: str) -> str:
    return ("  " * indent) + text


def _render_scalar(name: str, value: str | None, indent: int) -> list[str]:
    if value is None:
        return [_line(indent, f"({name} null)")]
    return [_line(indent, f"({name} {_quote(value)})")]


def _render_position(name: str, position: Position | None, indent: int) -> list[str]:
    if position is None:
        return [_line(indent, f"({name} null)")]
    if position.rotation_deg is None:
        return [_line(indent, f"({name} {position.x_mm:g} {position.y_mm:g})")]
    return [
        _line(
            indent,
            f"({name} {position.x_mm:g} {position.y_mm:g} {position.rotation_deg:g})",
        )
    ]


def _render_string_list(name: str, values: list[str], indent: int) -> list[str]:
    if not values:
        return [_line(indent, f"({name})")]
    joined = " ".join(_quote(value) for value in values)
    return [_line(indent, f"({name} {joined})")]


def _render_float_tuple(name: str, values: tuple[float, ...] | None, indent: int) -> list[str]:
    if values is None:
        return [_line(indent, f"({name} null)")]
    joined = " ".join(f"{value:g}" for value in values)
    return [_line(indent, f"({name} {joined})")]


def _render_pad(pad: Pad, indent: int) -> list[str]:
    lines = [_line(indent, "(pad")]
    lines.extend(_render_scalar("number", pad.number, indent + 1))
    lines.extend(_render_scalar("pad_type", pad.pad_type, indent + 1))
    lines.extend(_render_scalar("shape", pad.shape, indent + 1))
    lines.extend(_render_string_list("layers", pad.layers, indent + 1))
    lines.extend(_render_scalar("net", pad.net_name, indent + 1))
    if pad.net_code is None:
        lines.append(_line(indent + 1, "(net_code null)"))
    else:
        lines.append(_line(indent + 1, f"(net_code {pad.net_code})"))
    lines.extend(_render_position("position", pad.position, indent + 1))
    lines.extend(_render_float_tuple("size_mm", pad.size_mm, indent + 1))
    lines.extend(_render_float_tuple("drill_mm", pad.drill_mm, indent + 1))
    lines.extend(_render_scalar("uuid", pad.uuid, indent + 1))
    lines.append(_line(indent, ")"))
    return lines


def _render_component(component: Component, indent: int) -> list[str]:
    lines = [_line(indent, "(component")]
    lines.extend(_render_scalar("reference", component.reference, indent + 1))
    lines.extend(_render_scalar("value", component.value, indent + 1))
    lines.extend(_render_scalar("kind", component.kind, indent + 1))
    lines.extend(_render_scalar("footprint", component.footprint, indent + 1))
    lines.extend(_render_scalar("layer", component.layer, indent + 1))
    lines.extend(_render_position("position", component.position, indent + 1))
    lines.extend(_render_scalar("uuid", component.uuid, indent + 1))

    lines.append(_line(indent + 1, "(properties"))
    for key, value in sorted(component.properties.items()):
        lines.append(_line(indent + 2, f"(property {_quote(key)} {_quote(value)})"))
    lines.append(_line(indent + 1, ")"))

    for pad in component.pads:
        lines.extend(_render_pad(pad, indent + 1))

    lines.append(_line(indent, ")"))
    return lines


def _render_net(net: Net, indent: int) -> list[str]:
    lines = [_line(indent, "(net")]
    lines.extend(_render_scalar("name", net.name, indent + 1))
    if net.code is None:
        lines.append(_line(indent + 1, "(code null)"))
    else:
        lines.append(_line(indent + 1, f"(code {net.code})"))
    for connection in net.connections:
        lines.append(
            _line(
                indent + 1,
                "(connect "
                + " ".join(
                    [
                        _quote(connection.component),
                        _quote(connection.pad),
                        _quote(connection.component_kind or ""),
                    ]
                )
                + ")",
            )
        )
    lines.append(_line(indent, ")"))
    return lines


def dump_board_dsl(board: Board) -> str:
    lines = ["(board"]
    lines.extend(_render_scalar("name", board.name, 1))
    lines.extend(_render_scalar("title", board.title, 1))
    lines.extend(_render_scalar("source_format", board.source_format, 1))
    lines.extend(_render_scalar("source_path", board.source_path, 1))
    lines.extend(_render_scalar("generator", board.generator, 1))
    lines.extend(_render_scalar("generator_version", board.generator_version, 1))
    lines.extend(_render_scalar("board_version", board.board_version, 1))
    lines.extend(_render_scalar("paper", board.paper, 1))
    lines.extend(_render_string_list("layers", board.layers, 1))

    for component in board.components:
        lines.extend(_render_component(component, 1))

    for net in board.nets:
        lines.extend(_render_net(net, 1))

    lines.append(")")
    return "\n".join(lines) + "\n"
