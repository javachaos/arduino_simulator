use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::assembly_bundle::AssemblyBundle;
use crate::behavior_definition::BehaviorDefinition;
use crate::definitions::{AssemblyDefinition, BoardDefinition, ModuleDefinition};
use crate::error::ProjectError;
use crate::simulation_project::SimulationProject;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AvrSimDocument {
    SimulationProject(SimulationProject),
    BoardDefinition(BoardDefinition),
    ModuleDefinition(ModuleDefinition),
    AssemblyDefinition(AssemblyDefinition),
    AssemblyBundle(AssemblyBundle),
    BehaviorDefinition(BehaviorDefinition),
}

impl AvrSimDocument {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::SimulationProject(_) => "simulation_project",
            Self::BoardDefinition(_) => "board_definition",
            Self::ModuleDefinition(_) => "module_definition",
            Self::AssemblyDefinition(_) => "assembly_definition",
            Self::AssemblyBundle(_) => "assembly_bundle",
            Self::BehaviorDefinition(_) => "behavior_definition",
        }
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        match self {
            Self::SimulationProject(project) => project.validate(),
            Self::BoardDefinition(definition) => definition.validate(),
            Self::ModuleDefinition(definition) => definition.validate(),
            Self::AssemblyDefinition(definition) => definition.validate(),
            Self::AssemblyBundle(bundle) => bundle.validate(),
            Self::BehaviorDefinition(definition) => definition.validate(),
        }
    }

    pub fn to_json_pretty(&self) -> Result<String, ProjectError> {
        Ok(serde_json::to_string_pretty(self)? + "\n")
    }

    pub fn save_json(&self, path: &Path) -> Result<(), ProjectError> {
        fs::write(path, self.to_json_pretty()?)?;
        Ok(())
    }

    pub fn load_json(path: &Path) -> Result<Self, ProjectError> {
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::AvrSimDocument;
    use crate::definitions::{BoardDefinition, DefinitionSource};

    #[test]
    fn document_kind_names_are_stable() {
        let document = AvrSimDocument::BoardDefinition(BoardDefinition::new(
            "carrier",
            DefinitionSource::virtual_only(),
        ));
        assert_eq!(document.kind_name(), "board_definition");
    }

    #[test]
    fn tagged_document_json_contains_kind() {
        let document = AvrSimDocument::BoardDefinition(BoardDefinition::new(
            "carrier",
            DefinitionSource::kicad_pcb(PathBuf::from("/tmp/board.kicad_pcb")),
        ));
        let json = document.to_json_pretty().expect("json");
        assert!(json.contains("\"kind\": \"board_definition\""));
    }
}
