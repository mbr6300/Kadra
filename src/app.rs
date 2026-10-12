//! Hauptanwendung: Aufnahme-Ablauf, Bibliothek, Editor-Layout.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use egui::{Color32, RichText, Vec2, ViewportCommand};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use image::RgbaImage;

use crate::capture::{self, CaptureKind, Grab};
use crate::doc::Document;
use crate::editor::Editor;
use crate::library::{self, Item, Settings};
use crate::overlay::{Overlay, OverlayResult};
use crate::platform;
use crate::render;

enum Phase {
    Normal,
    /// Fenster ist versteckt, Aufnahme läuft im Hintergrund.
    Capturing(Receiver<Result<Grab, String>>),
    Overlay(Box<Overlay>),
}

struct Hotkeys {
    _manager: GlobalHotKeyManager,
    map: Vec<(u32, CaptureKind)>,
}

pub struct SnapApp {
    settings: Settings,
    items: Vec<Item>,
    editor: Option<Editor>,
    phase: Phase,
    saved_window: Option<(egui::Pos2, Vec2)>,
    status: Option<(String, Instant)>,
    thumb_tx: Sender<(String, RgbaImage)>,
    thumb_rx: Receiver<(String, RgbaImage)>,
    hotkeys: Option<Hotkeys>,
    hotkey_rx: Receiver<u32>,
    show_settings: bool,
    show_permission: bool,
    confirm_delete: Option<String>,
}

pub const HOTKEYS: [(Code, CaptureKind, &str); 4] = [
    (Code::Digit1, CaptureKind::Region, "⌃⇧1"),
    (Code::Digit2, CaptureKind::Window, "⌃⇧2"),
    (Code::Digit3, CaptureKind::Screen, "⌃⇧3"),
    (Code::Digit4, CaptureKind::AllScreens, "⌃⇧4"),
];

impl SnapApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = Settings::load();
        let (thumb_tx, thumb_rx) = channel();
        let (hk_tx, hotkey_rx) = channel();

        let hotkeys = settings.hotkeys.then(|| register_hotkeys(&cc.egui_ctx, hk_tx)).flatten();

        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.button_padding = Vec2::new(8.0, 4.0);
        cc.egui_ctx.set_style(style);

        let mut app = Self {
            settings,
            items: library::list(),
            editor: None,
            phase: Phase::Normal,
            saved_window: None,
            status: None,
            thumb_tx,
            thumb_rx,
            hotkeys,
            hotkey_rx,
            show_settings: false,
            show_permission: false,
            confirm_delete: None,
        };
        // `snapit bild.png` öffnet eine Datei direkt im Editor.
        if let Some(path) = std::env::args_os().nth(1) {
            app.open_path(std::path::Path::new(&path));
        }
        app
    }

    fn notify(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), Instant::now()));
    }

    // ---------- Aufnahme ----------

    fn start_capture(&mut self, ctx: &egui::Context, kind: CaptureKind) {
        if !matches!(self.phase, Phase::Normal) {
            return;
        }
        if !platform::screen_capture_allowed() {
            // Ohne Berechtigung liefert macOS nur das Hintergrundbild ohne Fenster.
            platform::request_screen_capture();
            self.show_permission = true;
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            return;
        }
        self.flush_save(true);
        self.saved_window = ctx.input(|i| {
            let vp = i.viewport();
            Some((vp.outer_rect?.min, vp.inner_rect?.size()))
        });
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        let (tx, rx) = channel();
        let delay = Duration::from_secs(self.settings.delay_secs as u64) + Duration::from_millis(350);
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let _ = tx.send(capture::grab(kind));
            ctx2.request_repaint();
        });
        self.phase = Phase::Capturing(rx);
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    fn restore_window(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        platform::set_overlay_level(frame, false);
        ctx.send_viewport_cmd(ViewportCommand::Decorations(true));
        if let Some((pos, size)) = self.saved_window.take() {
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        }
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    fn poll_capture(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let Phase::Capturing(rx) = &self.phase else { return };
        let Ok(res) = rx.try_recv() else {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        };
        match res {
            Err(e) => {
                self.phase = Phase::Normal;
                self.restore_window(ctx, frame);
                self.notify(format!("Fehler: {e} – Bildschirmaufnahme-Berechtigung prüfen."));
            }
            Ok(grab) if grab.kind.needs_overlay() => {
                let (origin, size) = (grab.origin, grab.size);
                self.phase = Phase::Overlay(Box::new(Overlay::new(ctx, grab)));
                platform::set_overlay_level(frame, true);
                ctx.send_viewport_cmd(ViewportCommand::Decorations(false));
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(origin));
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
                ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(ViewportCommand::Focus);
            }
            Ok(grab) => {
                self.phase = Phase::Normal;
                self.restore_window(ctx, frame);
                self.open_new(grab.image);
            }
        }
    }

    /// Neues Bild in Bibliothek und Editor übernehmen.
    fn open_new(&mut self, img: RgbaImage) {
        self.flush_save(true);
        let id = library::new_id();
        let doc = Document::new(img);
        if self.settings.copy_after_capture {
            let _ = copy_to_clipboard(&doc.image);
        }
        let mut ed = Editor::new(id.clone(), doc);
        ed.dirty = true;
        ed.image_changed = true;
        self.items.insert(0, Item { id, thumb: None, texture: None });
        self.editor = Some(ed);
        self.flush_save(true);
    }

    fn open_item(&mut self, id: &str) {
        if self.editor.as_ref().is_some_and(|e| e.id == id) {
            return;
        }
        self.flush_save(true);
        match library::load(id) {
            Ok(doc) => self.editor = Some(Editor::new(id.to_string(), doc)),
            Err(e) => self.notify(format!("Konnte nicht laden: {e}")),
        }
    }

    /// Speichert den Editor-Zustand im Hintergrund in die Bibliothek.
    fn flush_save(&mut self, force: bool) {
        let Some(ed) = &mut self.editor else { return };
        if !ed.dirty || ed.is_busy() || (!force && ed.last_change.elapsed() < Duration::from_millis(1200)) {
            return;
        }
        ed.dirty = false;
        let image_changed = std::mem::take(&mut ed.image_changed);
        let (id, doc, tx) = (ed.id.clone(), ed.doc.clone(), self.thumb_tx.clone());
        std::thread::spawn(move || {
            if let Ok(thumb) = library::save(&id, &doc, image_changed) {
                let _ = tx.send((id, thumb));
            }
        });
    }

    // ---------- Export ----------

    fn rendered(&self) -> Option<RgbaImage> {
        self.editor.as_ref().map(|e| render::render(&e.doc))
    }

    fn copy(&mut self) {
        if let Some(img) = self.rendered() {
            match copy_to_clipboard(&img) {
                Ok(()) => self.notify("In die Zwischenablage kopiert"),
                Err(e) => self.notify(format!("Zwischenablage: {e}")),
            }
        }
    }

    fn quick_save(&mut self) {
        let Some(img) = self.rendered() else { return };
        let name = format!("SnapIt {}.{}", chrono::Local::now().format("%Y-%m-%d um %H.%M.%S"), self.settings.format);
        let path = self.settings.save_dir.join(name);
        match library::export(&img, &path) {
            Ok(()) => self.notify(format!("Gespeichert: {}", path.display())),
            Err(e) => self.notify(format!("Speichern fehlgeschlagen: {e}")),
        }
    }

    fn save_as(&mut self) {
        let Some(img) = self.rendered() else { return };
        let name = format!("SnapIt {}.{}", chrono::Local::now().format("%Y-%m-%d um %H.%M.%S"), self.settings.format);
        let _ = std::fs::create_dir_all(&self.settings.save_dir);
        let Some(path) = rfd::FileDialog::new()
            .set_directory(&self.settings.save_dir)
            .set_file_name(name)
            .add_filter("PNG", &["png"])
            .add_filter("JPEG", &["jpg", "jpeg"])
            .add_filter("WebP", &["webp"])
            .add_filter("BMP", &["bmp"])
            .add_filter("GIF", &["gif"])
            .add_filter("TIFF", &["tiff"])
            .save_file()
        else {
            return;
        };
        match library::export(&img, &path) {
            Ok(()) => self.notify(format!("Gespeichert: {}", path.display())),
            Err(e) => self.notify(format!("Speichern fehlgeschlagen: {e}")),
        }
    }

    fn open_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Bilder", &["png", "jpg", "jpeg", "bmp", "gif", "webp", "tiff"])
            .pick_file()
        {
            self.open_path(&path);
        }
    }

    fn open_path(&mut self, path: &std::path::Path) {
        match image::open(path) {
            Ok(img) => self.open_new(img.to_rgba8()),
            Err(e) => self.notify(format!("Öffnen fehlgeschlagen: {e}")),
        }
    }

    fn paste(&mut self) {
        let img = arboard::Clipboard::new().and_then(|mut c| c.get_image()).ok().and_then(|d| {
            RgbaImage::from_raw(d.width as u32, d.height as u32, d.bytes.into_owned())
        });
        match img {
            Some(img) => self.open_new(img),
            None => self.notify("Kein Bild in der Zwischenablage"),
        }
    }

    // ---------- UI ----------

    fn toolbar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            for (i, kind) in [CaptureKind::Region, CaptureKind::Window, CaptureKind::Screen, CaptureKind::AllScreens]
                .into_iter()
                .enumerate()
            {
                let icon = ["⛶", "🗔", "🖵", "🖥"][i];
                let text = RichText::new(format!("{icon}  {}", kind.label())).size(14.0);
                let btn = egui::Button::new(text).fill(if i == 0 { Color32::from_rgb(220, 90, 20) } else { ui.visuals().widgets.inactive.bg_fill });
                if ui.add(btn).on_hover_text(format!("Aufnehmen ({})", HOTKEYS[i].2)).clicked() {
                    self.start_capture(ctx, kind);
                }
            }
            ui.separator();
            ui.label("⏱");
            let mut d = self.settings.delay_secs;
            egui::ComboBox::from_id_salt("delay")
                .width(70.0)
                .selected_text(if d == 0 { "sofort".into() } else { format!("{d} s") })
                .show_ui(ui, |ui| {
                    for v in [0, 1, 3, 5, 10] {
                        ui.selectable_value(&mut d, v, if v == 0 { "sofort".into() } else { format!("{v} s") });
                    }
                });
            if d != self.settings.delay_secs {
                self.settings.delay_secs = d;
                self.settings.save();
            }
            ui.separator();
            if ui.button("📋 Einfügen").on_hover_text("Bild aus Zwischenablage").clicked() {
                self.paste();
            }
            if ui.button("📂 Öffnen").clicked() {
                self.open_file();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⚙").on_hover_text("Einstellungen").clicked() {
                    self.show_settings = true;
                }
                let has = self.editor.is_some();
                ui.add_enabled_ui(has, |ui| {
                    if ui.button("💾 Speichern unter…").on_hover_text("⌘⇧S").clicked() {
                        self.save_as();
                    }
                    if ui.button("💾 Speichern").on_hover_text("Schnell speichern in den Zielordner (⌘S)").clicked() {
                        self.quick_save();
                    }
                    if ui.button("📋 Kopieren").on_hover_text("⌘C").clicked() {
                        self.copy();
                    }
                    ui.separator();
                    let (cr, cu) = self.editor.as_ref().map(|e| (e.can_redo(), e.can_undo())).unwrap_or_default();
                    if ui.add_enabled(cr, egui::Button::new("↷")).on_hover_text("Wiederholen (⌘⇧Z)").clicked() {
                        if let Some(e) = &mut self.editor {
                            e.redo()
                        }
                    }
                    if ui.add_enabled(cu, egui::Button::new("↶")).on_hover_text("Rückgängig (⌘Z)").clicked() {
                        if let Some(e) = &mut self.editor {
                            e.undo()
                        }
                    }
                });
            });
        });
    }

    fn tray(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        for it in &mut self.items {
            if it.texture.is_none() {
                if let Some(t) = &it.thumb {
                    let c = egui::ColorImage::from_rgba_unmultiplied([t.width() as usize, t.height() as usize], t.as_raw());
                    it.texture = Some(ctx.load_texture(format!("thumb-{}", it.id), c, Default::default()));
                }
            }
        }
        let current = self.editor.as_ref().map(|e| e.id.clone());
        let mut open = None;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                for it in &self.items {
                    let h = 76.0;
                    let size = it.texture.as_ref().map(|t| {
                        let s = t.size_vec2();
                        Vec2::new((s.x / s.y * h).clamp(40.0, 180.0), h)
                    });
                    let (rect, resp) = ui.allocate_exact_size(size.unwrap_or(Vec2::new(100.0, h)), egui::Sense::click());
                    let selected = current.as_deref() == Some(&it.id);
                    if let Some(t) = &it.texture {
                        egui::Image::new(egui::load::SizedTexture::new(t.id(), rect.size()))
                            .corner_radius(4)
                            .paint_at(ui, rect);
                    } else {
                        ui.painter().rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "…", Default::default(), Color32::GRAY);
                    }
                    let stroke = if selected {
                        egui::Stroke::new(3.0, Color32::from_rgb(255, 120, 30))
                    } else if resp.hovered() {
                        egui::Stroke::new(1.5, Color32::GRAY)
                    } else {
                        egui::Stroke::NONE
                    };
                    ui.painter().rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Outside);
                    if resp.clicked() {
                        open = Some(it.id.clone());
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Öffnen").clicked() {
                            open = Some(it.id.clone());
                            ui.close();
                        }
                        if ui.button("🗑 Aus Bibliothek löschen…").clicked() {
                            self.confirm_delete = Some(it.id.clone());
                            ui.close();
                        }
                    })
                    .map(|_| ());
                    resp.on_hover_text(&it.id);
                }
            });
        });
        if let Some(id) = open {
            self.open_item(&id);
        }
    }

    fn welcome(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.2);
            ui.heading(RichText::new("SnapIt").size(40.0).strong());
            ui.label("Bildschirmfotos aufnehmen, beschriften und teilen.");
            ui.add_space(24.0);
            for (i, (_, kind, hk)) in HOTKEYS.iter().enumerate() {
                let txt = RichText::new(format!("{}   {hk}", kind.label())).size(18.0);
                let btn = egui::Button::new(txt).min_size(Vec2::new(260.0, 44.0));
                let btn = if i == 0 { btn.fill(Color32::from_rgb(220, 90, 20)) } else { btn };
                if ui.add(btn).clicked() {
                    self.start_capture(ctx, *kind);
                }
            }
            ui.add_space(16.0);
            ui.weak("Tipp: Im Bereichsmodus ein Fenster anklicken, um nur dieses aufzunehmen.");
            ui.weak("Bilder können auch per Drag & Drop geöffnet werden.");
        });
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        let mut changed = false;
        egui::Window::new("Einstellungen").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Zielordner:");
                ui.monospace(self.settings.save_dir.display().to_string());
                if ui.button("Ändern…").clicked() {
                    if let Some(d) = rfd::FileDialog::new().set_directory(&self.settings.save_dir).pick_folder() {
                        self.settings.save_dir = d;
                        changed = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Format (Schnellspeichern):");
                for f in ["png", "jpg", "webp"] {
                    changed |= ui.selectable_value(&mut self.settings.format, f.to_string(), f.to_uppercase()).changed();
                }
            });
            changed |= ui.checkbox(&mut self.settings.copy_after_capture, "Nach Aufnahme automatisch in Zwischenablage kopieren").changed();
            changed |= ui.checkbox(&mut self.settings.hotkeys, "Globale Tastenkürzel (Neustart nötig)").changed();
            ui.separator();
            ui.label(RichText::new("Tastenkürzel").strong());
            for (_, k, hk) in HOTKEYS {
                ui.label(format!("{hk}  –  {}", k.label()));
            }
            if self.settings.hotkeys && self.hotkeys.is_none() {
                ui.colored_label(Color32::from_rgb(220, 120, 0), "Globale Kürzel konnten nicht registriert werden.");
            }
            ui.label("Editor: V Auswahl · A Pfeil · L Linie · R Rechteck · E Ellipse · P Stift · H Marker · T Text · N Nummer · B Weichzeichnen · X Verpixeln · S Spotlight · C Zuschneiden");
            ui.label("⌘Z / ⌘⇧Z Rückgängig/Wiederholen · ⌘D Duplizieren · Entf Löschen · Pfeiltasten Verschieben · ⌘+Scroll Zoom");
            ui.separator();
            ui.weak(format!("Bibliothek: {}", library::data_dir().join("library").display()));
        });
        if changed {
            self.settings.save();
        }
        self.show_settings = open;
    }

    fn permission_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_permission {
            return;
        }
        egui::Window::new("Berechtigung erforderlich")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("SnapIt braucht die Berechtigung „Bildschirm- und Systemaudioaufnahme“.");
                ui.label("Ohne sie liefert macOS nur das Hintergrundbild – ohne Fenster.");
                ui.add_space(6.0);
                ui.label("1. Einstellungen öffnen und SnapIt aktivieren");
                ui.label("2. SnapIt beenden und neu starten");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Einstellungen öffnen").clicked() {
                        platform::open_privacy_settings();
                    }
                    if ui.button("SnapIt beenden").clicked() {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                    if ui.button("Schließen").clicked() {
                        self.show_permission = false;
                    }
                });
            });
    }

    fn delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm_delete.clone() else { return };
        let mut close = false;
        egui::Window::new("Löschen?").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(format!("Aufnahme „{id}“ endgültig aus der Bibliothek löschen?"));
            ui.horizontal(|ui| {
                if ui.button("Löschen").clicked() {
                    library::delete(&id);
                    self.items.retain(|i| i.id != id);
                    if self.editor.as_ref().is_some_and(|e| e.id == id) {
                        self.editor = None;
                    }
                    close = true;
                }
                if ui.button("Abbrechen").clicked() {
                    close = true;
                }
            });
        });
        if close {
            self.confirm_delete = None;
        }
    }

    fn global_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let (copy, paste) = ctx.input(|i| {
            (
                i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
                i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))),
            )
        });
        if copy {
            self.copy();
        }
        if paste {
            self.paste();
        }
        let (cmd, shift) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
        if cmd && ctx.input(|i| i.key_pressed(egui::Key::S)) {
            if shift {
                self.save_as()
            } else {
                self.quick_save()
            }
        }
        if cmd && ctx.input(|i| i.key_pressed(egui::Key::O)) {
            self.open_file();
        }
    }
}

impl eframe::App for SnapApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Globale Hotkeys
        while let Ok(id) = self.hotkey_rx.try_recv() {
            if let Some(kind) = self.hotkeys.as_ref().and_then(|h| h.map.iter().find(|(i, _)| *i == id)).map(|(_, k)| *k) {
                self.start_capture(ctx, kind);
            }
        }
        // Fertige Thumbnails
        while let Ok((id, thumb)) = self.thumb_rx.try_recv() {
            if let Some(it) = self.items.iter_mut().find(|i| i.id == id) {
                it.thumb = Some(thumb);
                it.texture = None;
            }
        }

        self.poll_capture(ctx, frame);
        if let Phase::Overlay(ov) = &mut self.phase {
            match ov.ui(ctx) {
                OverlayResult::Pending => {}
                OverlayResult::Cancel => {
                    self.phase = Phase::Normal;
                    self.restore_window(ctx, frame);
                }
                OverlayResult::Done(img) => {
                    self.phase = Phase::Normal;
                    self.restore_window(ctx, frame);
                    self.open_new(img);
                }
            }
            return;
        }
        if matches!(self.phase, Phase::Capturing(_)) {
            egui::CentralPanel::default().show(ctx, |ui| ui.label("Aufnahme…"));
            return;
        }

        // Drag & Drop
        let dropped: Vec<_> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if let Some(p) = dropped.first() {
            self.open_path(p);
        }

        self.global_shortcuts(ctx);
        if let Some(ed) = &mut self.editor {
            ed.shortcuts(ctx);
        }
        self.flush_save(false);
        if self.editor.as_ref().is_some_and(|e| e.dirty) {
            ctx.request_repaint_after(Duration::from_millis(500));
        }

        egui::TopBottomPanel::top("toolbar").exact_height(44.0).show(ctx, |ui| {
            ui.add_space(6.0);
            self.toolbar(ctx, ui);
        });

        egui::TopBottomPanel::bottom("status").exact_height(22.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some((msg, t)) = &self.status {
                    if t.elapsed() < Duration::from_secs(6) {
                        ui.label(msg);
                        ctx.request_repaint_after(Duration::from_secs(1));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(format!("{} Aufnahmen in der Bibliothek", self.items.len()));
                });
            });
        });

        if !self.items.is_empty() {
            egui::TopBottomPanel::bottom("tray").exact_height(96.0).show(ctx, |ui| {
                ui.add_space(6.0);
                self.tray(ctx, ui);
            });
        }

        if let Some(ed) = &mut self.editor {
            egui::SidePanel::left("tools").exact_width(52.0).resizable(false).show(ctx, |ui| {
                ui.vertical_centered(|ui| ed.tools_ui(ui));
            });
            egui::SidePanel::right("props").default_width(250.0).show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| ed.properties_ui(ui));
            });
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| ed.canvas_ui(ui));
        } else {
            egui::CentralPanel::default().show(ctx, |ui| self.welcome(ctx, ui));
        }

        self.settings_window(ctx);
        self.permission_dialog(ctx);
        self.delete_dialog(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(ed) = &mut self.editor {
            if ed.dirty {
                ed.dirty = false;
                let _ = library::save(&ed.id, &ed.doc, ed.image_changed);
            }
        }
    }
}

fn copy_to_clipboard(img: &RgbaImage) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_image(arboard::ImageData {
        width: img.width() as usize,
        height: img.height() as usize,
        bytes: std::borrow::Cow::Borrowed(img.as_raw()),
    })
    .map_err(|e| e.to_string())
}

fn register_hotkeys(ctx: &egui::Context, tx: Sender<u32>) -> Option<Hotkeys> {
    let manager = GlobalHotKeyManager::new().ok()?;
    let mut map = vec![];
    for (code, kind, _) in HOTKEYS {
        let hk = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), code);
        if manager.register(hk).is_ok() {
            map.push((hk.id(), kind));
        }
    }
    let ctx = ctx.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
        if e.state == HotKeyState::Pressed {
            let _ = tx.send(e.id);
            ctx.request_repaint();
        }
    }));
    Some(Hotkeys { _manager: manager, map })
}
