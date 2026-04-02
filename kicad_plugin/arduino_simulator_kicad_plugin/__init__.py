from __future__ import annotations

try:
    from .plugin import ArduinoSimulatorPlugin
except Exception:
    ArduinoSimulatorPlugin = None
else:
    if __name__ != "__main__":
        ArduinoSimulatorPlugin().register()
