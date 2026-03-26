use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use crate::assembly_bundle::{AssemblyBundle, AssemblyMember};
use crate::definitions::{
    AssemblyDefinition, AttachedInstance, BoardDefinition, DefinitionReference, DefinitionSource,
    ModuleDefinition,
};
use crate::document::AvrSimDocument;
use crate::shared::{FirmwareSource, PcbSource};
use crate::simulation_project::SimulationProject;

pub(crate) fn prepare_document_for_save(document: &mut AvrSimDocument, document_path: &Path) {
    let base_dir = document_base_dir(document_path);
    match document {
        AvrSimDocument::SimulationProject(project) => {
            prepare_simulation_project_for_save(project, &base_dir);
        }
        AvrSimDocument::BoardDefinition(definition) => {
            prepare_board_definition_for_save(definition, &base_dir);
        }
        AvrSimDocument::ModuleDefinition(definition) => {
            prepare_module_definition_for_save(definition, &base_dir);
        }
        AvrSimDocument::AssemblyDefinition(definition) => {
            prepare_assembly_definition_for_save(definition, &base_dir);
        }
        AvrSimDocument::AssemblyBundle(bundle) => {
            prepare_assembly_bundle_for_save(bundle, &base_dir)
        }
        AvrSimDocument::BehaviorDefinition(_) => {}
    }
}

pub(crate) fn resolve_document_after_load(document: &mut AvrSimDocument, document_path: &Path) {
    let base_dir = document_base_dir(document_path);
    match document {
        AvrSimDocument::SimulationProject(project) => {
            resolve_simulation_project_after_load_at_base(project, &base_dir);
        }
        AvrSimDocument::BoardDefinition(definition) => {
            resolve_board_definition_after_load(definition, &base_dir);
        }
        AvrSimDocument::ModuleDefinition(definition) => {
            resolve_module_definition_after_load(definition, &base_dir);
        }
        AvrSimDocument::AssemblyDefinition(definition) => {
            resolve_assembly_definition_after_load(definition, &base_dir);
        }
        AvrSimDocument::AssemblyBundle(bundle) => {
            resolve_assembly_bundle_after_load(bundle, &base_dir)
        }
        AvrSimDocument::BehaviorDefinition(_) => {}
    }
}

pub(crate) fn resolve_simulation_project_after_load(
    project: &mut SimulationProject,
    document_path: &Path,
) {
    let base_dir = document_base_dir(document_path);
    resolve_simulation_project_after_load_at_base(project, &base_dir);
}

fn prepare_simulation_project_for_save(project: &mut SimulationProject, base_dir: &Path) {
    prepare_firmware_source_for_save(&mut project.firmware, base_dir);
    prepare_pcb_source_for_save(&mut project.pcb, base_dir);
    if let Some(reference) = &mut project.root_assembly {
        prepare_definition_reference_for_save(reference, base_dir);
    }
}

fn resolve_simulation_project_after_load_at_base(project: &mut SimulationProject, base_dir: &Path) {
    resolve_firmware_source_after_load(&mut project.firmware, base_dir);
    resolve_pcb_source_after_load(&mut project.pcb, base_dir);
    if let Some(reference) = &mut project.root_assembly {
        resolve_definition_reference_after_load(reference, base_dir);
    }
}

fn prepare_board_definition_for_save(definition: &mut BoardDefinition, base_dir: &Path) {
    prepare_definition_source_for_save(&mut definition.source, base_dir);
    if let Some(firmware) = &mut definition.default_firmware {
        prepare_firmware_source_for_save(firmware, base_dir);
    }
}

fn resolve_board_definition_after_load(definition: &mut BoardDefinition, base_dir: &Path) {
    resolve_definition_source_after_load(&mut definition.source, base_dir);
    if let Some(firmware) = &mut definition.default_firmware {
        resolve_firmware_source_after_load(firmware, base_dir);
    }
}

fn prepare_module_definition_for_save(definition: &mut ModuleDefinition, base_dir: &Path) {
    prepare_definition_source_for_save(&mut definition.source, base_dir);
    if let Some(firmware) = &mut definition.default_firmware {
        prepare_firmware_source_for_save(firmware, base_dir);
    }
}

fn resolve_module_definition_after_load(definition: &mut ModuleDefinition, base_dir: &Path) {
    resolve_definition_source_after_load(&mut definition.source, base_dir);
    if let Some(firmware) = &mut definition.default_firmware {
        resolve_firmware_source_after_load(firmware, base_dir);
    }
}

fn prepare_assembly_definition_for_save(definition: &mut AssemblyDefinition, base_dir: &Path) {
    prepare_definition_reference_for_save(&mut definition.primary, base_dir);
    for child in &mut definition.children {
        prepare_attached_instance_for_save(child, base_dir);
    }
}

fn resolve_assembly_definition_after_load(definition: &mut AssemblyDefinition, base_dir: &Path) {
    resolve_definition_reference_after_load(&mut definition.primary, base_dir);
    for child in &mut definition.children {
        resolve_attached_instance_after_load(child, base_dir);
    }
}

fn prepare_assembly_bundle_for_save(bundle: &mut AssemblyBundle, base_dir: &Path) {
    prepare_assembly_member_for_save(&mut bundle.primary, base_dir);
    for child in &mut bundle.children {
        prepare_assembly_member_for_save(child, base_dir);
    }
}

fn resolve_assembly_bundle_after_load(bundle: &mut AssemblyBundle, base_dir: &Path) {
    resolve_assembly_member_after_load(&mut bundle.primary, base_dir);
    for child in &mut bundle.children {
        resolve_assembly_member_after_load(child, base_dir);
    }
}

fn prepare_attached_instance_for_save(instance: &mut AttachedInstance, base_dir: &Path) {
    prepare_definition_reference_for_save(&mut instance.reference, base_dir);
    if let Some(firmware) = &mut instance.firmware_override {
        prepare_firmware_source_for_save(firmware, base_dir);
    }
}

fn resolve_attached_instance_after_load(instance: &mut AttachedInstance, base_dir: &Path) {
    resolve_definition_reference_after_load(&mut instance.reference, base_dir);
    if let Some(firmware) = &mut instance.firmware_override {
        resolve_firmware_source_after_load(firmware, base_dir);
    }
}

fn prepare_assembly_member_for_save(member: &mut AssemblyMember, base_dir: &Path) {
    prepare_definition_source_for_save(&mut member.source, base_dir);
    if let Some(firmware) = &mut member.firmware {
        prepare_firmware_source_for_save(firmware, base_dir);
    }
    if let Some(reference) = &mut member.behavior {
        prepare_definition_reference_for_save(reference, base_dir);
    }
}

fn resolve_assembly_member_after_load(member: &mut AssemblyMember, base_dir: &Path) {
    resolve_definition_source_after_load(&mut member.source, base_dir);
    if let Some(firmware) = &mut member.firmware {
        resolve_firmware_source_after_load(firmware, base_dir);
    }
    if let Some(reference) = &mut member.behavior {
        resolve_definition_reference_after_load(reference, base_dir);
    }
}

fn prepare_definition_source_for_save(source: &mut DefinitionSource, base_dir: &Path) {
    if let Some(path) = &mut source.path {
        make_path_relative(path, base_dir);
    }
}

fn resolve_definition_source_after_load(source: &mut DefinitionSource, base_dir: &Path) {
    if let Some(path) = &mut source.path {
        resolve_relative_path(path, base_dir);
    }
}

fn prepare_definition_reference_for_save(reference: &mut DefinitionReference, base_dir: &Path) {
    if let Some(path) = &mut reference.path {
        make_path_relative(path, base_dir);
    }
}

fn resolve_definition_reference_after_load(reference: &mut DefinitionReference, base_dir: &Path) {
    if let Some(path) = &mut reference.path {
        resolve_relative_path(path, base_dir);
    }
}

fn prepare_firmware_source_for_save(firmware: &mut FirmwareSource, base_dir: &Path) {
    make_path_relative(&mut firmware.path, base_dir);
    if let Some(path) = &mut firmware.compiled_hex_path {
        make_path_relative(path, base_dir);
    }
}

fn resolve_firmware_source_after_load(firmware: &mut FirmwareSource, base_dir: &Path) {
    resolve_relative_path(&mut firmware.path, base_dir);
    if let Some(path) = &mut firmware.compiled_hex_path {
        resolve_relative_path(path, base_dir);
    }
}

fn prepare_pcb_source_for_save(pcb: &mut PcbSource, base_dir: &Path) {
    make_path_relative(&mut pcb.path, base_dir);
}

fn resolve_pcb_source_after_load(pcb: &mut PcbSource, base_dir: &Path) {
    resolve_relative_path(&mut pcb.path, base_dir);
}

fn make_path_relative(path: &mut PathBuf, base_dir: &Path) {
    if path.as_os_str().is_empty() || path.is_relative() {
        return;
    }
    let absolute = absolutize_lexically(path);
    if let Some(relative) = diff_paths(&absolute, base_dir) {
        *path = relative;
    } else {
        *path = absolute;
    }
}

fn resolve_relative_path(path: &mut PathBuf, base_dir: &Path) {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return;
    }
    *path = normalize_lexical(&base_dir.join(&*path));
}

fn document_base_dir(document_path: &Path) -> PathBuf {
    let base = document_path.parent().unwrap_or_else(|| Path::new("."));
    absolutize_lexically(base)
}

fn absolutize_lexically(path: &Path) -> PathBuf {
    if path.is_absolute() {
        normalize_lexical(path)
    } else if let Ok(current_dir) = std::env::current_dir() {
        normalize_lexical(&current_dir.join(path))
    } else {
        normalize_lexical(path)
    }
}

fn diff_paths(path: &Path, base_dir: &Path) -> Option<PathBuf> {
    let (path_prefix, path_root, path_parts) = split_path_components(path);
    let (base_prefix, base_root, base_parts) = split_path_components(base_dir);
    if path_prefix != base_prefix || path_root != base_root {
        return None;
    }

    let mut common = 0usize;
    while common < path_parts.len()
        && common < base_parts.len()
        && path_parts[common] == base_parts[common]
    {
        common += 1;
    }

    let mut relative = PathBuf::new();
    for _ in common..base_parts.len() {
        relative.push("..");
    }
    for part in &path_parts[common..] {
        relative.push(part);
    }

    if relative.as_os_str().is_empty() {
        Some(PathBuf::from("."))
    } else {
        Some(relative)
    }
}

fn split_path_components(path: &Path) -> (Option<OsString>, bool, Vec<OsString>) {
    let normalized = normalize_lexical(path);
    let mut prefix = None;
    let mut has_root = false;
    let mut parts = Vec::new();
    for component in normalized.components() {
        match component {
            Component::Prefix(value) => prefix = Some(value.as_os_str().to_os_string()),
            Component::RootDir => has_root = true,
            Component::Normal(value) => parts.push(value.to_os_string()),
            Component::CurDir | Component::ParentDir => {}
        }
    }
    (prefix, has_root, parts)
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut prefix = None;
    let mut has_root = false;
    let mut parts: Vec<OsString> = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(value) => prefix = Some(value.as_os_str().to_os_string()),
            Component::RootDir => has_root = true,
            Component::CurDir => {}
            Component::ParentDir => match parts.last() {
                Some(last) if last != ".." => {
                    parts.pop();
                }
                _ if !has_root => parts.push(OsString::from("..")),
                _ => {}
            },
            Component::Normal(value) => parts.push(value.to_os_string()),
        }
    }

    let mut normalized = PathBuf::new();
    if let Some(prefix) = prefix {
        normalized.push(prefix);
    }
    if has_root {
        normalized.push(Path::new(std::path::MAIN_SEPARATOR_STR));
    }
    for part in parts {
        normalized.push(part);
    }

    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::{prepare_document_for_save, resolve_document_after_load};
    use crate::assembly_bundle::{AssemblyBundle, AssemblyMember, AssemblyMemberKind};
    use crate::definitions::{
        DefinitionReference, DefinitionReferenceKind, DefinitionSource, PortClass, PortDefinition,
        PortDirection,
    };
    use crate::document::AvrSimDocument;
    use crate::shared::{FirmwareSource, FirmwareSourceKind, HostBoard, PcbSource};
    use crate::simulation_project::SimulationProject;

    #[test]
    fn simulation_project_paths_are_saved_relative_and_resolved_on_load() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("workspace");
        let document_path = root.join("docs/controller.avrsim.json");

        let project = SimulationProject {
            format_version: crate::PROJECT_FORMAT_VERSION.to_string(),
            name: "Controller".to_string(),
            description: None,
            host_board: HostBoard::Mega2560Rev3,
            firmware: FirmwareSource {
                kind: FirmwareSourceKind::Ino,
                path: root.join("firmware/dewpoint_controller.ino"),
                compiled_hex_path: Some(root.join("build/dewpoint_controller.ino.hex")),
            },
            pcb: PcbSource {
                path: root.join(
                    "pcb/mega_r3_sidecar_controller_rev_a/mega_r3_sidecar_controller_rev_a.kicad_pcb",
                ),
                board_name_hint: Some("mega_r3_sidecar_controller_rev_a".to_string()),
            },
            root_assembly: Some(DefinitionReference::file(
                DefinitionReferenceKind::AssemblyBundle,
                root.join("assemblies/controller.board.avrsim.json"),
            )),
            module_overlays: Vec::new(),
            bindings: Vec::new(),
            probes: Vec::new(),
            stimuli: Vec::new(),
        };

        let mut document = AvrSimDocument::SimulationProject(project.clone());
        prepare_document_for_save(&mut document, &document_path);
        let json = serde_json::to_string_pretty(&document).expect("json");

        assert!(json.contains("../firmware/dewpoint_controller.ino"));
        assert!(json.contains("../build/dewpoint_controller.ino.hex"));
        assert!(json.contains(
            "../pcb/mega_r3_sidecar_controller_rev_a/mega_r3_sidecar_controller_rev_a.kicad_pcb"
        ));
        assert!(json.contains("../assemblies/controller.board.avrsim.json"));
        assert!(!json.contains(root.to_string_lossy().as_ref()));

        resolve_document_after_load(&mut document, &document_path);
        assert_eq!(document, AvrSimDocument::SimulationProject(project));
    }

    #[test]
    fn assembly_bundle_paths_are_saved_relative_and_resolved_on_load() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("workspace");
        let document_path = root.join("docs/controller_stack.board.avrsim.json");

        let mut primary = AssemblyMember::new(
            "primary",
            AssemblyMemberKind::Board,
            "controller",
            DefinitionSource::kicad_pcb(root.join("pcb/controller.kicad_pcb")),
        );
        primary.embedded_host_board = Some(HostBoard::Mega2560Rev3);
        primary.firmware = Some(FirmwareSource {
            kind: FirmwareSourceKind::Ino,
            path: root.join("firmware/controller.ino"),
            compiled_hex_path: Some(root.join("build/controller.ino.hex")),
        });
        primary.ports = vec![PortDefinition::new(
            "CANH",
            PortClass::Bus,
            PortDirection::Bidirectional,
        )];

        let mut child = AssemblyMember::new(
            "sensor_1",
            AssemblyMemberKind::Module,
            "sensor",
            DefinitionSource::kicad_pcb(root.join("pcb/sensor.kicad_pcb")),
        );
        child.behavior = Some(DefinitionReference::file(
            DefinitionReferenceKind::BehaviorDefinition,
            root.join("behaviors/sensor.behavior.avrsim.json"),
        ));

        let mut bundle = AssemblyBundle::new("Controller Stack", primary);
        bundle.children.push(child);

        let mut document = AvrSimDocument::AssemblyBundle(bundle.clone());
        prepare_document_for_save(&mut document, &document_path);
        let json = serde_json::to_string_pretty(&document).expect("json");

        assert!(json.contains("../pcb/controller.kicad_pcb"));
        assert!(json.contains("../firmware/controller.ino"));
        assert!(json.contains("../build/controller.ino.hex"));
        assert!(json.contains("../pcb/sensor.kicad_pcb"));
        assert!(json.contains("../behaviors/sensor.behavior.avrsim.json"));
        assert!(!json.contains(root.to_string_lossy().as_ref()));

        resolve_document_after_load(&mut document, &document_path);
        assert_eq!(document, AvrSimDocument::AssemblyBundle(bundle));
    }

    #[test]
    fn relative_paths_are_resolved_without_touching_builtin_references() {
        let document_path = PathBuf::from("/tmp/project/docs/controller.board.avrsim.json");
        let bundle = AssemblyBundle::new(
            "Controller Stack",
            AssemblyMember::new(
                "primary",
                AssemblyMemberKind::Board,
                "controller",
                DefinitionSource::builtin_board_model("arduino_mega_2560_rev3"),
            ),
        );
        let mut document = AvrSimDocument::AssemblyBundle(bundle.clone());

        resolve_document_after_load(&mut document, &document_path);

        assert_eq!(document, AvrSimDocument::AssemblyBundle(bundle));
    }
}
