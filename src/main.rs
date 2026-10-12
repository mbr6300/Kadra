//! SnapIt – Screenshot-Werkzeug im Stil von SnagIt (ohne Video).

mod app;
mod capture;
mod doc;
mod editor;
mod hotkeys;
mod library;
mod overlay;
mod platform;
mod render;
mod tray;
mod ui_kit;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SnapIt")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([200.0, 150.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("SnapIt", options, Box::new(|cc| Ok(Box::new(app::SnapApp::new(cc)))))
}
