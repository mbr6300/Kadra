//! Bibliothek aller Aufnahmen (editierbar gespeichert) und Einstellungen.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::thread::JoinHandle;

use image::codecs::jpeg::JpegEncoder;
use image::RgbaImage;
use serde::{Deserialize, Serialize};

use crate::doc::{DocMeta, Document};
use crate::hotkeys::Shortcut;
use crate::render;

/// Früherer App-Name; nur für die einmalige Datenübernahme.
const LEGACY_NAME: &str = "SnapIt";

pub fn data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let d = base.join("Kadra");
    static MIGRATE: std::sync::Once = std::sync::Once::new();
    MIGRATE.call_once(|| {
        // Bibliothek und Einstellungen aus dem alten Ordner übernehmen (Umbenennung SnapIt → Kadra).
        let old = base.join(LEGACY_NAME);
        if !d.exists() && old.is_dir() {
            let _ = fs::rename(&old, &d);
        }
    });
    let _ = fs::create_dir_all(d.join("library"));
    d
}

fn default_save_dir(name: &str) -> PathBuf {
    dirs::picture_dir().unwrap_or_else(|| PathBuf::from(".")).join(name)
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

/// Schreibt erst in eine Temp-Datei und benennt dann um – ein Absturz hinterlässt
/// nie eine halb geschriebene Datei.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes).and_then(|_| fs::rename(&tmp, path)).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e.to_string()
    })
}

fn write_png_atomic(img: &RgbaImage, path: &Path) -> Result<(), String> {
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    write_atomic(path, &buf)
}

/// Speichert Originalbild, Anmerkungen und Vorschaubild. Gibt das Thumbnail zurück.
pub fn save(id: &str, doc: &Document, image_changed: bool) -> Result<RgbaImage, String> {
    let dir = lib_dir();
    let png = dir.join(format!("{id}.png"));
    if image_changed || !png.exists() {
        write_png_atomic(&doc.image, &png)?;
    }
    let meta = DocMeta { annotations: doc.annotations.clone(), effects: doc.effects.clone() };
    write_atomic(&dir.join(format!("{id}.json")), &serde_json::to_vec_pretty(&meta).unwrap())?;
    let thumb = render::thumbnail(&render::render(doc), 320);
    write_png_atomic(&thumb, &dir.join(format!("{id}.thumb.png")))?;
    Ok(thumb)
}

enum Job {
    Save { id: String, doc: Document, image_changed: bool },
    Trash(String),
    Restore(String),
    /// Meldet sich, sobald alle vorherigen Aufträge erledigt sind.
    Barrier(Sender<()>),
}

/// Einziger Hintergrund-Thread für alle Schreibzugriffe auf die Bibliothek.
/// Aufträge laufen strikt nacheinander – ein älterer Stand kann so nie einen neueren
/// überschreiben, und Löschen wartet auf laufendes Speichern.
pub struct Saver {
    tx: Option<Sender<Job>>,
    handle: Option<JoinHandle<()>>,
}

impl Saver {
    /// Fertige Vorschaubilder gehen an `thumbs`.
    pub fn new(thumbs: Sender<(String, RgbaImage)>, ctx: egui::Context) -> Self {
        let (tx, rx) = channel::<Job>();
        let handle = std::thread::spawn(move || {
            while let Ok(first) = rx.recv() {
                // Aufgelaufene Aufträge einsammeln; direkt aufeinanderfolgende Speicherungen
                // desselben Eintrags zu einer zusammenfassen (nur der neueste Stand zählt).
                let mut batch = vec![first];
                while let Ok(job) = rx.try_recv() {
                    if let (Some(Job::Save { id: last, image_changed: changed, .. }), Job::Save { id, .. }) =
                        (batch.last(), &job)
                    {
                        if last == id {
                            let changed = *changed;
                            batch.pop();
                            if let Job::Save { id, doc, image_changed } = job {
                                batch.push(Job::Save { id, doc, image_changed: image_changed || changed });
                            }
                            continue;
                        }
                    }
                    batch.push(job);
                }
                for job in batch {
                    match job {
                        Job::Save { id, doc, image_changed } => {
                            if let Ok(thumb) = save(&id, &doc, image_changed) {
                                let _ = thumbs.send((id, thumb));
                                ctx.request_repaint();
                            }
                        }
                        Job::Trash(id) => trash(&id),
                        Job::Restore(id) => restore(&id),
                        Job::Barrier(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            }
        });
        Self { tx: Some(tx), handle: Some(handle) }
    }

    fn send(&self, job: Job) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(job);
        }
    }

    pub fn save(&self, id: String, doc: Document, image_changed: bool) {
        self.send(Job::Save { id, doc, image_changed });
    }

    pub fn trash(&self, id: String) {
        self.send(Job::Trash(id));
    }

    pub fn restore(&self, id: String) {
        self.send(Job::Restore(id));
    }

    /// Blockiert, bis alle bisher übergebenen Aufträge erledigt sind.
    pub fn wait(&self) {
        let (done, rx) = channel();
        self.send(Job::Barrier(done));
        let _ = rx.recv();
    }

    /// Wartet, bis alle Aufträge geschrieben sind, und beendet den Thread (beim Beenden).
    pub fn finish(&mut self) {
        self.tx = None;
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
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

const ITEM_FILES: [&str; 3] = ["png", "json", "thumb.png"];

fn trash_dir() -> PathBuf {
    data_dir().join("trash")
}

/// Aufnahme in den internen Papierkorb verschieben (rückgängig machbar mit `restore`).
pub fn trash(id: &str) {
    let (dir, trash) = (lib_dir(), trash_dir());
    let _ = fs::create_dir_all(&trash);
    for ext in ITEM_FILES {
        let name = format!("{id}.{ext}");
        let _ = fs::rename(dir.join(&name), trash.join(&name));
    }
}

/// Aufnahme aus dem Papierkorb zurückholen.
pub fn restore(id: &str) {
    let (dir, trash) = (lib_dir(), trash_dir());
    for ext in ITEM_FILES {
        let name = format!("{id}.{ext}");
        let _ = fs::rename(trash.join(&name), dir.join(&name));
    }
}

/// Papierkorb endgültig leeren (beim Start).
pub fn empty_trash() {
    let _ = fs::remove_dir_all(trash_dir());
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
            save_dir: default_save_dir("Kadra"),
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
        let mut s: Self =
            fs::read(Self::path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        // Alter Standard-Zielordner (~/Pictures/SnapIt) → neuer Standard (~/Pictures/Kadra);
        // bereits gespeicherte Bilder bleiben im alten Ordner.
        if s.save_dir == default_save_dir(LEGACY_NAME) {
            s.save_dir = default_save_dir("Kadra");
            s.save();
        }
        s
    }
    pub fn save(&self) {
        let _ = write_atomic(&Self::path(), &serde_json::to_vec_pretty(self).unwrap());
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
    fn trash_and_restore_item() {
        let id = format!("test-trash-{}", std::process::id());
        for ext in ITEM_FILES {
            fs::write(lib_dir().join(format!("{id}.{ext}")), b"x").unwrap();
        }
        trash(&id);
        assert!(ITEM_FILES.iter().all(|e| !lib_dir().join(format!("{id}.{e}")).exists()), "aus Bibliothek entfernt");
        assert!(ITEM_FILES.iter().all(|e| trash_dir().join(format!("{id}.{e}")).exists()), "im Papierkorb");
        restore(&id);
        assert!(ITEM_FILES.iter().all(|e| lib_dir().join(format!("{id}.{e}")).exists()), "wiederhergestellt");
        for ext in ITEM_FILES {
            let _ = fs::remove_file(lib_dir().join(format!("{id}.{ext}")));
        }
    }

    #[test]
    fn saver_keeps_order_save_then_trash() {
        let id = format!("test-saver-{}", std::process::id());
        let (tx, rx) = channel();
        let mut saver = Saver::new(tx, egui::Context::default());
        let mut doc = Document::new(RgbaImage::from_pixel(400, 300, image::Rgba([10, 20, 30, 255])));
        // Mehrere Stände hintereinander: der letzte muss auf der Platte landen.
        for n in 1..=3 {
            doc.effects.round_corners = n as f32;
            saver.save(id.clone(), doc.clone(), true);
        }
        saver.wait();
        assert_eq!(load(&id).unwrap().effects.round_corners, 3.0, "neuester Stand gespeichert");
        assert!(rx.try_iter().any(|(i, _)| i == id), "Vorschaubild gemeldet");
        assert!(
            fs::read_dir(lib_dir()).unwrap().flatten().all(|e| !e.file_name().to_string_lossy().ends_with(".tmp")),
            "keine Temp-Dateien übrig"
        );

        // Speichern direkt vor dem Löschen darf die Dateien nicht in die Bibliothek zurückschreiben.
        saver.save(id.clone(), doc.clone(), true);
        saver.trash(id.clone());
        saver.finish();
        assert!(ITEM_FILES.iter().all(|e| !lib_dir().join(format!("{id}.{e}")).exists()), "bleibt gelöscht");
        assert!(ITEM_FILES.iter().all(|e| trash_dir().join(format!("{id}.{e}")).exists()), "im Papierkorb");
        for ext in ITEM_FILES {
            let _ = fs::remove_file(trash_dir().join(format!("{id}.{ext}")));
        }
    }

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
        // KADRA_TEST_OUT_DIR: Dateien zur Sichtprüfung behalten
        let keep = std::env::var_os("KADRA_TEST_OUT_DIR").map(PathBuf::from);
        let dir = keep.clone().unwrap_or_else(|| std::env::temp_dir().join(format!("kadra-export-{}", std::process::id())));
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
