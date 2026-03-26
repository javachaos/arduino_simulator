"""Geometry domain objects for avrsim board rendering."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass(frozen=True)
class Point:
    x_mm: float
    y_mm: float

    def to_dict(self) -> dict[str, float]:
        return {
            "x_mm": self.x_mm,
            "y_mm": self.y_mm,
        }


@dataclass(frozen=True)
class Bounds:
    min_x_mm: float
    min_y_mm: float
    max_x_mm: float
    max_y_mm: float

    @property
    def width_mm(self) -> float:
        return self.max_x_mm - self.min_x_mm

    @property
    def height_mm(self) -> float:
        return self.max_y_mm - self.min_y_mm

    def expand(self, margin_mm: float) -> "Bounds":
        return Bounds(
            min_x_mm=self.min_x_mm - margin_mm,
            min_y_mm=self.min_y_mm - margin_mm,
            max_x_mm=self.max_x_mm + margin_mm,
            max_y_mm=self.max_y_mm + margin_mm,
        )

    def to_dict(self) -> dict[str, float]:
        return {
            "min_x_mm": self.min_x_mm,
            "min_y_mm": self.min_y_mm,
            "max_x_mm": self.max_x_mm,
            "max_y_mm": self.max_y_mm,
            "width_mm": self.width_mm,
            "height_mm": self.height_mm,
        }


@dataclass(frozen=True)
class LinePrimitive:
    start: Point
    end: Point
    layer: str
    width_mm: float
    owner: str | None = None
    owner_kind: str | None = None
    net_name: str | None = None
    stroke_type: str | None = None

    def to_dict(self) -> dict[str, object]:
        return {
            "start": self.start.to_dict(),
            "end": self.end.to_dict(),
            "layer": self.layer,
            "width_mm": self.width_mm,
            "owner": self.owner,
            "owner_kind": self.owner_kind,
            "net_name": self.net_name,
            "stroke_type": self.stroke_type,
        }


@dataclass(frozen=True)
class CirclePrimitive:
    center: Point
    radius_mm: float
    layer: str
    width_mm: float
    owner: str | None = None
    owner_kind: str | None = None
    fill: bool = False

    def to_dict(self) -> dict[str, object]:
        return {
            "center": self.center.to_dict(),
            "radius_mm": self.radius_mm,
            "layer": self.layer,
            "width_mm": self.width_mm,
            "owner": self.owner,
            "owner_kind": self.owner_kind,
            "fill": self.fill,
        }


@dataclass(frozen=True)
class TextPrimitive:
    text: str
    position: Point
    layer: str
    owner: str | None = None
    size_mm: tuple[float, float] | None = None
    rotation_deg: float | None = None

    def to_dict(self) -> dict[str, object]:
        return {
            "text": self.text,
            "position": self.position.to_dict(),
            "layer": self.layer,
            "owner": self.owner,
            "size_mm": list(self.size_mm) if self.size_mm else None,
            "rotation_deg": self.rotation_deg,
        }


@dataclass(frozen=True)
class PadGeometry:
    component: str
    number: str
    shape: str
    pad_type: str
    position: Point
    size_mm: tuple[float, float]
    layers: list[str]
    net_name: str | None = None
    rotation_deg: float | None = None
    drill_mm: tuple[float, ...] | None = None
    display_layer: str | None = None

    def to_dict(self) -> dict[str, object]:
        return {
            "component": self.component,
            "number": self.number,
            "shape": self.shape,
            "pad_type": self.pad_type,
            "position": self.position.to_dict(),
            "size_mm": list(self.size_mm),
            "layers": list(self.layers),
            "net_name": self.net_name,
            "rotation_deg": self.rotation_deg,
            "drill_mm": list(self.drill_mm) if self.drill_mm else None,
            "display_layer": self.display_layer,
        }


@dataclass(frozen=True)
class ViaGeometry:
    position: Point
    size_mm: float
    layers: list[str]
    net_name: str | None = None
    drill_mm: float | None = None

    def to_dict(self) -> dict[str, object]:
        return {
            "position": self.position.to_dict(),
            "size_mm": self.size_mm,
            "layers": list(self.layers),
            "net_name": self.net_name,
            "drill_mm": self.drill_mm,
        }


@dataclass(frozen=True)
class ZonePolygon:
    layer: str
    points: list[Point]
    name: str | None = None
    keepout: bool = False

    def to_dict(self) -> dict[str, object]:
        return {
            "layer": self.layer,
            "name": self.name,
            "keepout": self.keepout,
            "points": [point.to_dict() for point in self.points],
        }


@dataclass(frozen=True)
class FootprintLayout:
    reference: str
    footprint: str
    layer: str
    position: Point
    pads: list[PadGeometry]
    graphics: list[LinePrimitive] = field(default_factory=list)
    label: TextPrimitive | None = None
    rotation_deg: float | None = None

    def to_dict(self) -> dict[str, object]:
        return {
            "reference": self.reference,
            "footprint": self.footprint,
            "layer": self.layer,
            "position": self.position.to_dict(),
            "rotation_deg": self.rotation_deg,
            "pads": [pad.to_dict() for pad in self.pads],
            "graphics": [graphic.to_dict() for graphic in self.graphics],
            "label": self.label.to_dict() if self.label else None,
        }


@dataclass(frozen=True)
class BoardLayout:
    name: str
    source_path: str
    bounds: Bounds
    footprints: list[FootprintLayout]
    edge_cuts: list[LinePrimitive]
    drawings: list[LinePrimitive] = field(default_factory=list)
    circles: list[CirclePrimitive] = field(default_factory=list)
    texts: list[TextPrimitive] = field(default_factory=list)
    tracks: list[LinePrimitive] = field(default_factory=list)
    vias: list[ViaGeometry] = field(default_factory=list)
    zones: list[ZonePolygon] = field(default_factory=list)

    def to_dict(self) -> dict[str, object]:
        return {
            "name": self.name,
            "source_path": self.source_path,
            "bounds": self.bounds.to_dict(),
            "footprints": [footprint.to_dict() for footprint in self.footprints],
            "edge_cuts": [primitive.to_dict() for primitive in self.edge_cuts],
            "drawings": [primitive.to_dict() for primitive in self.drawings],
            "circles": [primitive.to_dict() for primitive in self.circles],
            "texts": [primitive.to_dict() for primitive in self.texts],
            "tracks": [track.to_dict() for track in self.tracks],
            "vias": [via.to_dict() for via in self.vias],
            "zones": [zone.to_dict() for zone in self.zones],
            "stats": {
                "footprint_count": len(self.footprints),
                "pad_count": sum(len(footprint.pads) for footprint in self.footprints),
                "graphic_count": sum(len(footprint.graphics) for footprint in self.footprints)
                + len(self.drawings)
                + len(self.edge_cuts),
                "track_count": len(self.tracks),
                "via_count": len(self.vias),
                "text_count": len(self.texts),
                "zone_count": len(self.zones),
            },
        }
