//! Bibliothek aller Aufnahmen (editierbar gespeichert) und Einstellungen.

use std::fs;
use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
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
    /// 1–100, gilt für JPEG und PDF.
    pub jpeg_quality: u8,
    /// Hinweis „läuft in der Menüleiste weiter“ wurde schon gezeigt.
    pub background_hint_shown: bool,
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
            jpeg_quality: 90,
            background_hint_shown: false,
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

/// Exportformate: (Dateiendung, Anzeigename, weitere Endungen).
pub const FORMATS: [(&str, &str, &[&str]); 7] = [
    ("png", "PNG", &[]),
    ("jpg", "JPEG", &["jpeg"]),
    ("tiff", "TIFF", &["tif"]),
    ("webp", "WebP", &[]),
    ("gif", "GIF", &[]),
    ("bmp", "BMP", &[]),
    ("pdf", "PDF", &[]),
];

pub fn format_name(ext: &str) -> &'static str {
    FORMATS.iter().find(|f| f.0 == ext || f.2.contains(&ext)).map(|f| f.1).unwrap_or("PNG")
}

/// Exportiert ein gerendertes Bild; das Format ergibt sich aus der Dateiendung.
pub fn export(img: &RgbaImage, path: &Path, jpeg_quality: u8) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
    let rgb = || image::DynamicImage::ImageRgba8(flatten_white(img)).to_rgb8();
    let res = match ext.as_str() {
        "jpg" | "jpeg" => {
            let file = fs::File::create(path).map_err(|e| e.to_string())?;
            JpegEncoder::new_with_quality(std::io::BufWriter::new(file), jpeg_quality.clamp(1, 100))
                .encode_image(&rgb())
        }
        "bmp" => rgb().save(path),
        "pdf" => return write_pdf(img, path, jpeg_quality),
        _ => img.save(path),
    };
    res.map_err(|e| e.to_string())
}

/// Formate mit einstellbarer Qualität.
pub fn has_quality(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
    matches!(ext.as_str(), "jpg" | "jpeg" | "pdf")
}

/// Größe des Bildes als JPEG in Bytes (für die Vorschau im Export-Dialog).
pub fn jpeg_size(img: &RgbaImage, quality: u8) -> usize {
    let rgb = image::DynamicImage::ImageRgba8(flatten_white(img)).to_rgb8();
    let mut buf = Vec::new();
    let _ = JpegEncoder::new_with_quality(&mut buf, quality.clamp(1, 100)).encode_image(&rgb);
    buf.len()
}

/// Schreibt eine einseitige PDF mit dem Bild als JPEG (96 dpi Seitengröße).
fn write_pdf(img: &RgbaImage, path: &Path, jpeg_quality: u8) -> Result<(), String> {
    let rgb = image::DynamicImage::ImageRgba8(flatten_white(img)).to_rgb8();
    let mut jpg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpg, jpeg_quality.clamp(1, 100)).encode_image(&rgb).map_err(|e| e.to_string())?;
    let (w, h) = img.dimensions();
    let (pw, ph) = (w as f32 * 0.75, h as f32 * 0.75);

    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = vec![];
    let content = format!("q {pw:.2} 0 0 {ph:.2} 0 0 cm /Im0 Do Q");
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] \
             /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>"
        )
        .into_bytes(),
        [
            format!(
                "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB \
                 /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
                jpg.len()
            )
            .into_bytes(),
            jpg,
            b"\nendstream".to_vec(),
        ]
        .concat(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes(),
    ];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes(),
    );
    fs::write(path, out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_quality_changes_size() {
        let mut img = RgbaImage::new(200, 150);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = image::Rgba([(x * 7 % 256) as u8, (y * 13 % 256) as u8, ((x ^ y) % 256) as u8, 255]);
        }
        let (low, high) = (jpeg_size(&img, 30), jpeg_size(&img, 95));
        assert!(low > 0 && low < high, "30 % = {low} B, 95 % = {high} B");
    }

    #[test]
    fn exports_all_formats() {
        // SNAPIT_TEST_OUT_DIR: Dateien zur Sichtprüfung behalten
        let keep = std::env::var_os("SNAPIT_TEST_OUT_DIR").map(PathBuf::from);
        let dir = keep.clone().unwrap_or_else(|| std::env::temp_dir().join(format!("snapit-export-{}", std::process::id())));
        let mut img = RgbaImage::from_pixel(64, 48, image::Rgba([30, 120, 230, 255]));
        img.put_pixel(0, 0, image::Rgba([0, 0, 0, 0])); // transparentes Pixel
        for (ext, _, alt) in FORMATS {
            for e in std::iter::once(ext).chain(alt.iter().copied()) {
                let path = dir.join(format!("test.{e}"));
                export(&img, &path, 90).unwrap_or_else(|err| panic!("{e}: {err}"));
                let bytes = fs::read(&path).unwrap();
                if e == "pdf" {
                    assert!(bytes.starts_with(b"%PDF-1.4") && bytes.ends_with(b"%%EOF\n"));
                } else {
                    let back = image::open(&path).unwrap_or_else(|err| panic!("{e} lesen: {err}"));
                    assert_eq!((back.width(), back.height()), (64, 48), "{e}");
                }
            }
        }
        if keep.is_none() {
            let _ = fs::remove_dir_all(dir);
        }
    }
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
