use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::assembly_bundle::AssemblyBundle;
use crate::behavior_definition::BehaviorDefinition;
use crate::definitions::{AssemblyDefinition, BoardDefinition, ModuleDefinition};
use crate::error::ProjectError;
use crate::paths::{prepare_document_for_save, resolve_document_after_load};
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
        let mut document = self.clone();
        prepare_document_for_save(&mut document, path);
        fs::write(path, serde_json::to_string_pretty(&document)? + "\n")?;
        Ok(())
    }

    pub fn load_json(path: &Path) -> Result<Self, ProjectError> {
        let mut document = serde_json::from_str(&fs::read_to_string(path)?)?;
        resolve_document_after_load(&mut document, path);
        Ok(document)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::AvrSimDocument;
    use crate::definitions::{BoardDefinition, DefinitionSource};
    use crate::error::ProjectError;
    use crate::shared::{FirmwareSource, FirmwareSourceKind};

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

    #[test]
    fn validate_delegates_to_inner_document() {
        let document = AvrSimDocument::BoardDefinition(BoardDefinition::new(
            "   ",
            DefinitionSource::virtual_only(),
        ));

        assert!(matches!(
            document.validate(),
            Err(ProjectError::EmptyName("board definition"))
        ));
    }

    #[test]
    fn save_and_load_json_round_trip_relative_paths() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("workspace");
        let document_path = root.join("docs/board.avrsim.json");
        fs::create_dir_all(document_path.parent().expect("parent")).expect("create docs dir");

        let mut definition = BoardDefinition::new(
            "carrier",
            DefinitionSource::kicad_pcb(root.join("pcb/board.kicad_pcb")),
        );
        definition.default_firmware = Some(FirmwareSource {
            kind: FirmwareSourceKind::Hex,
            path: root.join("firmware/carrier.hex"),
            compiled_hex_path: Some(root.join("build/carrier.hex")),
        });
        let document = AvrSimDocument::BoardDefinition(definition.clone());

        document.save_json(&document_path).expect("save");
        let raw = fs::read_to_string(&document_path).expect("json");
        assert!(raw.contains("../pcb/board.kicad_pcb"));
        assert!(raw.contains("../firmware/carrier.hex"));
        assert!(raw.contains("../build/carrier.hex"));
        assert!(!raw.contains(root.to_string_lossy().as_ref()));

        let loaded = AvrSimDocument::load_json(&document_path).expect("load");
        assert_eq!(loaded, document);
    }

    #[test]
    fn load_json_surfaces_json_errors() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("broken.avrsim.json");
        fs::write(&path, "{ definitely not json").expect("write invalid json");

        assert!(matches!(
            AvrSimDocument::load_json(&path),
            Err(ProjectError::Json(_))
        ));
    }
}
