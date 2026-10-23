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

const MAC: bool = cfg!(target_os = "macos");

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
    CopyText,
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
            Action::CopyText,
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
                | Action::CopyText
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

        // Kadra (macOS) – unter Windows wandern Einstellungen/Beenden ins Datei-Menü, „Über“ in die Hilfe.
        let about = PredefinedMenuItem::about(Some("Über Kadra"), Some(about_metadata(about_icon)));
        let settings = tracked(Action::Settings, "Einstellungen …", Some("CMDORCTRL+COMMA"));
        let quit = tracked(Action::Quit, "Kadra beenden", Some("CMDORCTRL+Q"));
        let app = Submenu::with_items(
            "Kadra",
            true,
            &[
                &about,
                &PredefinedMenuItem::separator(),
                &settings,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(Some("Dienste")),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(Some("Kadra ausblenden")),
                &PredefinedMenuItem::hide_others(Some("Andere ausblenden")),
                &PredefinedMenuItem::show_all(Some("Alle einblenden")),
                &PredefinedMenuItem::separator(),
                &quit,
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
        let open = tracked(Action::Open, "Öffnen …", Some("CMDORCTRL+O"));
        let quick = tracked(Action::QuickSave, "Schnell speichern", Some("CMDORCTRL+S"));
        let save_as = tracked(Action::SaveAs, "Speichern unter …", Some("CMDORCTRL+SHIFT+S"));
        let reveal_text = if MAC { "Zielordner im Finder zeigen" } else { "Zielordner im Explorer zeigen" };
        let reveal = tracked(Action::RevealFolder, reveal_text, None);
        let close = tracked(Action::HideWindow, "Fenster schließen", Some("CMDORCTRL+W"));
        let sep = PredefinedMenuItem::separator();
        let mut file_items: Vec<&dyn IsMenuItem> =
            vec![&capture, &sep, &open, &sep, &quick, &save_as, &export, &sep, &reveal, &sep, &close];
        if !MAC {
            file_items.extend([&sep as &dyn IsMenuItem, &settings, &sep, &quit]);
        }
        let file = Submenu::with_items(if MAC { "Ablage" } else { "Datei" }, true, &file_items).ok()?;

        // Bearbeiten
        let undo = item(Action::Undo, "Rückgängig", Some("CMDORCTRL+Z"));
        let redo = item(Action::Redo, "Wiederholen", Some("CMDORCTRL+SHIFT+Z"));
        let (cut, copy, paste, select_all) = (
            tracked(Action::Cut, "Ausschneiden", Some("CMDORCTRL+X")),
            tracked(Action::Copy, "Kopieren", Some("CMDORCTRL+C")),
            tracked(Action::Paste, "Einfügen", Some("CMDORCTRL+V")),
            tracked(Action::SelectAll, "Alles auswählen", Some("CMDORCTRL+A")),
        );
        // Texterkennung gibt es bisher nur auf macOS (Apple Vision).
        let copy_text = tracked(Action::CopyText, "Text kopieren", Some("CMDORCTRL+SHIFT+C"));
        let (duplicate, delete, deselect) = (
            tracked(Action::Duplicate, "Duplizieren", Some("CMDORCTRL+D")),
            tracked(Action::Delete, "Objekt löschen", None),
            tracked(Action::Deselect, "Auswahl aufheben", None),
        );
        let sep = PredefinedMenuItem::separator();
        let mut edit_items: Vec<&dyn IsMenuItem> = vec![&undo, &redo, &sep, &cut, &copy];
        if crate::ocr::AVAILABLE {
            edit_items.push(&copy_text);
        }
        edit_items.extend([&paste as &dyn IsMenuItem, &select_all, &sep, &duplicate, &delete, &deselect]);
        let edit = Submenu::with_items("Bearbeiten", true, &edit_items).ok()?;

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
                &tracked(Action::ZoomFit, "Ins Fenster einpassen", Some("CMDORCTRL+0")),
                &tracked(Action::ZoomActual, "Originalgröße (100 %)", Some("CMDORCTRL+1")),
                &tracked(Action::ZoomIn, "Vergrößern", Some("CMDORCTRL+EQUAL")),
                &tracked(Action::ZoomOut, "Verkleinern", Some("CMDORCTRL+MINUS")),
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
        let (docs, keys, website, issues) = (
            item(Action::Docs, "Kadra-Dokumentation", None),
            item(Action::Settings, "Tastenkürzel …", None),
            item(Action::Website, "Kadra auf GitHub", None),
            item(Action::Issues, "Problem melden …", None),
        );
        let mut help_items: Vec<&dyn IsMenuItem> = vec![&docs, &keys, &sep, &website, &issues];
        if !MAC {
            help_items.extend([&sep as &dyn IsMenuItem, &about]);
        }
        let help = Submenu::with_items("Hilfe", true, &help_items).ok()?;

        let menu = if MAC {
            Menu::with_items(&[&app, &file, &edit, &image, &tools, &view, &window, &help])
        } else {
            Menu::with_items(&[&file, &edit, &image, &tools, &view, &help])
        }
        .ok()?;
        #[cfg(target_os = "macos")]
        {
            menu.init_for_nsapp();
            window.set_as_windows_menu_for_nsapp();
            help.set_as_help_menu_for_nsapp();
        }
        #[cfg(target_os = "windows")]
        {
            let hwnd = crate::platform::win::main_window();
            // SAFETY: Hauptfenster von eframe, lebt so lange wie die App.
            unsafe { menu.init_for_hwnd(hwnd).ok()? };
            crate::platform::win::set_accelerators(menu.haccel());
        }
        Some(Self { _menu: menu, capture_items, editor_items, undo, redo, last_state: None })
    }

    /// Windows: Menüleiste während des Auswahl-Overlays ausblenden (sie gehört zum Fenster).
    #[cfg(target_os = "windows")]
    pub fn set_bar_visible(&self, visible: bool) {
        let hwnd = crate::platform::win::main_window();
        // SAFETY: wie in `new`.
        let _ = unsafe { if visible { self._menu.show_for_hwnd(hwnd) } else { self._menu.hide_for_hwnd(hwnd) } };
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
        // muda baut die Kürzel-Tabelle dabei neu auf.
        #[cfg(target_os = "windows")]
        crate::platform::win::set_accelerators(self._menu.haccel());
    }
}

fn about_metadata(icon: Option<Icon>) -> AboutMetadata {
    let (os, arch) = if MAC {
        ("macOS", if cfg!(target_arch = "aarch64") { "Apple Silicon" } else { "Intel" })
    } else {
        ("Windows", if cfg!(target_arch = "aarch64") { "ARM64" } else { "x64" })
    };
    let summary = "Screenshot-Werkzeug: Bildschirmfotos aufnehmen, beschriften und exportieren.";
    AboutMetadata {
        name: Some("Kadra".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        copyright: Some("© 2026 mbr6300 · GNU GPL v3.0 oder neuer".into()),
        credits: Some(format!(
            "{summary}\n\n\
             Aufnahme von Bereich, Fenster oder Bildschirm · Editor mit Pfeilen, Text, Formen, \
             Nummern, Füllen, Weichzeichnen und Zuschneiden · Export als PNG, JPEG, TIFF, WebP, GIF, BMP, PDF.\n\n\
             Quellcode: {REPO_URL}\n\
             Lizenz: GNU General Public License v3.0 oder neuer\n\n\
             Erstellt mit Rust, egui/eframe, xcap, tiny-skia, image, global-hotkey, tray-icon und muda.\n\
             Build: {os} · {arch}"
        )),
        // Windows zeigt statt `credits` diese Felder an.
        comments: Some(format!("{summary}\nBuild: {os} · {arch}")),
        license: Some("GNU GPL v3.0 oder neuer".into()),
        website: Some(REPO_URL.into()),
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
        for s in ["CMDORCTRL+COMMA", "CMDORCTRL+Q", "CMDORCTRL+O", "CMDORCTRL+S", "CMDORCTRL+SHIFT+S", "CMDORCTRL+SHIFT+C", "CMDORCTRL+W", "CMDORCTRL+Z", "CMDORCTRL+SHIFT+Z", "CMDORCTRL+X", "CMDORCTRL+C", "CMDORCTRL+V", "CMDORCTRL+A", "CMDORCTRL+D", "CMDORCTRL+0", "CMDORCTRL+1", "CMDORCTRL+EQUAL", "CMDORCTRL+MINUS"] {
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
