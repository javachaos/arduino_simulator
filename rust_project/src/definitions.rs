use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::document::AvrSimDocument;
use crate::error::ProjectError;
use crate::shared::{BindingMode, FirmwareSource, HostBoard, PROJECT_FORMAT_VERSION};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionSourceKind {
    BuiltinBoardModel,
    KicadPcb,
    Virtual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefinitionSource {
    pub kind: DefinitionSourceKind,
    pub path: Option<PathBuf>,
    pub builtin_name: Option<String>,
}

impl DefinitionSource {
    pub fn builtin_board_model(name: impl Into<String>) -> Self {
        Self {
            kind: DefinitionSourceKind::BuiltinBoardModel,
            path: None,
            builtin_name: Some(name.into()),
        }
    }

    pub fn kicad_pcb(path: impl Into<PathBuf>) -> Self {
        Self {
            kind: DefinitionSourceKind::KicadPcb,
            path: Some(path.into()),
            builtin_name: None,
        }
    }

    pub fn virtual_only() -> Self {
        Self {
            kind: DefinitionSourceKind::Virtual,
            path: None,
            builtin_name: None,
        }
    }

    pub fn validate(&self, owner: &str) -> Result<(), ProjectError> {
        match self.kind {
            DefinitionSourceKind::BuiltinBoardModel => {
                if self
                    .builtin_name
                    .as_ref()
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
                {
                    return Err(ProjectError::MissingDefinitionSource(owner.to_string()));
                }
                if self.path.is_some() {
                    return Err(ProjectError::InvalidDefinitionSource(format!(
                        "{owner} uses builtin_board_model source but also has a path"
                    )));
                }
            }
            DefinitionSourceKind::KicadPcb => {
                if self
                    .path
                    .as_ref()
                    .map(|value| value.as_os_str().is_empty())
                    .unwrap_or(true)
                {
                    return Err(ProjectError::MissingDefinitionSource(owner.to_string()));
                }
                if self.builtin_name.is_some() {
                    return Err(ProjectError::InvalidDefinitionSource(format!(
                        "{owner} uses kicad_pcb source but also has a builtin name"
                    )));
                }
            }
            DefinitionSourceKind::Virtual => {
                if self.path.is_some() || self.builtin_name.is_some() {
                    return Err(ProjectError::InvalidDefinitionSource(format!(
                        "{owner} uses virtual source but also specifies an external source"
                    )));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortDirection {
    Input,
    Output,
    Bidirectional,
    Passive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortClass {
    Digital,
    Analog,
    Power,
    Bus,
    Passive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortDefinition {
    pub name: String,
    pub class: PortClass,
    pub direction: PortDirection,
    pub aliases: Vec<String>,
    pub note: Option<String>,
}

impl PortDefinition {
    pub fn new(name: impl Into<String>, class: PortClass, direction: PortDirection) -> Self {
        Self {
            name: name.into(),
            class,
            direction,
            aliases: Vec::new(),
            note: None,
        }
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.name.trim().is_empty() {
            return Err(ProjectError::EmptyPortName);
        }
        Ok(())
    }
}

fn validate_port_set(ports: &[PortDefinition]) -> Result<(), ProjectError> {
    let mut seen = BTreeSet::new();
    for port in ports {
        port.validate()?;
        if !seen.insert(port.name.clone()) {
            return Err(ProjectError::DuplicatePortName(port.name.clone()));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionReferenceKind {
    BoardDefinition,
    ModuleDefinition,
    AssemblyDefinition,
    AssemblyBundle,
    BehaviorDefinition,
    HostBoard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefinitionReference {
    pub kind: DefinitionReferenceKind,
    pub path: Option<PathBuf>,
    pub builtin_name: Option<String>,
    pub host_board: Option<HostBoard>,
}

impl DefinitionReference {
    pub fn file(kind: DefinitionReferenceKind, path: impl Into<PathBuf>) -> Self {
        Self {
            kind,
            path: Some(path.into()),
            builtin_name: None,
            host_board: None,
        }
    }

    pub fn builtin(kind: DefinitionReferenceKind, name: impl Into<String>) -> Self {
        Self {
            kind,
            path: None,
            builtin_name: Some(name.into()),
            host_board: None,
        }
    }

    pub fn host_board(board: HostBoard) -> Self {
        Self {
            kind: DefinitionReferenceKind::HostBoard,
            path: None,
            builtin_name: None,
            host_board: Some(board),
        }
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        match self.kind {
            DefinitionReferenceKind::HostBoard => {
                if self.host_board.is_none() || self.path.is_some() || self.builtin_name.is_some() {
                    return Err(ProjectError::InvalidDefinitionReference(
                        "host_board references must only define host_board".to_string(),
                    ));
                }
            }
            _ => {
                let has_path = self
                    .path
                    .as_ref()
                    .map(|value| !value.as_os_str().is_empty())
                    .unwrap_or(false);
                let has_builtin = self
                    .builtin_name
                    .as_ref()
                    .map(|value| !value.trim().is_empty())
                    .unwrap_or(false);
                if self.host_board.is_some() || (has_path == has_builtin) {
                    return Err(ProjectError::InvalidDefinitionReference(
                        "document references must define exactly one of path or builtin_name"
                            .to_string(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardDefinition {
    pub format_version: String,
    pub name: String,
    pub description: Option<String>,
    pub source: DefinitionSource,
    pub ports: Vec<PortDefinition>,
    pub default_firmware: Option<FirmwareSource>,
}

impl BoardDefinition {
    pub fn new(name: impl Into<String>, source: DefinitionSource) -> Self {
        Self {
            format_version: PROJECT_FORMAT_VERSION.to_string(),
            name: name.into(),
            description: None,
            source,
            ports: Vec::new(),
            default_firmware: None,
        }
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.name.trim().is_empty() {
            return Err(ProjectError::EmptyName("board definition"));
        }
        self.source.validate(&self.name)?;
        validate_port_set(&self.ports)?;
        if let Some(firmware) = &self.default_firmware {
            firmware.validate()?;
        }
        Ok(())
    }

    pub fn to_json_pretty(&self) -> Result<String, ProjectError> {
        AvrSimDocument::BoardDefinition(self.clone()).to_json_pretty()
    }

    pub fn save_json(&self, path: &Path) -> Result<(), ProjectError> {
        AvrSimDocument::BoardDefinition(self.clone()).save_json(path)
    }

    pub fn load_json(path: &Path) -> Result<Self, ProjectError> {
        match AvrSimDocument::load_json(path)? {
            AvrSimDocument::BoardDefinition(definition) => Ok(definition),
            other => Err(ProjectError::UnexpectedDocumentKind(
                other.kind_name().to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleDefinition {
    pub format_version: String,
    pub name: String,
    pub description: Option<String>,
    pub source: DefinitionSource,
    pub ports: Vec<PortDefinition>,
    pub embedded_host_board: Option<HostBoard>,
    pub default_firmware: Option<FirmwareSource>,
}

impl ModuleDefinition {
    pub fn new(name: impl Into<String>, source: DefinitionSource) -> Self {
        Self {
            format_version: PROJECT_FORMAT_VERSION.to_string(),
            name: name.into(),
            description: None,
            source,
            ports: Vec::new(),
            embedded_host_board: None,
            default_firmware: None,
        }
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.name.trim().is_empty() {
            return Err(ProjectError::EmptyName("module definition"));
        }
        self.source.validate(&self.name)?;
        validate_port_set(&self.ports)?;
        if let Some(firmware) = &self.default_firmware {
            firmware.validate()?;
        }
        Ok(())
    }

    pub fn to_json_pretty(&self) -> Result<String, ProjectError> {
        AvrSimDocument::ModuleDefinition(self.clone()).to_json_pretty()
    }

    pub fn save_json(&self, path: &Path) -> Result<(), ProjectError> {
        AvrSimDocument::ModuleDefinition(self.clone()).save_json(path)
    }

    pub fn load_json(path: &Path) -> Result<Self, ProjectError> {
        match AvrSimDocument::load_json(path)? {
            AvrSimDocument::ModuleDefinition(definition) => Ok(definition),
            other => Err(ProjectError::UnexpectedDocumentKind(
                other.kind_name().to_string(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachedInstance {
    pub id: String,
    pub label: Option<String>,
    pub reference: DefinitionReference,
    pub ports: Vec<PortDefinition>,
    pub firmware_override: Option<FirmwareSource>,
    pub note: Option<String>,
}

impl AttachedInstance {
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.id.trim().is_empty() {
            return Err(ProjectError::EmptyName("attached instance"));
        }
        self.reference.validate()?;
        validate_port_set(&self.ports)?;
        if let Some(firmware) = &self.firmware_override {
            firmware.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentEndpoint {
    pub instance_id: Option<String>,
    pub port: String,
}

impl AttachmentEndpoint {
    pub fn primary(port: impl Into<String>) -> Self {
        Self {
            instance_id: None,
            port: port.into(),
        }
    }

    pub fn child(instance_id: impl Into<String>, port: impl Into<String>) -> Self {
        Self {
            instance_id: Some(instance_id.into()),
            port: port.into(),
        }
    }

    fn validate_shape(&self) -> Result<(), ProjectError> {
        if self.port.trim().is_empty() {
            return Err(ProjectError::EmptyPortName);
        }
        if let Some(instance_id) = &self.instance_id {
            if instance_id.trim().is_empty() {
                return Err(ProjectError::UnknownInstanceId(instance_id.clone()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentBinding {
    pub from: AttachmentEndpoint,
    pub to: AttachmentEndpoint,
    pub mode: BindingMode,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssemblyExport {
    pub name: String,
    pub source: AttachmentEndpoint,
    pub aliases: Vec<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssemblyDefinition {
    pub format_version: String,
    pub name: String,
    pub description: Option<String>,
    pub primary: DefinitionReference,
    pub primary_ports: Vec<PortDefinition>,
    pub children: Vec<AttachedInstance>,
    pub attachments: Vec<AttachmentBinding>,
    pub exports: Vec<AssemblyExport>,
}

impl AssemblyDefinition {
    pub fn new(name: impl Into<String>, primary: DefinitionReference) -> Self {
        Self {
            format_version: PROJECT_FORMAT_VERSION.to_string(),
            name: name.into(),
            description: None,
            primary,
            primary_ports: Vec::new(),
            children: Vec::new(),
            attachments: Vec::new(),
            exports: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.name.trim().is_empty() {
            return Err(ProjectError::EmptyName("assembly definition"));
        }
        self.primary.validate()?;
        validate_port_set(&self.primary_ports)?;

        let mut child_ports = BTreeMap::new();
        for child in &self.children {
            child.validate()?;
            if child_ports
                .insert(
                    child.id.clone(),
                    child
                        .ports
                        .iter()
                        .map(|port| port.name.clone())
                        .collect::<BTreeSet<_>>(),
                )
                .is_some()
            {
                return Err(ProjectError::DuplicateInstanceId(child.id.clone()));
            }
        }

        let primary_ports = self
            .primary_ports
            .iter()
            .map(|port| port.name.clone())
            .collect::<BTreeSet<_>>();

        for attachment in &self.attachments {
            attachment.from.validate_shape()?;
            attachment.to.validate_shape()?;
            validate_endpoint(&attachment.from, &primary_ports, &child_ports)?;
            validate_endpoint(&attachment.to, &primary_ports, &child_ports)?;
        }

        let mut export_names = BTreeSet::new();
        for export in &self.exports {
            if export.name.trim().is_empty() {
                return Err(ProjectError::EmptyName("assembly export"));
            }
            if !export_names.insert(export.name.clone()) {
                return Err(ProjectError::DuplicateExportName(export.name.clone()));
            }
            export.source.validate_shape()?;
            validate_endpoint(&export.source, &primary_ports, &child_ports)?;
        }

        Ok(())
    }

    pub fn to_json_pretty(&self) -> Result<String, ProjectError> {
        AvrSimDocument::AssemblyDefinition(self.clone()).to_json_pretty()
    }

    pub fn save_json(&self, path: &Path) -> Result<(), ProjectError> {
        AvrSimDocument::AssemblyDefinition(self.clone()).save_json(path)
    }

    pub fn load_json(path: &Path) -> Result<Self, ProjectError> {
        match AvrSimDocument::load_json(path)? {
            AvrSimDocument::AssemblyDefinition(definition) => Ok(definition),
            other => Err(ProjectError::UnexpectedDocumentKind(
                other.kind_name().to_string(),
            )),
        }
    }
}

fn validate_endpoint(
    endpoint: &AttachmentEndpoint,
    primary_ports: &BTreeSet<String>,
    child_ports: &BTreeMap<String, BTreeSet<String>>,
) -> Result<(), ProjectError> {
    match &endpoint.instance_id {
        None => {
            if !primary_ports.contains(&endpoint.port) {
                return Err(ProjectError::UnknownPrimaryPort(endpoint.port.clone()));
            }
        }
        Some(instance_id) => {
            let Some(ports) = child_ports.get(instance_id) else {
                return Err(ProjectError::UnknownInstanceId(instance_id.clone()));
            };
            if !ports.contains(&endpoint.port) {
                return Err(ProjectError::UnknownChildPort {
                    instance_id: instance_id.clone(),
                    port: endpoint.port.clone(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::{
        AssemblyDefinition, AssemblyExport, AttachedInstance, AttachmentBinding,
        AttachmentEndpoint, BoardDefinition, DefinitionReference, DefinitionReferenceKind,
        DefinitionSource, ModuleDefinition, PortClass, PortDefinition, PortDirection,
    };
    use crate::error::ProjectError;
    use crate::shared::{BindingMode, FirmwareSource, FirmwareSourceKind, HostBoard};

    fn sample_board_definition() -> BoardDefinition {
        let mut board = BoardDefinition::new(
            "mega_sidecar",
            DefinitionSource::kicad_pcb("/tmp/mega_r3_sidecar_controller_rev_a.kicad_pcb"),
        );
        board.ports = vec![
            PortDefinition::new("D27", PortClass::Digital, PortDirection::Bidirectional),
            PortDefinition::new("D44_PWM", PortClass::Analog, PortDirection::Output),
            PortDefinition::new("A10", PortClass::Analog, PortDirection::Input),
        ];
        board
    }

    fn sample_module_definition() -> ModuleDefinition {
        let mut module = ModuleDefinition::new("air_node_nano", DefinitionSource::virtual_only());
        module.embedded_host_board = Some(HostBoard::NanoV3);
        module.ports = vec![
            PortDefinition::new("CAN_H", PortClass::Bus, PortDirection::Bidirectional),
            PortDefinition::new("CAN_L", PortClass::Bus, PortDirection::Bidirectional),
            PortDefinition::new("+24V", PortClass::Power, PortDirection::Input),
            PortDefinition::new("GND", PortClass::Power, PortDirection::Passive),
        ];
        module.default_firmware = Some(FirmwareSource {
            kind: FirmwareSourceKind::Hex,
            path: PathBuf::from("/tmp/air_node.hex"),
            compiled_hex_path: None,
        });
        module
    }

    fn sample_assembly_definition() -> AssemblyDefinition {
        let mut assembly = AssemblyDefinition::new(
            "air_node_assembly",
            DefinitionReference::file(
                DefinitionReferenceKind::BoardDefinition,
                "/tmp/air_node_board.avrsim.json",
            ),
        );
        assembly.primary_ports = vec![
            PortDefinition::new("J1_1", PortClass::Bus, PortDirection::Bidirectional),
            PortDefinition::new("J1_2", PortClass::Bus, PortDirection::Bidirectional),
            PortDefinition::new("J1_7", PortClass::Power, PortDirection::Input),
            PortDefinition::new("J1_8", PortClass::Power, PortDirection::Passive),
        ];
        assembly.children.push(AttachedInstance {
            id: "nano".to_string(),
            label: Some("Arduino Nano".to_string()),
            reference: DefinitionReference::host_board(HostBoard::NanoV3),
            ports: vec![
                PortDefinition::new("D10_SS", PortClass::Bus, PortDirection::Output),
                PortDefinition::new("D11_MOSI", PortClass::Bus, PortDirection::Output),
                PortDefinition::new("D12_MISO", PortClass::Bus, PortDirection::Input),
                PortDefinition::new("D13_SCK", PortClass::Bus, PortDirection::Output),
            ],
            firmware_override: Some(FirmwareSource {
                kind: FirmwareSourceKind::Ino,
                path: PathBuf::from("/tmp/nano_sht31_can_node.ino"),
                compiled_hex_path: None,
            }),
            note: None,
        });
        assembly.children.push(AttachedInstance {
            id: "mcp2515".to_string(),
            label: Some("CAN breakout".to_string()),
            reference: DefinitionReference::file(
                DefinitionReferenceKind::ModuleDefinition,
                "/tmp/mcp2515_module.avrsim.json",
            ),
            ports: vec![
                PortDefinition::new("CS", PortClass::Bus, PortDirection::Input),
                PortDefinition::new("MOSI", PortClass::Bus, PortDirection::Input),
                PortDefinition::new("MISO", PortClass::Bus, PortDirection::Output),
                PortDefinition::new("SCK", PortClass::Bus, PortDirection::Input),
            ],
            firmware_override: None,
            note: None,
        });
        assembly.attachments = vec![
            AttachmentBinding {
                from: AttachmentEndpoint::child("nano", "D10_SS"),
                to: AttachmentEndpoint::child("mcp2515", "CS"),
                mode: BindingMode::Bus,
                note: None,
            },
            AttachmentBinding {
                from: AttachmentEndpoint::primary("J1_1"),
                to: AttachmentEndpoint::child("mcp2515", "MISO"),
                mode: BindingMode::Bus,
                note: Some("Illustrative export path".to_string()),
            },
        ];
        assembly.exports = vec![AssemblyExport {
            name: "CAN_H".to_string(),
            source: AttachmentEndpoint::primary("J1_1"),
            aliases: vec!["RJ45.1".to_string()],
            note: None,
        }];
        assembly
    }

    #[test]
    fn board_definition_round_trips_and_validates() {
        let board = sample_board_definition();
        board.validate().expect("valid");
        let json = board.to_json_pretty().expect("json");
        assert!(json.contains("\"kind\": \"board_definition\""));
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("board.avrsim.json");
        board.save_json(&path).expect("save");
        let loaded = BoardDefinition::load_json(&path).expect("load");
        assert_eq!(loaded, board);
    }

    #[test]
    fn module_definition_round_trips_and_validates() {
        let module = sample_module_definition();
        module.validate().expect("valid");
        let json = module.to_json_pretty().expect("json");
        assert!(json.contains("\"kind\": \"module_definition\""));
    }

    #[test]
    fn assembly_definition_round_trips_and_validates() {
        let assembly = sample_assembly_definition();
        assembly.validate().expect("valid");
        let json = assembly.to_json_pretty().expect("json");
        assert!(json.contains("\"kind\": \"assembly_definition\""));
    }

    #[test]
    fn assembly_definition_rejects_duplicate_child_ids() {
        let mut assembly = sample_assembly_definition();
        assembly.children.push(assembly.children[0].clone());
        let error = assembly.validate().expect_err("duplicate child ids");
        assert!(matches!(error, ProjectError::DuplicateInstanceId(_)));
    }

    #[test]
    fn assembly_definition_rejects_unknown_ports() {
        let mut assembly = sample_assembly_definition();
        assembly.attachments[0].to.port = "NOPE".to_string();
        let error = assembly.validate().expect_err("unknown port");
        assert!(matches!(error, ProjectError::UnknownChildPort { .. }));
    }

    #[test]
    fn board_definition_rejects_duplicate_ports() {
        let mut board = sample_board_definition();
        board.ports.push(board.ports[0].clone());
        let error = board.validate().expect_err("duplicate ports");
        assert!(matches!(error, ProjectError::DuplicatePortName(_)));
    }
}
