//! Portierung von AppModel.swift: Navigation, Sortierung, Auswahl, Filter,
//! Favoriten, Hintergrundsuche und die Persistenz (statt UserDefaults eine
//! JSON-Datei in Application Support).

use crate::entry::Entry;
use crate::filter::ColumnFilter;
use crate::search::Query;
use crate::{integrate, ops, scanner, search};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const SEARCH_LIMIT: usize = 1000;
/// Ab so vielen Zeichen wird zusätzlich im Dateiinhalt gesucht.
const MIN_CONTENT_QUERY: usize = 2;
/// Wartezeit, bis der Tiefenscan startet — die lokalen Treffer stehen sofort.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(200);
const FLASH_DURATION: Duration = Duration::from_millis(1600);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortKey {
    Name,
    Date,
    Size,
    Kind,
}

impl SortKey {
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Name => "Name",
            SortKey::Date => "Änderungsdatum",
            SortKey::Size => "Größe",
            SortKey::Kind => "Art",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SortCriterion {
    pub key: SortKey,
    pub ascending: bool,
}

#[derive(Serialize, Deserialize)]
struct Persisted {
    current_dir: Option<String>,
    show_hidden: bool,
    show_sidebar: bool,
    sort_stack: Vec<SortCriterion>,
    favorites: Vec<String>,
    /// Fenstergröße und -position beim letzten Beenden.
    #[serde(default)]
    window_size: Option<[f32; 2]>,
    #[serde(default)]
    window_pos: Option<[f32; 2]>,
}

/// Startgröße, wenn noch nichts gespeichert ist — die Mindestgröße aus
/// ContentView.swift. Kompakt; wer mehr will, zieht das Fenster auf, und ab
/// dann wird die Größe gemerkt.
pub const DEFAULT_WINDOW_SIZE: [f32; 2] = [820.0, 520.0];
pub const MIN_WINDOW_SIZE: [f32; 2] = [820.0, 520.0];

/// Liest die gemerkte Fenstergeometrie, bevor überhaupt ein Fenster existiert.
pub fn saved_window_geometry() -> (Option<[f32; 2]>, Option<[f32; 2]>) {
    match load_persisted() {
        Some(p) => (p.window_size, p.window_pos),
        None => (None, None),
    }
}

pub struct AppState {
    pub current_dir: PathBuf,
    all_entries: Vec<Entry>,
    search_results: Vec<Entry>,
    /// Die tatsächlich gezeigte Liste: gefiltert und sortiert.
    pub view: Vec<Entry>,

    pub filter: String,
    /// Wahr, wenn der Filter ein Spaltenausdruck war (art:/größe/datum) — dann
    /// gab es keinen Tiefenscan, und die Statuszeile sagt das auch.
    pub column_filtered: bool,
    pub is_searching: bool,
    /// Läuft gerade auch eine Suche im Dateiinhalt?
    pub searching_contents: bool,
    /// Der Namensdurchgang ist fertig, die Inhaltssuche läuft noch.
    pub names_done: bool,
    /// Hat der Nutzer den Cursor selbst bewegt? Solange nicht, bleibt er beim
    /// Nachliefern von Treffern oben stehen, statt mit der wachsenden Liste
    /// nach unten zu wandern.
    pub user_moved_selection: bool,

    pub selected_index: usize,
    pub selection: HashSet<PathBuf>,
    /// Anker für Umschalt-Bereichsauswahl.
    anchor_index: usize,

    pub sort_stack: Vec<SortCriterion>,
    pub show_hidden: bool,
    pub show_sidebar: bool,
    pub favorites: Vec<PathBuf>,

    back_stack: Vec<PathBuf>,
    forward_stack: Vec<PathBuf>,

    pub error: Option<String>,
    flash: Option<(String, Instant)>,
    pub load_ms: f64,

    /// App-interne Ablage für Kopieren/Einfügen von Dateien.
    pub clipboard: Vec<PathBuf>,

    /// Zuletzt gesehene Fenstergeometrie und wann sie sich zuletzt änderte.
    /// Beim Ziehen am Fensterrand ändert sie sich jeden Frame — deshalb wird
    /// erst geschrieben, wenn es kurz ruhig ist.
    window_size: Option<[f32; 2]>,
    window_pos: Option<[f32; 2]>,
    geometry_dirty_since: Option<Instant>,

    generation: Arc<AtomicU64>,
    search_tx: Sender<SearchBatch>,
    search_rx: Receiver<SearchBatch>,

    /// Das Einlesen läuft auf einem eigenen Thread: 50 000 Einträge dauern
    /// ~180 ms, und die hätte man sonst als eingefrorenes Fenster.
    scan_generation: Arc<AtomicU64>,
    scan_tx: Sender<ScanResult>,
    scan_rx: Receiver<ScanResult>,
    /// Was nach dem nächsten Einlesen ausgewählt werden soll.
    after_load: Option<AfterLoad>,
    pub is_loading: bool,
}

/// Was beim Auslösen des Papierkorbs zu tun ist.
pub enum TrashPlan {
    /// Nichts ausgewählt.
    Nothing,
    /// Nur Dateien — die gehen direkt weg, sie sind in der Liste sichtbar.
    Immediate(Vec<PathBuf>),
    /// Mindestens ein Ordner ist dabei: erst fragen.
    Confirm { paths: Vec<PathBuf>, summary: String },
}

/// Auswahl, die erst gesetzt werden kann, wenn die Einträge da sind.
enum AfterLoad {
    Name(String),
    Index(usize),
}

/// Eine Lieferung der Hintergrundsuche.
struct SearchBatch {
    generation: u64,
    entries: Vec<Entry>,
    kind: BatchKind,
}

enum BatchKind {
    /// Teilergebnis eines fertigen Unterbaums — wird angehängt, damit die
    /// ersten Zeilen sofort dastehen.
    Partial,
    /// Ende eines Durchgangs: ersetzt die Liste durch die endgültigen,
    /// nach Rang gekürzten Treffer. Nur der letzte Durchgang beendet die Suche.
    Complete { last: bool },
}

struct ScanResult {
    generation: u64,
    dir: PathBuf,
    entries: Vec<Entry>,
    error: Option<String>,
    took_ms: f64,
}

impl AppState {
    pub fn new() -> Self {
        let home = home_dir();
        let saved = load_persisted();

        let current_dir = saved
            .as_ref()
            .and_then(|p| p.current_dir.clone())
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| home.clone());

        let favorites = match saved.as_ref() {
            Some(p) => p.favorites.iter().map(PathBuf::from).filter(|p| p.exists()).collect(),
            None => [
                home.clone(),
                home.join("Desktop"),
                home.join("Documents"),
                home.join("Downloads"),
                home.join("Developer"),
            ]
            .into_iter()
            .filter(|p| p.exists())
            .collect(),
        };

        let (search_tx, search_rx) = channel();
        let (scan_tx, scan_rx) = channel();
        let mut state = AppState {
            current_dir,
            all_entries: Vec::new(),
            search_results: Vec::new(),
            view: Vec::new(),
            filter: String::new(),
            column_filtered: false,
            is_searching: false,
            searching_contents: false,
            names_done: false,
            user_moved_selection: false,
            selected_index: 0,
            selection: HashSet::new(),
            anchor_index: 0,
            sort_stack: saved
                .as_ref()
                .map(|p| p.sort_stack.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    vec![SortCriterion { key: SortKey::Name, ascending: true }]
                }),
            show_hidden: saved.as_ref().map(|p| p.show_hidden).unwrap_or(false),
            show_sidebar: saved.as_ref().map(|p| p.show_sidebar).unwrap_or(true),
            favorites,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            error: None,
            flash: None,
            load_ms: 0.0,
            clipboard: Vec::new(),
            window_size: saved.as_ref().and_then(|p| p.window_size),
            window_pos: saved.as_ref().and_then(|p| p.window_pos),
            geometry_dirty_since: None,
            generation: Arc::new(AtomicU64::new(0)),
            search_tx,
            search_rx,
            scan_generation: Arc::new(AtomicU64::new(0)),
            scan_tx,
            scan_rx,
            after_load: None,
            is_loading: false,
        };
        state.load();
        state
    }

    // MARK: - Laden

    /// Stößt das Einlesen an. Läuft auf einem eigenen Thread; das Ergebnis holt
    /// `poll_scan` ab. Ein noch laufendes Einlesen wird über den
    /// Generationszähler entwertet, wenn schon weitergeklickt wurde.
    pub fn load(&mut self) {
        let generation = self.scan_generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.is_loading = true;

        let dir = self.current_dir.clone();
        let hidden = self.show_hidden;
        let tx = self.scan_tx.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            let (entries, error) = match scanner::scan(&dir, hidden) {
                Ok(entries) => (entries, None),
                Err(e) => (Vec::new(), Some(format!("{}: {e}", dir.display()))),
            };
            let _ = tx.send(ScanResult {
                generation,
                dir,
                entries,
                error,
                took_ms: started.elapsed().as_secs_f64() * 1000.0,
            });
        });
    }

    /// Holt fertige Einlese-Ergebnisse ab. `true`, wenn sich etwas geändert hat.
    pub fn poll_scan(&mut self) -> bool {
        let mut changed = false;
        while let Ok(res) = self.scan_rx.try_recv() {
            if res.generation != self.scan_generation.load(Ordering::SeqCst) {
                continue; // veraltet — es wurde schon weitergeklickt
            }
            // Ergebnis eines Ordners, den wir gar nicht mehr anzeigen: verwerfen.
            if res.dir != self.current_dir {
                continue;
            }
            self.all_entries = res.entries;
            self.error = res.error;
            self.load_ms = res.took_ms;
            self.is_loading = false;
            self.on_filter_changed();
            match self.after_load.take() {
                Some(AfterLoad::Name(name)) => self.select_by_name(&name),
                Some(AfterLoad::Index(idx)) => {
                    if !self.view.is_empty() {
                        self.select_only(idx.min(self.view.len() - 1));
                    }
                }
                None => {}
            }
            changed = true;
        }
        changed
    }

    pub fn total_count(&self) -> usize {
        self.all_entries.len()
    }

    /// Wie viele der angezeigten Treffer aus dem Dateiinhalt kamen.
    pub fn content_hits(&self) -> usize {
        self.view.iter().filter(|e| e.matched_line.is_some()).count()
    }

    // MARK: - Filter und Suche

    pub fn set_filter(&mut self, text: String) {
        if text == self.filter {
            return;
        }
        self.filter = text;
        self.on_filter_changed();
    }

    fn on_filter_changed(&mut self) {
        // Jede Änderung entwertet eine noch laufende Suche.
        let gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.column_filtered = false;

        if self.filter.is_empty() {
            self.is_searching = false;
            self.searching_contents = false;
            self.names_done = false;
            self.search_results.clear();
            self.rebuild_view();
            self.reset_selection_to_top();
            return;
        }

        // Spaltenfilter (art:/größe/datum) erkennen; sonst Namenssuche.
        // Beide laufen danach denselben Weg: erst die Treffer im aktuellen
        // Ordner, dann der Tiefenscan.
        //
        // ABWEICHUNG von der SwiftUI-Fassung: dort galten Spaltenfilter NUR für
        // den aktuellen Ordner. Damit fand `art:pdf` in jedem Elternordner
        // nichts — in /Users etwa, wo nur zwei Unterordner liegen. Der
        // Platzhalter des Feldes verspricht „auch Unterordner", also gilt das
        // jetzt für den Filter genauso.
        // Inhalte erst ab zwei Zeichen durchsuchen: bei einem einzelnen
        // Buchstaben passt praktisch jede Textdatei, und das Lesen kostet
        // dann umsonst.
        let with_contents = self.filter.chars().count() >= MIN_CONTENT_QUERY;

        let (instant, deep) = match ColumnFilter::parse(&self.filter) {
            Some(f) => {
                self.column_filtered = true;
                (Query::Column(f.clone()), Query::Column(f))
            }
            None => (
                // Für die Sofortanzeige NUR der Name: local_matches läuft auf
                // dem UI-Thread, und dort darf nichts von der Platte gelesen
                // werden — sonst ruckelt jeder Tastendruck.
                Query::Name { needle: self.filter.clone(), contents: false },
                Query::Name { needle: self.filter.clone(), contents: with_contents },
            ),
        };
        self.searching_contents = with_contents && !self.column_filtered;
        self.names_done = false;
        self.user_moved_selection = false;

        // Sofort: Treffer im aktuellen Ordner …
        self.search_results = search::local_matches(&instant, &self.all_entries);
        self.rebuild_view();
        self.reset_selection_to_top();
        // … danach im Hintergrund rekursiv in die Unterordner erweitern, dann
        // auch im Dateiinhalt.
        self.spawn_recursive_search(gen, deep);
    }

    fn spawn_recursive_search(&mut self, gen: u64, query: Query) {
        self.is_searching = true;
        let root = self.current_dir.clone();
        let hidden = self.show_hidden;
        let generation = Arc::clone(&self.generation);
        let tx = self.search_tx.clone();

        std::thread::spawn(move || {
            // Entprellen: erst scannen, wenn das Tippen ruht.
            std::thread::sleep(SEARCH_DEBOUNCE);
            if generation.load(Ordering::SeqCst) != gen {
                return;
            }

            // Teilergebnisse fließen laufend herein, damit die ersten Zeilen
            // nach Millisekunden dastehen statt erst am Ende.
            let (part_tx, part_rx) = channel::<Vec<Entry>>();
            let pump = {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    while let Ok(entries) = part_rx.recv() {
                        let _ = tx.send(SearchBatch {
                            generation: gen,
                            entries,
                            kind: BatchKind::Partial,
                        });
                    }
                })
            };

            // ZWEI Durchgänge, wenn Inhalte gesucht werden. Der erste sucht nur
            // Namen und ist um ein Vielfaches schneller: ab /Users sind das
            // ~0,5 s gegen ~2 s. Vorher hingen die Namenstreffer an der
            // Inhaltssuche und erschienen erst mit ihr zusammen — die Suche
            // wirkte dadurch viel langsamer, als sie ist.
            let searches_content = matches!(query, Query::Name { contents: true, .. });
            if searches_content {
                let names_only = match &query {
                    Query::Name { needle, .. } => Query::Name {
                        needle: needle.clone(),
                        contents: false,
                    },
                    other => other.clone(),
                };
                let hits = search::search(
                    &root, &names_only, hidden, SEARCH_LIMIT, &generation, gen, Some(&part_tx),
                );
                if generation.load(Ordering::SeqCst) != gen {
                    return;
                }
                let _ = tx.send(SearchBatch {
                    generation: gen,
                    entries: hits,
                    kind: BatchKind::Complete { last: false },
                });
            }

            let hits = search::search(
                &root, &query, hidden, SEARCH_LIMIT, &generation, gen,
                // Im Inhaltsdurchgang stehen die Namenstreffer schon; laufend
                // nachliefern lohnt trotzdem, weil er lange dauert.
                Some(&part_tx),
            );
            drop(part_tx);
            let _ = pump.join();
            if generation.load(Ordering::SeqCst) == gen {
                let _ = tx.send(SearchBatch {
                    generation: gen,
                    entries: hits,
                    kind: BatchKind::Complete { last: true },
                });
            }
        });
    }

    /// Holt fertige Suchergebnisse ab. `true`, wenn sich die Liste geändert hat.
    pub fn poll_search(&mut self) -> bool {
        let mut changed = false;
        while let Ok(batch) = self.search_rx.try_recv() {
            if batch.generation != self.generation.load(Ordering::SeqCst) {
                continue; // veraltet — eine neuere Suche läuft bereits
            }
            // Auswahl nur festhalten, wenn der Nutzer sie selbst gesetzt hat.
            // Sonst bliebe der Cursor am automatisch gewählten ersten Treffer
            // kleben und wanderte mit der wachsenden Liste nach unten — die
            // Ansicht scrollte dann von allein weg.
            let previously = if self.user_moved_selection {
                self.selected_entry().map(|e| e.path.clone())
            } else {
                None
            };

            match batch.kind {
                BatchKind::Partial => {
                    // Anhängen und Dubletten fernhalten: derselbe Pfad kann aus
                    // dem Aufklappen und aus einem Unterbaum kommen.
                    let known: std::collections::HashSet<PathBuf> =
                        self.search_results.iter().map(|e| e.path.clone()).collect();
                    self.search_results
                        .extend(batch.entries.into_iter().filter(|e| !known.contains(&e.path)));
                    self.search_results.truncate(SEARCH_LIMIT);
                }
                BatchKind::Complete { last } => {
                    // Der Durchgang ist fertig: die endgültige, nach Rang
                    // gekürzte Liste ersetzt die zusammengetragene.
                    self.search_results = batch.entries;
                    self.is_searching = !last;
                    self.names_done = !last;
                }
            }
            self.rebuild_view();

            match previously.and_then(|p| self.view.iter().position(|e| e.path == p)) {
                Some(idx) => self.select_only(idx),
                None => self.reset_selection_to_top(),
            }
            changed = true;
        }
        changed
    }

    // MARK: - Sortierung

    /// Ordner immer zuoberst, dann der mehrstufige Sortierstapel (das erste
    /// Kriterium führt, der Rest bricht Gleichstände).
    fn rebuild_view(&mut self) {
        let source = if self.filter.is_empty() {
            &self.all_entries
        } else {
            &self.search_results
        };
        let mut list = source.clone();
        let stack = self.sort_stack.clone();
        list.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then_with(|| {
                for crit in &stack {
                    let c = compare(a, b, crit.key);
                    if c != std::cmp::Ordering::Equal {
                        return if crit.ascending { c } else { c.reverse() };
                    }
                }
                cmp_ignoring_case(&a.name, &b.name)
            })
        });
        self.view = list;
        // Auswahl auf das beschränken, was noch sichtbar ist.
        let visible: HashSet<&PathBuf> = self.view.iter().map(|e| &e.path).collect();
        self.selection.retain(|p| visible.contains(p));
    }

    /// Klick auf eine Spaltenüberschrift: gleiche Spalte kehrt die Richtung um,
    /// eine neue Spalte wandert nach vorn und schiebt die alte zum Gleichstand-Brecher.
    pub fn sort_by(&mut self, key: SortKey) {
        if self.sort_stack.first().map(|c| c.key) == Some(key) {
            self.sort_stack[0].ascending = !self.sort_stack[0].ascending;
        } else {
            self.sort_stack.retain(|c| c.key != key);
            self.sort_stack.insert(0, SortCriterion { key, ascending: true });
            self.sort_stack.truncate(3);
        }
        self.rebuild_view();
        self.persist();
    }

    pub fn toggle_sort_direction(&mut self) {
        if let Some(first) = self.sort_stack.first_mut() {
            first.ascending = !first.ascending;
        }
        self.rebuild_view();
        self.persist();
    }

    pub fn sort_key(&self) -> SortKey {
        self.sort_stack.first().map(|c| c.key).unwrap_or(SortKey::Name)
    }

    pub fn sort_ascending(&self) -> bool {
        self.sort_stack.first().map(|c| c.ascending).unwrap_or(true)
    }

    // MARK: - Navigation

    pub fn navigate(&mut self, url: PathBuf, record_history: bool) {
        if !url.is_dir() {
            self.error = Some(format!("Kein Ordner: {}", url.display()));
            return;
        }
        if record_history && url != self.current_dir {
            self.back_stack.push(self.current_dir.clone());
            self.forward_stack.clear();
        }
        self.current_dir = url;
        self.filter.clear();
        self.selection.clear();
        self.load();
        self.persist();
    }

    pub fn go_into(&mut self, entry: &Entry) {
        if entry.is_dir {
            self.navigate(entry.path.clone(), true);
        } else if let Err(e) = integrate::open_default(&entry.path) {
            self.error = Some(e);
        }
    }

    /// Springt zum Ordner, der einen Suchtreffer enthält, und wählt ihn dort aus.
    pub fn reveal_in_parent(&mut self, entry: &Entry) {
        let Some(parent) = entry.path.parent().map(Path::to_path_buf) else {
            return;
        };
        let name = entry.name.clone();
        self.navigate(parent, true);
        self.after_load = Some(AfterLoad::Name(name));
    }

    pub fn go_up(&mut self) {
        if let Some(parent) = self.current_dir.parent().map(Path::to_path_buf) {
            if parent != self.current_dir {
                self.navigate(parent, true);
            }
        }
    }

    pub fn go_back(&mut self) {
        if let Some(url) = self.back_stack.pop() {
            self.forward_stack.push(self.current_dir.clone());
            self.current_dir = url;
            self.filter.clear();
            self.selection.clear();
            self.load();
            self.persist();
        }
    }

    pub fn go_forward(&mut self) {
        if let Some(url) = self.forward_stack.pop() {
            self.back_stack.push(self.current_dir.clone());
            self.current_dir = url;
            self.filter.clear();
            self.selection.clear();
            self.load();
            self.persist();
        }
    }

    pub fn go_home(&mut self) {
        self.navigate(home_dir(), true);
    }

    pub fn can_go_back(&self) -> bool {
        !self.back_stack.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward_stack.is_empty()
    }

    // MARK: - Favoriten

    pub fn is_favorite(&self, url: &Path) -> bool {
        self.favorites.iter().any(|f| f == url)
    }

    pub fn add_favorite(&mut self, url: PathBuf) {
        if url.is_dir() && !self.is_favorite(&url) {
            let name = url.file_name().unwrap_or_default().to_string_lossy().into_owned();
            self.favorites.push(url);
            self.persist();
            self.flash(format!("Favorit hinzugefügt · {name}"));
        }
    }

    pub fn remove_favorite(&mut self, url: &Path) {
        self.favorites.retain(|f| f != url);
        self.persist();
    }

    // MARK: - Auswahl

    fn reset_selection_to_top(&mut self) {
        self.selected_index = 0;
        self.anchor_index = 0;
        self.selection.clear();
        if let Some(first) = self.view.first() {
            self.selection.insert(first.path.clone());
        }
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.view.get(self.selected_index)
    }

    pub fn selected_entries(&self) -> Vec<&Entry> {
        self.view.iter().filter(|e| self.selection.contains(&e.path)).collect()
    }

    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected_entries().into_iter().map(|e| e.path.clone()).collect()
    }

    pub fn select_only(&mut self, idx: usize) {
        if self.view.is_empty() {
            return;
        }
        let idx = idx.min(self.view.len() - 1);
        self.selected_index = idx;
        self.anchor_index = idx;
        self.selection.clear();
        self.selection.insert(self.view[idx].path.clone());
    }

    pub fn toggle_select(&mut self, idx: usize) {
        let Some(entry) = self.view.get(idx) else { return };
        let path = entry.path.clone();
        if !self.selection.remove(&path) {
            self.selection.insert(path);
        }
        self.selected_index = idx;
        self.anchor_index = idx;
    }

    /// Bereichsauswahl vom Anker bis `idx` (Umschalt-Klick).
    pub fn range_select(&mut self, idx: usize) {
        if self.view.is_empty() {
            return;
        }
        let idx = idx.min(self.view.len() - 1);
        let (lo, hi) = if self.anchor_index <= idx {
            (self.anchor_index, idx)
        } else {
            (idx, self.anchor_index)
        };
        self.selection.clear();
        for e in &self.view[lo..=hi] {
            self.selection.insert(e.path.clone());
        }
        self.selected_index = idx;
    }

    pub fn move_selection(&mut self, delta: i64) {
        if self.view.is_empty() {
            return;
        }
        let last = self.view.len() as i64 - 1;
        let next = (self.selected_index as i64 + delta).clamp(0, last) as usize;
        self.select_only(next);
    }

    /// Auswahl erweitern statt verschieben (Umschalt + Pfeiltaste).
    pub fn extend_selection(&mut self, delta: i64) {
        if self.view.is_empty() {
            return;
        }
        let last = self.view.len() as i64 - 1;
        let next = (self.selected_index as i64 + delta).clamp(0, last) as usize;
        self.range_select(next);
    }

    pub fn select_edge(&mut self, start: bool) {
        if self.view.is_empty() {
            return;
        }
        self.select_only(if start { 0 } else { self.view.len() - 1 });
    }

    pub fn select_all(&mut self) {
        self.selection = self.view.iter().map(|e| e.path.clone()).collect();
    }

    fn select_by_name(&mut self, name: &str) {
        if let Some(idx) = self.view.iter().position(|e| e.name == name) {
            self.select_only(idx);
        }
    }

    /// Enter: Dateien in ihrer Standard-App öffnen; ist nur ein Ordner gewählt,
    /// hineinnavigieren.
    pub fn activate_selection(&mut self) {
        let selected: Vec<Entry> = self.selected_entries().into_iter().cloned().collect();
        if selected.is_empty() {
            if let Some(e) = self.selected_entry().cloned() {
                self.go_into(&e);
            }
            return;
        }
        let files: Vec<&Entry> = selected.iter().filter(|e| !e.is_dir).collect();
        for f in &files {
            if let Err(e) = integrate::open_default(&f.path) {
                self.error = Some(e);
            }
        }
        if files.is_empty() {
            if let Some(folder) = selected.iter().find(|e| e.is_dir) {
                self.navigate(folder.path.clone(), true);
            }
        }
    }

    // MARK: - Dateioperationen

    pub fn reveal_in_finder(&mut self) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
        if let Err(e) = integrate::reveal_in_file_manager(&refs) {
            self.error = Some(e);
        }
    }

    /// Zielordner für „öffnen in …“: der markierte Ordner, sonst der aktuelle.
    fn open_target(&self) -> PathBuf {
        match self.selected_entry() {
            Some(e) if e.is_dir => e.path.clone(),
            _ => self.current_dir.clone(),
        }
    }

    pub fn open_in_zed(&mut self) {
        let target = self.open_target();
        match integrate::open_in_zed(&target) {
            Ok(()) => self.flash(format!(
                "In Zed geöffnet · {}",
                target.file_name().unwrap_or_default().to_string_lossy()
            )),
            Err(e) => self.error = Some(e),
        }
    }

    pub fn open_in_terminal(&mut self) {
        let target = self.open_target();
        if let Err(e) = integrate::open_in_terminal(&target) {
            self.error = Some(e);
        }
    }

    /// Kopiert den Pfad des ANGEZEIGTEN Ordners (nicht der Auswahl) — wie in der
    /// SwiftUI-Fassung; für einen Unterordner erst hineingehen.
    pub fn copy_path(&mut self) {
        let path = self.current_dir.clone();
        match integrate::copy_text(&path.to_string_lossy()) {
            Ok(()) => self.flash(format!(
                "Pfad kopiert · {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )),
            Err(e) => self.error = Some(e),
        }
    }

    pub fn copy_path_of(&mut self, entry: &Entry) {
        match integrate::copy_text(&entry.path.to_string_lossy()) {
            Ok(()) => self.flash(format!("Pfad kopiert · {}", entry.name)),
            Err(e) => self.error = Some(e),
        }
    }

    /// Legt die Auswahl in die app-interne Ablage; die Pfade gehen zusätzlich als
    /// Text in die System-Zwischenablage.
    pub fn copy_selection(&mut self) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        let joined = paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        let _ = integrate::copy_text(&joined);
        let msg = if paths.len() == 1 {
            format!(
                "Kopiert · {}",
                paths[0].file_name().unwrap_or_default().to_string_lossy()
            )
        } else {
            format!("{} kopiert", paths.len())
        };
        self.clipboard = paths;
        self.flash(msg);
    }

    pub fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        let sources = self.clipboard.clone();
        let dest = self.current_dir.clone();
        match ops::paste(&sources, &dest) {
            Ok((msg, last)) => {
                self.flash(msg);
                // Auswahl erst setzen, wenn die Einträge da sind — das
                // Einlesen läuft jetzt nebenläufig.
                self.after_load = last.map(AfterLoad::Name);
                self.load();
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// Was beim Löschen passieren soll. Ordner gehen nie ohne Rückfrage weg:
    /// dahinter können tausende Dateien stecken, die man in der Liste nicht
    /// sieht, und ein Tastendruck ist dafür zu wenig.
    pub fn plan_trash(&self) -> TrashPlan {
        let entries: Vec<&Entry> = self.selected_entries();
        if entries.is_empty() {
            return TrashPlan::Nothing;
        }
        let paths: Vec<PathBuf> = entries.iter().map(|e| e.path.clone()).collect();
        let folders: Vec<&&Entry> = entries.iter().filter(|e| e.is_dir).collect();
        if folders.is_empty() {
            return TrashPlan::Immediate(paths);
        }

        // Zählen, was an den Ordnern hängt — gedeckelt, damit die Rückfrage
        // sofort kommt und nicht erst nach einem Tiefenscan.
        const CAP: usize = 5000;
        let mut total = 0usize;
        let mut capped = false;
        for f in &folders {
            let (n, hit_cap) = ops::count_entries(&f.path, CAP - total.min(CAP));
            total += n;
            capped |= hit_cap;
            if total >= CAP {
                capped = true;
                break;
            }
        }
        let amount = if capped {
            format!("mehr als {total} Einträgen")
        } else if total == 1 {
            "einem Eintrag".to_string()
        } else {
            format!("{total} Einträgen")
        };

        let summary = if entries.len() == 1 {
            format!("Ordner „{}“ mit {amount}", folders[0].name)
        } else if folders.len() == entries.len() {
            format!("{} Ordner mit zusammen {amount}", folders.len())
        } else {
            format!(
                "{} Objekte, darunter {} Ordner mit zusammen {amount}",
                entries.len(),
                folders.len()
            )
        };
        TrashPlan::Confirm { paths, summary }
    }

    pub fn trash_paths(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let keep = self.selected_index;
        match ops::move_to_trash(&paths) {
            Ok(msg) => {
                self.flash(msg);
                self.after_load = Some(AfterLoad::Index(keep));
                self.load();
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub fn rename(&mut self, path: &Path, new_name: &str) {
        match ops::rename(path, new_name) {
            Ok(msg) => {
                if !msg.is_empty() {
                    self.flash(msg);
                }
                self.after_load = Some(AfterLoad::Name(new_name.trim().to_string()));
                self.load();
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub fn duplicate_selection(&mut self) {
        let Some(path) = self.selected_entry().map(|e| e.path.clone()) else {
            return;
        };
        match ops::duplicate(&path) {
            Ok((msg, created)) => {
                self.flash(msg);
                self.after_load = Some(AfterLoad::Name(created));
                self.load();
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub fn new_folder(&mut self, name: &str) {
        let parent = self.current_dir.clone();
        match ops::new_folder(&parent, name) {
            Ok((msg, created)) => {
                self.flash(msg);
                self.after_load = Some(AfterLoad::Name(created));
                self.load();
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub fn go_to_path(&mut self, raw: &str) {
        let expanded = expand_tilde(raw.trim());
        if expanded.is_dir() {
            self.navigate(expanded, true);
        } else if expanded.is_file() {
            // Auf eine Datei zeigen heißt: Ordner öffnen und sie markieren.
            if let Some(parent) = expanded.parent().map(Path::to_path_buf) {
                let name = expanded.file_name().unwrap_or_default().to_string_lossy().into_owned();
                self.navigate(parent, true);
                self.after_load = Some(AfterLoad::Name(name));
            }
        } else {
            self.error = Some(format!("Nicht gefunden: {raw}"));
        }
    }

    // MARK: - Einstellungen

    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.load();
        self.persist();
    }

    pub fn toggle_sidebar(&mut self) {
        self.show_sidebar = !self.show_sidebar;
        self.persist();
    }

    // MARK: - Statuszeile

    fn flash(&mut self, message: String) {
        self.flash = Some((message, Instant::now()));
    }

    /// Die flüchtige Meldung, solange sie frisch ist (~1,6 s).
    pub fn transient_message(&self) -> Option<&str> {
        self.flash
            .as_ref()
            .filter(|(_, at)| at.elapsed() < FLASH_DURATION)
            .map(|(m, _)| m.as_str())
    }

    pub fn has_pending_flash(&self) -> bool {
        self.transient_message().is_some()
    }

    /// Nimmt die aktuelle Fenstergeometrie entgegen und schreibt sie, sobald
    /// das Ziehen aufgehört hat.
    pub fn note_window_geometry(
        &mut self,
        size: Option<[f32; 2]>,
        pos: Option<[f32; 2]>,
        special_state: bool,
    ) {
        // Vollbild und Zoom nicht merken: sonst startet die App beim nächsten
        // Mal als bildschirmgroßes Fenster, das gar kein Vollbild ist.
        // Minimierte oder noch nicht eingerichtete Fenster melden ohnehin Unsinn.
        if special_state || !size.is_some_and(|s| s[0] >= 200.0 && s[1] >= 200.0) {
            return;
        }
        let moved = |a: Option<[f32; 2]>, b: Option<[f32; 2]>| match (a, b) {
            (Some(a), Some(b)) => (a[0] - b[0]).abs() > 1.0 || (a[1] - b[1]).abs() > 1.0,
            (None, Some(_)) => true,
            _ => false,
        };
        if moved(self.window_size, size) || moved(self.window_pos, pos) {
            self.window_size = size;
            self.window_pos = pos;
            self.geometry_dirty_since = Some(Instant::now());
        }
        // Erst schreiben, wenn ~400 ms nichts mehr passiert ist.
        if self
            .geometry_dirty_since
            .is_some_and(|t| t.elapsed() > Duration::from_millis(400))
        {
            self.geometry_dirty_since = None;
            self.persist();
        }
    }

    // MARK: - Persistenz

    pub fn persist(&self) {
        let data = Persisted {
            current_dir: Some(self.current_dir.to_string_lossy().into_owned()),
            show_hidden: self.show_hidden,
            show_sidebar: self.show_sidebar,
            sort_stack: self.sort_stack.clone(),
            favorites: self
                .favorites
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            window_size: self.window_size,
            window_pos: self.window_pos,
        };
        let Some(path) = state_file() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(&data) {
            let _ = std::fs::write(path, json);
        }
    }
}

fn compare(a: &Entry, b: &Entry, key: SortKey) -> std::cmp::Ordering {
    match key {
        SortKey::Name => cmp_ignoring_case(&a.name, &b.name),
        SortKey::Date => a.modified.cmp(&b.modified),
        SortKey::Size => a.size.cmp(&b.size),
        SortKey::Kind => cmp_ignoring_case(&a.kind(), &b.kind()),
    }
}

/// Vergleich ohne Rücksicht auf Groß- und Kleinschreibung, **ohne** für jeden
/// Vergleich zwei Strings anzulegen. Bei 50 000 Einträgen sind das sonst
/// hunderttausende Allokationen je Sortierung.
fn cmp_ignoring_case(a: &str, b: &str) -> std::cmp::Ordering {
    a.chars()
        .flat_map(char::to_lowercase)
        .cmp(b.chars().flat_map(char::to_lowercase))
}

pub fn home_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home);
    }
    // Windows setzt HOME meist nicht.
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(profile);
    }
    PathBuf::from(if cfg!(windows) { r"C:\" } else { "/" })
}

pub fn expand_tilde(raw: &str) -> PathBuf {
    if raw == "~" {
        return home_dir();
    }
    match raw.strip_prefix("~/") {
        Some(rest) => home_dir().join(rest),
        None => PathBuf::from(raw),
    }
}

/// Der systemübliche Ort für Einstellungen: Application Support auf macOS,
/// %APPDATA% auf Windows, $XDG_CONFIG_HOME (sonst ~/.config) auf Linux.
fn state_file() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let dir = home_dir().join("Library/Application Support/FileRune");

    #[cfg(target_os = "windows")]
    let dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join("AppData/Roaming"))
        .join("FileRune");

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("filerune");

    Some(dir.join("state.json"))
}

fn load_persisted() -> Option<Persisted> {
    let json = std::fs::read_to_string(state_file()?).ok()?;
    serde_json::from_str(&json).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_expands_to_home() {
        assert_eq!(expand_tilde("~"), home_dir());
        assert_eq!(expand_tilde("~/Desktop"), home_dir().join("Desktop"));
        assert_eq!(expand_tilde("/tmp"), PathBuf::from("/tmp"));
    }
}
