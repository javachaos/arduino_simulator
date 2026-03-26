use eframe::egui;

use rust_gui::AvrSimGuiApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("avrsim-rs GUI"),
        ..Default::default()
    };

    eframe::run_native(
        "avrsim-rs GUI",
        options,
        Box::new(|_cc| Ok(Box::new(AvrSimGuiApp::default()))),
    )
}
