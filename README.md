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
| `src/ops.rs` | Dateioperationen in `AppModel` | umbenennen, duplizieren, einfügen, Papierkorb |
| `src/integrate.rs` | `openInZed`/`openInTerminal` | fremde Apps, Zwischenablage |
| `src/platform/macos.rs` | `IconProvider.swift` | Finder-Icons + SF Symbols als egui-Texturen |
| `src/platform/generic.rs` | — | dieselben Symbole für Windows/Linux, selbst gezeichnet |
| `src/state.rs` | `AppModel.swift` | Navigation, Sortierstapel, Auswahl, Favoriten |
| `src/ui.rs` | `ContentView.swift` | Leisten, Liste, Blätter, Tastatur |
| `bundle.sh` / `bundle.ps1` | — | App-Bundle auf macOS, `.exe` und Installation auf Windows |

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
Ordner-Icons/Tints, Inhaltssuche über Spotlight, Quick-Look-Vorschau,
Sparkle-Updates und das Lizenz-Gate.

Kopieren/Einfügen läuft über eine app-interne Ablage; die Pfade landen zusätzlich
als Text in der System-Zwischenablage. Dateien aus dem Finder einzufügen geht
damit noch nicht — dafür bräuchte es `NSPasteboard` mit `NSURL`-Objekten.

Das Einlesen eines Ordners läuft synchron. Bei den üblichen Größen ist das unter
einer Millisekunde (die Statuszeile zeigt es an); ein Ordner mit sehr vielen
Zehntausend Einträgen würde kurz haken.
