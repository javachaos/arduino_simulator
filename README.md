# avrsim

`avrsim` is a standalone AVR simulation and board-tooling workspace.

## License

This workspace is licensed under the GNU General Public License, version 3
or later. See `LICENSE`.

## Current Layout

The workspace is now split cleanly by responsibility:

- `rust_cpu/`
  Native AVR instruction-set simulator
- `rust_mcu/`
  Native ATmega328P / ATmega2560 MMIO and board-facing runtime layers
- `rust_board/`
  Native board DSL, KiCad import, and layout geometry
- `rust_project/`
  Native document model for projects, boards, modules, and hierarchical assemblies
- `rust_behavior/`
  Native declarative behavior definitions plus compiled runtime behavior engines for reusable modules
- `rust_runtime/`
  Native CLI for running Mega/Nano firmware from Intel HEX
- `rust_gui/`
  Native desktop GUI for compile/load/run plus CPU and serial inspection

The remaining Python package is now focused on board-oriented tooling:

- `dsl.py`
- `board_models.py`
- `lang.py`
- `kicad_pcb.py`
- `kicad_layout.py`
- `behavior.py`

The legacy Python execution/runtime stack has been archived at:

- `archive/python_runtime_legacy_20260324.zip`

## Python Board Tooling

Generate the native DSL from a KiCad board:

```sh
python3 -m avrsim from-pcb \
  examples/pcbs/air_node.kicad_pcb
```

Generate JSON instead:

```sh
python3 -m avrsim from-pcb \
  examples/pcbs/air_node.kicad_pcb \
  --format json
```

Generate a built-in logical board model:

```sh
python3 -m avrsim from-builtin arduino_nano_v3
```

List built-in models:

```sh
python3 -m avrsim list-board-models
```

Verify a live board against its behavior profile:

```sh
python3 -m avrsim verify-board \
  examples/pcbs/air_node.kicad_pcb
```

Dump parsed KiCad layout geometry:

```sh
python3 -m avrsim layout-json \
  examples/pcbs/air_node.kicad_pcb
```

## Native Runtime

Run Nano firmware:

```sh
cargo run -p rust_runtime -- run-nano /path/to/firmware.hex
```

Run Mega firmware:

```sh
cargo run -p rust_runtime -- run-mega /path/to/firmware.hex
```

Launch the native terminal monitor:

```sh
cargo run -p rust_runtime -- monitor-mega /path/to/firmware.hex
```

## Native GUI

Launch the desktop GUI:

```sh
cargo run -p rust_gui
```

The GUI can:

- compile an Arduino `.ino` sketch with `arduino-cli`
- load a `.hex` image directly
- open and save `.avrsim.json` simulation projects
- open a dedicated board editor for authoring multi-PCB board-stack files
- keep the selected host board, firmware source, and PCB path together
- load `.avrsim.json` board files and render their primary PCB-backed member directly
- edit and persist host signal to PCB net bindings
- load multiple PCB-backed board/module members into one assembly bundle
- associate optional `Mega` / `Nano` host boards plus `.ino` / `.hex` firmware per member
- associate built-in or file-backed runtime behaviors with reusable sensor/interface modules
- wire member ports together and save the result as an `assembly_bundle` board file
- open a bidirectional serial console window with host baud selection
- run, pause, step, reset, and clear serial output
- show CPU state, registers, next instruction, compile log, and scrollable serial output

## Rust Workspace

Run the native test suites from the workspace root:

```sh
cargo test -p rust_cpu
cargo test -p rust_mcu
cargo test -p rust_board
cargo test -p rust_project
cargo test -p rust_runtime
cargo test -p rust_gui
```

Or run everything:

```sh
cargo test
```
