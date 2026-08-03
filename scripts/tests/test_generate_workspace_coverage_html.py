from __future__ import annotations

from decimal import Decimal
import importlib.util
import json
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "generate_workspace_coverage_html.py"
SPEC = importlib.util.spec_from_file_location("workspace_coverage_script", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
COVERAGE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = COVERAGE
SPEC.loader.exec_module(COVERAGE)


def file_coverage(path: str, covered: int, count: int):
    return COVERAGE.FileCoverage(
        path=Path(path),
        relative_path=path,
        crate=path.split("/", 1)[0],
        lines=COVERAGE.MetricSummary(count=count, covered=covered),
        functions=COVERAGE.MetricSummary(count=1, covered=1),
        regions=COVERAGE.MetricSummary(count=1, covered=1),
    )


class ObjectSelectionTests(unittest.TestCase):
    def test_web_and_kicad_test_objects_are_included(self) -> None:
        names = (
            "rust_web-deadbeef",
            "rust_kicad-deadbeef",
            "arduino_simulator_web-deadbeef",
            "arduino_simulator_kicad-deadbeef",
        )
        with tempfile.TemporaryDirectory() as directory:
            for name in names:
                path = Path(directory) / name
                path.write_bytes(b"")
                path.chmod(path.stat().st_mode | stat.S_IXUSR)
                self.assertTrue(COVERAGE.include_object(path), name)


class LlvmToolSelectionTests(unittest.TestCase):
    def test_active_sysroot_tools_are_preferred(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            sysroot = Path(directory)
            bin_dir = sysroot / "lib" / "rustlib" / "test-host" / "bin"
            bin_dir.mkdir(parents=True)
            llvm_cov = bin_dir / "llvm-cov"
            llvm_profdata = bin_dir / "llvm-profdata"
            llvm_cov.write_bytes(b"")
            llvm_profdata.write_bytes(b"")

            selected = COVERAGE.sysroot_llvm_tools(sysroot, "test-host")

        self.assertIsNotNone(selected)
        assert selected is not None
        self.assertEqual(selected.llvm_cov, llvm_cov)
        self.assertEqual(selected.llvm_profdata, llvm_profdata)
        self.assertEqual(selected.source, "active Rust sysroot")

    def test_locator_falls_back_to_path_when_sysroot_has_no_tools(self) -> None:
        rustc = Path("/fake/rustc")
        fallback = COVERAGE.LlvmTools(
            Path("/tools/llvm-cov"), Path("/tools/llvm-profdata"), "PATH fallback"
        )

        def command_result(command, **_kwargs):
            if command[0] == str(rustc):
                stdout = (
                    "/empty/sysroot\n"
                    if "sysroot" in command
                    else "host: test-host\nLLVM version: 22.1.0\n"
                )
            else:
                stdout = "LLVM version 22.1.0\n"
            return subprocess.CompletedProcess(command, 0, stdout=stdout, stderr="")

        with (
            mock.patch.object(COVERAGE.subprocess, "run", side_effect=command_result),
            mock.patch.object(COVERAGE, "path_llvm_tools", return_value=fallback),
            mock.patch.object(COVERAGE, "xcrun_llvm_tools") as xcrun,
        ):
            selected = COVERAGE.locate_rust_llvm_tools(rustc)

        self.assertEqual(selected, fallback)
        xcrun.assert_not_called()

    def test_locator_rejects_fallback_tools_from_another_llvm_major(self) -> None:
        rustc = Path("/fake/rustc")
        fallback = COVERAGE.LlvmTools(
            Path("/tools/llvm-cov"), Path("/tools/llvm-profdata"), "PATH fallback"
        )

        def command_result(command, **_kwargs):
            if command[0] == str(rustc):
                stdout = (
                    "/empty/sysroot\n"
                    if "sysroot" in command
                    else "host: test-host\nLLVM version: 22.1.0\n"
                )
            else:
                stdout = "LLVM version 21.0.0\n"
            return subprocess.CompletedProcess(command, 0, stdout=stdout, stderr="")

        with (
            mock.patch.object(COVERAGE.subprocess, "run", side_effect=command_result),
            mock.patch.object(COVERAGE, "path_llvm_tools", return_value=fallback),
            mock.patch.object(COVERAGE, "xcrun_llvm_tools"),
            self.assertRaises(SystemExit),
        ):
            COVERAGE.locate_rust_llvm_tools(rustc)

    def test_rustc_host_parser_rejects_incomplete_output(self) -> None:
        self.assertEqual(COVERAGE.parse_rustc_host("host: aarch64-test\n"), "aarch64-test")
        with self.assertRaises(SystemExit):
            COVERAGE.parse_rustc_host("rustc 1.0\n")


class CoverageGateTests(unittest.TestCase):
    def test_threshold_uses_unrounded_ratio(self) -> None:
        metric = COVERAGE.MetricSummary(count=3, covered=2)
        self.assertTrue(COVERAGE.metric_meets_threshold(metric, Decimal("66.66")))
        self.assertFalse(COVERAGE.metric_meets_threshold(metric, Decimal("66.67")))
        self.assertFalse(
            COVERAGE.metric_meets_threshold(
                COVERAGE.MetricSummary(count=0, covered=0), Decimal("0")
            )
        )

    def test_aggregate_and_per_file_failures_are_reported(self) -> None:
        entries = [
            file_coverage("rust_cpu/src/cpu.rs", 8, 10),
            file_coverage("rust_web/src/app.rs", 1, 2),
        ]

        failures = COVERAGE.line_gate_failures(
            entries,
            Decimal("80"),
            Decimal("60"),
        )

        self.assertEqual(len(failures), 2)
        self.assertIn("aggregate source lines", failures[0])
        self.assertIn("rust_web/src/app.rs", failures[1])

    def test_percentage_parser_rejects_nonfinite_or_out_of_range_values(self) -> None:
        for value in ("nan", "101", "-0.1", "not-a-number"):
            with self.subTest(value=value), self.assertRaises(
                COVERAGE.argparse.ArgumentTypeError
            ):
                COVERAGE.coverage_percentage(value)
        self.assertEqual(COVERAGE.coverage_percentage("88.5"), Decimal("88.5"))


class SourceSelectionTests(unittest.TestCase):
    def test_standalone_and_inline_tests_do_not_enter_source_denominator(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            production = root / "rust_web/src/runtime.rs"
            private_tests = root / "rust_web/src/test_runtime.rs"
            integration_tests = root / "rust_web/tests/runtime.rs"
            summary = root / "summary.json"
            lcov = root / "coverage.lcov"
            production.parent.mkdir(parents=True)
            integration_tests.parent.mkdir(parents=True)
            production.write_text(
                "\n".join(
                    [
                        "fn production_one() {}",
                        "",
                        "#[cfg(test)]",
                        "mod tests {",
                        "    fn private_test() {}",
                        "}",
                        "fn production_two() {}",
                        "fn production_three() {}",
                        "fn uncovered_one() {}",
                        "fn uncovered_two() {}",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )
            private_tests.write_text("fn private_test() {}\n", encoding="utf-8")
            integration_tests.write_text("fn integration_test() {}\n", encoding="utf-8")

            def llvm_file(path: Path, covered: int, count: int) -> dict:
                metric = {"covered": covered, "count": count}
                return {
                    "filename": str(path),
                    "summary": {
                        "lines": metric,
                        "functions": metric,
                        "regions": metric,
                    },
                }

            summary.write_text(
                json.dumps(
                    {
                        "data": [
                            {
                                "files": [
                                    llvm_file(production, 8, 10),
                                    llvm_file(private_tests, 100, 100),
                                    llvm_file(integration_tests, 100, 100),
                                ]
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )
            lcov.write_text(
                "\n".join(
                    [
                        f"SF:{production}",
                        *[f"DA:{line},{1 if line <= 8 else 0}" for line in range(1, 11)],
                        "end_of_record",
                        f"SF:{private_tests}",
                        "DA:1,1",
                        "end_of_record",
                        f"SF:{integration_tests}",
                        "DA:1,1",
                        "end_of_record",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )

            entries = COVERAGE.load_project_source_entries(root, summary, lcov)

        self.assertEqual(
            [entry.relative_path for entry in entries],
            ["rust_web/src/runtime.rs"],
        )
        self.assertAlmostEqual(
            COVERAGE.sum_metric(entries, "lines").percent,
            100.0 * 4.0 / 6.0,
        )

    def test_workspace_source_omissions_fail_except_explicit_declarations(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source_root = root / "crate/src"
            source_root.mkdir(parents=True)
            (root / "crate/Cargo.toml").write_text("[package]\n", encoding="utf-8")
            (source_root / "lib.rs").write_text("pub use crate::logic::*;\n", encoding="utf-8")
            (source_root / "logic.rs").write_text("pub fn covered() {}\n", encoding="utf-8")
            entries = [file_coverage("crate/src/logic.rs", 1, 1)]

            with mock.patch.object(
                COVERAGE,
                "DECLARATION_ONLY_SOURCE_PATHS",
                frozenset({"crate/src/lib.rs"}),
            ):
                COVERAGE.validate_workspace_source_completeness(root, entries)
                (source_root / "omitted.rs").write_text(
                    "pub fn omitted() {}\n", encoding="utf-8"
                )
                with self.assertRaises(SystemExit):
                    COVERAGE.validate_workspace_source_completeness(root, entries)


class BuildDirectoryTests(unittest.TestCase):
    def test_build_directory_is_cleaned_unless_reuse_is_requested(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            root.mkdir()
            output = root / "coverage-output"
            build = root / "target" / "coverage-build"
            build.mkdir(parents=True)
            stale = build / "stale-object"
            stale.write_bytes(b"old")
            (build / COVERAGE.BUILD_SENTINEL).write_text(
                "Arduino Simulator coverage build\n", encoding="utf-8"
            )

            COVERAGE.prepare_build_directory(
                build, repo_root=root, output_dir=output, reuse=False
            )
            self.assertFalse(stale.exists())

            retained = build / "retained-object"
            retained.write_bytes(b"keep")
            COVERAGE.prepare_build_directory(
                build, repo_root=root, output_dir=output, reuse=True
            )
            self.assertTrue(retained.exists())

    def test_output_cannot_be_nested_under_cleaned_build_directory(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            root.mkdir()
            build = root / "target" / "coverage-build"
            with self.assertRaises(SystemExit):
                COVERAGE.prepare_build_directory(
                    build,
                    repo_root=root,
                    output_dir=build / "output",
                    reuse=False,
                )

    def test_source_and_unrecognized_custom_directories_are_never_cleaned(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            source = root / "rust_web"
            source.mkdir(parents=True)
            source_marker = source / "lib.rs"
            source_marker.write_text("production", encoding="utf-8")
            with self.assertRaises(SystemExit):
                COVERAGE.prepare_build_directory(
                    source,
                    repo_root=root,
                    output_dir=root / "coverage-output",
                    reuse=False,
                )
            self.assertTrue(source_marker.is_file())

            custom = Path(directory) / "custom-build"
            custom.mkdir()
            custom_marker = custom / "keep.txt"
            custom_marker.write_text("important", encoding="utf-8")
            with self.assertRaises(SystemExit):
                COVERAGE.prepare_build_directory(
                    custom,
                    repo_root=root,
                    output_dir=root / "coverage-output",
                    reuse=False,
                )
            self.assertTrue(custom_marker.is_file())

    def test_display_path_prefers_workspace_relative_paths(self) -> None:
        root = Path("/workspace/project")
        self.assertEqual(COVERAGE.display_path(root, root), ".")
        self.assertEqual(
            COVERAGE.display_path(root / "target" / "coverage-html", root),
            "target/coverage-html",
        )


if __name__ == "__main__":
    unittest.main()
