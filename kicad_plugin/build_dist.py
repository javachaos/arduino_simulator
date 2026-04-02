#!/usr/bin/env python3

from __future__ import annotations

from dataclasses import dataclass
import argparse
import binascii
import json
from pathlib import Path
import platform
import re
import shutil
import struct
import subprocess
import zipfile
import zlib


PLUGIN_NAME = "arduino_simulator_kicad_plugin"
PACKAGE_IDENTIFIER = "com.github.alfredladeroute.arduino-simulator"
PACKAGE_NAME = "Arduino Simulator"
PACKAGE_DESCRIPTION = (
    "Open KiCad PCBs in generic mode or firmware-backed Arduino simulation mode."
)
PACKAGE_DESCRIPTION_FULL = (
    "Arduino Simulator adds a KiCad PCB action plugin that can open arbitrary boards in "
    "generic PCB mode or generate and refresh .avrsim project files for Arduino-oriented "
    "simulations."
)
ADAPTER_BASENAME = "arduino-simulator-kicad"
GUI_BASENAME = "arduino-simulator-gui"
DEFAULT_BINARY_ROOT = Path("dist/binaries")
RUST_TARGET_TO_PLATFORM_TAG = {
    "x86_64-unknown-linux-gnu": "linux-x86_64",
    "aarch64-unknown-linux-gnu": "linux-arm64",
    "x86_64-pc-windows-msvc": "windows-x86_64",
    "aarch64-pc-windows-msvc": "windows-arm64",
    "x86_64-apple-darwin": "macos-x86_64",
    "aarch64-apple-darwin": "macos-arm64",
}


@dataclass(frozen=True)
class BinarySet:
    platform_tag: str
    adapter_binary: Path
    gui_binary: Path


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build a distributable KiCad plugin archive for arduino_simulator."
    )
    parser.add_argument(
        "--skip-build",
        action="store_true",
        help="Reuse existing binaries instead of running cargo build --release for the host platform.",
    )
    parser.add_argument(
        "--version",
        help="Override the package version recorded in metadata.json.",
    )
    parser.add_argument(
        "--binary-root",
        action="append",
        type=Path,
        default=[],
        help=(
            "Directory containing per-platform binary folders such as "
            "linux-x86_64/arduino-simulator-gui. Can be passed multiple times."
        ),
    )
    parser.add_argument(
        "--platform",
        action="append",
        dest="platforms",
        help="Limit the packaged bundle to one or more specific platform tags.",
    )
    return parser.parse_args()


def workspace_version(cargo_toml: Path) -> str:
    content = cargo_toml.read_text(encoding="utf-8")
    match = re.search(r'(?m)^version\s*=\s*"([^"]+)"\s*$', content)
    if not match:
        raise ValueError(f"could not locate workspace version in {cargo_toml}")
    return match.group(1)


def current_platform_tag() -> str:
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


def os_name_from_platform_tag(platform_tag: str) -> str:
    return platform_tag.split("-", 1)[0].lower()


def executable_name_for_platform(platform_tag: str, basename: str) -> str:
    if os_name_from_platform_tag(platform_tag) == "windows":
        return f"{basename}.exe"
    return basename


def build_release_binaries(workspace_root: Path) -> None:
    subprocess.run(
        ["cargo", "build", "--release", "-p", "rust_gui", "-p", "rust_kicad"],
        cwd=workspace_root,
        check=True,
    )


def png_chunk(chunk_type: bytes, payload: bytes) -> bytes:
    return (
        struct.pack(">I", len(payload))
        + chunk_type
        + payload
        + struct.pack(">I", binascii.crc32(chunk_type + payload) & 0xFFFFFFFF)
    )


def solid_png(path: Path, width: int, height: int, rgb: tuple[int, int, int]) -> None:
    def row_bytes() -> bytes:
        return b"".join(bytes(rgb) for _ in range(width))

    raw = b"".join(b"\x00" + row_bytes() for _ in range(height))
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    data = (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", ihdr)
        + png_chunk(b"IDAT", zlib.compress(raw, level=9))
        + png_chunk(b"IEND", b"")
    )
    path.write_bytes(data)


def metadata(version: str) -> dict[str, object]:
    return {
        "$schema": "https://go.kicad.org/pcm/schemas/v2",
        "name": PACKAGE_NAME,
        "description": PACKAGE_DESCRIPTION,
        "description_full": PACKAGE_DESCRIPTION_FULL,
        "identifier": PACKAGE_IDENTIFIER,
        "type": "plugin",
        "author": {
            "name": "Alfred Laderoute",
            "contact": {
                "web": "https://github.com/alfredladeroute",
            },
        },
        "license": "GPL-3.0-or-later",
        "resources": {
            "homepage": "https://github.com/alfredladeroute/arduino_simulator",
        },
        "versions": [
            {
                "version": version,
                "status": "testing",
                "kicad_version": "10.0",
                "runtime": "swig",
            }
        ],
    }


def ensure_binary_set(platform_tag: str, directory: Path) -> BinarySet | None:
    adapter = directory / executable_name_for_platform(platform_tag, ADAPTER_BASENAME)
    gui = directory / executable_name_for_platform(platform_tag, GUI_BASENAME)
    if adapter.is_file() and gui.is_file():
        return BinarySet(
            platform_tag=platform_tag,
            adapter_binary=adapter.resolve(),
            gui_binary=gui.resolve(),
        )
    return None


def discover_local_binary_sets(workspace_root: Path) -> dict[str, BinarySet]:
    discovered: dict[str, BinarySet] = {}

    current_release = workspace_root / "target" / "release"
    current_set = ensure_binary_set(current_platform_tag(), current_release)
    if current_set is not None:
        discovered[current_set.platform_tag] = current_set

    for rust_target, platform_tag in RUST_TARGET_TO_PLATFORM_TAG.items():
        target_release = workspace_root / "target" / rust_target / "release"
        binary_set = ensure_binary_set(platform_tag, target_release)
        if binary_set is not None:
            discovered[binary_set.platform_tag] = binary_set

    return discovered


def discover_external_binary_sets(binary_roots: list[Path]) -> dict[str, BinarySet]:
    discovered: dict[str, BinarySet] = {}
    for root in binary_roots:
        if not root.is_dir():
            continue
        for child in sorted(root.iterdir()):
            if not child.is_dir():
                continue
            binary_set = ensure_binary_set(child.name, child)
            if binary_set is not None:
                discovered[binary_set.platform_tag] = binary_set
    return discovered


def discover_binary_sets(
    workspace_root: Path,
    binary_roots: list[Path],
) -> dict[str, BinarySet]:
    discovered = discover_local_binary_sets(workspace_root)
    discovered.update(discover_external_binary_sets(binary_roots))
    return dict(sorted(discovered.items()))


def select_binary_sets(
    binary_sets: dict[str, BinarySet],
    requested_platforms: list[str] | None,
) -> dict[str, BinarySet]:
    if not requested_platforms:
        return binary_sets

    selected: dict[str, BinarySet] = {}
    missing: list[str] = []
    for platform_tag in requested_platforms:
        binary_set = binary_sets.get(platform_tag)
        if binary_set is None:
            missing.append(platform_tag)
        else:
            selected[platform_tag] = binary_set

    if missing:
        missing_list = ", ".join(missing)
        available = ", ".join(binary_sets) or "none"
        raise FileNotFoundError(
            f"missing requested platform binaries for: {missing_list}. Available: {available}"
        )

    return selected


def archive_suffix(binary_sets: dict[str, BinarySet]) -> str:
    if len(binary_sets) == 1:
        return next(iter(binary_sets))
    return "multi-platform"


def build_package(
    skip_build: bool,
    version: str,
    binary_roots: list[Path],
    requested_platforms: list[str] | None,
) -> tuple[Path, dict[str, BinarySet]]:
    workspace_root = repo_root()
    dist_root = workspace_root / "dist"
    stage_root = dist_root / "package"
    manual_root = dist_root / "manual" / PLUGIN_NAME
    plugin_source = workspace_root / "kicad_plugin" / PLUGIN_NAME

    if not skip_build:
        build_release_binaries(workspace_root)

    discovered = discover_binary_sets(workspace_root, binary_roots)
    selected = select_binary_sets(discovered, requested_platforms)
    if not selected:
        searched = ", ".join(str(path) for path in binary_roots) or "no external roots"
        raise FileNotFoundError(
            "no platform binaries were found. Build the host release binaries or provide "
            f"additional per-platform binary roots. Searched external roots: {searched}"
        )

    dist_root.mkdir(parents=True, exist_ok=True)
    if stage_root.exists():
        shutil.rmtree(stage_root)
    if manual_root.exists():
        shutil.rmtree(manual_root)
    (stage_root / "plugins").mkdir(parents=True, exist_ok=True)
    (stage_root / "resources").mkdir(parents=True, exist_ok=True)
    manual_root.mkdir(parents=True, exist_ok=True)

    for file_name in ("__init__.py", "core.py", "plugin.py"):
        shutil.copy2(plugin_source / file_name, stage_root / "plugins" / file_name)
        shutil.copy2(plugin_source / file_name, manual_root / file_name)

    for platform_tag, binary_set in selected.items():
        stage_bin = stage_root / "plugins" / "bin" / platform_tag
        stage_bin.mkdir(parents=True, exist_ok=True)
        shutil.copy2(binary_set.adapter_binary, stage_bin / binary_set.adapter_binary.name)
        shutil.copy2(binary_set.gui_binary, stage_bin / binary_set.gui_binary.name)

        manual_bin = manual_root / "bin" / platform_tag
        manual_bin.mkdir(parents=True, exist_ok=True)
        shutil.copy2(binary_set.adapter_binary, manual_bin / binary_set.adapter_binary.name)
        shutil.copy2(binary_set.gui_binary, manual_bin / binary_set.gui_binary.name)

    solid_png(stage_root / "resources" / "icon.png", 64, 64, (44, 117, 255))
    solid_png(manual_root / "icon_24x24.png", 24, 24, (44, 117, 255))
    shutil.copy2(
        manual_root / "icon_24x24.png",
        stage_root / "plugins" / "icon_24x24.png",
    )

    metadata_path = stage_root / "metadata.json"
    metadata_path.write_text(json.dumps(metadata(version), indent=2) + "\n", encoding="utf-8")

    bundled_platforms_path = dist_root / "BUNDLED_PLATFORMS.txt"
    bundled_platforms_path.write_text(
        "".join(f"{platform_tag}\n" for platform_tag in selected),
        encoding="utf-8",
    )

    suffix = archive_suffix(selected)
    archive_name = f"arduino-simulator-kicad-{version}-{suffix}.zip"
    archive_path = dist_root / archive_name
    if archive_path.exists():
        archive_path.unlink()
    with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(stage_root.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(stage_root).as_posix())

    return archive_path, selected


def main() -> int:
    args = parse_args()
    version = args.version or workspace_version(repo_root() / "Cargo.toml")
    binary_roots = [(repo_root() / DEFAULT_BINARY_ROOT).resolve()]
    for root in args.binary_root:
        binary_roots.append((root if root.is_absolute() else (Path.cwd() / root)).resolve())
    archive_path, selected = build_package(
        args.skip_build,
        version,
        binary_roots=binary_roots,
        requested_platforms=args.platforms,
    )
    platform_summary = ", ".join(selected) or "none"
    print(f"Built KiCad plugin archive: {archive_path}")
    print(f"Bundled platforms: {platform_summary}")
    print(f"Bundled platform manifest: {archive_path.parent / 'BUNDLED_PLATFORMS.txt'}")
    print(f"Manual-install plugin folder: {archive_path.parent / 'manual' / PLUGIN_NAME}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
