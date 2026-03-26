"""Built-in logical board models for common Arduino AVR boards."""

from __future__ import annotations

from typing import Callable

from .dsl import Board, Component, DSL_VERSION, Pad, Position, derive_nets


_HEADER_LAYERS = ["F.Cu", "B.Cu", "F.Mask", "B.Mask"]
_VIRTUAL_LAYERS = ["virtual"]


def _unique_signals(signals: list[str]) -> list[str]:
    seen: set[str] = set()
    ordered: list[str] = []
    for signal in signals:
        if signal in seen:
            continue
        seen.add(signal)
        ordered.append(signal)
    return ordered


def _header_component(
    reference: str,
    value: str,
    footprint: str,
    signals: list[str],
    *,
    position: Position,
) -> Component:
    pads: list[Pad] = []
    for index, signal in enumerate(signals, start=1):
        pads.append(
            Pad(
                number=str(index),
                pad_type="through_hole",
                shape="oval",
                layers=list(_HEADER_LAYERS),
                net_name=signal,
                position=Position(0.0, (index - 1) * 2.54),
                size_mm=(1.7, 1.7),
                drill_mm=(1.0,),
            )
        )

    return Component(
        reference=reference,
        value=value,
        kind="connector",
        footprint=footprint,
        layer="F.Cu",
        position=position,
        pads=pads,
        properties={"model_role": "logical_header"},
    )


def _mcu_component(
    reference: str,
    value: str,
    footprint: str,
    signals: list[str],
    *,
    position: Position,
) -> Component:
    pads: list[Pad] = []
    for index, signal in enumerate(signals, start=1):
        column = 0 if index <= ((len(signals) + 1) // 2) else 1
        row = (index - 1) if column == 0 else (index - 1 - ((len(signals) + 1) // 2))
        pads.append(
            Pad(
                number=signal,
                pad_type="virtual",
                shape="round",
                layers=list(_VIRTUAL_LAYERS),
                net_name=signal,
                position=Position(column * 10.0, row * 1.27),
                size_mm=(0.8, 0.8),
            )
        )

    return Component(
        reference=reference,
        value=value,
        kind="mcu",
        footprint=footprint,
        layer="virtual",
        position=position,
        pads=pads,
        properties={"model_role": "mcu_abstraction"},
    )


def _build_board(
    *,
    name: str,
    title: str,
    components: list[Component],
) -> Board:
    return Board(
        name=name,
        title=title,
        source_format="builtin",
        source_path=f"builtin://{name}",
        generator="avrsim",
        generator_version=DSL_VERSION,
        layers=["virtual", "F.Cu", "B.Cu"],
        components=components,
        nets=derive_nets(components),
    )


def build_arduino_mega_2560_rev3_board() -> Board:
    power_signals = ["IOREF", "RESET", "+3V3", "+5V", "GND", "GND", "VIN", "AREF"]
    analog_signals = [f"A{index}" for index in range(16)]
    digital_low_signals = [
        "D0_RX0",
        "D1_TX0",
        "D2",
        "D3_PWM",
        "D4",
        "D5_PWM",
        "D6_PWM",
        "D7",
        "D8",
        "D9_PWM",
        "D10_PWM",
        "D11_PWM",
        "D12",
        "D13",
        "D14_TX3",
        "D15_RX3",
        "D16_TX2",
        "D17_RX2",
        "D18_TX1",
        "D19_RX1",
        "D20_SDA",
        "D21_SCL",
    ]
    digital_high_signals = [
        "D22",
        "D23",
        "D24",
        "D25",
        "D26",
        "D27",
        "D28",
        "D29",
        "D30",
        "D31",
        "D32",
        "D33",
        "D34",
        "D35",
        "D36",
        "D37",
        "D38",
        "D39",
        "D40",
        "D41",
        "D42",
        "D43",
        "D44_PWM",
        "D45_PWM",
        "D46_PWM",
        "D47",
        "D48",
        "D49",
        "D50_MISO",
        "D51_MOSI",
        "D52_SCK",
        "D53_SS",
    ]

    exposed_signals = _unique_signals(
        power_signals + analog_signals + digital_low_signals + digital_high_signals
    )
    components = [
        _mcu_component(
            "U1",
            "ATmega2560",
            "Virtual:ATmega2560_BoardAbstraction",
            exposed_signals,
            position=Position(55.0, 25.0),
        ),
        _header_component(
            "J_POWER",
            "POWER",
            "Virtual:Header_1x08_2.54mm",
            power_signals,
            position=Position(5.0, 5.0),
        ),
        _header_component(
            "J_ANALOG",
            "ANALOG A0-A15",
            "Virtual:Header_1x16_2.54mm",
            analog_signals,
            position=Position(20.0, 5.0),
        ),
        _header_component(
            "J_DIGITAL_LOW",
            "DIGITAL D0-D21",
            "Virtual:Header_1x22_2.54mm",
            digital_low_signals,
            position=Position(95.0, 5.0),
        ),
        _header_component(
            "J_DIGITAL_HIGH",
            "DIGITAL D22-D53",
            "Virtual:Header_1x32_2.54mm",
            digital_high_signals,
            position=Position(120.0, 5.0),
        ),
    ]

    return _build_board(
        name="arduino_mega_2560_rev3",
        title="Arduino Mega 2560 Rev3 (Logical Model)",
        components=components,
    )


def build_arduino_nano_v3_board() -> Board:
    left_header_signals = [
        "D13_SCK",
        "+3V3",
        "AREF",
        "A0",
        "A1",
        "A2",
        "A3",
        "A4_SDA",
        "A5_SCL",
        "A6",
        "A7",
        "+5V",
        "RESET",
        "GND",
        "VIN",
    ]
    right_header_signals = [
        "D12_MISO",
        "D11_MOSI",
        "D10_SS",
        "D9_PWM",
        "D8",
        "D7",
        "D6_PWM",
        "D5_PWM",
        "D4",
        "D3_PWM",
        "D2",
        "GND",
        "RESET",
        "D0_RX",
        "D1_TX",
    ]

    exposed_signals = _unique_signals(left_header_signals + right_header_signals)
    components = [
        _mcu_component(
            "U1",
            "ATmega328P",
            "Virtual:ATmega328P_BoardAbstraction",
            exposed_signals,
            position=Position(25.0, 20.0),
        ),
        _header_component(
            "J_LEFT",
            "LEFT HEADER",
            "Virtual:Header_1x15_2.54mm",
            left_header_signals,
            position=Position(5.0, 5.0),
        ),
        _header_component(
            "J_RIGHT",
            "RIGHT HEADER",
            "Virtual:Header_1x15_2.54mm",
            right_header_signals,
            position=Position(45.0, 5.0),
        ),
    ]

    return _build_board(
        name="arduino_nano_v3",
        title="Arduino Nano V3 (Logical Model)",
        components=components,
    )


_BOARD_BUILDERS: dict[str, Callable[[], Board]] = {
    "arduino_mega_2560_rev3": build_arduino_mega_2560_rev3_board,
    "arduino_nano_v3": build_arduino_nano_v3_board,
}


def built_in_board_model_names() -> list[str]:
    return sorted(_BOARD_BUILDERS)


def load_built_in_board_model(board_name: str) -> Board:
    try:
        builder = _BOARD_BUILDERS[board_name]
    except KeyError as error:
        available = ", ".join(built_in_board_model_names())
        raise FileNotFoundError(
            f"unknown built-in board model {board_name!r}; available models: {available}"
        ) from error
    return builder()
