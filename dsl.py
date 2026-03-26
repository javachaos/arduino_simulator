"""Domain objects for the avrsim board DSL."""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass, field


DSL_VERSION = "0.1.0"


@dataclass(frozen=True)
class Position:
    x_mm: float
    y_mm: float
    rotation_deg: float | None = None

    def to_dict(self) -> dict[str, float | None]:
        return {
            "x_mm": self.x_mm,
            "y_mm": self.y_mm,
            "rotation_deg": self.rotation_deg,
        }


@dataclass(frozen=True)
class Pad:
    number: str
    pad_type: str
    shape: str
    layers: list[str]
    net_name: str | None = None
    net_code: int | None = None
    position: Position | None = None
    size_mm: tuple[float, float] | None = None
    drill_mm: tuple[float, ...] | None = None
    uuid: str | None = None

    def to_dict(self) -> dict[str, object]:
        return {
            "number": self.number,
            "pad_type": self.pad_type,
            "shape": self.shape,
            "layers": list(self.layers),
            "net_name": self.net_name,
            "net_code": self.net_code,
            "position": self.position.to_dict() if self.position else None,
            "size_mm": list(self.size_mm) if self.size_mm else None,
            "drill_mm": list(self.drill_mm) if self.drill_mm else None,
            "uuid": self.uuid,
        }


@dataclass(frozen=True)
class Component:
    reference: str
    kind: str
    footprint: str
    layer: str
    pads: list[Pad]
    value: str | None = None
    position: Position | None = None
    uuid: str | None = None
    properties: dict[str, str] = field(default_factory=dict)

    def to_dict(self) -> dict[str, object]:
        return {
            "reference": self.reference,
            "value": self.value,
            "kind": self.kind,
            "footprint": self.footprint,
            "layer": self.layer,
            "position": self.position.to_dict() if self.position else None,
            "uuid": self.uuid,
            "properties": dict(sorted(self.properties.items())),
            "pads": [pad.to_dict() for pad in self.pads],
        }


@dataclass(frozen=True)
class NetConnection:
    component: str
    pad: str
    component_kind: str | None = None

    def to_dict(self) -> dict[str, object]:
        return {
            "component": self.component,
            "component_kind": self.component_kind,
            "pad": self.pad,
        }


@dataclass(frozen=True)
class Net:
    name: str
    connections: list[NetConnection]
    code: int | None = None

    def to_dict(self) -> dict[str, object]:
        return {
            "name": self.name,
            "code": self.code,
            "connections": [connection.to_dict() for connection in self.connections],
        }


@dataclass(frozen=True)
class Board:
    name: str
    source_path: str
    components: list[Component]
    nets: list[Net]
    source_format: str = "kicad_pcb"
    title: str | None = None
    generator: str | None = None
    generator_version: str | None = None
    board_version: str | None = None
    paper: str | None = None
    layers: list[str] = field(default_factory=list)

    def to_dict(self) -> dict[str, object]:
        pad_count = sum(len(component.pads) for component in self.components)
        return {
            "dsl_version": DSL_VERSION,
            "kind": "avrsim.board",
            "name": self.name,
            "title": self.title,
            "source_format": self.source_format,
            "source_path": self.source_path,
            "generator": self.generator,
            "generator_version": self.generator_version,
            "board_version": self.board_version,
            "paper": self.paper,
            "layers": list(self.layers),
            "components": [component.to_dict() for component in self.components],
            "nets": [net.to_dict() for net in self.nets],
            "stats": {
                "component_count": len(self.components),
                "net_count": len(self.nets),
                "pad_count": pad_count,
            },
        }


def derive_nets(components: list[Component]) -> list[Net]:
    connections_by_net: dict[str, list[NetConnection]] = defaultdict(list)
    codes_by_net: dict[str, int] = {}

    for component in components:
        for pad in component.pads:
            if not pad.net_name:
                continue
            connections_by_net[pad.net_name].append(
                NetConnection(
                    component=component.reference,
                    component_kind=component.kind,
                    pad=pad.number,
                )
            )
            if pad.net_code is not None:
                codes_by_net[pad.net_name] = pad.net_code

    nets: list[Net] = []
    for net_name in sorted(connections_by_net):
        connections = sorted(
            connections_by_net[net_name],
            key=lambda connection: (connection.component, connection.pad),
        )
        nets.append(
            Net(
                name=net_name,
                code=codes_by_net.get(net_name),
                connections=connections,
            )
        )
    return nets
