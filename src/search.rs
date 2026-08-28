//! Rekursive Namenssuche über einen Unterbaum:
//!  • durchsucht die Unterordner der obersten Ebene **parallel** (ein Worker je Kern),
//!  • überspringt Müll- und Systembäume (node_modules/.git/target/Caches/Library/…),
//!    damit die Suche bei den Dateien des Nutzers landet statt in Caches zu ertrinken,
//!  • fuzzy-matcht Namen, sortiert nach Score und bricht ab, sobald eine neuere
//!    Suche startet (Generationszähler).

use crate::entry::Entry;
use crate::filter::ColumnFilter;
use crate::fuzzy;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
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

fn should_skip(name: &str, path: &Path) -> bool {
    SKIP_NAMES.contains(&name) || SKIP_PATHS.iter().any(|p| path == Path::new(p))
}

/// Wonach gesucht wird: nach dem Namen oder nach einer Spalte. Beides läuft
/// durch dieselbe Maschinerie, damit auch `art:pdf` in die Unterordner reicht.
#[derive(Clone)]
pub enum Query {
    Name(String),
    Column(ColumnFilter),
}

impl Query {
    /// Bewertung eines Eintrags, oder `None` wenn er nicht passt. Spaltenfilter
    /// kennen keine Rangfolge — dort entscheidet allein der Sortierstapel.
    fn score(&self, entry: &Entry) -> Option<i32> {
        match self {
            Query::Name(q) => fuzzy::score(q, &entry.name),
            Query::Column(f) => f.matches(entry).then_some(0),
        }
    }

    /// Billige Vorauswahl allein am Dateinamen, bevor der Eintrag mit einem
    /// `stat` teuer aufgebaut wird. Spaltenfilter brauchen Größe und Datum,
    /// können hier also nichts ausschließen.
    fn may_match_name(&self, name: &str) -> bool {
        match self {
            Query::Name(q) => fuzzy::score(q, name).is_some(),
            Query::Column(_) => true,
        }
    }
}

/// Sofortige Treffer im bereits geladenen Ordner — steht vor dem Tiefenscan da.
pub fn local_matches(query: &Query, entries: &[Entry]) -> Vec<Entry> {
    let mut scored: Vec<(Entry, i32)> = entries
        .iter()
        .filter_map(|e| query.score(e).map(|s| (e.clone(), s)))
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored.into_iter().map(|(e, _)| e).collect()
}

/// Der Tiefenscan. Bricht ab, sobald `generation` nicht mehr `my_gen` ist.
pub fn search(
    root: &Path,
    query: &Query,
    show_hidden: bool,
    limit: usize,
    generation: &Arc<AtomicU64>,
    my_gen: u64,
) -> Vec<Entry> {
    let cancelled = || generation.load(Ordering::Relaxed) != my_gen;

    let Ok(read) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut top_dirs: Vec<PathBuf> = Vec::new();
    let mut collected: Vec<(Entry, i32)> = Vec::new();

    for item in read.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let path = item.path();
        let is_dir = item.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir && !should_skip(&name, &path) {
            top_dirs.push(path.clone());
        }
        if query.may_match_name(&name) {
            if let Some(e) = Entry::from_path(path, Some(root)) {
                if let Some(s) = query.score(&e) {
                    collected.push((e, s));
                }
            }
        }
    }

    if cancelled() {
        return Vec::new();
    }

    // Ein Worker je Kern zieht sich Unterbäume aus einer gemeinsamen Warteschlange.
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(top_dirs.len().max(1));

    let partials: Vec<Vec<(Entry, i32)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                let next = &next;
                let top_dirs = &top_dirs;
                scope.spawn(move || {
                    let mut out: Vec<(Entry, i32)> = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(dir) = top_dirs.get(i) else { break };
                        if cancelled() {
                            break;
                        }
                        out.extend(walk(dir, query, show_hidden, root, limit, &cancelled));
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
    // Schleife oben, einmal als Wurzel seines eigenen Laufs. `FileManager.enumerator`
    // der Swift-Fassung tut das von sich aus nicht.
    let walker = WalkDir::new(dir).min_depth(1).into_iter().filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        if !show_hidden && name.starts_with('.') && e.depth() > 0 {
            return false;
        }
        !(e.file_type().is_dir() && should_skip(&name, e.path()))
    });

    for item in walker.flatten() {
        if scanned & 0x7FF == 0 && cancelled() {
            return hits;
        }
        scanned += 1;
        if scanned > MAX_SCANNED_PER_SUBTREE {
            break;
        }

        let name = item.file_name().to_string_lossy();
        if !query.may_match_name(&name) {
            continue;
        }
        let Some(e) = Entry::from_path(item.path().to_path_buf(), Some(root)) else {
            continue;
        };
        let Some(s) = query.score(&e) else {
            continue;
        };
        hits.push((e, s));

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

    #[test]
    fn skips_system_roots_but_not_lookalikes() {
        assert!(should_skip("System", Path::new("/System")));
        // Ein gleichnamiger Ordner tiefer im Baum darf nicht mitgesperrt werden.
        assert!(!should_skip("System", Path::new("/Users/x/System")));
    }
}
