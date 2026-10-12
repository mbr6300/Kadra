//! Vollbild-Overlay zur Auswahl von Bereich oder Fenster auf einem eingefrorenen Screenshot.

use egui::{
    Color32, CornerRadius, FontId, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle,
    TextureOptions, Vec2,
};
use image::RgbaImage;

use crate::capture::{CaptureKind, Grab, WinInfo};

pub enum OverlayResult {
    Pending,
    Cancel,
    Done(RgbaImage),
}

pub struct Overlay {
    pub grab: Grab,
    texture: TextureHandle,
    /// Startpunkt eines laufenden Ziehens (lokale Koordinaten).
    drag_start: Option<Pos2>,
    /// Tab-Index im Fensterstapel unter dem Mauszeiger.
    cycle: usize,
    last_stack: Vec<u32>,
    show_list: bool,
    filter: String,
    message: Option<String>,
}

const ACCENT: Color32 = crate::ui_kit::ORANGE;

impl Overlay {
    pub fn new(ctx: &egui::Context, grab: Grab) -> Self {
        let img = &grab.image;
        let color = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        let texture = ctx.load_texture("overlay", color, TextureOptions::NEAREST);
        let show_list = grab.kind == CaptureKind::Window;
        Self {
            grab,
            texture,
            drag_start: None,
            cycle: 0,
            last_stack: vec![],
            show_list,
            filter: String::new(),
            message: None,
        }
    }

    fn screen_rect(&self) -> Rect {
        Rect::from_min_size(Pos2::ZERO, self.grab.size)
    }

    /// Sichtbare Fenster unter dem Punkt, vorderstes zuerst.
    fn stack_at(&self, p: Pos2) -> Vec<WinInfo> {
        self.grab.windows.iter().filter(|w| w.on_screen && w.rect.contains(p)).cloned().collect()
    }

    /// Ist das Fenster (teilweise) von einem davorliegenden verdeckt?
    fn is_covered(&self, w: &WinInfo) -> bool {
        self.grab
            .windows
            .iter()
            .take_while(|o| o.id != w.id)
            .any(|o| o.on_screen && o.rect.intersects(w.rect))
    }

    /// Fensterinhalt direkt aufnehmen; sichtbare Fenster notfalls aus dem Standbild schneiden.
    fn capture(&mut self, w: &WinInfo) -> Option<RgbaImage> {
        let img = crate::platform::capture_window(w.id)
            .or_else(|| (w.on_screen && !self.is_covered(w)).then(|| self.grab.crop(w.rect)).flatten());
        if img.is_none() {
            self.message = Some(if w.on_screen {
                format!("„{}“ konnte nicht aufgenommen werden.", w.label())
            } else {
                format!("„{}“ ist minimiert – bitte zuerst wiederherstellen.", w.label())
            });
        }
        img
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> OverlayResult {
        let mut result = OverlayResult::Pending;
        let typing = ctx.wants_keyboard_input();
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            return OverlayResult::Cancel;
        }
        if !typing && ctx.input(|i| i.key_pressed(Key::Enter)) {
            return OverlayResult::Done(self.grab.image.clone());
        }
        // Tab / ⇧Tab: durch übereinanderliegende Fenster blättern
        let (next, prev) = ctx.input_mut(|i| {
            (i.consume_key(Modifiers::NONE, Key::Tab), i.consume_key(Modifiers::SHIFT, Key::Tab))
        });
        if next {
            self.cycle = self.cycle.wrapping_add(1);
        }
        if prev {
            self.cycle = self.cycle.wrapping_sub(1);
        }
        if !typing && ctx.input(|i| i.key_pressed(Key::L)) {
            self.show_list = !self.show_list;
        }

        let list_pick = self.window_list(ctx);
        if let Some(w) = &list_pick.clicked {
            if let Some(img) = self.capture(w) {
                return OverlayResult::Done(img);
            }
        }

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let full = ui.max_rect();
            let resp = ui.allocate_rect(full, Sense::click_and_drag());
            let painter = ui.painter_at(full);
            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            painter.image(self.texture.id(), Rect::from_min_size(full.min, self.grab.size), uv, Color32::WHITE);

            let to_local = |p: Pos2| (p - full.min).to_pos2();
            let to_screen = |r: Rect| r.translate(full.min.to_vec2());
            let (cursor, pressed, released) = ctx.input(|i| {
                (i.pointer.latest_pos().map(to_local), i.pointer.primary_pressed(), i.pointer.primary_released())
            });
            let region_mode = self.grab.kind == CaptureKind::Region;

            // Ziehen selbst verfolgen: beim Loslassen ist `dragged()` bereits false.
            if pressed && resp.hovered() {
                self.drag_start = cursor;
                self.message = None;
            }
            let drag = self.drag_start.zip(cursor);
            let dragging = region_mode && drag.is_some_and(|(a, b)| (a - b).length() > 4.0);

            // Fenster unter dem Cursor (Tab wählt dahinterliegende)
            let stack = if resp.hovered() { cursor.map(|c| self.stack_at(c)).unwrap_or_default() } else { vec![] };
            let ids: Vec<u32> = stack.iter().map(|w| w.id).collect();
            if ids != self.last_stack {
                self.last_stack = ids;
                self.cycle = 0;
            }
            let chosen = (!stack.is_empty()).then(|| stack[self.cycle % stack.len()].clone());
            let target = list_pick.hovered.clone().or(chosen);
            let behind = target.as_ref().is_some_and(|w| self.is_covered(w));

            let selection: Option<Rect> = if dragging {
                drag.map(|(a, b)| Rect::from_two_pos(a, b))
            } else {
                target
                    .as_ref()
                    .filter(|w| w.on_screen && w.rect.intersects(self.screen_rect()))
                    .map(|w| w.rect.intersect(self.screen_rect()))
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
                    if behind && !dragging {
                        // Verdecktes Fenster: gestrichelter Rahmen
                        let pts = [s.left_top(), s.right_top(), s.right_bottom(), s.left_bottom(), s.left_top()];
                        painter.extend(egui::Shape::dashed_line(&pts, Stroke::new(2.5, ACCENT), 10.0, 6.0));
                    } else {
                        painter.rect_stroke(s, 0.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
                    }
                    let px = sel.size() * self.grab.scale();
                    let mut label = format!("{} × {}", px.x.round(), px.y.round());
                    if !dragging {
                        if let Some(w) = &target {
                            label = format!("{}   {label}", w.label());
                            if stack.len() > 1 && list_pick.hovered.is_none() {
                                label += &format!("   ({}/{} · Tab = dahinter)", self.cycle % stack.len() + 1, stack.len());
                            } else if behind {
                                label += "   (verdeckt)";
                            }
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
            if let Some(c) = cursor.filter(|_| resp.hovered()) {
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
                "Ziehen: Bereich  •  Klick: Fenster  •  Tab: dahinterliegendes Fenster  •  L: Fensterliste  •  Enter: ganzer Bildschirm  •  Esc"
            } else {
                "Klick: Fenster  •  Tab: dahinterliegendes Fenster  •  L: Fensterliste  •  Enter: ganzer Bildschirm  •  Esc"
            };
            let text = self.message.as_deref().unwrap_or(hint).to_string();
            let color = if self.message.is_some() { Color32::from_rgb(255, 200, 120) } else { Color32::WHITE };
            let galley = painter.layout_no_wrap(text, FontId::proportional(14.0), color);
            let pos = Pos2::new(full.center().x - galley.size().x / 2.0, full.min.y + 40.0);
            painter.rect_filled(
                Rect::from_min_size(pos - Vec2::new(12.0, 8.0), galley.size() + Vec2::new(24.0, 16.0)),
                CornerRadius::same(8),
                Color32::from_black_alpha(190),
            );
            painter.galley(pos, galley, color);

            if released && self.drag_start.take().is_some() {
                if dragging {
                    if let Some(img) = selection.and_then(|s| self.grab.crop(s)) {
                        result = OverlayResult::Done(img);
                    }
                } else if let Some(w) = target {
                    if let Some(img) = self.capture(&w) {
                        result = OverlayResult::Done(img);
                    }
                } else if region_mode {
                    result = OverlayResult::Done(self.grab.image.clone());
                }
            }
        });
        if !ctx.is_pointer_over_area() || ctx.input(|i| i.pointer.primary_down()) {
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        ctx.request_repaint();
        result
    }

    /// Fensterliste mit Suchfeld; liefert überfahrenes bzw. angeklicktes Fenster.
    fn window_list(&mut self, ctx: &egui::Context) -> ListPick {
        let mut pick = ListPick::default();
        if !self.show_list {
            return pick;
        }
        let max_h = self.grab.size.y * 0.7;
        egui::Window::new("Fenster auswählen")
            .default_pos([24.0, 90.0])
            .default_width(380.0)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Suchen…").desired_width(f32::INFINITY));
                });
                ui.weak("Auch verdeckte Fenster und solche auf anderen Schreibtischen.");
                ui.separator();
                let needle = self.filter.to_lowercase();
                egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
                    for w in self.grab.windows.iter() {
                        let label = w.label();
                        if !needle.is_empty() && !label.to_lowercase().contains(&needle) {
                            continue;
                        }
                        let status = if !w.on_screen {
                            "anderer Schreibtisch / minimiert"
                        } else if !w.rect.intersects(self.screen_rect()) {
                            "anderer Bildschirm"
                        } else if self.is_covered(w) {
                            "verdeckt"
                        } else {
                            "sichtbar"
                        };
                        let resp = ui
                            .add(egui::Button::new(format!("{label}\n{status}")).wrap().min_size(Vec2::new(ui.available_width(), 0.0)))
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if resp.hovered() {
                            pick.hovered = Some(w.clone());
                        }
                        if resp.clicked() {
                            pick.clicked = Some(w.clone());
                        }
                    }
                });
            });
        pick
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

#[derive(Default)]
struct ListPick {
    hovered: Option<WinInfo>,
    clicked: Option<WinInfo>,
}
