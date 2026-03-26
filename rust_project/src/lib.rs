pub mod assembly_bundle;
pub mod behavior_definition;
pub mod definitions;
pub mod document;
pub mod error;
pub mod shared;
pub mod simulation_project;

pub use assembly_bundle::{AssemblyBundle, AssemblyMember, AssemblyMemberKind};
pub use behavior_definition::{
    BehaviorDefinition, BehaviorEngine, BehaviorPortBinding, BehaviorValue,
};
pub use definitions::{
    AssemblyDefinition, AssemblyExport, AttachedInstance, AttachmentBinding, AttachmentEndpoint,
    BoardDefinition, DefinitionReference, DefinitionReferenceKind, DefinitionSource,
    DefinitionSourceKind, ModuleDefinition, PortClass, PortDefinition, PortDirection,
};
pub use document::AvrSimDocument;
pub use error::ProjectError;
pub use shared::{
    validate_host_board_signal_bindings, validate_module_overlays, BindingMode, FirmwareSource,
    FirmwareSourceKind, HostBoard, ModuleOverlay, ModuleSignalBinding, PcbSource, ProbeKind,
    ProbeSpec, SignalBinding, StimulusKind, StimulusSpec, PROJECT_FORMAT_VERSION,
};
pub use simulation_project::SimulationProject;
