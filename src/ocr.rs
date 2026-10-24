//! Texterkennung in Bildern. macOS: Apple Vision – läuft lokal, ohne Netzwerk.

use image::RgbaImage;

/// Gibt es Texterkennung auf diesem System?
pub const AVAILABLE: bool = cfg!(target_os = "macos");

/// Erkannte Textzeile; `rect` = [x, y, Breite, Höhe] in Bildpixeln (Ursprung oben links).
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub text: String,
    pub rect: [f32; 4],
    /// x-Grenzen der Zeichen in Bildpixeln: `xs[i]` = linker Rand von Zeichen `i`,
    /// `xs[n]` = rechter Rand der Zeile (Länge = Zeichenzahl + 1, aufsteigend).
    pub xs: Vec<f32>,
}

impl Line {
    /// Zeile mit gleichmäßig verteilten Zeichengrenzen.
    #[cfg(test)]
    pub fn uniform(text: &str, rect: [f32; 4]) -> Self {
        let n = text.chars().count();
        let xs = (0..=n).map(|i| rect[0] + rect[2] * i as f32 / n.max(1) as f32).collect();
        Self { text: text.into(), rect, xs }
    }
}

/// Zeichengrenzen einer Zeile aus Wort- und Zeichenboxen. `box_of(a, b)` liefert die x-Ausdehnung
/// der Zeichen `a..b` (oder `None`). Innerhalb eines Worts werden Einzelboxen genutzt, wenn sie
/// plausibel sind (aufsteigend, innerhalb des Worts), sonst nach den typischen Zeichenbreiten
/// `width_of` verteilt (ein „l“ ist schmaler als ein „m“); Leerzeichen und fehlende Werte werden
/// zwischen den Nachbarn interpoliert.
// Plattformneutral; unter Windows fehlt bisher die Texterkennung, die sie nutzt.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn char_bounds(
    chars: &[char],
    x0: f32,
    x1: f32,
    box_of: impl Fn(usize, usize) -> Option<(f32, f32)>,
    width_of: impl Fn(char) -> f32,
) -> Vec<f32> {
    let n = chars.len();
    let mut xs = vec![f32::NAN; n + 1];
    let mut i = 0;
    while i < n {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let j = (i..n).find(|&k| chars[k].is_whitespace()).unwrap_or(n);
        if let Some((l, r)) = box_of(i, j).filter(|(l, r)| r > l) {
            xs[i] = l;
            xs[j] = r;
            let inner: Vec<f32> = (i + 1..j).filter_map(|k| box_of(k, k + 1).map(|(a, _)| a)).collect();
            let precise = inner.len() == j - i - 1
                && inner.windows(2).all(|w| w[1] > w[0])
                && inner.iter().all(|x| *x > l && *x < r);
            let w: Vec<f32> = chars[i..j].iter().map(|c| width_of(*c).max(0.01)).collect();
            let total: f32 = w.iter().sum();
            let mut acc = 0.0;
            for (m, k) in (i + 1..j).enumerate() {
                acc += w[m];
                xs[k] = if precise { inner[m] } else { l + (r - l) * acc / total };
            }
        }
        i = j;
    }
    if xs[0].is_nan() {
        xs[0] = x0;
    }
    if xs[n].is_nan() {
        xs[n] = x1.max(xs[0]);
    }
    // Lücken linear füllen, danach monoton machen
    let mut k = 1;
    while k < n {
        if xs[k].is_nan() {
            let b = (k..=n).find(|&m| !xs[m].is_nan()).unwrap_or(n);
            let a = k - 1;
            for m in k..b {
                xs[m] = xs[a] + (xs[b] - xs[a]) * (m - a) as f32 / (b - a) as f32;
            }
            k = b;
        } else {
            k += 1;
        }
    }
    for m in 1..=n {
        xs[m] = xs[m].max(xs[m - 1]);
    }
    xs
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
    use objc2_foundation::{NSArray, NSDictionary, NSRange, NSString};
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
        let rect = [bx * wf, (1.0 - by - bh) * hf, bw * wf, bh * hf];
        // Zeichengrenzen über Boxen für Textbereiche (NSRange zählt UTF-16-Einheiten)
        let chars: Vec<char> = text.chars().collect();
        let mut u16 = Vec::with_capacity(chars.len() + 1);
        let mut o = 0;
        for c in &chars {
            u16.push(o);
            o += c.len_utf16();
        }
        u16.push(o);
        let box_of = |a: usize, b: usize| {
            let r = unsafe { best.boundingBoxForRange_error(NSRange::new(u16[a], u16[b] - u16[a])) }.ok()?;
            let rb = unsafe { r.boundingBox() };
            let (l, w) = (rb.origin.x as f32 * wf, rb.size.width as f32 * wf);
            (l.is_finite() && w > 0.0).then_some((l, l + w))
        };
        // Ersatz-Zeichenbreiten aus der eingebauten Schrift (Vision liefert oft nur Wortboxen)
        let width_of = |c: char| crate::render::text_size(c.encode_utf8(&mut [0; 4]), 100.0).0;
        let xs = char_bounds(&chars, rect[0], rect[0] + rect[2], box_of, width_of);
        lines.push(Line { text, rect, xs });
    }
    Ok(lines)
}

#[cfg(not(target_os = "macos"))]
pub fn recognize(_img: &RgbaImage) -> Result<Vec<Line>, String> {
    Err("Texterkennung ist auf diesem System noch nicht verfügbar".into())
}

/// Lesereihenfolge: Reihen von oben nach unten (Zeilen, die sich vertikal mindestens zur halben
/// Zeilenhöhe überlappen, bilden eine Reihe), innerhalb einer Reihe von links nach rechts.
/// Liefert Indizes in `lines`.
pub fn rows(lines: &[Line]) -> Vec<Vec<usize>> {
    let mut sorted: Vec<usize> = (0..lines.len()).collect();
    sorted.sort_by(|&a, &b| lines[a].rect[1].total_cmp(&lines[b].rect[1]));
    let mut rows: Vec<Vec<usize>> = vec![];
    for i in sorted {
        let l = &lines[i];
        let (top, bottom) = (l.rect[1], l.rect[1] + l.rect[3]);
        let same_row = rows.last().is_some_and(|row| {
            let r = &lines[row[0]];
            let overlap = bottom.min(r.rect[1] + r.rect[3]) - top.max(r.rect[1]);
            overlap > 0.5 * l.rect[3].min(r.rect[3])
        });
        if same_row {
            rows.last_mut().unwrap().push(i);
        } else {
            rows.push(vec![i]);
        }
    }
    for row in &mut rows {
        row.sort_by(|&a, &b| lines[a].rect[0].total_cmp(&lines[b].rect[0]));
    }
    rows
}

/// Zeilen in Lesereihenfolge zusammensetzen: Zeilen auf gleicher Höhe nebeneinander
/// (weiter Abstand = Tabulator, z. B. Tabellenspalten), größerer Zeilenabstand = Leerzeile.
pub fn to_text(lines: &[Line]) -> String {
    let rows: Vec<Vec<&Line>> = rows(lines).into_iter().map(|r| r.into_iter().map(|i| &lines[i]).collect()).collect();
    let mut out = String::new();
    let mut prev_bottom: Option<(f32, f32)> = None;
    for row in &rows {
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
        Line::uniform(text, [x, y, w, h])
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

        // Zeichengrenzen: „K“ beginnt dort, wo der Text „Hallo “ endet
        let first = lines.iter().find(|l| l.text.contains("Kadra")).unwrap();
        assert_eq!(first.xs.len(), first.text.chars().count() + 1);
        assert!(first.xs.windows(2).all(|w| w[1] >= w[0]), "aufsteigend: {:?}", first.xs);
        // Jede Grenze nahe der tatsächlich gerenderten Position (Breite des Textanfangs)
        let chars: Vec<char> = first.text.chars().collect();
        let errors: Vec<(String, f32)> = (1..chars.len())
            .filter(|&k| !chars[k - 1].is_whitespace() && !chars[k].is_whitespace())
            .map(|k| {
                let prefix: String = chars[..k].iter().collect();
                let expected = 30.0 + crate::render::text_size(&prefix, 56.0).0;
                (prefix, first.xs[k] - expected)
            })
            .collect();
        let worst = errors.iter().map(|(_, e)| e.abs()).fold(0.0, f32::max);
        assert!(worst < 8.0, "Zeichengrenzen weichen ab: {errors:?}");
    }

    #[test]
    fn char_bounds_use_word_boxes_and_fill_spaces() {
        let chars: Vec<char> = "ab cd".chars().collect();
        // Wortboxen: „ab“ 0–20, „cd“ 30–50; Einzelzeichen unbrauchbar (= Wortbox)
        let xs = char_bounds(&chars, 0.0, 50.0, |a, b| match (a, b) {
            (0, 2) | (0, 1) | (1, 2) => Some((0.0, 20.0)),
            (3, 5) | (3, 4) | (4, 5) => Some((30.0, 50.0)),
            _ => None,
        }, |_| 1.0);
        assert_eq!(xs, vec![0.0, 10.0, 20.0, 30.0, 40.0, 50.0]);
    }

    #[test]
    fn char_bounds_prefer_plausible_char_boxes() {
        let chars: Vec<char> = "Wil".chars().collect();
        let xs = char_bounds(&chars, 0.0, 30.0, |a, b| match (a, b) {
            (0, 3) => Some((0.0, 30.0)),
            (1, 2) => Some((18.0, 22.0)),
            (2, 3) => Some((22.0, 30.0)),
            _ => None,
        }, |_| 1.0);
        assert_eq!(xs, vec![0.0, 18.0, 22.0, 30.0]);
    }

    #[test]
    fn char_bounds_weight_by_char_width() {
        let chars: Vec<char> = "lm".chars().collect();
        let xs = char_bounds(&chars, 0.0, 40.0, |a, b| (a == 0 && b == 2).then_some((0.0, 40.0)), |c| if c == 'l' { 1.0 } else { 3.0 });
        assert_eq!(xs, vec![0.0, 10.0, 40.0]);
    }
}
