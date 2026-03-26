use std::fmt;

#[derive(Debug)]
pub enum ProjectError {
    Io(std::io::Error),
    Json(serde_json::Error),
    EmptyName(&'static str),
    MissingFirmwarePath,
    MissingPcbPath,
    UnknownBoardModel(String),
    UnknownModuleModel(String),
    UnknownBoardSignal(String),
    DuplicateBoardSignal(String),
    DuplicateModuleOverlay(String),
    EmptyModuleSignal(String),
    DuplicateModuleSignal { module_name: String, signal: String },
    EmptyPcbNet(String),
    MissingDefinitionSource(String),
    InvalidDefinitionSource(String),
    InvalidDefinitionReference(String),
    DuplicateBehaviorPort(String),
    DuplicateBehaviorRole(String),
    EmptyBehaviorRole,
    InvalidBehaviorParameter(String),
    DuplicatePortName(String),
    EmptyPortName,
    DuplicateInstanceId(String),
    UnknownInstanceId(String),
    UnknownPrimaryPort(String),
    UnknownChildPort { instance_id: String, port: String },
    DuplicateExportName(String),
    UnexpectedDocumentKind(String),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Json(error) => write!(f, "{error}"),
            Self::EmptyName(kind) => write!(f, "{kind} name must not be empty"),
            Self::MissingFirmwarePath => write!(f, "firmware source path must not be empty"),
            Self::MissingPcbPath => write!(f, "PCB source path must not be empty"),
            Self::UnknownBoardModel(model) => write!(f, "unknown built-in board model {model}"),
            Self::UnknownModuleModel(model) => write!(f, "unknown module overlay model {model}"),
            Self::UnknownBoardSignal(signal) => write!(f, "unknown host-board signal {signal}"),
            Self::DuplicateBoardSignal(signal) => {
                write!(f, "duplicate binding for host-board signal {signal}")
            }
            Self::DuplicateModuleOverlay(name) => {
                write!(f, "duplicate module overlay named {name}")
            }
            Self::EmptyModuleSignal(module_name) => {
                write!(
                    f,
                    "module overlay {module_name} contains an empty signal binding"
                )
            }
            Self::DuplicateModuleSignal {
                module_name,
                signal,
            } => {
                write!(
                    f,
                    "module overlay {module_name} binds signal {signal} more than once"
                )
            }
            Self::EmptyPcbNet(signal) => {
                write!(f, "binding for {signal} does not specify a PCB net")
            }
            Self::MissingDefinitionSource(name) => {
                write!(f, "definition source for {name} is incomplete")
            }
            Self::InvalidDefinitionSource(message) => write!(f, "{message}"),
            Self::InvalidDefinitionReference(message) => write!(f, "{message}"),
            Self::DuplicateBehaviorPort(port) => {
                write!(f, "duplicate behavior port binding for {port}")
            }
            Self::DuplicateBehaviorRole(role) => {
                write!(f, "duplicate behavior role binding for {role}")
            }
            Self::EmptyBehaviorRole => write!(f, "behavior role must not be empty"),
            Self::InvalidBehaviorParameter(parameter) => {
                write!(f, "invalid behavior parameter {parameter}")
            }
            Self::DuplicatePortName(port) => write!(f, "duplicate port name {port}"),
            Self::EmptyPortName => write!(f, "port name must not be empty"),
            Self::DuplicateInstanceId(instance_id) => {
                write!(f, "duplicate attached instance id {instance_id}")
            }
            Self::UnknownInstanceId(instance_id) => {
                write!(f, "unknown attached instance id {instance_id}")
            }
            Self::UnknownPrimaryPort(port) => write!(f, "unknown primary port {port}"),
            Self::UnknownChildPort { instance_id, port } => {
                write!(f, "unknown port {port} on attached instance {instance_id}")
            }
            Self::DuplicateExportName(name) => write!(f, "duplicate assembly export {name}"),
            Self::UnexpectedDocumentKind(kind) => {
                write!(f, "document kind {kind} does not match the requested type")
            }
        }
    }
}

impl std::error::Error for ProjectError {}

impl From<std::io::Error> for ProjectError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for ProjectError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}
