pub mod cli;
pub mod firmware;
pub mod runtime;
pub mod simulator;
pub mod tui;

pub use cli::{run_cli, CliError};
pub use firmware::{load_hex_file, load_hex_into_cpu, HexLoadError};
pub use runtime::{MegaRuntime, NanoRuntime, RuntimeExit};
pub use simulator::{
    SimulationSnapshot, SimulatorBoard, SimulatorConfig, SimulatorCore, SimulatorError,
};
pub use tui::{
    default_chunk_size as monitor_default_chunk_size,
    default_refresh_ms as monitor_default_refresh_ms, monitor_mega, monitor_nano, MonitorConfig,
};
