"""Import KiCad board geometry for avrsim rendering."""

from __future__ import annotations

import math
from pathlib import Path

from .kicad_sexpr import SExpr, parse_sexpr
from .layout import (
    BoardLayout,
    Bounds,
    CirclePrimitive,
    FootprintLayout,
    LinePrimitive,
    PadGeometry,
    Point,
    TextPrimitive,
    ViaGeometry,
    ZonePolygon,
)


def _iter_children(node: SExpr, child_name: str) -> list[list[SExpr]]:
    if not isinstance(node, list):
        return []
    return [
        child
        for child in node[1:]
        if isinstance(child, list) and child and child[0] == child_name
    ]


def _first_child(node: list[SExpr] | None, child_name: str) -> list[SExpr] | None:
    if node is None:
        return None
    children = _iter_children(node, child_name)
    if children:
        return children[0]
    return None


def _first_atom(node: list[SExpr] | None, index: int = 1) -> str | None:
    if node is None or len(node) <= index or not isinstance(node[index], str):
        return None
    return node[index]


def _parse_float(token: str | None) -> float | None:
    if token is None:
        return None
    return float(token)


def _parse_point(node: list[SExpr] | None, start_index: int = 1) -> Point | None:
    if node is None:
        return None
    x_mm = _parse_float(_first_atom(node, start_index))
    y_mm = _parse_float(_first_atom(node, start_index + 1))
    if x_mm is None or y_mm is None:
        return None
    return Point(x_mm=x_mm, y_mm=y_mm)


def _parse_position(node: list[SExpr] | None) -> tuple[Point, float | None] | None:
    point = _parse_point(node)
    if point is None:
        return None
    return (point, _parse_float(_first_atom(node, 3)))


def _parse_size(node: list[SExpr] | None) -> tuple[float, float] | None:
    if node is None:
        return None
    width_mm = _parse_float(_first_atom(node, 1))
    height_mm = _parse_float(_first_atom(node, 2))
    if width_mm is None or height_mm is None:
        return None
    return (width_mm, height_mm)


def _parse_stroke_width(node: list[SExpr] | None) -> float:
    stroke_node = _first_child(node, "stroke")
    width_node = _first_child(stroke_node, "width")
    return _parse_float(_first_atom(width_node, 1)) or 0.15


def _parse_stroke_type(node: list[SExpr] | None) -> str | None:
    stroke_node = _first_child(node, "stroke")
    type_node = _first_child(stroke_node, "type")
    return _first_atom(type_node, 1)


def _parse_text_size(node: list[SExpr] | None) -> tuple[float, float] | None:
    effects_node = _first_child(node, "effects")
    font_node = _first_child(effects_node, "font")
    size_node = _first_child(font_node, "size")
    return _parse_size(size_node)


def _parse_property_map(footprint: list[SExpr]) -> dict[str, str]:
    properties: dict[str, str] = {}
    for property_node in _iter_children(footprint, "property"):
        if len(property_node) >= 3 and isinstance(property_node[1], str) and isinstance(property_node[2], str):
            properties[property_node[1]] = property_node[2]
    return properties


def _parse_polygon_points(node: list[SExpr] | None) -> list[Point]:
    pts_node = _first_child(node, "pts")
    if pts_node is None:
        return []
    points: list[Point] = []
    for child in pts_node[1:]:
        if isinstance(child, list) and child and child[0] == "xy":
            point = _parse_point(child)
            if point is not None:
                points.append(point)
    return points


def _rotate_point(point: Point, rotation_deg: float) -> Point:
    # KiCad board coordinates render with the opposite visible rotation sign
    # from standard Cartesian math, so negate the footprint angle here.
    radians = math.radians(-rotation_deg)
    cosine = math.cos(radians)
    sine = math.sin(radians)
    return Point(
        x_mm=(point.x_mm * cosine) - (point.y_mm * sine),
        y_mm=(point.x_mm * sine) + (point.y_mm * cosine),
    )


def _transform_local_point(
    local_point: Point,
    origin: Point,
    rotation_deg: float,
    mirrored: bool,
) -> Point:
    transformed = local_point
    if mirrored:
        transformed = Point(x_mm=-transformed.x_mm, y_mm=transformed.y_mm)
    transformed = _rotate_point(transformed, rotation_deg)
    return Point(
        x_mm=origin.x_mm + transformed.x_mm,
        y_mm=origin.y_mm + transformed.y_mm,
    )


def _points_from_rect(start: Point, end: Point) -> list[tuple[Point, Point]]:
    top_left = Point(min(start.x_mm, end.x_mm), min(start.y_mm, end.y_mm))
    bottom_right = Point(max(start.x_mm, end.x_mm), max(start.y_mm, end.y_mm))
    top_right = Point(bottom_right.x_mm, top_left.y_mm)
    bottom_left = Point(top_left.x_mm, bottom_right.y_mm)
    return [
        (top_left, top_right),
        (top_right, bottom_right),
        (bottom_right, bottom_left),
        (bottom_left, top_left),
    ]


def _parse_line_primitive(
    node: list[SExpr],
    *,
    layer: str,
    width_mm: float,
    owner: str | None = None,
    owner_kind: str | None = None,
    net_name: str | None = None,
) -> LinePrimitive | None:
    start = _parse_point(_first_child(node, "start"))
    end = _parse_point(_first_child(node, "end"))
    if start is None or end is None:
        return None
    return LinePrimitive(
        start=start,
        end=end,
        layer=layer,
        width_mm=width_mm,
        owner=owner,
        owner_kind=owner_kind,
        net_name=net_name,
        stroke_type=_parse_stroke_type(node),
    )


def _transform_line(
    local_start: Point,
    local_end: Point,
    *,
    origin: Point,
    rotation_deg: float,
    mirrored: bool,
    layer: str,
    width_mm: float,
    owner: str,
    owner_kind: str,
) -> LinePrimitive:
    return LinePrimitive(
        start=_transform_local_point(local_start, origin, rotation_deg, mirrored),
        end=_transform_local_point(local_end, origin, rotation_deg, mirrored),
        layer=layer,
        width_mm=width_mm,
        owner=owner,
        owner_kind=owner_kind,
        stroke_type="solid",
    )


def _parse_footprint_graphics(
    footprint: list[SExpr],
    *,
    reference: str,
    origin: Point,
    rotation_deg: float,
    mirrored: bool,
) -> list[LinePrimitive]:
    graphics: list[LinePrimitive] = []
    for child in footprint[1:]:
        if not isinstance(child, list) or not child:
            continue
        tag = child[0]
        layer = _first_atom(_first_child(child, "layer"), 1)
        if layer is None:
            continue
        width_mm = _parse_stroke_width(child)
        if tag == "fp_line":
            start = _parse_point(_first_child(child, "start"))
            end = _parse_point(_first_child(child, "end"))
            if start is None or end is None:
                continue
            graphics.append(
                _transform_line(
                    start,
                    end,
                    origin=origin,
                    rotation_deg=rotation_deg,
                    mirrored=mirrored,
                    layer=layer,
                    width_mm=width_mm,
                    owner=reference,
                    owner_kind="footprint_graphic",
                )
            )
        elif tag == "fp_rect":
            start = _parse_point(_first_child(child, "start"))
            end = _parse_point(_first_child(child, "end"))
            if start is None or end is None:
                continue
            for rect_start, rect_end in _points_from_rect(start, end):
                graphics.append(
                    _transform_line(
                        rect_start,
                        rect_end,
                        origin=origin,
                        rotation_deg=rotation_deg,
                        mirrored=mirrored,
                        layer=layer,
                        width_mm=width_mm,
                        owner=reference,
                        owner_kind="footprint_graphic",
                    )
                )
    return graphics


def _derive_pad_display_layer(footprint_layer: str, pad_layers: list[str]) -> str:
    if footprint_layer.startswith("B."):
        return "B.Cu"
    if footprint_layer.startswith("F."):
        return "F.Cu"
    if "B.Cu" in pad_layers and "F.Cu" not in pad_layers:
        return "B.Cu"
    return "F.Cu"


def _parse_footprint_layout(footprint: list[SExpr]) -> FootprintLayout | None:
    footprint_name = _first_atom(footprint, 1) or ""
    layer = _first_atom(_first_child(footprint, "layer"), 1) or ""
    at = _parse_position(_first_child(footprint, "at"))
    properties = _parse_property_map(footprint)
    reference = properties.get("Reference", "")
    if at is None:
        return None

    origin, rotation_deg = at
    # KiCad board files already store bottom-side footprint geometry in the
    # placed board orientation, so applying an additional X mirror here
    # double-flips parts like the Mega headers.
    mirrored = False
    graphics = _parse_footprint_graphics(
        footprint,
        reference=reference,
        origin=origin,
        rotation_deg=rotation_deg or 0.0,
        mirrored=mirrored,
    )

    pads: list[PadGeometry] = []
    for pad_node in _iter_children(footprint, "pad"):
        number = _first_atom(pad_node, 1) or ""
        pad_type = _first_atom(pad_node, 2) or ""
        shape = _first_atom(pad_node, 3) or ""
        pad_at = _parse_position(_first_child(pad_node, "at"))
        size_mm = _parse_size(_first_child(pad_node, "size"))
        drill_node = _first_child(pad_node, "drill")
        layers_node = _first_child(pad_node, "layers")
        net_node = _first_child(pad_node, "net")

        if size_mm is None:
            continue

        local_point = Point(0.0, 0.0)
        pad_rotation_deg = 0.0
        if pad_at is not None:
            local_point, pad_rotation_deg = pad_at
            pad_rotation_deg = pad_rotation_deg or 0.0
        absolute_position = _transform_local_point(
            local_point,
            origin=origin,
            rotation_deg=rotation_deg or 0.0,
            mirrored=mirrored,
        )

        layers = []
        if layers_node is not None:
            layers = [value for value in layers_node[1:] if isinstance(value, str)]

        net_name = None
        if net_node is not None:
            if len(net_node) >= 3 and isinstance(net_node[2], str):
                net_name = net_node[2]
            elif len(net_node) >= 2 and isinstance(net_node[1], str):
                net_name = net_node[1]

        drill_mm = None
        if drill_node is not None:
            drill_values = [
                float(value)
                for value in drill_node[1:]
                if isinstance(value, str) and value.replace(".", "", 1).replace("-", "", 1).isdigit()
            ]
            if drill_values:
                drill_mm = tuple(drill_values)

        absolute_rotation = (rotation_deg or 0.0) + pad_rotation_deg
        pads.append(
            PadGeometry(
                component=reference,
                number=number,
                shape=shape,
                pad_type=pad_type,
                position=absolute_position,
                size_mm=size_mm,
                layers=layers,
                net_name=net_name,
                rotation_deg=absolute_rotation,
                drill_mm=drill_mm,
                display_layer=_derive_pad_display_layer(layer, layers),
            )
        )

    label = TextPrimitive(
        text=reference,
        position=origin,
        layer=layer.replace(".Cu", ".SilkS") if ".Cu" in layer else layer,
        owner=reference,
        size_mm=(1.0, 1.0),
        rotation_deg=rotation_deg,
    )

    return FootprintLayout(
        reference=reference,
        footprint=footprint_name,
        layer=layer,
        position=origin,
        rotation_deg=rotation_deg,
        pads=pads,
        graphics=graphics,
        label=label,
    )


def _circle_from_node(
    node: list[SExpr],
    *,
    layer: str,
    width_mm: float,
) -> CirclePrimitive | None:
    center = _parse_point(_first_child(node, "center"))
    end = _parse_point(_first_child(node, "end"))
    if center is None or end is None:
        return None
    radius_mm = math.hypot(end.x_mm - center.x_mm, end.y_mm - center.y_mm)
    return CirclePrimitive(
        center=center,
        radius_mm=radius_mm,
        layer=layer,
        width_mm=width_mm,
    )


def _bounds_from_layout(
    footprints: list[FootprintLayout],
    edge_cuts: list[LinePrimitive],
    drawings: list[LinePrimitive],
    circles: list[CirclePrimitive],
    tracks: list[LinePrimitive],
    vias: list[ViaGeometry],
    zones: list[ZonePolygon],
    texts: list[TextPrimitive],
) -> Bounds:
    xs: list[float] = []
    ys: list[float] = []

    def add_point(point: Point) -> None:
        xs.append(point.x_mm)
        ys.append(point.y_mm)

    for primitive in edge_cuts + drawings + tracks:
        add_point(primitive.start)
        add_point(primitive.end)

    for circle in circles:
        xs.extend((circle.center.x_mm - circle.radius_mm, circle.center.x_mm + circle.radius_mm))
        ys.extend((circle.center.y_mm - circle.radius_mm, circle.center.y_mm + circle.radius_mm))

    for footprint in footprints:
        add_point(footprint.position)
        for pad in footprint.pads:
            add_point(pad.position)
            xs.extend((pad.position.x_mm - (pad.size_mm[0] / 2.0), pad.position.x_mm + (pad.size_mm[0] / 2.0)))
            ys.extend((pad.position.y_mm - (pad.size_mm[1] / 2.0), pad.position.y_mm + (pad.size_mm[1] / 2.0)))
        for graphic in footprint.graphics:
            add_point(graphic.start)
            add_point(graphic.end)

    for via in vias:
        xs.extend((via.position.x_mm - (via.size_mm / 2.0), via.position.x_mm + (via.size_mm / 2.0)))
        ys.extend((via.position.y_mm - (via.size_mm / 2.0), via.position.y_mm + (via.size_mm / 2.0)))

    for zone in zones:
        for point in zone.points:
            add_point(point)

    for text in texts:
        add_point(text.position)

    if not xs or not ys:
        return Bounds(0.0, 0.0, 100.0, 100.0)

    return Bounds(
        min_x_mm=min(xs),
        min_y_mm=min(ys),
        max_x_mm=max(xs),
        max_y_mm=max(ys),
    ).expand(3.0)


def layout_from_kicad_pcb(pcb_path: str | Path) -> BoardLayout:
    path = Path(pcb_path).resolve()
    root = parse_sexpr(path.read_text(encoding="utf-8"))
    if not isinstance(root, list) or not root or root[0] != "kicad_pcb":
        raise ValueError(f"{path} is not a KiCad PCB file")

    footprints = [
        footprint
        for footprint in (
            _parse_footprint_layout(child)
            for child in root[1:]
            if isinstance(child, list) and child and child[0] == "footprint"
        )
        if footprint is not None
    ]
    footprints.sort(key=lambda footprint: footprint.reference)

    edge_cuts: list[LinePrimitive] = []
    drawings: list[LinePrimitive] = []
    circles: list[CirclePrimitive] = []
    texts: list[TextPrimitive] = []
    tracks: list[LinePrimitive] = []
    vias: list[ViaGeometry] = []
    zones: list[ZonePolygon] = []

    for child in root[1:]:
        if not isinstance(child, list) or not child:
            continue
        tag = child[0]

        if tag in {"gr_line", "gr_rect"}:
            layer = _first_atom(_first_child(child, "layer"), 1) or ""
            width_mm = _parse_stroke_width(child)
            if tag == "gr_line":
                primitive = _parse_line_primitive(child, layer=layer, width_mm=width_mm)
                if primitive is None:
                    continue
                if layer == "Edge.Cuts":
                    edge_cuts.append(primitive)
                else:
                    drawings.append(primitive)
            else:
                start = _parse_point(_first_child(child, "start"))
                end = _parse_point(_first_child(child, "end"))
                if start is None or end is None:
                    continue
                target = edge_cuts if layer == "Edge.Cuts" else drawings
                for rect_start, rect_end in _points_from_rect(start, end):
                    target.append(
                        LinePrimitive(
                            start=rect_start,
                            end=rect_end,
                            layer=layer,
                            width_mm=width_mm,
                            stroke_type=_parse_stroke_type(child),
                        )
                    )
        elif tag == "gr_circle":
            layer = _first_atom(_first_child(child, "layer"), 1) or ""
            primitive = _circle_from_node(child, layer=layer, width_mm=_parse_stroke_width(child))
            if primitive is not None:
                circles.append(primitive)
        elif tag == "gr_text":
            at = _parse_position(_first_child(child, "at"))
            layer = _first_atom(_first_child(child, "layer"), 1) or ""
            if at is None:
                continue
            text = _first_atom(child, 1)
            if text is None:
                continue
            position, rotation_deg = at
            texts.append(
                TextPrimitive(
                    text=text,
                    position=position,
                    layer=layer,
                    size_mm=_parse_text_size(child),
                    rotation_deg=rotation_deg,
                )
            )
        elif tag == "segment":
            layer = _first_atom(_first_child(child, "layer"), 1) or ""
            width_mm = _parse_float(_first_atom(_first_child(child, "width"), 1)) or 0.2
            net_name = _first_atom(_first_child(child, "net"), 1)
            primitive = _parse_line_primitive(
                child,
                layer=layer,
                width_mm=width_mm,
                owner_kind="track",
                net_name=net_name,
            )
            if primitive is not None:
                tracks.append(primitive)
        elif tag == "via":
            at = _parse_point(_first_child(child, "at"))
            size_mm = _parse_float(_first_atom(_first_child(child, "size"), 1))
            drill_mm = _parse_float(_first_atom(_first_child(child, "drill"), 1))
            layers_node = _first_child(child, "layers")
            net_name = _first_atom(_first_child(child, "net"), 1)
            if at is None or size_mm is None:
                continue
            layers = []
            if layers_node is not None:
                layers = [value for value in layers_node[1:] if isinstance(value, str)]
            vias.append(
                ViaGeometry(
                    position=at,
                    size_mm=size_mm,
                    drill_mm=drill_mm,
                    layers=layers,
                    net_name=net_name,
                )
            )
        elif tag == "zone":
            layer = _first_atom(_first_child(child, "layer"), 1) or ""
            points = _parse_polygon_points(_first_child(child, "polygon"))
            if points:
                zones.append(
                    ZonePolygon(
                        layer=layer,
                        points=points,
                        name=_first_atom(_first_child(child, "name"), 1),
                        keepout=_first_child(child, "keepout") is not None,
                    )
                )

    return BoardLayout(
        name=path.stem,
        source_path=str(path),
        bounds=_bounds_from_layout(footprints, edge_cuts, drawings, circles, tracks, vias, zones, texts),
        footprints=footprints,
        edge_cuts=edge_cuts,
        drawings=drawings,
        circles=circles,
        texts=texts,
        tracks=tracks,
        vias=vias,
        zones=zones,
    )
