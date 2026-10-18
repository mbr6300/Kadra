//! App-Icon: abgerundetes Quadrat in Kadra-Orange mit weißem Sucher (Ecken) und Auslöser-Punkt.
//! Wird für das Dock zur Laufzeit und für das Iconset im App-Paket (examples/make_icon.rs) genutzt.

use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point, Shader, SpreadMode, Stroke,
    Transform,
};

/// Rendert das Icon quadratisch in `size` Pixeln (macOS-Raster: Fläche 824/1024, zentriert).
pub fn render(size: u32) -> Pixmap {
    render_inset(size, 100.0 / 1024.0)
}

/// Fast randlos – für Windows (Taskleiste, Infobereich, .ico), wo Icons die Fläche füllen.
pub fn render_full(size: u32) -> Pixmap {
    render_inset(size, 0.03)
}

/// `inset`: Rand als Anteil der Kantenlänge.
fn render_inset(size: u32, inset: f32) -> Pixmap {
    let s = size as f32;
    let mut pm = Pixmap::new(size, size).unwrap();
    let inset = s * inset;
    let side = s - 2.0 * inset;
    let r = side * 0.225;

    let body = rounded_rect(inset, inset, side, side, r);
    let mut shadow = Paint::default();
    shadow.set_color(Color::from_rgba8(0, 0, 0, 60));
    shadow.anti_alias = true;
    pm.fill_path(&body, &shadow, FillRule::Winding, Transform::from_translate(0.0, s * 0.012), None);

    let mut paint = Paint::default();
    paint.anti_alias = true;
    paint.shader = LinearGradient::new(
        Point::from_xy(0.0, inset),
        Point::from_xy(0.0, inset + side),
        vec![
            GradientStop::new(0.0, Color::from_rgba8(224, 112, 60, 255)),
            GradientStop::new(1.0, Color::from_rgba8(176, 72, 32, 255)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap_or(Shader::SolidColor(Color::from_rgba8(186, 82, 40, 255)));
    pm.fill_path(&body, &paint, FillRule::Winding, Transform::identity(), None);

    // Sucher-Ecken
    let mut white = Paint::default();
    white.set_color(Color::WHITE);
    white.anti_alias = true;
    let stroke = Stroke { width: side * 0.07, line_cap: tiny_skia::LineCap::Round, ..Default::default() };
    let (a, b) = (inset + side * 0.24, inset + side * 0.76);
    let arm = side * 0.16;
    for (x, y, dx, dy) in [(a, a, 1.0, 1.0), (b, a, -1.0, 1.0), (a, b, 1.0, -1.0), (b, b, -1.0, -1.0)] {
        let mut pb = PathBuilder::new();
        pb.move_to(x, y + dy * arm);
        pb.line_to(x, y);
        pb.line_to(x + dx * arm, y);
        pm.stroke_path(&pb.finish().unwrap(), &white, &stroke, Transform::identity(), None);
    }
    // Auslöser-Punkt
    let c = s / 2.0;
    let dot = PathBuilder::from_circle(c, c, side * 0.085).unwrap();
    pm.fill_path(&dot, &white, FillRule::Winding, Transform::identity(), None);
    pm
}

/// Icon als nicht-vormultipliziertes RGBA (für das Dock-Icon).
#[allow(dead_code)]
pub fn rgba(size: u32) -> Vec<u8> {
    to_rgba(&render(size))
}

/// Wie `rgba`, aber fast randlos (Windows).
#[allow(dead_code)]
pub fn rgba_full(size: u32) -> Vec<u8> {
    to_rgba(&render_full(size))
}

fn to_rgba(pm: &Pixmap) -> Vec<u8> {
    pm.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn dock_icon_pixels() {
        let size = 128;
        let px = super::rgba(size);
        assert_eq!(px.len(), (size * size * 4) as usize);
        let at = |x: u32, y: u32| &px[((y * size + x) * 4) as usize..][..4];
        assert_eq!(at(0, 0)[3], 0, "Ecke transparent");
        assert_eq!(at(size / 2, size / 2), [255, 255, 255, 255], "Auslöser-Punkt weiß");
        let body = at(size / 2, size * 3 / 10);
        assert!(body[0] > 150 && body[2] < 100 && body[3] == 255, "Fläche orange: {body:?}");
    }
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let k = 0.5523 * r; // Bézier-Näherung für Viertelkreise
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish().unwrap()
}
