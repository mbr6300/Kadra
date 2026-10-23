//! Gemeinsame UI-Bausteine: Signaturfarbe, Leisten-Buttons, Dialog-Buttons und Grundstil.

use egui::{Align2, Color32, CornerRadius, FontId, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

/// Kadra-Signaturfarbe als Fläche (weiße Schrift darauf ≈ 4,8 : 1 Kontrast).
pub const ORANGE: Color32 = Color32::from_rgb(186, 82, 40);
pub const ORANGE_HOVER: Color32 = Color32::from_rgb(200, 92, 48);
/// Hellere Variante für orange Linien/Symbole auf dunklem Grund.
pub const ORANGE_LIGHT: Color32 = Color32::from_rgb(232, 138, 94);

/// Orange für Vordergrund (Symbole, Rahmen) passend zum Hell-/Dunkelmodus.
pub fn accent_fg(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode { ORANGE_LIGHT } else { ORANGE }
}

pub const BAR_HEIGHT: f32 = 64.0;
pub const TOOL_WIDTH: f32 = 62.0;

#[derive(Clone, Copy)]
pub enum Look {
    /// Normales Werkzeug, ggf. ausgewählt.
    Tool { selected: bool },
    /// Gefüllter Hauptbutton in Signaturfarbe.
    Primary,
}

/// Breite eines kompakten Leisten-Buttons (nur Symbol).
pub const COMPACT_WIDTH: f32 = 40.0;

fn compact_id() -> egui::Id {
    egui::Id::new("kadra_bar_compact")
}

/// Leisten-Buttons ab jetzt nur als Symbol (Beschriftung als Tooltip) zeichnen.
pub fn set_compact(ctx: &egui::Context, compact: bool) {
    ctx.data_mut(|d| d.insert_temp(compact_id(), compact));
}

fn is_compact(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp(compact_id())).unwrap_or(false)
}

fn label_text(label: &str, menu: bool) -> String {
    if menu { format!("{label} ▾") } else { label.to_string() }
}

/// Breite, die `bar_button` belegen wird.
pub fn bar_button_width(ui: &Ui, label: &str, menu: bool, compact: bool) -> f32 {
    if compact {
        return COMPACT_WIDTH;
    }
    let label_w = ui.fonts_mut(|f| {
        f.layout_no_wrap(label_text(label, menu), FontId::proportional(11.5), Color32::WHITE).size().x
    });
    TOOL_WIDTH.max(label_w + 14.0)
}

/// Button mit Symbol oben und Beschriftung darunter (kompakt: nur Symbol).
pub fn bar_button(ui: &mut Ui, icon: &str, label: &str, look: Look, enabled: bool, menu: bool) -> Response {
    let compact = is_compact(ui.ctx());
    let size = Vec2::new(bar_button_width(ui, label, menu, compact), 52.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, mut resp) = ui.allocate_exact_size(size, sense);
    if compact {
        resp = resp.on_hover_text(label_text(label, false));
    }
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let accent = accent_fg(ui);
    let v = ui.visuals();
    let hovered = enabled && resp.hovered();
    let (bg, fg, icon_col) = match look {
        Look::Primary => {
            let bg = if !enabled {
                v.widgets.inactive.bg_fill
            } else if hovered {
                ORANGE_HOVER
            } else {
                ORANGE
            };
            (Some(bg), Color32::WHITE, Color32::WHITE)
        }
        Look::Tool { selected } => {
            let fg = if !enabled { v.weak_text_color() } else { v.text_color() };
            let bg = if selected {
                Some(v.widgets.active.weak_bg_fill)
            } else if hovered {
                Some(v.widgets.hovered.weak_bg_fill)
            } else {
                None
            };
            (bg, fg, if selected && enabled { accent } else { fg })
        }
    };
    let p = ui.painter();
    if let Some(bg) = bg {
        p.rect_filled(rect, CornerRadius::same(7), bg);
    }
    if let Look::Tool { selected: true } = look {
        p.rect_stroke(rect, CornerRadius::same(7), Stroke::new(1.0, accent.gamma_multiply(0.6)), StrokeKind::Inside);
    }
    if compact {
        paint_icon(p, rect.center(), icon, icon_col);
        if menu {
            p.text(rect.center_bottom() - Vec2::new(0.0, 8.0), Align2::CENTER_CENTER, "▾", FontId::proportional(10.0), fg);
        }
    } else {
        paint_icon(p, rect.center_top() + Vec2::new(0.0, 19.0), icon, icon_col);
        p.text(rect.center_bottom() - Vec2::new(0.0, 10.0), Align2::CENTER_CENTER, label_text(label, menu), FontId::proportional(11.5), fg);
    }
    resp
}

/// Symbole, die in der Leiste als Vektorgrafik gezeichnet werden: Die passenden Unicode-Zeichen
/// fehlen in den Schriften (Ersatzkästchen) oder sehen selbst wie eines aus. In Menütexten
/// stehen diese Zeichen, die überall vorhanden sind.
pub const ICON_RECT: &str = "▭";
pub const ICON_STEP: &str = "🔢";
pub const ICON_MARKER: &str = "🖊";

/// Leisten-Symbol (ca. 21 pt) zentriert an `c`.
fn paint_icon(p: &egui::Painter, c: egui::Pos2, icon: &str, col: Color32) {
    use egui::{pos2, vec2, Rect};
    match icon {
        ICON_RECT => {
            let r = Rect::from_center_size(c, vec2(20.0, 14.0));
            p.rect_stroke(r, CornerRadius::same(2), Stroke::new(2.0, col), StrokeKind::Middle);
        }
        ICON_STEP => {
            p.circle_filled(c, 10.0, col);
            // Ziffer in Hintergrundfarbe: hell auf dunklem Kreis, dunkel auf hellem.
            let lum = col.r() as u32 * 3 + col.g() as u32 * 6 + col.b() as u32;
            let digit = if lum > 1500 { Color32::from_gray(30) } else { Color32::WHITE };
            p.text(c + vec2(0.0, 0.5), Align2::CENTER_CENTER, "1", FontId::proportional(13.0), digit);
        }
        ICON_MARKER => {
            // Breiter, halbtransparenter Strich mit schräger Markerspitze darüber.
            p.line_segment([pos2(c.x - 10.0, c.y + 6.0), pos2(c.x + 10.0, c.y + 6.0)], Stroke::new(5.0, col.gamma_multiply(0.45)));
            let tip = [pos2(c.x + 1.0, c.y + 3.0), pos2(c.x - 3.0, c.y - 1.0), pos2(c.x + 5.0, c.y - 9.0), pos2(c.x + 9.0, c.y - 5.0)];
            p.add(egui::Shape::convex_polygon(tip.to_vec(), Color32::TRANSPARENT, Stroke::new(1.8, col)));
            p.line_segment([pos2(c.x + 1.0, c.y + 3.0), pos2(c.x - 3.0, c.y + 4.0)], Stroke::new(1.8, col));
            p.line_segment([pos2(c.x - 3.0, c.y + 4.0), pos2(c.x - 3.0, c.y - 1.0)], Stroke::new(1.8, col));
        }
        _ => {
            p.text(c, Align2::CENTER_CENTER, icon, FontId::proportional(21.0), col);
        }
    }
}

/// Großzügigerer, macOS-naher Grundstil: größere Schrift, mehr Luft,
/// Schaltflächen mindestens 30 pt hoch, rundere Ecken.
pub fn apply_style(ctx: &egui::Context) {
    use egui::{FontFamily, Margin, TextStyle};
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(19.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
        let sp = &mut s.spacing;
        sp.item_spacing = Vec2::new(10.0, 10.0);
        sp.button_padding = Vec2::new(14.0, 6.0);
        sp.interact_size = Vec2::new(40.0, 30.0);
        sp.window_margin = Margin::same(20);
        sp.menu_margin = Margin::same(8);
        sp.slider_width = 200.0;
        sp.combo_width = 150.0;
        sp.icon_width = 18.0;
        sp.icon_width_inner = 10.0;
        sp.icon_spacing = 8.0;
        sp.indent = 20.0;
        let v = &mut s.visuals;
        v.window_corner_radius = CornerRadius::same(12);
        v.menu_corner_radius = CornerRadius::same(10);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = CornerRadius::same(7);
        }
    });
}

/// Mindestgröße für Dialog-Schaltflächen (wie macOS-Standardbuttons).
pub const DIALOG_BUTTON: Vec2 = Vec2::new(100.0, 32.0);

/// Hauptbutton in Signaturfarbe mit weißer Schrift.
pub fn primary_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    let btn = egui::Button::new(egui::RichText::new(label).color(Color32::WHITE)).fill(ORANGE).min_size(DIALOG_BUTTON);
    ui.add_enabled(enabled, btn)
}

/// Dialogabschluss wie unter macOS: rechtsbündig, Hauptaktion ganz rechts.
/// Gibt (Hauptaktion, Abbrechen) zurück; Enter/Esc lösen ebenfalls aus.
pub fn dialog_buttons(ui: &mut Ui, primary: &str, enabled: bool) -> (bool, bool) {
    let mut out = (false, false);
    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let p = primary_button(ui, primary, enabled);
        let c = ui.add(egui::Button::new("Abbrechen").min_size(DIALOG_BUTTON));
        let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
        out = (p.clicked() || (enabled && enter), c.clicked() || esc);
    });
    out
}

/// Runder Schließen-Knopf (28 pt Klickfläche, Hover-Kreis wie unter macOS).
pub fn close_button(ui: &mut Ui) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    let v = ui.visuals();
    let c = rect.center();
    if resp.hovered() {
        let bg = if resp.is_pointer_button_down_on() { v.widgets.active.weak_bg_fill } else { v.widgets.hovered.weak_bg_fill };
        ui.painter().circle_filled(c, 14.0, bg);
    }
    let col = if resp.hovered() { v.strong_text_color() } else { v.text_color() };
    let (a, st) = (5.0, Stroke::new(1.8, col));
    ui.painter().line_segment([c + Vec2::new(-a, -a), c + Vec2::new(a, a)], st);
    ui.painter().line_segment([c + Vec2::new(-a, a), c + Vec2::new(a, -a)], st);
    resp.on_hover_text("Schließen (Esc)")
}

/// Kopfzeile für Fenster ohne egui-Titelleiste: Titel links, Schließen-Knopf rechts.
/// Gibt `true` zurück, wenn geschlossen werden soll.
pub fn window_header(ui: &mut Ui, title: &str) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.heading(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            close = close_button(ui).clicked();
        });
    });
    ui.separator();
    close
}

/// Senkrechte Trennlinie in der Leiste.
pub fn bar_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(13.0, 40.0), Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color));
}

/// Tastenkürzel im macOS-Stil („⌘⇧S“) in der Schreibweise der Plattform –
/// unter Windows „Strg+Umschalt+S“.
pub fn keys(mac: &str) -> String {
    if cfg!(target_os = "macos") { mac.to_string() } else { pc_keys(mac) }
}

fn pc_keys(mac: &str) -> String {
    let mut out = String::new();
    for c in mac.chars() {
        match c {
            '⌘' | '⌃' => out += "Strg+",
            '⇧' => out += "Umschalt+",
            '⌥' => out += "Alt+",
            '⌫' => out += "Entf",
            c => out.push(c),
        }
    }
    out.replace("+ + ", " + ")
}

#[cfg(test)]
mod tests {
    #[test]
    fn pc_key_names() {
        assert_eq!(super::pc_keys("Speichern (⌘⇧S)"), "Speichern (Strg+Umschalt+S)");
        assert_eq!(super::pc_keys("⌫ Löschen · ⌘ + Scrollen Zoom"), "Entf Löschen · Strg + Scrollen Zoom");
        assert_eq!(super::pc_keys("⌃ + ⇧ + 1"), "Strg + Umschalt + 1");
    }
}
