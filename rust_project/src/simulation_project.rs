use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::definitions::DefinitionReference;
use crate::document::AvrSimDocument;
use crate::error::ProjectError;
use crate::shared::{
    validate_host_board_signal_bindings, validate_module_overlays, FirmwareSource, HostBoard,
    ModuleOverlay, PcbSource, ProbeSpec, SignalBinding, StimulusSpec, PROJECT_FORMAT_VERSION,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationProject {
    pub format_version: String,
    pub name: String,
    pub description: Option<String>,
    pub host_board: HostBoard,
    pub firmware: FirmwareSource,
    pub pcb: PcbSource,
    #[serde(default)]
    pub root_assembly: Option<DefinitionReference>,
    #[serde(default)]
    pub module_overlays: Vec<ModuleOverlay>,
    pub bindings: Vec<SignalBinding>,
    pub probes: Vec<ProbeSpec>,
    pub stimuli: Vec<StimulusSpec>,
}

impl SimulationProject {
    pub fn new(
        name: impl Into<String>,
        host_board: HostBoard,
        firmware: FirmwareSource,
        pcb: PcbSource,
    ) -> Self {
        Self {
            format_version: PROJECT_FORMAT_VERSION.to_string(),
            name: name.into(),
            description: None,
            host_board,
            firmware,
            pcb,
            root_assembly: None,
            module_overlays: Vec::new(),
            bindings: Vec::new(),
            probes: Vec::new(),
            stimuli: Vec::new(),
        }
    }

    pub fn to_json_pretty(&self) -> Result<String, ProjectError> {
        AvrSimDocument::SimulationProject(self.clone()).to_json_pretty()
    }

    pub fn save_json(&self, path: &Path) -> Result<(), ProjectError> {
        AvrSimDocument::SimulationProject(self.clone()).save_json(path)
    }

    pub fn load_json(path: &Path) -> Result<Self, ProjectError> {
        let text = std::fs::read_to_string(path)?;
        if let Ok(document) = serde_json::from_str::<AvrSimDocument>(&text) {
            return match document {
                AvrSimDocument::SimulationProject(project) => Ok(project),
                other => Err(ProjectError::UnexpectedDocumentKind(
                    other.kind_name().to_string(),
                )),
            };
        }
        Ok(serde_json::from_str(&text)?)
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.name.trim().is_empty() {
            return Err(ProjectError::EmptyName("project"));
        }
        self.firmware.validate()?;
        self.pcb.validate()?;
        if let Some(root_assembly) = &self.root_assembly {
            root_assembly.validate()?;
        }
        validate_host_board_signal_bindings(self.host_board, &self.bindings)?;
        validate_module_overlays(&self.module_overlays)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::SimulationProject;
    use crate::definitions::{DefinitionReference, DefinitionReferenceKind};
    use crate::error::ProjectError;
    use crate::shared::{
        BindingMode, FirmwareSource, FirmwareSourceKind, HostBoard, PcbSource, ProbeKind,
        ProbeSpec, SignalBinding, StimulusKind, StimulusSpec,
    };
    use std::path::PathBuf;

    fn sample_project() -> SimulationProject {
        let mut project = SimulationProject::new(
            "Main Controller",
            HostBoard::Mega2560Rev3,
            FirmwareSource {
                kind: FirmwareSourceKind::Ino,
                path: PathBuf::from("/tmp/dewpoint_controller.ino"),
                compiled_hex_path: Some(PathBuf::from("/tmp/dewpoint_controller.ino.hex")),
            },
            PcbSource {
                path: PathBuf::from("/tmp/mega_r3_sidecar_controller_rev_a.kicad_pcb"),
                board_name_hint: Some("mega_r3_sidecar_controller_rev_a".to_string()),
            },
        );
        project.root_assembly = Some(DefinitionReference::file(
            DefinitionReferenceKind::AssemblyBundle,
            "/tmp/main_controller_stack.avrsim.json",
        ));
        project.bindings.push(SignalBinding {
            board_signal: "D27".to_string(),
            pcb_net: "/MCP2515_CS".to_string(),
            mode: BindingMode::Digital,
            note: None,
        });
        project.bindings.push(SignalBinding {
            board_signal: "D44_PWM".to_string(),
            pcb_net: "PWM_44_RAW".to_string(),
            mode: BindingMode::Analog,
            note: Some("External 0-10V stage input".to_string()),
        });
        project.probes.push(ProbeSpec {
            name: "Actuator feedback".to_string(),
            pcb_net: "/ACT_U_RAW".to_string(),
            kind: ProbeKind::Analog,
        });
        project.stimuli.push(StimulusSpec {
            name: "Cooling call".to_string(),
            pcb_net: "COOLING_CALL_IN".to_string(),
            kind: StimulusKind::DigitalHigh,
            value: None,
        });
        project
    }

    #[test]
    fn project_round_trips_through_json_document() {
        let project = sample_project();
        let json = project.to_json_pretty().expect("json");
        assert!(json.contains("\"kind\": \"simulation_project\""));
        let decoded: SimulationProject =
            match serde_json::from_str::<crate::document::AvrSimDocument>(&json).expect("decode") {
                crate::document::AvrSimDocument::SimulationProject(project) => project,
                _ => panic!("unexpected document kind"),
            };
        assert_eq!(decoded, project);
    }

    #[test]
    fn project_save_and_load_work() {
        let project = sample_project();
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("project.avrsim.json");
        project.save_json(&path).expect("save");
        let loaded = SimulationProject::load_json(&path).expect("load");
        assert_eq!(loaded, project);
    }

    #[test]
    fn validation_accepts_known_host_signals() {
        let project = sample_project();
        project.validate().expect("valid");
    }

    #[test]
    fn validation_rejects_unknown_host_signals() {
        let mut project = sample_project();
        project.bindings[0].board_signal = "NOT_A_PIN".to_string();
        let error = project.validate().expect_err("invalid");
        assert!(matches!(error, ProjectError::UnknownBoardSignal(_)));
    }

    #[test]
    fn validation_rejects_duplicate_host_signals() {
        let mut project = sample_project();
        project.bindings.push(project.bindings[0].clone());
        let error = project.validate().expect_err("duplicate");
        assert!(matches!(error, ProjectError::DuplicateBoardSignal(_)));
    }

    #[test]
    fn host_board_metadata_is_stable() {
        assert_eq!(
            HostBoard::NanoV3.fqbn(),
            "arduino:avr:nano:cpu=atmega328old"
        );
        assert_eq!(HostBoard::Mega2560Rev3.short_name(), "mega");
        assert_eq!(
            HostBoard::Mega2560Rev3.builtin_board_model(),
            "arduino_mega_2560_rev3"
        );
    }

    #[test]
    fn validation_rejects_missing_required_paths() {
        let mut project = sample_project();
        project.name.clear();
        assert!(matches!(
            project.validate().expect_err("missing name"),
            ProjectError::EmptyName("project")
        ));

        let mut project = sample_project();
        project.firmware.path = PathBuf::new();
        assert!(matches!(
            project.validate().expect_err("missing firmware"),
            ProjectError::MissingFirmwarePath
        ));

        let mut project = sample_project();
        project.pcb.path = PathBuf::new();
        assert!(matches!(
            project.validate().expect_err("missing pcb"),
            ProjectError::MissingPcbPath
        ));
    }
}
