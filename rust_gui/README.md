# rust_gui

`rust_gui` is the native desktop front end for `avrsim-rs`.

Current scope:

- open and save `.avrsim.json` simulation projects
- open a board editor window for authoring reusable multi-board assembly files
- keep the selected host board, firmware source, and PCB path together
- load an `.avrsim.json` board file and render its primary PCB-backed member directly in the GUI
- edit host-board signal to PCB-net bindings and save them with the project
- associate a built-in or file-backed runtime behavior definition with each board/module member
- load a compiled Intel HEX image directly into the native Nano or Mega runtime
- compile an Arduino `.ino` sketch with `arduino-cli` and load the resulting `.hex`
- open a bidirectional serial console window with scrollback and host baud selection
- build a single-file `assembly_bundle` from one or more PCB-backed board/module members
- assign optional `Mega` / `Nano` host boards and `.ino` / `.hex` firmware per member
- wire member ports together and export stack-level ports
- run, pause, step, reset, and clear serial output without leaving the GUI
- inspect live CPU state, next instruction, register values, and peripheral summary lines

The GUI keeps emulation on a background worker thread so the window can fall
behind and skip stale frames instead of slowing the simulator down.

Run it from the workspace root:

```sh
cargo run -p rust_gui
```

The window supports these flows:

1. Open or save a `.avrsim.json` project that captures the host board, firmware source, and PCB path
2. Pick a `.ino` file, choose `Arduino Mega 2560` or `Arduino Nano (ATmega328P)`, then click `Compile && Load`
3. Pick a prebuilt `.hex` file and click `Load HEX`
4. Load an `.avrsim.json` board file and inspect the rendered primary board geometry
5. Bind host signals like `D27`, `D50_MISO`, or `A10` onto PCB nets and save those bindings in the project
6. Open `Serial Console`, choose a host baud, type commands, and send them into the simulated UART
7. Open `Board Editor`, add PCB-backed members or built-in modules, derive ports, assign behaviors, wire them together, and save the resulting board-stack document

Once firmware is loaded, use `Run`, `Pause`, `Step`, `Reset`, and `Clear Serial`
from the toolbar.
