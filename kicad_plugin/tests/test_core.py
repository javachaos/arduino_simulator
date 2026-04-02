from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from arduino_simulator_kicad_plugin.core import (
    adapter_base_command,
    build_create_command,
    build_open_pcb_command,
    build_sync_command,
    current_platform_tag,
    default_project_path,
    discover_firmware_candidates,
    discover_workspace_root,
    read_project_firmware_path,
)


class CoreTests(unittest.TestCase):
    def test_default_project_path_tracks_pcb_stem(self) -> None:
        pcb_path = Path("/tmp/demo_board.kicad_pcb")
        self.assertEqual(
            default_project_path(pcb_path),
            Path("/tmp/demo_board.avrsim.json"),
        )

    def test_adapter_base_command_falls_back_to_cargo(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            repo_root = Path(temp_dir)
            (repo_root / "Cargo.toml").write_text("[workspace]\n", encoding="utf-8")
            command = adapter_base_command(repo_root)
            self.assertEqual(command[1:], ["run", "-p", "rust_kicad", "--"])
            self.assertTrue(command[0].endswith("cargo"))

    def test_adapter_base_command_prefers_bundled_binary(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            plugin_root = Path(temp_dir)
            bundled = plugin_root / "bin" / current_platform_tag() / "arduino-simulator-kicad"
            bundled.parent.mkdir(parents=True)
            bundled.write_text("", encoding="utf-8")
            command = adapter_base_command(plugin_root)
            self.assertEqual(command, [str(bundled.resolve())])

    def test_discover_workspace_root_searches_ancestors(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            workspace = Path(temp_dir)
            (workspace / "Cargo.toml").write_text("[workspace]\n", encoding="utf-8")
            nested = workspace / "kicad_plugin" / "arduino_simulator_kicad_plugin"
            nested.mkdir(parents=True)
            self.assertEqual(discover_workspace_root(nested), workspace.resolve())

    def test_build_create_command_uses_requested_arguments(self) -> None:
        command = build_create_command(
            "/repo",
            "/repo/board.kicad_pcb",
            "/repo/sketch.ino",
            out_path="/repo/board.avrsim.json",
            requested_board="mega",
            launch_gui=True,
        )
        self.assertTrue(command[0].endswith("cargo"))
        self.assertEqual(
            command[1:],
            [
                "run",
                "-p",
                "rust_kicad",
                "--",
                "create-project",
                "--pcb",
                "/repo/board.kicad_pcb",
                "--firmware",
                "/repo/sketch.ino",
                "--out",
                "/repo/board.avrsim.json",
                "--board",
                "mega",
                "--launch-gui",
            ],
        )

    def test_build_sync_command_uses_requested_arguments(self) -> None:
        command = build_sync_command(
            "/repo",
            "/repo/board.avrsim.json",
            requested_board="nano",
            launch_gui=False,
        )
        self.assertTrue(command[0].endswith("cargo"))
        self.assertEqual(
            command[1:],
            [
                "run",
                "-p",
                "rust_kicad",
                "--",
                "sync-project",
                "--project",
                "/repo/board.avrsim.json",
                "--board",
                "nano",
            ],
        )

    def test_build_open_pcb_command_uses_requested_arguments(self) -> None:
        command = build_open_pcb_command(
            "/repo",
            "/repo/board.kicad_pcb",
            requested_board="mega",
        )
        self.assertTrue(command[0].endswith("cargo"))
        self.assertEqual(
            command[1:],
            [
                "run",
                "-p",
                "rust_kicad",
                "--",
                "open-pcb",
                "--pcb",
                "/repo/board.kicad_pcb",
                "--board",
                "mega",
            ],
        )

    def test_read_project_firmware_path_handles_saved_projects(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            project_path = Path(temp_dir) / "demo.avrsim.json"
            project_path.write_text(
                json.dumps({"firmware": {"path": "/tmp/sketch.ino"}}),
                encoding="utf-8",
            )
            self.assertEqual(
                read_project_firmware_path(project_path),
                Path("/tmp/sketch.ino"),
            )

    def test_discover_firmware_candidates_finds_ino_and_hex(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "firmware").mkdir()
            (root / "firmware" / "demo.ino").write_text("// sketch", encoding="utf-8")
            (root / "demo.hex").write_text(":00000001FF", encoding="utf-8")

            candidates = discover_firmware_candidates(root)

            self.assertEqual(
                [path.name for path in candidates],
                ["demo.ino", "demo.hex"],
            )


if __name__ == "__main__":
    unittest.main()
