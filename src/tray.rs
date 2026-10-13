//! Symbol in der Menüleiste (oben rechts) mit Schnellauswahl.

use tiny_skia::{Color, FillRule, LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform};
use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::capture::CaptureKind;
use crate::hotkeys::Shortcut;
use crate::menu::{capture_label, item, shortcut_accel, Action};

pub struct Tray {
    _icon: TrayIcon,
    capture_items: Vec<(CaptureKind, MenuItem)>,
}

impl Tray {
    /// Muss auf dem Hauptthread nach dem Start der Ereignisschleife erzeugt werden.
    /// Klicks laufen über den gemeinsamen Handler aus `menu::install_handler`.
    pub fn new(shortcuts: &[Option<Shortcut>; 4]) -> Option<Self> {
        let menu = Menu::new();
        let mut capture_items = vec![];
        for (kind, sc) in CaptureKind::ALL.into_iter().zip(shortcuts) {
            let it = MenuItem::with_id(Action::Capture(kind).id(), capture_label(kind), true, shortcut_accel(sc.as_ref()));
            menu.append(&it).ok()?;
            capture_items.push((kind, it));
        }
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&item(Action::Show, "Kadra öffnen", None)).ok()?;
        menu.append(&item(Action::CopyLast, "Letzte Aufnahme kopieren", None)).ok()?;
        menu.append(&item(Action::Settings, "Einstellungen …", None)).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&item(Action::Quit, "Kadra beenden", None)).ok()?;

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon_templated(template_icon())
            .with_tooltip("Kadra")
            .build()
            .ok()?;
        Some(Self { _icon: icon, capture_items })
    }

    /// Kürzel im Menü aktualisieren (nach Änderung in den Einstellungen).
    pub fn set_shortcuts(&self, shortcuts: &[Option<Shortcut>; 4]) {
        for ((_, it), sc) in self.capture_items.iter().zip(shortcuts) {
            let _ = it.set_accelerator(shortcut_accel(sc.as_ref()));
        }
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
