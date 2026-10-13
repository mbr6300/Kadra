# SnapIt

Screenshot-Werkzeug im Stil von SnagIt (ohne Video), geschrieben in Rust (eframe/egui, xcap, tiny-skia).

Ausführliche Beschreibung aller Funktionen, Architektur und möglicher Erweiterungen:
**[docs/DOKUMENTATION.md](docs/DOKUMENTATION.md)**

## Starten

```bash
./bundle.sh                 # baut target/SnapIt.app
open target/SnapIt.app
```

Installierbares DMG (App + Verknüpfung „Programme“ zum Hineinziehen):

```bash
./make_dmg.sh               # erzeugt target/SnapIt-<version>.dmg
```

Ist das Ziel `x86_64-apple-darwin` installiert (`rustup target add x86_64-apple-darwin`),
entsteht automatisch eine Universal-App für Apple Silicon und Intel.
Signiert wird mit der ersten Codesigning-Identität im Schlüsselbund (oder `SNAPIT_SIGN_IDENTITY`).
Für die Weitergabe an andere Macs ohne Gatekeeper-Warnung braucht es eine
„Developer ID Application“-Identität und Notarisierung (`xcrun notarytool`).

Beim ersten Aufnehmen fragt macOS nach der Berechtigung **Bildschirmaufnahme**
(Systemeinstellungen → Datenschutz & Sicherheit). Danach SnapIt neu starten.
Für die Entwicklung geht auch `cargo run --release [bild.png]`.

## Aufnahme

| Modus | Kürzel (global) | Beschreibung |
|---|---|---|
| Bereich | ⌃⇧1 | Bereich aufziehen **oder** Fenster anklicken; Lupe mit Pixelkoordinate und Farbwert; Enter = ganzer Bildschirm |
| Fenster | ⌃⇧2 | Fenster unter der Maus wird hervorgehoben, Klick nimmt es auf (auch verdeckte Teile) |
| Bildschirm | ⌃⇧3 | Bildschirm unter dem Mauszeiger |
| Alle Bildschirme | ⌃⇧4 | Alle Monitore zu einem Bild zusammengesetzt |

Zusätzlich: Verzögerung (1–10 s), Bild aus Zwischenablage, Datei öffnen, Drag & Drop.

## Nachbearbeitung

Werkzeuge (Taste): Auswahl (V), Pfeil (A), Linie (L), Rechteck (R), Ellipse (E), Stift (P),
Textmarker (H), Text (T), Nummerierung (N), Weichzeichnen (B), Verpixeln (X), Spotlight (S), Füllen (F), Zuschneiden (C).

- Füllen (Farbeimer): füllt die zusammenhängende Fläche ähnlicher Farbe; Toleranz und Farbe (inkl. Transparenz) bleiben nachträglich änderbar

- Alle Anmerkungen bleiben editierbar: verschieben, Größe über Griffe ändern, Farbe/Stärke/Füllung/Schatten anpassen
- Shift beim Zeichnen: 45°-Winkel bzw. Quadrat/Kreis
- Bild: Drehen, Spiegeln, Größe ändern, Zuschneiden
- Effekte: Rahmen, runde Ecken, Schlagschatten
- Undo/Redo (⌘Z / ⌘⇧Z), Duplizieren (⌘D), Löschen (Entf), Pfeiltasten verschieben, Zoom mit ⌘+Scroll/Pinch

## Ausgabe & Bibliothek

- Kopieren (⌘C), Schnellspeichern (⌘S) in `~/Pictures/SnapIt`, Speichern unter (⌘⇧S), „Exportieren als ▸“
- Formate: PNG, JPEG, TIFF, WebP, GIF, BMP, PDF – Schnellspeicher-Format und JPEG-/PDF-Qualität in den Einstellungen
- Formate ohne Transparenz (JPEG, BMP, PDF) bekommen weißen Hintergrund
- Jede Aufnahme landet automatisch in der Bibliothek (Leiste unten) und bleibt dort editierbar:
  `~/Library/Application Support/SnapIt/library/` (Original-PNG + Anmerkungen als JSON)

## Aufbau

| Datei | Inhalt |
|---|---|
| `src/capture.rs` | Bildschirm-/Fensteraufnahme (xcap) |
| `src/overlay.rs` | Vollbild-Auswahl mit Lupe |
| `src/editor.rs` | Editor, Werkzeuge, Undo |
| `src/doc.rs` | Dokumentmodell (Anmerkungen) |
| `src/render.rs` | Export-Rendering (tiny-skia, ab_glyph) |
| `src/library.rs` | Bibliothek, Einstellungen, Export |
| `src/platform.rs` | macOS-Spezifika (Fensterebene, Mausposition) |

## Lizenz

SnapIt ist freie Software unter der **GNU General Public License v3.0 oder neuer**
(`GPL-3.0-or-later`), siehe [LICENSE](LICENSE). Du darfst das Programm nutzen, verändern und
weitergeben; veränderte Versionen müssen ebenfalls unter der GPL mit Quellcode verbreitet werden.
Verwendete Bibliotheken stehen unter MIT- bzw. Apache-2.0-Lizenzen (GPL-3.0-kompatibel).
