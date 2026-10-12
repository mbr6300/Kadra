//! Gemeinsame UI-Bausteine: Signaturfarbe und Leisten-Buttons im SnagIt-Stil.

use egui::{Align2, Color32, CornerRadius, FontId, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

/// SnapIt-Signaturfarbe.
pub const ORANGE: Color32 = Color32::from_rgb(232, 98, 26);
pub const ORANGE_HOVER: Color32 = Color32::from_rgb(246, 118, 44);

pub const BAR_HEIGHT: f32 = 64.0;
pub const TOOL_WIDTH: f32 = 62.0;

#[derive(Clone, Copy)]
pub enum Look {
    /// Normales Werkzeug, ggf. ausgewählt.
    Tool { selected: bool },
    /// Gefüllter Hauptbutton in Signaturfarbe.
    Primary,
}

/// Button mit Symbol oben und Beschriftung darunter.
pub fn bar_button(ui: &mut Ui, icon: &str, label: &str, look: Look, enabled: bool, menu: bool) -> Response {
    let label_w = ui.fonts_mut(|f| f.layout_no_wrap(label.to_string(), FontId::proportional(11.5), Color32::WHITE).size().x);
    let size = Vec2::new(TOOL_WIDTH.max(label_w + 14.0), 52.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    if !ui.is_rect_visible(rect) {
        return resp;
    }
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
            (bg, fg, if selected && enabled { ORANGE } else { fg })
        }
    };
    let p = ui.painter();
    if let Some(bg) = bg {
        p.rect_filled(rect, CornerRadius::same(7), bg);
    }
    if let Look::Tool { selected: true } = look {
        p.rect_stroke(rect, CornerRadius::same(7), Stroke::new(1.0, ORANGE.gamma_multiply(0.6)), StrokeKind::Inside);
    }
    p.text(rect.center_top() + Vec2::new(0.0, 19.0), Align2::CENTER_CENTER, icon, FontId::proportional(21.0), icon_col);
    let text = if menu { format!("{label} ▾") } else { label.to_string() };
    p.text(rect.center_bottom() - Vec2::new(0.0, 10.0), Align2::CENTER_CENTER, text, FontId::proportional(11.5), fg);
    resp
}

/// Senkrechte Trennlinie in der Leiste.
pub fn bar_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(13.0, 40.0), Sense::hover());
    ui.painter().vline(rect.center().x, rect.y_range(), Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color));
}
