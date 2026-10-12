//! Symbol in der Menüleiste (oben rechts) mit Schnellauswahl.

use std::sync::mpsc::Sender;

use tiny_skia::{Color, FillRule, LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::capture::CaptureKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAction {
    Capture(CaptureKind),
    Show,
    CopyLast,
    Settings,
    Quit,
}

impl TrayAction {
    fn id(self) -> String {
        match self {
            TrayAction::Capture(k) => format!("capture-{}", k as u8),
            TrayAction::Show => "show".into(),
            TrayAction::CopyLast => "copy".into(),
            TrayAction::Settings => "settings".into(),
            TrayAction::Quit => "quit".into(),
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        let all = CaptureKind::ALL.map(TrayAction::Capture);
        all.into_iter()
            .chain([TrayAction::Show, TrayAction::CopyLast, TrayAction::Settings, TrayAction::Quit])
            .find(|a| a.id() == id)
    }
}

pub struct Tray {
    _icon: TrayIcon,
    capture_items: Vec<(CaptureKind, MenuItem)>,
}

impl Tray {
    /// Muss auf dem Hauptthread nach dem Start der Ereignisschleife erzeugt werden.
    pub fn new(ctx: &egui::Context, tx: Sender<TrayAction>, shortcuts: &[Option<String>; 4]) -> Option<Self> {
        let menu = Menu::new();
        let mut capture_items = vec![];
        for (kind, sc) in CaptureKind::ALL.into_iter().zip(shortcuts) {
            let item = MenuItem::with_id(TrayAction::Capture(kind).id(), capture_label(kind, sc.as_deref()), true, None);
            menu.append(&item).ok()?;
            capture_items.push((kind, item));
        }
        let items = [
            (TrayAction::Show, "SnapIt öffnen"),
            (TrayAction::CopyLast, "Letzte Aufnahme kopieren"),
            (TrayAction::Settings, "Einstellungen …"),
        ];
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        for (action, text) in items {
            menu.append(&MenuItem::with_id(action.id(), text, true, None)).ok()?;
        }
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&MenuItem::with_id(TrayAction::Quit.id(), "SnapIt beenden", true, None)).ok()?;

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon_templated(template_icon())
            .with_tooltip("SnapIt")
            .build()
            .ok()?;

        let ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if let Some(action) = TrayAction::from_id(e.id.as_ref()) {
                let _ = tx.send(action);
                ctx.request_repaint();
            }
        }));
        Some(Self { _icon: icon, capture_items })
    }

    /// Kürzel im Menü aktualisieren (nach Änderung in den Einstellungen).
    pub fn set_shortcuts(&self, shortcuts: &[Option<String>; 4]) {
        for ((kind, item), sc) in self.capture_items.iter().zip(shortcuts) {
            item.set_text(capture_label(*kind, sc.as_deref()));
        }
    }
}

fn capture_label(kind: CaptureKind, shortcut: Option<&str>) -> String {
    match shortcut {
        Some(sc) => format!("{} aufnehmen   {sc}", kind.label()),
        None => format!("{} aufnehmen", kind.label()),
    }
}

/// Schwarzes Sucher-Symbol (macOS färbt Template-Icons passend ein), 18 pt @2x.
fn template_icon() -> Icon {
    let s = 36u32;
    let mut pm = Pixmap::new(s, s).unwrap();
    let mut black = Paint::default();
    black.set_color(Color::BLACK);
    black.anti_alias = true;
    let stroke = Stroke { width: 3.4, line_cap: LineCap::Round, ..Default::default() };
    let (a, b, arm) = (5.0, 31.0, 8.0);
    for (x, y, dx, dy) in [(a, a, 1.0, 1.0), (b, a, -1.0, 1.0), (a, b, 1.0, -1.0), (b, b, -1.0, -1.0)] {
        let mut pb = PathBuilder::new();
        pb.move_to(x, y + dy * arm);
        pb.line_to(x, y);
        pb.line_to(x + dx * arm, y);
        pm.stroke_path(&pb.finish().unwrap(), &black, &stroke, Transform::identity(), None);
    }
    let dot = PathBuilder::from_circle(18.0, 18.0, 4.5).unwrap();
    pm.fill_path(&dot, &black, FillRule::Winding, Transform::identity(), None);
    // tiny-skia speichert vormultipliziert; bei Schwarz entspricht das RGBA direkt.
    Icon::from_rgba(pm.take(), s, s).expect("Tray-Icon")
}
