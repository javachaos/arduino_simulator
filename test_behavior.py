"""Behavior profile tests for avrsim."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from avrsim.behavior import (
    format_verification_report,
    load_built_in_profile,
    verify_board,
)
from avrsim.cli import main as cli_main
from avrsim.kicad_pcb import board_from_kicad_pcb


EXAMPLES_ROOT = Path(__file__).resolve().parent / "examples" / "pcbs"
AIR_NODE_PCB = EXAMPLES_ROOT / "air_node.kicad_pcb"
MEGA_SIDECAR_PCB = EXAMPLES_ROOT / "mega_r3_sidecar_controller_rev_a.kicad_pcb"


def test_load_profiles() -> None:
    air_profile = load_built_in_profile("air_node")
    mega_profile = load_built_in_profile("mega_r3_sidecar_controller_rev_a")
    assert air_profile.board_name == "air_node"
    assert mega_profile.board_name == "mega_r3_sidecar_controller_rev_a"
    assert len(air_profile.components) >= 7
    assert len(mega_profile.components) >= 20


def test_air_node_behavior_verification() -> None:
    board = board_from_kicad_pcb(AIR_NODE_PCB)
    profile = load_built_in_profile(board.name)
    report = verify_board(board, profile)
    assert report.ok
    assert "PASS air_node" in format_verification_report(report)


def test_mega_sidecar_behavior_verification() -> None:
    board = board_from_kicad_pcb(MEGA_SIDECAR_PCB)
    profile = load_built_in_profile(board.name)
    report = verify_board(board, profile)
    assert report.ok
    assert "PASS mega_r3_sidecar_controller_rev_a" in format_verification_report(report)


def test_verify_board_cli() -> None:
    assert cli_main(["verify-board", str(AIR_NODE_PCB)]) == 0
    assert cli_main(["verify-board", str(MEGA_SIDECAR_PCB)]) == 0


def main() -> int:
    test_load_profiles()
    test_air_node_behavior_verification()
    test_mega_sidecar_behavior_verification()
    test_verify_board_cli()
    print("avrsim behavior profile tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
