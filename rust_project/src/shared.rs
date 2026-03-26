use std::collections::BTreeSet;
use std::path::PathBuf;

use rust_board::{built_in_board_model_names, load_built_in_board_model};
use serde::{Deserialize, Serialize};

use crate::error::ProjectError;

pub const PROJECT_FORMAT_VERSION: &str = "0.3.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostBoard {
    NanoV3,
    Mega2560Rev3,
}

impl HostBoard {
    pub const ALL: [HostBoard; 2] = [HostBoard::Mega2560Rev3, HostBoard::NanoV3];

    pub fn label(self) -> &'static str {
        match self {
            Self::NanoV3 => "Arduino Nano (ATmega328P)",
            Self::Mega2560Rev3 => "Arduino Mega 2560",
        }
    }

    pub fn fqbn(self) -> &'static str {
        match self {
            Self::NanoV3 => "arduino:avr:nano:cpu=atmega328old",
            Self::Mega2560Rev3 => "arduino:avr:mega",
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            Self::NanoV3 => "nano",
            Self::Mega2560Rev3 => "mega",
        }
    }

    pub fn builtin_board_model(self) -> &'static str {
        match self {
            Self::NanoV3 => "arduino_nano_v3",
            Self::Mega2560Rev3 => "arduino_mega_2560_rev3",
        }
    }
}

impl Default for HostBoard {
    fn default() -> Self {
        Self::Mega2560Rev3
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FirmwareSourceKind {
    Ino,
    Hex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirmwareSource {
    pub kind: FirmwareSourceKind,
    pub path: PathBuf,
    pub compiled_hex_path: Option<PathBuf>,
}

impl FirmwareSource {
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.path.as_os_str().is_empty() {
            return Err(ProjectError::MissingFirmwarePath);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcbSource {
    pub path: PathBuf,
    pub board_name_hint: Option<String>,
}

impl PcbSource {
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.path.as_os_str().is_empty() {
            return Err(ProjectError::MissingPcbPath);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingMode {
    Auto,
    Digital,
    Analog,
    Power,
    Bus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalBinding {
    pub board_signal: String,
    pub pcb_net: String,
    pub mode: BindingMode,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleSignalBinding {
    pub module_signal: String,
    pub pcb_net: String,
    pub mode: BindingMode,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleOverlay {
    pub name: String,
    pub model: String,
    #[serde(default)]
    pub bindings: Vec<ModuleSignalBinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeKind {
    Digital,
    Analog,
    Serial,
    Bus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeSpec {
    pub name: String,
    pub pcb_net: String,
    pub kind: ProbeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StimulusKind {
    DigitalHigh,
    DigitalLow,
    AnalogVoltage,
    PulseTrain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StimulusSpec {
    pub name: String,
    pub pcb_net: String,
    pub kind: StimulusKind,
    pub value: Option<f64>,
}

pub fn validate_host_board_signal_bindings(
    host_board: HostBoard,
    bindings: &[SignalBinding],
) -> Result<(), ProjectError> {
    let host_board = load_built_in_board_model(host_board.builtin_board_model()).map_err(|_| {
        ProjectError::UnknownBoardModel(host_board.builtin_board_model().to_string())
    })?;
    let known_signals: BTreeSet<String> = host_board.nets.into_iter().map(|net| net.name).collect();
    let mut seen = BTreeSet::new();

    for binding in bindings {
        if binding.pcb_net.trim().is_empty() {
            return Err(ProjectError::EmptyPcbNet(binding.board_signal.clone()));
        }
        if !known_signals.contains(&binding.board_signal) {
            return Err(ProjectError::UnknownBoardSignal(
                binding.board_signal.clone(),
            ));
        }
        if !seen.insert(binding.board_signal.clone()) {
            return Err(ProjectError::DuplicateBoardSignal(
                binding.board_signal.clone(),
            ));
        }
    }

    Ok(())
}

pub fn validate_module_overlays(overlays: &[ModuleOverlay]) -> Result<(), ProjectError> {
    let available_models = built_in_board_model_names()
        .into_iter()
        .filter(|model| !model.starts_with("arduino_"))
        .collect::<BTreeSet<_>>();
    let mut seen_names = BTreeSet::new();

    for overlay in overlays {
        if overlay.name.trim().is_empty() {
            return Err(ProjectError::EmptyName("module overlay"));
        }
        if !seen_names.insert(overlay.name.clone()) {
            return Err(ProjectError::DuplicateModuleOverlay(overlay.name.clone()));
        }
        if !available_models.contains(overlay.model.as_str()) {
            return Err(ProjectError::UnknownModuleModel(overlay.model.clone()));
        }

        let mut seen_signals = BTreeSet::new();
        for binding in &overlay.bindings {
            if binding.module_signal.trim().is_empty() {
                return Err(ProjectError::EmptyModuleSignal(overlay.name.clone()));
            }
            if binding.pcb_net.trim().is_empty() {
                return Err(ProjectError::EmptyPcbNet(binding.module_signal.clone()));
            }
            if !seen_signals.insert(binding.module_signal.clone()) {
                return Err(ProjectError::DuplicateModuleSignal {
                    module_name: overlay.name.clone(),
                    signal: binding.module_signal.clone(),
                });
            }
        }
    }

    Ok(())
}
