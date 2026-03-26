# arduino_simulator

`arduino_simulator` is a standalone AVR simulation and board-tooling workspace.

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

The checked-in workspace is Rust-only. Board parsing, built-in models, layout
geometry, and document formats now live in the Rust crates.

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
- open and save simulation project JSON documents
- open a dedicated board editor for authoring multi-PCB board-stack files
- keep the selected host board, firmware source, and PCB path together
- load board-definition JSON documents and render their primary PCB-backed member directly
- edit and persist host signal to PCB net bindings
- load multiple PCB-backed board/module members into one assembly bundle
- associate optional `Mega` / `Nano` host boards plus `.ino` / `.hex` firmware per member
- associate built-in or file-backed runtime behaviors with reusable sensor/interface modules
- wire member ports together and save the result as an `assembly_bundle` board file
- open a bidirectional serial console window with host baud selection
- run, pause, step, reset, and clear serial output
- show CPU state, registers, next instruction, compile log, and scrollable serial output

The GUI and project crates use `rust_board` for KiCad import, built-in board
models, and layout rendering.

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
