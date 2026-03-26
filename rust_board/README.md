# rust_board

`rust_board` is the native board-description layer for `avrsim`.

Current scope:

- board DSL domain types such as `Board`, `Component`, `Pad`, and `Net`
- native KiCad S-expression parsing
- native KiCad PCB import for components and nets
- native KiCad layout import for edge cuts, tracks, vias, pads, and footprint graphics
- deterministic net derivation from component pads
- textual board DSL emission
- built-in logical board models for:
  - `arduino_mega_2560_rev3`
  - `arduino_nano_v3`
  - `gy_sht31_d`
  - `mcp2515_tja1050_can_module`
  - `max31865_breakout`
  - `lc_lm358_pwm_to_0_10v`

Run its tests from the workspace root:

```sh
cargo test -p rust_board
```

The goal is to replace the Python board/DSL layer incrementally while keeping
the data model deterministic and easy to regression-test.
