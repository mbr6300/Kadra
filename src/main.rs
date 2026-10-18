//! Kadra – Screenshot-Werkzeug für macOS und Windows: aufnehmen, beschriften, exportieren.

// Windows: kein Konsolenfenster neben der App (Release-Builds).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod capture;
mod doc;
mod editor;
mod hotkeys;
mod icon;
mod library;
mod menu;
mod overlay;
mod platform;
mod render;
mod tray;
mod ui_kit;

/// Systemschrift mit ⌃ ⌥ ⇧ ⌘, ✂ ⧉ & Co.; Ersatzschrift in Editor und Export.
#[cfg(target_os = "macos")]
pub const SYMBOL_FONT_PATH: &str = "/System/Library/Fonts/Apple Symbols.ttf";
#[cfg(target_os = "windows")]
pub const SYMBOL_FONT_PATH: &str = "C:\\Windows\\Fonts\\seguisym.ttf";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const SYMBOL_FONT_PATH: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";

fn main() -> eframe::Result {
    // Eigenes Icon setzen – sonst ersetzt eframe das Dock-Symbol durch sein Standard-Logo.
    let size = 512;
    let rgba = if cfg!(target_os = "macos") { icon::rgba(size) } else { icon::rgba_full(size) };
    let app_icon = egui::IconData { rgba, width: size, height: size };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Kadra")
            .with_app_id("ch.kadra.app")
            .with_icon(std::sync::Arc::new(app_icon))
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([560.0, 420.0])
            .with_drag_and_drop(true),
        // Windows: Menü-Kürzel (Strg+S …) wirken nur, wenn die Ereignisschleife sie übersetzt.
        #[cfg(target_os = "windows")]
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            builder.with_msg_hook(platform::win::translate_accelerator);
        })),
        ..Default::default()
    };
    eframe::run_native("Kadra", options, Box::new(|cc| Ok(Box::new(app::KadraApp::new(cc)))))
}
