//! Texterkennung in Bildern. macOS: Apple Vision – läuft lokal, ohne Netzwerk.

use image::RgbaImage;

/// Gibt es Texterkennung auf diesem System?
pub const AVAILABLE: bool = cfg!(target_os = "macos");

/// Erkannte Textzeile; `rect` = [x, y, Breite, Höhe] in Bildpixeln (Ursprung oben links).
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub text: String,
    pub rect: [f32; 4],
}

/// Erkennt alle Textzeilen im Bild (Deutsch und Englisch, genaue Erkennung).
#[cfg(target_os = "macos")]
pub fn recognize(img: &RgbaImage) -> Result<Vec<Line>, String> {
    if img.width() < 2 || img.height() < 2 {
        return Ok(vec![]);
    }
    // Vision legt Objective-C-Objekte im Autorelease-Pool ab; im Hintergrund-Thread daher eigener Pool.
    objc2::rc::autoreleasepool(|_| recognize_mac(img))
}

#[cfg(target_os = "macos")]
fn recognize_mac(img: &RgbaImage) -> Result<Vec<Line>, String> {
    use objc2::AnyThread;
    use objc2_core_foundation::CFData;
    use objc2_core_graphics::{CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage};
    use objc2_foundation::{NSArray, NSDictionary, NSString};
    use objc2_vision::{VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel};

    let (w, h) = (img.width() as usize, img.height() as usize);
    let data = CFData::from_bytes(img.as_raw());
    let provider = CGDataProvider::with_cf_data(Some(&data)).ok_or("Bilddaten nicht lesbar")?;
    let space = CGColorSpace::new_device_rgb().ok_or("Farbraum nicht verfügbar")?;
    // kCGImageAlphaLast (3): RGBA, nicht vormultipliziert – wie `RgbaImage`.
    let info = CGBitmapInfo(3);
    let cg = unsafe {
        CGImage::new(
            w,
            h,
            8,
            32,
            w * 4,
            Some(&space),
            info,
            Some(&provider),
            std::ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
    .ok_or("Bild konnte nicht übergeben werden")?;

    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    request.setUsesLanguageCorrection(true);
    // Feste Sprachliste statt automatischer Erkennung (die gibt es erst ab macOS 13).
    let langs = NSArray::from_retained_slice(&[NSString::from_str("de-DE"), NSString::from_str("en-US")]);
    request.setRecognitionLanguages(&langs);

    let handler = unsafe {
        VNImageRequestHandler::initWithCGImage_options(VNImageRequestHandler::alloc(), &cg, &NSDictionary::new())
    };
    let req: &VNRequest = &request;
    handler
        .performRequests_error(&NSArray::from_slice(&[req]))
        .map_err(|e| format!("Texterkennung fehlgeschlagen: {}", e.localizedDescription()))?;

    let (wf, hf) = (w as f32, h as f32);
    let mut lines = vec![];
    for obs in request.results().map(|r| r.to_vec()).unwrap_or_default() {
        let Some(best) = obs.topCandidates(1).firstObject() else { continue };
        let text = best.string().to_string();
        if text.trim().is_empty() {
            continue;
        }
        // Normalisiert, Ursprung unten links → Pixel, Ursprung oben links
        let bb = unsafe { obs.boundingBox() };
        let (bx, by, bw, bh) = (bb.origin.x as f32, bb.origin.y as f32, bb.size.width as f32, bb.size.height as f32);
        lines.push(Line { text, rect: [bx * wf, (1.0 - by - bh) * hf, bw * wf, bh * hf] });
    }
    Ok(lines)
}

#[cfg(not(target_os = "macos"))]
pub fn recognize(_img: &RgbaImage) -> Result<Vec<Line>, String> {
    Err("Texterkennung ist auf diesem System noch nicht verfügbar".into())
}

/// Zeilen in Lesereihenfolge zusammensetzen: Zeilen auf gleicher Höhe nebeneinander
/// (weiter Abstand = Tabulator, z. B. Tabellenspalten), größerer Zeilenabstand = Leerzeile.
pub fn to_text(lines: &[Line]) -> String {
    let mut sorted: Vec<&Line> = lines.iter().collect();
    sorted.sort_by(|a, b| a.rect[1].total_cmp(&b.rect[1]));
    // Zu Reihen gruppieren: vertikale Überlappung von mindestens der halben Zeilenhöhe
    let mut rows: Vec<Vec<&Line>> = vec![];
    for l in sorted {
        let (top, bottom) = (l.rect[1], l.rect[1] + l.rect[3]);
        let same_row = rows.last().is_some_and(|row| {
            let r = row[0];
            let overlap = bottom.min(r.rect[1] + r.rect[3]) - top.max(r.rect[1]);
            overlap > 0.5 * l.rect[3].min(r.rect[3])
        });
        if same_row {
            rows.last_mut().unwrap().push(l);
        } else {
            rows.push(vec![l]);
        }
    }
    let mut out = String::new();
    let mut prev_bottom: Option<(f32, f32)> = None;
    for row in &mut rows {
        row.sort_by(|a, b| a.rect[0].total_cmp(&b.rect[0]));
        let top = row.iter().map(|l| l.rect[1]).fold(f32::MAX, f32::min);
        let height = row.iter().map(|l| l.rect[3]).fold(0.0, f32::max);
        if let Some((bottom, h)) = prev_bottom {
            out.push('\n');
            if top - bottom > 1.2 * h.max(height) {
                out.push('\n');
            }
        }
        for (i, l) in row.iter().enumerate() {
            if i > 0 {
                let prev = row[i - 1];
                let gap = l.rect[0] - (prev.rect[0] + prev.rect[2]);
                out.push(if gap > 2.0 * height { '\t' } else { ' ' });
            }
            out.push_str(l.text.trim());
        }
        let bottom = row.iter().map(|l| l.rect[1] + l.rect[3]).fold(0.0, f32::max);
        prev_bottom = Some((bottom, height));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, x: f32, y: f32, w: f32, h: f32) -> Line {
        Line { text: text.into(), rect: [x, y, w, h] }
    }

    #[test]
    fn reading_order_rows_columns_and_paragraphs() {
        let lines = vec![
            line("Wert", 300.0, 12.0, 60.0, 20.0),
            line("Name", 10.0, 10.0, 60.0, 20.0),
            line("Kadra", 10.0, 40.0, 60.0, 20.0),
            line("1.0", 300.0, 41.0, 40.0, 20.0),
            line("Fußnote", 10.0, 120.0, 80.0, 20.0),
        ];
        assert_eq!(to_text(&lines), "Name\tWert\nKadra\t1.0\n\nFußnote");
    }

    #[test]
    fn words_close_together_join_with_space() {
        let lines = vec![line("World", 80.0, 0.0, 50.0, 20.0), line("Hello", 10.0, 2.0, 60.0, 20.0)];
        assert_eq!(to_text(&lines), "Hello World");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn recognizes_rendered_text() {
        use crate::doc::{Annotation, Document, Shape, Style};
        let mut doc = Document::new(RgbaImage::from_pixel(900, 220, image::Rgba([255, 255, 255, 255])));
        let style = Style { stroke: [0, 0, 0, 255], font_size: 56.0, shadow: false, ..Style::default() };
        doc.annotations.push(Annotation { shape: Shape::Text { pos: [30.0, 30.0], text: "Hallo Kadra 2026\nZweite Zeile".into() }, style });
        let lines = recognize(&crate::render::render_content(&doc)).expect("Vision verfügbar");
        let text = to_text(&lines);
        assert!(text.contains("Kadra 2026"), "erkannt: {text:?}");
        assert!(text.contains("Zweite Zeile"), "erkannt: {text:?}");
        assert!(text.find("Kadra").unwrap() < text.find("Zweite").unwrap(), "Reihenfolge: {text:?}");
    }
}
