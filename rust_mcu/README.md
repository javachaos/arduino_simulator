# rust_mcu

`rust_mcu` is the native MMIO/runtime side of the `avrsim` rewrite.

It depends on [`rust_cpu`](../rust_cpu/README.md) and keeps the CPU/MMIO
boundary explicit through the shared `DataBus` trait.

Current scope:

- typed ATmega328P and ATmega2560 buses
- timer0 and USART0 timing state
- Mega ADC timing state
- SPI/TWI hooks through narrow board traits
- side-by-side source layout so the Python implementation remains the reference

This crate is source-only for now because a Rust toolchain was not available in
this session when the rewrite started.

