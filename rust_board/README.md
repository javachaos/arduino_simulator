# rust_board

`rust_board` is the native board-description layer for `arduino_simulator`.

Current scope:

- board DSL domain types such as `Board`, `Component`, `Pad`, and `Net`
- native KiCad S-expression parsing
- native KiCad PCB import for components and nets
- native KiCad layout import for edge cuts, tracks, vias, pads, and footprint graphics
- deterministic net derivation from component pads
- textual board DSL emission
- built-in logical board models loaded from compact embedded JSON assets under
  `builtins/` for:
  - `arduino_mega_2560_rev3`
  - `arduino_nano_v3`
  - `gy_sht31_d`
  - `mcp2515_tja1050_can_module`
  - `max31865_breakout`
  - `lc_lm358_pwm_to_0_10v`
  - `aht20_breakout`
  - `ads1115_breakout`
  - `bh1750_breakout`
  - `bme280_breakout`
  - `bmp280_breakout`
  - `ina219_breakout`
  - `max31855_breakout`
  - `max6675_breakout`
  - `mpu6050_breakout`
  - `vl53l0x_breakout`

Run its tests from the workspace root:

```sh
cargo test -p rust_board
```

The goal is to keep the board/DSL layer deterministic, portable, and easy to
regression-test across the Rust workspace.
