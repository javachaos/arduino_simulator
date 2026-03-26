"""Command-line entry points for avrsim board and KiCad tooling."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from .behavior import (
    format_verification_report,
    load_behavior_profile,
    load_built_in_profile,
    verify_board,
)
from .board_models import built_in_board_model_names, load_built_in_board_model
from .kicad_layout import layout_from_kicad_pcb
from .kicad_pcb import board_from_kicad_pcb
from .lang import dump_board_dsl


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="avrsim")
    subparsers = parser.add_subparsers(dest="command", required=True)

    from_pcb = subparsers.add_parser(
        "from-pcb",
        help="Generate avrsim DSL or JSON from a KiCad .kicad_pcb file.",
    )
    from_pcb.add_argument("pcb_path", help="Path to the source .kicad_pcb file.")
    from_pcb.add_argument(
        "--out",
        dest="out_path",
        help="Optional output path. If omitted, the selected format is written to stdout.",
    )
    from_pcb.add_argument(
        "--format",
        choices=("dsl", "json"),
        default="dsl",
        help="Output format. The native avrsim DSL is the default.",
    )

    from_builtin = subparsers.add_parser(
        "from-builtin",
        help="Generate avrsim DSL or JSON from a built-in board model.",
    )
    from_builtin.add_argument(
        "board_name",
        choices=built_in_board_model_names(),
        help="Built-in board model name.",
    )
    from_builtin.add_argument(
        "--out",
        dest="out_path",
        help="Optional output path. If omitted, the selected format is written to stdout.",
    )
    from_builtin.add_argument(
        "--format",
        choices=("dsl", "json"),
        default="dsl",
        help="Output format. The native avrsim DSL is the default.",
    )

    subparsers.add_parser(
        "list-board-models",
        help="List the available built-in board models.",
    )

    verify_board_parser = subparsers.add_parser(
        "verify-board",
        help="Verify a KiCad board against an avrsim behavior profile.",
    )
    verify_board_parser.add_argument("pcb_path", help="Path to the source .kicad_pcb file.")
    verify_board_parser.add_argument(
        "--profile",
        dest="profile_path",
        help="Optional explicit behavior profile path. Defaults to the built-in profile for the board name.",
    )

    layout_parser = subparsers.add_parser(
        "layout-json",
        help="Dump parsed KiCad layout geometry as JSON.",
    )
    layout_parser.add_argument("pcb_path", help="Path to the source .kicad_pcb file.")
    layout_parser.add_argument(
        "--out",
        dest="out_path",
        help="Optional output path. If omitted, JSON is written to stdout.",
    )
    return parser


def _write_payload(payload: str, out_path: str | None) -> int:
    if out_path:
        Path(out_path).write_text(payload, encoding="utf-8")
    else:
        print(payload, end="")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)

    if args.command == "from-pcb":
        board = board_from_kicad_pcb(args.pcb_path)
        payload = (
            json.dumps(board.to_dict(), indent=2, sort_keys=True) + "\n"
            if args.format == "json"
            else dump_board_dsl(board)
        )
        return _write_payload(payload, args.out_path)

    if args.command == "from-builtin":
        board = load_built_in_board_model(args.board_name)
        payload = (
            json.dumps(board.to_dict(), indent=2, sort_keys=True) + "\n"
            if args.format == "json"
            else dump_board_dsl(board)
        )
        return _write_payload(payload, args.out_path)

    if args.command == "list-board-models":
        for board_name in built_in_board_model_names():
            print(board_name)
        return 0

    if args.command == "verify-board":
        board = board_from_kicad_pcb(args.pcb_path)
        profile = (
            load_behavior_profile(args.profile_path)
            if args.profile_path
            else load_built_in_profile(board.name)
        )
        report = verify_board(board, profile)
        print(format_verification_report(report))
        return 0 if report.ok else 1

    if args.command == "layout-json":
        payload = json.dumps(
            layout_from_kicad_pcb(args.pcb_path).to_dict(),
            indent=2,
            sort_keys=True,
        ) + "\n"
        return _write_payload(payload, args.out_path)

    parser.error(f"unsupported command: {args.command}")
    return 2
