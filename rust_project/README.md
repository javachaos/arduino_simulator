# rust_project

`rust_project` defines the saved document formats for `avrsim`.

Current document kinds:

- `simulation_project`
  A runnable setup that ties together the selected host board, firmware,
  PCB, bindings, probes, and stimuli.
- `board_definition`
  A reusable board-level definition with a physical or virtual source and
  a set of exposed ports.
- `module_definition`
  A reusable attached module, optionally with an embedded host board and
  default firmware.
- `assembly_definition`
  A hierarchical composition of a primary board plus attached child
  instances, attachments, and exported assembly ports.
- `assembly_bundle`
  A single-file board-stack document that inlines the primary board/module
  members, their PCB or built-in sources, optional host-board/firmware
  associations, attachments, and exported ports. This is the native format
  used by the GUI board editor.
- `behavior_definition`
  A declarative runtime-behavior document that binds member ports to a
  compiled Rust behavior engine like `sht31_i2c_sensor` or
  `max31865_rtd_frontend`.

The JSON format is tagged with a `kind` field so one file type can carry all
six document categories cleanly.

`assembly_bundle` members can optionally reference a `behavior_definition`
either by file path or by built-in behavior name.

Run its tests from the workspace root:

```sh
cargo test -p rust_project
```
