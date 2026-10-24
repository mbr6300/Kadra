//! Text im Bild markieren wie in einem Textdokument (Werkzeug „Text auswählen“):
//! Einfügemarken, Bereiche über mehrere Zeilen, Wort/Zeile, markierten Text zusammensetzen.

use egui::{Pos2, Rect};

use crate::ocr::{self, Line};

/// Position zwischen zwei Zeichen: Zeile (in Lesereihenfolge) und Zeichengrenze.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Caret {
    pub line: usize,
    pub ch: usize,
}

/// Erkannter Text eines Bildes, Zeilen in Lesereihenfolge.
pub struct TextLayer {
    pub lines: Vec<Line>,
    chars: Vec<Vec<char>>,
}

impl TextLayer {
    pub fn new(lines: Vec<Line>) -> Self {
        let order: Vec<usize> = ocr::rows(&lines).into_iter().flatten().collect();
        let lines: Vec<Line> = order.into_iter().map(|i| lines[i].clone()).collect();
        let chars = lines.iter().map(|l| l.text.chars().collect()).collect();
        Self { lines, chars }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    fn len(&self, line: usize) -> usize {
        self.chars[line].len()
    }

    pub fn line_rect(&self, i: usize) -> Rect {
        let [x, y, w, h] = self.lines[i].rect;
        Rect::from_min_size(Pos2::new(x, y), egui::vec2(w, h))
    }

    /// Zeile unter dem Punkt (mit Toleranz `pad` in Bildpixeln).
    pub fn line_at(&self, q: Pos2, pad: f32) -> Option<usize> {
        (0..self.lines.len()).find(|&i| self.line_rect(i).expand(pad).contains(q))
    }

    /// Einfügemarke zum Punkt: nächste Zeile (vertikaler Abstand zählt stärker, damit
    /// Ziehen neben dem Text in der richtigen Zeile bleibt), dort die nächste Zeichengrenze.
    pub fn caret_at(&self, q: Pos2) -> Option<Caret> {
        let dist = |r: Rect| {
            let dy = (r.min.y - q.y).max(q.y - r.max.y).max(0.0);
            let dx = (r.min.x - q.x).max(q.x - r.max.x).max(0.0);
            dy * 8.0 + dx
        };
        let line = (0..self.lines.len()).min_by(|&a, &b| dist(self.line_rect(a)).total_cmp(&dist(self.line_rect(b))))?;
        let xs = &self.lines[line].xs;
        let ch = (0..xs.len()).min_by(|&a, &b| (xs[a] - q.x).abs().total_cmp(&(xs[b] - q.x).abs())).unwrap_or(0);
        Some(Caret { line, ch })
    }

    /// Wort an der Einfügemarke (Doppelklick): zusammenhängende Buchstaben/Ziffern bzw.
    /// Satzzeichen; auf einem Leerzeichen nur dieses.
    pub fn word_at(&self, c: Caret) -> (Caret, Caret) {
        let chars = &self.chars[c.line];
        if chars.is_empty() {
            return (c, c);
        }
        let mut i = c.ch.min(chars.len() - 1);
        // Am Wortende geklickt (Grenze hinter dem letzten Zeichen) → Wort davor
        if chars[i].is_whitespace() && i > 0 && !chars[i - 1].is_whitespace() {
            i -= 1;
        }
        let class = |ch: char| if ch.is_whitespace() { 0 } else if ch.is_alphanumeric() { 1 } else { 2 };
        let k = class(chars[i]);
        let (mut a, mut b) = (i, i + 1);
        while a > 0 && class(chars[a - 1]) == k && k != 0 {
            a -= 1;
        }
        while b < chars.len() && class(chars[b]) == k && k != 0 {
            b += 1;
        }
        (Caret { line: c.line, ch: a }, Caret { line: c.line, ch: b })
    }

    /// Ganze Zeile (Dreifachklick).
    pub fn line_range(&self, line: usize) -> (Caret, Caret) {
        (Caret { line, ch: 0 }, Caret { line, ch: self.len(line) })
    }

    /// Gesamter Text (⌘A).
    pub fn all(&self) -> Option<(Caret, Caret)> {
        let last = self.lines.len().checked_sub(1)?;
        Some((Caret { line: 0, ch: 0 }, Caret { line: last, ch: self.len(last) }))
    }

    /// Markierte Abschnitte als (Zeile, von, bis) – Reihenfolge der Marken egal, leere entfallen.
    pub fn pieces(&self, a: Caret, b: Caret) -> Vec<(usize, usize, usize)> {
        let (s, e) = if a <= b { (a, b) } else { (b, a) };
        (s.line..=e.line.min(self.lines.len().saturating_sub(1)))
            .filter_map(|i| {
                let from = if i == s.line { s.ch } else { 0 };
                let to = if i == e.line { e.ch } else { self.len(i) };
                (to > from).then_some((i, from, to.min(self.len(i))))
            })
            .collect()
    }

    /// Fläche eines Abschnitts in Bildpixeln.
    pub fn piece_rect(&self, line: usize, from: usize, to: usize) -> Rect {
        let l = &self.lines[line];
        Rect::from_min_max(Pos2::new(l.xs[from], l.rect[1]), Pos2::new(l.xs[to], l.rect[1] + l.rect[3]))
    }

    /// Markierter Text – Reihen, Spalten (Tabulator) und Absätze wie beim „Text kopieren“.
    pub fn text(&self, a: Caret, b: Caret) -> String {
        let parts: Vec<Line> = self
            .pieces(a, b)
            .into_iter()
            .map(|(i, from, to)| {
                let r = self.piece_rect(i, from, to);
                Line {
                    text: self.chars[i][from..to].iter().collect(),
                    rect: [r.min.x, r.min.y, r.width(), r.height()],
                    xs: self.lines[i].xs[from..=to].to_vec(),
                }
            })
            .collect();
        ocr::to_text(&parts)
    }

    /// Anzahl markierter Zeichen.
    pub fn count(&self, a: Caret, b: Caret) -> usize {
        self.pieces(a, b).iter().map(|(_, f, t)| t - f).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zwei Zeilen untereinander, 10 px je Zeichen.
    fn layer() -> TextLayer {
        TextLayer::new(vec![
            Line::uniform("Zweite Zeile", [10.0, 50.0, 120.0, 20.0]),
            Line::uniform("Hallo Welt", [10.0, 10.0, 100.0, 20.0]),
        ])
    }

    fn c(line: usize, ch: usize) -> Caret {
        Caret { line, ch }
    }

    #[test]
    fn lines_in_reading_order_and_caret_hit() {
        let t = layer();
        assert_eq!(t.lines[0].text, "Hallo Welt");
        assert_eq!(t.caret_at(Pos2::new(34.0, 20.0)), Some(c(0, 2)));
        // Links neben dem Text → Zeilenanfang, rechts daneben → Zeilenende
        assert_eq!(t.caret_at(Pos2::new(0.0, 60.0)), Some(c(1, 0)));
        assert_eq!(t.caret_at(Pos2::new(400.0, 15.0)), Some(c(0, 10)));
        // Zwischen den Zeilen: die nähere gewinnt
        assert_eq!(t.caret_at(Pos2::new(20.0, 33.0)).unwrap().line, 0);
    }

    #[test]
    fn selection_across_lines_and_backwards() {
        let t = layer();
        // „Welt“ + „Zweite“, rückwärts gezogen
        assert_eq!(t.text(c(1, 6), c(0, 6)), "Welt\nZweite");
        assert_eq!(t.count(c(1, 6), c(0, 6)), 10);
        assert_eq!(t.pieces(c(0, 3), c(0, 3)), vec![]);
        let all = t.all().unwrap();
        assert_eq!(t.text(all.0, all.1), "Hallo Welt\nZweite Zeile");
        let r = t.piece_rect(0, 6, 10);
        assert_eq!((r.min.x, r.max.x), (70.0, 110.0));
    }

    #[test]
    fn word_and_line_ranges() {
        let t = layer();
        assert_eq!(t.word_at(c(0, 7)), (c(0, 6), c(0, 10)));
        assert_eq!(t.word_at(c(0, 10)), (c(0, 6), c(0, 10)), "am Zeilenende");
        assert_eq!(t.word_at(c(0, 5)), (c(0, 0), c(0, 5)), "Grenze vor Leerzeichen → Wort davor");
        assert_eq!(t.line_range(1), (c(1, 0), c(1, 12)));
    }
}
