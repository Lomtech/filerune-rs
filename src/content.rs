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

    // Der schnelle Weg: rein aszii-Suchbegriffe werden byteweise verglichen,
    // ohne für jede Zeile einen kleingeschriebenen String anzulegen. Genau das
    // war der teuerste Posten der Inhaltssuche.
    let at = if needle.is_ascii() {
        find_ascii_ci(&bytes, needle.as_bytes())?
    } else {
        // Umlaute und dergleichen brauchen echte Unicode-Kleinschreibung;
        // dafür nehmen wir den langsameren Weg in Kauf.
        let text = String::from_utf8_lossy(&bytes);
        let lower = text.to_lowercase();
        // Position im kleingeschriebenen Text ist nicht die im Original, wenn
        // sich die Bytelänge ändert (ẞ→ss). Deshalb hier zeilenweise.
        for (i, line) in text.lines().enumerate() {
            let line_lower = line.to_lowercase();
            if let Some(at) = line_lower.find(needle) {
                return Some(format!(
                    "Zeile {}: {}",
                    i + 1,
                    snippet(line, &line_lower, at, needle.len())
                ));
            }
        }
        let _ = lower;
        return None;
    };

    // Fundstelle gefunden: erst jetzt Zeilennummer und Ausschnitt bestimmen.
    let line_no = bytes[..at].iter().filter(|b| **b == b'\n').count() + 1;
    let line_start = bytes[..at].iter().rposition(|b| *b == b'\n').map_or(0, |p| p + 1);
    let line_end = bytes[at..]
        .iter()
        .position(|b| *b == b'\n')
        .map_or(bytes.len(), |p| at + p);
    let line = String::from_utf8_lossy(&bytes[line_start..line_end]).into_owned();
    let lower = line.to_lowercase();
    let in_line = lower.find(needle).unwrap_or(0);
    Some(format!("Zeile {line_no}: {}", snippet(&line, &lower, in_line, needle.len())))
}

/// Grobe Häufigkeit eines Zeichens in Text — kleiner heißt seltener. Dient nur
/// dazu, im Suchbegriff einen guten Einstiegspunkt zu wählen; die Zahlen müssen
/// nicht stimmen, nur die Reihenfolge grob.
fn commonness(b: u8) -> u8 {
    match b.to_ascii_lowercase() {
        b'e' => 255, b'n' => 240, b't' => 235, b'a' => 230, b'i' => 225,
        b'r' => 220, b's' => 215, b'o' => 205, b'l' => 190, b'h' => 180,
        b'd' => 175, b'u' => 170, b'c' => 160, b'm' => 150, b'g' => 130,
        b'p' => 120, b'f' => 115, b'b' => 105, b'w' => 100, b'y' => 80,
        b'v' => 70, b'k' => 60, b'x' => 30, b'j' => 25, b'q' => 20, b'z' => 20,
        b' ' => 250, b'\t' | b'\n' => 245,
        b'0'..=b'9' => 90,
        // Satz- und Sonderzeichen sind in Fließtext selten, in Quelltext aber
        // häufig — mittig einsortiert, damit sie weder bevorzugt noch gemieden werden.
        _ => 110,
    }
}

/// Aszii-Suche ohne Rücksicht auf Groß- und Kleinschreibung, ohne Allokation.
/// `needle` ist bereits kleingeschrieben.
///
/// Eingestiegen wird über das **seltenste** Zeichen des Suchbegriffs, nicht über
/// das erste: nach dem „p" in „Papierkorb" zu suchen liefert in Quelltext
/// Zehntausende Fehlversuche, nach dem „k" kaum welche. Die Kandidatensuche
/// selbst macht `memchr` mit SIMD.
fn find_ascii_ci(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    let pivot = (0..needle.len())
        .min_by_key(|&i| commonness(needle[i]))
        .unwrap_or(0);
    let lower = needle[pivot];
    let upper = lower.to_ascii_uppercase();

    let mut offset = pivot;
    loop {
        let rest = &hay[offset..];
        let found = if lower == upper {
            memchr::memchr(lower, rest)
        } else {
            memchr::memchr2(lower, upper, rest)
        };
        let at = offset + found?;
        // Vom Fundort des Einstiegszeichens auf den Anfang zurückrechnen.
        let start = at - pivot;
        if start + needle.len() <= hay.len()
            && hay[start..start + needle.len()]
                .iter()
                .zip(needle)
                .all(|(h, n)| h.to_ascii_lowercase() == *n)
        {
            return Some(start);
        }
        offset = at + 1;
    }
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

    /// Der Einstieg über das seltenste Zeichen darf das Ergebnis nicht ändern —
    /// nur die Geschwindigkeit. Treffer am Anfang, in der Mitte und am Ende.
    #[test]
    fn rare_byte_entry_finds_the_same_matches() {
        for (hay, needle, expect) in [
            ("Papierkorb ganz vorn", "papierkorb", Some(0)),
            ("davor Papierkorb dahinter", "papierkorb", Some(6)),
            ("nur am Ende: Papierkorb", "papierkorb", Some(13)),
            ("PAPIERKORB laut", "papierkorb", Some(0)),
            ("kein Treffer hier", "papierkorb", None),
            // Das seltenste Zeichen steht ganz vorn bzw. ganz hinten.
            ("wo ist xylophon", "xylophon", Some(7)),
            ("endet auf zack", "zack", Some(10)),
        ] {
            assert_eq!(
                find_ascii_ci(hay.as_bytes(), needle.as_bytes()),
                expect,
                "{hay:?} / {needle:?}"
            );
        }
    }

    #[test]
    fn snippet_survives_multibyte_characters() {
        let long = format!("{}Nadel{}", "ä".repeat(200), "ö".repeat(200));
        let p = tmp("e.txt", &long);
        let hit = find(&p, std::fs::metadata(&p).unwrap().len(), "nadel").unwrap();
        assert!(hit.contains("Nadel"), "{hit}");
    }
}
