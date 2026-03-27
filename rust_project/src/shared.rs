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

#[cfg(test)]
mod tests {
    use super::{
        validate_host_board_signal_bindings, validate_module_overlays, BindingMode, FirmwareSource,
        FirmwareSourceKind, HostBoard, ModuleOverlay, ModuleSignalBinding, PcbSource,
        SignalBinding,
    };
    use crate::error::ProjectError;

    fn known_host_signal(host_board: HostBoard) -> String {
        rust_board::load_built_in_board_model(host_board.builtin_board_model())
            .expect("host board")
            .nets
            .into_iter()
            .next()
            .expect("at least one signal")
            .name
    }

    fn known_module_model() -> String {
        rust_board::built_in_board_model_names()
            .into_iter()
            .find(|name| !name.starts_with("arduino_"))
            .expect("module model")
            .to_string()
    }

    #[test]
    fn host_board_metadata_is_consistent() {
        assert_eq!(HostBoard::ALL, [HostBoard::Mega2560Rev3, HostBoard::NanoV3]);
        assert_eq!(HostBoard::NanoV3.label(), "Arduino Nano (ATmega328P)");
        assert_eq!(
            HostBoard::NanoV3.fqbn(),
            "arduino:avr:nano:cpu=atmega328old"
        );
        assert_eq!(HostBoard::NanoV3.short_name(), "nano");
        assert_eq!(HostBoard::NanoV3.builtin_board_model(), "arduino_nano_v3");

        assert_eq!(HostBoard::Mega2560Rev3.label(), "Arduino Mega 2560");
        assert_eq!(HostBoard::Mega2560Rev3.fqbn(), "arduino:avr:mega");
        assert_eq!(HostBoard::Mega2560Rev3.short_name(), "mega");
        assert_eq!(
            HostBoard::Mega2560Rev3.builtin_board_model(),
            "arduino_mega_2560_rev3"
        );
        assert_eq!(HostBoard::default(), HostBoard::Mega2560Rev3);
    }

    #[test]
    fn firmware_and_pcb_sources_require_non_empty_paths() {
        let firmware = FirmwareSource {
            kind: FirmwareSourceKind::Hex,
            path: "".into(),
            compiled_hex_path: None,
        };
        assert!(matches!(
            firmware.validate(),
            Err(ProjectError::MissingFirmwarePath)
        ));

        let pcb = PcbSource {
            path: "".into(),
            board_name_hint: None,
        };
        assert!(matches!(pcb.validate(), Err(ProjectError::MissingPcbPath)));

        let firmware = FirmwareSource {
            kind: FirmwareSourceKind::Ino,
            path: "firmware/controller.ino".into(),
            compiled_hex_path: Some("build/controller.hex".into()),
        };
        let pcb = PcbSource {
            path: "pcb/controller.kicad_pcb".into(),
            board_name_hint: Some("controller".to_string()),
        };

        assert!(firmware.validate().is_ok());
        assert!(pcb.validate().is_ok());
    }

    #[test]
    fn host_board_binding_validation_accepts_known_unique_signals() {
        let signal = known_host_signal(HostBoard::NanoV3);
        let bindings = vec![SignalBinding {
            board_signal: signal,
            pcb_net: "NET_SIG".to_string(),
            mode: BindingMode::Digital,
            note: Some("status LED".to_string()),
        }];

        assert!(validate_host_board_signal_bindings(HostBoard::NanoV3, &bindings).is_ok());
    }

    #[test]
    fn host_board_binding_validation_rejects_unknown_duplicate_and_empty_nets() {
        let signal = known_host_signal(HostBoard::Mega2560Rev3);

        let unknown = vec![SignalBinding {
            board_signal: "NOT_A_REAL_SIGNAL".to_string(),
            pcb_net: "NET1".to_string(),
            mode: BindingMode::Auto,
            note: None,
        }];
        assert!(matches!(
            validate_host_board_signal_bindings(HostBoard::Mega2560Rev3, &unknown),
            Err(ProjectError::UnknownBoardSignal(_))
        ));

        let duplicate = vec![
            SignalBinding {
                board_signal: signal.clone(),
                pcb_net: "NET1".to_string(),
                mode: BindingMode::Digital,
                note: None,
            },
            SignalBinding {
                board_signal: signal.clone(),
                pcb_net: "NET2".to_string(),
                mode: BindingMode::Analog,
                note: None,
            },
        ];
        assert!(matches!(
            validate_host_board_signal_bindings(HostBoard::Mega2560Rev3, &duplicate),
            Err(ProjectError::DuplicateBoardSignal(found)) if found == signal
        ));

        let empty_net = vec![SignalBinding {
            board_signal: signal.clone(),
            pcb_net: "   ".to_string(),
            mode: BindingMode::Power,
            note: None,
        }];
        assert!(matches!(
            validate_host_board_signal_bindings(HostBoard::Mega2560Rev3, &empty_net),
            Err(ProjectError::EmptyPcbNet(found)) if found == signal
        ));
    }

    #[test]
    fn module_overlay_validation_accepts_known_models_and_bindings() {
        let overlays = vec![ModuleOverlay {
            name: "sensor".to_string(),
            model: known_module_model(),
            bindings: vec![
                ModuleSignalBinding {
                    module_signal: "SDA".to_string(),
                    pcb_net: "I2C_SDA".to_string(),
                    mode: BindingMode::Bus,
                    note: None,
                },
                ModuleSignalBinding {
                    module_signal: "SCL".to_string(),
                    pcb_net: "I2C_SCL".to_string(),
                    mode: BindingMode::Bus,
                    note: None,
                },
            ],
        }];

        assert!(validate_module_overlays(&overlays).is_ok());
    }

    #[test]
    fn module_overlay_validation_rejects_bad_names_models_and_bindings() {
        let model = known_module_model();

        let empty_name = vec![ModuleOverlay {
            name: "   ".to_string(),
            model: model.clone(),
            bindings: Vec::new(),
        }];
        assert!(matches!(
            validate_module_overlays(&empty_name),
            Err(ProjectError::EmptyName("module overlay"))
        ));

        let duplicate_name = vec![
            ModuleOverlay {
                name: "sensor".to_string(),
                model: model.clone(),
                bindings: Vec::new(),
            },
            ModuleOverlay {
                name: "sensor".to_string(),
                model: model.clone(),
                bindings: Vec::new(),
            },
        ];
        assert!(matches!(
            validate_module_overlays(&duplicate_name),
            Err(ProjectError::DuplicateModuleOverlay(found)) if found == "sensor"
        ));

        let unknown_model = vec![ModuleOverlay {
            name: "sensor".to_string(),
            model: "unknown_model".to_string(),
            bindings: Vec::new(),
        }];
        assert!(matches!(
            validate_module_overlays(&unknown_model),
            Err(ProjectError::UnknownModuleModel(found)) if found == "unknown_model"
        ));

        let empty_signal = vec![ModuleOverlay {
            name: "sensor".to_string(),
            model: model.clone(),
            bindings: vec![ModuleSignalBinding {
                module_signal: "   ".to_string(),
                pcb_net: "I2C_SDA".to_string(),
                mode: BindingMode::Bus,
                note: None,
            }],
        }];
        assert!(matches!(
            validate_module_overlays(&empty_signal),
            Err(ProjectError::EmptyModuleSignal(found)) if found == "sensor"
        ));

        let empty_net = vec![ModuleOverlay {
            name: "sensor".to_string(),
            model: model.clone(),
            bindings: vec![ModuleSignalBinding {
                module_signal: "SDA".to_string(),
                pcb_net: " ".to_string(),
                mode: BindingMode::Bus,
                note: None,
            }],
        }];
        assert!(matches!(
            validate_module_overlays(&empty_net),
            Err(ProjectError::EmptyPcbNet(found)) if found == "SDA"
        ));

        let duplicate_signal = vec![ModuleOverlay {
            name: "sensor".to_string(),
            model,
            bindings: vec![
                ModuleSignalBinding {
                    module_signal: "SDA".to_string(),
                    pcb_net: "I2C_SDA".to_string(),
                    mode: BindingMode::Bus,
                    note: None,
                },
                ModuleSignalBinding {
                    module_signal: "SDA".to_string(),
                    pcb_net: "I2C_SDA_2".to_string(),
                    mode: BindingMode::Bus,
                    note: None,
                },
            ],
        }];
        assert!(matches!(
            validate_module_overlays(&duplicate_signal),
            Err(ProjectError::DuplicateModuleSignal { module_name, signal })
                if module_name == "sensor" && signal == "SDA"
        ));
    }
}
