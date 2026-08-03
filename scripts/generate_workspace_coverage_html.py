#!/usr/bin/env python3
"""Generate an HTML coverage summary for the Arduino Simulator workspace."""

from __future__ import annotations

import argparse
import bisect
import datetime as dt
from decimal import Decimal, InvalidOperation
import html
import json
import os
from dataclasses import dataclass
from pathlib import Path
import re
import shutil
import stat
import subprocess
import tempfile
from typing import Iterable


WORKSPACE_OBJECT_PREFIXES = (
    "librust_",
    "rust_behavior-",
    "rust_board-",
    "rust_cpu-",
    "rust_gui-",
    "rust_kicad-",
    "rust_mcu-",
    "rust_project-",
    "rust_runtime-",
    "rust_web-",
    "avr_",
    "cpu_",
    "runtime_bus_tests-",
    "cli_runtime-",
    "arduino_simulator",
    "arduino_simulator-",
    "arduino_simulator_gui",
    "arduino_simulator_gui-",
    "arduino_simulator_kicad",
    "arduino_simulator_kicad-",
    "arduino_simulator_web",
    "arduino_simulator_web-",
)
BUILD_SENTINEL = ".arduino-simulator-coverage-build"
DECLARATION_ONLY_SOURCE_PATHS = frozenset(
    {
        "rust_board/src/lib.rs",
        "rust_cpu/src/instruction.rs",
        "rust_gui/src/lib.rs",
        "rust_project/src/lib.rs",
        "rust_runtime/src/lib.rs",
        "rust_web/src/lib.rs",
    }
)


@dataclass
class MetricSummary:
    count: int
    covered: int

    @property
    def missed(self) -> int:
        return self.count - self.covered

    @property
    def percent(self) -> float:
        if self.count == 0:
            return 0.0
        return (self.covered / self.count) * 100.0


@dataclass
class FileCoverage:
    path: Path
    relative_path: str
    crate: str
    lines: MetricSummary
    functions: MetricSummary
    regions: MetricSummary


@dataclass(frozen=True)
class LlvmTools:
    llvm_cov: Path
    llvm_profdata: Path
    source: str


def coverage_percentage(value: str) -> Decimal:
    try:
        percentage = Decimal(value)
    except InvalidOperation as error:
        raise argparse.ArgumentTypeError(f"invalid percentage: {value}") from error
    if not percentage.is_finite() or not Decimal(0) <= percentage <= Decimal(100):
        raise argparse.ArgumentTypeError("percentage must be between 0 and 100")
    return percentage


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    script_dir = Path(__file__).resolve().parent
    repo_root = script_dir.parent
    default_output_dir = repo_root / "target" / "coverage-html"
    default_build_dir = repo_root / "target" / "coverage-build"

    parser = argparse.ArgumentParser(
        description="Run workspace coverage and generate an HTML summary."
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=default_output_dir,
        help="Directory for generated HTML and data files "
        f"(default: {default_output_dir.relative_to(repo_root)})",
    )
    parser.add_argument(
        "--build-dir",
        type=Path,
        default=default_build_dir,
        help="Instrumented cargo target directory "
        f"(default: {default_build_dir.relative_to(repo_root)})",
    )
    parser.add_argument(
        "--reuse-build",
        action="store_true",
        help="Reuse the instrumented Cargo build directory instead of cleaning it first.",
    )
    parser.add_argument(
        "--fail-under-lines",
        type=coverage_percentage,
        metavar="PERCENT",
        help="Fail when aggregate source line coverage is below this percentage.",
    )
    parser.add_argument(
        "--fail-under-file-lines",
        type=coverage_percentage,
        metavar="PERCENT",
        help="Fail when any included source file is below this line percentage.",
    )
    return parser.parse_args(argv)


def require_program(name: str) -> Path:
    resolved = shutil.which(name)
    if not resolved:
        raise SystemExit(f"missing required tool: {name}")
    return Path(resolved)


def parse_rustc_host(verbose_version: str) -> str:
    for line in verbose_version.splitlines():
        if line.startswith("host:"):
            host = line.partition(":")[2].strip()
            if host:
                return host
    raise SystemExit("could not determine active rustc host from `rustc -vV`")


def parse_llvm_major(version: str, source: str) -> int:
    match = re.search(r"LLVM version:?\s*(\d+)", version, re.IGNORECASE)
    if not match:
        raise SystemExit(f"could not determine LLVM version from {source}")
    return int(match.group(1))


def tool_llvm_major(tool: Path) -> int:
    result = subprocess.run(
        [str(tool), "--version"],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise SystemExit(f"failed to query LLVM version from {tool}")
    return parse_llvm_major(result.stdout + result.stderr, str(tool))


def sysroot_llvm_tools(sysroot: Path, host: str) -> LlvmTools | None:
    bin_dir = sysroot / "lib" / "rustlib" / host / "bin"
    suffixes = (".exe", "") if os.name == "nt" else ("", ".exe")
    for suffix in suffixes:
        llvm_cov = bin_dir / f"llvm-cov{suffix}"
        llvm_profdata = bin_dir / f"llvm-profdata{suffix}"
        if llvm_cov.is_file() and llvm_profdata.is_file():
            return LlvmTools(llvm_cov, llvm_profdata, "active Rust sysroot")
    return None


def path_llvm_tools() -> LlvmTools | None:
    llvm_cov = shutil.which("llvm-cov")
    llvm_profdata = shutil.which("llvm-profdata")
    if llvm_cov and llvm_profdata:
        return LlvmTools(Path(llvm_cov), Path(llvm_profdata), "PATH fallback")
    return None


def xcrun_llvm_tools() -> LlvmTools | None:
    xcrun = shutil.which("xcrun")
    if not xcrun:
        return None

    resolved: dict[str, Path] = {}
    for tool in ("llvm-cov", "llvm-profdata"):
        result = subprocess.run(
            [xcrun, "--find", tool],
            check=False,
            capture_output=True,
            text=True,
        )
        candidate = Path(result.stdout.strip())
        if result.returncode != 0 or not candidate.is_file():
            return None
        resolved[tool] = candidate
    return LlvmTools(resolved["llvm-cov"], resolved["llvm-profdata"], "xcrun fallback")


def locate_rust_llvm_tools(rustc: Path, *, cwd: Path | None = None) -> LlvmTools:
    sysroot_result = subprocess.run(
        [str(rustc), "--print", "sysroot"],
        cwd=cwd,
        check=False,
        capture_output=True,
        text=True,
    )
    version_result = subprocess.run(
        [str(rustc), "-vV"],
        cwd=cwd,
        check=False,
        capture_output=True,
        text=True,
    )
    if sysroot_result.returncode != 0 or version_result.returncode != 0:
        raise SystemExit("failed to query the active rustc toolchain")

    sysroot = Path(sysroot_result.stdout.strip())
    host = parse_rustc_host(version_result.stdout)
    rustc_llvm_major = parse_llvm_major(version_result.stdout, "`rustc -vV`")
    tools = sysroot_llvm_tools(sysroot, host)
    if tools:
        return tools

    tools = path_llvm_tools() or xcrun_llvm_tools()
    if tools:
        for tool in (tools.llvm_cov, tools.llvm_profdata):
            actual_major = tool_llvm_major(tool)
            if actual_major != rustc_llvm_major:
                raise SystemExit(
                    f"{tool} uses LLVM {actual_major}, but rustc uses LLVM "
                    f"{rustc_llvm_major}; install Rust's llvm-tools component"
                )
        return tools

    raise SystemExit(
        "missing llvm-cov and llvm-profdata for the active Rust toolchain; "
        "install Rust's llvm-tools component or put matching LLVM tools on PATH"
    )


def run_command(
    command: list[str],
    *,
    cwd: Path,
    env: dict[str, str] | None = None,
    capture_output: bool = False,
) -> subprocess.CompletedProcess[str]:
    print(f"$ {' '.join(command)}", flush=True)
    return subprocess.run(
        command,
        cwd=cwd,
        env=env,
        check=True,
        text=True,
        capture_output=capture_output,
    )


def include_object(path: Path) -> bool:
    if path.suffix == ".rlib":
        return path.name.startswith("librust_")
    try:
        mode = path.stat().st_mode
    except FileNotFoundError:
        return False
    if not (mode & stat.S_IXUSR):
        return False
    return path.name.startswith(WORKSPACE_OBJECT_PREFIXES)


def collect_objects(build_dir: Path) -> list[Path]:
    deps_dir = build_dir / "debug" / "deps"
    objects: list[Path] = []
    for path in sorted(deps_dir.iterdir()):
        if path.is_file() and include_object(path):
            objects.append(path)
    if not objects:
        raise SystemExit(f"no workspace objects found under {deps_dir}")
    return objects


def export_summary_json(
    repo_root: Path,
    objects: Iterable[Path],
    profdata_path: Path,
    summary_json_path: Path,
    llvm_cov: Path,
) -> None:
    command = [
        str(llvm_cov),
        "export",
        "-summary-only",
        f"-instr-profile={profdata_path}",
    ]
    command.extend(f"--object={path}" for path in objects)
    result = run_command(command, cwd=repo_root, capture_output=True)
    summary_json_path.write_text(result.stdout, encoding="utf-8")


def export_lcov(
    repo_root: Path,
    objects: Iterable[Path],
    profdata_path: Path,
    lcov_path: Path,
    llvm_cov: Path,
) -> None:
    command = [
        str(llvm_cov),
        "export",
        "-format=lcov",
        "--skip-functions",
        "--skip-branches",
        f"-instr-profile={profdata_path}",
    ]
    command.extend(f"--object={path}" for path in objects)
    result = run_command(command, cwd=repo_root, capture_output=True)
    lcov_path.write_text(result.stdout, encoding="utf-8")


def parse_lcov(lcov_path: Path) -> dict[Path, dict[int, int]]:
    files: dict[Path, dict[int, int]] = {}
    current: Path | None = None
    for record in lcov_path.read_text(encoding="utf-8").splitlines():
        if record.startswith("SF:"):
            current = Path(record[3:]).resolve()
            files.setdefault(current, {})
        elif record.startswith("DA:"):
            if current is None:
                raise SystemExit("LCOV DA record appeared before an SF record")
            fields = record[3:].split(",", 2)
            try:
                line_number = int(fields[0])
                execution_count = int(fields[1])
            except (IndexError, ValueError) as error:
                raise SystemExit(f"invalid LCOV line record: {record!r}") from error
            if line_number <= 0 or execution_count < 0:
                raise SystemExit(f"invalid LCOV line record: {record!r}")
            files[current][line_number] = max(
                files[current].get(line_number, 0), execution_count
            )
        elif record == "end_of_record":
            current = None
    if not files:
        raise SystemExit("LLVM LCOV export did not contain any source files")
    return files


def mask_rust_comments_and_literals(source: str) -> str:
    masked = list(source)
    length = len(source)

    def blank(start: int, end: int) -> None:
        for index in range(start, end):
            if masked[index] != "\n":
                masked[index] = " "

    index = 0
    while index < length:
        if source.startswith("//", index):
            end = source.find("\n", index)
            end = length if end < 0 else end
            blank(index, end)
            index = end
            continue
        if source.startswith("/*", index):
            start = index
            index += 2
            depth = 1
            while index < length and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            if depth:
                raise SystemExit("unterminated Rust block comment")
            blank(start, index)
            continue

        raw_match = re.match(r"(?:br|rb|r)(?P<hashes>#+)?\"", source[index:])
        if raw_match:
            hashes = raw_match.group("hashes") or ""
            end_marker = f'\"{hashes}'
            end = source.find(end_marker, index + raw_match.end())
            if end < 0:
                raise SystemExit("unterminated Rust raw string")
            end += len(end_marker)
            blank(index, end)
            index = end
            continue

        quote_index = index
        if source[index] == '"':
            pass
        elif source[index] == "b" and index + 1 < length and source[index + 1] == '"':
            quote_index += 1
        else:
            quote_index = -1
        if quote_index >= 0:
            start = index
            index = quote_index + 1
            while index < length:
                if source[index] == "\\":
                    index += 2
                elif source[index] == '"':
                    index += 1
                    break
                else:
                    index += 1
            else:
                raise SystemExit("unterminated Rust string")
            blank(start, index)
            continue

        char_index = index
        if source[index] == "'":
            pass
        elif source[index] == "b" and index + 1 < length and source[index + 1] == "'":
            char_index += 1
        else:
            char_index = -1
        if char_index >= 0:
            if (
                char_index == index
                and char_index + 1 < length
                and (source[char_index + 1].isalpha() or source[char_index + 1] == "_")
            ):
                lifetime_end = char_index + 2
                while lifetime_end < length and (
                    source[lifetime_end].isalnum() or source[lifetime_end] == "_"
                ):
                    lifetime_end += 1
                if lifetime_end >= length or source[lifetime_end] != "'":
                    index += 1
                    continue
            candidate = char_index + 1
            while candidate < length and source[candidate] != "\n":
                if source[candidate] == "\\":
                    candidate += 2
                elif source[candidate] == "'":
                    candidate += 1
                    blank(index, candidate)
                    index = candidate
                    break
                else:
                    candidate += 1
            else:
                index += 1
            continue
        index += 1
    return "".join(masked)


def rust_cfg_test_line_ranges(source: str, *, label: str) -> list[tuple[int, int]]:
    clean = mask_rust_comments_and_literals(source)
    generic_cfg = re.compile(r"#\s*\[\s*cfg\s*\(([^\]]*)\)\s*\]")
    matches: list[re.Match[str]] = []
    for match in generic_cfg.finditer(clean):
        body = match.group(1).strip()
        if not re.search(r"\btest\b", body):
            continue
        if body != "test":
            raise SystemExit(
                f"unsupported test-related cfg expression in {label}: {body!r}"
            )
        matches.append(match)

    line_starts = [0, *[index + 1 for index, char in enumerate(source) if char == "\n"]]
    ranges: list[tuple[int, int]] = []
    for match in matches:
        index = match.end()
        parentheses = 0
        brackets = 0
        block_depth = 0
        in_block = False
        end: int | None = None
        while index < len(clean):
            char = clean[index]
            if in_block:
                if char == "{":
                    block_depth += 1
                elif char == "}":
                    block_depth -= 1
                    if block_depth == 0:
                        end = index + 1
                        break
            elif char == "(":
                parentheses += 1
            elif char == ")":
                parentheses = max(0, parentheses - 1)
            elif char == "[":
                brackets += 1
            elif char == "]":
                brackets = max(0, brackets - 1)
            elif char == "{" and parentheses == 0 and brackets == 0:
                in_block = True
                block_depth = 1
            elif char in ";," and parentheses == 0 and brackets == 0:
                end = index + 1
                break
            index += 1
        if end is None:
            start_line = bisect.bisect_right(line_starts, match.start())
            raise SystemExit(f"unterminated #[cfg(test)] item in {label}:{start_line}")
        ranges.append(
            (
                bisect.bisect_right(line_starts, match.start()),
                bisect.bisect_right(line_starts, end - 1),
            )
        )

    merged: list[tuple[int, int]] = []
    for start, end in sorted(ranges):
        if merged and start <= merged[-1][1] + 1:
            merged[-1] = (merged[-1][0], max(merged[-1][1], end))
        else:
            merged.append((start, end))
    return merged


def load_project_source_entries(
    repo_root: Path, summary_json_path: Path, lcov_path: Path
) -> list[FileCoverage]:
    raw = json.loads(summary_json_path.read_text(encoding="utf-8"))
    summaries = {
        Path(item["filename"]).resolve(): item["summary"]
        for item in raw["data"][0]["files"]
    }
    entries: list[FileCoverage] = []
    for path, counters in parse_lcov(lcov_path).items():
        try:
            relative = path.relative_to(repo_root.resolve())
        except ValueError:
            continue
        if "src" not in relative.parts:
            continue
        # Rust unit tests sometimes need private access and therefore live in
        # a `src/test_*.rs` child module. Keep those test bodies out of the
        # production-source denominator just like top-level `tests/` files.
        if relative.name.startswith("test_"):
            continue
        summary = summaries.get(path)
        if summary is None:
            raise SystemExit(f"LCOV source is missing from LLVM JSON summary: {relative}")
        excluded_lines: set[int] = set()
        source = path.read_text(encoding="utf-8")
        for start, end in rust_cfg_test_line_ranges(
            source, label=relative.as_posix()
        ):
            excluded_lines.update(range(start, end + 1))
        included = {
            line: count for line, count in counters.items() if line not in excluded_lines
        }
        relative_path = relative.as_posix()
        crate = relative_path.split("/", 1)[0]
        entries.append(
            FileCoverage(
                path=path,
                relative_path=relative_path,
                crate=crate,
                lines=MetricSummary(
                    count=len(included),
                    covered=sum(count > 0 for count in included.values()),
                ),
                functions=MetricSummary(
                    count=summary["functions"]["count"],
                    covered=summary["functions"]["covered"],
                ),
                regions=MetricSummary(
                    count=summary["regions"]["count"],
                    covered=summary["regions"]["covered"],
                ),
            )
        )
    if not entries:
        raise SystemExit("no workspace source entries found in llvm-cov summary")
    return sorted(entries, key=lambda entry: entry.relative_path)


def validate_workspace_source_completeness(
    repo_root: Path, entries: list[FileCoverage]
) -> None:
    declared: set[str] = set()
    for manifest in repo_root.glob("*/Cargo.toml"):
        source_root = manifest.parent / "src"
        if not source_root.is_dir():
            continue
        for path in source_root.rglob("*.rs"):
            if not path.name.startswith("test_"):
                declared.add(path.relative_to(repo_root).as_posix())

    unknown_allowlist = sorted(DECLARATION_ONLY_SOURCE_PATHS - declared)
    if unknown_allowlist:
        raise SystemExit(
            "declaration-only source allowlist contains missing paths: "
            + ", ".join(unknown_allowlist)
        )
    measured = {entry.relative_path for entry in entries}
    unexpectedly_mapped = sorted(DECLARATION_ONLY_SOURCE_PATHS.intersection(measured))
    if unexpectedly_mapped:
        raise SystemExit(
            "declaration-only files now have executable mappings; update the allowlist: "
            + ", ".join(unexpectedly_mapped)
        )
    missing = sorted(declared - DECLARATION_ONLY_SOURCE_PATHS - measured)
    if missing:
        raise SystemExit(
            "instrumented export omitted workspace source files: " + ", ".join(missing)
        )


def sum_metric(entries: Iterable[FileCoverage], attr: str) -> MetricSummary:
    total = 0
    covered = 0
    for entry in entries:
        metric = getattr(entry, attr)
        total += metric.count
        covered += metric.covered
    return MetricSummary(count=total, covered=covered)


def metric_meets_threshold(metric: MetricSummary, threshold: Decimal) -> bool:
    if metric.count == 0:
        return False
    return Decimal(metric.covered) * Decimal(100) >= threshold * Decimal(metric.count)


def line_gate_failures(
    entries: list[FileCoverage],
    aggregate_threshold: Decimal | None,
    file_threshold: Decimal | None,
) -> list[str]:
    failures: list[str] = []
    if aggregate_threshold is not None:
        total = sum_metric(entries, "lines")
        if not metric_meets_threshold(total, aggregate_threshold):
            failures.append(
                "aggregate source lines "
                f"({total.covered}/{total.count}, {total.percent:.4f}%) are below "
                f"{aggregate_threshold}%"
            )

    if file_threshold is not None:
        for entry in entries:
            if not metric_meets_threshold(entry.lines, file_threshold):
                failures.append(
                    f"{entry.relative_path} ({entry.lines.covered}/{entry.lines.count}, "
                    f"{entry.lines.percent:.4f}%) is below {file_threshold}%"
                )
    return failures


def display_path(path: Path, base: Path) -> str:
    try:
        relative = path.relative_to(base)
    except ValueError:
        return str(path)
    return relative.as_posix() or "."


def prepare_build_directory(
    build_dir: Path,
    *,
    repo_root: Path,
    output_dir: Path,
    reuse: bool,
) -> None:
    if build_dir == repo_root or build_dir in repo_root.parents:
        raise SystemExit(
            "refusing to use repository root or an ancestor as build dir: "
            f"{build_dir}"
        )
    repo_target = repo_root / "target"
    if repo_root in build_dir.parents and repo_target not in build_dir.parents:
        raise SystemExit(
            "a coverage build directory inside the workspace must be below target/: "
            f"{build_dir}"
        )
    if build_dir == repo_target:
        raise SystemExit("refusing to clean the workspace's entire target directory")
    if build_dir.exists() and not build_dir.is_dir():
        raise SystemExit(f"coverage build path is not a directory: {build_dir}")
    if not reuse and (output_dir == build_dir or build_dir in output_dir.parents):
        raise SystemExit(
            "output directory must not be inside a build directory that will be cleaned"
        )
    sentinel = build_dir / BUILD_SENTINEL
    existing_nonempty = build_dir.exists() and any(build_dir.iterdir())
    if existing_nonempty and not sentinel.is_file():
        raise SystemExit(
            "refusing to use a nonempty unrecognized coverage build directory: "
            f"{build_dir}"
        )
    if build_dir.exists() and not reuse:
        shutil.rmtree(build_dir)
    build_dir.mkdir(parents=True, exist_ok=True)
    sentinel.write_text("Arduino Simulator coverage build\n", encoding="utf-8")


def build_crate_rollups(entries: list[FileCoverage]) -> list[dict[str, object]]:
    grouped: dict[str, list[FileCoverage]] = {}
    for entry in entries:
        grouped.setdefault(entry.crate, []).append(entry)

    rollups: list[dict[str, object]] = []
    for crate, files in grouped.items():
        line_metric = sum_metric(files, "lines")
        function_metric = sum_metric(files, "functions")
        region_metric = sum_metric(files, "regions")
        rollups.append(
            {
                "crate": crate,
                "lines": line_metric,
                "functions": function_metric,
                "regions": region_metric,
            }
        )
    rollups.sort(key=lambda item: item["lines"].percent, reverse=True)
    return rollups


def write_tsv(entries: list[FileCoverage], tsv_path: Path) -> None:
    lines = [
        "\t".join(
            [
                entry.relative_path,
                str(entry.lines.count),
                str(entry.lines.covered),
                f"{entry.lines.percent:.2f}",
                str(entry.functions.count),
                str(entry.functions.covered),
                f"{entry.functions.percent:.2f}",
                str(entry.regions.count),
                str(entry.regions.covered),
                f"{entry.regions.percent:.2f}",
            ]
        )
        for entry in entries
    ]
    tsv_path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def html_table_row(cells: list[str], *, numeric: set[int] | None = None) -> str:
    numeric = numeric or set()
    rendered: list[str] = []
    for index, cell in enumerate(cells):
        css_class = ' class="num"' if index in numeric else ""
        rendered.append(f"<td{css_class}>{cell}</td>")
    return "<tr>" + "".join(rendered) + "</tr>"


def build_html(
    generated_at: dt.datetime,
    repo_root: Path,
    output_dir: Path,
    entries: list[FileCoverage],
    crate_rollups: list[dict[str, object]],
) -> str:
    workspace_display = display_path(repo_root, repo_root)
    output_display = display_path(output_dir, repo_root)
    total_lines = sum_metric(entries, "lines")
    total_functions = sum_metric(entries, "functions")
    total_regions = sum_metric(entries, "regions")

    hotspot_entries = sorted(entries, key=lambda entry: entry.lines.missed, reverse=True)[:10]
    low_entries = sorted(entries, key=lambda entry: entry.lines.percent)[:10]
    high_entries = [
        entry for entry in sorted(entries, key=lambda entry: entry.lines.percent, reverse=True)
        if entry.lines.percent >= 90.0
    ]

    crate_rows = "\n".join(
        html_table_row(
            [
                f'<span class="mono">{html.escape(item["crate"])}</span>',
                f"{item['lines'].percent:.2f}%",
                f"{item['lines'].covered:,} / {item['lines'].count:,}",
                f"{item['functions'].percent:.2f}%",
                f"{item['functions'].covered:,} / {item['functions'].count:,}",
                f"{item['regions'].percent:.2f}%",
            ],
            numeric={1, 2, 3, 4, 5},
        )
        for item in crate_rollups
    )

    def file_rows(items: Iterable[FileCoverage], *, include_total: bool) -> str:
        rows: list[str] = []
        for entry in items:
            cells = [
                f'<span class="mono">{html.escape(entry.relative_path)}</span>',
                f"{entry.lines.percent:.2f}%",
                f"{entry.lines.missed:,}",
            ]
            numeric = {1, 2}
            if include_total:
                cells.append(f"{entry.lines.covered:,} / {entry.lines.count:,}")
                numeric.add(3)
            rows.append(html_table_row(cells, numeric=numeric))
        return "\n".join(rows)

    appendix_rows = "\n".join(
        html_table_row(
            [
                f'<span class="mono">{html.escape(entry.relative_path)}</span>',
                f"{entry.lines.percent:.2f}%",
                f"{entry.lines.missed:,}",
                f"{entry.lines.covered:,} / {entry.lines.count:,}",
            ],
            numeric={1, 2, 3},
        )
        for entry in entries
    )

    best_crates = ", ".join(
        f"<span class=\"mono\">{html.escape(item['crate'])}</span> ({item['lines'].percent:.2f}%)"
        for item in crate_rollups[:3]
    )
    worst_file = low_entries[0]

    return f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8" />
<title>Arduino Simulator Coverage Summary</title>
<style>
  :root {{
    --paper: #fffdf8;
    --ink: #1f2430;
    --muted: #5b6475;
    --accent: #0f766e;
    --accent-soft: #dff5f2;
    --line: #ddd6c8;
  }}
  body {{
    margin: 0;
    padding: 32px;
    background: linear-gradient(180deg, #f4efe6 0%, #f8f6f0 100%);
    color: var(--ink);
    font-family: "Avenir Next", "Helvetica Neue", Helvetica, Arial, sans-serif;
    line-height: 1.45;
  }}
  .page {{
    max-width: 1080px;
    margin: 0 auto;
    background: var(--paper);
    border: 1px solid var(--line);
    box-shadow: 0 10px 40px rgba(31, 36, 48, 0.08);
    padding: 36px 40px 44px;
  }}
  h1, h2 {{
    margin: 0 0 12px;
    line-height: 1.15;
  }}
  h1 {{
    font-size: 30px;
    letter-spacing: -0.02em;
  }}
  h2 {{
    margin-top: 28px;
    font-size: 20px;
    border-top: 2px solid var(--line);
    padding-top: 18px;
  }}
  p, li {{
    font-size: 13px;
  }}
  .lede {{
    color: var(--muted);
    max-width: 860px;
    margin-bottom: 20px;
  }}
  .meta {{
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 12px;
    margin: 22px 0 8px;
  }}
  .card {{
    border: 1px solid var(--line);
    background: #fff;
    padding: 14px 16px;
    border-radius: 10px;
  }}
  .card .label {{
    display: block;
    color: var(--muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin-bottom: 6px;
  }}
  .card .value {{
    font-size: 24px;
    font-weight: 700;
  }}
  .small {{
    color: var(--muted);
    font-size: 12px;
  }}
  .summary-box {{
    border-left: 4px solid var(--accent);
    background: var(--accent-soft);
    padding: 14px 16px;
    margin-top: 16px;
  }}
  table {{
    width: 100%;
    border-collapse: collapse;
    margin-top: 12px;
    font-size: 12px;
  }}
  th, td {{
    border-bottom: 1px solid var(--line);
    padding: 8px 10px;
    text-align: left;
    vertical-align: top;
  }}
  th {{
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
    background: #fcfaf4;
  }}
  td.num, th.num {{
    text-align: right;
    white-space: nowrap;
  }}
  .mono {{
    font-family: Menlo, Monaco, Consolas, monospace;
    font-size: 11px;
  }}
  ul {{
    margin: 8px 0 0 18px;
    padding: 0;
  }}
  .footer {{
    margin-top: 22px;
    color: var(--muted);
    font-size: 11px;
  }}
</style>
</head>
<body>
<div class="page">
  <h1>Arduino Simulator Coverage Summary</h1>
  <p class="lede">
    Workspace source coverage summary generated on {generated_at:%B %d, %Y %H:%M:%S}
    for workspace root <span class="mono">{html.escape(workspace_display)}</span>.
    This report covers project source files under <span class="mono">*/src/*.rs</span>,
    excluding standalone tests, <span class="mono">src/test_*.rs</span> child modules,
    and complete items gated by exact <span class="mono">#[cfg(test)]</span> attributes.
  </p>

  <div class="meta">
    <div class="card">
      <span class="label">Source Files</span>
      <span class="value">{len(entries)}</span>
      <span class="small">Project source files in the rollup</span>
    </div>
    <div class="card">
      <span class="label">Output Dir</span>
      <span class="value mono" style="font-size:13px">{html.escape(output_display)}</span>
      <span class="small">HTML plus raw JSON, physical-line LCOV, and TSV</span>
    </div>
    <div class="card">
      <span class="label">Coverage Command</span>
      <span class="value mono" style="font-size:13px">cargo test --workspace</span>
      <span class="small">With <span class="mono">-Cinstrument-coverage</span></span>
    </div>
    <div class="card">
      <span class="label">Scope</span>
      <span class="value mono" style="font-size:13px">physical source lines</span>
      <span class="small">Line totals exclude test bodies; LLVM function/region diagnostics do not</span>
    </div>
  </div>

  <h2>Topline</h2>
  <div class="summary-box">
    <strong>Overall source line coverage is {total_lines.percent:.2f}%</strong> with
    <span class="mono">{total_lines.covered:,} / {total_lines.count:,}</span> lines covered.
    Raw LLVM function coverage is <span class="mono">{total_functions.percent:.2f}%</span>
    and region coverage is <span class="mono">{total_regions.percent:.2f}%</span>;
    those two diagnostic metrics can include inline test items and do not gate line coverage.
  </div>
  <table>
    <thead>
      <tr>
        <th>Metric</th>
        <th class="num">Covered</th>
        <th class="num">Total</th>
        <th class="num">Percent</th>
      </tr>
    </thead>
    <tbody>
      {html_table_row(["Lines", f"{total_lines.covered:,}", f"{total_lines.count:,}", f"{total_lines.percent:.2f}%"], numeric={1, 2, 3})}
      {html_table_row(["LLVM functions (diagnostic)", f"{total_functions.covered:,}", f"{total_functions.count:,}", f"{total_functions.percent:.2f}%"], numeric={1, 2, 3})}
      {html_table_row(["LLVM regions (diagnostic)", f"{total_regions.covered:,}", f"{total_regions.count:,}", f"{total_regions.percent:.2f}%"], numeric={1, 2, 3})}
    </tbody>
  </table>

  <h2>What Stands Out</h2>
  <ul>
    <li><strong>Best-covered crates:</strong> {best_crates}.</li>
    <li><strong>Main uncovered concentration:</strong> the biggest raw line-count gaps are still in <span class="mono">rust_gui</span> and <span class="mono">rust_runtime</span>.</li>
    <li><strong>CPU status:</strong> <span class="mono">rust_cpu</span> sits at {next(item['lines'].percent for item in crate_rollups if item['crate'] == 'rust_cpu'):.2f}% line coverage, with most of the remaining gap concentrated in <span class="mono">rust_cpu/src/cpu.rs</span>.</li>
    <li><strong>Most concerning single file:</strong> <span class="mono">{html.escape(worst_file.relative_path)}</span> is at {worst_file.lines.percent:.2f}%.</li>
  </ul>

  <h2>Coverage By Crate</h2>
  <table>
    <thead>
      <tr>
        <th>Crate</th>
        <th class="num">Line %</th>
        <th class="num">Covered / Total Lines</th>
        <th class="num">Function %</th>
        <th class="num">Covered / Total Functions</th>
        <th class="num">Region %</th>
      </tr>
    </thead>
    <tbody>
      {crate_rows}
    </tbody>
  </table>

  <h2>Biggest Hotspots By Missed Lines</h2>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th class="num">Coverage</th>
        <th class="num">Missed Lines</th>
        <th class="num">Covered / Total</th>
      </tr>
    </thead>
    <tbody>
      {file_rows(hotspot_entries, include_total=True)}
    </tbody>
  </table>

  <h2>Lowest-Coverage Files</h2>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th class="num">Coverage</th>
        <th class="num">Missed Lines</th>
      </tr>
    </thead>
    <tbody>
      {file_rows(low_entries, include_total=False)}
    </tbody>
  </table>

  <h2>Strongest Files (&gt;= 90% line coverage)</h2>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th class="num">Coverage</th>
        <th class="num">Missed Lines</th>
      </tr>
    </thead>
    <tbody>
      {file_rows(high_entries, include_total=False)}
    </tbody>
  </table>

  <h2>Recommended Next Targets</h2>
  <ul>
    <li><strong>GUI depth:</strong> <span class="mono">rust_gui/src/app.rs</span> and <span class="mono">rust_gui/src/board_editor.rs</span> offer the biggest raw line-count wins.</li>
    <li><strong>Runtime confidence:</strong> <span class="mono">rust_runtime/src/tui.rs</span> and <span class="mono">rust_runtime/src/firmware.rs</span> are still lightly exercised.</li>
    <li><strong>Error handling:</strong> <span class="mono">rust_project/src/error.rs</span> is a clean target for compact formatting and variant tests.</li>
    <li><strong>CPU depth:</strong> <span class="mono">rust_cpu/src/cpu.rs</span> remains one of the largest single hotspots once the GUI and runtime gaps are reduced.</li>
  </ul>

  <h2>Per-File Appendix</h2>
  <table>
    <thead>
      <tr>
        <th>File</th>
        <th class="num">Line %</th>
        <th class="num">Missed Lines</th>
        <th class="num">Covered / Total</th>
      </tr>
    </thead>
    <tbody>
      {appendix_rows}
    </tbody>
  </table>

  <p class="footer">
    Generated from the active Rust toolchain's
    <span class="mono">llvm-cov export -summary-only</span>.
    Raw artifacts are written alongside this page as JSON and TSV for deeper inspection.
  </p>
</div>
</body>
</html>
"""


def main() -> int:
    args = parse_args()

    repo_root = Path(__file__).resolve().parent.parent
    cargo = require_program(os.environ.get("CARGO", "cargo"))
    rustc = require_program(os.environ.get("RUSTC", "rustc"))
    llvm_tools = locate_rust_llvm_tools(rustc, cwd=repo_root)

    output_dir = args.output_dir.resolve()
    build_dir = args.build_dir.resolve()
    prepare_build_directory(
        build_dir,
        repo_root=repo_root,
        output_dir=output_dir,
        reuse=args.reuse_build,
    )
    profiles_root = build_dir / "profiles"

    output_dir.mkdir(parents=True, exist_ok=True)
    profiles_root.mkdir(parents=True, exist_ok=True)
    print(
        "LLVM tools: "
        f"{display_path(llvm_tools.llvm_cov, repo_root)} "
        f"({llvm_tools.source})"
    )

    with tempfile.TemporaryDirectory(dir=profiles_root, prefix="run-") as profile_dir_raw:
        profile_dir = Path(profile_dir_raw)
        env = os.environ.copy()
        env["CARGO_INCREMENTAL"] = "0"
        existing_flags = env.get("RUSTFLAGS", "").strip()
        coverage_flags = "-Cinstrument-coverage -Ccodegen-units=1"
        env["RUSTFLAGS"] = f"{existing_flags} {coverage_flags}".strip()
        env["CARGO_TARGET_DIR"] = str(build_dir)
        env["LLVM_PROFILE_FILE"] = str(profile_dir / "%m.profraw")

        run_command(
            [str(cargo), "test", "--workspace", "--", "--test-threads=1"],
            cwd=repo_root,
            env=env,
        )

        profraw_files = sorted(profile_dir.glob("*.profraw"))
        if not profraw_files:
            raise SystemExit(f"no profraw files generated under {profile_dir}")

        profdata_path = output_dir / "workspace_coverage.profdata"
        merge_command = [
            str(llvm_tools.llvm_profdata),
            "merge",
            "-sparse",
            *[str(path) for path in profraw_files],
            "-o",
            str(profdata_path),
        ]
        run_command(merge_command, cwd=repo_root)

    objects = collect_objects(build_dir)

    summary_json_path = output_dir / "workspace_coverage_summary.json"
    export_summary_json(
        repo_root,
        objects,
        profdata_path,
        summary_json_path,
        llvm_tools.llvm_cov,
    )
    lcov_path = output_dir / "workspace_coverage.lcov"
    export_lcov(
        repo_root,
        objects,
        profdata_path,
        lcov_path,
        llvm_tools.llvm_cov,
    )

    entries = load_project_source_entries(repo_root, summary_json_path, lcov_path)
    validate_workspace_source_completeness(repo_root, entries)
    crate_rollups = build_crate_rollups(entries)

    tsv_path = output_dir / "workspace_source_coverage.tsv"
    write_tsv(entries, tsv_path)

    generated_at = dt.datetime.now()
    html_path = output_dir / "workspace_coverage_summary.html"
    html_path.write_text(
        build_html(generated_at, repo_root, output_dir, entries, crate_rollups),
        encoding="utf-8",
    )

    print()
    print(f"HTML summary: {display_path(html_path, repo_root)}")
    print(f"Raw summary JSON: {display_path(summary_json_path, repo_root)}")
    print(f"Physical line LCOV: {display_path(lcov_path, repo_root)}")
    print(f"Filtered TSV: {display_path(tsv_path, repo_root)}")

    failures = line_gate_failures(
        entries,
        args.fail_under_lines,
        args.fail_under_file_lines,
    )
    if failures:
        print("Coverage gate failed:")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    if args.fail_under_lines is not None or args.fail_under_file_lines is not None:
        print("Coverage gates passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
