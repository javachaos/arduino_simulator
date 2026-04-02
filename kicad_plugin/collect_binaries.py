#!/usr/bin/env python3

from __future__ import annotations

import argparse
from pathlib import Path
import shutil


ADAPTER_BASENAME = "arduino-simulator-kicad"
GUI_BASENAME = "arduino-simulator-gui"


def current_platform_tag() -> str:
    import platform

    system_name = platform.system().lower()
    machine_name = platform.machine().lower()

    if system_name.startswith("darwin") or system_name == "macos":
        os_name = "macos"
    elif system_name.startswith("win"):
        os_name = "windows"
    else:
        os_name = "linux"

    if machine_name in {"arm64", "aarch64"}:
        arch = "arm64"
    elif machine_name in {"x86_64", "amd64"}:
        arch = "x86_64"
    else:
        arch = (
            "".join(character if character.isalnum() else "-" for character in machine_name)
            .strip("-")
            or "unknown"
        )

    return f"{os_name}-{arch}"


def executable_name_for_platform(platform_tag: str, basename: str) -> str:
    if platform_tag.startswith("windows-"):
        return f"{basename}.exe"
    return basename


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Collect normalized arduino_simulator release binaries for KiCad packaging."
    )
    parser.add_argument(
        "--platform-tag",
        default=current_platform_tag(),
        help="Normalized package platform tag such as macos-arm64 or windows-x86_64.",
    )
    parser.add_argument(
        "--release-root",
        type=Path,
        required=True,
        help="Directory containing the built release binaries.",
    )
    parser.add_argument(
        "--out-root",
        type=Path,
        required=True,
        help="Destination root that will receive <platform-tag>/binary files.",
    )
    return parser.parse_args()


def copy_binaries(platform_tag: str, release_root: Path, out_root: Path) -> Path:
    adapter = release_root / executable_name_for_platform(platform_tag, ADAPTER_BASENAME)
    gui = release_root / executable_name_for_platform(platform_tag, GUI_BASENAME)
    missing = [str(path) for path in (adapter, gui) if not path.is_file()]
    if missing:
        missing_list = ", ".join(missing)
        raise FileNotFoundError(f"missing release binaries: {missing_list}")

    target_dir = out_root / platform_tag
    target_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(adapter, target_dir / adapter.name)
    shutil.copy2(gui, target_dir / gui.name)
    return target_dir


def main() -> int:
    args = parse_args()
    target_dir = copy_binaries(
        platform_tag=args.platform_tag,
        release_root=args.release_root,
        out_root=args.out_root,
    )
    print(f"Collected binaries into {target_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
