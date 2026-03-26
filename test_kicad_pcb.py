"""Smoke tests for KiCad PCB import into the avrsim DSL."""

from __future__ import annotations

import json
import sys
from pathlib import Path
from tempfile import TemporaryDirectory

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from avrsim.cli import main as cli_main
from avrsim.kicad_pcb import board_from_kicad_pcb
from avrsim.lang import dump_board_dsl


EXAMPLES_ROOT = Path(__file__).resolve().parent / "examples" / "pcbs"
AIR_NODE_PCB = EXAMPLES_ROOT / "air_node.kicad_pcb"
MEGA_SIDECAR_PCB = EXAMPLES_ROOT / "mega_r3_sidecar_controller_rev_a.kicad_pcb"


def _component_by_ref(board, reference: str):
    for component in board.components:
        if component.reference == reference:
            return component
    raise AssertionError(f"missing component {reference}")


def _pad_net_map(component) -> dict[str, str | None]:
    return {pad.number: pad.net_name for pad in component.pads}


def test_air_node_import() -> None:
    board = board_from_kicad_pcb(AIR_NODE_PCB)
    assert board.name == "air_node"
    assert board.generator == "pcbnew"
    assert len(board.components) >= 5

    rj45 = _component_by_ref(board, "RJ45")
    assert rj45.kind == "connector"
    assert _pad_net_map(rj45)["1"] == "CAN_H"
    assert _pad_net_map(rj45)["2"] == "CAN_L"
    assert _pad_net_map(rj45)["7"] == "+24V"
    assert _pad_net_map(rj45)["8"] == "GND"

    sht31 = _component_by_ref(board, "SHT31")
    assert _pad_net_map(sht31)["1"] == "/A4{slash}SDA"
    assert _pad_net_map(sht31)["2"] == "/A5{slash}SCL"
    assert _pad_net_map(sht31)["3"] == "GND"
    assert _pad_net_map(sht31)["4"] == "VCC"


def test_mega_sidecar_import() -> None:
    board = board_from_kicad_pcb(MEGA_SIDECAR_PCB)
    assert board.name == "mega_r3_sidecar_controller_rev_a"
    assert board.title == "Mega R3 Sidecar Controller Rev A"
    assert len(board.components) >= 10

    k1 = _component_by_ref(board, "K1")
    assert _pad_net_map(k1)["1"] == "CANH"
    assert _pad_net_map(k1)["2"] == "CANL"
    assert _pad_net_map(k1)["7"] == "K1_24V"
    assert _pad_net_map(k1)["8"] == "GND"

    j101 = _component_by_ref(board, "J101")
    assert _pad_net_map(j101)["1"] == "/28"
    assert _pad_net_map(j101)["2"] == "/*52"
    assert _pad_net_map(j101)["5"] == "/27"


def test_cli_generation() -> None:
    with TemporaryDirectory() as temp_dir:
        out_path = Path(temp_dir) / "air_node.avrsim.json"
        exit_code = cli_main(
            ["from-pcb", str(AIR_NODE_PCB), "--format", "json", "--out", str(out_path)]
        )
        assert exit_code == 0
        payload = json.loads(out_path.read_text(encoding="utf-8"))
        assert payload["kind"] == "avrsim.board"
        assert payload["name"] == "air_node"
        assert payload["stats"]["component_count"] >= 5


def test_native_dsl_generation() -> None:
    board = board_from_kicad_pcb(AIR_NODE_PCB)
    payload = dump_board_dsl(board)
    assert '(name "air_node")' in payload
    assert '(reference "RJ45")' in payload
    assert '(net "CAN_H")' in payload


def main() -> int:
    test_air_node_import()
    test_mega_sidecar_import()
    test_cli_generation()
    test_native_dsl_generation()
    print("avrsim KiCad PCB importer tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
