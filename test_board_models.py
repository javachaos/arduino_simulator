"""Tests for built-in logical board models."""

from __future__ import annotations

import contextlib
import io
import json
import sys
from pathlib import Path
from tempfile import TemporaryDirectory

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from avrsim.board_models import (
    build_arduino_mega_2560_rev3_board,
    build_arduino_nano_v3_board,
    built_in_board_model_names,
    load_built_in_board_model,
)
from avrsim.cli import main as cli_main
from avrsim.lang import dump_board_dsl


def _component_by_ref(board, reference: str):
    for component in board.components:
        if component.reference == reference:
            return component
    raise AssertionError(f"missing component {reference}")


def _pad_net_map(component) -> dict[str, str | None]:
    return {pad.number: pad.net_name for pad in component.pads}


def _net_connection_map(board, net_name: str) -> set[tuple[str, str]]:
    for net in board.nets:
        if net.name == net_name:
            return {(connection.component, connection.pad) for connection in net.connections}
    raise AssertionError(f"missing net {net_name}")


def test_built_in_board_model_names() -> None:
    assert built_in_board_model_names() == [
        "arduino_mega_2560_rev3",
        "arduino_nano_v3",
    ]


def test_arduino_mega_board_model() -> None:
    board = build_arduino_mega_2560_rev3_board()
    assert board.name == "arduino_mega_2560_rev3"
    assert board.source_format == "builtin"
    assert len(board.components) == 5

    power = _component_by_ref(board, "J_POWER")
    assert _pad_net_map(power)["1"] == "IOREF"
    assert _pad_net_map(power)["4"] == "+5V"
    assert _pad_net_map(power)["8"] == "AREF"

    digital_high = _component_by_ref(board, "J_DIGITAL_HIGH")
    assert _pad_net_map(digital_high)["7"] == "D28"
    assert _pad_net_map(digital_high)["31"] == "D52_SCK"
    assert _pad_net_map(digital_high)["32"] == "D53_SS"

    assert _net_connection_map(board, "D52_SCK") == {
        ("J_DIGITAL_HIGH", "31"),
        ("U1", "D52_SCK"),
    }
    assert _net_connection_map(board, "+5V") == {
        ("J_POWER", "4"),
        ("U1", "+5V"),
    }


def test_arduino_nano_board_model() -> None:
    board = build_arduino_nano_v3_board()
    assert board.name == "arduino_nano_v3"
    assert board.source_format == "builtin"
    assert len(board.components) == 3

    left = _component_by_ref(board, "J_LEFT")
    right = _component_by_ref(board, "J_RIGHT")
    assert _pad_net_map(left)["1"] == "D13_SCK"
    assert _pad_net_map(left)["15"] == "VIN"
    assert _pad_net_map(right)["1"] == "D12_MISO"
    assert _pad_net_map(right)["14"] == "D0_RX"
    assert _pad_net_map(right)["15"] == "D1_TX"

    assert _net_connection_map(board, "RESET") == {
        ("J_LEFT", "13"),
        ("J_RIGHT", "13"),
        ("U1", "RESET"),
    }
    assert _net_connection_map(board, "A4_SDA") == {
        ("J_LEFT", "8"),
        ("U1", "A4_SDA"),
    }


def test_load_built_in_board_model() -> None:
    board = load_built_in_board_model("arduino_nano_v3")
    assert board.title == "Arduino Nano V3 (Logical Model)"


def test_builtin_board_model_cli() -> None:
    with TemporaryDirectory() as temp_dir:
        out_path = Path(temp_dir) / "arduino_nano_v3.json"
        exit_code = cli_main(
            [
                "from-builtin",
                "arduino_nano_v3",
                "--format",
                "json",
                "--out",
                str(out_path),
            ]
        )
        assert exit_code == 0
        payload = json.loads(out_path.read_text(encoding="utf-8"))
        assert payload["source_format"] == "builtin"
        assert payload["name"] == "arduino_nano_v3"

    stdout = io.StringIO()
    with contextlib.redirect_stdout(stdout):
        assert cli_main(["list-board-models"]) == 0
    assert stdout.getvalue().splitlines() == [
        "arduino_mega_2560_rev3",
        "arduino_nano_v3",
    ]


def test_builtin_board_dsl_generation() -> None:
    payload = dump_board_dsl(build_arduino_mega_2560_rev3_board())
    assert '(name "arduino_mega_2560_rev3")' in payload
    assert '(source_format "builtin")' in payload
    assert '(reference "J_DIGITAL_HIGH")' in payload


def main() -> int:
    test_built_in_board_model_names()
    test_arduino_mega_board_model()
    test_arduino_nano_board_model()
    test_load_built_in_board_model()
    test_builtin_board_model_cli()
    test_builtin_board_dsl_generation()
    print("avrsim built-in board model tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
