# rust_runtime

`rust_runtime` is the standalone native runtime and CLI layer for the Rust side
of `arduino_simulator`.

It sits on top of:

- `rust_cpu/` for AVR fetch/decode/execute
- `rust_mcu/` for ATmega328P / ATmega2560 MMIO behavior

Run the crate from the workspace root with `cargo run -p rust_runtime -- ...`.

Examples:

```sh
cargo run -p rust_runtime -- run-nano /tmp/serial_probe_build/serial_probe_sketch.ino.hex
```

```sh
cargo run -p rust_runtime -- run-mega /tmp/dewpoint_mega_build/dewpoint_controller.ino.hex
```

Stop after the first serial byte:

```sh
cargo run -p rust_runtime -- run-mega /tmp/dewpoint_mega_build/dewpoint_controller.ino.hex --until-serial
```

Write captured serial to a file:

```sh
cargo run -p rust_runtime -- run-nano /tmp/serial_probe_build/serial_probe_sketch.ino.hex --out /tmp/nano_serial.txt
```

Launch the native split monitor for the Nano runtime:

```sh
cargo run -p rust_runtime -- monitor-nano /tmp/serial_probe_build/serial_probe_sketch.ino.hex
```

Launch the same split monitor for the Mega runtime:

```sh
cargo run -p rust_runtime -- monitor-mega /tmp/dewpoint_mega_build/dewpoint_controller.ino.hex
```

Monitor controls:

- `Space` pauses or resumes execution
- `S` steps one instruction while paused
- `C` clears the serial pane
- `Q` exits the monitor
