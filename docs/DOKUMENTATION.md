# Kadra – Dokumentation

Kadra ist ein Screenshot-Werkzeug für macOS.
Es nimmt Bildschirmbereiche, Fenster oder ganze Bildschirme auf, bietet einen Editor zum
Beschriften und Nachbearbeiten und gibt das Ergebnis als Datei oder über die Zwischenablage aus.
Geschrieben in Rust (eframe/egui), läuft als native App mit Menüleisten-Symbol.

---

## 1. Installation

| Weg | Vorgehen |
|---|---|
| DMG (Release) | DMG öffnen, **Kadra** auf **Programme** ziehen. Da die App nicht notarisiert ist: beim ersten Start im Finder *Rechtsklick → Öffnen*. |
| Aus dem Quellcode | `./bundle.sh` baut `target/Kadra.app`, `./make_dmg.sh` zusätzlich das DMG. |

Beim ersten Aufnehmen fragt macOS nach **Bildschirm- und Systemaudioaufnahme**
(Systemeinstellungen → Datenschutz & Sicherheit). Danach Kadra neu starten.
Fehlt die Berechtigung, zeigt Kadra einen Hinweis mit Direktlink zu den Einstellungen.

Voraussetzungen: macOS 12+, Apple Silicon (Universal-Build möglich, siehe Abschnitt 6).

**Umstieg von früheren Versionen (bis 0.8.2 unter dem Namen „SnapIt“):** Bibliothek und
Einstellungen werden beim ersten Start automatisch übernommen. Die Berechtigung für
Bildschirmaufnahme muss für Kadra einmal neu erteilt werden; die alte App kann gelöscht und ihr
Eintrag in der Berechtigungsliste entfernt werden.

---

## 2. Funktionen

### 2.1 Aufnahme

| Modus | Standard-Kürzel | Beschreibung |
|---|---|---|
| **Bereich** | ⌃⇧1 | Bereich aufziehen **oder** ein Fenster anklicken (beides in einem Modus) |
| **Fenster** | ⌃⇧2 | Fenster unter der Maus wird hervorgehoben; Klick nimmt es auf |
| **Bildschirm** | ⌃⇧3 | Bildschirm, auf dem sich der Mauszeiger befindet |
| **Alle Bildschirme** | ⌃⇧4 | Alle Monitore zu einem Bild zusammengesetzt |

**Auswahl-Overlay** (eingefrorenes Bildschirmbild):
- Lupe mit 8-facher Vergrößerung, Pixelkoordinate und Farbwert (Hex)
- Größenanzeige in Pixeln, Fadenkreuz im Bereichsmodus
- **Tab / ⇧Tab** blättert durch übereinanderliegende Fenster unter dem Zeiger
- **L** blendet eine durchsuchbare **Fensterliste** ein – auch verdeckte Fenster und solche auf
  anderen Schreibtischen; Status je Fenster (sichtbar, verdeckt, anderer Bildschirm, minimiert)
- Fensteraufnahmen enthalten den echten Fensterinhalt (auch verdeckte Teile)
- **Enter** = ganzer Bildschirm, **Esc** = Abbrechen

Weitere Quellen: **Verzögerung** (1–10 s), **Einfügen** aus der Zwischenablage, **Öffnen** von
Bilddateien, **Drag & Drop** ins Fenster.

### 2.2 Editor

Werkzeuge in der oberen Leiste (Taste in Klammern):

| Werkzeug | Funktion |
|---|---|
| Auswahl (V) | Objekte wählen, verschieben, über Griffe skalieren; Pfeiltasten verschieben (⇧ = 10 px) |
| Pfeil (A) | Pfeil mit Spitze; ⇧ rastet auf 45° |
| Text (T) | Text mit optionalem Hintergrund; Doppelklick zum Bearbeiten |
| Formen: Rechteck (R), Ellipse (E), Linie (L) | Mit/ohne Füllung; ⇧ = Quadrat/Kreis bzw. 45° |
| Nummer (N) | Fortlaufende Schritt-Nummern (①②③ …) |
| Marker (H) | Halbtransparenter Textmarker |
| Füllen (F) | Farbeimer: füllt zusammenhängende Fläche ähnlicher Farbe (Toleranz einstellbar) |
| Unkenntlich: Weichzeichnen (B), Verpixeln (X) | Bereiche unkenntlich machen, Stärke einstellbar |
| Zuschneiden (C) | Rahmen mit 8 Griffen verkleinern/verschieben, Formate (Frei, Original, 1:1, 4:3, 16:9), mit Enter bestätigen |
| Mehr: Stift (P), Spotlight (S) | Freihand; Bereich hervorheben, Rest abdunkeln |

Eigenschaften (rechtes Panel): Farbe mit Palette, Füllung, Stärke, Schriftgröße, Schatten.
Alle Anmerkungen bleiben **nicht-destruktiv** editierbar – auch Weichzeichnen, Verpixeln und Füllen.

**Bild:** Drehen (links/rechts), Spiegeln (horizontal/vertikal), Größe ändern (%).
**Effekte** (beim Export angewendet): Rahmen, runde Ecken, Schlagschatten.
**Allgemein:** Rückgängig/Wiederholen (⌘Z / ⌘⇧Z, 100 Schritte), Duplizieren (⌘D),
Löschen (⌫), Zoom (⌘ + Scrollen / Pinch, „Einpassen“, „100 %“), Verschieben (Scrollen).

### 2.3 Ausgabe

| Aktion | Kürzel | Beschreibung |
|---|---|---|
| Kopieren | ⌘C | Fertiges Bild in die Zwischenablage |
| Schnell speichern | ⌘S | Ohne Dialog in den Zielordner (Standard `~/Pictures/Kadra`) |
| Speichern unter … | ⌘⇧S | Dialog mit allen Formaten |
| Exportieren als ▸ | – | Format gezielt wählen |

Formate: **PNG, JPEG, TIFF, WebP, GIF, BMP, PDF**. Bei JPEG/PDF erscheint ein Dialog mit
Qualitätsregler und geschätzter Dateigröße. Formate ohne Transparenz erhalten weißen Hintergrund.

### 2.4 Bibliothek

Jede Aufnahme wird automatisch gespeichert und erscheint in der Leiste unten.
Gespeichert werden Originalbild, Anmerkungen (JSON) und Vorschaubild – Aufnahmen bleiben also
jederzeit weiter bearbeitbar. Ort: `~/Library/Application Support/Kadra/library/`.
Rechtsklick auf ein Vorschaubild: Öffnen oder Löschen.

### 2.5 Menüleiste und Hintergrundbetrieb

- **Fenster schließen** blendet Kadra aus; die App läuft mit Symbol in der Menüleiste weiter
  (kein Dock-Symbol). Globale Kürzel funktionieren weiterhin.
- Menü des Symbols: Aufnahmemodi, Kadra öffnen, Letzte Aufnahme kopieren, Einstellungen, **Kadra beenden**.
- Abgebrochene Aufnahmen aus dem Hintergrund lassen Kadra im Hintergrund; fertige öffnen den Editor.

### 2.6 Einstellungen

Zielordner, Format fürs Schnellspeichern, JPEG-/PDF-Qualität, automatisches Kopieren nach der
Aufnahme, globale Kürzel ein/aus und **frei belegbar** (Kombination per Tastendruck aufnehmen,
Konflikterkennung). Gespeichert in `~/Library/Application Support/Kadra/settings.json`.

---

## 3. Tastenkürzel (Übersicht)

| Bereich | Kürzel |
|---|---|
| Global (Standard) | ⌃⇧1 Bereich · ⌃⇧2 Fenster · ⌃⇧3 Bildschirm · ⌃⇧4 Alle Bildschirme |
| Overlay | Tab/⇧Tab Fenster dahinter · L Fensterliste · Enter ganzer Bildschirm · Esc Abbrechen |
| Editor-Werkzeuge | V A T R E L N H F B X C P S |
| Bearbeiten | ⌘Z / ⌘⇧Z · ⌘D · ⌫ · Pfeiltasten · Esc |
| Datei | ⌘C Kopieren · ⌘S Speichern · ⌘⇧S Speichern unter · ⌘O Öffnen |

---

## 4. Architektur

```
src/
├── main.rs       Start, Fenster-Optionen, Dock-Icon
├── app.rs        Hauptanwendung: Aufnahme-Ablauf, Leiste, Bibliothek, Dialoge, Menüleiste
├── capture.rs    Bildschirmaufnahme (xcap), Fensterliste je Monitor
├── overlay.rs    Auswahl-Overlay: Bereich/Fenster, Lupe, Tab-Wechsel, Fensterliste
├── editor.rs     Editor: Werkzeuge, Interaktion, Undo, Zuschneiden, Bildoperationen
├── doc.rs        Dokumentmodell (Bild + Anmerkungen, serialisierbar)
├── render.rs     Export-Rendering (tiny-skia, ab_glyph), Effekte, Füllen
├── library.rs    Bibliothek, Einstellungen, Export aller Formate inkl. PDF-Writer
├── hotkeys.rs    Globale, frei belegbare Tastenkürzel (global-hotkey)
├── tray.rs       Menüleisten-Symbol und -Menü (tray-icon)
├── platform.rs   macOS-spezifisch: Berechtigung, Fensterliste/-aufnahme (CoreGraphics), Fensterebene, Dock
├── ui_kit.rs     Stil, Signaturfarbe, Leisten- und Dialog-Buttons
└── icon.rs       App-Icon (Dock, Iconset)
```

**Datenfluss:** Aufnahme → `Document` (Originalbild + Liste von `Annotation`) → Editor
(Vorschau mit egui, Raster-Effekte als Textur) → `render::render` (tiny-skia) → Export/Zwischenablage.
Anmerkungen werden nie ins Original gerechnet, daher bleibt alles bis zum Export änderbar.

**Wichtige Abhängigkeiten:** eframe/egui 0.33 (UI), xcap (Monitoraufnahme), tiny-skia + ab_glyph
(Rendering), image (Formate), global-hotkey, tray-icon, arboard (Zwischenablage), rfd (Dateidialoge).

**Build-Skripte:** `bundle.sh` (App-Paket, Icon, Signatur, optional Universal-Binary),
`make_dmg.sh` (DMG mit Finder-Layout), `examples/make_icon.rs` (Iconset).

**Tests:** `cargo test` – Rendering aller Formen, Füllalgorithmus, Export aller Formate,
JPEG-Qualität, Zuschneide-Logik, Dock-Icon, Fensterliste.

---

## 5. Bekannte Grenzen

- Nur macOS getestet (Code ist teilweise plattformneutral, siehe Erweiterungen).
- Release-DMG nur für Apple Silicon und nicht notarisiert (Gatekeeper-Warnung beim ersten Start).
- Minimierte Fenster liefert macOS meist nicht als Bild.
- Auswahl-Overlay jeweils auf dem Bildschirm unter dem Mauszeiger.
- ⌘V fügt ein reines Bild aus der Zwischenablage nicht ein (egui liefert nur Text) – Button „Einfügen“ nutzen.
- Füllkanten sind nicht geglättet; WebP wird verlustfrei gespeichert.

---

## 6. Mögliche Erweiterungen

**Aufnahme**
- Scrolling-Capture (lange Webseiten/Dokumente zusammensetzen)
- Overlay über alle Bildschirme gleichzeitig
- Countdown-Anzeige bei verzögerter Aufnahme, „Letzten Bereich wiederholen“
- Mauszeiger optional mit aufnehmen
- Texterkennung (OCR) über das macOS-Vision-Framework, Text kopieren
- Videoaufnahme/GIF (bewusst ausgelassen, xcap bietet einen Video-Recorder)

**Editor**
- Callouts/Sprechblasen, Stempel/Sticker, Lupen-Werkzeug (vergrößerter Ausschnitt)
- Pipette (Farbe aus dem Bild übernehmen), eigene Farbpaletten und Stil-Vorlagen
- Schriftarten-Auswahl, fett/kursiv, Textausrichtung
- Effekte: abgerissene Kanten, Perspektive, Rahmen mit Verlauf, Wasserzeichen
- Mehrfachauswahl, Ebenen-Reihenfolge per Drag & Drop, Ausrichtungshilfen/Raster
- Geglättete Füllkanten, „Smart Move“ (Objekte im Bild verschieben)

**Ausgabe und Teilen**
- Teilen-Link (Cloud-Upload), Teilen-Menü von macOS, Mail/Messages
- Mehrseitiges PDF aus mehreren Aufnahmen, Batch-Export
- Export der Anmerkungen als SVG, verlustbehaftetes WebP, HEIC/AVIF
- Schnellspeichern mit Dateinamen-Vorlagen

**Bibliothek**
- Suche, Tags, Favoriten, Filter nach Datum/App
- Speichergrenze bzw. automatisches Aufräumen alter Aufnahmen

**App und Plattform**
- Notarisierung und Developer-ID-Signatur für reibungslose Installation
- Universal-Binary (Apple Silicon + Intel) als Standard-Release
- Automatische Updates (z. B. Sparkle), Start bei Anmeldung
- Englische Oberfläche / Mehrsprachigkeit
- Windows- und Linux-Unterstützung (xcap, tray-icon und global-hotkey sind plattformübergreifend;
  macOS-Spezifika liegen gekapselt in `platform.rs`)
