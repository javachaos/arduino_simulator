pub mod assembly_bundle;
pub mod autobind;
pub mod behavior_definition;
pub mod definitions;
pub mod document;
pub mod error;
mod paths;
pub mod shared;
pub mod simulation_project;

pub use assembly_bundle::{AssemblyBundle, AssemblyMember, AssemblyMemberKind};
pub use autobind::{
    auto_bind_host_board, controller_signal_suggestions, default_project_name,
    host_signals_for_board, infer_binding_mode, inferred_host_board_from_source,
    sync_host_board_bindings, NetSuggestion,
};
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
