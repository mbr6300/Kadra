//! Dokumentmodell: Bild + nicht-destruktive Anmerkungen.

use std::sync::Arc;

use egui::{Pos2, Rect, Vec2};
use image::RgbaImage;
use serde::{Deserialize, Serialize};

pub type P = [f32; 2];
pub type Rgba = [u8; 4];

pub fn p(v: Pos2) -> P {
    [v.x, v.y]
}
pub fn pos(v: P) -> Pos2 {
    Pos2::new(v[0], v[1])
}
pub fn rect(a: P, b: P) -> Rect {
    Rect::from_two_pos(pos(a), pos(b))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    Arrow { a: P, b: P },
    Line { a: P, b: P },
    Rect { a: P, b: P },
    Ellipse { a: P, b: P },
    Pen { pts: Vec<P> },
    Highlight { a: P, b: P },
    Text { pos: P, text: String },
    Step { pos: P, n: u32 },
    Blur { a: P, b: P },
    Pixelate { a: P, b: P },
    Spotlight { a: P, b: P },
    /// Farbeimer: füllt die zusammenhängende Fläche ab `pos` (Toleranz = style.width).
    Fill { pos: P },
}

impl Shape {
    /// Raster-Effekte werden direkt ins Basisbild gerechnet.
    pub fn is_effect(&self) -> bool {
        matches!(self, Shape::Blur { .. } | Shape::Pixelate { .. } | Shape::Fill { .. })
    }

    pub fn translate(&mut self, d: Vec2) {
        self.map_points(|q| [q[0] + d.x, q[1] + d.y]);
    }

    /// Wendet eine Punkttransformation auf alle Koordinaten an (Drehen, Spiegeln, Skalieren).
    pub fn map_points(&mut self, f: impl Fn(P) -> P) {
        let mv = |q: &mut P| *q = f(*q);
        match self {
            Shape::Arrow { a, b }
            | Shape::Line { a, b }
            | Shape::Rect { a, b }
            | Shape::Ellipse { a, b }
            | Shape::Highlight { a, b }
            | Shape::Blur { a, b }
            | Shape::Pixelate { a, b }
            | Shape::Spotlight { a, b } => {
                mv(a);
                mv(b);
            }
            Shape::Pen { pts } => pts.iter_mut().for_each(mv),
            Shape::Text { pos, .. } | Shape::Step { pos, .. } | Shape::Fill { pos } => mv(pos),
        }
    }

    /// Ziehpunkte zur Größenänderung (in Bildkoordinaten).
    pub fn handles(&self) -> Vec<Pos2> {
        match self {
            Shape::Arrow { a, b } | Shape::Line { a, b } => vec![pos(*a), pos(*b)],
            Shape::Rect { a, b }
            | Shape::Ellipse { a, b }
            | Shape::Highlight { a, b }
            | Shape::Blur { a, b }
            | Shape::Pixelate { a, b }
            | Shape::Spotlight { a, b } => {
                vec![pos(*a), Pos2::new(b[0], a[1]), pos(*b), Pos2::new(a[0], b[1])]
            }
            _ => vec![],
        }
    }

    pub fn set_handle(&mut self, idx: usize, q: Pos2) {
        match self {
            Shape::Arrow { a, b } | Shape::Line { a, b } => {
                if idx == 0 {
                    *a = p(q)
                } else {
                    *b = p(q)
                }
            }
            Shape::Rect { a, b }
            | Shape::Ellipse { a, b }
            | Shape::Highlight { a, b }
            | Shape::Blur { a, b }
            | Shape::Pixelate { a, b }
            | Shape::Spotlight { a, b } => match idx {
                0 => *a = p(q),
                1 => {
                    b[0] = q.x;
                    a[1] = q.y
                }
                2 => *b = p(q),
                _ => {
                    a[0] = q.x;
                    b[1] = q.y
                }
            },
            _ => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub stroke: Rgba,
    pub fill: Option<Rgba>,
    pub width: f32,
    pub font_size: f32,
    pub shadow: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            stroke: [230, 40, 40, 255],
            fill: None,
            width: 5.0,
            font_size: 32.0,
            shadow: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub shape: Shape,
    pub style: Style,
}

/// Nicht-destruktive Bildeffekte, die beim Export angewendet werden.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Effects {
    pub border: Option<(f32, Rgba)>,
    pub drop_shadow: bool,
    pub round_corners: f32,
}

impl Default for Effects {
    fn default() -> Self {
        Self { border: None, drop_shadow: false, round_corners: 0.0 }
    }
}

#[derive(Clone)]
pub struct Document {
    pub image: Arc<RgbaImage>,
    pub annotations: Vec<Annotation>,
    pub effects: Effects,
}

/// Serialisierbarer Teil eines Dokuments (Bild liegt separat als PNG).
#[derive(Serialize, Deserialize, Default)]
pub struct DocMeta {
    pub annotations: Vec<Annotation>,
    #[serde(default)]
    pub effects: Effects,
}

impl Document {
    pub fn new(image: RgbaImage) -> Self {
        Self { image: Arc::new(image), annotations: vec![], effects: Effects::default() }
    }

    pub fn size(&self) -> Vec2 {
        Vec2::new(self.image.width() as f32, self.image.height() as f32)
    }

    pub fn next_step(&self) -> u32 {
        self.annotations
            .iter()
            .filter_map(|a| match a.shape {
                Shape::Step { n, .. } => Some(n),
                _ => None,
            })
            .max()
            .unwrap_or(0)
            + 1
    }
}

/// Abstand Punkt–Strecke.
pub fn dist_to_segment(q: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() > 0.0 { ((q - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t - q).length()
}
