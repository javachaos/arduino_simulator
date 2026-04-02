from __future__ import annotations

from pathlib import Path
import shlex
import textwrap

import pcbnew
import wx

from .core import (
    build_create_command,
    build_open_pcb_command,
    build_sync_command,
    default_project_path,
    discover_firmware_candidates,
    read_project_firmware_path,
    resolve_runtime_environment,
    run_adapter,
)


TITLE = "Arduino Simulator"


class ArduinoSimulatorPlugin(pcbnew.ActionPlugin):
    def defaults(self) -> None:
        self.name = "Open Arduino Simulator"
        self.category = "Simulation"
        self.description = (
            "Open the active KiCad PCB in generic mode or create and refresh "
            "an arduino_simulator project with firmware bindings."
        )
        self.show_toolbar_button = False

    def Run(self) -> None:
        board = pcbnew.GetBoard()
        pcb_path = Path(board.GetFileName())
        if not pcb_path.is_file():
            self._show_error(
                "Save the PCB to disk before opening the Arduino simulator plugin."
            )
            return

        runtime_environment = resolve_runtime_environment(__file__)
        if not self._runtime_environment_is_usable(runtime_environment):
            self._show_error(
                "Could not locate a bundled Arduino Simulator adapter binary or a "
                "local arduino_simulator workspace from this plugin install.\n\n"
                "If you are running from the source repository, build the adapter with:\n"
                "cargo build -p rust_kicad\n\n"
                "If you are running from a packaged plugin, reinstall the bundle so "
                "the platform binaries are present under plugins/bin."
            )
            return

        if not self._maybe_save_board(board, pcb_path):
            return

        project_path = default_project_path(pcb_path)
        launch_mode = self._choose_launch_mode(project_path.is_file())
        if launch_mode is None:
            return

        if launch_mode == "generic":
            command = build_open_pcb_command(runtime_environment.plugin_root, pcb_path)
            self._run_command(runtime_environment.working_directory, command)
            return

        requested_board = self._choose_board()
        if requested_board is None:
            return

        if project_path.is_file():
            command = build_sync_command(
                runtime_environment.plugin_root,
                project_path,
                requested_board=requested_board,
                launch_gui=True,
            )
            result = self._run_command(runtime_environment.working_directory, command)
            if result is not None:
                firmware_path = read_project_firmware_path(project_path)
                detail = (
                    f"Synced {project_path.name}"
                    + (f" using {firmware_path}" if firmware_path else "")
                    + "."
                )
                self._show_info(detail)
            return

        firmware_path = self._choose_firmware_path(pcb_path.parent)
        if firmware_path is None:
            return

        command = build_create_command(
            runtime_environment.plugin_root,
            pcb_path,
            firmware_path,
            out_path=project_path,
            requested_board=requested_board,
            launch_gui=True,
        )
        result = self._run_command(runtime_environment.working_directory, command)
        if result is not None:
            self._show_info(f"Created {project_path.name} from {firmware_path.name}.")

    def _runtime_environment_is_usable(self, runtime_environment: object) -> bool:
        return bool(getattr(runtime_environment, "bundled", False)) or Path(
            getattr(runtime_environment, "working_directory", "")
        ).joinpath("Cargo.toml").is_file()

    def _maybe_save_board(self, board: object, pcb_path: Path) -> bool:
        is_modified = getattr(board, "IsModified", None)
        if callable(is_modified) and not is_modified():
            return True

        response = wx.MessageBox(
            (
                "The board has unsaved changes.\n\n"
                "Save it before syncing with Arduino Simulator?"
            ),
            TITLE,
            wx.YES_NO | wx.CANCEL | wx.ICON_QUESTION,
        )
        if response == wx.CANCEL:
            return False
        if response == wx.NO:
            return True

        try:
            pcbnew.SaveBoard(str(pcb_path), board)
        except Exception as error:
            self._show_error(f"Failed to save {pcb_path}.\n\n{error}")
            return False

        return True

    def _choose_launch_mode(self, has_project: bool) -> str | None:
        choices = [
            ("Generic PCB mode", "generic"),
            (
                "Arduino simulation project"
                + (" (reuse existing .avrsim project)" if has_project else ""),
                "project",
            ),
        ]
        dialog = wx.SingleChoiceDialog(
            None,
            "Choose how Arduino Simulator should open the active KiCad board.",
            TITLE,
            [label for label, _value in choices],
        )
        dialog.SetSelection(0)
        try:
            if dialog.ShowModal() != wx.ID_OK:
                return None
            return choices[dialog.GetSelection()][1]
        finally:
            dialog.Destroy()

    def _choose_board(self) -> str | None:
        choices = [
            ("Auto-detect from file names", "auto"),
            ("Arduino Mega 2560", "mega"),
            ("Arduino Nano", "nano"),
        ]
        dialog = wx.SingleChoiceDialog(
            None,
            "Select the host board profile to use for KiCad net binding.",
            TITLE,
            [label for label, _value in choices],
        )
        dialog.SetSelection(0)
        try:
            if dialog.ShowModal() != wx.ID_OK:
                return None
            return choices[dialog.GetSelection()][1]
        finally:
            dialog.Destroy()

    def _choose_firmware_path(self, search_root: Path) -> Path | None:
        candidates = discover_firmware_candidates(search_root)
        if candidates:
            labels = [str(path.relative_to(search_root)) for path in candidates]
            labels.extend(
                [
                    "Browse for an .ino or .hex file...",
                    "Choose a sketch directory...",
                ]
            )
            dialog = wx.SingleChoiceDialog(
                None,
                "Choose the firmware source for this KiCad board.",
                TITLE,
                labels,
            )
            dialog.SetSelection(0)
            try:
                if dialog.ShowModal() != wx.ID_OK:
                    return None
                selection = dialog.GetSelection()
            finally:
                dialog.Destroy()

            if selection < len(candidates):
                return candidates[selection]
            if selection == len(candidates):
                return self._browse_firmware_file(search_root)
            return self._browse_sketch_directory(search_root)

        choice = wx.MessageBox(
            (
                "No nearby .ino or .hex firmware candidates were found.\n\n"
                "Choose Yes to browse for a file, No to choose a sketch directory, "
                "or Cancel to stop."
            ),
            TITLE,
            wx.YES_NO | wx.CANCEL | wx.ICON_QUESTION,
        )
        if choice == wx.CANCEL:
            return None
        if choice == wx.YES:
            return self._browse_firmware_file(search_root)
        return self._browse_sketch_directory(search_root)

    def _browse_firmware_file(self, search_root: Path) -> Path | None:
        dialog = wx.FileDialog(
            None,
            "Choose an Arduino sketch or compiled hex file",
            defaultDir=str(search_root),
            wildcard="Arduino firmware (*.ino;*.hex)|*.ino;*.hex",
            style=wx.FD_OPEN | wx.FD_FILE_MUST_EXIST,
        )
        try:
            if dialog.ShowModal() != wx.ID_OK:
                return None
            return Path(dialog.GetPath())
        finally:
            dialog.Destroy()

    def _browse_sketch_directory(self, search_root: Path) -> Path | None:
        dialog = wx.DirDialog(
            None,
            "Choose a sketch directory",
            defaultPath=str(search_root),
            style=wx.DD_DIR_MUST_EXIST,
        )
        try:
            if dialog.ShowModal() != wx.ID_OK:
                return None
            return Path(dialog.GetPath())
        finally:
            dialog.Destroy()

    def _run_command(self, working_directory: Path, command: list[str]) -> object | None:
        try:
            with wx.BusyInfo("Starting Arduino Simulator..."):
                result = run_adapter(command, cwd=working_directory)
        except FileNotFoundError as error:
            self._show_error(
                (
                    "Could not start the Arduino Simulator adapter.\n\n"
                    f"Missing executable: {error.filename or error}\n\n"
                    "If the Rust adapter binary has not been built yet, run:\n"
                    "cargo build -p rust_kicad"
                )
            )
            return None

        if result.returncode == 0:
            return result

        details = "\n\n".join(
            part
            for part in (
                f"Command:\n{shlex.join(command)}",
                f"stdout:\n{result.stdout.strip()}" if result.stdout.strip() else "",
                f"stderr:\n{result.stderr.strip()}" if result.stderr.strip() else "",
            )
            if part
        )
        self._show_error(details)
        return None

    def _show_error(self, message: str) -> None:
        wx.MessageBox(
            textwrap.dedent(message).strip(),
            TITLE,
            wx.OK | wx.ICON_ERROR,
        )

    def _show_info(self, message: str) -> None:
        wx.MessageBox(
            textwrap.dedent(message).strip(),
            TITLE,
            wx.OK | wx.ICON_INFORMATION,
        )
