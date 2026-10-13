//! macOS-Menüleiste (oben, wenn Kadra aktiv ist) und gemeinsame Aktionen für
//! Menüleiste und Menüleisten-Symbol.

use std::sync::mpsc::Sender;

use tray_icon::menu::{
    AboutMetadata, Icon, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};

use crate::capture::CaptureKind;
use crate::editor::Tool;
use crate::hotkeys::Shortcut;
use crate::library::FORMATS;

pub const REPO_URL: &str = "https://github.com/mbr6300/Kadra";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    // Aufnahme & App
    Capture(CaptureKind),
    Show,
    CopyLast,
    Settings,
    HideWindow,
    Quit,
    // Ablage
    Open,
    QuickSave,
    SaveAs,
    Export(&'static str),
    RevealFolder,
    // Bearbeiten
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Duplicate,
    Delete,
    Deselect,
    // Bild
    RotateLeft,
    RotateRight,
    FlipH,
    FlipV,
    // Werkzeuge
    Tool(Tool),
    // Darstellung
    ZoomFit,
    ZoomActual,
    ZoomIn,
    ZoomOut,
    // Hilfe
    Docs,
    Website,
    Issues,
}

impl Action {
    pub fn id(self) -> String {
        match self {
            Action::Capture(k) => format!("capture-{}", k as u8),
            Action::Export(ext) => format!("export-{ext}"),
            Action::Tool(t) => format!("tool-{t:?}"),
            other => format!("{other:?}"),
        }
    }

    fn all() -> Vec<Action> {
        let mut v: Vec<Action> = CaptureKind::ALL.map(Action::Capture).to_vec();
        v.extend(FORMATS.iter().map(|f| Action::Export(f.0)));
        v.extend(Tool::ALL.map(Action::Tool));
        v.extend([
            Action::Show,
            Action::CopyLast,
            Action::Settings,
            Action::HideWindow,
            Action::Quit,
            Action::Open,
            Action::QuickSave,
            Action::SaveAs,
            Action::RevealFolder,
            Action::Undo,
            Action::Redo,
            Action::Cut,
            Action::Copy,
            Action::Paste,
            Action::SelectAll,
            Action::Duplicate,
            Action::Delete,
            Action::Deselect,
            Action::RotateLeft,
            Action::RotateRight,
            Action::FlipH,
            Action::FlipV,
            Action::ZoomFit,
            Action::ZoomActual,
            Action::ZoomIn,
            Action::ZoomOut,
            Action::Docs,
            Action::Website,
            Action::Issues,
        ]);
        v
    }

    pub fn from_id(id: &str) -> Option<Action> {
        Action::all().into_iter().find(|a| a.id() == id)
    }

    /// Braucht ein geöffnetes Bild.
    fn needs_editor(self) -> bool {
        matches!(
            self,
            Action::QuickSave
                | Action::SaveAs
                | Action::Export(_)
                | Action::Duplicate
                | Action::Delete
                | Action::Deselect
                | Action::RotateLeft
                | Action::RotateRight
                | Action::FlipH
                | Action::FlipV
                | Action::Tool(_)
                | Action::ZoomFit
                | Action::ZoomActual
                | Action::ZoomIn
                | Action::ZoomOut
        )
    }
}

/// Leitet Klicks aus Menüleiste und Menüleisten-Symbol an die App weiter.
/// Der Handler lässt sich nur einmal setzen – daher hier zentral für beide Menüs.
pub fn install_handler(ctx: &egui::Context, tx: Sender<Action>) {
    let ctx = ctx.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        if let Some(action) = Action::from_id(e.id.as_ref()) {
            let _ = tx.send(action);
            ctx.request_repaint();
        }
    }));
}

/// Menüeintrag mit Aktion und optionalem Tastenkürzel (z. B. "CMD+SHIFT+S").
pub fn item(action: Action, text: &str, accel: Option<&str>) -> MenuItem {
    MenuItem::with_id(action.id(), text, true, accel.and_then(|a| a.parse().ok()))
}

/// Tastenkürzel einer Aufnahme als Menü-Kürzel.
pub fn shortcut_accel(sc: Option<&Shortcut>) -> Option<tray_icon::menu::accelerator::Accelerator> {
    let sc = sc?;
    let mut s = String::new();
    for (on, m) in [(sc.ctrl, "CTRL+"), (sc.alt, "ALT+"), (sc.shift, "SHIFT+"), (sc.cmd, "CMD+")] {
        if on {
            s += m;
        }
    }
    s += &sc.key;
    s.parse().ok()
}

pub fn capture_label(kind: CaptureKind) -> String {
    format!("{} aufnehmen", kind.label())
}

/// Die Menüleiste der App.
pub struct AppMenu {
    _menu: Menu,
    capture_items: Vec<(CaptureKind, MenuItem)>,
    editor_items: Vec<(Action, MenuItem)>,
    undo: MenuItem,
    redo: MenuItem,
    last_state: Option<(bool, bool, bool)>,
}

impl AppMenu {
    /// Baut die Menüleiste und setzt sie als Hauptmenü der App (nur macOS).
    pub fn new(shortcuts: &[Option<Shortcut>; 4], about_icon: Option<Icon>) -> Option<Self> {
        let mut editor_items = vec![];
        let mut tracked = |action: Action, text: &str, accel: Option<&str>| {
            let it = item(action, text, accel);
            if action.needs_editor() {
                editor_items.push((action, it.clone()));
            }
            it
        };

        // Kadra
        let about = PredefinedMenuItem::about(Some("Über Kadra"), Some(about_metadata(about_icon)));
        let app = Submenu::with_items(
            "Kadra",
            true,
            &[
                &about,
                &PredefinedMenuItem::separator(),
                &tracked(Action::Settings, "Einstellungen …", Some("CMD+COMMA")),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(Some("Dienste")),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(Some("Kadra ausblenden")),
                &PredefinedMenuItem::hide_others(Some("Andere ausblenden")),
                &PredefinedMenuItem::show_all(Some("Alle einblenden")),
                &PredefinedMenuItem::separator(),
                &tracked(Action::Quit, "Kadra beenden", Some("CMD+Q")),
            ],
        )
        .ok()?;

        // Ablage
        let capture_items: Vec<(CaptureKind, MenuItem)> = CaptureKind::ALL
            .into_iter()
            .zip(shortcuts)
            .map(|(k, sc)| {
                let it = MenuItem::with_id(Action::Capture(k).id(), capture_label(k), true, shortcut_accel(sc.as_ref()));
                (k, it)
            })
            .collect();
        let capture_refs: Vec<&dyn IsMenuItem> = capture_items.iter().map(|(_, i)| i as &dyn IsMenuItem).collect();
        let capture = Submenu::with_items("Neue Aufnahme", true, &capture_refs).ok()?;
        let export_items: Vec<MenuItem> =
            FORMATS.iter().map(|f| tracked(Action::Export(f.0), &format!("{} (.{})", f.1, f.0), None)).collect();
        let export_refs: Vec<&dyn IsMenuItem> = export_items.iter().map(|i| i as &dyn IsMenuItem).collect();
        let export = Submenu::with_items("Exportieren als", true, &export_refs).ok()?;
        let file = Submenu::with_items(
            "Ablage",
            true,
            &[
                &capture,
                &PredefinedMenuItem::separator(),
                &tracked(Action::Open, "Öffnen …", Some("CMD+O")),
                &PredefinedMenuItem::separator(),
                &tracked(Action::QuickSave, "Schnell speichern", Some("CMD+S")),
                &tracked(Action::SaveAs, "Speichern unter …", Some("CMD+SHIFT+S")),
                &export,
                &PredefinedMenuItem::separator(),
                &tracked(Action::RevealFolder, "Zielordner im Finder zeigen", None),
                &PredefinedMenuItem::separator(),
                &tracked(Action::HideWindow, "Fenster schließen", Some("CMD+W")),
            ],
        )
        .ok()?;

        // Bearbeiten
        let undo = item(Action::Undo, "Rückgängig", Some("CMD+Z"));
        let redo = item(Action::Redo, "Wiederholen", Some("CMD+SHIFT+Z"));
        let edit = Submenu::with_items(
            "Bearbeiten",
            true,
            &[
                &undo,
                &redo,
                &PredefinedMenuItem::separator(),
                &tracked(Action::Cut, "Ausschneiden", Some("CMD+X")),
                &tracked(Action::Copy, "Kopieren", Some("CMD+C")),
                &tracked(Action::Paste, "Einfügen", Some("CMD+V")),
                &tracked(Action::SelectAll, "Alles auswählen", Some("CMD+A")),
                &PredefinedMenuItem::separator(),
                &tracked(Action::Duplicate, "Duplizieren", Some("CMD+D")),
                &tracked(Action::Delete, "Objekt löschen", None),
                &tracked(Action::Deselect, "Auswahl aufheben", None),
            ],
        )
        .ok()?;

        // Bild
        let image = Submenu::with_items(
            "Bild",
            true,
            &[
                &tracked(Action::RotateLeft, "Links drehen", None),
                &tracked(Action::RotateRight, "Rechts drehen", None),
                &PredefinedMenuItem::separator(),
                &tracked(Action::FlipH, "Horizontal spiegeln", None),
                &tracked(Action::FlipV, "Vertikal spiegeln", None),
                &PredefinedMenuItem::separator(),
                &tracked(Action::Tool(Tool::Crop), "Zuschneiden …", None),
            ],
        )
        .ok()?;

        // Werkzeuge – Taste im Titel statt als Menü-Kürzel (sonst würde sie das Tippen von Text abfangen)
        let tool_items: Vec<MenuItem> = Tool::ALL
            .iter()
            .filter(|t| **t != Tool::Crop)
            .map(|t| tracked(Action::Tool(*t), &format!("{}   ({})", t.name(), t.key().name()), None))
            .collect();
        let tool_refs: Vec<&dyn IsMenuItem> = tool_items.iter().map(|i| i as &dyn IsMenuItem).collect();
        let tools = Submenu::with_items("Werkzeuge", true, &tool_refs).ok()?;

        // Darstellung
        let view = Submenu::with_items(
            "Darstellung",
            true,
            &[
                &tracked(Action::ZoomFit, "Ins Fenster einpassen", Some("CMD+0")),
                &tracked(Action::ZoomActual, "Originalgröße (100 %)", Some("CMD+1")),
                &tracked(Action::ZoomIn, "Vergrößern", Some("CMD+EQUAL")),
                &tracked(Action::ZoomOut, "Verkleinern", Some("CMD+MINUS")),
            ],
        )
        .ok()?;

        // Fenster
        let window = Submenu::with_items(
            "Fenster",
            true,
            &[
                &PredefinedMenuItem::minimize(Some("Im Dock ablegen")),
                &PredefinedMenuItem::maximize(Some("Zoomen")),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::bring_all_to_front(Some("Alle nach vorne bringen")),
            ],
        )
        .ok()?;

        // Hilfe
        let help = Submenu::with_items(
            "Hilfe",
            true,
            &[
                &item(Action::Docs, "Kadra-Dokumentation", None),
                &item(Action::Settings, "Tastenkürzel …", None),
                &PredefinedMenuItem::separator(),
                &item(Action::Website, "Kadra auf GitHub", None),
                &item(Action::Issues, "Problem melden …", None),
            ],
        )
        .ok()?;

        let menu = Menu::with_items(&[&app, &file, &edit, &image, &tools, &view, &window, &help]).ok()?;
        #[cfg(target_os = "macos")]
        {
            menu.init_for_nsapp();
            window.set_as_windows_menu_for_nsapp();
            help.set_as_help_menu_for_nsapp();
        }
        Some(Self { _menu: menu, capture_items, editor_items, undo, redo, last_state: None })
    }

    /// Aktiviert/deaktiviert Einträge je nach Zustand (nur bei Änderung).
    pub fn update(&mut self, has_editor: bool, can_undo: bool, can_redo: bool) {
        let state = (has_editor, can_undo, can_redo);
        if self.last_state == Some(state) {
            return;
        }
        self.last_state = Some(state);
        for (_, it) in &self.editor_items {
            it.set_enabled(has_editor);
        }
        // In Textfeldern wirken Rückgängig/Wiederholen auch ohne Bild – daher nur bei offenem Bild sperren
        self.undo.set_enabled(!has_editor || can_undo);
        self.redo.set_enabled(!has_editor || can_redo);
    }

    /// Kürzel der Aufnahme-Einträge nach Änderung in den Einstellungen anpassen.
    pub fn set_shortcuts(&self, shortcuts: &[Option<Shortcut>; 4]) {
        for ((_, it), sc) in self.capture_items.iter().zip(shortcuts) {
            let _ = it.set_accelerator(shortcut_accel(sc.as_ref()));
        }
    }
}

fn about_metadata(icon: Option<Icon>) -> AboutMetadata {
    let arch = if cfg!(target_arch = "aarch64") { "Apple Silicon" } else { "Intel" };
    AboutMetadata {
        name: Some("Kadra".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        copyright: Some("© 2026 mbr6300 · GNU GPL v3.0 oder neuer".into()),
        credits: Some(format!(
            "Screenshot-Werkzeug für macOS: Bildschirmfotos aufnehmen, beschriften und exportieren.\n\n\
             Aufnahme von Bereich, Fenster oder Bildschirm · Editor mit Pfeilen, Text, Formen, \
             Nummern, Füllen, Weichzeichnen und Zuschneiden · Export als PNG, JPEG, TIFF, WebP, GIF, BMP, PDF.\n\n\
             Quellcode: {REPO_URL}\n\
             Lizenz: GNU General Public License v3.0 oder neuer\n\n\
             Erstellt mit Rust, egui/eframe, xcap, tiny-skia, image, global-hotkey, tray-icon und muda.\n\
             Build: macOS · {arch}"
        )),
        icon,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_ids_roundtrip_and_are_unique() {
        let all = Action::all();
        for a in &all {
            assert_eq!(Action::from_id(&a.id()), Some(*a), "{a:?}");
        }
        let mut ids: Vec<String> = all.iter().map(|a| a.id()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "doppelte Menü-IDs");
    }

    #[test]
    fn accelerators_parse() {
        for s in ["CMD+COMMA", "CMD+Q", "CMD+O", "CMD+S", "CMD+SHIFT+S", "CMD+W", "CMD+Z", "CMD+SHIFT+Z", "CMD+X", "CMD+C", "CMD+V", "CMD+A", "CMD+D", "CMD+0", "CMD+1", "CMD+EQUAL", "CMD+MINUS"] {
            assert!(s.parse::<tray_icon::menu::accelerator::Accelerator>().is_ok(), "{s}");
        }
        for sc in Shortcut::defaults() {
            assert!(shortcut_accel(sc.as_ref()).is_some(), "Standard-Kürzel {sc:?}");
        }
    }
}

/// Icon für das Info-Fenster.
pub fn about_icon() -> Option<Icon> {
    let size = 256;
    Icon::from_rgba(crate::icon::rgba(size), size, size).ok()
}
