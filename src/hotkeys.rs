//! Frei belegbare globale Tastenkürzel für die Aufnahmemodi.

use std::sync::mpsc::Sender;

use egui::Key;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use serde::{Deserialize, Serialize};

use crate::capture::CaptureKind;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub cmd: bool,
    /// egui-Tastenname, z. B. "1", "A", "F5".
    pub key: String,
}

impl Shortcut {
    pub fn new(m: egui::Modifiers, key: Key) -> Self {
        Self { ctrl: m.ctrl, alt: m.alt, shift: m.shift, cmd: m.mac_cmd, key: key.name().to_string() }
    }

    fn key(&self) -> Option<Key> {
        Key::from_name(&self.key)
    }

    /// Standard: ⌃⇧1 … ⌃⇧4
    pub fn defaults() -> [Option<Shortcut>; 4] {
        [Key::Num1, Key::Num2, Key::Num3, Key::Num4].map(|k| {
            Some(Shortcut { ctrl: true, alt: false, shift: true, cmd: false, key: k.name().to_string() })
        })
    }

    /// Globale Kürzel brauchen ⌃, ⌥ oder ⌘ (Funktionstasten gehen auch allein).
    pub fn is_valid(&self) -> bool {
        let fkey = self.key.starts_with('F') && self.key.len() > 1;
        (self.ctrl || self.alt || self.cmd || fkey) && self.key().and_then(key_code).is_some()
    }

    fn hotkey(&self) -> Option<HotKey> {
        let mut m = Modifiers::empty();
        m.set(Modifiers::CONTROL, self.ctrl);
        m.set(Modifiers::ALT, self.alt);
        m.set(Modifiers::SHIFT, self.shift);
        m.set(Modifiers::SUPER, self.cmd);
        Some(HotKey::new(Some(m), key_code(self.key()?)?))
    }

    /// Kurzform, z. B. „⌃⇧1“.
    pub fn compact(&self) -> String {
        let mac = cfg!(target_os = "macos");
        let mut s = String::new();
        for (on, sym, word) in [(self.ctrl, "⌃", "Ctrl+"), (self.alt, "⌥", "Alt+"), (self.shift, "⇧", "Shift+"), (self.cmd, "⌘", "Win+")] {
            if on {
                s += if mac { sym } else { word };
            }
        }
        s + &self.key().map(|k| k.symbol_or_name().to_string()).unwrap_or_else(|| self.key.clone())
    }
}

/// Sondertaste für die gezeichneten Tastenkappen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mod {
    Ctrl,
    Alt,
    Shift,
    Cmd,
}

impl Mod {
    pub fn word(self) -> &'static str {
        match self {
            Mod::Ctrl => "Ctrl",
            Mod::Alt => if cfg!(target_os = "macos") { "Option" } else { "Alt" },
            Mod::Shift => "Shift",
            Mod::Cmd => if cfg!(target_os = "macos") { "Cmd" } else { "Win" },
        }
    }
}

impl Shortcut {
    /// Sondertasten in macOS-Reihenfolge (⌃ ⌥ ⇧ ⌘).
    pub fn mods(&self) -> Vec<Mod> {
        [(self.ctrl, Mod::Ctrl), (self.alt, Mod::Alt), (self.shift, Mod::Shift), (self.cmd, Mod::Cmd)]
            .into_iter()
            .filter_map(|(on, m)| on.then_some(m))
            .collect()
    }

    pub fn key_label(&self) -> String {
        self.key().map(|k| k.symbol_or_name().to_string()).unwrap_or_else(|| self.key.clone())
    }

    /// Ausgeschrieben, z. B. „Ctrl + Shift + 1“ (für Tooltips).
    pub fn words(&self) -> String {
        let mut v: Vec<String> = self.mods().iter().map(|m| m.word().to_string()).collect();
        v.push(self.key_label());
        v.join(" + ")
    }
}

/// Zeichnet das Symbol einer Sondertaste als Vektorgrafik (zentriert in `c`, Kantenlänge `s`).
pub fn paint_mod_symbol(p: &egui::Painter, m: Mod, c: egui::Pos2, s: f32, color: egui::Color32) {
    use egui::{Pos2, Shape, Stroke};
    let w = (s * 0.11).max(1.2);
    let st = Stroke::new(w, color);
    let pt = |x: f32, y: f32| Pos2::new(c.x + x * s, c.y + y * s);
    match m {
        // ⌃ – Dach
        Mod::Ctrl => {
            p.add(Shape::line(vec![pt(-0.3, 0.12), pt(0.0, -0.2), pt(0.3, 0.12)], st));
        }
        // ⇧ – Umriss-Pfeil
        Mod::Shift => {
            let pts = vec![
                pt(0.0, -0.36),
                pt(0.36, 0.02),
                pt(0.16, 0.02),
                pt(0.16, 0.34),
                pt(-0.16, 0.34),
                pt(-0.16, 0.02),
                pt(-0.36, 0.02),
            ];
            p.add(Shape::closed_line(pts, st));
        }
        // ⌥ – Weiche
        Mod::Alt => {
            p.add(Shape::line(vec![pt(-0.38, -0.22), pt(-0.12, -0.22), pt(0.14, 0.24), pt(0.38, 0.24)], st));
            p.line_segment([pt(0.1, -0.22), pt(0.38, -0.22)], st);
        }
        // ⌘ – Quadrat mit vier Schlaufen
        Mod::Cmd => {
            let (a, r) = (0.14, 0.11);
            p.line_segment([pt(-a, -a - r), pt(-a, a + r)], st);
            p.line_segment([pt(a, -a - r), pt(a, a + r)], st);
            p.line_segment([pt(-a - r, -a), pt(a + r, -a)], st);
            p.line_segment([pt(-a - r, a), pt(a + r, a)], st);
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                p.circle_stroke(pt(dx * (a + r), dy * (a + r)), r * s, st);
            }
        }
    }
}

/// Farben für Tastenkappen: (Kappe, Symbol/Text).
pub fn cap_colors(ui: &egui::Ui, on_accent: bool) -> (egui::Color32, egui::Color32) {
    if on_accent {
        (egui::Color32::from_white_alpha(46), egui::Color32::WHITE)
    } else {
        let v = ui.visuals();
        (v.widgets.inactive.bg_fill, v.strong_text_color())
    }
}

/// Breite der Tastenkappen eines Kürzels bei Höhe `h`.
pub fn keycaps_width(ui: &egui::Ui, sc: &Shortcut, h: f32) -> f32 {
    let gap = h * 0.18;
    let key_w = key_cap_width(ui, &sc.key_label(), h);
    sc.mods().len() as f32 * (h + gap) + key_w
}

fn key_cap_width(ui: &egui::Ui, label: &str, h: f32) -> f32 {
    let tw = ui.fonts_mut(|f| {
        f.layout_no_wrap(label.to_string(), egui::FontId::proportional(h * 0.58), egui::Color32::WHITE).size().x
    });
    (tw + h * 0.6).max(h)
}

/// Malt die Tastenkappen ab `left_center` nach rechts; gibt die belegte Breite zurück.
pub fn paint_keycaps(ui: &egui::Ui, left_center: egui::Pos2, sc: &Shortcut, h: f32, on_accent: bool) -> f32 {
    use egui::{Align2, FontId, Pos2, Rect, Vec2};
    let (bg, fg) = cap_colors(ui, on_accent);
    let p = ui.painter();
    let gap = h * 0.18;
    let radius = (h * 0.24) as u8;
    let mut x = left_center.x;
    for m in sc.mods() {
        let r = Rect::from_min_size(Pos2::new(x, left_center.y - h / 2.0), Vec2::splat(h));
        p.rect_filled(r, radius, bg);
        paint_mod_symbol(p, m, r.center(), h * 0.62, fg);
        x += h + gap;
    }
    let label = sc.key_label();
    let w = key_cap_width(ui, &label, h);
    let r = Rect::from_min_size(Pos2::new(x, left_center.y - h / 2.0), Vec2::new(w, h));
    p.rect_filled(r, radius, bg);
    p.text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(h * 0.58), fg);
    x + w - left_center.x
}

/// Kürzel als Tastenkappen-Widget (Tooltip mit ausgeschriebenen Tasten).
pub fn caps_ui(ui: &mut egui::Ui, sc: &Shortcut) -> egui::Response {
    let h = 24.0;
    let (rect, resp) = ui.allocate_exact_size(egui::Vec2::new(keycaps_width(ui, sc, h), h), egui::Sense::hover());
    paint_keycaps(ui, rect.left_center(), sc, h, false);
    resp.on_hover_text(sc.words())
}

pub struct Hotkeys {
    manager: GlobalHotKeyManager,
    registered: Vec<(HotKey, CaptureKind)>,
}

impl Hotkeys {
    pub fn new(ctx: &egui::Context, tx: Sender<u32>) -> Option<Self> {
        let manager = GlobalHotKeyManager::new().ok()?;
        let ctx = ctx.clone();
        // Der Handler lässt sich nur einmal setzen; Kürzel werden danach beliebig neu registriert.
        GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
            if e.state == HotKeyState::Pressed {
                let _ = tx.send(e.id);
                ctx.request_repaint();
            }
        }));
        Some(Self { manager, registered: vec![] })
    }

    /// Registriert die Kürzel neu. Gibt die Modi zurück, deren Kürzel belegt/ungültig ist.
    pub fn apply(&mut self, shortcuts: &[Option<Shortcut>; 4], enabled: bool) -> Vec<CaptureKind> {
        self.suspend();
        let mut failed = vec![];
        if !enabled {
            return failed;
        }
        for (sc, kind) in shortcuts.iter().zip(CaptureKind::ALL) {
            let Some(sc) = sc else { continue };
            match sc.hotkey() {
                Some(hk) if self.manager.register(hk).is_ok() => self.registered.push((hk, kind)),
                _ => failed.push(kind),
            }
        }
        failed
    }

    /// Alle Kürzel abmelden (z. B. während ein neues aufgenommen wird).
    pub fn suspend(&mut self) {
        for (hk, _) in self.registered.drain(..) {
            let _ = self.manager.unregister(hk);
        }
    }

    pub fn kind_for(&self, id: u32) -> Option<CaptureKind> {
        self.registered.iter().find(|(hk, _)| hk.id() == id).map(|(_, k)| *k)
    }
}

fn key_code(k: Key) -> Option<Code> {
    use Code as C;
    use Key as K;
    Some(match k {
        K::A => C::KeyA,
        K::B => C::KeyB,
        K::C => C::KeyC,
        K::D => C::KeyD,
        K::E => C::KeyE,
        K::F => C::KeyF,
        K::G => C::KeyG,
        K::H => C::KeyH,
        K::I => C::KeyI,
        K::J => C::KeyJ,
        K::K => C::KeyK,
        K::L => C::KeyL,
        K::M => C::KeyM,
        K::N => C::KeyN,
        K::O => C::KeyO,
        K::P => C::KeyP,
        K::Q => C::KeyQ,
        K::R => C::KeyR,
        K::S => C::KeyS,
        K::T => C::KeyT,
        K::U => C::KeyU,
        K::V => C::KeyV,
        K::W => C::KeyW,
        K::X => C::KeyX,
        K::Y => C::KeyY,
        K::Z => C::KeyZ,
        K::Num0 => C::Digit0,
        K::Num1 => C::Digit1,
        K::Num2 => C::Digit2,
        K::Num3 => C::Digit3,
        K::Num4 => C::Digit4,
        K::Num5 => C::Digit5,
        K::Num6 => C::Digit6,
        K::Num7 => C::Digit7,
        K::Num8 => C::Digit8,
        K::Num9 => C::Digit9,
        K::F1 => C::F1,
        K::F2 => C::F2,
        K::F3 => C::F3,
        K::F4 => C::F4,
        K::F5 => C::F5,
        K::F6 => C::F6,
        K::F7 => C::F7,
        K::F8 => C::F8,
        K::F9 => C::F9,
        K::F10 => C::F10,
        K::F11 => C::F11,
        K::F12 => C::F12,
        K::F13 => C::F13,
        K::F14 => C::F14,
        K::F15 => C::F15,
        K::F16 => C::F16,
        K::F17 => C::F17,
        K::F18 => C::F18,
        K::F19 => C::F19,
        K::F20 => C::F20,
        K::Space => C::Space,
        K::Enter => C::Enter,
        K::Tab => C::Tab,
        K::Backspace => C::Backspace,
        K::Delete => C::Delete,
        K::Insert => C::Insert,
        K::Home => C::Home,
        K::End => C::End,
        K::PageUp => C::PageUp,
        K::PageDown => C::PageDown,
        K::ArrowUp => C::ArrowUp,
        K::ArrowDown => C::ArrowDown,
        K::ArrowLeft => C::ArrowLeft,
        K::ArrowRight => C::ArrowRight,
        K::Minus => C::Minus,
        K::Equals => C::Equal,
        K::Comma => C::Comma,
        K::Period => C::Period,
        K::Slash => C::Slash,
        K::Backslash => C::Backslash,
        K::Semicolon => C::Semicolon,
        K::Quote => C::Quote,
        K::OpenBracket => C::BracketLeft,
        K::CloseBracket => C::BracketRight,
        K::Backtick => C::Backquote,
        _ => return None,
    })
}
