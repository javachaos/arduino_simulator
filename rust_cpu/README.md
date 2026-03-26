# rust_cpu

`rust_cpu` is the native side-by-side AVR core rewrite for `avrsim`.

Design goals:

- keep the Python simulator as the reference implementation
- move the hot CPU loop to native code without dragging GUI/CLI code with it
- keep the MMIO boundary explicit through a `DataBus` trait
- prefer typed operands and explicit CPU state over dynamic dictionaries

This crate is intentionally focused on the important core pieces first:

- CPU configuration for `ATmega328P` and `ATmega2560`
- program/data memory and stack behavior
- fetch/decode/execute loop
- interrupt entry
- typed bus callbacks for MMIO/peripheral layers

Because the local machine does not currently have a Rust toolchain installed, this
crate is checked in as source only and has not yet been compiled in this session.

