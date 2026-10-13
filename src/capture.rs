//! Bildschirmaufnahme über xcap.

use egui::{Pos2, Rect, Vec2};
use image::{imageops, RgbaImage};
use xcap::Monitor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureKind {
    /// Bereich ziehen oder Fenster anklicken (beides in einem Modus).
    Region,
    Window,
    /// Bildschirm unter dem Mauszeiger.
    Screen,
    /// Alle Bildschirme zusammengesetzt.
    AllScreens,
}

impl CaptureKind {
    pub const ALL: [CaptureKind; 4] =
        [CaptureKind::Region, CaptureKind::Window, CaptureKind::Screen, CaptureKind::AllScreens];

    pub fn label(self) -> &'static str {
        match self {
            CaptureKind::Region => "Bereich",
            CaptureKind::Window => "Fenster",
            CaptureKind::Screen => "Bildschirm",
            CaptureKind::AllScreens => "Alle Bildschirme",
        }
    }
    pub fn needs_overlay(self) -> bool {
        matches!(self, CaptureKind::Region | CaptureKind::Window)
    }
}

#[derive(Clone, Debug)]
pub struct WinInfo {
    pub id: u32,
    pub title: String,
    pub app: String,
    /// Logische Koordinaten relativ zum Monitor.
    pub rect: Rect,
    /// false = auf anderem Schreibtisch oder minimiert.
    pub on_screen: bool,
}

impl WinInfo {
    pub fn label(&self) -> String {
        if self.title.is_empty() { self.app.clone() } else { format!("{} – {}", self.app, self.title) }
    }
}

pub struct Grab {
    pub kind: CaptureKind,
    pub image: RgbaImage,
    /// Logische Monitor-Position/-Größe (globale Koordinaten).
    pub origin: Pos2,
    pub size: Vec2,
    /// Alle Fenster (vorne zuerst), auch verdeckte und auf anderen Schreibtischen.
    pub windows: Vec<WinInfo>,
}

impl Grab {
    /// Pixel pro logischem Punkt.
    pub fn scale(&self) -> f32 {
        self.image.width() as f32 / self.size.x.max(1.0)
    }

    /// Schneidet einen logischen Bereich (relativ zum Monitor) aus.
    pub fn crop(&self, r: Rect) -> Option<RgbaImage> {
        let s = self.scale();
        let r = r.intersect(Rect::from_min_size(Pos2::ZERO, self.size));
        let x = (r.min.x * s).round().max(0.0) as u32;
        let y = (r.min.y * s).round().max(0.0) as u32;
        let w = ((r.width() * s).round() as u32).min(self.image.width().saturating_sub(x));
        let h = ((r.height() * s).round() as u32).min(self.image.height().saturating_sub(y));
        (w > 0 && h > 0).then(|| imageops::crop_imm(&self.image, x, y, w, h).to_image())
    }
}

fn current_monitor() -> Result<Monitor, String> {
    if let Some((x, y)) = crate::platform::cursor_pos() {
        if let Ok(m) = Monitor::from_point(x as i32, y as i32) {
            return Ok(m);
        }
    }
    let all = Monitor::all().map_err(|e| e.to_string())?;
    all.iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .or(all.first())
        .cloned()
        .ok_or_else(|| "Kein Bildschirm gefunden".into())
}

const IGNORED_APPS: &[&str] =
    &["Window Server", "Dock", "Control Center", "SystemUIServer", "Notification Center", "Spotlight"];

/// Alle aufnehmbaren Fenster, vorderstes zuerst – inkl. verdeckter und solcher
/// auf anderen Schreibtischen.
fn list_windows(origin: Pos2) -> Vec<WinInfo> {
    let own = std::process::id();
    crate::platform::list_windows(true)
        .into_iter()
        .filter_map(|w| {
            if w.pid == own || IGNORED_APPS.contains(&w.app.as_str()) {
                return None;
            }
            // Unsichtbare Hilfsfenster ohne Titel ausblenden.
            let hidden_junk = !w.on_screen && (w.title.trim().is_empty() || w.w < 160.0 || w.h < 100.0);
            if w.w < 60.0 || w.h < 40.0 || hidden_junk {
                return None;
            }
            let r = Rect::from_min_size(Pos2::new(w.x, w.y), Vec2::new(w.w, w.h));
            Some(WinInfo {
                id: w.id,
                title: w.title,
                app: w.app,
                rect: r.translate(-origin.to_vec2()),
                on_screen: w.on_screen,
            })
        })
        .collect()
}

pub fn grab(kind: CaptureKind) -> Result<Grab, String> {
    if kind == CaptureKind::AllScreens {
        return grab_all();
    }
    let m = current_monitor()?;
    let origin = Pos2::new(m.x().unwrap_or(0) as f32, m.y().unwrap_or(0) as f32);
    let size = Vec2::new(m.width().unwrap_or(1) as f32, m.height().unwrap_or(1) as f32);
    let image = m.capture_image().map_err(|e| format!("Aufnahme fehlgeschlagen: {e}"))?;
    let windows = if kind.needs_overlay() { list_windows(origin) } else { vec![] };
    Ok(Grab { kind, image, origin, size, windows })
}

fn grab_all() -> Result<Grab, String> {
    let monitors = Monitor::all().map_err(|e| e.to_string())?;
    let mut shots = vec![];
    for m in &monitors {
        let r = Rect::from_min_size(
            Pos2::new(m.x().unwrap_or(0) as f32, m.y().unwrap_or(0) as f32),
            Vec2::new(m.width().unwrap_or(1) as f32, m.height().unwrap_or(1) as f32),
        );
        let img = m.capture_image().map_err(|e| format!("Aufnahme fehlgeschlagen: {e}"))?;
        shots.push((r, img));
    }
    let bounds = shots.iter().fold(Rect::NOTHING, |acc, (r, _)| acc.union(*r));
    let scale = shots.iter().map(|(r, i)| i.width() as f32 / r.width()).fold(1.0, f32::max);
    let mut out = RgbaImage::new((bounds.width() * scale) as u32, (bounds.height() * scale) as u32);
    for (r, img) in shots {
        let (w, h) = ((r.width() * scale) as u32, (r.height() * scale) as u32);
        let img = if img.width() != w { imageops::resize(&img, w, h, imageops::FilterType::Triangle) } else { img };
        let x = ((r.min.x - bounds.min.x) * scale) as i64;
        let y = ((r.min.y - bounds.min.y) * scale) as i64;
        imageops::replace(&mut out, &img, x, y);
    }
    Ok(Grab { kind: CaptureKind::AllScreens, image: out, origin: bounds.min, size: bounds.size(), windows: vec![] })
}
