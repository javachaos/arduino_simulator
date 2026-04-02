# KiCad Plugin

This folder contains the KiCad-facing packaging and action-plugin layer for `arduino_simulator`.

The plugin stays outside the Rust simulator core on purpose. It keeps the KiCad-specific
UI and process-launch behavior at the edge while delegating project generation and board
loading back to `rust_kicad` and `rust_gui`.

## What it does

Inside KiCad PCB Editor, the plugin supports two launch modes.

Generic PCB mode:

1. reads the active `.kicad_pcb`
2. opens the simulator GUI directly on that board
3. does not require firmware or an `.avrsim.json` project

Arduino simulation project mode:

1. reads the active `.kicad_pcb`
2. reuses `<board>.avrsim.json` if one already exists
3. prompts for firmware only on first run
4. shells out to `arduino-simulator-kicad`
5. launches the simulator GUI into the generated or refreshed project

## Development install

From the workspace root:

```sh
python3 /Users/fred/Documents/arduino_simulator/kicad_plugin/install_kicad_plugin.py
```

The installer creates a symlink into your KiCad user plugin directory, usually:

```text
~/Documents/KiCad/10.0/scripting/plugins/arduino_simulator_kicad_plugin
```

## Packaged distribution

Build a KiCad bundle from every available platform binary set with:

```sh
python3 /Users/fred/Documents/arduino_simulator/kicad_plugin/build_dist.py
```

By default the script will:

- build fresh host-platform release binaries
- scan `target/<rust-target>/release` for any additional prebuilt targets
- scan `dist/binaries/<platform-tag>/` for externally collected binaries
- emit a single KiCad PCM ZIP that contains every discovered platform under `plugins/bin/`

The normalized package tags currently cover:

- `linux-x86_64`
- `linux-arm64`
- `windows-x86_64`
- `windows-arm64`
- `macos-x86_64`
- `macos-arm64`

To collect a runner's local binaries into that normalized layout, use:

```sh
python3 /Users/fred/Documents/arduino_simulator/kicad_plugin/collect_binaries.py \
  --release-root /path/to/target/release \
  --out-root /Users/fred/Documents/arduino_simulator/dist/binaries
```

That emits folders like:

```text
dist/binaries/linux-x86_64/
dist/binaries/linux-arm64/
dist/binaries/windows-x86_64/
dist/binaries/windows-arm64/
dist/binaries/macos-x86_64/
dist/binaries/macos-arm64/
```

Install the resulting ZIP in KiCad through **Plugin and Content Manager -> Install from File...**.
The build also writes `dist/BUNDLED_PLATFORMS.txt` so release automation and manual
testers can see exactly which platform binaries were included.

## CI packaging

The repository includes a GitHub Actions workflow at `.github/workflows/build-kicad-plugin.yml`
that builds the plugin binaries on the major runner types, uploads normalized binary artifacts,
and then assembles a multi-platform KiCad bundle from the successful targets.
When a tag like `v0.1.0` is pushed, that workflow also publishes a GitHub Release with
the generated ZIP, checksums, and bundled-platform manifest attached as assets.

## Use

In KiCad PCB Editor:

- open your board
- save the board if you want the latest changes reflected
- run `Tools -> External Plugins -> Open Arduino Simulator`

Then choose:

- `Generic PCB mode` for arbitrary boards
- `Arduino simulation project` when you want firmware, host-board bindings, and
  `.avrsim.json` project sync
