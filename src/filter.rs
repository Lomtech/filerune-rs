//! Das Spaltenfilter-DSL aus AppModel.columnFilter:
//!   art:csv · typ:pdf · art:ordner      — Art/Endung enthält …
//!   >1mb · <500kb · größe:>2gb          — Größe
//!   datum:2026-05 · datum:>2026-06-01   — Änderungsdatum (ISO yyyy-MM-dd)
//!
//! `ColumnFilter::parse` gibt `None` zurück, wenn die Eingabe kein Spaltenfilter
//! ist — dann übernimmt die normale Namenssuche. Das Prädikat ist vom Ordner
//! losgelöst, damit derselbe Filter auch im rekursiven Tiefenscan gilt.

use crate::entry::{iso_day, Entry};

const KIND_PREFIXES: [&str; 4] = ["art:", "typ:", "type:", "kind:"];
const SIZE_PREFIXES: [&str; 3] = ["größe:", "groesse:", "size:"];
const DATE_PREFIXES: [&str; 3] = ["datum:", "zeit:", "date:"];
/// Höflichkeitspräfixe: der Platzhalter im Suchfeld schreibt „filtern: art:csv",
/// und genau so tippen Leute es dann auch ab.
const LEAD_PREFIXES: [&str; 4] = ["filtern:", "filter:", "filtere:", "filtern"];

#[derive(Debug, Clone, PartialEq)]
pub enum ColumnFilter {
    /// Genau diese Dateiendung — die kurze Schreibweise `.pdf`, `.html`, `.go`.
    Extension(String),
    /// Art oder Dateiendung enthält den Text.
    Kind(String),
    /// Größe größer/kleiner als … — Ordner zählen nie mit.
    Size { greater: bool, bytes: u64 },
    /// ISO-Tag größer/kleiner als …
    DateCompare { greater: bool, target: String },
    /// ISO-Tag enthält den Text (z. B. „2026-05" für einen ganzen Monat).
    DateContains(String),
    /// Filterwort ohne Wert („art:") — bewusst leeres Ergebnis statt aller Dateien.
    Nothing,
}

impl ColumnFilter {
    pub fn parse(raw: &str) -> Option<ColumnFilter> {
        let mut s = raw.trim().to_lowercase();
        // Ein vorangestelltes „filtern:" abstreifen, damit die abgetippte
        // Platzhalterzeile trotzdem tut, was sie verspricht.
        for p in LEAD_PREFIXES {
            if let Some(rest) = s.strip_prefix(p) {
                s = rest.trim().to_string();
                break;
            }
        }
        if s.is_empty() {
            return None;
        }

        // Kurzform: `.pdf`, `.html`, `.go`. Bewusst eng gefasst — ein einzelnes
        // Endungswort, keine Punkte darin. Sonst würde `.tar.gz` oder ein nach
        // einer versteckten Datei gesuchtes `.zshrc` fälschlich hier landen
        // statt in der Namenssuche.
        if let Some(ext) = s.strip_prefix('.') {
            if is_extension_word(ext) {
                return Some(ColumnFilter::Extension(ext.to_string()));
            }
        }

        if let Some(v) = strip_any(&s, &KIND_PREFIXES) {
            return Some(if v.is_empty() {
                ColumnFilter::Nothing
            } else {
                ColumnFilter::Kind(v)
            });
        }

        let size_expr = strip_any(&s, &SIZE_PREFIXES).unwrap_or_else(|| s.clone());
        let mut chars = size_expr.chars();
        if let Some(op @ ('>' | '<')) = chars.next() {
            if let Some(bytes) = parse_size(chars.as_str()) {
                return Some(ColumnFilter::Size {
                    greater: op == '>',
                    bytes,
                });
            }
        }

        if let Some(v) = strip_any(&s, &DATE_PREFIXES) {
            if v.is_empty() {
                return Some(ColumnFilter::Nothing);
            }
            let mut c = v.chars();
            return Some(match c.next() {
                Some(op @ ('>' | '<')) => ColumnFilter::DateCompare {
                    greater: op == '>',
                    target: c.as_str().to_string(),
                },
                _ => ColumnFilter::DateContains(v),
            });
        }

        None
    }

    pub fn matches(&self, e: &Entry) -> bool {
        match self {
            ColumnFilter::Nothing => false,
            ColumnFilter::Extension(v) => e.ext_lower() == *v,
            ColumnFilter::Kind(v) => {
                e.kind().to_lowercase().contains(v) || e.ext_lower().contains(v)
            }
            ColumnFilter::Size { greater, bytes } => {
                // Ordner haben keine sinnvolle Größe und dürfen nie durchrutschen.
                !e.is_dir && if *greater { e.size > *bytes } else { e.size < *bytes }
            }
            ColumnFilter::DateCompare { greater, target } => match iso_day(e.modified) {
                Some(iso) => {
                    if *greater {
                        iso.as_str() > target.as_str()
                    } else {
                        iso.as_str() < target.as_str()
                    }
                }
                None => false,
            },
            ColumnFilter::DateContains(v) => {
                iso_day(e.modified).is_some_and(|iso| iso.contains(v))
            }
        }
    }
}

/// Sieht das nach einer Dateiendung aus? Buchstaben und Ziffern, höchstens
/// zwölf Zeichen — lang genug für `.markdown`, zu kurz für einen Satz.
fn is_extension_word(s: &str) -> bool {
    !s.is_empty()
        && s.chars().count() <= 12
        && s.chars().all(|c| c.is_alphanumeric())
}

fn strip_any(s: &str, prefixes: &[&str]) -> Option<String> {
    prefixes
        .iter()
        .find_map(|p| s.strip_prefix(p))
        .map(|rest| rest.trim().to_string())
}

/// „1.5mb", „500 kb", „2gb", „1024" → Bytes. Komma gilt als Dezimaltrenner.
pub fn parse_size(s: &str) -> Option<u64> {
    let t = s.trim().to_lowercase();
    let num: String = t
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .collect();
    let value: f64 = num.replace(',', ".").parse().ok()?;
    if value < 0.0 {
        return None;
    }
    let unit = t[num.len()..].trim();
    let factor: f64 = match unit {
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        "" | "b" => 1.0,
        _ => return None,
    };
    Some((value * factor) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    fn entry(name: &str, is_dir: bool, size: u64, epoch_secs: u64) -> Entry {
        Entry {
            path: PathBuf::from("/tmp").join(name),
            name: name.to_string(),
            is_dir,
            size,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(epoch_secs)),
            rel_parent: String::new(),
        }
    }

    fn corpus() -> Vec<Entry> {
        vec![
            entry("daten.csv", false, 2 * 1024 * 1024, 1_780_000_000),
            entry("bild.png", false, 500, 1_780_000_000),
            entry("Projekte", true, 0, 1_780_000_000),
        ]
    }

    fn apply(q: &str, c: &[Entry]) -> Vec<String> {
        let f = ColumnFilter::parse(q).expect("kein Spaltenfilter");
        c.iter()
            .filter(|e| f.matches(e))
            .map(|e| e.name.clone())
            .collect()
    }

    #[test]
    fn kind_filter_matches_extension() {
        assert_eq!(apply("art:csv", &corpus()), ["daten.csv"]);
    }

    #[test]
    fn kind_filter_matches_folders() {
        assert_eq!(apply("art:ordner", &corpus()), ["Projekte"]);
    }

    #[test]
    fn size_filter_excludes_directories() {
        assert_eq!(apply(">1mb", &corpus()), ["daten.csv"]);
        assert_eq!(apply("<1kb", &corpus()), ["bild.png"]);
    }

    #[test]
    fn plain_text_is_not_a_column_filter() {
        assert!(ColumnFilter::parse("daten").is_none());
    }

    /// Der Platzhalter im Suchfeld lautet „filtern: art:csv" — genau so tippen
    /// Leute es ab, und dann muss es auch greifen.
    #[test]
    fn tolerates_the_placeholder_wording() {
        assert_eq!(apply("filtern: art: pdf", &corpus()), Vec::<String>::new());
        assert_eq!(apply("filtern: art:csv", &corpus()), ["daten.csv"]);
        assert_eq!(apply("filter: >1mb", &corpus()), ["daten.csv"]);
        // Leerzeichen hinter dem Doppelpunkt ebenso.
        assert_eq!(apply("art: csv", &corpus()), ["daten.csv"]);
    }

    /// Die kurze Schreibweise, um die es eigentlich geht.
    #[test]
    fn leading_dot_filters_by_extension() {
        assert_eq!(apply(".csv", &corpus()), ["daten.csv"]);
        assert_eq!(apply(".png", &corpus()), ["bild.png"]);
        assert_eq!(apply(".PNG", &corpus()), ["bild.png"]);
        assert_eq!(apply(".pdf", &corpus()), Vec::<String>::new());
    }

    /// Genau, nicht „enthält": `.go` darf keine `.gohtml` einsammeln.
    #[test]
    fn extension_filter_is_exact() {
        let c = vec![
            entry("main.go", false, 10, 1_780_000_000),
            entry("seite.gohtml", false, 10, 1_780_000_000),
        ];
        assert_eq!(apply(".go", &c), ["main.go"]);
    }

    /// Versteckte Dateien und mehrteilige Endungen gehören in die Namenssuche,
    /// nicht in den Endungsfilter.
    #[test]
    fn dotted_names_stay_a_name_search() {
        assert!(ColumnFilter::parse(".zshrc").is_some()); // sieht aus wie eine Endung
        assert!(ColumnFilter::parse(".tar.gz").is_none()); // Punkt darin
        assert!(ColumnFilter::parse(".").is_none());
        assert!(ColumnFilter::parse(". pdf").is_none());
    }

    #[test]
    fn size_units_parse() {
        assert_eq!(parse_size("1024"), Some(1024));
        assert_eq!(parse_size("1kb"), Some(1024));
        assert_eq!(parse_size("1,5mb"), Some(1_572_864));
        assert_eq!(parse_size("2 gb"), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_size("abc"), None);
        assert_eq!(parse_size("5xyz"), None);
    }

    #[test]
    fn date_filter_compares_and_contains() {
        let c = corpus();
        let iso = crate::entry::iso_day(c[0].modified).unwrap();
        let month = &iso[..7];
        assert_eq!(apply(&format!("datum:{month}"), &c).len(), 3);
        assert_eq!(apply("datum:>2099-01-01", &c).len(), 0);
        assert_eq!(apply("datum:<2099-01-01", &c).len(), 3);
    }

    #[test]
    fn filter_word_without_value_matches_nothing() {
        assert_eq!(ColumnFilter::parse("art:"), Some(ColumnFilter::Nothing));
        assert_eq!(apply("art:", &corpus()).len(), 0);
    }
}
