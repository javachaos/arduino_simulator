# rust_kicad

`rust_kicad` is a thin KiCad-facing adapter layer for `arduino_simulator`.

It does not replace KiCad's built-in ngspice engine. Instead, it helps bridge a
KiCad PCB plus firmware source into an `arduino_simulator` project file that the
native GUI can open directly.

## Commands

Open any KiCad PCB directly in generic mode:

```sh
cargo run -p rust_kicad -- open-pcb \
  --pcb /path/to/board.kicad_pcb
```

Create a project from a KiCad PCB and firmware source:

```sh
cargo run -p rust_kicad -- create-project \
  --pcb /path/to/board.kicad_pcb \
  --firmware /path/to/sketch.ino \
  --launch-gui
```

Refresh an existing project after PCB net changes:

```sh
cargo run -p rust_kicad -- sync-project \
  --project /path/to/board.avrsim.json \
  --launch-gui
```

Open the GUI directly into an existing project:

```sh
cargo run -p rust_kicad -- open-gui \
  --project /path/to/board.avrsim.json
```

## Intended KiCad workflow

The adapter can still be called directly as an external tool, but the preferred
workflow is now the thin KiCad action plugin under
`/Users/fred/Documents/arduino_simulator/kicad_plugin`.

From KiCad PCB Editor, the plugin offers two paths:

1. Generic PCB mode:
   open any `.kicad_pcb` directly in the GUI without requiring firmware or a
   simulator project file.
2. Arduino simulation project mode:
   reuse `<board>.avrsim.json` when present, otherwise prompt for firmware,
   create the project, and launch the GUI into it.

This keeps the KiCad-specific behavior at the edge and avoids invasive changes
to the simulator core crates.
