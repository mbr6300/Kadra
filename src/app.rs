//! Hauptanwendung: Aufnahme-Ablauf, Bibliothek, Editor-Layout.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use egui::{Color32, RichText, Vec2, ViewportCommand};
use image::RgbaImage;

use crate::capture::{self, CaptureKind, Grab};
use crate::doc::Document;
use crate::editor::{self, Editor};
use crate::ui_kit::{self, bar_button, bar_separator, Look, ORANGE};
use crate::hotkeys::{self, Hotkeys, Shortcut};
use crate::library::{self, Item, Settings};
use crate::overlay::{Overlay, OverlayResult};
use crate::platform;
use crate::render;
use crate::tray::{Tray, TrayAction};

enum Phase {
    Normal,
    /// Fenster ist versteckt, Aufnahme läuft im Hintergrund.
    Capturing(Receiver<Result<Grab, String>>),
    Overlay(Box<Overlay>),
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
    overlay_fix_frames: u8,
    /// Modi, deren Kürzel nicht registriert werden konnte.
    hotkey_failed: Vec<CaptureKind>,
    /// Index des Modus, für den gerade ein neues Kürzel aufgenommen wird.
    recording: Option<usize>,
    recording_hint: Option<String>,
    pending_export: Option<PendingExport>,
    estimate_tx: Sender<(u8, usize)>,
    estimate_rx: Receiver<(u8, usize)>,
    /// Symbol in der Menüleiste (muss am Leben bleiben).
    tray: Option<Tray>,
    tray_rx: Receiver<TrayAction>,
    /// Fenster geschlossen, App läuft nur in der Menüleiste.
    hidden: bool,
    hidden_before_capture: bool,
    /// „SnapIt beenden“ gewählt – Schließen nicht mehr abfangen.
    quitting: bool,
    show_background_hint: bool,
}

/// macOS-Systemschrift mit ⌃ ⌥ ⇧ ⌘ als Ersatz anhängen (fehlen in den egui-Schriften).
fn install_symbol_font(ctx: &egui::Context) {
    let Ok(bytes) = std::fs::read("/System/Library/Fonts/Apple Symbols.ttf") else { return };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("symbols".into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("symbols".into());
    }
    ctx.set_fonts(fonts);
}

impl SnapApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = Settings::load();
        let (thumb_tx, thumb_rx) = channel();
        let (hk_tx, hotkey_rx) = channel();
        let (estimate_tx, estimate_rx) = channel();
        let (tray_tx, tray_rx) = channel();

        install_symbol_font(&cc.egui_ctx);
        let mut hotkeys = Hotkeys::new(&cc.egui_ctx, hk_tx);
        let hotkey_failed = hotkeys
            .as_mut()
            .map(|h| h.apply(&settings.shortcuts, settings.hotkeys))
            .unwrap_or_default();

        ui_kit::apply_style(&cc.egui_ctx);

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
            overlay_fix_frames: 0,
            hotkey_failed,
            recording: None,
            recording_hint: None,
            pending_export: None,
            estimate_tx,
            estimate_rx,
            tray: None,
            tray_rx,
            hidden: false,
            hidden_before_capture: false,
            quitting: false,
            show_background_hint: false,
        };
        app.tray = Tray::new(&cc.egui_ctx, tray_tx, &app.shortcut_texts());
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
            self.show_window(ctx);
            return;
        }
        self.flush_save(true);
        self.hidden_before_capture = self.hidden;
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

    /// Fenster nach der Aufnahme zurücksetzen. `show = false` lässt SnapIt im Hintergrund
    /// (wenn die Aufnahme aus der Menüleiste kam und abgebrochen wurde).
    fn restore_window(&mut self, ctx: &egui::Context, frame: &eframe::Frame, show: bool) {
        platform::set_overlay_level(frame, false);
        ctx.send_viewport_cmd(ViewportCommand::Decorations(true));
        if let Some((pos, size)) = self.saved_window.take() {
            // Erst Größe, dann Position: macOS rechnet die Position mit der aktuellen Höhe um.
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
        }
        if show {
            self.show_window(ctx);
        } else {
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }
    }

    /// Hauptfenster zeigen und SnapIt wieder ins Dock holen.
    fn show_window(&mut self, ctx: &egui::Context) {
        self.hidden = false;
        platform::set_dock_visible(true);
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    /// Fenster schließen = nur ausblenden; SnapIt läuft in der Menüleiste weiter.
    fn hide_to_background(&mut self, ctx: &egui::Context) {
        if !self.settings.background_hint_shown && self.tray.is_some() {
            // Beim ersten Mal erklären, wo SnapIt bleibt (Dialog blendet danach aus).
            self.show_background_hint = true;
            return;
        }
        self.flush_save(true);
        self.hidden = true;
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        platform::set_dock_visible(false);
    }

    fn background_hint_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_background_hint {
            return;
        }
        egui::Window::new("SnapIt läuft weiter")
            .collapsible(false)
            .resizable(false)
            .min_width(420.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Das Fenster wird geschlossen, SnapIt bleibt aber in der Menüleiste oben rechts aktiv.");
                ui.label("Über das Symbol ⛶ erreichst du die Aufnahmen, öffnest SnapIt wieder oder beendest es ganz.");
                ui.weak("Globale Tastenkürzel funktionieren weiterhin.");
                let (ok, cancel) = ui_kit::dialog_buttons(ui, "Verstanden", true);
                if ok {
                    self.show_background_hint = false;
                    self.settings.background_hint_shown = true;
                    self.settings.save();
                    self.hide_to_background(ctx);
                } else if cancel {
                    self.show_background_hint = false;
                }
            });
    }

    fn handle_tray(&mut self, ctx: &egui::Context) {
        while let Ok(action) = self.tray_rx.try_recv() {
            match action {
                TrayAction::Capture(kind) => self.start_capture(ctx, kind),
                TrayAction::Show => self.show_window(ctx),
                TrayAction::CopyLast => {
                    if self.editor.is_none() {
                        if let Some(id) = self.items.first().map(|i| i.id.clone()) {
                            self.open_item(&id);
                        }
                    }
                    self.copy();
                }
                TrayAction::Settings => {
                    self.show_settings = true;
                    self.show_window(ctx);
                }
                TrayAction::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }
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
                self.restore_window(ctx, frame, true);
                self.notify(format!("Fehler: {e} – Bildschirmaufnahme-Berechtigung prüfen."));
            }
            Ok(grab) if grab.kind.needs_overlay() => {
                let (origin, size) = (grab.origin, grab.size);
                self.phase = Phase::Overlay(Box::new(Overlay::new(ctx, grab)));
                // Aus dem Hintergrund: App aktivieren, damit das Overlay Tastatur/Maus bekommt.
                platform::activate_app();
                platform::set_overlay_level(frame, true);
                ctx.send_viewport_cmd(ViewportCommand::Decorations(false));
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(origin));
                self.overlay_fix_frames = 4;
                ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(ViewportCommand::Focus);
            }
            Ok(grab) => {
                self.phase = Phase::Normal;
                self.restore_window(ctx, frame, true);
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
        if let Some(prev) = &self.editor {
            ed.inherit(prev);
        }
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
            Ok(doc) => {
                let mut ed = Editor::new(id.to_string(), doc);
                if let Some(prev) = &self.editor {
                    ed.inherit(prev);
                }
                self.editor = Some(ed);
            }
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

    fn default_name(ext: &str) -> String {
        format!("SnapIt {}.{ext}", chrono::Local::now().format("%Y-%m-%d um %H.%M.%S"))
    }

    fn export_to(&mut self, img: &RgbaImage, path: &std::path::Path) {
        match library::export(img, path, self.settings.jpeg_quality) {
            Ok(()) => self.notify(format!("Gespeichert: {}", path.display())),
            Err(e) => self.notify(format!("Speichern fehlgeschlagen: {e}")),
        }
    }

    fn quick_save(&mut self) {
        let Some(img) = self.rendered() else { return };
        let path = self.settings.save_dir.join(Self::default_name(&self.settings.format));
        self.export_to(&img, &path);
    }

    /// Speichern-Dialog; mit `format` nur dieses Format, sonst alle (Standardformat zuerst).
    fn save_as(&mut self, format: Option<&str>) {
        let Some(img) = self.rendered() else { return };
        let ext = format.unwrap_or(&self.settings.format).to_string();
        let _ = std::fs::create_dir_all(&self.settings.save_dir);
        let mut dialog = rfd::FileDialog::new()
            .set_title(format!("Exportieren als {}", library::format_name(&ext)))
            .set_directory(&self.settings.save_dir)
            .set_file_name(Self::default_name(&ext));
        let mut formats: Vec<_> = library::FORMATS.iter().filter(|f| format.is_none() || f.0 == ext).collect();
        formats.sort_by_key(|f| f.0 != ext);
        for (main, name, alt) in formats {
            let exts: Vec<&str> = std::iter::once(*main).chain(alt.iter().copied()).collect();
            dialog = dialog.add_filter(*name, &exts);
        }
        let Some(mut path) = dialog.save_file() else { return };
        if path.extension().is_none() {
            path.set_extension(&ext);
        }
        if library::has_quality(&path) {
            // Qualität vor dem Schreiben abfragen (der Systemdialog kann das nicht).
            self.pending_export = Some(PendingExport {
                img: std::sync::Arc::new(img),
                path,
                quality: self.settings.jpeg_quality,
                remember: false,
                estimate: None,
                requested: None,
            });
        } else {
            self.export_to(&img, &path);
        }
    }

    /// Dialog mit Qualitätsregler und geschätzter Dateigröße für JPEG/PDF.
    fn export_dialog(&mut self, ctx: &egui::Context) {
        while let Ok((q, size)) = self.estimate_rx.try_recv() {
            if let Some(p) = &mut self.pending_export {
                p.estimate = Some((q, size));
            }
        }
        let Some(p) = &mut self.pending_export else { return };
        // Größe für die aktuelle Qualität im Hintergrund berechnen
        let dragging = ctx.input(|i| i.pointer.primary_down());
        if p.requested != Some(p.quality) && !dragging {
            p.requested = Some(p.quality);
            let (img, q, tx, ctx2) = (p.img.clone(), p.quality, self.estimate_tx.clone(), ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send((q, library::jpeg_size(&img, q)));
                ctx2.request_repaint();
            });
        }
        let name = p.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let fmt = library::format_name(&p.path.extension().and_then(|e| e.to_str()).unwrap_or("jpg").to_lowercase());
        let (mut save, mut cancel) = (false, false);
        egui::Window::new(format!("{fmt}-Export"))
            .collapsible(false)
            .resizable(false)
            .min_width(440.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(&name).strong().size(15.0));
                ui.weak(format!("{} × {} px", p.img.width(), p.img.height()));
                ui.add_space(12.0);
                egui::Grid::new("export_opts").num_columns(2).spacing([16.0, 14.0]).show(ui, |ui| {
                    ui.label("Qualität");
                    ui.add(egui::Slider::new(&mut p.quality, 10..=100).suffix(" %"));
                    ui.end_row();

                    ui.label("Voreinstellung");
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (label, q) in [("Klein", 60), ("Mittel", 80), ("Hoch", 90), ("Maximal", 100)] {
                            let btn = egui::Button::selectable(p.quality == q, label).min_size(Vec2::new(70.0, 30.0));
                            if ui.add(btn).clicked() {
                                p.quality = q;
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("Dateigröße");
                    let size = match p.estimate {
                        Some((q, bytes)) if q == p.quality => format!("ca. {}", human_size(bytes)),
                        Some((_, bytes)) => format!("ca. {} …", human_size(bytes)),
                        None => "wird berechnet …".into(),
                    };
                    ui.label(RichText::new(size).strong());
                    ui.end_row();
                });
                ui.add_space(10.0);
                ui.checkbox(&mut p.remember, "Als Standard merken (auch für Schnellspeichern)");
                ui.add_space(6.0);
                ui.separator();
                (save, cancel) = ui_kit::dialog_buttons(ui, "Speichern", true);
            });
        if save {
            let p = self.pending_export.take().unwrap();
            if p.remember {
                self.settings.jpeg_quality = p.quality;
                self.settings.save();
            }
            match library::export(&p.img, &p.path, p.quality) {
                Ok(()) => self.notify(format!("Gespeichert: {}", p.path.display())),
                Err(e) => self.notify(format!("Speichern fehlgeschlagen: {e}")),
            }
        } else if cancel {
            self.pending_export = None;
        }
    }

    fn open_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Bilder", &["png", "jpg", "jpeg", "bmp", "gif", "webp", "tiff", "tif"])
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

    /// Obere Leiste: Aufnahme/Datei links, Werkzeuge mittig, Ausgabe rechts.
    fn toolbar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let full = ui.max_rect();
        let tool = Look::Tool { selected: false };
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(4.0);
            let resp = bar_button(ui, "⏺", "Aufnahme", Look::Primary, true, true).on_hover_text("Aufnahmemodus wählen");
            egui::Popup::menu(&resp).show(|ui| self.capture_menu(ctx, ui));
            if bar_button(ui, "📂", "Öffnen", tool, true, false).on_hover_text("Bild öffnen (⌘O)").clicked() {
                self.open_file();
            }
            if bar_button(ui, "📋", "Einfügen", tool, true, false).on_hover_text("Bild aus der Zwischenablage").clicked() {
                self.paste();
            }
            bar_separator(ui);
            let (cu, cr) = self.editor.as_ref().map(|e| (e.can_undo(), e.can_redo())).unwrap_or_default();
            if bar_button(ui, "↶", "Zurück", tool, cu, false).on_hover_text("Rückgängig (⌘Z)").clicked() {
                if let Some(e) = &mut self.editor {
                    e.undo();
                }
            }
            if bar_button(ui, "↷", "Vor", tool, cr, false).on_hover_text("Wiederholen (⌘⇧Z)").clicked() {
                if let Some(e) = &mut self.editor {
                    e.redo();
                }
            }

            // Werkzeuge mittig ausrichten
            let start = (full.center().x - editor::tool_bar_width(ui) / 2.0).max(ui.cursor().min.x + 16.0);
            ui.add_space(start - ui.cursor().min.x);
            editor::tool_bar(ui, self.editor.as_mut());

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.add_space(4.0);
                if bar_button(ui, "⚙", "Einstellungen", tool, true, false).clicked() {
                    self.show_settings = true;
                }
                let has = self.editor.is_some();
                let resp = bar_button(ui, "💾", "Speichern", Look::Primary, has, true);
                egui::Popup::menu(&resp).show(|ui| {
                    ui.set_min_width(220.0);
                    let quick = format!("Schnell speichern ({})", library::format_name(&self.settings.format));
                    if ui.add(egui::Button::new(quick).shortcut_text("⌘S")).clicked() {
                        self.quick_save();
                        ui.close();
                    }
                    if ui.add(egui::Button::new("Speichern unter …").shortcut_text("⌘⇧S")).clicked() {
                        ui.close();
                        self.save_as(None);
                    }
                    ui.menu_button("Exportieren als", |ui| {
                        for (ext, name, _) in library::FORMATS {
                            if ui.button(format!("{name}  (.{ext})")).clicked() {
                                ui.close();
                                self.save_as(Some(ext));
                            }
                        }
                    });
                    ui.separator();
                    if ui.button("Zielordner im Finder zeigen").clicked() {
                        let _ = std::fs::create_dir_all(&self.settings.save_dir);
                        let _ = std::process::Command::new("open").arg(&self.settings.save_dir).spawn();
                        ui.close();
                    }
                });
                if bar_button(ui, "⧉", "Kopieren", tool, has, false).on_hover_text("In die Zwischenablage (⌘C)").clicked() {
                    self.copy();
                }
            });
        });
    }

    fn capture_menu(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.set_min_width(240.0);
        for (i, kind) in CaptureKind::ALL.into_iter().enumerate() {
            let icon = ["⛶", "🗔", "🖵", "🖥"][i];
            let btn = egui::Button::new(format!("{icon}  {}", kind.label())).shortcut_text(self.shortcut_text(i).unwrap_or_default());
            if ui.add(btn).clicked() {
                ui.close();
                self.start_capture(ctx, kind);
            }
        }
        ui.separator();
        ui.weak("Verzögerung");
        ui.horizontal(|ui| {
            for v in [0, 1, 3, 5, 10] {
                let label = if v == 0 { "sofort".to_string() } else { format!("{v} s") };
                if ui.selectable_label(self.settings.delay_secs == v, label).clicked() {
                    self.settings.delay_secs = v;
                    self.settings.save();
                }
            }
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
                        egui::Stroke::new(3.0, ui_kit::accent_fg(ui))
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
            for (i, kind) in CaptureKind::ALL.into_iter().enumerate() {
                let hk = self.shortcut_text(i).unwrap_or_default();
                let txt = RichText::new(format!("{}   {hk}", kind.label())).size(18.0);
                let btn = egui::Button::new(txt).min_size(Vec2::new(260.0, 44.0));
                let btn = if i == 0 { btn.fill(ORANGE) } else { btn };
                if ui.add(btn).clicked() {
                    self.start_capture(ctx, kind);
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
        let mut hotkeys_changed = false;
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
                egui::ComboBox::from_id_salt("format")
                    .selected_text(library::format_name(&self.settings.format))
                    .show_ui(ui, |ui| {
                        for (ext, name, _) in library::FORMATS {
                            changed |= ui.selectable_value(&mut self.settings.format, ext.to_string(), name).changed();
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("JPEG-/PDF-Qualität:");
                changed |= ui.add(egui::Slider::new(&mut self.settings.jpeg_quality, 50..=100).suffix(" %")).changed();
            });
            changed |= ui.checkbox(&mut self.settings.copy_after_capture, "Nach Aufnahme automatisch in Zwischenablage kopieren").changed();
            ui.separator();
            ui.label(RichText::new("Globale Tastenkürzel").strong());
            if ui.checkbox(&mut self.settings.hotkeys, "Aktiv (funktionieren auch, wenn SnapIt im Hintergrund ist)").changed() {
                changed = true;
                hotkeys_changed = true;
            }
            ui.add_enabled_ui(self.settings.hotkeys, |ui| {
                egui::Grid::new("shortcuts").num_columns(3).spacing([12.0, 8.0]).show(ui, |ui| {
                    for (i, kind) in CaptureKind::ALL.into_iter().enumerate() {
                        ui.label(kind.label());
                        ui.horizontal(|ui| {
                            if self.recording == Some(i) {
                                ui.colored_label(Color32::from_rgb(255, 150, 60), "Tastenkombination drücken …");
                            } else if let Some(sc) = &self.settings.shortcuts[i] {
                                hotkeys::caps_ui(ui, sc);
                                if self.hotkey_failed.contains(&kind) {
                                    ui.colored_label(Color32::from_rgb(220, 120, 0), "⚠")
                                        .on_hover_text("Konnte nicht registriert werden – evtl. von einer anderen App belegt.");
                                }
                            } else {
                                ui.weak("– keins –");
                            }
                        });
                        ui.horizontal(|ui| {
                            if self.recording == Some(i) {
                                if ui.button("Abbrechen").clicked() {
                                    self.recording = None;
                                    hotkeys_changed = true;
                                }
                            } else {
                                if ui.button("Ändern").clicked() {
                                    self.recording = Some(i);
                                    self.recording_hint = None;
                                    // Während der Aufnahme dürfen bestehende Kürzel nicht auslösen.
                                    if let Some(h) = &mut self.hotkeys {
                                        h.suspend();
                                    }
                                }
                                if self.settings.shortcuts[i].is_some() && ui.button("Entfernen").clicked() {
                                    self.settings.shortcuts[i] = None;
                                    changed = true;
                                    hotkeys_changed = true;
                                }
                            }
                        });
                        ui.end_row();
                    }
                });
                if let Some(hint) = &self.recording_hint {
                    ui.colored_label(Color32::from_rgb(220, 120, 0), hint);
                } else if self.recording.is_some() {
                    ui.weak("Mit ⌃ Ctrl, ⌥ Option oder ⌘ Cmd kombinieren (z. B. ⌃ + ⇧ + 1). Esc bricht ab.");
                }
                if ui.button("Standard wiederherstellen").clicked() {
                    self.settings.shortcuts = Shortcut::defaults();
                    self.recording = None;
                    changed = true;
                    hotkeys_changed = true;
                }
            });
            if self.hotkeys.is_none() {
                ui.colored_label(Color32::from_rgb(220, 120, 0), "Globale Kürzel werden auf diesem System nicht unterstützt.");
            }
            ui.separator();
            ui.label(RichText::new("Editor").strong());
            ui.label("V Auswahl · A Pfeil · L Linie · R Rechteck · E Ellipse · P Stift · H Marker · T Text · N Nummer · B Weichzeichnen · X Verpixeln · S Spotlight · F Füllen · C Zuschneiden");
            ui.label("⌘Z Rückgängig · ⌘⇧Z Wiederholen · ⌘D Duplizieren · ⌫ Löschen · Pfeiltasten Verschieben · ⌘ + Scrollen Zoom");
            ui.separator();
            ui.weak(format!("Bibliothek: {}", library::data_dir().join("library").display()));
        });
        if !open && self.recording.take().is_some() {
            hotkeys_changed = true;
        }
        if changed {
            self.settings.save();
        }
        if hotkeys_changed {
            self.apply_hotkeys();
        }
        self.show_settings = open;
    }

    fn apply_hotkeys(&mut self) {
        if let Some(h) = &mut self.hotkeys {
            self.hotkey_failed = h.apply(&self.settings.shortcuts, self.settings.hotkeys);
        }
        if let Some(t) = &self.tray {
            t.set_shortcuts(&self.shortcut_texts());
        }
    }

    fn shortcut_texts(&self) -> [Option<String>; 4] {
        std::array::from_fn(|i| self.shortcut_text(i))
    }

    fn shortcut_text(&self, i: usize) -> Option<String> {
        self.settings.hotkeys.then(|| self.settings.shortcuts[i].as_ref().map(Shortcut::compact)).flatten()
    }

    /// Nimmt während der Kürzel-Aufnahme die nächste Tastenkombination entgegen.
    fn record_shortcut(&mut self, ctx: &egui::Context) {
        let Some(i) = self.recording else { return };
        let pressed = ctx.input(|inp| {
            inp.events.iter().find_map(|e| match e {
                // Physische Taste: ⇧+1 soll „1“ ergeben, nicht „!“.
                egui::Event::Key { key, physical_key, pressed: true, modifiers, .. } => {
                    Some((physical_key.unwrap_or(*key), *modifiers))
                }
                _ => None,
            })
        });
        let Some((key, mods)) = pressed else { return };
        if key == egui::Key::Escape && !mods.any() {
            self.recording = None;
            self.apply_hotkeys();
            return;
        }
        let sc = Shortcut::new(mods, key);
        if !sc.is_valid() {
            self.recording_hint =
                Some(format!("„{}“ geht nicht – bitte mit ⌃ Ctrl, ⌥ Option oder ⌘ Cmd kombinieren.", sc.compact()));
            return;
        }
        // Doppelte Belegung auflösen
        for (j, other) in self.settings.shortcuts.iter_mut().enumerate() {
            if j != i && other.as_ref() == Some(&sc) {
                *other = None;
            }
        }
        self.settings.shortcuts[i] = Some(sc);
        self.recording = None;
        self.recording_hint = None;
        self.settings.save();
        self.apply_hotkeys();
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
                ui.label("   (steht SnapIt schon drin: mit − entfernen und neu hinzufügen)");
                ui.label("2. SnapIt beenden und neu starten");
                let (preflight, titles) = platform::screen_capture_diag();
                ui.add_space(4.0);
                ui.weak(format!(
                    "Status: Systemprüfung {} · Fenstertitel lesbar {} · App: {}",
                    if preflight { "✔" } else { "✘" },
                    if titles { "✔" } else { "✘" },
                    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()
                ));
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(8.0);
                let b = ui_kit::DIALOG_BUTTON;
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new("Erneut prüfen").min_size(b)).clicked() && (preflight || titles) {
                        self.show_permission = false;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui_kit::primary_button(ui, "Einstellungen öffnen", true).clicked() {
                            platform::open_privacy_settings();
                        }
                        if ui.add(egui::Button::new("SnapIt beenden").min_size(b)).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                        if ui.add(egui::Button::new("Schließen").min_size(b)).clicked() {
                            self.show_permission = false;
                        }
                    });
                });
            });
    }

    fn delete_dialog(&mut self, ctx: &egui::Context) {
        let Some(id) = self.confirm_delete.clone() else { return };
        let mut close = false;
        egui::Window::new("Aufnahme löschen?")
            .collapsible(false)
            .resizable(false)
            .min_width(380.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("„{id}“ wird endgültig aus der Bibliothek gelöscht."));
                ui.weak("Dies kann nicht rückgängig gemacht werden.");
                ui.add_space(6.0);
                let (delete, cancel) = ui_kit::dialog_buttons(ui, "Löschen", true);
                if delete {
                    library::delete(&id);
                    self.items.retain(|i| i.id != id);
                    if self.editor.as_ref().is_some_and(|e| e.id == id) {
                        self.editor = None;
                    }
                }
                close = delete || cancel;
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
                self.save_as(None)
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
        // Schließen-Knopf / ⌘W: nur ausblenden, außer „SnapIt beenden“ wurde gewählt.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.hide_to_background(ctx);
        }
        self.handle_tray(ctx);
        // Globale Hotkeys
        while let Ok(id) = self.hotkey_rx.try_recv() {
            if let Some(kind) = self.hotkeys.as_ref().and_then(|h| h.kind_for(id)) {
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
            // Geometrie in den ersten Frames nachziehen, bis das Fenster exakt den Monitor deckt.
            if self.overlay_fix_frames > 0 {
                self.overlay_fix_frames -= 1;
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(ov.grab.size));
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(ov.grab.origin));
            }
            match ov.ui(ctx) {
                OverlayResult::Pending => {}
                OverlayResult::Cancel => {
                    self.phase = Phase::Normal;
                    let show = !self.hidden_before_capture;
                    self.restore_window(ctx, frame, show);
                }
                OverlayResult::Done(img) => {
                    self.phase = Phase::Normal;
                    self.restore_window(ctx, frame, true);
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

        if self.recording.is_some() {
            // Tastendrücke gehören der Kürzel-Aufnahme, nicht dem Editor.
            self.record_shortcut(ctx);
        } else if self.pending_export.is_some() || self.confirm_delete.is_some() || self.show_background_hint {
            // Dialog offen: Tasten gehören dem Dialog.
        } else {
            self.global_shortcuts(ctx);
            if let Some(ed) = &mut self.editor {
                ed.shortcuts(ctx);
            }
        }
        self.flush_save(false);
        if self.editor.as_ref().is_some_and(|e| e.dirty) {
            ctx.request_repaint_after(Duration::from_millis(500));
        }

        let bar_frame = egui::Frame::new()
            .fill(ctx.style().visuals.window_fill)
            .inner_margin(egui::Margin::symmetric(4, 4))
            .stroke(ctx.style().visuals.widgets.noninteractive.bg_stroke);
        egui::TopBottomPanel::top("toolbar").exact_height(ui_kit::BAR_HEIGHT).frame(bar_frame).show(ctx, |ui| {
            self.toolbar(ctx, ui);
        });

        egui::TopBottomPanel::bottom("status").exact_height(28.0).show(ctx, |ui| {
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
            egui::SidePanel::right("props").default_width(300.0).min_width(260.0).show(ctx, |ui| {
                ui.add_space(8.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| ed.properties_ui(ui));
                });
            });
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| ed.canvas_ui(ui));
        } else {
            egui::CentralPanel::default().show(ctx, |ui| self.welcome(ctx, ui));
        }

        self.settings_window(ctx);
        self.export_dialog(ctx);
        self.background_hint_dialog(ctx);
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

/// Export, der noch auf die Qualitätswahl wartet.
struct PendingExport {
    img: std::sync::Arc<RgbaImage>,
    path: std::path::PathBuf,
    quality: u8,
    remember: bool,
    /// Zuletzt berechnete Größe: (Qualität, Bytes)
    estimate: Option<(u8, usize)>,
    requested: Option<u8>,
}

fn human_size(bytes: usize) -> String {
    match bytes {
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1 << 20) as f64),
        b => format!("{} KB", (b + 1023) / 1024),
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
