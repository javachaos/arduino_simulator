from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path
import platform
import shutil
import subprocess
from typing import Iterable


PROJECT_SUFFIX = ".avrsim.json"
ADAPTER_BASENAME = "arduino-simulator-kicad"
GUI_BASENAME = "arduino-simulator-gui"


@dataclass(frozen=True)
class AdapterRunResult:
    returncode: int
    stdout: str
    stderr: str


@dataclass(frozen=True)
class RuntimeEnvironment:
    plugin_root: Path
    working_directory: Path
    bundled: bool


def plugin_root_from_plugin_file(plugin_file: str | Path) -> Path:
    return Path(plugin_file).resolve().parent


def repo_root_from_plugin_file(plugin_file: str | Path) -> Path | None:
    return discover_workspace_root(plugin_root_from_plugin_file(plugin_file))


def discover_workspace_root(start: str | Path) -> Path | None:
    start_path = Path(start).resolve()
    for candidate in [start_path, *start_path.parents]:
        cargo_toml = candidate / 'Cargo.toml'
        if cargo_toml.is_file() and '[workspace]' in cargo_toml.read_text(encoding='utf-8'):
            return candidate
    return None


def resolve_runtime_environment(plugin_file: str | Path) -> RuntimeEnvironment:
    plugin_root = plugin_root_from_plugin_file(plugin_file)
    bundled = bundled_adapter_binary(plugin_root) is not None
    workspace_root = discover_workspace_root(plugin_root)
    working_directory = workspace_root or plugin_root
    return RuntimeEnvironment(
        plugin_root=plugin_root,
        working_directory=working_directory,
        bundled=bundled,
    )


def default_project_path(pcb_path: str | Path) -> Path:
    pcb_path = Path(pcb_path)
    stem = pcb_path.stem or 'simulation'
    return pcb_path.with_name(f'{stem}{PROJECT_SUFFIX}')


def read_project_firmware_path(project_path: str | Path) -> Path | None:
    project_path = Path(project_path)
    if not project_path.is_file():
        return None

    data = json.loads(project_path.read_text(encoding='utf-8'))
    firmware = data.get('firmware')
    if not isinstance(firmware, dict):
        return None

    path = firmware.get('path')
    if not isinstance(path, str) or not path.strip():
        return None

    return Path(path)


def current_platform_tag(system_name: str | None = None, machine_name: str | None = None) -> str:
    system_name = (system_name or platform.system()).lower()
    machine_name = (machine_name or platform.machine()).lower()

    if system_name.startswith('darwin') or system_name == 'macos':
        os_name = 'macos'
    elif system_name.startswith('win'):
        os_name = 'windows'
    else:
        os_name = 'linux'

    if machine_name in {'arm64', 'aarch64'}:
        arch = 'arm64'
    elif machine_name in {'x86_64', 'amd64'}:
        arch = 'x86_64'
    else:
        sanitized = ''.join(
            character if character.isalnum() else '-'
            for character in machine_name
        ).strip('-')
        arch = sanitized or 'unknown'

    return f'{os_name}-{arch}'


def executable_candidates(basename: str) -> tuple[str, ...]:
    if platform.system().lower().startswith('win'):
        return (f'{basename}.exe', basename)
    return (basename, f'{basename}.exe')


def bundled_binary_dir(plugin_root: str | Path) -> Path | None:
    plugin_root = Path(plugin_root).resolve()
    candidates = [
        plugin_root / 'bin' / current_platform_tag(),
        plugin_root / 'bin',
    ]

    for candidate in candidates:
        for executable in executable_candidates(ADAPTER_BASENAME):
            if (candidate / executable).is_file():
                return candidate

    return None


def bundled_adapter_binary(plugin_root: str | Path) -> Path | None:
    binary_dir = bundled_binary_dir(plugin_root)
    if binary_dir is None:
        return None

    for executable in executable_candidates(ADAPTER_BASENAME):
        candidate = binary_dir / executable
        if candidate.is_file():
            return candidate
    return None


def adapter_base_command(base_root: str | Path) -> list[str]:
    base_root = Path(base_root).resolve()

    bundled = bundled_adapter_binary(base_root)
    if bundled is not None:
        return [str(bundled)]

    workspace_root = discover_workspace_root(base_root)
    if workspace_root is not None:
        candidates = [
            workspace_root / 'target' / 'release' / executable
            for executable in executable_candidates(ADAPTER_BASENAME)
        ] + [
            workspace_root / 'target' / 'debug' / executable
            for executable in executable_candidates(ADAPTER_BASENAME)
        ]

        for candidate in candidates:
            if candidate.is_file():
                return [str(candidate)]

        cargo = resolve_cargo_executable()
        return [str(cargo) if cargo else 'cargo', 'run', '-p', 'rust_kicad', '--']

    cargo = resolve_cargo_executable()
    return [str(cargo) if cargo else 'cargo', 'run', '-p', 'rust_kicad', '--']


def resolve_cargo_executable() -> Path | None:
    detected = shutil.which('cargo')
    if detected:
        return Path(detected)

    home = Path.home()
    candidates = [
        home / '.cargo' / 'bin' / 'cargo',
        Path('/opt/homebrew/bin/cargo'),
        Path('/usr/local/bin/cargo'),
    ]

    for candidate in candidates:
        if candidate.is_file():
            return candidate

    return None


def build_create_command(
    base_root: str | Path,
    pcb_path: str | Path,
    firmware_path: str | Path,
    *,
    out_path: str | Path | None = None,
    requested_board: str = 'auto',
    launch_gui: bool = True,
) -> list[str]:
    command = adapter_base_command(base_root)
    command.extend(
        [
            'create-project',
            '--pcb',
            str(Path(pcb_path)),
            '--firmware',
            str(Path(firmware_path)),
        ]
    )
    if out_path is not None:
        command.extend(['--out', str(Path(out_path))])
    if requested_board and requested_board != 'auto':
        command.extend(['--board', requested_board])
    if launch_gui:
        command.append('--launch-gui')
    return command


def build_sync_command(
    base_root: str | Path,
    project_path: str | Path,
    *,
    requested_board: str = 'auto',
    launch_gui: bool = True,
) -> list[str]:
    command = adapter_base_command(base_root)
    command.extend(['sync-project', '--project', str(Path(project_path))])
    if requested_board and requested_board != 'auto':
        command.extend(['--board', requested_board])
    if launch_gui:
        command.append('--launch-gui')
    return command


def build_open_pcb_command(
    base_root: str | Path,
    pcb_path: str | Path,
    *,
    requested_board: str = 'auto',
) -> list[str]:
    command = adapter_base_command(base_root)
    command.extend(['open-pcb', '--pcb', str(Path(pcb_path))])
    if requested_board and requested_board != 'auto':
        command.extend(['--board', requested_board])
    return command


def discover_firmware_candidates(search_root: str | Path, limit: int = 12) -> list[Path]:
    search_root = Path(search_root)
    candidates: list[Path] = []

    for pattern in ('*.ino', '*.hex'):
        for path in sorted(search_root.rglob(pattern)):
            if path.is_file():
                candidates.append(path)
                if len(candidates) >= limit:
                    return candidates

    return candidates


def run_adapter(command: Iterable[str], *, cwd: str | Path) -> AdapterRunResult:
    completed = subprocess.run(
        list(command),
        cwd=str(Path(cwd)),
        capture_output=True,
        text=True,
        check=False,
    )
    return AdapterRunResult(
        returncode=completed.returncode,
        stdout=completed.stdout,
        stderr=completed.stderr,
    )
