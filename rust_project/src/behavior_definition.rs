use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::document::AvrSimDocument;
use crate::error::ProjectError;
use crate::shared::PROJECT_FORMAT_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorEngine {
    Sht31I2cSensor,
    Mcp2515CanModule,
    Max31865RtdFrontend,
    PwmToVoltage,
}

impl BehaviorEngine {
    pub fn label(self) -> &'static str {
        match self {
            Self::Sht31I2cSensor => "SHT31 I2C Sensor",
            Self::Mcp2515CanModule => "MCP2515 CAN Module",
            Self::Max31865RtdFrontend => "MAX31865 RTD Frontend",
            Self::PwmToVoltage => "PWM to Voltage",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BehaviorValue {
    Bool(bool),
    Integer(i64),
    Float(f64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorPortBinding {
    pub board_port: String,
    pub role: String,
    pub note: Option<String>,
}

impl BehaviorPortBinding {
    pub fn new(board_port: impl Into<String>, role: impl Into<String>) -> Self {
        Self {
            board_port: board_port.into(),
            role: role.into(),
            note: None,
        }
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.board_port.trim().is_empty() {
            return Err(ProjectError::EmptyPortName);
        }
        if self.role.trim().is_empty() {
            return Err(ProjectError::EmptyBehaviorRole);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehaviorDefinition {
    pub format_version: String,
    pub name: String,
    pub description: Option<String>,
    pub engine: BehaviorEngine,
    pub ports: Vec<BehaviorPortBinding>,
    pub parameters: BTreeMap<String, BehaviorValue>,
}

impl BehaviorDefinition {
    pub fn new(name: impl Into<String>, engine: BehaviorEngine) -> Self {
        Self {
            format_version: PROJECT_FORMAT_VERSION.to_string(),
            name: name.into(),
            description: None,
            engine,
            ports: Vec::new(),
            parameters: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.name.trim().is_empty() {
            return Err(ProjectError::EmptyName("behavior definition"));
        }

        let mut board_ports = BTreeSet::new();
        let mut roles = BTreeSet::new();
        for port in &self.ports {
            port.validate()?;
            if !board_ports.insert(port.board_port.clone()) {
                return Err(ProjectError::DuplicateBehaviorPort(port.board_port.clone()));
            }
            if !roles.insert(port.role.clone()) {
                return Err(ProjectError::DuplicateBehaviorRole(port.role.clone()));
            }
        }

        for parameter in self.parameters.keys() {
            if parameter.trim().is_empty() {
                return Err(ProjectError::InvalidBehaviorParameter(parameter.clone()));
            }
        }

        Ok(())
    }

    pub fn to_json_pretty(&self) -> Result<String, ProjectError> {
        AvrSimDocument::BehaviorDefinition(self.clone()).to_json_pretty()
    }

    pub fn save_json(&self, path: &Path) -> Result<(), ProjectError> {
        AvrSimDocument::BehaviorDefinition(self.clone()).save_json(path)
    }

    pub fn load_json(path: &Path) -> Result<Self, ProjectError> {
        match AvrSimDocument::load_json(path)? {
            AvrSimDocument::BehaviorDefinition(definition) => Ok(definition),
            other => Err(ProjectError::UnexpectedDocumentKind(
                other.kind_name().to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{BehaviorDefinition, BehaviorEngine, BehaviorPortBinding, BehaviorValue};
    use crate::error::ProjectError;

    fn sample_behavior() -> BehaviorDefinition {
        let mut definition =
            BehaviorDefinition::new("gy_sht31_d_behavior", BehaviorEngine::Sht31I2cSensor);
        definition.ports = vec![
            BehaviorPortBinding::new("SDA", "sda"),
            BehaviorPortBinding::new("SCL", "scl"),
            BehaviorPortBinding::new("VCC", "vcc"),
            BehaviorPortBinding::new("GND", "gnd"),
        ];
        definition
            .parameters
            .insert("address".to_string(), BehaviorValue::Integer(0x44));
        definition.parameters.insert(
            "measurement_delay_ms".to_string(),
            BehaviorValue::Integer(15),
        );
        definition
    }

    #[test]
    fn behavior_definition_round_trips_and_validates() {
        let definition = sample_behavior();
        definition.validate().expect("valid");
        let json = definition.to_json_pretty().expect("json");
        assert!(json.contains("\"kind\": \"behavior_definition\""));
    }

    #[test]
    fn behavior_definition_save_and_load_work() {
        let definition = sample_behavior();
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("behavior.avrsim.json");
        definition.save_json(&path).expect("save");
        let loaded = BehaviorDefinition::load_json(&path).expect("load");
        assert_eq!(loaded, definition);
    }

    #[test]
    fn behavior_definition_rejects_duplicate_ports_and_roles() {
        let mut definition = sample_behavior();
        definition
            .ports
            .push(BehaviorPortBinding::new("SDA", "spare"));
        assert!(matches!(
            definition.validate().expect_err("duplicate port"),
            ProjectError::DuplicateBehaviorPort(_)
        ));

        let mut definition = sample_behavior();
        definition
            .ports
            .push(BehaviorPortBinding::new("ADDR", "sda"));
        assert!(matches!(
            definition.validate().expect_err("duplicate role"),
            ProjectError::DuplicateBehaviorRole(_)
        ));
    }
}
