# `rust_web`

Minimal browser frontend for `arduino_simulator`.

Current scope:

- load a precompiled `firmware.hex`
- choose `Arduino Nano` or `Arduino Mega 2560`
- run, pause, step, and reset the simulation
- watch a live board canvas with pin activity
- read serial output

## Local development

From the repo root:

```bash
cargo check -p rust_web
trunk serve rust_web
```

## Static build

For a GitHub Pages deployment, point Trunk at the repo path as the public URL:

```bash
trunk build rust_web --release --public-url /arduino_simulator/
```
