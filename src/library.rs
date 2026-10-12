//! Bibliothek aller Aufnahmen (editierbar gespeichert) und Einstellungen.

use std::fs;
use std::path::PathBuf;

use image::RgbaImage;
use serde::{Deserialize, Serialize};

use crate::doc::{DocMeta, Document};
use crate::hotkeys::Shortcut;
use crate::render;

pub fn data_dir() -> PathBuf {
    let d = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("SnapIt");
    let _ = fs::create_dir_all(d.join("library"));
    d
}

fn lib_dir() -> PathBuf {
    data_dir().join("library")
}

pub fn new_id() -> String {
    chrono::Local::now().format("%Y-%m-%d_%H-%M-%S-%3f").to_string()
}

pub struct Item {
    pub id: String,
    pub thumb: Option<RgbaImage>,
    pub texture: Option<egui::TextureHandle>,
}

/// IDs aller Einträge, neueste zuerst.
pub fn list() -> Vec<Item> {
    let mut ids: Vec<String> = fs::read_dir(lib_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.strip_suffix(".png").filter(|n| !n.ends_with(".thumb")).map(str::to_string)
        })
        .collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.into_iter()
        .map(|id| {
            let thumb = image::open(lib_dir().join(format!("{id}.thumb.png"))).ok().map(|i| i.to_rgba8());
            Item { id, thumb, texture: None }
        })
        .collect()
}

/// Speichert Originalbild, Anmerkungen und Vorschaubild. Gibt das Thumbnail zurück.
pub fn save(id: &str, doc: &Document, image_changed: bool) -> Result<RgbaImage, String> {
    let dir = lib_dir();
    let png = dir.join(format!("{id}.png"));
    if image_changed || !png.exists() {
        doc.image.save(&png).map_err(|e| e.to_string())?;
    }
    let meta = DocMeta { annotations: doc.annotations.clone(), effects: doc.effects.clone() };
    fs::write(dir.join(format!("{id}.json")), serde_json::to_vec_pretty(&meta).unwrap()).map_err(|e| e.to_string())?;
    let thumb = render::thumbnail(&render::render(doc), 320);
    thumb.save(dir.join(format!("{id}.thumb.png"))).map_err(|e| e.to_string())?;
    Ok(thumb)
}

pub fn load(id: &str) -> Result<Document, String> {
    let dir = lib_dir();
    let img = image::open(dir.join(format!("{id}.png"))).map_err(|e| e.to_string())?.to_rgba8();
    let mut doc = Document::new(img);
    if let Ok(bytes) = fs::read(dir.join(format!("{id}.json"))) {
        if let Ok(meta) = serde_json::from_slice::<DocMeta>(&bytes) {
            doc.annotations = meta.annotations;
            doc.effects = meta.effects;
        }
    }
    Ok(doc)
}

pub fn delete(id: &str) {
    let dir = lib_dir();
    for ext in ["png", "json", "thumb.png"] {
        let _ = fs::remove_file(dir.join(format!("{id}.{ext}")));
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub save_dir: PathBuf,
    pub format: String,
    pub delay_secs: u32,
    pub copy_after_capture: bool,
    pub hotkeys: bool,
    /// Kürzel je Aufnahmemodus (Reihenfolge wie `CaptureKind::ALL`).
    pub shortcuts: [Option<Shortcut>; 4],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            save_dir: dirs::picture_dir().unwrap_or_else(|| PathBuf::from(".")).join("SnapIt"),
            format: "png".into(),
            delay_secs: 0,
            copy_after_capture: false,
            hotkeys: true,
            shortcuts: Shortcut::defaults(),
        }
    }
}

impl Settings {
    fn path() -> PathBuf {
        data_dir().join("settings.json")
    }
    pub fn load() -> Self {
        fs::read(Self::path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }
    pub fn save(&self) {
        let _ = fs::write(Self::path(), serde_json::to_vec_pretty(self).unwrap());
    }
}

/// Exportiert ein gerendertes Bild im gewünschten Format.
pub fn export(img: &RgbaImage, path: &std::path::Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
    let res = match ext.as_str() {
        "jpg" | "jpeg" | "bmp" => image::DynamicImage::ImageRgba8(flatten_white(img)).to_rgb8().save(path),
        _ => img.save(path),
    };
    res.map_err(|e| e.to_string())
}

/// Transparente Bereiche auf Weiß legen (für Formate ohne Alpha).
fn flatten_white(img: &RgbaImage) -> RgbaImage {
    let mut out = img.clone();
    for px in out.pixels_mut() {
        let a = px.0[3] as u32;
        for c in 0..3 {
            px.0[c] = ((px.0[c] as u32 * a + 255 * (255 - a)) / 255) as u8;
        }
        px.0[3] = 255;
    }
    out
}
