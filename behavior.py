"""Behavior profiles and board verification for avrsim."""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from .dsl import Board
from .kicad_sexpr import SExpr, parse_sexpr


@dataclass(frozen=True)
class ComponentProfile:
    reference: str
    behavior: str
    pads: dict[str, str]
    footprint: str | None = None
    value: str | None = None


@dataclass(frozen=True)
class BoardBehaviorProfile:
    board_name: str
    components: list[ComponentProfile]
    strict: bool = True
    source_path: str | None = None


@dataclass(frozen=True)
class ComponentVerification:
    reference: str
    behavior: str
    ok: bool
    messages: list[str] = field(default_factory=list)
    matched_pad_count: int = 0
    expected_pad_count: int = 0


@dataclass(frozen=True)
class BoardVerification:
    board_name: str
    profile_name: str
    strict: bool
    component_results: list[ComponentVerification]
    unexpected_components: list[str] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        if any(not result.ok for result in self.component_results):
            return False
        if self.strict and self.unexpected_components:
            return False
        return True


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


def _first_atom(node: list[SExpr] | None, index: int = 1) -> str | None:
    if node is None or len(node) <= index or not isinstance(node[index], str):
        return None
    return node[index]


def _parse_bool(token: str | None, default: bool) -> bool:
    if token is None:
        return default
    lowered = token.lower()
    if lowered in {"yes", "true", "1"}:
        return True
    if lowered in {"no", "false", "0"}:
        return False
    raise ValueError(f"invalid boolean token in behavior profile: {token}")


def _parse_component_profile(node: list[SExpr]) -> ComponentProfile:
    reference = _first_atom(_first_child(node, "reference"))
    behavior = _first_atom(_first_child(node, "behavior"))
    if not reference or not behavior:
        raise ValueError("behavior profile component requires reference and behavior")

    pads: dict[str, str] = {}
    for pad_node in _iter_children(node, "pad"):
        if len(pad_node) < 3 or not isinstance(pad_node[1], str) or not isinstance(pad_node[2], str):
            raise ValueError(f"invalid pad entry in behavior profile component {reference}")
        pads[pad_node[1]] = pad_node[2]

    return ComponentProfile(
        reference=reference,
        behavior=behavior,
        footprint=_first_atom(_first_child(node, "footprint")),
        value=_first_atom(_first_child(node, "value")),
        pads=pads,
    )


def load_behavior_profile(profile_path: str | Path) -> BoardBehaviorProfile:
    path = Path(profile_path).resolve()
    root = parse_sexpr(path.read_text(encoding="utf-8"))
    if not isinstance(root, list) or not root or root[0] != "profile":
        raise ValueError(f"{path} is not an avrsim behavior profile")

    board_name = _first_atom(_first_child(root, "board"))
    if not board_name:
        raise ValueError(f"{path} does not define a board name")

    components = [
        _parse_component_profile(component_node)
        for component_node in _iter_children(root, "component")
    ]

    return BoardBehaviorProfile(
        board_name=board_name,
        components=components,
        strict=_parse_bool(_first_atom(_first_child(root, "strict")), True),
        source_path=str(path),
    )


def built_in_profile_path(board_name: str) -> Path:
    return Path(__file__).resolve().parent / "profiles" / f"{board_name}.behavior.avrsim"


def load_built_in_profile(board_name: str) -> BoardBehaviorProfile:
    return load_behavior_profile(built_in_profile_path(board_name))


def verify_board(board: Board, profile: BoardBehaviorProfile) -> BoardVerification:
    board_components = {component.reference: component for component in board.components}
    results: list[ComponentVerification] = []

    for expected in profile.components:
        component = board_components.get(expected.reference)
        messages: list[str] = []
        matched_pad_count = 0

        if component is None:
            messages.append("missing component on board")
            results.append(
                ComponentVerification(
                    reference=expected.reference,
                    behavior=expected.behavior,
                    ok=False,
                    messages=messages,
                    matched_pad_count=matched_pad_count,
                    expected_pad_count=len(expected.pads),
                )
            )
            continue

        if expected.footprint and component.footprint != expected.footprint:
            messages.append(
                f"footprint mismatch: expected {expected.footprint}, found {component.footprint}"
            )

        if expected.value is not None and component.value != expected.value:
            messages.append(f"value mismatch: expected {expected.value}, found {component.value}")

        actual_pad_map = {pad.number: pad.net_name for pad in component.pads}
        for pad_number, expected_net in sorted(expected.pads.items()):
            actual_net = actual_pad_map.get(pad_number)
            if actual_net != expected_net:
                messages.append(
                    f"pad {pad_number} net mismatch: expected {expected_net}, found {actual_net}"
                )
            else:
                matched_pad_count += 1

        results.append(
            ComponentVerification(
                reference=expected.reference,
                behavior=expected.behavior,
                ok=not messages,
                messages=messages,
                matched_pad_count=matched_pad_count,
                expected_pad_count=len(expected.pads),
            )
        )

    expected_refs = {component.reference for component in profile.components}
    unexpected_components = sorted(
        component.reference for component in board.components if component.reference not in expected_refs
    )

    return BoardVerification(
        board_name=board.name,
        profile_name=profile.board_name,
        strict=profile.strict,
        component_results=results,
        unexpected_components=unexpected_components,
    )


def format_verification_report(report: BoardVerification) -> str:
    status = "PASS" if report.ok else "FAIL"
    lines = [f"{status} {report.board_name} against behavior profile {report.profile_name}"]

    for result in report.component_results:
        component_status = "OK" if result.ok else "ERR"
        lines.append(
            f"  {component_status} {result.reference} [{result.behavior}] "
            f"{result.matched_pad_count}/{result.expected_pad_count} pads matched"
        )
        for message in result.messages:
            lines.append(f"    - {message}")

    if report.unexpected_components:
        prefix = "ERR" if report.strict else "WARN"
        lines.append(
            f"  {prefix} unexpected components: {', '.join(report.unexpected_components)}"
        )

    return "\n".join(lines)
