//! Volltextsuche IN Dateien — gelesen wird direkt, ohne Suchindex, damit es in
//! jedem Ordner funktioniert und auf allen drei Systemen gleich. Portierung von
//! ContentSearch.swift; die Endungsliste ist dieselbe.

use std::path::Path;

/// Dateien größer als das werden nicht gelesen.
const MAX_FILE_SIZE: u64 = 16 * 1024 * 1024;
/// So viel Kontext steht in der Fundstelle.
const SNIPPET_CHARS: usize = 90;

/// Endungen, die als Text gelesen werden. Alles andere wird nicht angefasst —
/// eine Endungsliste ist billiger und verlässlicher, als jede Datei aufzumachen
/// und zu raten, ob sie Text ist.
const TEXT_EXTS: &[&str] = &[
    "txt", "text", "md", "markdown", "rst", "log", "csv", "tsv", "json", "ndjson",
    "yaml", "yml", "toml", "ini", "cfg", "conf", "env", "properties", "plist",
    "xml", "html", "htm", "css", "scss", "svg", "tex", "srt", "vtt", "sql",
    "swift", "js", "mjs", "cjs", "ts", "tsx", "jsx", "py", "rb", "go", "rs",
    "c", "h", "cpp", "hpp", "cc", "cxx", "m", "mm", "java", "kt", "kts", "scala",
    "sh", "bash", "zsh", "fish", "pl", "php", "lua", "r", "jl", "dart", "vue",
    "gradle", "make", "mk", "cmake", "dockerfile", "gitignore", "editorconfig",
    "ex", "exs", "erl", "hs", "ml", "nim", "zig", "abap", "bat", "ps1", "rc",
];

/// Dateien ohne Endung, die trotzdem Text sind.
const TEXT_NAMES: &[&str] = &[
    "Makefile", "Dockerfile", "Rakefile", "Gemfile", "Procfile", "LICENSE",
    "README", "CHANGELOG", "CMakeLists.txt",
];

fn is_text_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        return TEXT_EXTS.contains(&ext.to_lowercase().as_str());
    }
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| TEXT_NAMES.contains(&n))
}

/// Sucht `needle` (bereits kleingeschrieben) im Inhalt und gibt die Fundstelle
/// als „Zeile N: …" zurück. `None`, wenn nichts passt oder die Datei nicht
/// gelesen wird.
pub fn find(path: &Path, size: u64, needle: &str) -> Option<String> {
    if size == 0 || size > MAX_FILE_SIZE || needle.is_empty() || !is_text_file(path) {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    // Ein NUL im ersten Kilobyte heißt Binärdatei — trotz passender Endung.
    if bytes.iter().take(1024).any(|b| *b == 0) {
        return None;
    }
    let text = String::from_utf8_lossy(&bytes);

    for (i, line) in text.lines().enumerate() {
        let lower = line.to_lowercase();
        if let Some(at) = lower.find(needle) {
            return Some(format!("Zeile {}: {}", i + 1, snippet(line, &lower, at, needle.len())));
        }
    }
    None
}

/// Schneidet einen lesbaren Ausschnitt um die Fundstelle heraus.
fn snippet(line: &str, lower: &str, byte_at: usize, needle_len: usize) -> String {
    // Bytepositionen auf Zeichenpositionen bringen — sonst zerschneidet das
    // Kürzen mehrbytige Zeichen.
    let start_char = lower[..byte_at].chars().count();
    let needle_chars = lower[byte_at..byte_at + needle_len].chars().count();
    let chars: Vec<char> = line.chars().collect();

    // Etwas Vorlauf, damit die Fundstelle im Zusammenhang steht.
    let lead = SNIPPET_CHARS / 3;
    let from = start_char.saturating_sub(lead);
    let to = (from + SNIPPET_CHARS).min(chars.len());

    let mut out = String::new();
    if from > 0 {
        out.push('…');
    }
    out.extend(&chars[from..to.max(from + needle_chars.min(chars.len() - from))]);
    if to < chars.len() {
        out.push('…');
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp(name: &str, body: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("filerune-content-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn finds_text_and_reports_the_line() {
        let p = tmp("a.txt", "erste Zeile\nzweite mit Nadel drin\ndritte\n");
        let hit = find(&p, std::fs::metadata(&p).unwrap().len(), "nadel").unwrap();
        assert!(hit.starts_with("Zeile 2:"), "{hit}");
        assert!(hit.contains("Nadel"));
    }

    #[test]
    fn ignores_unknown_extensions() {
        let p = tmp("b.bin", "Nadel");
        assert!(find(&p, 5, "nadel").is_none());
    }

    #[test]
    fn ignores_binary_despite_text_extension() {
        let p = tmp("c.txt", "Nadel");
        std::fs::write(&p, b"\x00\x01Nadel").unwrap();
        assert!(find(&p, 7, "nadel").is_none());
    }

    #[test]
    fn respects_the_size_cap() {
        let p = tmp("d.txt", "Nadel");
        assert!(find(&p, MAX_FILE_SIZE + 1, "nadel").is_none());
        assert!(find(&p, 0, "nadel").is_none());
    }

    #[test]
    fn snippet_survives_multibyte_characters() {
        let long = format!("{}Nadel{}", "ä".repeat(200), "ö".repeat(200));
        let p = tmp("e.txt", &long);
        let hit = find(&p, std::fs::metadata(&p).unwrap().len(), "nadel").unwrap();
        assert!(hit.contains("Nadel"), "{hit}");
    }
}
