#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 900.0])
            .with_min_inner_size([980.0, 680.0])
            .with_title("Arduino Simulator Web"),
        ..Default::default()
    };

    eframe::run_native(
        "Arduino Simulator Web",
        options,
        Box::new(|_cc| Ok(Box::new(rust_web::RustWebApp::default()))),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{prelude::*, JsCast};

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start_web() -> Result<(), JsValue> {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();

    let window = web_sys::window().ok_or_else(|| JsValue::from_str("missing browser window"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("missing browser document"))?;
    let canvas = document
        .get_element_by_id("app-canvas")
        .ok_or_else(|| JsValue::from_str("missing canvas element"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;

    let runner = eframe::WebRunner::new();
    wasm_bindgen_futures::spawn_local(async move {
        let result = runner
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|_cc| Ok(Box::new(rust_web::RustWebApp::default()))),
            )
            .await;

        if let Err(error) = result {
            log::error!("failed to start rust_web: {error:?}");
        }
    });

    Ok(())
}
