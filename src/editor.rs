//! Bildeditor mit Werkzeugen, Auswahl, Undo/Redo und Bildoperationen.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use egui::{
    Align2, Color32, CornerRadius, FontId, Id, Key, Painter, PointerButton, Pos2, Rect, RichText, Sense, Stroke, StrokeKind,
    TextureHandle, TextureOptions, Ui, Vec2,
};
use image::{imageops, RgbaImage};

use crate::doc::{dist_to_segment, p, pos, rect, Annotation, Document, Effects, Rgba, Shape, Style, P};
use crate::render;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tool {
    Select,
    Arrow,
    Line,
    Rect,
    Ellipse,
    Pen,
    Highlight,
    Text,
    Step,
    Blur,
    Pixelate,
    Redact,
    Spotlight,
    Fill,
    Crop,
}

impl Tool {
    pub const ALL: [Tool; 15] = [
        Tool::Select,
        Tool::Arrow,
        Tool::Line,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Pen,
        Tool::Highlight,
        Tool::Text,
        Tool::Step,
        Tool::Blur,
        Tool::Pixelate,
        Tool::Redact,
        Tool::Spotlight,
        Tool::Fill,
        Tool::Crop,
    ];

    pub fn icon(self) -> &'static str {
        match self {
            Tool::Select => "⬉",
            Tool::Arrow => "➡",
            Tool::Line => "╱",
            Tool::Rect => "⬜",
            Tool::Ellipse => "⭕",
            Tool::Pen => "✏",
            Tool::Highlight => "🖍",
            Tool::Text => "T",
            Tool::Step => "①",
            Tool::Blur => "💧",
            Tool::Pixelate => "▦",
            Tool::Redact => "⬛",
            Tool::Spotlight => "🔦",
            Tool::Fill => "🎨",
            Tool::Crop => "✂",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tool::Select => "Auswahl",
            Tool::Arrow => "Pfeil",
            Tool::Line => "Linie",
            Tool::Rect => "Rechteck",
            Tool::Ellipse => "Ellipse",
            Tool::Pen => "Stift",
            Tool::Highlight => "Textmarker",
            Tool::Text => "Text",
            Tool::Step => "Nummerierung",
            Tool::Blur => "Weichzeichnen",
            Tool::Pixelate => "Verpixeln",
            Tool::Redact => "Schwärzen",
            Tool::Spotlight => "Spotlight",
            Tool::Fill => "Füllen",
            Tool::Crop => "Zuschneiden",
        }
    }

    pub fn key(self) -> Key {
        match self {
            Tool::Select => Key::V,
            Tool::Arrow => Key::A,
            Tool::Line => Key::L,
            Tool::Rect => Key::R,
            Tool::Ellipse => Key::E,
            Tool::Pen => Key::P,
            Tool::Highlight => Key::H,
            Tool::Text => Key::T,
            Tool::Step => Key::N,
            Tool::Blur => Key::B,
            Tool::Pixelate => Key::X,
            Tool::Redact => Key::K,
            Tool::Spotlight => Key::S,
            Tool::Fill => Key::F,
            Tool::Crop => Key::C,
        }
    }

    fn of(shape: &Shape) -> Tool {
        match shape {
            Shape::Arrow { .. } => Tool::Arrow,
            Shape::Line { .. } => Tool::Line,
            Shape::Rect { .. } => Tool::Rect,
            Shape::Ellipse { .. } => Tool::Ellipse,
            Shape::Pen { .. } => Tool::Pen,
            Shape::Highlight { .. } => Tool::Highlight,
            Shape::Text { .. } => Tool::Text,
            Shape::Step { .. } => Tool::Step,
            Shape::Blur { .. } => Tool::Blur,
            Shape::Pixelate { .. } => Tool::Pixelate,
            Shape::Redact { .. } => Tool::Redact,
            Shape::Spotlight { .. } => Tool::Spotlight,
            Shape::Fill { .. } => Tool::Fill,
        }
    }

    fn default_style(self) -> Style {
        let base = Style::default();
        match self {
            Tool::Highlight => Style { stroke: [255, 225, 0, 255], shadow: false, ..base },
            Tool::Text => Style { shadow: false, ..base },
            Tool::Step => Style { font_size: 28.0, ..base },
            Tool::Blur => Style { width: 6.0, shadow: false, ..base },
            Tool::Pixelate => Style { width: 4.0, shadow: false, ..base },
            Tool::Redact => Style { stroke: [0, 0, 0, 255], shadow: false, ..base },
            Tool::Pen => Style { width: 4.0, ..base },
            // width = Toleranz in Prozent
            Tool::Fill => Style { stroke: [255, 225, 0, 255], width: 15.0, shadow: false, ..base },
            _ => base,
        }
    }
}

/// Werkzeuggruppe in der oberen Leiste; mehrere Werkzeuge teilen sich einen Button.
pub struct ToolGroup {
    pub name: &'static str,
    pub tools: &'static [Tool],
}

pub const MAIN_GROUPS: [ToolGroup; 9] = [
    ToolGroup { name: "Auswahl", tools: &[Tool::Select] },
    ToolGroup { name: "Pfeil", tools: &[Tool::Arrow] },
    ToolGroup { name: "Text", tools: &[Tool::Text] },
    ToolGroup { name: "Formen", tools: &[Tool::Rect, Tool::Ellipse, Tool::Line] },
    ToolGroup { name: "Nummer", tools: &[Tool::Step] },
    ToolGroup { name: "Marker", tools: &[Tool::Highlight] },
    ToolGroup { name: "Füllen", tools: &[Tool::Fill] },
    ToolGroup { name: "Unkenntlich", tools: &[Tool::Blur, Tool::Pixelate, Tool::Redact] },
    ToolGroup { name: "Zuschneiden", tools: &[Tool::Crop] },
];

/// Seltener genutzte Werkzeuge im „Mehr“-Menü.
pub const MORE_TOOLS: [Tool; 2] = [Tool::Pen, Tool::Spotlight];

fn group_of(t: Tool) -> Option<&'static ToolGroup> {
    MAIN_GROUPS.iter().find(|g| g.tools.contains(&t))
}

/// Werkzeugleiste (Mitte der oberen Leiste). Ohne offenes Bild ausgegraut.
/// Nur die ersten `visible` Gruppen bekommen einen Button, der Rest landet im „Mehr“-Menü.
pub fn tool_bar(ui: &mut Ui, mut editor: Option<&mut Editor>, visible: usize) {
    use crate::ui_kit::{bar_button, bar_separator, Look};
    let enabled = editor.is_some();
    let current = editor.as_ref().map(|e| e.tool);
    let visible = visible.clamp(1, MAIN_GROUPS.len());
    for g in &MAIN_GROUPS[..visible] {
        let shown = current
            .filter(|t| g.tools.contains(t))
            .or_else(|| editor.as_ref().and_then(|e| e.group_last.get(g.name).copied()))
            .unwrap_or(g.tools[0]);
        let selected = current.is_some_and(|t| g.tools.contains(&t));
        let tip = g.tools.iter().map(|t| format!("{} ({})", t.name(), t.key().name())).collect::<Vec<_>>().join(" · ");
        let resp = bar_button(ui, shown.icon(), g.name, Look::Tool { selected }, enabled, false).on_hover_text(tip);
        if resp.clicked() {
            if let Some(e) = editor.as_deref_mut() {
                e.set_tool(shown);
            }
        }
    }
    bar_separator(ui);
    // Mehr-Menü: übergelaufene Gruppen + seltene Werkzeuge
    let more: Vec<Tool> = MAIN_GROUPS[visible..].iter().flat_map(|g| g.tools.iter().copied()).chain(MORE_TOOLS).collect();
    let more_selected = current.is_some_and(|t| more.contains(&t));
    let icon = current.filter(|t| more.contains(t)).map(|t| t.icon()).unwrap_or("⋯");
    let resp = bar_button(ui, icon, "Mehr", Look::Tool { selected: more_selected }, enabled, true);
    egui::Popup::menu(&resp).show(|ui| {
        for t in more {
            let label = format!("{}  {}   ({})", t.icon(), t.name(), t.key().name());
            if ui.add_enabled(enabled, egui::Button::selectable(current == Some(t), label)).clicked() {
                if let Some(e) = editor.as_deref_mut() {
                    e.set_tool(t);
                }
                ui.close();
            }
        }
    });
}

/// Breite der Werkzeugleiste mit `visible` Gruppen (für Zentrierung und Platzberechnung).
pub fn tool_bar_width(ui: &Ui, visible: usize, compact: bool) -> f32 {
    use crate::ui_kit::bar_button_width;
    let gap = ui.spacing().item_spacing.x;
    let groups: f32 = MAIN_GROUPS[..visible.clamp(1, MAIN_GROUPS.len())]
        .iter()
        .map(|g| bar_button_width(ui, g.name, false, compact) + gap)
        .sum();
    groups + 13.0 + gap + bar_button_width(ui, "Mehr", true, compact)
}

pub fn tool_group_count() -> usize {
    MAIN_GROUPS.len()
}

#[derive(Clone)]
struct Snapshot {
    image: Arc<RgbaImage>,
    anns: Vec<Annotation>,
    effects: Effects,
}

enum Drag {
    Create { ann: Annotation, start: Pos2 },
    Move { idx: usize, last: Pos2 },
    Handle { idx: usize, handle: usize },
    /// Zuschneiderahmen: `grip` = Kante/Ecke (−1/0/1 je Achse), `None` = verschieben.
    Crop { grip: Option<(i8, i8)>, orig: Rect, start: Pos2 },
}

pub struct Editor {
    pub id: String,
    pub doc: Document,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    coalesce: Option<String>,
    pub tool: Tool,
    styles: HashMap<Tool, Style>,
    tex: Option<TextureHandle>,
    tex_key: Option<(usize, Vec<Annotation>)>,
    zoom: f32,
    pan: Vec2,
    fit: bool,
    drag: Option<Drag>,
    drag_snapshot: Option<Snapshot>,
    pub selected: Option<usize>,
    editing_text: Option<usize>,
    focus_text: bool,
    crop: Option<Rect>,
    /// Festes Seitenverhältnis (Breite/Höhe) für den Zuschneiderahmen.
    crop_aspect: Option<f32>,
    pub dirty: bool,
    pub image_changed: bool,
    pub last_change: Instant,
    resize_pct: f32,
    /// Zuletzt gewähltes Werkzeug je Gruppe (Gruppenname → Werkzeug).
    group_last: HashMap<&'static str, Tool>,
}

fn c32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

const PALETTE: [Rgba; 9] = [
    [230, 40, 40, 255],
    [255, 140, 0, 255],
    [255, 225, 0, 255],
    [40, 180, 70, 255],
    [30, 120, 230, 255],
    [140, 70, 200, 255],
    [255, 255, 255, 255],
    [128, 128, 128, 255],
    [0, 0, 0, 255],
];

impl Editor {
    pub fn new(id: String, doc: Document) -> Self {
        Self {
            id,
            doc,
            undo: vec![],
            redo: vec![],
            coalesce: None,
            tool: Tool::Arrow,
            styles: Tool::ALL.iter().map(|t| (*t, t.default_style())).collect(),
            tex: None,
            tex_key: None,
            zoom: 1.0,
            pan: Vec2::ZERO,
            fit: true,
            drag: None,
            drag_snapshot: None,
            selected: None,
            editing_text: None,
            focus_text: false,
            crop: None,
            crop_aspect: None,
            dirty: false,
            image_changed: false,
            last_change: Instant::now(),
            resize_pct: 50.0,
            group_last: HashMap::new(),
        }
    }

    pub fn is_busy(&self) -> bool {
        self.drag.is_some() || self.editing_text.is_some()
    }

    // ---------- Undo/Redo ----------

    fn snapshot(&self) -> Snapshot {
        Snapshot { image: self.doc.image.clone(), anns: self.doc.annotations.clone(), effects: self.doc.effects.clone() }
    }

    fn restore(&mut self, s: Snapshot) {
        if !Arc::ptr_eq(&s.image, &self.doc.image) {
            self.image_changed = true;
        }
        self.doc.image = s.image;
        self.doc.annotations = s.anns;
        self.doc.effects = s.effects;
        self.selected = None;
        self.editing_text = None;
        // Nach Undo/Redo beginnt die nächste Änderung immer einen eigenen Undo-Schritt.
        self.coalesce = None;
        if self.tool == Tool::Crop {
            self.crop = Some(self.full_rect());
            self.crop_aspect = None;
        }
        self.changed();
    }

    fn changed(&mut self) {
        self.dirty = true;
        self.last_change = Instant::now();
    }

    fn push_snapshot(&mut self, s: Snapshot) {
        self.undo.push(s);
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.coalesce = None;
        self.changed();
    }

    fn push_undo(&mut self) {
        let s = self.snapshot();
        self.push_snapshot(s);
    }

    /// Mehrere aufeinanderfolgende Änderungen (z. B. Farbregler) als ein Undo-Schritt.
    fn push_undo_coalesced(&mut self, key: String) {
        if self.coalesce.as_ref() != Some(&key) {
            self.push_undo();
            self.coalesce = Some(key);
        }
        self.changed();
    }

    pub fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            let cur = self.snapshot();
            self.redo.push(cur);
            self.restore(s);
        }
    }

    pub fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            let cur = self.snapshot();
            self.undo.push(cur);
            self.restore(s);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn duplicate_selected(&mut self) {
        if let Some(i) = self.selected.filter(|i| *i < self.doc.annotations.len()) {
            self.push_undo();
            let mut a = self.doc.annotations[i].clone();
            a.shape.translate(Vec2::splat(20.0));
            self.doc.annotations.push(a);
            self.selected = Some(self.doc.annotations.len() - 1);
        }
    }

    // ---------- Ansicht ----------

    /// Bild ins Fenster einpassen.
    pub fn zoom_fit(&mut self) {
        self.fit = true;
    }

    /// 1 Bildpixel = 1 Bildschirmpixel.
    pub fn zoom_actual(&mut self, pixels_per_point: f32) {
        self.fit = false;
        self.zoom = 1.0 / pixels_per_point;
        self.pan = Vec2::ZERO;
    }

    /// Um die Fenstermitte zoomen.
    pub fn zoom_by(&mut self, factor: f32) {
        let new = (self.zoom * factor).clamp(0.02, 32.0);
        self.pan *= new / self.zoom;
        self.zoom = new;
        self.fit = false;
    }

    pub fn delete_selected(&mut self) {
        if let Some(i) = self.selected.take() {
            if i < self.doc.annotations.len() {
                self.push_undo();
                self.doc.annotations.remove(i);
            }
        }
    }

    pub fn set_tool(&mut self, t: Tool) {
        self.finish_text();
        self.tool = t;
        if let Some(g) = group_of(t) {
            self.group_last.insert(g.name, t);
        }
        // Zuschneiden startet mit einem Rahmen um das ganze Bild.
        self.crop = (t == Tool::Crop).then(|| self.full_rect());
        self.crop_aspect = None;
        if t != Tool::Select {
            self.selected = None;
        }
    }

    fn full_rect(&self) -> Rect {
        Rect::from_min_size(Pos2::ZERO, self.doc.size())
    }

    pub fn cancel_crop(&mut self) {
        self.crop = None;
        self.crop_aspect = None;
        self.tool = Tool::Select;
    }

    /// Rahmen auf festes Seitenverhältnis setzen (größtmöglich, zentriert); `None` = frei.
    fn set_crop_aspect(&mut self, aspect: Option<f32>) {
        self.crop_aspect = aspect;
        let Some(a) = aspect else { return };
        let s = self.doc.size();
        let size = if s.x / s.y > a { Vec2::new(s.y * a, s.y) } else { Vec2::new(s.x, s.x / a) };
        let r = Rect::from_center_size(self.full_rect().center(), size);
        self.crop = Some(Rect::from_min_max(r.min.round(), r.max.round()));
    }

    fn crop_changed(&self) -> bool {
        self.crop.is_some_and(|c| c != self.full_rect())
    }

    // ---------- Bildoperationen ----------

    fn set_image(&mut self, img: RgbaImage) {
        self.doc.image = Arc::new(img);
        self.image_changed = true;
        self.fit = true;
        self.changed();
    }

    pub fn apply_crop(&mut self) {
        let unchanged = !self.crop_changed();
        let Some(r) = self.crop.take() else { return };
        self.crop_aspect = None;
        self.tool = Tool::Select;
        let r = r.intersect(self.full_rect());
        let (x, y) = (r.min.x.round() as u32, r.min.y.round() as u32);
        let (w, h) = (r.width().round() as u32, r.height().round() as u32);
        if unchanged || w < 2 || h < 2 {
            return;
        }
        self.push_undo();
        let img = imageops::crop_imm(&*self.doc.image, x, y, w, h).to_image();
        for a in &mut self.doc.annotations {
            a.shape.translate(Vec2::new(-(x as f32), -(y as f32)));
        }
        self.set_image(img);
    }

    pub fn rotate(&mut self, clockwise: bool) {
        self.push_undo();
        let (w, h) = (self.doc.image.width() as f32, self.doc.image.height() as f32);
        let img = if clockwise { imageops::rotate90(&*self.doc.image) } else { imageops::rotate270(&*self.doc.image) };
        for a in &mut self.doc.annotations {
            if clockwise {
                a.shape.map_points(|q| [h - q[1], q[0]]);
            } else {
                a.shape.map_points(|q| [q[1], w - q[0]]);
            }
        }
        self.set_image(img);
    }

    pub fn flip(&mut self, horizontal: bool) {
        self.push_undo();
        let (w, h) = (self.doc.image.width() as f32, self.doc.image.height() as f32);
        let img = if horizontal { imageops::flip_horizontal(&*self.doc.image) } else { imageops::flip_vertical(&*self.doc.image) };
        for a in &mut self.doc.annotations {
            if horizontal {
                a.shape.map_points(|q| [w - q[0], q[1]]);
            } else {
                a.shape.map_points(|q| [q[0], h - q[1]]);
            }
        }
        self.set_image(img);
    }

    pub fn resize(&mut self, factor: f32) {
        let (w, h) = (self.doc.image.width() as f32 * factor, self.doc.image.height() as f32 * factor);
        if w < 4.0 || h < 4.0 || w > 20000.0 || h > 20000.0 {
            return;
        }
        self.push_undo();
        let img = imageops::resize(&*self.doc.image, w as u32, h as u32, imageops::FilterType::Lanczos3);
        for a in &mut self.doc.annotations {
            a.shape.map_points(|q| [q[0] * factor, q[1] * factor]);
            // Beim Füllen ist `width` die Farbtoleranz in Prozent, keine Länge.
            if !matches!(a.shape, Shape::Fill { .. }) {
                a.style.width *= factor;
            }
            a.style.font_size *= factor;
        }
        self.set_image(img);
    }

    // ---------- Texturen ----------

    fn refresh_texture(&mut self, ctx: &egui::Context) {
        let effects: Vec<Annotation> = self.doc.annotations.iter().filter(|a| a.shape.is_effect()).cloned().collect();
        let key = (Arc::as_ptr(&self.doc.image) as usize, effects);
        if self.tex.is_some() && self.tex_key.as_ref() == Some(&key) {
            return;
        }
        let owned;
        let mut img: &RgbaImage = &self.doc.image;
        if !key.1.is_empty() {
            owned = render::apply_effects(img, &key.1);
            img = &owned;
        }
        let max_side = ctx.input(|i| i.max_texture_side) as u32;
        let scaled;
        if img.width() > max_side || img.height() > max_side {
            let s = max_side as f32 / img.width().max(img.height()) as f32;
            scaled = imageops::resize(img, (img.width() as f32 * s) as u32, (img.height() as f32 * s) as u32, imageops::FilterType::Triangle);
            img = &scaled;
        }
        let color = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        let opts = TextureOptions {
            magnification: egui::TextureFilter::Nearest,
            minification: egui::TextureFilter::Linear,
            mipmap_mode: Some(egui::TextureFilter::Linear),
            ..Default::default()
        };
        match &mut self.tex {
            Some(t) => t.set(color, opts),
            None => self.tex = Some(ctx.load_texture("doc", color, opts)),
        }
        self.tex_key = Some(key);
    }

    // ---------- Panels ----------

    /// Werkzeug, Stile und Gruppenwahl aus dem vorherigen Editor übernehmen.
    pub fn inherit(&mut self, prev: &Editor) {
        self.tool = if prev.tool == Tool::Crop { Tool::Select } else { prev.tool };
        self.styles = prev.styles.clone();
        self.group_last = prev.group_last.clone();
    }

    pub fn properties_ui(&mut self, ui: &mut Ui) {
        let sel = self.selected.filter(|i| *i < self.doc.annotations.len());
        let kind = sel.map(|i| Tool::of(&self.doc.annotations[i].shape)).unwrap_or(self.tool);
        ui.heading(if sel.is_some() { format!("{} (Auswahl)", kind.name()) } else { kind.name().to_string() });
        // Varianten einer Werkzeuggruppe (z. B. Rechteck/Ellipse/Linie)
        if sel.is_none() {
            if let Some(g) = group_of(self.tool).filter(|g| g.tools.len() > 1) {
                ui.horizontal(|ui| {
                    for t in g.tools {
                        if ui.selectable_label(self.tool == *t, format!("{} {}", t.icon(), t.name())).clicked() {
                            self.set_tool(*t);
                        }
                    }
                });
            }
        }
        ui.separator();

        if kind == Tool::Crop {
            ui.label("Rahmen an Ecken und Kanten verkleinern oder im Inneren verschieben. ⇧ an einer Ecke hält das Seitenverhältnis.");
            if let Some(r) = self.crop {
                ui.label(RichText::new(format!("{} × {} px", r.width().round(), r.height().round())).strong());
            }
            ui.add_space(4.0);
            ui.label("Format");
            ui.horizontal_wrapped(|ui| {
                let s = self.doc.size();
                let presets = [("Frei", None), ("Original", Some(s.x / s.y)), ("1:1", Some(1.0)), ("4:3", Some(4.0 / 3.0)), ("16:9", Some(16.0 / 9.0))];
                for (label, aspect) in presets {
                    let active = match (self.crop_aspect, aspect) {
                        (None, None) => true,
                        (Some(a), Some(b)) => (a - b).abs() < 0.001,
                        _ => false,
                    };
                    if ui.selectable_label(active, label).clicked() {
                        self.set_crop_aspect(aspect);
                    }
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if crate::ui_kit::primary_button(ui, "✔ Zuschneiden", self.crop_changed()).clicked() {
                    self.apply_crop();
                }
                if ui.add(egui::Button::new("Abbrechen").min_size(crate::ui_kit::DIALOG_BUTTON)).clicked() {
                    self.cancel_crop();
                }
            });
            if ui.add_enabled(self.crop_changed(), egui::Button::new("Rahmen zurücksetzen")).clicked() {
                self.crop = Some(self.full_rect());
                self.crop_aspect = None;
            }
        } else if kind != Tool::Select {
            let original = sel.map(|i| self.doc.annotations[i].style.clone()).unwrap_or_else(|| self.styles[&kind].clone());
            let mut st = original.clone();
            style_ui(ui, kind, &mut st);
            if st != original {
                if let Some(i) = sel {
                    self.push_undo_coalesced(format!("style{i}"));
                    self.doc.annotations[i].style = st.clone();
                }
                self.styles.insert(kind, st);
            }
            if let Some(i) = sel {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("⬆ Nach vorne").clicked() && i + 1 < self.doc.annotations.len() {
                        self.push_undo();
                        let a = self.doc.annotations.remove(i);
                        self.doc.annotations.push(a);
                        self.selected = Some(self.doc.annotations.len() - 1);
                    }
                    if ui.button("🗑 Löschen").clicked() {
                        self.delete_selected();
                    }
                });
            }
        } else {
            ui.label("Objekt anklicken zum Auswählen, ziehen zum Verschieben, Griffe zum Ändern der Größe.");
        }

        ui.add_space(16.0);
        ui.heading("Bild");
        ui.separator();
        ui.label(format!("{} × {} px", self.doc.image.width(), self.doc.image.height()));
        ui.horizontal(|ui| {
            if ui.button("⟲").on_hover_text("Links drehen").clicked() {
                self.rotate(false);
            }
            if ui.button("⟳").on_hover_text("Rechts drehen").clicked() {
                self.rotate(true);
            }
            if ui.button("⇔").on_hover_text("Horizontal spiegeln").clicked() {
                self.flip(true);
            }
            if ui.button("⇕").on_hover_text("Vertikal spiegeln").clicked() {
                self.flip(false);
            }
        });
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.resize_pct).range(5.0..=400.0).suffix(" %"));
            if ui.button("Größe ändern").clicked() {
                self.resize(self.resize_pct / 100.0);
            }
        });

        ui.add_space(16.0);
        ui.heading("Effekte");
        ui.separator();
        let mut fx = self.doc.effects.clone();
        let mut has_border = fx.border.is_some();
        ui.checkbox(&mut has_border, "Rahmen");
        if has_border {
            let (mut w, mut c) = fx.border.unwrap_or((6.0, [40, 40, 40, 255]));
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut w, 1.0..=60.0).suffix(" px"));
                ui.color_edit_button_srgba_unmultiplied(&mut c);
            });
            fx.border = Some((w, c));
        } else {
            fx.border = None;
        }
        ui.horizontal(|ui| {
            ui.label("Runde Ecken");
            ui.add(egui::Slider::new(&mut fx.round_corners, 0.0..=80.0).suffix(" px"));
        });
        ui.checkbox(&mut fx.drop_shadow, "Schlagschatten");
        if fx != self.doc.effects {
            self.push_undo_coalesced("effects".into());
            self.doc.effects = fx;
        }

        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.label("Zoom");
            if ui.button("Einpassen").clicked() {
                self.fit = true;
            }
            if ui.button("100 %").clicked() {
                self.fit = false;
                self.zoom = 1.0 / ui.ctx().pixels_per_point();
                self.pan = Vec2::ZERO;
            }
        });
        ui.label(format!("{:.0} %", self.zoom * ui.ctx().pixels_per_point() * 100.0));
    }

    // ---------- Tastatur ----------

    pub fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            if self.editing_text.is_some() && ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.finish_text();
            }
            return;
        }
        let (cmd, shift) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
        if cmd {
            if ctx.input(|i| i.key_pressed(Key::Z)) {
                if shift {
                    self.redo()
                } else {
                    self.undo()
                }
            }
            if ctx.input(|i| i.key_pressed(Key::Y)) {
                self.redo();
            }
            if ctx.input(|i| i.key_pressed(Key::D)) {
                self.duplicate_selected();
            }
            return;
        }
        if ctx.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)) {
            self.delete_selected();
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.selected = None;
            if self.tool == Tool::Crop {
                self.cancel_crop();
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Enter)) && self.crop.is_some() {
            self.apply_crop();
        }
        // Pfeiltasten verschieben die Auswahl
        if let Some(i) = self.selected {
            let step = if shift { 10.0 } else { 1.0 };
            let d = ctx.input(|inp| {
                let mut d = Vec2::ZERO;
                if inp.key_pressed(Key::ArrowLeft) {
                    d.x -= step
                }
                if inp.key_pressed(Key::ArrowRight) {
                    d.x += step
                }
                if inp.key_pressed(Key::ArrowUp) {
                    d.y -= step
                }
                if inp.key_pressed(Key::ArrowDown) {
                    d.y += step
                }
                d
            });
            if d != Vec2::ZERO {
                self.push_undo_coalesced(format!("nudge{i}"));
                self.doc.annotations[i].shape.translate(d);
            }
        }
        for t in Tool::ALL {
            if ctx.input(|i| i.key_pressed(t.key())) {
                self.set_tool(t);
            }
        }
    }

    // ---------- Text ----------

    fn finish_text(&mut self) {
        if let Some(i) = self.editing_text.take() {
            if let Some(Shape::Text { text, .. }) = self.doc.annotations.get(i).map(|a| &a.shape) {
                if text.trim().is_empty() {
                    self.doc.annotations.remove(i);
                    self.selected = None;
                }
            }
            self.changed();
        }
    }

    fn start_text(&mut self, at: Pos2) {
        self.push_undo();
        self.doc.annotations.push(Annotation {
            shape: Shape::Text { pos: p(at), text: String::new() },
            style: self.styles[&Tool::Text].clone(),
        });
        let i = self.doc.annotations.len() - 1;
        self.editing_text = Some(i);
        self.selected = Some(i);
        self.focus_text = true;
    }

    // ---------- Canvas ----------

    fn hit(&self, q: Pos2, tol: f32) -> Option<usize> {
        self.doc.annotations.iter().enumerate().rev().find(|(_, a)| hit_test(a, q, tol)).map(|(i, _)| i)
    }

    pub fn canvas_ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.refresh_texture(&ctx);
        let avail = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(avail, Sense::click_and_drag());
        let ppp = ctx.pixels_per_point();
        let size = self.doc.size();

        if self.fit {
            let margin = 40.0;
            self.zoom = ((avail.width() - margin) / size.x).min((avail.height() - margin) / size.y).min(1.0 / ppp).max(0.01);
            self.pan = Vec2::ZERO;
        }

        // Zoom (Cmd/Ctrl + Scroll, Pinch) und Verschieben (Scroll)
        if resp.hovered() {
            let (zd, scroll, hover) = ctx.input(|i| (i.zoom_delta(), i.smooth_scroll_delta, i.pointer.hover_pos()));
            if zd != 1.0 {
                if let Some(c) = hover {
                    let img_min = avail.center() + self.pan - size * self.zoom / 2.0;
                    let before = (c - img_min) / self.zoom;
                    self.zoom = (self.zoom * zd).clamp(0.02, 32.0);
                    self.pan = c - before * self.zoom - (avail.center() - size * self.zoom / 2.0);
                    self.fit = false;
                }
            } else if scroll != Vec2::ZERO {
                self.pan += scroll;
                self.fit = false;
            }
        }
        if resp.dragged_by(PointerButton::Middle) || resp.dragged_by(PointerButton::Secondary) {
            self.pan += resp.drag_delta();
            self.fit = false;
        }

        let zoom = self.zoom;
        let img_rect = Rect::from_center_size(avail.center() + self.pan, size * zoom);
        let to_img = move |s: Pos2| ((s - img_rect.min) / zoom).to_pos2();
        let to_scr = move |q: Pos2| img_rect.min + q.to_vec2() * zoom;

        // ----- Interaktion -----
        let shift = ctx.input(|i| i.modifiers.shift);
        let pointer = resp.interact_pointer_pos().or(resp.hover_pos());
        let tol = 6.0 / zoom;

        if resp.drag_started_by(PointerButton::Primary) {
            self.finish_text();
            let start = to_img(ctx.input(|i| i.pointer.press_origin()).unwrap_or_default());
            self.drag_snapshot = Some(self.snapshot());
            self.drag = match self.tool {
                Tool::Select => {
                    let handle = self.selected.and_then(|i| {
                        self.doc.annotations.get(i)?.shape.handles().iter().position(|h| (to_scr(*h) - to_scr(start)).length() < 9.0)
                    });
                    if let (Some(idx), Some(handle)) = (self.selected, handle) {
                        Some(Drag::Handle { idx, handle })
                    } else if let Some(idx) = self.hit(start, tol) {
                        self.selected = Some(idx);
                        Some(Drag::Move { idx, last: start })
                    } else {
                        self.selected = None;
                        None
                    }
                }
                Tool::Crop => self.crop.and_then(|c| {
                    let cr = Rect::from_min_max(to_scr(c.min), to_scr(c.max));
                    crop_grip(cr, to_scr(start), self.crop_aspect.is_some())
                        .map(|grip| Drag::Crop { grip, orig: c, start })
                }),
                Tool::Text | Tool::Step | Tool::Fill => None,
                t => {
                    let shape = new_shape(t, p(start));
                    Some(Drag::Create { ann: Annotation { shape, style: self.styles[&t].clone() }, start })
                }
            };
        }

        if let (Some(drag), Some(ptr)) = (&mut self.drag, pointer) {
            if resp.dragged_by(PointerButton::Primary) {
                let cur = to_img(ptr);
                match drag {
                    Drag::Create { ann, start } => update_shape(&mut ann.shape, *start, cur, shift, zoom),
                    Drag::Move { idx, last } => {
                        if let Some(a) = self.doc.annotations.get_mut(*idx) {
                            a.shape.translate(cur - *last);
                        }
                        *last = cur;
                    }
                    Drag::Handle { idx, handle } => {
                        if let Some(a) = self.doc.annotations.get_mut(*idx) {
                            a.shape.set_handle(*handle, cur);
                        }
                    }
                    Drag::Crop { grip, orig, start } => {
                        let bounds = Rect::from_min_size(Pos2::ZERO, size);
                        let aspect = self.crop_aspect.or(shift.then(|| orig.width() / orig.height()));
                        self.crop = Some(drag_crop(*orig, *grip, cur - *start, bounds, aspect));
                    }
                }
            }
        }

        if resp.drag_stopped_by(PointerButton::Primary) {
            if let Some(Drag::Create { ann, .. }) = self.drag.take() {
                let b = bounds(&ann);
                if b.width().max(b.height()) * zoom > 4.0 {
                    let snap = self.drag_snapshot.take().unwrap_or_else(|| self.snapshot());
                    self.push_snapshot(snap);
                    self.doc.annotations.push(ann);
                    self.selected = Some(self.doc.annotations.len() - 1);
                }
            } else if let Some(snap) = self.drag_snapshot.take() {
                if snap.anns != self.doc.annotations {
                    self.push_snapshot(snap);
                }
            }
            self.drag = None;
            self.drag_snapshot = None;
        }

        if resp.clicked_by(PointerButton::Primary) {
            if let Some(ptr) = pointer {
                let q = to_img(ptr);
                let was_editing = self.editing_text.is_some();
                self.finish_text();
                match self.tool {
                    Tool::Text if !was_editing => {
                        let hit = self.hit(q, tol).filter(|i| matches!(self.doc.annotations[*i].shape, Shape::Text { .. }));
                        if let Some(i) = hit {
                            self.push_undo();
                            self.editing_text = Some(i);
                            self.selected = Some(i);
                            self.focus_text = true;
                        } else {
                            self.start_text(q);
                        }
                    }
                    Tool::Step => {
                        self.push_undo();
                        let n = self.doc.next_step();
                        self.doc.annotations.push(Annotation {
                            shape: Shape::Step { pos: p(q), n },
                            style: self.styles[&Tool::Step].clone(),
                        });
                        self.selected = Some(self.doc.annotations.len() - 1);
                    }
                    Tool::Fill => {
                        if Rect::from_min_size(Pos2::ZERO, size).contains(q) {
                            self.push_undo();
                            self.doc.annotations.push(Annotation {
                                shape: Shape::Fill { pos: p(q) },
                                style: self.styles[&Tool::Fill].clone(),
                            });
                            self.selected = Some(self.doc.annotations.len() - 1);
                        }
                    }
                    Tool::Select => self.selected = self.hit(q, tol),
                    _ => {}
                }
            }
        }
        if resp.double_clicked() && self.tool == Tool::Select {
            if let Some(i) = self.selected {
                if matches!(self.doc.annotations[i].shape, Shape::Text { .. }) {
                    self.push_undo();
                    self.editing_text = Some(i);
                    self.focus_text = true;
                }
            }
        }

        // Cursor
        if resp.hovered() {
            let icon = match self.tool {
                Tool::Select => {
                    if pointer.map(to_img).and_then(|q| self.hit(q, tol)).is_some() {
                        egui::CursorIcon::Move
                    } else {
                        egui::CursorIcon::Default
                    }
                }
                Tool::Text => egui::CursorIcon::Text,
                Tool::Crop => {
                    let grip = match &self.drag {
                        Some(Drag::Crop { grip, .. }) => Some(*grip),
                        _ => self.crop.zip(pointer).and_then(|(c, ptr)| {
                            crop_grip(Rect::from_min_max(to_scr(c.min), to_scr(c.max)), ptr, self.crop_aspect.is_some())
                        }),
                    };
                    match grip {
                        Some(None) => egui::CursorIcon::Move,
                        Some(Some((x, y))) if x == y => egui::CursorIcon::ResizeNwSe,
                        Some(Some((x, y))) if x == -y => egui::CursorIcon::ResizeNeSw,
                        Some(Some((_, 0))) => egui::CursorIcon::ResizeHorizontal,
                        Some(Some(_)) => egui::CursorIcon::ResizeVertical,
                        None => egui::CursorIcon::Default,
                    }
                }
                _ => egui::CursorIcon::Crosshair,
            };
            ctx.set_cursor_icon(icon);
        }

        // ----- Zeichnen -----
        let painter = ui.painter_at(avail);
        painter.rect_filled(avail, 0.0, ui.visuals().extreme_bg_color);
        let fx = &self.doc.effects;
        let radius = fx.round_corners * zoom;
        let mut outer = img_rect;
        if let Some((w, _)) = fx.border {
            outer = img_rect.expand(w * zoom);
        }
        if fx.drop_shadow {
            let shadow = egui::epaint::Shadow { offset: [8, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(140) };
            painter.add(shadow.as_shape(outer, CornerRadius::same(radius.min(255.0) as u8)));
        }
        if let Some((_, c)) = fx.border {
            painter.rect_filled(outer, CornerRadius::same(radius.min(255.0) as u8), c32(c));
        }
        if let Some(tex) = &self.tex {
            let inner_r = if fx.border.is_some() { 0.0 } else { radius };
            egui::Image::new(egui::load::SizedTexture::new(tex.id(), img_rect.size()))
                .corner_radius(CornerRadius::same(inner_r.min(255.0) as u8))
                .paint_at(ui, img_rect);
        }

        let clip = painter.with_clip_rect(avail.intersect(img_rect.expand(2000.0)));
        for a in render::draw_order(&self.doc.annotations) {
            if render::casts_shadow(a) {
                paint_annotation(&clip, a, img_rect, zoom, true);
            }
            paint_annotation(&clip, a, img_rect, zoom, false);
        }
        // Effekt-Bereiche im Auswahlmodus andeuten
        if self.tool == Tool::Select {
            for a in self.doc.annotations.iter().filter(|a| a.shape.is_effect()) {
                if let Shape::Fill { pos: fp } = &a.shape {
                    // Ansatzpunkt der Füllung als Markierung
                    let c = to_scr(pos(*fp));
                    clip.circle_filled(c, 5.0, c32(a.style.stroke));
                    clip.circle_stroke(c, 6.0, Stroke::new(1.5, Color32::WHITE));
                    clip.circle_stroke(c, 7.5, Stroke::new(1.0, Color32::from_black_alpha(160)));
                    continue;
                }
                let r = bounds(a);
                let r = Rect::from_min_max(to_scr(r.min), to_scr(r.max));
                clip.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_white_alpha(60)), StrokeKind::Middle);
            }
        }
        if let Some(Drag::Create { ann, .. }) = &self.drag {
            if render::casts_shadow(ann) {
                paint_annotation(&clip, ann, img_rect, zoom, true);
            }
            paint_annotation(&clip, ann, img_rect, zoom, false);
            if ann.shape.is_effect() {
                let r = bounds(ann);
                clip.rect_stroke(
                    Rect::from_min_max(to_scr(r.min), to_scr(r.max)),
                    0.0,
                    Stroke::new(1.5, Color32::WHITE),
                    StrokeKind::Middle,
                );
            }
        }

        // Auswahl
        if let Some(a) = self.selected.and_then(|i| self.doc.annotations.get(i)) {
            let r = bounds(a);
            let r = Rect::from_min_max(to_scr(r.min), to_scr(r.max)).expand(4.0);
            let st = Stroke::new(1.0, Color32::from_rgb(0, 150, 255));
            let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
            clip.extend(egui::Shape::dashed_line(&pts, st, 5.0, 4.0));
            for h in a.shape.handles() {
                let hr = Rect::from_center_size(to_scr(h), Vec2::splat(9.0));
                clip.rect_filled(hr, 2.0, Color32::WHITE);
                clip.rect_stroke(hr, 2.0, Stroke::new(1.0, Color32::from_rgb(0, 150, 255)), StrokeKind::Middle);
            }
        }

        // Zuschneiden: Rahmen mit Griffen + Bestätigungsleiste
        if let Some(c) = self.crop {
            let cr = Rect::from_min_max(to_scr(c.min), to_scr(c.max));
            let dim = Color32::from_black_alpha(150);
            let o = img_rect;
            clip.rect_filled(Rect::from_min_max(o.min, Pos2::new(o.max.x, cr.min.y)), 0.0, dim);
            clip.rect_filled(Rect::from_min_max(Pos2::new(o.min.x, cr.max.y), o.max), 0.0, dim);
            clip.rect_filled(Rect::from_min_max(Pos2::new(o.min.x, cr.min.y), Pos2::new(cr.min.x, cr.max.y)), 0.0, dim);
            clip.rect_filled(Rect::from_min_max(Pos2::new(cr.max.x, cr.min.y), Pos2::new(o.max.x, cr.max.y)), 0.0, dim);
            // Drittel-Linien während der Bearbeitung
            if matches!(self.drag, Some(Drag::Crop { .. })) {
                let thin = Stroke::new(1.0, Color32::from_white_alpha(90));
                for f in [1.0 / 3.0, 2.0 / 3.0] {
                    let x = cr.min.x + cr.width() * f;
                    let y = cr.min.y + cr.height() * f;
                    clip.line_segment([Pos2::new(x, cr.min.y), Pos2::new(x, cr.max.y)], thin);
                    clip.line_segment([Pos2::new(cr.min.x, y), Pos2::new(cr.max.x, y)], thin);
                }
            }
            clip.rect_stroke(cr, 0.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Middle);
            // Eck-Griffe als L-Form, Kanten-Griffe als Balken
            let grip = Stroke::new(4.0, Color32::WHITE);
            let arm = 18.0f32.min(cr.width() / 3.0).min(cr.height() / 3.0);
            for (corner, dx, dy) in [
                (cr.left_top(), 1.0, 1.0),
                (cr.right_top(), -1.0, 1.0),
                (cr.left_bottom(), 1.0, -1.0),
                (cr.right_bottom(), -1.0, -1.0),
            ] {
                clip.line_segment([corner, corner + Vec2::new(dx * arm, 0.0)], grip);
                clip.line_segment([corner, corner + Vec2::new(0.0, dy * arm)], grip);
            }
            if self.crop_aspect.is_none() {
                let half = arm * 0.7;
                let (cx, cy) = (cr.center().x, cr.center().y);
                clip.line_segment([Pos2::new(cx - half, cr.min.y), Pos2::new(cx + half, cr.min.y)], grip);
                clip.line_segment([Pos2::new(cx - half, cr.max.y), Pos2::new(cx + half, cr.max.y)], grip);
                clip.line_segment([Pos2::new(cr.min.x, cy - half), Pos2::new(cr.min.x, cy + half)], grip);
                clip.line_segment([Pos2::new(cr.max.x, cy - half), Pos2::new(cr.max.x, cy + half)], grip);
            }

            // Leiste unter dem Rahmen (oder darin, wenn unten kein Platz ist)
            let below = cr.max.y + 56.0 < avail.max.y;
            let (anchor, pivot) = if below {
                (Pos2::new(cr.center().x, cr.max.y + 12.0), Align2::CENTER_TOP)
            } else {
                (Pos2::new(cr.center().x, cr.max.y - 12.0), Align2::CENTER_BOTTOM)
            };
            let (mut apply, mut cancel) = (false, false);
            egui::Area::new(Id::new("kadra_crop_bar"))
                .order(egui::Order::Foreground)
                .pivot(pivot)
                .fixed_pos(anchor)
                .show(&ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{} × {} px", c.width(), c.height())).monospace());
                            ui.add_space(6.0);
                            cancel = ui.add(egui::Button::new("Abbrechen")).on_hover_text("Esc").clicked();
                            apply = crate::ui_kit::primary_button(ui, "✔ Zuschneiden", self.crop_changed())
                                .on_hover_text("Enter")
                                .clicked();
                        });
                    });
                });
            if apply {
                self.apply_crop();
            } else if cancel {
                self.cancel_crop();
            }
        }

        // Texteingabe
        if let Some(i) = self.editing_text {
            let mut done = false;
            if let Some(Annotation { shape: Shape::Text { pos: tp, text }, style }) = self.doc.annotations.get_mut(i) {
                let (_, th) = render::text_size(if text.is_empty() { " " } else { text }, style.font_size);
                let anchor = to_scr(pos(*tp)) + Vec2::new(0.0, th * zoom + 10.0);
                let focus = std::mem::take(&mut self.focus_text);
                egui::Area::new(Id::new("kadra_text_edit"))
                    .fixed_pos(anchor)
                    .order(egui::Order::Foreground)
                    .show(&ctx, |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            let r = ui.add(
                                egui::TextEdit::multiline(text)
                                    .desired_rows(2)
                                    .desired_width(280.0)
                                    .hint_text("Text eingeben…"),
                            );
                            if focus {
                                r.request_focus();
                            }
                            ui.horizontal(|ui| {
                                if ui.button("Fertig").clicked() {
                                    done = true;
                                }
                                ui.weak("Esc beendet");
                            });
                        });
                    });
                if text.is_empty() {
                    // Platzhalter-Cursor im Bild
                    let s = to_scr(pos(*tp));
                    clip.line_segment([s, s + Vec2::new(0.0, style.font_size * zoom)], Stroke::new(2.0, c32(style.stroke)));
                }
            }
            if done {
                self.finish_text();
            }
            self.changed();
        }
    }
}

// ---------- Hilfsfunktionen ----------

/// Welcher Griff des Zuschneiderahmens liegt unter `p` (Bildschirmkoordinaten)?
/// `Some(None)` = im Rahmen (verschieben), `Some(Some((x, y)))` = Kante/Ecke.
fn crop_grip(cr: Rect, p: Pos2, aspect_locked: bool) -> Option<Option<(i8, i8)>> {
    const TOL: f32 = 10.0;
    let near = |a: f32, b: f32| (a - b).abs() <= TOL;
    let in_x = p.x >= cr.min.x - TOL && p.x <= cr.max.x + TOL;
    let in_y = p.y >= cr.min.y - TOL && p.y <= cr.max.y + TOL;
    let gx = if near(p.x, cr.min.x) { -1 } else if near(p.x, cr.max.x) { 1 } else { 0 };
    let gy = if near(p.y, cr.min.y) { -1 } else if near(p.y, cr.max.y) { 1 } else { 0 };
    if in_x && in_y && (gx != 0 || gy != 0) {
        let corner = gx != 0 && gy != 0;
        if corner || !aspect_locked {
            return Some(Some((gx, gy)));
        }
    }
    // Innen (inkl. Toleranzsaum, z. B. Kante bei festem Format) = verschieben
    (in_x && in_y).then_some(None)
}

/// Neuer Rahmen nach Ziehen um `d` (Bildpixel), innerhalb `bounds`, optional mit Seitenverhältnis.
fn drag_crop(orig: Rect, grip: Option<(i8, i8)>, d: Vec2, bounds: Rect, aspect: Option<f32>) -> Rect {
    const MIN: f32 = 8.0;
    let r = match grip {
        None => {
            let dx = d.x.clamp(bounds.min.x - orig.min.x, bounds.max.x - orig.max.x);
            let dy = d.y.clamp(bounds.min.y - orig.min.y, bounds.max.y - orig.max.y);
            orig.translate(Vec2::new(dx, dy))
        }
        Some((gx, gy)) => {
            let mut r = orig;
            match gx {
                -1 => r.min.x = (orig.min.x + d.x).clamp(bounds.min.x, orig.max.x - MIN),
                1 => r.max.x = (orig.max.x + d.x).clamp(orig.min.x + MIN, bounds.max.x),
                _ => {}
            }
            match gy {
                -1 => r.min.y = (orig.min.y + d.y).clamp(bounds.min.y, orig.max.y - MIN),
                1 => r.max.y = (orig.max.y + d.y).clamp(orig.min.y + MIN, bounds.max.y),
                _ => {}
            }
            match aspect {
                Some(a) if gx != 0 && gy != 0 => fit_aspect(r, gx, gy, a, bounds),
                _ => r,
            }
        }
    };
    Rect::from_min_max(r.min.round(), r.max.round())
}

#[cfg(test)]
mod edit_tests {
    use super::*;

    fn editor_with(shape: Shape) -> Editor {
        let mut doc = Document::new(RgbaImage::new(100, 80));
        doc.annotations.push(Annotation { shape, style: Style { width: 20.0, ..Style::default() } });
        Editor::new("t".into(), doc)
    }

    #[test]
    fn change_after_undo_is_own_step() {
        let mut ed = editor_with(Shape::Step { pos: [10.0, 10.0], n: 1 });
        let nudge = |ed: &mut Editor| {
            ed.push_undo_coalesced("nudge0".into());
            ed.doc.annotations[0].shape.translate(Vec2::new(1.0, 0.0));
        };
        nudge(&mut ed);
        ed.undo();
        nudge(&mut ed);
        assert!(ed.can_undo(), "zweites Verschieben ist rückgängig machbar");
        assert!(!ed.can_redo(), "neue Änderung leert den Redo-Stapel");
        ed.undo();
        assert_eq!(ed.doc.annotations[0].shape, Shape::Step { pos: [10.0, 10.0], n: 1 });
    }

    #[test]
    fn resize_keeps_fill_tolerance() {
        let mut ed = editor_with(Shape::Fill { pos: [40.0, 40.0] });
        ed.resize(0.5);
        let a = &ed.doc.annotations[0];
        assert_eq!(a.shape, Shape::Fill { pos: [20.0, 20.0] });
        assert_eq!(a.style.width, 20.0, "Toleranz unverändert");
    }
}

#[cfg(test)]
mod crop_tests {
    use super::*;

    fn r(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1))
    }

    #[test]
    fn grips_are_detected() {
        let cr = r(100.0, 100.0, 300.0, 200.0);
        assert_eq!(crop_grip(cr, Pos2::new(102.0, 98.0), false), Some(Some((-1, -1))), "Ecke oben links");
        assert_eq!(crop_grip(cr, Pos2::new(200.0, 201.0), false), Some(Some((0, 1))), "Kante unten");
        assert_eq!(crop_grip(cr, Pos2::new(200.0, 150.0), false), Some(None), "innen = verschieben");
        assert_eq!(crop_grip(cr, Pos2::new(50.0, 50.0), false), None, "außerhalb");
        assert_eq!(crop_grip(cr, Pos2::new(200.0, 201.0), true), Some(None), "Kanten bei festem Format gesperrt");
    }

    #[test]
    fn drag_respects_bounds_and_min_size() {
        let bounds = r(0.0, 0.0, 400.0, 300.0);
        let orig = r(50.0, 50.0, 350.0, 250.0);
        // Verschieben über den Rand hinaus wird begrenzt
        assert_eq!(drag_crop(orig, None, Vec2::new(500.0, -500.0), bounds, None), r(100.0, 0.0, 400.0, 200.0));
        // Linke Kante über die rechte hinaus: Mindestbreite bleibt
        let shrunk = drag_crop(orig, Some((-1, 0)), Vec2::new(1000.0, 0.0), bounds, None);
        assert_eq!(shrunk.width(), 8.0);
        // Ecke nach außen: an Bildgrenze gestoppt
        assert_eq!(drag_crop(orig, Some((1, 1)), Vec2::new(999.0, 999.0), bounds, None), r(50.0, 50.0, 400.0, 300.0));
    }

    #[test]
    fn corner_keeps_aspect() {
        let bounds = r(0.0, 0.0, 1000.0, 1000.0);
        let orig = r(100.0, 100.0, 300.0, 200.0); // 2:1
        let out = drag_crop(orig, Some((1, 1)), Vec2::new(100.0, 10.0), bounds, Some(2.0));
        assert_eq!(out.min, orig.min, "gegenüberliegende Ecke bleibt fest");
        assert!((out.width() / out.height() - 2.0).abs() < 0.02, "{out:?}");
    }
}

/// Rahmen auf Seitenverhältnis `a` bringen; die dem Griff gegenüberliegende Ecke bleibt fest.
fn fit_aspect(r: Rect, gx: i8, gy: i8, a: f32, bounds: Rect) -> Rect {
    let anchor = Pos2::new(if gx < 0 { r.max.x } else { r.min.x }, if gy < 0 { r.max.y } else { r.min.y });
    // Platz bis zum Bildrand in Zugrichtung
    let max_w = if gx < 0 { anchor.x - bounds.min.x } else { bounds.max.x - anchor.x };
    let max_h = if gy < 0 { anchor.y - bounds.min.y } else { bounds.max.y - anchor.y };
    let mut w = r.width().max(r.height() * a);
    let mut h = w / a;
    if w > max_w {
        w = max_w;
        h = w / a;
    }
    if h > max_h {
        h = max_h;
        w = h * a;
    }
    let corner = anchor + Vec2::new(gx as f32 * w, gy as f32 * h);
    Rect::from_two_pos(anchor, corner)
}

fn new_shape(t: Tool, a: P) -> Shape {
    match t {
        Tool::Arrow => Shape::Arrow { a, b: a },
        Tool::Line => Shape::Line { a, b: a },
        Tool::Ellipse => Shape::Ellipse { a, b: a },
        Tool::Pen => Shape::Pen { pts: vec![a] },
        Tool::Highlight => Shape::Highlight { a, b: a },
        Tool::Blur => Shape::Blur { a, b: a },
        Tool::Pixelate => Shape::Pixelate { a, b: a },
        Tool::Redact => Shape::Redact { a, b: a },
        Tool::Spotlight => Shape::Spotlight { a, b: a },
        _ => Shape::Rect { a, b: a },
    }
}

/// Aktualisiert die gerade gezeichnete Form. Shift = 45°-Winkel bzw. Quadrat/Kreis.
fn update_shape(shape: &mut Shape, start: Pos2, cur: Pos2, shift: bool, zoom: f32) {
    match shape {
        Shape::Arrow { b, .. } | Shape::Line { b, .. } => {
            let mut c = cur;
            if shift {
                let d = cur - start;
                let ang = (d.y.atan2(d.x) / std::f32::consts::FRAC_PI_4).round() * std::f32::consts::FRAC_PI_4;
                c = start + Vec2::angled(ang) * d.length();
            }
            *b = p(c);
        }
        Shape::Pen { pts } => {
            if pts.last().is_none_or(|l| (pos(*l) - cur).length() * zoom > 1.5) {
                pts.push(p(cur));
            }
        }
        Shape::Rect { b, .. }
        | Shape::Ellipse { b, .. }
        | Shape::Highlight { b, .. }
        | Shape::Blur { b, .. }
        | Shape::Pixelate { b, .. }
        | Shape::Redact { b, .. }
        | Shape::Spotlight { b, .. } => {
            let mut c = cur;
            if shift {
                let d = cur - start;
                let m = d.x.abs().max(d.y.abs());
                c = start + Vec2::new(m * d.x.signum(), m * d.y.signum());
            }
            *b = p(c);
        }
        _ => {}
    }
}

pub fn bounds(a: &Annotation) -> Rect {
    let w = a.style.width / 2.0;
    match &a.shape {
        Shape::Arrow { a: pa, b } | Shape::Line { a: pa, b } => rect(*pa, *b).expand(w),
        Shape::Rect { a: pa, b } | Shape::Ellipse { a: pa, b } => rect(*pa, *b).expand(w),
        Shape::Highlight { a: pa, b }
        | Shape::Blur { a: pa, b }
        | Shape::Pixelate { a: pa, b }
        | Shape::Redact { a: pa, b }
        | Shape::Spotlight { a: pa, b } => rect(*pa, *b),
        Shape::Pen { pts } => {
            let mut r = Rect::NOTHING;
            for q in pts {
                r.extend_with(pos(*q));
            }
            r.expand(w)
        }
        Shape::Text { pos: tp, text } => {
            let (tw, th) = render::text_size(if text.is_empty() { " " } else { text }, a.style.font_size);
            Rect::from_min_size(pos(*tp), Vec2::new(tw.max(10.0), th))
        }
        Shape::Step { pos: c, .. } => Rect::from_center_size(pos(*c), Vec2::splat(render::step_radius(a.style.font_size) * 2.0)),
        // Füllung: nur der Ansatzpunkt ist greifbar
        Shape::Fill { pos: c } => Rect::from_center_size(pos(*c), Vec2::splat(12.0)),
    }
}

fn hit_test(a: &Annotation, q: Pos2, tol: f32) -> bool {
    let w = a.style.width / 2.0 + tol;
    match &a.shape {
        Shape::Arrow { a: pa, b } | Shape::Line { a: pa, b } => dist_to_segment(q, pos(*pa), pos(*b)) < w,
        Shape::Pen { pts } => pts.windows(2).any(|s| dist_to_segment(q, pos(s[0]), pos(s[1])) < w)
            || pts.first().is_some_and(|f| (pos(*f) - q).length() < w),
        Shape::Rect { a: pa, b } => {
            let r = rect(*pa, *b);
            if a.style.fill.is_some() {
                r.expand(w).contains(q)
            } else {
                r.expand(w).contains(q) && !r.shrink(w).contains(q)
            }
        }
        Shape::Ellipse { a: pa, b } => {
            let r = rect(*pa, *b);
            let (rx, ry) = (r.width() / 2.0 + 0.01, r.height() / 2.0 + 0.01);
            let d = q - r.center();
            let n = ((d.x / rx).powi(2) + (d.y / ry).powi(2)).sqrt();
            if a.style.fill.is_some() {
                n <= 1.0 + w / rx.min(ry)
            } else {
                (n - 1.0).abs() * rx.min(ry) < w
            }
        }
        Shape::Spotlight { a: pa, b } => {
            let r = rect(*pa, *b);
            r.expand(tol).contains(q) && !r.shrink(tol).contains(q)
        }
        _ => bounds(a).expand(tol).contains(q),
    }
}

pub fn paint_annotation(painter: &Painter, a: &Annotation, img_rect: Rect, zoom: f32, shadow: bool) {
    let off = if shadow { render::shadow_offset(a) } else { 0.0 };
    let tr = |q: P| img_rect.min + Vec2::new(q[0] + off, q[1] + off) * zoom;
    let st = &a.style;
    let shadow_col = Color32::from_black_alpha(90);
    let col = if shadow { shadow_col } else { c32(st.stroke) };
    let fill = st.fill.map(|f| if shadow { shadow_col } else { c32(f) });
    let stroke = Stroke::new(st.width * zoom, col);
    let rr = |pa: P, pb: P| Rect::from_two_pos(tr(pa), tr(pb));
    match &a.shape {
        Shape::Arrow { a: pa, b } => {
            let (end, tri) = render::arrow_head(*pa, *b, st.width);
            painter.line_segment([tr(*pa), tr(end)], stroke);
            painter.circle_filled(tr(*pa), st.width * zoom / 2.0, col);
            painter.add(egui::Shape::convex_polygon(tri.iter().map(|q| tr(*q)).collect(), col, Stroke::NONE));
        }
        Shape::Line { a: pa, b } => {
            painter.line_segment([tr(*pa), tr(*b)], stroke);
            painter.circle_filled(tr(*pa), st.width * zoom / 2.0, col);
            painter.circle_filled(tr(*b), st.width * zoom / 2.0, col);
        }
        Shape::Rect { a: pa, b } => {
            let r = rr(*pa, *b);
            if let Some(f) = fill {
                painter.rect_filled(r, 0.0, f);
            }
            painter.rect_stroke(r, 0.0, stroke, StrokeKind::Middle);
        }
        Shape::Ellipse { a: pa, b } => {
            let r = rr(*pa, *b);
            if let Some(f) = fill {
                painter.add(egui::Shape::ellipse_filled(r.center(), r.size() / 2.0, f));
            }
            painter.add(egui::Shape::ellipse_stroke(r.center(), r.size() / 2.0, stroke));
        }
        Shape::Pen { pts } => {
            let v: Vec<Pos2> = pts.iter().map(|q| tr(*q)).collect();
            if st.width * zoom > 2.5 {
                for q in &v {
                    painter.circle_filled(*q, st.width * zoom / 2.0, col);
                }
            }
            painter.add(egui::Shape::line(v, stroke));
        }
        Shape::Highlight { a: pa, b } => {
            if !shadow {
                let c = st.stroke;
                painter.rect_filled(rr(*pa, *b), 0.0, Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 100));
            }
        }
        Shape::Redact { a: pa, b } => {
            if !shadow {
                let c = st.stroke;
                painter.rect_filled(rr(*pa, *b), 0.0, Color32::from_rgb(c[0], c[1], c[2]));
            }
        }
        Shape::Text { pos: tp, text } => {
            let (w, h) = render::text_size(text, st.font_size);
            let pad = st.font_size * 0.25;
            if let Some(f) = fill {
                if !text.is_empty() {
                    painter.rect_filled(rr([tp[0] - pad, tp[1] - pad], [tp[0] + w + pad, tp[1] + h + pad]), 0.0, f);
                }
            }
            let font = FontId::proportional(st.font_size * zoom);
            if shadow {
                if st.fill.is_none() {
                    painter.text(tr(*tp), Align2::LEFT_TOP, text, font, col);
                }
            } else {
                painter.text(tr(*tp), Align2::LEFT_TOP, text, font, c32(st.stroke));
            }
        }
        Shape::Step { pos: c, n } => {
            painter.circle_filled(tr(*c), render::step_radius(st.font_size) * zoom, col);
            if !shadow {
                painter.text(tr(*c), Align2::CENTER_CENTER, n.to_string(), FontId::proportional(st.font_size * zoom), Color32::WHITE);
            }
        }
        Shape::Spotlight { a: pa, b } => {
            if !shadow {
                let s = rr(*pa, *b);
                let o = img_rect;
                let dim = Color32::from_black_alpha(150);
                painter.rect_filled(Rect::from_min_max(o.min, Pos2::new(o.max.x, s.min.y)), 0.0, dim);
                painter.rect_filled(Rect::from_min_max(Pos2::new(o.min.x, s.max.y), o.max), 0.0, dim);
                painter.rect_filled(Rect::from_min_max(Pos2::new(o.min.x, s.min.y), Pos2::new(s.min.x, s.max.y)), 0.0, dim);
                painter.rect_filled(Rect::from_min_max(Pos2::new(s.max.x, s.min.y), Pos2::new(o.max.x, s.max.y)), 0.0, dim);
            }
        }
        Shape::Blur { .. } | Shape::Pixelate { .. } | Shape::Fill { .. } => {}
    }
}

fn style_ui(ui: &mut Ui, kind: Tool, st: &mut Style) {
    let has_color = !matches!(kind, Tool::Blur | Tool::Pixelate | Tool::Spotlight);
    if has_color {
        ui.horizontal(|ui| {
            ui.label("Farbe");
            ui.color_edit_button_srgba_unmultiplied(&mut st.stroke);
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(4.0);
            for c in PALETTE {
                let (r, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                ui.painter().rect_filled(r, 4.0, c32(c));
                let sel = st.stroke == c;
                ui.painter().rect_stroke(
                    r,
                    4.0,
                    Stroke::new(if sel { 2.5 } else { 1.0 }, if sel { Color32::from_rgb(0, 150, 255) } else { Color32::GRAY }),
                    StrokeKind::Outside,
                );
                if resp.clicked() {
                    st.stroke = c;
                }
            }
        });
    }
    if matches!(kind, Tool::Rect | Tool::Ellipse | Tool::Text) {
        let mut has_fill = st.fill.is_some();
        ui.horizontal(|ui| {
            ui.checkbox(&mut has_fill, if kind == Tool::Text { "Hintergrund" } else { "Füllung" });
            if has_fill {
                let mut f = st.fill.unwrap_or([255, 255, 255, 220]);
                ui.color_edit_button_srgba_unmultiplied(&mut f);
                st.fill = Some(f);
            } else {
                st.fill = None;
            }
        });
    }
    match kind {
        Tool::Arrow | Tool::Line | Tool::Rect | Tool::Ellipse | Tool::Pen => {
            ui.horizontal(|ui| {
                ui.label("Stärke");
                ui.add(egui::Slider::new(&mut st.width, 1.0..=40.0).suffix(" px"));
            });
        }
        Tool::Blur | Tool::Pixelate => {
            ui.horizontal(|ui| {
                ui.label("Intensität");
                ui.add(egui::Slider::new(&mut st.width, 1.0..=30.0));
            });
            ui.weak("Für Passwörter und vertrauliche Daten besser Schwärzen (K) verwenden – \
                     weichgezeichneter oder verpixelter Text lässt sich teils rekonstruieren.");
        }
        Tool::Redact => {
            ui.weak("Deckender Balken ohne Transparenz – der Inhalt darunter ist im Export nicht mehr enthalten.");
        }
        Tool::Text | Tool::Step => {
            ui.horizontal(|ui| {
                ui.label("Schriftgröße");
                ui.add(egui::Slider::new(&mut st.font_size, 8.0..=200.0).suffix(" px"));
            });
        }
        Tool::Fill => {
            ui.horizontal(|ui| {
                ui.label("Toleranz");
                ui.add(egui::Slider::new(&mut st.width, 0.0..=100.0).suffix(" %"));
            });
            ui.weak("In eine Fläche klicken: zusammenhängende Pixel ähnlicher Farbe werden gefüllt. \
                     Höhere Toleranz erfasst mehr Farbtöne. Transparenz über den Farbwähler.");
        }
        _ => {}
    }
    if !matches!(kind, Tool::Blur | Tool::Pixelate | Tool::Redact | Tool::Spotlight | Tool::Highlight | Tool::Fill) {
        ui.checkbox(&mut st.shadow, "Schatten");
    }
}
