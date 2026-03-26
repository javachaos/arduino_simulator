"""Import KiCad PCB files into the avrsim board DSL."""

from __future__ import annotations

from pathlib import Path

from .dsl import Board, Component, Pad, Position, derive_nets
from .kicad_sexpr import SExpr, parse_sexpr


def _node_name(node: SExpr) -> str | None:
    if isinstance(node, list) and node and isinstance(node[0], str):
        return node[0]
    return None


def _iter_children(node: SExpr, child_name: str) -> list[list[SExpr]]:
    if not isinstance(node, list):
        return []
    return [
        child
        for child in node[1:]
        if isinstance(child, list) and child and child[0] == child_name
    ]


def _first_child(node: SExpr, child_name: str) -> list[SExpr] | None:
    children = _iter_children(node, child_name)
    if children:
        return children[0]
    return None


def _first_atom(node: list[SExpr], index: int = 1) -> str | None:
    if len(node) <= index or not isinstance(node[index], str):
        return None
    return node[index]


def _parse_float(token: str | None) -> float | None:
    if token is None:
        return None
    return float(token)


def _parse_position(node: list[SExpr] | None) -> Position | None:
    if node is None:
        return None

    x_mm = _parse_float(_first_atom(node, 1))
    y_mm = _parse_float(_first_atom(node, 2))
    rotation_deg = _parse_float(_first_atom(node, 3))
    if x_mm is None or y_mm is None:
        return None
    return Position(x_mm=x_mm, y_mm=y_mm, rotation_deg=rotation_deg)


def _parse_float_tuple(node: list[SExpr] | None, start_index: int = 1) -> tuple[float, ...] | None:
    if node is None:
        return None

    values: list[float] = []
    for value in node[start_index:]:
        if isinstance(value, list):
            break
        values.append(float(value))
    if not values:
        return None
    return tuple(values)


def _parse_property_map(footprint: list[SExpr]) -> dict[str, str]:
    properties: dict[str, str] = {}
    for property_node in _iter_children(footprint, "property"):
        if len(property_node) >= 3 and isinstance(property_node[1], str) and isinstance(property_node[2], str):
            properties[property_node[1]] = property_node[2]
    return properties


def _infer_component_kind(reference: str, footprint: str) -> str:
    prefix = "".join(character for character in reference if character.isalpha()).upper()
    footprint_upper = footprint.upper()
    if "CONNECTOR" in footprint_upper or "PINHEADER" in footprint_upper or "RJ45" in footprint_upper:
        return "connector"
    if prefix.startswith("RJ"):
        return "connector"
    if prefix in {"J", "K", "P", "CN"}:
        return "connector"
    if prefix in {"R"}:
        return "resistor"
    if prefix in {"C"}:
        return "capacitor"
    if prefix in {"D"}:
        return "diode"
    if prefix in {"F"}:
        return "fuse"
    if prefix in {"L"}:
        return "inductor"
    if prefix in {"Q"}:
        return "transistor"
    if prefix in {"U", "IC"}:
        return "integrated_circuit"
    if prefix in {"SW"}:
        return "switch"
    if prefix in {"Y", "X"}:
        return "crystal"
    if prefix in {"TP"}:
        return "testpoint"
    return "component"


def _parse_pad(pad_node: list[SExpr]) -> Pad:
    number = _first_atom(pad_node, 1) or ""
    pad_type = _first_atom(pad_node, 2) or ""
    shape = _first_atom(pad_node, 3) or ""
    layers_node = _first_child(pad_node, "layers")
    at_node = _first_child(pad_node, "at")
    size_node = _first_child(pad_node, "size")
    drill_node = _first_child(pad_node, "drill")
    net_node = _first_child(pad_node, "net")
    uuid = _first_atom(_first_child(pad_node, "uuid") or [], 1)

    net_name: str | None = None
    net_code: int | None = None
    if net_node is not None:
        if len(net_node) >= 3 and isinstance(net_node[1], str) and net_node[1].lstrip("-").isdigit():
            net_code = int(net_node[1])
            if isinstance(net_node[2], str):
                net_name = net_node[2]
        elif len(net_node) >= 2 and isinstance(net_node[1], str):
            net_name = net_node[1]

    layers = []
    if layers_node is not None:
        layers = [value for value in layers_node[1:] if isinstance(value, str)]

    return Pad(
        number=number,
        pad_type=pad_type,
        shape=shape,
        layers=layers,
        net_name=net_name,
        net_code=net_code,
        position=_parse_position(at_node),
        size_mm=_parse_float_tuple(size_node),
        drill_mm=_parse_float_tuple(drill_node),
        uuid=uuid,
    )


def _parse_component(footprint: list[SExpr]) -> Component:
    footprint_name = _first_atom(footprint, 1) or ""
    layer_node = _first_child(footprint, "layer")
    uuid_node = _first_child(footprint, "uuid")
    properties = _parse_property_map(footprint)
    reference = properties.get("Reference", "")
    value = properties.get("Value")
    pads = [
        _parse_pad(child)
        for child in footprint[1:]
        if isinstance(child, list) and child and child[0] == "pad"
    ]

    return Component(
        reference=reference,
        value=value,
        kind=_infer_component_kind(reference, footprint_name),
        footprint=footprint_name,
        layer=_first_atom(layer_node or [], 1) or "",
        position=_parse_position(_first_child(footprint, "at")),
        uuid=_first_atom(uuid_node or [], 1),
        properties=properties,
        pads=pads,
    )


def _component_sort_key(component: Component) -> tuple[str, str]:
    return (component.reference, component.footprint)


def board_from_kicad_pcb(pcb_path: str | Path) -> Board:
    path = Path(pcb_path).resolve()
    root = parse_sexpr(path.read_text(encoding="utf-8"))
    if not isinstance(root, list) or not root or root[0] != "kicad_pcb":
        raise ValueError(f"{path} is not a KiCad PCB file")

    title_block = _first_child(root, "title_block")
    title = None
    if title_block is not None:
        title_node = _first_child(title_block, "title")
        title = _first_atom(title_node or [], 1)

    layers_node = _first_child(root, "layers")
    layer_names: list[str] = []
    if layers_node is not None:
        for child in layers_node[1:]:
            if isinstance(child, list) and len(child) >= 2 and isinstance(child[1], str):
                layer_names.append(child[1])

    components = sorted(
        [
            _parse_component(child)
            for child in root[1:]
            if isinstance(child, list) and child and child[0] == "footprint"
        ],
        key=_component_sort_key,
    )

    return Board(
        name=path.stem,
        title=title,
        source_format="kicad_pcb",
        source_path=str(path),
        generator=_first_atom(_first_child(root, "generator") or [], 1),
        generator_version=_first_atom(_first_child(root, "generator_version") or [], 1),
        board_version=_first_atom(_first_child(root, "version") or [], 1),
        paper=_first_atom(_first_child(root, "paper") or [], 1),
        layers=layer_names,
        components=components,
        nets=derive_nets(components),
    )
