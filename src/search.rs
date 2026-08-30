//! Rekursive Suche über einen Unterbaum — nach Namen und, wenn gewünscht, im
//! Dateiinhalt:
//!  • verteilt die Arbeit auf einen Arbeiter je Kern,
//!  • überspringt Müll- und Systembäume (node_modules/.git/target/Caches/Library/…),
//!    damit die Suche bei den Dateien des Nutzers landet statt in Caches zu ertrinken,
//!  • bricht ab, sobald eine neuere Suche startet (Generationszähler).

use crate::content;
use crate::entry::Entry;
use crate::filter::ColumnFilter;
use crate::fuzzy;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use walkdir::WalkDir;

/// Ordnernamen, die überall übersprungen werden.
const SKIP_NAMES: &[&str] = &[
    "node_modules", ".git", ".svn", ".hg", "Pods", "vendor", "bower_components",
    ".npm", ".yarn", ".pnpm-store", ".cocoapods", ".gradle", ".cargo", ".rustup",
    ".pub-cache", ".nuget", ".m2",
    // Build-Ausgaben: regenerierbar, riesig, voller Zufallsnamen. "target" (Rust)
    // fehlte in der ersten Fassung — eine Suche ab /Users ertrank in target/debug/deps.
    "target", ".build", "DerivedData", "__pycache__", ".venv", "venv",
    ".next", ".nuxt", ".turbo", ".parcel-cache", ".dart_tool", ".terraform",
    "Caches", "Library", ".cache", ".Trash", ".Trashes",
    ".Spotlight-V100", ".fseventsd", ".DocumentRevisions-V100", ".TemporaryItems",
    // Windows und Linux
    "AppData", "$RECYCLE.BIN", "System Volume Information", ".local", ".config",
];

/// Absolute Systempfade, die übersprungen werden, solange der Nutzer nicht
/// ausdrücklich darin sucht.
#[cfg(target_os = "macos")]
const SKIP_PATHS: &[&str] = &[
    "/System", "/usr", "/bin", "/sbin", "/cores", "/dev", "/opt",
    "/private", "/Network", "/.vol", "/Volumes",
];

#[cfg(target_os = "windows")]
const SKIP_PATHS: &[&str] = &[
    r"C:\Windows", r"C:\Program Files", r"C:\Program Files (x86)",
    r"C:\ProgramData", r"C:\$Recycle.Bin", r"C:\System Volume Information",
];

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
const SKIP_PATHS: &[&str] = &[
    // /proc und /sys sind virtuell und praktisch endlos tief.
    "/proc", "/sys", "/dev", "/run", "/boot", "/usr", "/bin", "/sbin",
    "/lib", "/lib64", "/opt", "/snap", "/var/lib", "/tmp",
];

/// Sicherheitsgrenze je Unterbaum, damit ein pathologischer Ordner die Suche
/// nicht endlos beschäftigt.
const MAX_SCANNED_PER_SUBTREE: usize = 200_000;

/// Inhaltstreffer landen unter allen Namenstreffern. Ohne diesen Abstand
/// würden sie sich mit den Namenstreffern mischen, und man fände seine Datei
/// nicht wieder, obwohl der Name genau passt.
const CONTENT_SCORE_BASE: i32 = -100_000;

fn should_skip(name: &str, path: &Path) -> bool {
    SKIP_NAMES.contains(&name) || SKIP_PATHS.iter().any(|p| path == Path::new(p))
}

/// Wonach gesucht wird. Beides läuft durch dieselbe Maschinerie, damit auch
/// `art:pdf` in die Unterordner reicht.
#[derive(Clone)]
pub enum Query {
    /// Namenssuche; `contents` schaltet zusätzlich die Suche im Dateiinhalt dazu.
    Name { needle: String, contents: bool },
    Column(ColumnFilter),
}

impl Query {
    /// Bewertung eines Eintrags samt Fundstelle, oder `None` wenn er nicht passt.
    /// Spaltenfilter kennen keine Rangfolge — dort entscheidet der Sortierstapel.
    fn evaluate(&self, entry: &Entry) -> Option<(i32, Option<String>)> {
        match self {
            Query::Column(f) => f.matches(entry).then_some((0, None)),
            Query::Name { needle, contents } => {
                if let Some(s) = fuzzy::score(needle, &entry.name) {
                    return Some((s, None));
                }
                if !*contents || entry.is_dir {
                    return None;
                }
                // Erst wenn der Name nicht passt, wird die Datei aufgemacht.
                let lower = needle.to_lowercase();
                content::find(&entry.path, entry.size, &lower)
                    .map(|line| (CONTENT_SCORE_BASE, Some(line)))
            }
        }
    }

    /// Billige Vorauswahl allein am Dateinamen, bevor der Eintrag mit einem
    /// `stat` teuer aufgebaut wird. Spaltenfilter brauchen Größe und Datum, die
    /// Inhaltssuche muss ohnehin jede Datei ansehen — beide können hier nichts
    /// ausschließen.
    fn may_match_name(&self, name: &str, is_dir: bool) -> bool {
        match self {
            Query::Name { needle, contents } => {
                // Bei Inhaltssuche muss jede Datei aufgemacht werden; Ordner
                // haben keinen Inhalt und fallen hier schon raus.
                (*contents && !is_dir) || fuzzy::score(needle, name).is_some()
            }
            Query::Column(f) => !f.quick_reject(name, is_dir),
        }
    }
}

/// Sofortige Treffer im bereits geladenen Ordner — steht vor dem Tiefenscan da.
pub fn local_matches(query: &Query, entries: &[Entry]) -> Vec<Entry> {
    let mut scored: Vec<(Entry, i32)> = entries
        .iter()
        .filter_map(|e| {
            query.evaluate(e).map(|(s, line)| {
                let mut e = e.clone();
                e.matched_line = line;
                (e, s)
            })
        })
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored.into_iter().map(|(e, _)| e).collect()
}

/// Der Tiefenscan. Bricht ab, sobald `generation` nicht mehr `my_gen` ist.
/// `progress` bekommt Teilergebnisse, sobald ein Unterbaum fertig ist — dann
/// steht die erste Zeile nach Millisekunden statt erst am Ende des ganzen
/// Durchlaufs. Ab /Users dauert der komplett rund eine halbe Sekunde, und so
/// lange auf ein leeres Fenster zu sehen wirkt langsamer, als es ist.
pub fn search(
    root: &Path,
    query: &Query,
    show_hidden: bool,
    limit: usize,
    generation: &Arc<AtomicU64>,
    my_gen: u64,
    progress: Option<&Sender<Vec<Entry>>>,
) -> Vec<Entry> {
    let cancelled = || generation.load(Ordering::Relaxed) != my_gen;

    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    // Arbeitsliste aufbauen und dabei die Dateien der oberen Ebenen gleich prüfen.
    let (dirs, mut collected) = build_queue(root, query, show_hidden, workers * 4, &cancelled);
    if cancelled() {
        return Vec::new();
    }

    // Was schon beim Aufklappen gefunden wurde, sofort melden.
    if let Some(tx) = progress {
        if !collected.is_empty() {
            let _ = tx.send(collected.iter().map(|(e, _)| e.clone()).collect());
        }
    }

    // Ein Arbeiter je Kern zieht sich Unterbäume aus einer gemeinsamen Warteschlange.
    let next = AtomicUsize::new(0);
    let active = workers.min(dirs.len().max(1));

    let partials: Vec<Vec<(Entry, i32)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..active)
            .map(|_| {
                let next = &next;
                let dirs = &dirs;
                // Jeder Arbeiter bekommt seinen eigenen Absender.
                let tx = progress.cloned();
                scope.spawn(move || {
                    let mut out: Vec<(Entry, i32)> = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(dir) = dirs.get(i) else { break };
                        if cancelled() {
                            break;
                        }
                        let part = walk(dir, query, show_hidden, root, limit, &cancelled);
                        if let Some(tx) = &tx {
                            if !part.is_empty() && !cancelled() {
                                let _ = tx.send(part.iter().map(|(e, _)| e.clone()).collect());
                            }
                        }
                        out.extend(part);
                    }
                    out
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });

    if cancelled() {
        return Vec::new();
    }
    for p in partials {
        collected.extend(p);
    }
    collected.sort_by(|a, b| b.1.cmp(&a.1));
    collected.truncate(limit);
    collected.into_iter().map(|(e, _)| e).collect()
}

/// Baut die Arbeitsliste für die Arbeiter: erst die Unterordner der obersten
/// Ebene, und solange das weniger sind als `want`, jeweils eine Ebene tiefer
/// aufklappen. Ohne das läge bei einem Baum mit zwei Unterordnern der Großteil
/// der Kerne brach — was beim Lesen von Dateiinhalten richtig weh tut.
///
/// Zurückgegeben wird **nur die tiefste** aufgeklappte Ebene. Gäbe man auch die
/// darüberliegenden Ordner mit, liefe deren Unterbaum zweimal durch und jeder
/// Treffer erschiene doppelt. Die Einträge der bereits gelesenen Ebenen sind
/// hier schon geprüft und stecken in `hits`.
fn build_queue(
    root: &Path,
    query: &Query,
    show_hidden: bool,
    want: usize,
    cancelled: &dyn Fn() -> bool,
) -> (Vec<PathBuf>, Vec<(Entry, i32)>) {
    /// Weiter als so viele Ebenen wird nicht aufgeklappt — in einem tiefen,
    /// schmalen Baum sonst endlos, ohne mehr Parallelität zu gewinnen.
    const MAX_EXPANSIONS: usize = 3;

    let mut hits: Vec<(Entry, i32)> = Vec::new();
    let mut frontier: Vec<PathBuf> = vec![root.to_path_buf()];

    for _ in 0..MAX_EXPANSIONS {
        let mut next: Vec<PathBuf> = Vec::new();
        for dir in &frontier {
            if cancelled() {
                return (Vec::new(), hits);
            }
            let Ok(read) = std::fs::read_dir(dir) else { continue };
            for item in read.flatten() {
                let name = item.file_name().to_string_lossy().into_owned();
                if !show_hidden && name.starts_with('.') {
                    continue;
                }
                let path = item.path();
                let is_dir = item.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if is_dir && !should_skip(&name, &path) {
                    next.push(path.clone());
                }
                // Auch ein Ordner kann selbst ein Treffer sein.
                if query.may_match_name(&name, is_dir) {
                    if let Some(mut e) = Entry::from_path(path, None) {
                        if let Some((score, line)) = query.evaluate(&e) {
                            e.matched_line = line;
                            e.set_rel_parent(root);
                            hits.push((e, score));
                        }
                    }
                }
            }
        }
        // Genug Arbeit für alle Kerne, oder nichts mehr da: fertig.
        if next.len() >= want || next.is_empty() {
            return (next, hits);
        }
        frontier = next;
    }
    (frontier, hits)
}

fn walk(
    dir: &Path,
    query: &Query,
    show_hidden: bool,
    root: &Path,
    limit: usize,
    cancelled: &dyn Fn() -> bool,
) -> Vec<(Entry, i32)> {
    let mut hits: Vec<(Entry, i32)> = Vec::new();
    let mut scanned = 0usize;
    let prune_at = (limit * 4).max(256);

    // min_depth(1): WalkDir liefert `dir` selbst als ersten Eintrag mit. Ohne das
    // erschien jeder passende Ordner der obersten Ebene doppelt — einmal aus der
    // Arbeitsliste, einmal als Wurzel seines eigenen Laufs.
    let walker = WalkDir::new(dir).min_depth(1).into_iter().filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        if !show_hidden && name.starts_with('.') && e.depth() > 0 {
            return false;
        }
        !(e.file_type().is_dir() && should_skip(&name, e.path()))
    });

    for item in walker.flatten() {
        // Häufiger prüfen als früher: beim Lesen von Inhalten ist jeder Eintrag
        // teuer, da darf eine abgelöste Suche nicht lange weiterlaufen.
        if scanned & 0xFF == 0 && cancelled() {
            return hits;
        }
        scanned += 1;
        if scanned > MAX_SCANNED_PER_SUBTREE {
            break;
        }

        let name = item.file_name().to_string_lossy();
        if !query.may_match_name(&name, item.file_type().is_dir()) {
            continue;
        }
        // Der Ordnerpfad wird erst für echte Treffer gebaut — sonst legt die
        // Inhaltssuche für jede Datei im Baum einen String an, den sie wegwirft.
        let Some(mut e) = Entry::from_path(item.path().to_path_buf(), None) else {
            continue;
        };
        let Some((score, line)) = query.evaluate(&e) else {
            continue;
        };
        e.matched_line = line;
        e.set_rel_parent(root);
        hits.push((e, score));

        // NICHT bei `limit` abbrechen: der Walker läuft in die Tiefe, also füllte
        // ein einziger Build-Ordner das Budget, bevor die Geschwister drankamen —
        // eine Suche ab /Users lieferte nur target/debug/deps und erreichte
        // Downloads nie. Stattdessen laufend auf die besten Treffer eindampfen:
        // es gewinnt der Score, nicht die Reihenfolge des Verzeichnisbaums.
        if hits.len() >= prune_at {
            hits.sort_by(|a, b| b.1.cmp(&a.1));
            hits.truncate(limit);
        }
    }
    hits.sort_by(|a, b| b.1.cmp(&a.1));
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_build_output_dirs() {
        assert!(should_skip("target", Path::new("/x/target")));
        assert!(should_skip("node_modules", Path::new("/x/node_modules")));
        assert!(!should_skip("src", Path::new("/x/src")));
    }

    /// Bewusst gegen SKIP_PATHS selbst geprüft statt gegen einen festen Pfad:
    /// `/System` gibt es nur auf macOS, `/proc` nur auf Linux, `C:\Windows` nur
    /// auf Windows — ein fest verdrahteter Pfad lässt den Test anderswo scheitern.
    #[test]
    fn skips_system_roots_but_not_lookalikes() {
        let root = Path::new(SKIP_PATHS[0]);
        let name = root.file_name().unwrap().to_string_lossy().into_owned();
        assert!(should_skip(&name, root));

        let deeper = root.parent().unwrap().join("irgendwo").join(&name);
        assert!(!should_skip(&name, &deeper));
    }

    /// Baum anlegen, der flach genug ist, dass `build_queue` mehrfach
    /// aufklappen muss — genau dort entstanden schon zweimal Dubletten.
    fn tree(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("filerune-search-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("eins/zwei/drei")).unwrap();
        std::fs::write(root.join("nadel-oben.txt"), "nichts").unwrap();
        std::fs::write(root.join("eins/nadel-mitte.txt"), "nichts").unwrap();
        std::fs::write(root.join("eins/zwei/drei/nadel-tief.txt"), "nichts").unwrap();
        std::fs::write(root.join("eins/zwei/inhalt.txt"), "hier steht Nadel drin").unwrap();
        root
    }

    fn run(root: &Path, query: Query) -> Vec<Entry> {
        let generation = Arc::new(AtomicU64::new(7));
        search(root, &query, false, 1000, &generation, 7, None)
    }

    /// Jeder Pfad darf höchstens einmal vorkommen. Vorher lief der Unterbaum
    /// doppelt: einmal aus der aufgeklappten Ebene, einmal aus der darüber.
    #[test]
    fn results_contain_no_duplicates() {
        let root = tree("dup");
        let hits = run(&root, Query::Name { needle: "nadel".into(), contents: true });
        let mut paths: Vec<_> = hits.iter().map(|e| e.path.clone()).collect();
        let before = paths.len();
        paths.sort();
        paths.dedup();
        assert_eq!(before, paths.len(), "Dubletten in {hits:#?}");
        assert!(before >= 4, "zu wenige Treffer: {before}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Der Inhaltstreffer heißt nicht wie die Suche — er kann nur über den
    /// Dateiinhalt gefunden worden sein.
    #[test]
    fn finds_matches_inside_files() {
        let root = tree("inhalt");
        let hits = run(&root, Query::Name { needle: "nadel".into(), contents: true });
        let by_content: Vec<_> = hits.iter().filter(|e| e.matched_line.is_some()).collect();
        assert_eq!(by_content.len(), 1, "{hits:#?}");
        assert_eq!(by_content[0].name, "inhalt.txt");
        assert!(by_content[0].matched_line.as_ref().unwrap().contains("Nadel"));

        // Ohne eingeschaltete Inhaltssuche darf dieselbe Datei nicht auftauchen.
        let ohne = run(&root, Query::Name { needle: "nadel".into(), contents: false });
        assert!(ohne.iter().all(|e| e.name != "inhalt.txt"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Ein Namenstreffer muss über jedem Inhaltstreffer stehen — sonst findet
    /// man die Datei nicht wieder, deren Name genau passt.
    #[test]
    fn name_hits_outrank_content_hits() {
        assert!(CONTENT_SCORE_BASE < 0);
        let q = Query::Name { needle: "x".into(), contents: true };
        let named = Entry {
            path: PathBuf::from("/tmp/x.txt"),
            name: "x.txt".into(),
            is_dir: false,
            size: 1,
            modified: None,
            rel_parent: String::new(),
            matched_line: None,
        };
        let (score, line) = q.evaluate(&named).unwrap();
        assert!(score > CONTENT_SCORE_BASE);
        assert!(line.is_none(), "Namenstreffer öffnet die Datei nicht");
    }
}
