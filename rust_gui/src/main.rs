use eframe::egui;

use rust_gui::AvrSimGuiApp;
use rust_project::HostBoard;

enum StartupInput {
    Project(std::path::PathBuf),
    Pcb {
        path: std::path::PathBuf,
        board: Option<HostBoard>,
    },
}

fn main() -> eframe::Result<()> {
    let startup = parse_startup_input(std::env::args().skip(1));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("Arduino Simulator GUI"),
        ..Default::default()
    };

    eframe::run_native(
        "Arduino Simulator GUI",
        options,
        Box::new(move |_cc| {
            let app = match startup.as_ref() {
                Some(StartupInput::Project(path)) => AvrSimGuiApp::from_project_path(path),
                Some(StartupInput::Pcb { path, board }) => {
                    AvrSimGuiApp::from_pcb_path(path, *board)
                }
                None => AvrSimGuiApp::default(),
            };
            Ok(Box::new(app))
        }),
    )
}

fn parse_startup_input(args: impl IntoIterator<Item = String>) -> Option<StartupInput> {
    let mut args = args.into_iter();
    let mut project = None;
    let mut pcb = None;
    let mut board = None;
    let mut positional = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--project" => project = args.next().map(std::path::PathBuf::from),
            "--pcb" => pcb = args.next().map(std::path::PathBuf::from),
            "--board" => {
                board = args.next().and_then(|value| parse_host_board(&value));
            }
            value if value.starts_with('-') => {}
            value => {
                if positional.is_none() {
                    positional = Some(std::path::PathBuf::from(value));
                }
            }
        }
    }

    if let Some(path) = project {
        return Some(StartupInput::Project(path));
    }
    if let Some(path) = pcb {
        return Some(StartupInput::Pcb { path, board });
    }
    positional.map(StartupInput::Project)
}

fn parse_host_board(value: &str) -> Option<HostBoard> {
    match value.trim().to_ascii_lowercase().as_str() {
        "mega" | "mega2560" => Some(HostBoard::Mega2560Rev3),
        "nano" | "nano328p" => Some(HostBoard::NanoV3),
        _ => None,
    }
}
