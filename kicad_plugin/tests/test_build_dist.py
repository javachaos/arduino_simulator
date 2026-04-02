from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

import build_dist


class BuildDistTests(unittest.TestCase):
    def test_discover_binary_sets_merges_local_and_external_platforms(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            workspace = Path(temp_dir)
            local_release = workspace / "target" / "release"
            external_root = workspace / "dist" / "binaries" / "windows-x86_64"
            local_release.mkdir(parents=True)
            external_root.mkdir(parents=True)

            current_tag = build_dist.current_platform_tag()
            (local_release / build_dist.executable_name_for_platform(current_tag, build_dist.ADAPTER_BASENAME)).write_text("", encoding="utf-8")
            (local_release / build_dist.executable_name_for_platform(current_tag, build_dist.GUI_BASENAME)).write_text("", encoding="utf-8")
            (external_root / "arduino-simulator-kicad.exe").write_text("", encoding="utf-8")
            (external_root / "arduino-simulator-gui.exe").write_text("", encoding="utf-8")

            discovered = build_dist.discover_binary_sets(workspace, [workspace / "dist" / "binaries"])

            self.assertIn(current_tag, discovered)
            self.assertIn("windows-x86_64", discovered)

    def test_select_binary_sets_requires_requested_platforms(self) -> None:
        binary_sets = {
            "linux-x86_64": build_dist.BinarySet(
                platform_tag="linux-x86_64",
                adapter_binary=Path("/tmp/adapter"),
                gui_binary=Path("/tmp/gui"),
            )
        }

        with self.assertRaises(FileNotFoundError):
            build_dist.select_binary_sets(binary_sets, ["windows-x86_64"])

    def test_archive_suffix_uses_multi_platform_for_multiple_targets(self) -> None:
        binary_sets = {
            "linux-x86_64": build_dist.BinarySet(
                platform_tag="linux-x86_64",
                adapter_binary=Path("/tmp/adapter"),
                gui_binary=Path("/tmp/gui"),
            ),
            "windows-x86_64": build_dist.BinarySet(
                platform_tag="windows-x86_64",
                adapter_binary=Path("/tmp/adapter.exe"),
                gui_binary=Path("/tmp/gui.exe"),
            ),
        }

        self.assertEqual(build_dist.archive_suffix(binary_sets), "multi-platform")


if __name__ == "__main__":
    unittest.main()
