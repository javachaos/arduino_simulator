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

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::ProjectError;

    #[test]
    fn display_messages_cover_every_project_error_variant() {
        let io_error = std::io::Error::other("disk offline");
        let io_expected = io_error.to_string();

        let json_error = serde_json::from_str::<Value>("{").expect_err("invalid json");
        let json_expected = json_error.to_string();

        let cases = vec![
            (ProjectError::Io(io_error), io_expected),
            (ProjectError::Json(json_error), json_expected),
            (
                ProjectError::EmptyName("board definition"),
                "board definition name must not be empty".to_string(),
            ),
            (
                ProjectError::MissingFirmwarePath,
                "firmware source path must not be empty".to_string(),
            ),
            (
                ProjectError::MissingPcbPath,
                "PCB source path must not be empty".to_string(),
            ),
            (
                ProjectError::UnknownBoardModel("mystery".to_string()),
                "unknown built-in board model mystery".to_string(),
            ),
            (
                ProjectError::UnknownModuleModel("sensor".to_string()),
                "unknown module overlay model sensor".to_string(),
            ),
            (
                ProjectError::UnknownBoardSignal("A7".to_string()),
                "unknown host-board signal A7".to_string(),
            ),
            (
                ProjectError::DuplicateBoardSignal("D13".to_string()),
                "duplicate binding for host-board signal D13".to_string(),
            ),
            (
                ProjectError::DuplicateModuleOverlay("display".to_string()),
                "duplicate module overlay named display".to_string(),
            ),
            (
                ProjectError::EmptyModuleSignal("sensor".to_string()),
                "module overlay sensor contains an empty signal binding".to_string(),
            ),
            (
                ProjectError::DuplicateModuleSignal {
                    module_name: "sensor".to_string(),
                    signal: "SCL".to_string(),
                },
                "module overlay sensor binds signal SCL more than once".to_string(),
            ),
            (
                ProjectError::EmptyPcbNet("D2".to_string()),
                "binding for D2 does not specify a PCB net".to_string(),
            ),
            (
                ProjectError::MissingDefinitionSource("carrier".to_string()),
                "definition source for carrier is incomplete".to_string(),
            ),
            (
                ProjectError::InvalidDefinitionSource("bad source".to_string()),
                "bad source".to_string(),
            ),
            (
                ProjectError::InvalidDefinitionReference("bad reference".to_string()),
                "bad reference".to_string(),
            ),
            (
                ProjectError::DuplicateBehaviorPort("uart_tx".to_string()),
                "duplicate behavior port binding for uart_tx".to_string(),
            ),
            (
                ProjectError::DuplicateBehaviorRole("sensor".to_string()),
                "duplicate behavior role binding for sensor".to_string(),
            ),
            (
                ProjectError::EmptyBehaviorRole,
                "behavior role must not be empty".to_string(),
            ),
            (
                ProjectError::InvalidBehaviorParameter("baud".to_string()),
                "invalid behavior parameter baud".to_string(),
            ),
            (
                ProjectError::DuplicatePortName("VIN".to_string()),
                "duplicate port name VIN".to_string(),
            ),
            (
                ProjectError::EmptyPortName,
                "port name must not be empty".to_string(),
            ),
            (
                ProjectError::DuplicateInstanceId("rtc".to_string()),
                "duplicate attached instance id rtc".to_string(),
            ),
            (
                ProjectError::UnknownInstanceId("rtc".to_string()),
                "unknown attached instance id rtc".to_string(),
            ),
            (
                ProjectError::UnknownPrimaryPort("CANH".to_string()),
                "unknown primary port CANH".to_string(),
            ),
            (
                ProjectError::UnknownChildPort {
                    instance_id: "sensor".to_string(),
                    port: "SDA".to_string(),
                },
                "unknown port SDA on attached instance sensor".to_string(),
            ),
            (
                ProjectError::DuplicateExportName("telemetry".to_string()),
                "duplicate assembly export telemetry".to_string(),
            ),
            (
                ProjectError::UnexpectedDocumentKind("module_definition".to_string()),
                "document kind module_definition does not match the requested type".to_string(),
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn io_and_json_errors_convert_via_from_impls() {
        let io_error: ProjectError = std::io::Error::other("permission denied").into();
        assert!(matches!(io_error, ProjectError::Io(_)));

        let json_source = serde_json::from_str::<Value>("{").expect_err("invalid json");
        let json_error: ProjectError = json_source.into();
        assert!(matches!(json_error, ProjectError::Json(_)));
    }
}
