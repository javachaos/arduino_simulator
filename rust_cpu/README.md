# rust_cpu

`rust_cpu` is the native side-by-side AVR core rewrite for `arduino_simulator`.

Design goals:

- keep CPU behavior locked down with deterministic conformance-style tests
- move the hot CPU loop to native code without dragging GUI/CLI code with it
- keep the MMIO boundary explicit through a `DataBus` trait
- prefer typed operands and explicit CPU state over dynamic dictionaries
