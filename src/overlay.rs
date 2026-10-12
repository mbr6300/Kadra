//! Vollbild-Overlay zur Auswahl von Bereich oder Fenster auf einem eingefrorenen Screenshot.

use egui::{
    Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions,
    Vec2,
};
use image::RgbaImage;

use crate::capture::{self, CaptureKind, Grab};

pub enum OverlayResult {
    Pending,
    Cancel,
    Done(RgbaImage),
}

pub struct Overlay {
    pub grab: Grab,
    texture: TextureHandle,
}

const ACCENT: Color32 = Color32::from_rgb(255, 120, 30);

impl Overlay {
    pub fn new(ctx: &egui::Context, grab: Grab) -> Self {
        let img = &grab.image;
        let color = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        let texture = ctx.load_texture("overlay", color, TextureOptions::NEAREST);
        Self { grab, texture }
    }

    fn window_at(&self, p: Pos2) -> Option<&capture::WinInfo> {
        self.grab.windows.iter().find(|w| w.rect.contains(p))
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> OverlayResult {
        let mut result = OverlayResult::Pending;
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            return OverlayResult::Cancel;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            return OverlayResult::Done(self.grab.image.clone());
        }

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let full = ui.max_rect();
            let resp = ui.allocate_rect(full, Sense::click_and_drag());
            let painter = ui.painter_at(full);
            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            painter.image(self.texture.id(), Rect::from_min_size(full.min, self.grab.size), uv, Color32::WHITE);

            let to_local = |p: Pos2| (p - full.min).to_pos2();
            let to_screen = |r: Rect| r.translate(full.min.to_vec2());
            let cursor = ctx.input(|i| i.pointer.hover_pos()).map(to_local);
            let press = ctx.input(|i| i.pointer.press_origin()).map(to_local);
            let region_mode = self.grab.kind == CaptureKind::Region;

            let dragging = region_mode
                && resp.dragged()
                && press.zip(cursor).is_some_and(|(a, b)| (a - b).length() > 4.0);
            let hovered_win = cursor.and_then(|c| self.window_at(c)).cloned();

            let selection: Option<Rect> = if dragging {
                press.zip(cursor).map(|(a, b)| Rect::from_two_pos(a, b))
            } else {
                hovered_win.as_ref().map(|w| w.rect.intersect(Rect::from_min_size(Pos2::ZERO, self.grab.size)))
            };

            // Abdunkeln außerhalb der Auswahl
            let dim = Color32::from_black_alpha(110);
            match selection {
                Some(sel) => {
                    let s = to_screen(sel);
                    painter.rect_filled(Rect::from_min_max(full.min, Pos2::new(full.max.x, s.min.y)), 0.0, dim);
                    painter.rect_filled(Rect::from_min_max(Pos2::new(full.min.x, s.max.y), full.max), 0.0, dim);
                    painter.rect_filled(Rect::from_min_max(Pos2::new(full.min.x, s.min.y), Pos2::new(s.min.x, s.max.y)), 0.0, dim);
                    painter.rect_filled(Rect::from_min_max(Pos2::new(s.max.x, s.min.y), Pos2::new(full.max.x, s.max.y)), 0.0, dim);
                    painter.rect_stroke(s, 0.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
                    let px = sel.size() * self.grab.scale();
                    let mut label = format!("{} × {}", px.x.round(), px.y.round());
                    if !dragging {
                        if let Some(w) = &hovered_win {
                            label = format!("{} – {}   {label}", w.app, w.title);
                        }
                    }
                    let lp = Pos2::new(s.min.x, (s.min.y - 26.0).max(full.min.y + 2.0));
                    let galley = painter.layout_no_wrap(label, FontId::proportional(13.0), Color32::WHITE);
                    let bg = Rect::from_min_size(lp, galley.size() + Vec2::new(12.0, 6.0));
                    painter.rect_filled(bg, CornerRadius::same(4), Color32::from_black_alpha(200));
                    painter.galley(lp + Vec2::new(6.0, 3.0), galley, Color32::WHITE);
                }
                None => {
                    painter.rect_filled(full, 0.0, dim);
                }
            }

            // Fadenkreuz + Lupe
            if let Some(c) = cursor {
                if region_mode {
                    let sc = c + full.min.to_vec2();
                    let cross = Stroke::new(1.0, Color32::from_white_alpha(120));
                    painter.line_segment([Pos2::new(full.min.x, sc.y), Pos2::new(full.max.x, sc.y)], cross);
                    painter.line_segment([Pos2::new(sc.x, full.min.y), Pos2::new(sc.x, full.max.y)], cross);
                }
                self.magnifier(&painter, full, c);
            }

            // Hinweis
            let hint = if region_mode {
                "Ziehen: Bereich   •   Klick: Fenster   •   Enter: ganzer Bildschirm   •   Esc: Abbrechen"
            } else {
                "Klick: Fenster aufnehmen   •   Enter: ganzer Bildschirm   •   Esc: Abbrechen"
            };
            let galley = painter.layout_no_wrap(hint.into(), FontId::proportional(14.0), Color32::WHITE);
            let pos = Pos2::new(full.center().x - galley.size().x / 2.0, full.min.y + 40.0);
            painter.rect_filled(
                Rect::from_min_size(pos - Vec2::new(12.0, 8.0), galley.size() + Vec2::new(24.0, 16.0)),
                CornerRadius::same(8),
                Color32::from_black_alpha(190),
            );
            painter.galley(pos, galley, Color32::WHITE);

            if resp.drag_stopped() && dragging {
                if let Some(img) = selection.and_then(|s| self.grab.crop(s)) {
                    result = OverlayResult::Done(img);
                }
            } else if resp.clicked() || (resp.drag_stopped() && !dragging) {
                if let Some(w) = hovered_win {
                    // Ausschnitt aus dem eingefrorenen Bild = genau das, was sichtbar war.
                    if let Some(img) = self.grab.crop(w.rect) {
                        result = OverlayResult::Done(img);
                    }
                } else if region_mode {
                    result = OverlayResult::Done(self.grab.image.clone());
                }
            }
        });
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        ctx.request_repaint();
        result
    }

    fn magnifier(&self, painter: &egui::Painter, full: Rect, c: Pos2) {
        let box_size = 130.0;
        let src = 16.0; // logische Punkte im Ausschnitt
        let mut at = c + full.min.to_vec2() + Vec2::new(24.0, 24.0);
        if at.x + box_size > full.max.x {
            at.x -= box_size + 48.0;
        }
        if at.y + box_size + 22.0 > full.max.y {
            at.y -= box_size + 70.0;
        }
        let dst = Rect::from_min_size(at, Vec2::splat(box_size));
        let size = self.grab.size;
        let uv = Rect::from_center_size(
            Pos2::new(c.x / size.x, c.y / size.y),
            Vec2::new(src / size.x, src / size.y),
        );
        painter.rect_filled(dst.expand(2.0), CornerRadius::same(4), Color32::BLACK);
        painter.image(self.texture.id(), dst, uv, Color32::WHITE);
        let mid = dst.center();
        let st = Stroke::new(1.0, ACCENT);
        painter.line_segment([Pos2::new(dst.min.x, mid.y), Pos2::new(dst.max.x, mid.y)], st);
        painter.line_segment([Pos2::new(mid.x, dst.min.y), Pos2::new(mid.x, dst.max.y)], st);
        painter.rect_stroke(dst, CornerRadius::same(4), Stroke::new(1.5, Color32::WHITE), StrokeKind::Outside);

        // Pixelkoordinate + Farbe unter dem Cursor
        let s = self.grab.scale();
        let (px, py) = ((c.x * s) as u32, (c.y * s) as u32);
        if let Some(col) = self.grab.image.get_pixel_checked(px, py) {
            let [r, g, b, _] = col.0;
            let txt = format!("{px}, {py}   #{r:02X}{g:02X}{b:02X}");
            let galley = painter.layout_no_wrap(txt, FontId::monospace(11.0), Color32::WHITE);
            let lp = Pos2::new(dst.min.x, dst.max.y + 4.0);
            painter.rect_filled(
                Rect::from_min_size(lp, Vec2::new(box_size, galley.size().y + 6.0)),
                CornerRadius::same(3),
                Color32::from_black_alpha(200),
            );
            painter.galley(lp + Vec2::new(4.0, 3.0), galley, Color32::WHITE);
        }
    }
}
