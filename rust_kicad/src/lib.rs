use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use rust_board::board_from_kicad_pcb;
use rust_project::{
    auto_bind_host_board, default_project_name, inferred_host_board_from_source,
    sync_host_board_bindings, FirmwareSource, FirmwareSourceKind, HostBoard, PcbSource,
    ProjectError, SimulationProject,
};

#[derive(Debug)]
pub enum KiCadCliError {
    Usage(String),
    Io(std::io::Error),
    Project(ProjectError),
    Board(rust_board::DslError),
    UnsupportedFirmwarePath(PathBuf),
    MissingGuiBinary,
}

impl fmt::Display for KiCadCliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => write!(f, "{message}"),
            Self::Io(error) => write!(f, "{error}"),
            Self::Project(error) => write!(f, "{error}"),
            Self::Board(error) => write!(f, "{error}"),
            Self::UnsupportedFirmwarePath(path) => write!(
                f,
                "firmware path {} must point to an .ino sketch, a directory containing a sketch, or a .hex image",
                path.display()
            ),
            Self::MissingGuiBinary => write!(
                f,
                "could not find arduino-simulator-gui next to the adapter binary and could not fall back to cargo"
            ),
        }
    }
}

impl std::error::Error for KiCadCliError {}

impl From<std::io::Error> for KiCadCliError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ProjectError> for KiCadCliError {
    fn from(value: ProjectError) -> Self {
        Self::Project(value)
    }
}

impl From<rust_board::DslError> for KiCadCliError {
    fn from(value: rust_board::DslError) -> Self {
        Self::Board(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestedHostBoard {
    Auto,
    Mega,
    Nano,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateProjectOptions {
    pub pcb_path: PathBuf,
    pub firmware_path: PathBuf,
    pub out_path: Option<PathBuf>,
    pub name: Option<String>,
    pub launch_gui: bool,
    pub requested_board: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncProjectOptions {
    pub project_path: PathBuf,
    pub launch_gui: bool,
    pub requested_board: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenPcbOptions {
    pub pcb_path: PathBuf,
    pub requested_board: Option<String>,
}

pub fn run_cli(args: impl IntoIterator<Item = String>) -> Result<i32, KiCadCliError> {
    let mut iter = args.into_iter();
    let _program_name = iter.next();
    let Some(command) = iter.next() else {
        return Err(KiCadCliError::Usage(usage()));
    };

    match command.as_str() {
        "create-project" => {
            let options = parse_create_project_options(iter)?;
            create_project(&options)?;
            Ok(0)
        }
        "sync-project" => {
            let options = parse_sync_project_options(iter)?;
            sync_project(&options)?;
            Ok(0)
        }
        "open-pcb" => {
            let options = parse_open_pcb_options(iter)?;
            open_pcb(&options)?;
            Ok(0)
        }
        "open-gui" => {
            let project_path = parse_open_gui_options(iter)?;
            launch_gui_with_project(&project_path)?;
            Ok(0)
        }
        "--help" | "-h" | "help" => {
            print!("{}", usage());
            Ok(0)
        }
        other => Err(KiCadCliError::Usage(format!(
            "unknown command `{other}`\n\n{}",
            usage()
        ))),
    }
}

pub fn create_project(options: &CreateProjectOptions) -> Result<PathBuf, KiCadCliError> {
    let host_board = resolve_requested_board(
        parse_requested_board(options.requested_board.as_deref())?,
        &options.firmware_path,
        &options.pcb_path,
    );
    let board = board_from_kicad_pcb(&options.pcb_path)?;
    let available_nets = board
        .nets
        .iter()
        .map(|net| net.name.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let firmware_kind = classify_firmware_source(&options.firmware_path)?;
    let output_path = options
        .out_path
        .clone()
        .unwrap_or_else(|| default_output_path(&options.pcb_path));
    let project_name = options
        .name
        .clone()
        .unwrap_or_else(|| default_project_name(&options.firmware_path, &options.pcb_path));

    let mut project = SimulationProject::new(
        project_name,
        host_board,
        FirmwareSource {
            kind: firmware_kind,
            path: options.firmware_path.clone(),
            compiled_hex_path: None,
        },
        PcbSource {
            path: options.pcb_path.clone(),
            board_name_hint: options
                .pcb_path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(|value| value.to_string()),
        },
    );
    project.description = Some("Generated by the arduino_simulator KiCad adapter.".to_string());
    project.bindings = auto_bind_host_board(host_board, &available_nets);
    project.validate()?;
    project.save_json(&output_path)?;

    println!(
        "Created {} with {} auto-bound signal(s) for {}.",
        output_path.display(),
        project.bindings.len(),
        host_board.label()
    );

    if options.launch_gui {
        launch_gui_with_project(&output_path)?;
    }

    Ok(output_path)
}

pub fn sync_project(options: &SyncProjectOptions) -> Result<PathBuf, KiCadCliError> {
    let mut project = SimulationProject::load_json(&options.project_path)?;
    let requested = parse_requested_board(options.requested_board.as_deref())?;
    let host_board = match requested {
        RequestedHostBoard::Auto => resolve_requested_board(
            RequestedHostBoard::Auto,
            &project.firmware.path,
            &project.pcb.path,
        ),
        RequestedHostBoard::Mega => HostBoard::Mega2560Rev3,
        RequestedHostBoard::Nano => HostBoard::NanoV3,
    };
    let board = board_from_kicad_pcb(&project.pcb.path)?;
    let available_nets = board
        .nets
        .iter()
        .map(|net| net.name.clone())
        .collect::<std::collections::BTreeSet<_>>();
    project.host_board = host_board;
    project.bindings = sync_host_board_bindings(host_board, &available_nets, &project.bindings);
    project.validate()?;
    project.save_json(&options.project_path)?;

    println!(
        "Synced {} with {} binding(s) for {}.",
        options.project_path.display(),
        project.bindings.len(),
        host_board.label()
    );

    if options.launch_gui {
        launch_gui_with_project(&options.project_path)?;
    }

    Ok(options.project_path.clone())
}

pub fn open_pcb(options: &OpenPcbOptions) -> Result<PathBuf, KiCadCliError> {
    let host_board = match parse_requested_board(options.requested_board.as_deref())? {
        RequestedHostBoard::Auto => inferred_host_board_from_source(&options.pcb_path)
            .unwrap_or(HostBoard::Mega2560Rev3),
        RequestedHostBoard::Mega => HostBoard::Mega2560Rev3,
        RequestedHostBoard::Nano => HostBoard::NanoV3,
    };

    launch_gui_with_pcb(&options.pcb_path, Some(host_board))?;
    println!(
        "Opened {} in generic PCB mode (target {}).",
        options.pcb_path.display(),
        host_board.label()
    );
    Ok(options.pcb_path.clone())
}

pub fn launch_gui_with_project(project_path: &Path) -> Result<(), KiCadCliError> {
    let project = project_path.display().to_string();
    launch_gui_with_arguments(&["--project".to_string(), project])
}

pub fn launch_gui_with_pcb(
    pcb_path: &Path,
    host_board: Option<HostBoard>,
) -> Result<(), KiCadCliError> {
    let mut arguments = vec!["--pcb".to_string(), pcb_path.display().to_string()];
    if let Some(host_board) = host_board {
        arguments.push("--board".to_string());
        arguments.push(host_board.short_name().to_string());
    }
    launch_gui_with_arguments(&arguments)
}

fn launch_gui_with_arguments(arguments: &[String]) -> Result<(), KiCadCliError> {
    if let Some(gui_binary) = resolve_sibling_gui_binary() {
        Command::new(gui_binary).args(arguments).spawn()?;
        return Ok(());
    }

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or(KiCadCliError::MissingGuiBinary)?;
    if workspace_root.join("Cargo.toml").is_file() {
        let cargo = resolve_cargo_executable().unwrap_or_else(|| PathBuf::from("cargo"));
        Command::new(cargo)
            .arg("run")
            .arg("-p")
            .arg("rust_gui")
            .arg("--bin")
            .arg("arduino-simulator-gui")
            .arg("--")
            .args(arguments)
            .current_dir(workspace_root)
            .spawn()?;
        return Ok(());
    }

    Err(KiCadCliError::MissingGuiBinary)
}

fn parse_create_project_options(
    args: impl IntoIterator<Item = String>,
) -> Result<CreateProjectOptions, KiCadCliError> {
    let mut pcb_path = None;
    let mut firmware_path = None;
    let mut out_path = None;
    let mut name = None;
    let mut launch_gui = false;
    let mut requested_board = None;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pcb" => pcb_path = Some(require_path_arg(&mut args, "--pcb")?),
            "--firmware" => firmware_path = Some(require_path_arg(&mut args, "--firmware")?),
            "--out" => out_path = Some(require_path_arg(&mut args, "--out")?),
            "--name" => name = Some(require_string_arg(&mut args, "--name")?),
            "--board" => requested_board = Some(require_string_arg(&mut args, "--board")?),
            "--launch-gui" => launch_gui = true,
            "--help" | "-h" => return Err(KiCadCliError::Usage(usage())),
            value if value.starts_with('-') => {
                return Err(KiCadCliError::Usage(format!(
                    "unknown option `{value}`\n\n{}",
                    usage()
                )));
            }
            value => {
                return Err(KiCadCliError::Usage(format!(
                    "unexpected positional argument `{value}`\n\n{}",
                    usage()
                )));
            }
        }
    }

    Ok(CreateProjectOptions {
        pcb_path: pcb_path
            .ok_or_else(|| KiCadCliError::Usage(format!("missing --pcb\n\n{}", usage())))?,
        firmware_path: firmware_path
            .ok_or_else(|| KiCadCliError::Usage(format!("missing --firmware\n\n{}", usage())))?,
        out_path,
        name,
        launch_gui,
        requested_board,
    })
}

fn parse_sync_project_options(
    args: impl IntoIterator<Item = String>,
) -> Result<SyncProjectOptions, KiCadCliError> {
    let mut project_path = None;
    let mut launch_gui = false;
    let mut requested_board = None;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--project" => project_path = Some(require_path_arg(&mut args, "--project")?),
            "--board" => requested_board = Some(require_string_arg(&mut args, "--board")?),
            "--launch-gui" => launch_gui = true,
            "--help" | "-h" => return Err(KiCadCliError::Usage(usage())),
            value if value.starts_with('-') => {
                return Err(KiCadCliError::Usage(format!(
                    "unknown option `{value}`\n\n{}",
                    usage()
                )));
            }
            value => {
                return Err(KiCadCliError::Usage(format!(
                    "unexpected positional argument `{value}`\n\n{}",
                    usage()
                )));
            }
        }
    }

    Ok(SyncProjectOptions {
        project_path: project_path
            .ok_or_else(|| KiCadCliError::Usage(format!("missing --project\n\n{}", usage())))?,
        launch_gui,
        requested_board,
    })
}

fn parse_open_gui_options(
    args: impl IntoIterator<Item = String>,
) -> Result<PathBuf, KiCadCliError> {
    let mut project_path = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--project" => project_path = Some(require_path_arg(&mut args, "--project")?),
            "--help" | "-h" => return Err(KiCadCliError::Usage(usage())),
            value if value.starts_with('-') => {
                return Err(KiCadCliError::Usage(format!(
                    "unknown option `{value}`\n\n{}",
                    usage()
                )));
            }
            value => {
                return Err(KiCadCliError::Usage(format!(
                    "unexpected positional argument `{value}`\n\n{}",
                    usage()
                )));
            }
        }
    }
    project_path.ok_or_else(|| KiCadCliError::Usage(format!("missing --project\n\n{}", usage())))
}

fn parse_open_pcb_options(
    args: impl IntoIterator<Item = String>,
) -> Result<OpenPcbOptions, KiCadCliError> {
    let mut pcb_path = None;
    let mut requested_board = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pcb" => pcb_path = Some(require_path_arg(&mut args, "--pcb")?),
            "--board" => requested_board = Some(require_string_arg(&mut args, "--board")?),
            "--help" | "-h" => return Err(KiCadCliError::Usage(usage())),
            value if value.starts_with('-') => {
                return Err(KiCadCliError::Usage(format!(
                    "unknown option `{value}`\n\n{}",
                    usage()
                )));
            }
            value => {
                return Err(KiCadCliError::Usage(format!(
                    "unexpected positional argument `{value}`\n\n{}",
                    usage()
                )));
            }
        }
    }

    Ok(OpenPcbOptions {
        pcb_path: pcb_path
            .ok_or_else(|| KiCadCliError::Usage(format!("missing --pcb\n\n{}", usage())))?,
        requested_board,
    })
}

fn require_path_arg(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<PathBuf, KiCadCliError> {
    let Some(value) = args.next() else {
        return Err(KiCadCliError::Usage(format!(
            "{flag} requires a file path\n\n{}",
            usage()
        )));
    };
    Ok(PathBuf::from(value))
}

fn require_string_arg(
    args: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, KiCadCliError> {
    args.next()
        .ok_or_else(|| KiCadCliError::Usage(format!("{flag} requires a value\n\n{}", usage())))
}

fn parse_requested_board(value: Option<&str>) -> Result<RequestedHostBoard, KiCadCliError> {
    match value.map(|value| value.trim().to_ascii_lowercase()) {
        None => Ok(RequestedHostBoard::Auto),
        Some(value) if value == "auto" => Ok(RequestedHostBoard::Auto),
        Some(value) if value == "mega" || value == "mega2560" => Ok(RequestedHostBoard::Mega),
        Some(value) if value == "nano" || value == "nano328p" => Ok(RequestedHostBoard::Nano),
        Some(value) => Err(KiCadCliError::Usage(format!(
            "unsupported board selection `{value}`; use auto, mega, or nano\n\n{}",
            usage()
        ))),
    }
}

fn resolve_requested_board(
    requested: RequestedHostBoard,
    firmware_path: &Path,
    pcb_path: &Path,
) -> HostBoard {
    match requested {
        RequestedHostBoard::Mega => HostBoard::Mega2560Rev3,
        RequestedHostBoard::Nano => HostBoard::NanoV3,
        RequestedHostBoard::Auto => inferred_host_board_from_source(firmware_path)
            .or_else(|| inferred_host_board_from_source(pcb_path))
            .unwrap_or(HostBoard::Mega2560Rev3),
    }
}

fn classify_firmware_source(path: &Path) -> Result<FirmwareSourceKind, KiCadCliError> {
    if path.is_dir() {
        return Ok(FirmwareSourceKind::Ino);
    }

    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("ino") => Ok(FirmwareSourceKind::Ino),
        Some("hex") => Ok(FirmwareSourceKind::Hex),
        _ => Err(KiCadCliError::UnsupportedFirmwarePath(path.to_path_buf())),
    }
}

fn default_output_path(pcb_path: &Path) -> PathBuf {
    let stem = pcb_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("simulation");
    pcb_path.with_file_name(format!("{stem}.avrsim.json"))
}

fn resolve_sibling_gui_binary() -> Option<PathBuf> {
    let current = std::env::current_exe().ok()?;
    let parent = current.parent()?;
    let candidates = ["arduino-simulator-gui", "arduino-simulator-gui.exe"];

    for candidate in candidates {
        let path = parent.join(candidate);
        if path.is_file() {
            return Some(path);
        }
    }

    None
}

fn resolve_cargo_executable() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join("cargo");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut candidates = vec![
        PathBuf::from("/opt/homebrew/bin/cargo"),
        PathBuf::from("/usr/local/bin/cargo"),
    ];
    if let Some(home) = home {
        candidates.push(home.join(".cargo/bin/cargo"));
    }

    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn usage() -> String {
    [
        "Usage:",
        "  arduino-simulator-kicad create-project --pcb <board.kicad_pcb> --firmware <sketch.ino|firmware.hex|sketch_dir> [--board auto|mega|nano] [--out <project.avrsim.json>] [--name <project name>] [--launch-gui]",
        "  arduino-simulator-kicad sync-project --project <project.avrsim.json> [--board auto|mega|nano] [--launch-gui]",
        "  arduino-simulator-kicad open-pcb --pcb <board.kicad_pcb> [--board auto|mega|nano]",
        "  arduino-simulator-kicad open-gui --project <project.avrsim.json>",
        "",
        "The KiCad adapter can open a PCB directly in generic mode, or import a PCB, infer host-pin",
        "bindings, write an .avrsim.json project, and optionally launch the arduino_simulator GUI.",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use rust_project::FirmwareSourceKind;
    use rust_project::SimulationProject;
    use tempfile::tempdir;

    use super::{
        classify_firmware_source, create_project, default_output_path, parse_open_pcb_options,
        parse_requested_board, sync_project, CreateProjectOptions, RequestedHostBoard,
        SyncProjectOptions,
    };

    fn example_pcb_path(file_name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/pcbs")
            .join(file_name)
    }

    fn example_ino_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/ino/mega_pin_sweep/mega_pin_sweep.ino")
    }

    #[test]
    fn firmware_classifier_accepts_ino_and_hex_inputs() {
        assert!(matches!(
            classify_firmware_source(&example_ino_path()),
            Ok(FirmwareSourceKind::Ino)
        ));
        assert!(matches!(
            classify_firmware_source(Path::new("/tmp/test.hex")),
            Ok(FirmwareSourceKind::Hex)
        ));
    }

    #[test]
    fn board_parser_accepts_auto_mega_nano() {
        assert_eq!(
            parse_requested_board(None).unwrap(),
            RequestedHostBoard::Auto
        );
        assert_eq!(
            parse_requested_board(Some("mega")).unwrap(),
            RequestedHostBoard::Mega
        );
        assert_eq!(
            parse_requested_board(Some("nano")).unwrap(),
            RequestedHostBoard::Nano
        );
    }

    #[test]
    fn default_output_path_tracks_the_pcb_file_name() {
        let path = PathBuf::from("/tmp/mega_r3_sidecar_controller_rev_a.kicad_pcb");
        assert_eq!(
            default_output_path(&path),
            PathBuf::from("/tmp/mega_r3_sidecar_controller_rev_a.avrsim.json")
        );
    }

    #[test]
    fn create_project_writes_an_auto_bound_project_file() {
        let temp = tempdir().expect("tempdir");
        let out_path = temp.path().join("generated.avrsim.json");
        let created_path = create_project(&CreateProjectOptions {
            pcb_path: example_pcb_path("mega_r3_sidecar_controller_rev_a.kicad_pcb"),
            firmware_path: example_ino_path(),
            out_path: Some(out_path.clone()),
            name: Some("Generated".to_string()),
            launch_gui: false,
            requested_board: Some("auto".to_string()),
        })
        .expect("create project");

        assert_eq!(created_path, out_path);
        let project = SimulationProject::load_json(&created_path).expect("load project");
        assert!(project
            .bindings
            .iter()
            .any(|binding| binding.board_signal == "D27" && binding.pcb_net == "/PA5"));
    }

    #[test]
    fn sync_project_preserves_file_and_repairs_bindings() {
        let temp = tempdir().expect("tempdir");
        let project_path = temp.path().join("generated.avrsim.json");
        create_project(&CreateProjectOptions {
            pcb_path: example_pcb_path("mega_r3_sidecar_controller_rev_a.kicad_pcb"),
            firmware_path: example_ino_path(),
            out_path: Some(project_path.clone()),
            name: Some("Generated".to_string()),
            launch_gui: false,
            requested_board: Some("mega".to_string()),
        })
        .expect("create project");

        let mut project = SimulationProject::load_json(&project_path).expect("load project");
        project
            .bindings
            .retain(|binding| binding.board_signal != "D27");
        project.save_json(&project_path).expect("save modified");

        sync_project(&SyncProjectOptions {
            project_path: project_path.clone(),
            launch_gui: false,
            requested_board: None,
        })
        .expect("sync project");

        let synced = fs::read_to_string(&project_path).expect("read synced");
        assert!(synced.contains("\"board_signal\": \"D27\""));
    }

    #[test]
    fn open_pcb_parser_accepts_board_and_path() {
        let parsed = parse_open_pcb_options(
            ["--pcb", "/tmp/demo.kicad_pcb", "--board", "mega"]
                .into_iter()
                .map(|value| value.to_string()),
        )
        .expect("parse");
        assert_eq!(parsed.pcb_path, PathBuf::from("/tmp/demo.kicad_pcb"));
        assert_eq!(parsed.requested_board.as_deref(), Some("mega"));
    }
}
