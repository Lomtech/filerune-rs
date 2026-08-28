//! Grobe Messung der Suchpfade, damit Optimierungen an Zahlen hängen und
//! nicht an Vermutungen.  cargo run --release --example bench -- <Ordner>

use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Instant;

#[path = "../src/content.rs"] mod content;
#[path = "../src/entry.rs"] mod entry;
#[path = "../src/filter.rs"] mod filter;
#[path = "../src/fuzzy.rs"] mod fuzzy;
#[path = "../src/scanner.rs"] mod scanner;
#[path = "../src/search.rs"] mod search;

use search::Query;

fn bench(label: &str, root: &PathBuf, query: Query) {
    let gen = Arc::new(AtomicU64::new(1));
    let t = Instant::now();
    let hits = search::search(root, &query, false, 1000, &gen, 1);
    println!("  {label:<34} {:>8.0} ms   {:>5} Treffer", t.elapsed().as_secs_f64() * 1000.0, hits.len());
}

fn main() {
    let root = PathBuf::from(
        std::env::args().nth(1).unwrap_or_else(|| {
            std::env::var("HOME").unwrap_or_else(|_| "/".into())
        }),
    );
    println!("Wurzel: {}", root.display());
    println!("Kerne:  {}", std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0));

    let t = Instant::now();
    let entries = scanner::scan(&root, false).unwrap_or_default();
    println!("  {:<34} {:>8.2} ms   {:>5} Einträge", "Ordner einlesen", t.elapsed().as_secs_f64() * 1000.0, entries.len());

    bench("Name: \"cargo\"", &root, Query::Name { needle: "cargo".into(), contents: false });
    bench("Name: \"z\" (viele Treffer)", &root, Query::Name { needle: "z".into(), contents: false });
    bench("Endung: .rs", &root, Query::Column(filter::ColumnFilter::parse(".rs").unwrap()));
    // Zweimal dasselbe: der zweite Lauf findet den Dateicache warm vor. Die
    // Differenz zeigt, wie viel von der Zeit reines Lesen von der Platte ist.
    bench("Inhalt: \"Papierkorb\" (kalt)", &root, Query::Name { needle: "Papierkorb".into(), contents: true });
    bench("Inhalt: \"Papierkorb\" (warm)", &root, Query::Name { needle: "Papierkorb".into(), contents: true });
    bench("Inhalt: \"fn main\" (warm)", &root, Query::Name { needle: "fn main".into(), contents: true });
    bench("Inhalt: \"qqzxj\" (warm, 0 Treffer)", &root, Query::Name { needle: "qqzxj".into(), contents: true });

    // Aufteilung: wie viel ist Lesen von der Platte, wie viel Suchen?
    {
        use std::io::Read;
        let mut files: Vec<PathBuf> = Vec::new();
        for e in walkdir::WalkDir::new(&root).min_depth(1).into_iter().filter_entry(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            !(e.file_type().is_dir() && (n.starts_with('.') || matches!(n.as_str(),
                "target" | "node_modules" | "Library" | "Caches" | ".git" | "build")))
        }).flatten() {
            if e.file_type().is_file() { files.push(e.path().to_path_buf()); }
        }
        let t = Instant::now();
        let mut bytes = 0usize;
        let mut buf = Vec::new();
        let mut read_files = 0usize;
        for f in &files {
            buf.clear();
            if let Ok(mut fh) = std::fs::File::open(f) {
                if fh.read_to_end(&mut buf).is_ok() { bytes += buf.len(); read_files += 1; }
            }
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        println!("  {:<34} {:>8.0} ms   {read_files} Dateien, {:.0} MB", "NUR Lesen (alle Dateien)", ms, bytes as f64 / 1e6);
    }

    // Großer Ordner: das Einlesen läuft synchron auf dem UI-Thread.
    if let Ok(big) = std::env::var("BIGDIR") {
        let big = PathBuf::from(big);
        let t = Instant::now();
        let entries = scanner::scan(&big, false).unwrap_or_default();
        println!("  {:<34} {:>8.0} ms   {:>5} Einträge", "Großer Ordner einlesen", t.elapsed().as_secs_f64() * 1000.0, entries.len());

        // Sortieren, wie es rebuild_view bei jedem Tastendruck tut.
        let t = Instant::now();
        for _ in 0..10 {
            let mut v = entries.clone();
            v.sort_by(|a, b| b.is_dir.cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
            std::hint::black_box(&v);
        }
        println!("  {:<34} {:>8.0} ms   (10× klonen+sortieren)", "  davon Ansicht neu aufbauen", t.elapsed().as_secs_f64() * 1000.0);

        // Nur der Vergleich, ohne Klonen — zeigt, was to_lowercase kostet.
        let t = Instant::now();
        for _ in 0..10 {
            let mut v: Vec<&entry::Entry> = entries.iter().collect();
            v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            std::hint::black_box(&v);
        }
        println!("  {:<34} {:>8.0} ms   (10×)", "  davon nur sortieren", t.elapsed().as_secs_f64() * 1000.0);

        // Allokationsfreier Vergleich zum Gegenhalten.
        let t = Instant::now();
        for _ in 0..10 {
            let mut v: Vec<&entry::Entry> = entries.iter().collect();
            v.sort_by(|a, b| a.name.chars().flat_map(char::to_lowercase)
                .cmp(b.name.chars().flat_map(char::to_lowercase)));
            std::hint::black_box(&v);
        }
        println!("  {:<34} {:>8.0} ms   (10×)", "  sortieren ohne Allokation", t.elapsed().as_secs_f64() * 1000.0);
    }

    // Sortieren: passiert bei jeder Filteränderung neu.
    let mut many: Vec<entry::Entry> = Vec::new();
    for i in 0..1000 {
        let mut e = entries.first().cloned().unwrap_or_else(|| {
            entry::Entry { path: PathBuf::from("/tmp/x"), name: "x".into(), is_dir: false, size: 0, modified: None, rel_parent: String::new(), matched_line: None }
        });
        e.name = format!("Datei-{:04}-{}.txt", (i * 7919) % 1000, i);
        many.push(e);
    }
    let t = Instant::now();
    for _ in 0..100 {
        let mut v = many.clone();
        v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        std::hint::black_box(&v);
    }
    println!("  {:<34} {:>8.2} ms   (100× 1000 Einträge)", "Sortieren nach Name", t.elapsed().as_secs_f64() * 1000.0);
}
