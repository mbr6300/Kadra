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
use crate::menu::{self, Action, AppMenu};
use crate::tray::Tray;

enum Phase {
    Normal,
    /// Fenster ist versteckt, Aufnahme läuft im Hintergrund.
    Capturing(Receiver<Result<Grab, String>>),
    Overlay(Box<Overlay>),
}

pub struct KadraApp {
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
    /// Zuletzt gelöschte Aufnahme (Position, Eintrag, Zeitpunkt) für „Rückgängig“.
    undo_delete: Option<(usize, Item, Instant)>,
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
    /// macOS-Menüleiste (muss am Leben bleiben).
    app_menu: Option<AppMenu>,
    /// Aktionen aus Menüleiste und Menüleisten-Symbol.
    menu_rx: Receiver<Action>,
    /// Fenster geschlossen, App läuft nur in der Menüleiste.
    hidden: bool,
    hidden_before_capture: bool,
    /// „Kadra beenden“ gewählt – Schließen nicht mehr abfangen.
    quitting: bool,
    show_background_hint: bool,
}

const CAPTURE_ICONS: [&str; 4] = ["⛶", "🗔", "🖵", "🖥"];

/// So lange wird nach dem Löschen „Rückgängig“ angeboten.
const UNDO_DELETE_FOR: Duration = Duration::from_secs(8);

/// Runder Löschen-Knopf (dunkler Kreis mit weißem ×, rot beim Darüberfahren).
fn paint_delete_badge(p: &egui::Painter, c: egui::Pos2, hot: bool) {
    let r = 9.0;
    let bg = if hot { Color32::from_rgb(214, 58, 48) } else { Color32::from_black_alpha(190) };
    p.circle_filled(c + Vec2::new(0.0, 1.0), r + 0.5, Color32::from_black_alpha(60));
    p.circle_filled(c, r, bg);
    p.circle_stroke(c, r, egui::Stroke::new(1.2, Color32::from_white_alpha(200)));
    let (a, st) = (3.6, egui::Stroke::new(1.8, Color32::WHITE));
    p.line_segment([c + Vec2::new(-a, -a), c + Vec2::new(a, a)], st);
    p.line_segment([c + Vec2::new(-a, a), c + Vec2::new(a, -a)], st);
}

/// Mindestabstand zwischen den drei Gruppen der oberen Leiste.
const TOOLBAR_GAP: f32 = 16.0;
const RIGHT_SPACING: f32 = 4.0;

/// Ergebnis der Platzberechnung für die obere Leiste.
struct ToolbarLayout {
    /// Linke/rechte Gruppe nur als Symbol.
    compact_sides: bool,
    /// Werkzeuge nur als Symbol.
    compact_tools: bool,
    /// Anzahl Werkzeuggruppen mit eigenem Button (Rest im „Mehr“-Menü).
    visible_groups: usize,
    /// Breite der rechten Gruppe.
    right_w: f32,
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

impl KadraApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = Settings::load();
        // Gelöschte Aufnahmen der letzten Sitzung endgültig entfernen.
        library::empty_trash();
        let (thumb_tx, thumb_rx) = channel();
        let (hk_tx, hotkey_rx) = channel();
        let (estimate_tx, estimate_rx) = channel();
        let (menu_tx, menu_rx) = channel();

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
            undo_delete: None,
            overlay_fix_frames: 0,
            hotkey_failed,
            recording: None,
            recording_hint: None,
            pending_export: None,
            estimate_tx,
            estimate_rx,
            tray: None,
            app_menu: None,
            menu_rx,
            hidden: false,
            hidden_before_capture: false,
            quitting: false,
            show_background_hint: false,
        };
        menu::install_handler(&cc.egui_ctx, menu_tx);
        let shortcuts = app.active_shortcuts();
        app.tray = Tray::new(&shortcuts);
        app.app_menu = AppMenu::new(&shortcuts, menu::about_icon());
        // `kadra bild.png` öffnet eine Datei direkt im Editor.
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

    /// Fenster nach der Aufnahme zurücksetzen. `show = false` lässt Kadra im Hintergrund
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

    /// Hauptfenster zeigen und Kadra wieder ins Dock holen.
    fn show_window(&mut self, ctx: &egui::Context) {
        self.hidden = false;
        platform::set_dock_visible(true);
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
    }

    /// Fenster schließen = nur ausblenden; Kadra läuft in der Menüleiste weiter.
    fn hide_to_background(&mut self, ctx: &egui::Context) {
        if !self.settings.background_hint_shown && self.tray.is_some() {
            // Beim ersten Mal erklären, wo Kadra bleibt (Dialog blendet danach aus).
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
        egui::Window::new("Kadra läuft weiter")
            .collapsible(false)
            .resizable(false)
            .min_width(420.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Das Fenster wird geschlossen, Kadra bleibt aber in der Menüleiste oben rechts aktiv.");
                ui.label("Über das Symbol ⛶ erreichst du die Aufnahmen, öffnest Kadra wieder oder beendest es ganz.");
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

    /// Aktionen aus Menüleiste und Menüleisten-Symbol ausführen.
    fn handle_menu_actions(&mut self, ctx: &egui::Context) {
        while let Ok(action) = self.menu_rx.try_recv() {
            // Menü-Kürzel fangen die Taste ab, bevor egui sie sieht – in Textfeldern
            // die Bearbeitungsbefehle daher als egui-Ereignis an das Textfeld weitergeben.
            if ctx.wants_keyboard_input() && forward_to_text_field(ctx, action) {
                continue;
            }
            match action {
                Action::Capture(kind) => self.start_capture(ctx, kind),
                Action::Show => self.show_window(ctx),
                Action::CopyLast => {
                    if self.editor.is_none() {
                        if let Some(id) = self.items.first().map(|i| i.id.clone()) {
                            self.open_item(&id);
                        }
                    }
                    self.copy();
                }
                Action::Settings => {
                    self.show_settings = true;
                    self.show_window(ctx);
                }
                Action::HideWindow => self.hide_to_background(ctx),
                Action::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                Action::Open => self.open_file(),
                Action::QuickSave => self.quick_save(),
                Action::SaveAs => self.save_as(None),
                Action::Export(ext) => self.save_as(Some(ext)),
                Action::RevealFolder => {
                    let _ = std::fs::create_dir_all(&self.settings.save_dir);
                    open_url(&self.settings.save_dir.to_string_lossy());
                }
                Action::Copy => self.copy(),
                Action::Paste => self.paste(),
                Action::Cut | Action::SelectAll => {}
                Action::Docs => open_url(&format!("{}/blob/master/docs/DOKUMENTATION.md", menu::REPO_URL)),
                Action::Website => open_url(menu::REPO_URL),
                Action::Issues => open_url(&format!("{}/issues", menu::REPO_URL)),
                _ => {
                    let ppp = ctx.pixels_per_point();
                    if let Some(ed) = &mut self.editor {
                        match action {
                            Action::Undo => ed.undo(),
                            Action::Redo => ed.redo(),
                            Action::Duplicate => ed.duplicate_selected(),
                            Action::Delete => ed.delete_selected(),
                            Action::Deselect => ed.selected = None,
                            Action::RotateLeft => ed.rotate(false),
                            Action::RotateRight => ed.rotate(true),
                            Action::FlipH => ed.flip(true),
                            Action::FlipV => ed.flip(false),
                            Action::Tool(t) => ed.set_tool(t),
                            Action::ZoomFit => ed.zoom_fit(),
                            Action::ZoomActual => ed.zoom_actual(ppp),
                            Action::ZoomIn => ed.zoom_by(1.25),
                            Action::ZoomOut => ed.zoom_by(0.8),
                            _ => {}
                        }
                    }
                }
            }
        }
        if let Some(m) = &mut self.app_menu {
            let (u, r) = self.editor.as_ref().map(|e| (e.can_undo(), e.can_redo())).unwrap_or_default();
            m.update(self.editor.is_some(), u, r);
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
        format!("Kadra {}.{ext}", chrono::Local::now().format("%Y-%m-%d um %H.%M.%S"))
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
        let layout = self.toolbar_layout(ui, full.width());
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(4.0);
            ui_kit::set_compact(ctx, layout.compact_sides);
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

            // Werkzeuge mittig, aber nie in die rechte Gruppe hinein
            let tools_w = editor::tool_bar_width(ui, layout.visible_groups, layout.compact_tools);
            let left_end = ui.cursor().min.x + TOOLBAR_GAP;
            let right_start = full.max.x - layout.right_w - TOOLBAR_GAP;
            let start = (full.center().x - tools_w / 2.0).min(right_start - tools_w).max(left_end);
            ui.add_space(start - ui.cursor().min.x);
            ui_kit::set_compact(ctx, layout.compact_tools);
            editor::tool_bar(ui, self.editor.as_mut(), layout.visible_groups);
            ui_kit::set_compact(ctx, layout.compact_sides);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = RIGHT_SPACING;
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
        ui_kit::set_compact(ctx, false);
    }

    /// Wählt die Darstellung der Leiste so, dass sich nichts überschneidet:
    /// 1. alles beschriftet, 2. Werkzeuge nur Symbol, 3. alles nur Symbol,
    /// 4. zusätzlich Werkzeuggruppen ins „Mehr“-Menü verschieben.
    fn toolbar_layout(&self, ui: &egui::Ui, width: f32) -> ToolbarLayout {
        use ui_kit::bar_button_width as bw;
        let left_w = |c: bool| {
            let items = [("Aufnahme", true), ("Öffnen", false), ("Einfügen", false), ("Zurück", false), ("Vor", false)];
            4.0 + 13.0 + items.iter().map(|(l, m)| bw(ui, l, *m, c) + 2.0).sum::<f32>() + 2.0
        };
        let right_w = |c: bool| {
            let items = [("Einstellungen", false), ("Speichern", true), ("Kopieren", false)];
            4.0 + items.iter().map(|(l, m)| bw(ui, l, *m, c) + RIGHT_SPACING).sum::<f32>()
        };
        let all = editor::tool_group_count();
        let fits = |sides: bool, tools: bool, n: usize| {
            left_w(sides) + right_w(sides) + editor::tool_bar_width(ui, n, tools) + 2.0 * TOOLBAR_GAP <= width
        };
        for (sides, tools) in [(false, false), (false, true), (true, true)] {
            if fits(sides, tools, all) {
                return ToolbarLayout { compact_sides: sides, compact_tools: tools, visible_groups: all, right_w: right_w(sides) };
            }
        }
        let visible = (1..all).rev().find(|n| fits(true, true, *n)).unwrap_or(1);
        ToolbarLayout { compact_sides: true, compact_tools: true, visible_groups: visible, right_w: right_w(true) }
    }

    fn capture_menu(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.set_min_width(280.0);
        for (i, kind) in CaptureKind::ALL.into_iter().enumerate() {
            let btn = egui::Button::new(format!("{}  {}", CAPTURE_ICONS[i], kind.label()))
                .min_size(Vec2::new(ui.available_width(), 32.0));
            let resp = ui.add(btn);
            // Tastenkappen rechtsbündig über den Button malen
            if let Some(sc) = self.active_shortcut(i) {
                let h = 22.0;
                let w = hotkeys::keycaps_width(ui, sc, h);
                hotkeys::paint_keycaps(ui, resp.rect.right_center() - Vec2::new(10.0 + w, 0.0), sc, h, false);
            }
            if resp.clicked() {
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
        let mut delete = None;
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
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
                    // Runder Löschen-Knopf an der rechten oberen Ecke (beim Darüberfahren)
                    let del_rect = egui::Rect::from_center_size(rect.right_top() + Vec2::new(-11.0, 11.0), Vec2::splat(20.0));
                    let del = ui.interact(del_rect, ui.id().with(("del", &it.id)), egui::Sense::click());
                    let hovered = resp.hovered() || del.hovered();
                    let stroke = if selected {
                        egui::Stroke::new(3.0, ui_kit::accent_fg(ui))
                    } else if hovered {
                        egui::Stroke::new(1.5, Color32::GRAY)
                    } else {
                        egui::Stroke::NONE
                    };
                    ui.painter().rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Outside);
                    if hovered {
                        paint_delete_badge(ui.painter(), del_rect.center(), del.hovered());
                    }
                    if del.on_hover_text("Löschen").clicked() {
                        delete = Some(it.id.clone());
                    } else if resp.clicked() {
                        open = Some(it.id.clone());
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Öffnen").clicked() {
                            open = Some(it.id.clone());
                            ui.close();
                        }
                        if ui.button("🗑 Löschen").clicked() {
                            delete = Some(it.id.clone());
                            ui.close();
                        }
                    })
                    .map(|_| ());
                    resp.on_hover_text(&it.id);
                }
            });
        });
        if let Some(id) = delete {
            self.delete_item(&id);
        } else if let Some(id) = open {
            self.open_item(&id);
        }
    }

    /// Aufnahme sofort löschen; ein paar Sekunden lang per „Rückgängig“ zurückholbar.
    fn delete_item(&mut self, id: &str) {
        let Some(index) = self.items.iter().position(|i| i.id == id) else { return };
        if self.editor.as_ref().is_some_and(|e| e.id == id) {
            // Ohne Speichern schließen – die Aufnahme wird ja gelöscht.
            self.editor = None;
        }
        let item = self.items.remove(index);
        library::trash(id);
        self.undo_delete = Some((index, item, Instant::now()));
        self.status = None;
    }

    fn undo_last_delete(&mut self) {
        if let Some((index, item, _)) = self.undo_delete.take() {
            library::restore(&item.id);
            let index = index.min(self.items.len());
            self.items.insert(index, item);
        }
    }

    /// Großer Aufnahme-Button der Startseite: Symbol + Name links, Tastenkappen rechts.
    fn capture_button(&self, ui: &mut egui::Ui, i: usize, kind: CaptureKind) -> egui::Response {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(340.0, 50.0), egui::Sense::click());
        let primary = i == 0;
        let v = ui.visuals();
        let bg = match (primary, resp.hovered()) {
            (true, false) => ORANGE,
            (true, true) => ui_kit::ORANGE_HOVER,
            (false, false) => v.widgets.inactive.weak_bg_fill,
            (false, true) => v.widgets.hovered.weak_bg_fill,
        };
        let fg = if primary { Color32::WHITE } else { v.strong_text_color() };
        let p = ui.painter();
        p.rect_filled(rect, 10.0, bg);
        let icon = CAPTURE_ICONS[i];
        p.text(rect.left_center() + Vec2::new(22.0, 0.0), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(20.0), fg);
        p.text(rect.left_center() + Vec2::new(44.0, 0.0), egui::Align2::LEFT_CENTER, kind.label(), egui::FontId::proportional(17.0), fg);
        if let Some(sc) = self.active_shortcut(i) {
            let h = 26.0;
            let w = hotkeys::keycaps_width(ui, sc, h);
            hotkeys::paint_keycaps(ui, rect.right_center() - Vec2::new(14.0 + w, 0.0), sc, h, primary);
            resp.on_hover_text(format!("{} aufnehmen ({})", kind.label(), sc.words()))
        } else {
            resp
        }
    }

    fn active_shortcut(&self, i: usize) -> Option<&Shortcut> {
        self.settings.hotkeys.then(|| self.settings.shortcuts[i].as_ref()).flatten()
    }

    fn welcome(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.2);
            ui.heading(RichText::new("Kadra").size(40.0).strong());
            ui.label("Bildschirmfotos aufnehmen, beschriften und teilen.");
            ui.add_space(24.0);
            for (i, kind) in CaptureKind::ALL.into_iter().enumerate() {
                if self.capture_button(ui, i, kind).clicked() {
                    self.start_capture(ctx, kind);
                }
            }
            ui.add_space(16.0);
            ui.weak("Tipp: Im Bereichsmodus ein Fenster anklicken, um nur dieses aufzunehmen.");
            ui.weak("Bilder können auch per Drag & Drop geöffnet werden.");
        });
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        let mut changed = false;
        let mut hotkeys_changed = false;
        // Eigene Kopfzeile statt egui-Titelleiste: größerer, sauber platzierter Schließen-Knopf.
        egui::Window::new("Einstellungen")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if ui_kit::window_header(ui, "Einstellungen") {
                    open = false;
                }
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
                if ui.checkbox(&mut self.settings.hotkeys, "Aktiv (funktionieren auch, wenn Kadra im Hintergrund ist)").changed() {
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
        // Esc schließt (während einer Kürzel-Aufnahme bricht Esc stattdessen nur diese ab)
        if self.recording.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
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
        let active = self.active_shortcuts();
        if let Some(t) = &self.tray {
            t.set_shortcuts(&active);
        }
        if let Some(m) = &self.app_menu {
            m.set_shortcuts(&active);
        }
    }

    /// Aktive globale Kürzel (leer, wenn ausgeschaltet) für die Anzeige in Menüs.
    fn active_shortcuts(&self) -> [Option<Shortcut>; 4] {
        std::array::from_fn(|i| self.active_shortcut(i).cloned())
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
            // Esc verbrauchen, damit es nicht zusätzlich das Einstellungsfenster schließt.
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
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
                ui.label("Kadra braucht die Berechtigung „Bildschirm- und Systemaudioaufnahme“.");
                ui.label("Ohne sie liefert macOS nur das Hintergrundbild – ohne Fenster.");
                ui.add_space(6.0);
                ui.label("1. Einstellungen öffnen und Kadra aktivieren");
                ui.label("   (steht Kadra schon drin: mit − entfernen und neu hinzufügen)");
                ui.label("2. Kadra beenden und neu starten");
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
                        if ui.add(egui::Button::new("Kadra beenden").min_size(b)).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                        if ui.add(egui::Button::new("Schließen").min_size(b)).clicked() {
                            self.show_permission = false;
                        }
                    });
                });
            });
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

impl eframe::App for KadraApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Schließen-Knopf / ⌘W: nur ausblenden, außer „Kadra beenden“ wurde gewählt.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.hide_to_background(ctx);
        }
        self.handle_menu_actions(ctx);
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
        } else if self.pending_export.is_some() || self.show_background_hint {
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

        // „Rückgängig“ nach dem Löschen nur kurz anbieten
        if self.undo_delete.as_ref().is_some_and(|(_, _, t)| t.elapsed() >= UNDO_DELETE_FOR) {
            self.undo_delete = None;
        }
        egui::TopBottomPanel::bottom("status").exact_height(32.0).show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                if self.undo_delete.is_some() {
                    ui.label("Aufnahme gelöscht");
                    if ui.add(egui::Button::new("Rückgängig").min_size(Vec2::new(0.0, 24.0))).clicked() {
                        self.undo_last_delete();
                    }
                    ctx.request_repaint_after(Duration::from_millis(500));
                } else if let Some((msg, t)) = &self.status {
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

fn open_url(target: &str) {
    let _ = std::process::Command::new("open").arg(target).spawn();
}

/// Bearbeitungsbefehl aus dem Menü an das fokussierte Textfeld weitergeben.
/// Gibt `true` zurück, wenn der Befehl damit erledigt ist.
fn forward_to_text_field(ctx: &egui::Context, action: Action) -> bool {
    use egui::{Event, Key, Modifiers};
    let key = |key: Key, shift: bool| Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: if shift { Modifiers::COMMAND | Modifiers::SHIFT } else { Modifiers::COMMAND },
    };
    let event = match action {
        Action::Copy => Event::Copy,
        Action::Cut => Event::Cut,
        Action::Paste => {
            let text = arboard::Clipboard::new().and_then(|mut c| c.get_text()).unwrap_or_default();
            if text.is_empty() {
                return true;
            }
            Event::Paste(text.replace("\r\n", "\n"))
        }
        Action::Undo => key(Key::Z, false),
        Action::Redo => key(Key::Z, true),
        Action::SelectAll => key(Key::A, false),
        _ => return false,
    };
    ctx.input_mut(|i| i.events.push(event));
    true
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
