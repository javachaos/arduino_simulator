#!/usr/bin/env python3

from __future__ import annotations

import argparse
import shutil
from pathlib import Path


PLUGIN_NAME = "arduino_simulator_kicad_plugin"


def discover_default_plugins_dir() -> Path:
    kicad_root = Path.home() / "Documents" / "KiCad"
    candidates = sorted(
        (path / "scripting" / "plugins" for path in kicad_root.iterdir() if path.is_dir()),
        reverse=True,
    )

    for path in candidates:
        if path.is_dir():
            return path

    raise FileNotFoundError(
        f"could not find a KiCad user plugin directory under {kicad_root}"
    )


def install_plugin(destination_root: Path, *, copy_built_bundle: bool = False) -> Path:
    plugin_root = Path(__file__).resolve().parent
    if copy_built_bundle:
        source = plugin_root.parent / "dist" / "manual" / PLUGIN_NAME
        if not source.is_dir():
            raise FileNotFoundError(
                f"built plugin bundle not found at {source}; run build_dist.py first"
            )
    else:
        source = plugin_root / PLUGIN_NAME

    destination = destination_root / PLUGIN_NAME

    if destination.is_symlink() and destination.resolve() == source.resolve():
        return destination

    if destination.exists() or destination.is_symlink():
        raise FileExistsError(
            f"{destination} already exists; remove it first or pick a different directory"
        )

    destination_root.mkdir(parents=True, exist_ok=True)
    if copy_built_bundle:
        shutil.copytree(source, destination)
    else:
        destination.symlink_to(source, target_is_directory=True)
    return destination


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Install the arduino_simulator KiCad action plugin."
    )
    parser.add_argument(
        "--dest",
        type=Path,
        help="Override the KiCad user plugin directory.",
    )
    parser.add_argument(
        "--copy-built-bundle",
        action="store_true",
        help="Install the packaged plugin copy from dist/manual instead of a development symlink.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    destination_root = args.dest or discover_default_plugins_dir()
    installed_path = install_plugin(
        destination_root,
        copy_built_bundle=args.copy_built_bundle,
    )
    install_mode = "copied" if args.copy_built_bundle else "symlinked"
    print(f"{install_mode.capitalize()} KiCad plugin at {installed_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
