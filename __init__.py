"""Board, DSL, and KiCad tooling for the avrsim workspace."""

from .behavior import (
    BoardBehaviorProfile,
    BoardVerification,
    ComponentProfile,
    ComponentVerification,
    format_verification_report,
    load_built_in_profile,
    load_behavior_profile,
    verify_board,
)
from .board_models import (
    build_arduino_mega_2560_rev3_board,
    build_arduino_nano_v3_board,
    built_in_board_model_names,
    load_built_in_board_model,
)
from .dsl import Board, Component, Net, Pad, Position
from .kicad_pcb import board_from_kicad_pcb
from .lang import dump_board_dsl

__all__ = [
    "Board",
    "BoardBehaviorProfile",
    "BoardVerification",
    "Component",
    "ComponentProfile",
    "ComponentVerification",
    "Net",
    "Pad",
    "Position",
    "board_from_kicad_pcb",
    "build_arduino_mega_2560_rev3_board",
    "build_arduino_nano_v3_board",
    "built_in_board_model_names",
    "dump_board_dsl",
    "format_verification_report",
    "load_built_in_board_model",
    "load_built_in_profile",
    "load_behavior_profile",
    "verify_board",
]
