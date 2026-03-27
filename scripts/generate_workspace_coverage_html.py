#!/usr/bin/env python3
"""Generate an HTML coverage summary for the Arduino Simulator workspace."""

from __future__ import annotations

import argparse
import datetime as dt
import html
import json
import os
from dataclasses import dataclass
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
from typing import Iterable


WORKSPACE_OBJECT_PREFIXES = (
    "librust_",
    "rust_behavior-",
    "rust_board-",
    "rust_cpu-",
    "rust_gui-",
    "rust_mcu-",
    "rust_project-",
    "rust_runtime-",
    "avr_",
    "cpu_",
    "runtime_bus_tests-",
    "cli_runtime-",
    "arduino_simulator",
    "arduino_simulator-",
    "arduino_simulator_gui",
    "arduino_simulator_gui-",
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


def parse_args() -> argparse.Namespace:
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
        help=f"Directory for generated HTML and data files (default: {default_output_dir})",
    )
    parser.add_argument(
        "--build-dir",
        type=Path,
        default=default_build_dir,
        help=f"Instrumented cargo target directory (default: {default_build_dir})",
    )
    return parser.parse_args()


def ensure_tool(name: str) -> None:
    if not shutil_which(name):
        raise SystemExit(f"missing required tool: {name}")


def shutil_which(name: str) -> str | None:
    return subprocess.run(
        ["which", name],
        check=False,
        capture_output=True,
        text=True,
    ).stdout.strip() or None


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
) -> None:
    command = [
        "xcrun",
        "llvm-cov",
        "export",
        "-summary-only",
        f"-instr-profile={profdata_path}",
    ]
    command.extend(f"--object={path}" for path in objects)
    result = run_command(command, cwd=repo_root, capture_output=True)
    summary_json_path.write_text(result.stdout, encoding="utf-8")


def load_project_source_entries(repo_root: Path, summary_json_path: Path) -> list[FileCoverage]:
    raw = json.loads(summary_json_path.read_text(encoding="utf-8"))
    files = raw["data"][0]["files"]
    repo_prefix = f"{repo_root}{os.sep}"
    entries: list[FileCoverage] = []
    for item in files:
        filename = item["filename"]
        if not filename.startswith(repo_prefix):
            continue
        if "/src/" not in filename:
            continue
        path = Path(filename)
        relative_path = path.relative_to(repo_root).as_posix()
        crate = relative_path.split("/", 1)[0]
        summary = item["summary"]
        entries.append(
            FileCoverage(
                path=path,
                relative_path=relative_path,
                crate=crate,
                lines=MetricSummary(
                    count=summary["lines"]["count"],
                    covered=summary["lines"]["covered"],
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


def sum_metric(entries: Iterable[FileCoverage], attr: str) -> MetricSummary:
    total = 0
    covered = 0
    for entry in entries:
        metric = getattr(entry, attr)
        total += metric.count
        covered += metric.covered
    return MetricSummary(count=total, covered=covered)


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
    for <span class="mono">{html.escape(str(repo_root))}</span>.
    This report covers project source files under <span class="mono">*/src/*.rs</span>.
  </p>

  <div class="meta">
    <div class="card">
      <span class="label">Source Files</span>
      <span class="value">{len(entries)}</span>
      <span class="small">Project source files in the rollup</span>
    </div>
    <div class="card">
      <span class="label">Output Dir</span>
      <span class="value mono" style="font-size:13px">{html.escape(str(output_dir))}</span>
      <span class="small">HTML plus raw JSON and TSV</span>
    </div>
    <div class="card">
      <span class="label">Coverage Command</span>
      <span class="value mono" style="font-size:13px">cargo test --workspace</span>
      <span class="small">With <span class="mono">-Cinstrument-coverage</span></span>
    </div>
    <div class="card">
      <span class="label">Scope</span>
      <span class="value mono" style="font-size:13px">source only</span>
      <span class="small">Tests are excluded from percentages</span>
    </div>
  </div>

  <h2>Topline</h2>
  <div class="summary-box">
    <strong>Overall source line coverage is {total_lines.percent:.2f}%</strong> with
    <span class="mono">{total_lines.covered:,} / {total_lines.count:,}</span> lines covered.
    Function coverage is <span class="mono">{total_functions.percent:.2f}%</span> and
    region coverage is <span class="mono">{total_regions.percent:.2f}%</span>.
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
      {html_table_row(["Functions", f"{total_functions.covered:,}", f"{total_functions.count:,}", f"{total_functions.percent:.2f}%"], numeric={1, 2, 3})}
      {html_table_row(["Regions", f"{total_regions.covered:,}", f"{total_regions.count:,}", f"{total_regions.percent:.2f}%"], numeric={1, 2, 3})}
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
    Generated from <span class="mono">xcrun llvm-cov export -summary-only</span>.
    Raw artifacts are written alongside this page as JSON and TSV for deeper inspection.
  </p>
</div>
</body>
</html>
"""


def main() -> int:
    args = parse_args()

    ensure_tool("cargo")
    ensure_tool("xcrun")

    repo_root = Path(__file__).resolve().parent.parent
    output_dir = args.output_dir.resolve()
    build_dir = args.build_dir.resolve()
    profiles_root = build_dir / "profiles"

    output_dir.mkdir(parents=True, exist_ok=True)
    profiles_root.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(dir=profiles_root, prefix="run-") as profile_dir_raw:
        profile_dir = Path(profile_dir_raw)
        env = os.environ.copy()
        env["CARGO_INCREMENTAL"] = "0"
        env["RUSTFLAGS"] = "-Cinstrument-coverage"
        env["CARGO_TARGET_DIR"] = str(build_dir)
        env["LLVM_PROFILE_FILE"] = str(profile_dir / "%m.profraw")

        run_command(
            ["cargo", "test", "--workspace", "--", "--test-threads=1"],
            cwd=repo_root,
            env=env,
        )

        profraw_files = sorted(profile_dir.glob("*.profraw"))
        if not profraw_files:
            raise SystemExit(f"no profraw files generated under {profile_dir}")

        profdata_path = output_dir / "workspace_coverage.profdata"
        merge_command = [
            "xcrun",
            "llvm-profdata",
            "merge",
            "-sparse",
            *[str(path) for path in profraw_files],
            "-o",
            str(profdata_path),
        ]
        run_command(merge_command, cwd=repo_root)

    objects = collect_objects(build_dir)

    summary_json_path = output_dir / "workspace_coverage_summary.json"
    export_summary_json(repo_root, objects, profdata_path, summary_json_path)

    entries = load_project_source_entries(repo_root, summary_json_path)
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
    print(f"HTML summary: {html_path}")
    print(f"Raw summary JSON: {summary_json_path}")
    print(f"Filtered TSV: {tsv_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
