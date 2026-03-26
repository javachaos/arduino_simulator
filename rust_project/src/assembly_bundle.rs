use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::definitions::{
    AssemblyExport, AttachmentBinding, AttachmentEndpoint, DefinitionReference, DefinitionSource,
    PortDefinition,
};
use crate::document::AvrSimDocument;
use crate::error::ProjectError;
use crate::shared::{FirmwareSource, HostBoard, PROJECT_FORMAT_VERSION};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyMemberKind {
    Board,
    Module,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssemblyMember {
    pub id: String,
    pub kind: AssemblyMemberKind,
    pub name: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub source: DefinitionSource,
    pub ports: Vec<PortDefinition>,
    pub embedded_host_board: Option<HostBoard>,
    pub firmware: Option<FirmwareSource>,
    pub behavior: Option<DefinitionReference>,
    pub note: Option<String>,
}

impl AssemblyMember {
    pub fn new(
        id: impl Into<String>,
        kind: AssemblyMemberKind,
        name: impl Into<String>,
        source: DefinitionSource,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            name: name.into(),
            label: None,
            description: None,
            source,
            ports: Vec::new(),
            embedded_host_board: None,
            firmware: None,
            behavior: None,
            note: None,
        }
    }

    pub fn validate(&self, role: &'static str) -> Result<(), ProjectError> {
        if self.id.trim().is_empty() {
            return Err(ProjectError::EmptyName(role));
        }
        if self.name.trim().is_empty() {
            return Err(ProjectError::EmptyName(role));
        }
        self.source.validate(&self.name)?;
        validate_port_set(&self.ports)?;
        if let Some(firmware) = &self.firmware {
            firmware.validate()?;
        }
        if let Some(behavior) = &self.behavior {
            behavior.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssemblyBundle {
    pub format_version: String,
    pub name: String,
    pub description: Option<String>,
    pub primary: AssemblyMember,
    pub children: Vec<AssemblyMember>,
    pub attachments: Vec<AttachmentBinding>,
    pub exports: Vec<AssemblyExport>,
}

impl AssemblyBundle {
    pub fn new(name: impl Into<String>, primary: AssemblyMember) -> Self {
        Self {
            format_version: PROJECT_FORMAT_VERSION.to_string(),
            name: name.into(),
            description: None,
            primary,
            children: Vec::new(),
            attachments: Vec::new(),
            exports: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.name.trim().is_empty() {
            return Err(ProjectError::EmptyName("assembly bundle"));
        }
        self.primary.validate("primary assembly member")?;

        let mut child_ports = BTreeMap::new();
        let mut child_ids = BTreeSet::new();
        for child in &self.children {
            child.validate("assembly member")?;
            if child.id == self.primary.id || !child_ids.insert(child.id.clone()) {
                return Err(ProjectError::DuplicateInstanceId(child.id.clone()));
            }
            child_ports.insert(
                child.id.clone(),
                child
                    .ports
                    .iter()
                    .map(|port| port.name.clone())
                    .collect::<BTreeSet<_>>(),
            );
        }

        let primary_ports = self
            .primary
            .ports
            .iter()
            .map(|port| port.name.clone())
            .collect::<BTreeSet<_>>();

        for attachment in &self.attachments {
            validate_endpoint_shape(&attachment.from)?;
            validate_endpoint_shape(&attachment.to)?;
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
            validate_endpoint_shape(&export.source)?;
            validate_endpoint(&export.source, &primary_ports, &child_ports)?;
        }

        Ok(())
    }

    pub fn to_json_pretty(&self) -> Result<String, ProjectError> {
        AvrSimDocument::AssemblyBundle(self.clone()).to_json_pretty()
    }

    pub fn save_json(&self, path: &Path) -> Result<(), ProjectError> {
        AvrSimDocument::AssemblyBundle(self.clone()).save_json(path)
    }

    pub fn load_json(path: &Path) -> Result<Self, ProjectError> {
        match AvrSimDocument::load_json(path)? {
            AvrSimDocument::AssemblyBundle(bundle) => Ok(bundle),
            other => Err(ProjectError::UnexpectedDocumentKind(
                other.kind_name().to_string(),
            )),
        }
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

fn validate_endpoint_shape(endpoint: &AttachmentEndpoint) -> Result<(), ProjectError> {
    if endpoint.port.trim().is_empty() {
        return Err(ProjectError::EmptyPortName);
    }
    if let Some(instance_id) = &endpoint.instance_id {
        if instance_id.trim().is_empty() {
            return Err(ProjectError::UnknownInstanceId(instance_id.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::{AssemblyBundle, AssemblyMember, AssemblyMemberKind};
    use crate::definitions::{
        AssemblyExport, AttachmentBinding, AttachmentEndpoint, DefinitionSource, PortClass,
        PortDefinition, PortDirection,
    };
    use crate::error::ProjectError;
    use crate::shared::{BindingMode, FirmwareSource, FirmwareSourceKind, HostBoard};

    fn sample_bundle() -> AssemblyBundle {
        let mut primary = AssemblyMember::new(
            "primary",
            AssemblyMemberKind::Board,
            "main_controller_sidecar",
            DefinitionSource::kicad_pcb("/tmp/mega_r3_sidecar_controller_rev_a.kicad_pcb"),
        );
        primary.embedded_host_board = Some(HostBoard::Mega2560Rev3);
        primary.firmware = Some(FirmwareSource {
            kind: FirmwareSourceKind::Ino,
            path: PathBuf::from("/tmp/dewpoint_controller.ino"),
            compiled_hex_path: None,
        });
        primary.ports = vec![
            PortDefinition::new("CANH", PortClass::Bus, PortDirection::Bidirectional),
            PortDefinition::new("CANL", PortClass::Bus, PortDirection::Bidirectional),
            PortDefinition::new("+24V", PortClass::Power, PortDirection::Input),
            PortDefinition::new("GND", PortClass::Power, PortDirection::Passive),
        ];

        let mut bundle = AssemblyBundle::new("DewPoint Stack", primary);

        let mut air_node = AssemblyMember::new(
            "air_node_1",
            AssemblyMemberKind::Board,
            "air_node",
            DefinitionSource::kicad_pcb("/tmp/air_node.kicad_pcb"),
        );
        air_node.embedded_host_board = Some(HostBoard::NanoV3);
        air_node.firmware = Some(FirmwareSource {
            kind: FirmwareSourceKind::Hex,
            path: PathBuf::from("/tmp/air_node.hex"),
            compiled_hex_path: None,
        });
        air_node.ports = vec![
            PortDefinition::new("CANH", PortClass::Bus, PortDirection::Bidirectional),
            PortDefinition::new("CANL", PortClass::Bus, PortDirection::Bidirectional),
            PortDefinition::new("+24V", PortClass::Power, PortDirection::Input),
            PortDefinition::new("GND", PortClass::Power, PortDirection::Passive),
        ];
        bundle.children.push(air_node);
        bundle.attachments = vec![
            AttachmentBinding {
                from: AttachmentEndpoint::primary("CANH"),
                to: AttachmentEndpoint::child("air_node_1", "CANH"),
                mode: BindingMode::Bus,
                note: None,
            },
            AttachmentBinding {
                from: AttachmentEndpoint::primary("CANL"),
                to: AttachmentEndpoint::child("air_node_1", "CANL"),
                mode: BindingMode::Bus,
                note: None,
            },
        ];
        bundle.exports = vec![AssemblyExport {
            name: "bus_canh".to_string(),
            source: AttachmentEndpoint::primary("CANH"),
            aliases: vec!["CAN_H".to_string()],
            note: None,
        }];
        bundle
    }

    #[test]
    fn bundle_round_trips_through_json_document() {
        let bundle = sample_bundle();
        let json = bundle.to_json_pretty().expect("json");
        assert!(json.contains("\"kind\": \"assembly_bundle\""));
        let decoded = AssemblyBundle::load_json_from_str(&json).expect("decode");
        assert_eq!(decoded, bundle);
    }

    #[test]
    fn bundle_save_and_load_work() {
        let bundle = sample_bundle();
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("board_bundle.avrsim.json");
        bundle.save_json(&path).expect("save");
        let loaded = AssemblyBundle::load_json(&path).expect("load");
        assert_eq!(loaded, bundle);
    }

    #[test]
    fn bundle_validation_rejects_duplicate_child_ids() {
        let mut bundle = sample_bundle();
        let child = bundle.children[0].clone();
        bundle.children.push(child);
        let error = bundle.validate().expect_err("duplicate");
        assert!(matches!(error, ProjectError::DuplicateInstanceId(_)));
    }

    #[test]
    fn bundle_validation_rejects_unknown_attachment_ports() {
        let mut bundle = sample_bundle();
        bundle.attachments[0].to.port = "NOPE".to_string();
        let error = bundle.validate().expect_err("bad port");
        assert!(matches!(error, ProjectError::UnknownChildPort { .. }));
    }

    impl AssemblyBundle {
        fn load_json_from_str(text: &str) -> Result<Self, ProjectError> {
            match serde_json::from_str::<crate::document::AvrSimDocument>(text)? {
                crate::document::AvrSimDocument::AssemblyBundle(bundle) => Ok(bundle),
                other => Err(ProjectError::UnexpectedDocumentKind(
                    other.kind_name().to_string(),
                )),
            }
        }
    }
}
