//! Rastert ein Dokument in ein flaches Bild (Export, Zwischenablage, Thumbnails).

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{imageops, Rgba as Px, RgbaImage};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Stroke, Transform,
};

use crate::doc::{rect, Annotation, Document, Effects, Rgba, Shape, P};

pub fn font() -> FontRef<'static> {
    FontRef::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT).expect("Schrift")
}

/// Pfeilspitze: Länge abhängig von der Strichstärke.
pub fn arrow_head(a: P, b: P, width: f32) -> (P, [P; 3]) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / len, dy / len);
    let head = (width * 4.0 + 8.0).min(len);
    let half = head * 0.55;
    let base = [b[0] - ux * head, b[1] - uy * head];
    let l = [base[0] - uy * half, base[1] + ux * half];
    let r = [base[0] + uy * half, base[1] - ux * half];
    // Schaft endet etwas innerhalb der Spitze, damit keine runde Kappe heraussteht.
    let shaft_end = [b[0] - ux * head * 0.8, b[1] - uy * head * 0.8];
    (shaft_end, [b, l, r])
}

pub fn step_radius(style_font: f32) -> f32 {
    style_font * 0.75
}

/// Wendet Weichzeichnen/Verpixeln auf eine Kopie des Bildes an.
pub fn apply_effects(base: &RgbaImage, anns: &[Annotation]) -> RgbaImage {
    let mut img = base.clone();
    for a in anns {
        let (pa, pb, blur) = match &a.shape {
            Shape::Blur { a, b } => (*a, *b, true),
            Shape::Pixelate { a, b } => (*a, *b, false),
            _ => continue,
        };
        let r = rect(pa, pb);
        let x0 = r.min.x.max(0.0) as u32;
        let y0 = r.min.y.max(0.0) as u32;
        let x1 = (r.max.x.max(0.0) as u32).min(img.width());
        let y1 = (r.max.y.max(0.0) as u32).min(img.height());
        if x1 <= x0 + 1 || y1 <= y0 + 1 {
            continue;
        }
        let (w, h) = (x1 - x0, y1 - y0);
        let sub = imageops::crop_imm(&img, x0, y0, w, h).to_image();
        let out = if blur {
            let sigma = (a.style.width * 2.0).max(4.0);
            imageops::fast_blur(&sub, sigma)
        } else {
            pixelate(&sub, (a.style.width * 3.0).max(6.0) as u32)
        };
        imageops::replace(&mut img, &out, x0 as i64, y0 as i64);
    }
    img
}

fn pixelate(img: &RgbaImage, block: u32) -> RgbaImage {
    let mut out = img.clone();
    let (w, h) = img.dimensions();
    for by in (0..h).step_by(block as usize) {
        for bx in (0..w).step_by(block as usize) {
            let (ex, ey) = ((bx + block).min(w), (by + block).min(h));
            let mut sum = [0u64; 4];
            let mut n = 0u64;
            for y in by..ey {
                for x in bx..ex {
                    let px = img.get_pixel(x, y).0;
                    for i in 0..4 {
                        sum[i] += px[i] as u64;
                    }
                    n += 1;
                }
            }
            let avg = Px([(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8, (sum[3] / n) as u8]);
            for y in by..ey {
                for x in bx..ex {
                    out.put_pixel(x, y, avg);
                }
            }
        }
    }
    out
}

fn to_pixmap(img: &RgbaImage) -> Pixmap {
    let mut pm = Pixmap::new(img.width(), img.height()).expect("Bildgröße");
    for (dst, src) in pm.data_mut().chunks_exact_mut(4).zip(img.as_raw().chunks_exact(4)) {
        let a = src[3] as u32;
        dst[0] = (src[0] as u32 * a / 255) as u8;
        dst[1] = (src[1] as u32 * a / 255) as u8;
        dst[2] = (src[2] as u32 * a / 255) as u8;
        dst[3] = src[3];
    }
    pm
}

fn from_pixmap(pm: &Pixmap) -> RgbaImage {
    let mut img = RgbaImage::new(pm.width(), pm.height());
    for (dst, src) in img.as_mut().chunks_exact_mut(4).zip(pm.data().chunks_exact(4)) {
        let a = src[3] as u32;
        if a == 0 {
            dst.copy_from_slice(&[0, 0, 0, 0]);
        } else {
            dst[0] = ((src[0] as u32 * 255) / a).min(255) as u8;
            dst[1] = ((src[1] as u32 * 255) / a).min(255) as u8;
            dst[2] = ((src[2] as u32 * 255) / a).min(255) as u8;
            dst[3] = src[3];
        }
    }
    img
}

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(Color::from_rgba8(c[0], c[1], c[2], c[3]));
    p.anti_alias = true;
    p
}

fn stroke(width: f32) -> Stroke {
    Stroke { width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() }
}

fn rect_path(a: P, b: P) -> Option<Path> {
    let r = rect(a, b);
    tiny_skia::Rect::from_ltrb(r.min.x, r.min.y, r.max.x, r.max.y).map(PathBuilder::from_rect)
}

fn oval_path(a: P, b: P) -> Option<Path> {
    let r = rect(a, b);
    tiny_skia::Rect::from_ltrb(r.min.x, r.min.y, r.max.x, r.max.y).and_then(PathBuilder::from_oval)
}

fn poly_path(pts: &[P], close: bool) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let (first, rest) = pts.split_first()?;
    pb.move_to(first[0], first[1]);
    for q in rest {
        pb.line_to(q[0], q[1]);
    }
    if close {
        pb.close();
    }
    pb.finish()
}

/// Zeichnet Text mit ab_glyph; `size` = Em-Größe in Pixeln (wie egui).
pub fn draw_text(pm: &mut Pixmap, x: f32, y: f32, text: &str, size: f32, color: Rgba) {
    let f = font();
    let px = PxScale::from(size * f.height_unscaled() / f.units_per_em().unwrap_or(1000.0));
    let sf = f.as_scaled(px);
    let line_h = sf.height() + sf.line_gap();
    let (w, h) = (pm.width() as i32, pm.height() as i32);
    let data = pm.data_mut();
    for (li, line) in text.lines().enumerate() {
        let mut caret = ab_glyph::point(x, y + sf.ascent() + li as f32 * line_h);
        let mut prev = None;
        for ch in line.chars() {
            let gid = f.glyph_id(ch);
            if let Some(pg) = prev {
                caret.x += sf.kern(pg, gid);
            }
            let glyph = gid.with_scale_and_position(px, caret);
            caret.x += sf.h_advance(gid);
            prev = Some(gid);
            let Some(og) = f.outline_glyph(glyph) else { continue };
            let bb = og.px_bounds();
            og.draw(|gx, gy, cov| {
                let (ix, iy) = (bb.min.x as i32 + gx as i32, bb.min.y as i32 + gy as i32);
                if ix < 0 || iy < 0 || ix >= w || iy >= h {
                    return;
                }
                let a = cov.clamp(0.0, 1.0) * color[3] as f32 / 255.0;
                let i = ((iy * w + ix) * 4) as usize;
                for c in 0..3 {
                    let src = color[c] as f32 * a;
                    data[i + c] = (src + data[i + c] as f32 * (1.0 - a)) as u8;
                }
                data[i + 3] = (a * 255.0 + data[i + 3] as f32 * (1.0 - a)) as u8;
            });
        }
    }
}

pub fn text_size(text: &str, size: f32) -> (f32, f32) {
    let f = font();
    let px = PxScale::from(size * f.height_unscaled() / f.units_per_em().unwrap_or(1000.0));
    let sf = f.as_scaled(px);
    let mut maxw: f32 = 0.0;
    let mut n = 0;
    for line in text.lines() {
        n += 1;
        let w: f32 = line.chars().map(|c| sf.h_advance(f.glyph_id(c))).sum();
        maxw = maxw.max(w);
    }
    (maxw, n.max(1) as f32 * (sf.height() + sf.line_gap()))
}

fn draw_shape(pm: &mut Pixmap, ann: &Annotation, off: f32, shadow: bool) {
    let st = &ann.style;
    let col = if shadow { [0, 0, 0, 90] } else { st.stroke };
    let fill = if shadow { st.fill.map(|_| [0, 0, 0, 90]) } else { st.fill };
    let t = Transform::from_translate(off, off);
    let sk = stroke(st.width);
    match &ann.shape {
        Shape::Arrow { a, b } => {
            let (end, tri) = arrow_head(*a, *b, st.width);
            if let Some(path) = poly_path(&[*a, end], false) {
                pm.stroke_path(&path, &paint(col), &sk, t, None);
            }
            if let Some(path) = poly_path(&tri, true) {
                pm.fill_path(&path, &paint(col), FillRule::Winding, t, None);
            }
        }
        Shape::Line { a, b } => {
            if let Some(path) = poly_path(&[*a, *b], false) {
                pm.stroke_path(&path, &paint(col), &sk, t, None);
            }
        }
        Shape::Rect { a, b } | Shape::Ellipse { a, b } => {
            let path = if matches!(ann.shape, Shape::Rect { .. }) { rect_path(*a, *b) } else { oval_path(*a, *b) };
            if let Some(path) = path {
                if let Some(f) = fill {
                    pm.fill_path(&path, &paint(f), FillRule::Winding, t, None);
                }
                pm.stroke_path(&path, &paint(col), &sk, t, None);
            }
        }
        Shape::Pen { pts } => {
            if let Some(path) = poly_path(pts, false) {
                pm.stroke_path(&path, &paint(col), &sk, t, None);
            }
        }
        Shape::Highlight { a, b } => {
            if !shadow {
                if let Some(path) = rect_path(*a, *b) {
                    let c = st.stroke;
                    pm.fill_path(&path, &paint([c[0], c[1], c[2], 100]), FillRule::Winding, t, None);
                }
            }
        }
        Shape::Text { pos, text } => {
            let (w, h) = text_size(text, st.font_size);
            let pad = st.font_size * 0.25;
            if let Some(f) = fill {
                if let Some(path) = rect_path([pos[0] - pad, pos[1] - pad], [pos[0] + w + pad, pos[1] + h + pad]) {
                    pm.fill_path(&path, &paint(f), FillRule::Winding, t, None);
                }
            }
            if shadow {
                if st.fill.is_none() {
                    draw_text(pm, pos[0] + off, pos[1] + off, text, st.font_size, col);
                }
            } else {
                draw_text(pm, pos[0], pos[1], text, st.font_size, st.stroke);
            }
        }
        Shape::Step { pos, n } => {
            let r = step_radius(st.font_size);
            if let Some(path) = PathBuilder::from_circle(pos[0], pos[1], r) {
                pm.fill_path(&path, &paint(col), FillRule::Winding, t, None);
            }
            if !shadow {
                let s = n.to_string();
                let fs = st.font_size;
                let (w, h) = text_size(&s, fs);
                draw_text(pm, pos[0] - w / 2.0, pos[1] - h / 2.0, &s, fs, [255, 255, 255, 255]);
            }
        }
        Shape::Spotlight { a, b } => {
            if !shadow {
                let mut pb = PathBuilder::new();
                pb.push_rect(tiny_skia::Rect::from_xywh(0.0, 0.0, pm.width() as f32, pm.height() as f32).unwrap());
                let r = rect(*a, *b);
                if let Some(inner) = tiny_skia::Rect::from_ltrb(r.min.x, r.min.y, r.max.x, r.max.y) {
                    pb.push_rect(inner);
                }
                if let Some(path) = pb.finish() {
                    pm.fill_path(&path, &paint([0, 0, 0, 150]), FillRule::EvenOdd, Transform::identity(), None);
                }
            }
        }
        Shape::Blur { .. } | Shape::Pixelate { .. } => {}
    }
}

pub fn casts_shadow(a: &Annotation) -> bool {
    a.style.shadow && !matches!(a.shape, Shape::Highlight { .. } | Shape::Spotlight { .. } | Shape::Blur { .. } | Shape::Pixelate { .. })
}

pub fn shadow_offset(a: &Annotation) -> f32 {
    (a.style.width * 0.6).max(3.0)
}

/// Spotlights zuerst, damit sie nur das Bild und nicht die Anmerkungen abdunkeln.
pub fn draw_order(anns: &[Annotation]) -> impl Iterator<Item = &Annotation> {
    let spot = |a: &&Annotation| matches!(a.shape, Shape::Spotlight { .. });
    anns.iter().filter(spot).chain(anns.iter().filter(move |a| !spot(a)))
}

/// Rendert das komplette Dokument inkl. Effekten.
pub fn render(doc: &Document) -> RgbaImage {
    let base = apply_effects(&doc.image, &doc.annotations);
    let mut pm = to_pixmap(&base);
    for a in draw_order(&doc.annotations) {
        if casts_shadow(a) {
            draw_shape(&mut pm, a, shadow_offset(a), true);
        }
        draw_shape(&mut pm, a, 0.0, false);
    }
    let img = from_pixmap(&pm);
    apply_frame_effects(img, &doc.effects)
}

/// Rahmen, abgerundete Ecken, Schlagschatten.
pub fn apply_frame_effects(mut img: RgbaImage, fx: &Effects) -> RgbaImage {
    if let Some((w, c)) = fx.border {
        let w = w.max(0.0) as u32;
        if w > 0 {
            let mut out = RgbaImage::from_pixel(img.width() + 2 * w, img.height() + 2 * w, Px(c));
            imageops::overlay(&mut out, &img, w as i64, w as i64);
            img = out;
        }
    }
    if fx.round_corners > 0.5 {
        let r = fx.round_corners.min(img.width().min(img.height()) as f32 / 2.0);
        let (w, h) = (img.width() as f32, img.height() as f32);
        for (x, y, px) in img.enumerate_pixels_mut() {
            let (fx_, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let cx = if fx_ < r { r } else if fx_ > w - r { w - r } else { continue };
            let cy = if fy < r { r } else if fy > h - r { h - r } else { continue };
            let d = ((fx_ - cx).powi(2) + (fy - cy).powi(2)).sqrt();
            let cov = (r - d + 0.5).clamp(0.0, 1.0);
            px.0[3] = (px.0[3] as f32 * cov) as u8;
        }
    }
    if fx.drop_shadow {
        let blur = 14.0;
        let off = 8u32;
        let pad = (blur * 2.0) as u32;
        let (w, h) = (img.width() + pad * 2 + off, img.height() + pad * 2 + off);
        let mut shadow = RgbaImage::new(w, h);
        for (x, y, px) in img.enumerate_pixels() {
            let a = (px.0[3] as f32 * 0.55) as u8;
            shadow.put_pixel(x + pad + off, y + pad + off, Px([0, 0, 0, a]));
        }
        let mut out = imageops::blur(&shadow, blur / 2.0);
        imageops::overlay(&mut out, &img, pad as i64, pad as i64);
        img = out;
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Annotation, Document, Effects, Style};

    #[test]
    fn renders_all_shapes() {
        let mut img = RgbaImage::new(600, 400);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Px([(x % 256) as u8, (y % 256) as u8, 180, 255]);
        }
        let mut doc = Document::new(img);
        let st = Style::default();
        let shapes = vec![
            Shape::Arrow { a: [20.0, 20.0], b: [200.0, 120.0] },
            Shape::Rect { a: [250.0, 20.0], b: [400.0, 120.0] },
            Shape::Ellipse { a: [420.0, 20.0], b: [580.0, 120.0] },
            Shape::Pen { pts: vec![[20.0, 200.0], [60.0, 160.0], [100.0, 220.0]] },
            Shape::Highlight { a: [120.0, 150.0], b: [300.0, 190.0] },
            Shape::Text { pos: [120.0, 200.0], text: "Hallo SnapIt\nZeile 2".into() },
            Shape::Step { pos: [400.0, 200.0], n: 7 },
            Shape::Blur { a: [20.0, 280.0], b: [200.0, 380.0] },
            Shape::Pixelate { a: [220.0, 280.0], b: [400.0, 380.0] },
            Shape::Spotlight { a: [420.0, 260.0], b: [580.0, 390.0] },
        ];
        doc.annotations = shapes.into_iter().map(|shape| Annotation { shape, style: st.clone() }).collect();
        doc.effects = Effects { border: Some((6.0, [30, 30, 30, 255])), drop_shadow: true, round_corners: 12.0 };
        let out = render(&doc);
        assert!(out.width() > 600 && out.height() > 400);
        if let Ok(p) = std::env::var("SNAPIT_TEST_OUT") {
            out.save(p).unwrap();
        }
    }
}

/// Erstellt ein verkleinertes Vorschaubild.
pub fn thumbnail(img: &RgbaImage, max: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let s = (max as f32 / w.max(h) as f32).min(1.0);
    imageops::thumbnail(img, ((w as f32 * s) as u32).max(1), ((h as f32 * s) as u32).max(1))
}
