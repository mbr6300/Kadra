//! SnapIt – Screenshot-Werkzeug im Stil von SnagIt (ohne Video).

mod app;
mod capture;
mod doc;
mod editor;
mod hotkeys;
mod icon;
mod library;
mod overlay;
mod platform;
mod render;
mod tray;
mod ui_kit;

fn main() -> eframe::Result {
    // Eigenes Icon setzen – sonst ersetzt eframe das Dock-Symbol durch sein Standard-Logo.
    let size = 512;
    let app_icon = egui::IconData { rgba: icon::rgba(size), width: size, height: size };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SnapIt")
            .with_app_id("ch.snapit.app")
            .with_icon(std::sync::Arc::new(app_icon))
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([560.0, 420.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("SnapIt", options, Box::new(|cc| Ok(Box::new(app::SnapApp::new(cc)))))
}
