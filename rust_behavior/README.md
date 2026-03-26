# rust_behavior

`rust_behavior` provides the declarative runtime-behavior layer for `arduino_simulator`.

Current scope:

- built-in behavior definitions for the reusable module boards in this project
- compiled Rust behavior engines for:
  - `SHT31` I2C sensor modules
  - `MCP2515 + TJA1050` CAN modules
  - `MAX31865` RTD frontends
  - `PWM to 0-10V` interface boards
- helpers for loading a behavior definition from either a built-in name or a
  saved `behavior_definition` document reference

The board editor uses this crate to suggest behaviors for known built-in
modules and to preview the configured runtime behavior for each member.

Run its tests from the workspace root:

```sh
cargo test -p rust_behavior
```
