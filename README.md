# FileRune (Rust)

Zeilengetreue Rust-Fassung der SwiftUI-App FileRune — derselbe Aufbau, dieselben
Kürzel, dieselbe Such- und Filterlogik. Oberfläche: [egui/eframe](https://github.com/emilk/egui).
Icons und Typografie kommen aus macOS selbst (SF Symbols, Finder-Dateisymbole,
SF Pro/SF Mono), damit die App neben dem Original nicht auffällt.

Läuft auf **macOS, Windows und Linux**. Fertige Bauten hängen an jedem
[Release](https://github.com/Lomtech/filerune-rs/releases); wer selbst baut,
braucht [Rust](https://rustup.rs).

### macOS

```bash
./bundle.sh && open build/FileRune.app
```

### Windows

`bundle.sh` ist ein Bash-Skript für das macOS-Bundle und läuft hier nicht — dafür
gibt es `bundle.ps1`. Es legt `build\FileRune.exe` ab, damit auf beiden Systemen
derselbe Pfad herauskommt:

```powershell
.\bundle.ps1
.\build\FileRune.exe
```

Weigert sich PowerShell mit „Die Datei kann nicht geladen werden, da die
Ausführung von Skripts auf diesem System deaktiviert ist", dann blockiert die
Ausführungsrichtlinie — das ist die Voreinstellung auf Windows-Clients und hat
nichts mit dem Skript zu tun. Einmalig nur für dieses Fenster umgehen:

```powershell
powershell -ExecutionPolicy Bypass -File .\bundle.ps1
```

Fest ins Benutzerprofil samt Startmenü-Eintrag (`%LOCALAPPDATA%\Programs\FileRune`):

```powershell
.\bundle.ps1 -Install
```

Ohne Skript geht es genauso — nur heißt die Datei dann nach dem Cargo-Paket,
also klein und unter `target\`:

```powershell
cargo build --release
.\target\release\filerune.exe
```

### Linux

Erst die Entwicklungspakete für X11/Wayland, sonst findet winit kein
Fenster-Backend (die vollständige Liste steht in `.github/workflows/release.yml`):

```bash
sudo apt-get install -y libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev
```

```bash
cargo build --release && ./target/release/filerune
```

Mit Startereintrag ins Benutzerprofil:

```bash
install -Dm755 target/release/filerune ~/.local/bin/filerune && install -Dm644 icon.png ~/.local/share/icons/filerune.png
```

### Icons

Drei Dateien, drei Zwecke — alle aus dem Original übernommen:

| Datei | Wofür |
|---|---|
| `Icon.icns` | das macOS-App-Bundle |
| `icon.png` | das Programmsymbol zur Laufzeit — **nur Windows und Linux** |
| `icon.ico` | die Windows-`.exe` (über `build.rs`) |

Das Zusammenspiel ist heikler, als es aussieht. Ohne gesetztes Icon legt eframe
**sein eigenes Standardsymbol** als Programmsymbol an — auf macOS überschreibt das
sogar `CFBundleIconFile` aus dem Bundle. Setzt man es dort aber programmatisch,
zeichnet das Dock es randlos und damit **größer als jedes Symbol daneben**. Deshalb
bekommt macOS ein leeres `IconData::default()`: das weist eframe an, das
Programmsymbol nicht anzufassen, sodass das Bundle-Icon gilt und macOS den üblichen
Rand anlegt. Windows und Linux bekommen `icon.png`.

Fehlt `Icon.icns`, bricht `bundle.sh` ab — lieber laut scheitern als still eine App
ohne Icon ausliefern.

## Aufbau

| Datei | Entspricht | Inhalt |
|---|---|---|
| `src/entry.rs` | `Entry.swift` | Zeilenmodell, Größen- und Datumsformat, ISO-Tag |
| `src/scanner.rs` | `DirectoryScanner.swift` | flaches Einlesen eines Ordners |
| `src/fuzzy.rs` | `FuzzyMatch.swift` | Subsequenz-Match mit Positionsboni |
| `src/filter.rs` | `AppModel.columnFilter` | `art:` · `>1mb` · `datum:` |
| `src/search.rs` | `DirectorySearch.swift` | rekursive Suche, parallel, abbrechbar |
| `src/content.rs` | `ContentSearch.swift` | Volltextsuche in Dateien |
| `src/ops.rs` | Dateioperationen in `AppModel` | umbenennen, duplizieren, einfügen, Papierkorb |
| `src/integrate.rs` | `openInZed`/`openInTerminal` | fremde Apps, Zwischenablage |
| `src/platform/macos.rs` | `IconProvider.swift` | Finder-Icons + SF Symbols als egui-Texturen |
| `src/platform/generic.rs` | — | dieselben Symbole für Windows/Linux, selbst gezeichnet |
| `src/state.rs` | `AppModel.swift` | Navigation, Sortierstapel, Auswahl, Favoriten |
| `src/ui.rs` | `ContentView.swift` | Leisten, Liste, Blätter, Tastatur |
| `bundle.sh` / `bundle.ps1` | — | App-Bundle auf macOS, `.exe` und Installation auf Windows |
| `examples/bench.rs` | — | Messung der Suchpfade, damit Optimierungen an Zahlen hängen |

## Bedienung

Das Filterfeld hat **immer** den Fokus; die Pfeiltasten steuern trotzdem die
Liste, nie die Schreibmarke. Das ist die Signatur der App und entspricht dem
`doCommandBySelector`-Routing aus `FilterField.swift`.

Getippter Text sucht rekursiv nach Namen. Erkennt die Eingabe einen Filter, gilt
stattdessen dieser — ebenfalls rekursiv:

| Eingabe | Wirkung |
|---|---|
| `.pdf` · `.html` · `.go` | genau diese Dateiendung |
| `art:pdf` · `typ:ordner` | Art oder Endung **enthält** den Text |
| `>1mb` · `<500kb` · `größe:>2gb` | Größe |
| `datum:2026-05` · `datum:>2026-06-01` | Änderungsdatum |

Die Kurzform mit Punkt ist eng gefasst: ein Endungswort ohne weitere Punkte, damit
`.tar.gz` und `.zshrc` in der Namenssuche bleiben. Ein vorangestelltes `filtern:`
wird geschluckt — der Platzhalter des Originals sah so aus, und Leute tippten ihn ab.

### Suche im Dateiinhalt

Ab zwei Zeichen wird zusätzlich **in den Dateien** gesucht, nicht nur in ihren
Namen — ohne Suchindex, direkt gelesen, damit es in jedem Ordner und auf allen
drei Systemen gleich funktioniert. Solche Treffer stehen in Akzentfarbe unter dem
Dateinamen, mit Zeilennummer und Fundstelle; die Statuszeile sagt, wie viele der
Treffer daher kamen.

Gelesen wird nur, was nach Text aussieht (Endungsliste in `src/content.rs`, dazu
Namen wie `Makefile`) und höchstens 16 MB groß ist; ein Null-Byte im ersten
Kilobyte gilt trotz passender Endung als Binärdatei. **Namenstreffer stehen immer
über Inhaltstreffern** — sonst findet man die Datei nicht wieder, deren Name genau
passt. Die Sofortanzeige beim Tippen sucht bewusst nur im Namen: sie läuft auf dem
UI-Thread, dort darf nichts von der Platte gelesen werden. Der Tiefenscan im
Hintergrund nimmt dann die Inhalte dazu.

PDFs bleiben außen vor. Die SwiftUI-Fassung liest sie über PDFKit; in Rust wäre
das eine große, wacklige Abhängigkeit.

### Geschwindigkeit

Gemessen mit `cargo run --release --example bench -- <Ordner>` über einen Baum
mit 10 884 Dateien / 664 MB auf acht Kernen:

| | vorher | jetzt |
|---|---|---|
| Endungsfilter `.rs` | 62 ms | 13 ms |
| Inhaltssuche, Dateicache warm | ~315 ms | ~80 ms |
| Inhaltssuche, Dateicache kalt | 684 ms | 382 ms |
| Namenssuche | 40 ms | 33 ms |

Drei Hebel, in dieser Reihenfolge wirksam:

1. **Kein `stat` für Aussichtslose.** Endung und Art stehen im Dateinamen; die
   Vorauswahl entscheidet ohne Metadaten. Dazu wird der Ordnerpfad eines Treffers
   erst gebaut, wenn es einer ist — vorher legte die Inhaltssuche für jede Datei
   im Baum einen String an, den sie wegwarf.
2. **Kein `to_lowercase()` je Zeile.** Aszii-Suchbegriffe werden byteweise
   verglichen, Kandidaten findet `memchr` mit SIMD — und zwar über das
   *seltenste* Zeichen des Suchbegriffs, nicht das erste.
3. Die Vorauswahl darf nie strenger sein als der eigentliche Vergleich, sonst
   fehlen Treffer. Ein Test hält das fest; zwei solche Fehler steckten schon drin
   (`art:dokument` und `.app` auf Paketordnern).

**Das Einlesen eines Ordners läuft nebenläufig.** 50 000 Einträge brauchen ~160 ms;
synchron wäre das ein sichtbar eingefrorenes Fenster bei jedem Wechsel in einen
großen Ordner. Ein noch laufendes Einlesen wird über einen Generationszähler
entwertet, sobald weitergeklickt wurde, und eine Auswahl, die danach gesetzt
werden soll (etwa die frisch umbenannte Datei), wartet auf das Ergebnis.

Sortiert und verglichen wird ohne Allokation: `a.name.to_lowercase()` je Vergleich
sind bei 50 000 Einträgen hunderttausende Strings pro Sortierung.

Warm ist die Inhaltssuche mit ~80 ms **unabhängig vom Suchbegriff** — auch einer
ohne einen einzigen Treffer, der jede Datei ganz durchlaufen muss, kostet nicht
mehr. Das Suchen selbst ist damit nicht mehr der Engpass; die verbleibende Zeit
ist Plattenzugriff. Zum Vergleich: dieselben Dateien einmal einlesen, ohne jede
Suche und einzeln statt parallel, dauert 1050 ms.

### Warum sich die Suche schnell anfühlt

Ab `/Users` liegen 6,7 GB in 36 000 Dateien. Ein vollständiger Inhaltsdurchgang
dauert dort ~1,9 s, und daran lässt sich wenig ändern — dieselbe Suche kostet
kalt wie warm gleich viel, weil das nicht mehr in den Dateicache passt. Was sich
ändern lässt, ist, **wann man etwas sieht**:

- **Teillieferungen.** Jeder Arbeiter meldet seine Treffer, sobald ein Unterbaum
  fertig ist, statt bis zum Ende zu sammeln. Die erste Zeile steht nach **~10 ms**.
- **Zwei Durchgänge.** Erst nur Namen (~0,4 s), dann die Inhalte (~1,9 s). Vorher
  erledigte ein Durchgang beides und lieferte am Ende — die Namenstreffer hingen
  also an der Inhaltssuche und erschienen erst mit ihr zusammen. Genau das ließ
  die Suche viel langsamer wirken, als sie ist.
- Der Cursor bleibt beim Nachliefern oben stehen, solange man ihn nicht selbst
  bewegt hat. Sonst wanderte er mit der wachsenden Liste nach unten und die
  Ansicht scrollte von allein weg.

### Parallelität

Ein Arbeiter je Kern zieht sich Unterbäume aus einer gemeinsamen Warteschlange.
Die Warteschlange entsteht, indem der Baum so weit aufgeklappt wird, bis genug
Ordner für alle Kerne da sind (höchstens drei Ebenen) — bei einem Ordner mit nur
zwei Unterordnern läge sonst der Großteil der Maschine brach, was beim Lesen von
Dateiinhalten richtig weh tut. Zurückgegeben wird dabei **nur die tiefste**
aufgeklappte Ebene; gäbe man die darüberliegenden mit, liefe deren Unterbaum
zweimal und jeder Treffer erschiene doppelt.

| Taste | Wirkung |
|---|---|
| Tippen | filtert; ohne Treffer im Ordner wird rekursiv weitergesucht |
| `↑` `↓` | Auswahl bewegen · `⇧` erweitert · `⇞` `⇟` in 15er-Schritten |
| `←` `→` | Ordner hoch / in den markierten Ordner |
| `⏎` | öffnen · `⌘⏎` umbenennen · `⌘D` duplizieren · `⌘⌫` Papierkorb |
| `⌘⇧E` | in Zed öffnen · `⌘⇧T` Terminal · `⌘⇧R` Finder |
| `⌘←` `⌘→` `⌘↑` | zurück / vorwärts / hoch · `⌘⇧H` Persönlicher Ordner |
| `⌘⇧G` | Gehe zu Ordner · `⌘⇧N` neuer Ordner · `⌘⇧D` zu Favoriten |
| `⌘⌥1/2/3` | nach Name / Datum / Größe sortieren · `⌘⌥S` Richtung |
| `⌘⇧.` | versteckte Dateien · `⌃⌘S` Seitenleiste · `⌘R` neu laden |
| `Esc` | Filter leeren (auch über den Löschknopf im Suchfeld) |

Der Sortierstapel ist mehrstufig: ein Klick auf eine andere Spalte schiebt die
bisherige nach hinten, wo sie Gleichstände bricht (kleine Ziffer im Spaltenkopf).

## Was pro System verschieden ist

Alles hinter `src/platform/`. Auf macOS kommen Dateisymbole, SF Symbols,
Systemschriften und Akzentfarbe echt aus AppKit — dort ist die App eine exakte
Kopie der SwiftUI-Fassung. Auf Windows und Linux zeichnet sie dieselben Symbole
selbst: kein Icon-Theme, keine mitgelieferten Bilddateien, keine Lizenzfrage, und
bei jeder Auflösung scharf. Dateien bekommen dort einen Farbstreifen nach Endung
statt eines Typ-Icons.

Kürzel sind überall gleich belegt (egui bildet `COMMAND` auf ⌘ bzw. Strg ab), nur
anders beschriftet: „⌘⇧E" heißt unter Windows „Strg+Umschalt+E".

Fremde Apps: `open` auf macOS, `explorer`/`start`/`wt.exe` unter Windows,
`xdg-open` und die üblichen Terminal-Namen unter Linux. Zed wird über die
Bundle-ID, den PATH-Namen (`zed`, unter manchen Distributionen `zeditor`), die
Installationspfade und zuletzt Flatpak gesucht.

## Zustand

Ordner, Favoriten, Sortierung, Seitenleiste, versteckte Dateien **und die
Fenstergröße/-position** — das Fenster kommt wieder, wie es zuletzt stand.
Geschrieben wird erst, wenn das Ziehen ~400 ms ruht, damit nicht bei jedem Pixel
die Datei angefasst wird; Vollbild und Zoom werden bewusst nicht gemerkt, sonst
startet die App als bildschirmgroßes Fenster, das gar kein Vollbild ist. Beim
ersten Start: 820 × 520, zugleich die Mindestgröße aus `ContentView.swift`.

| System | Ort |
|---|---|
| macOS | `~/Library/Application Support/FileRune/state.json` |
| Windows | `%APPDATA%\FileRune\state.json` |
| Linux | `$XDG_CONFIG_HOME/filerune/state.json` (sonst `~/.config`) |

Eigene Bundle-ID (`com.lom.filerune-rs`), damit sich die beiden Fassungen auf dem
Mac nicht in die Quere kommen.

## Ausliefern

`.github/workflows/release.yml` baut bei jedem Tag `v*` auf drei Systemen und
hängt die Ergebnisse an ein Release: **macOS als Universal-Binary** (arm64 + x86_64
per `lipo` — die SwiftUI-Fassung war nur arm64 und lief auf Intel-Macs gar nicht),
Linux als `.tar.gz` mit `.desktop`-Eintrag, Windows als `.zip` mit Icon in der `.exe`.

Die Bauten sind **unsigniert**. Beim ersten Start meldet sich Gatekeeper
(Rechtsklick → „Öffnen") bzw. SmartScreen („Weitere Informationen" → „Trotzdem
ausführen"). Ohne Warnung geht es erst mit Apple-Notarisierung (Developer Program,
99 $/Jahr) und einem Windows-Code-Signing-Zertifikat.

## Abweichung von der SwiftUI-Fassung

Spaltenfilter sind hier **rekursiv**, im Original galten sie nur für den gerade
angezeigten Ordner. Damit fand `art:pdf` in jedem Elternordner nichts — in
`/Users` etwa, wo bloß zwei Unterordner liegen. Der Platzhalter des Feldes
verspricht „auch Unterordner"; jetzt hält der Filter das auch. Rückgängig zu machen
wäre es in `AppState::on_filter_changed`, indem der Tiefenscan für
`Query::Column` entfällt.

## Was bewusst fehlt

Gegenüber der SwiftUI-Fassung nicht enthalten: Speicher-Analyse mit Diagramm,
Ordner-Icons/Tints, Quick-Look-Vorschau,
Sparkle-Updates und das Lizenz-Gate.

Kopieren/Einfügen läuft über eine app-interne Ablage; die Pfade landen zusätzlich
als Text in der System-Zwischenablage. Dateien aus dem Finder einzufügen geht
damit noch nicht — dafür bräuchte es `NSPasteboard` mit `NSURL`-Objekten.

Das Einlesen eines Ordners läuft synchron. Bei den üblichen Größen ist das unter
einer Millisekunde (die Statuszeile zeigt es an); ein Ordner mit sehr vielen
Zehntausend Einträgen würde kurz haken.
