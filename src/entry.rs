//! Das Zeilenmodell der Liste: ein Eintrag im aktuellen Ordner (oder ein Suchtreffer).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// Ordnerpfad relativ zur Suchwurzel ("" wenn direkt darin) — nur bei Suchtreffern gesetzt.
    pub rel_parent: String,
    /// Die Fundstelle, wenn der Treffer aus dem Dateiinhalt kam statt aus dem Namen.
    pub matched_line: Option<String>,
}

impl Entry {
    /// Der Wert der Finder-Spalte „Art“.
    pub fn kind(&self) -> String {
        if self.is_dir {
            return "Ordner".to_string();
        }
        match self.path.extension().and_then(|e| e.to_str()) {
            Some(ext) if !ext.is_empty() => ext.to_uppercase(),
            _ => "Dokument".to_string(),
        }
    }

    pub fn ext_lower(&self) -> String {
        self.path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase()
    }

    /// Setzt den Ordnerpfad relativ zur Suchwurzel. Bewusst getrennt vom
    /// Anlegen: bei der Inhaltssuche wird jede Datei zum Kandidaten, und ein
    /// String für jeden Kandidaten wäre reine Verschwendung — die allermeisten
    /// passen nicht.
    pub fn set_rel_parent(&mut self, root: &Path) {
        self.rel_parent = rel_parent_of(&self.path, root);
    }

    /// Baut einen Eintrag aus einem Pfad; `rel_parent` wird gegen `root` bestimmt.
    pub fn from_path(path: PathBuf, root: Option<&Path>) -> Option<Entry> {
        let md = std::fs::symlink_metadata(&path).ok()?;
        let name = path.file_name()?.to_string_lossy().into_owned();
        // Symlinks folgen wir für „ist das ein Ordner?“, damit verlinkte Ordner
        // navigierbar bleiben — die Größe bleibt aber die des Links selbst.
        let is_dir = if md.file_type().is_symlink() {
            std::fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false)
        } else {
            md.is_dir()
        };
        let rel_parent = match root {
            Some(root) => rel_parent_of(&path, root),
            None => String::new(),
        };
        Some(Entry {
            name,
            is_dir,
            size: if is_dir { 0 } else { md.len() },
            modified: md.modified().ok(),
            path,
            rel_parent,
            matched_line: None,
        })
    }
}

fn rel_parent_of(path: &Path, root: &Path) -> String {
    let Some(parent) = path.parent() else {
        return String::new();
    };
    match parent.strip_prefix(root) {
        Ok(rel) => rel.to_string_lossy().into_owned(),
        Err(_) => parent.to_string_lossy().into_owned(),
    }
}

/// Größe im Finder-Stil (dezimal, wie ByteCountFormatter mit .file).
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if value >= 100.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Änderungsdatum als „TT.MM.JJ, HH:MM“ — das Kurzformat der SwiftUI-Fassung.
pub fn format_date(t: Option<SystemTime>) -> String {
    match t {
        Some(t) => chrono::DateTime::<chrono::Local>::from(t)
            .format("%d.%m.%y, %H:%M")
            .to_string(),
        None => String::new(),
    }
}

/// Änderungsdatum als ISO-Tag (yyyy-MM-dd) — Vergleichsbasis des `datum:`-Filters.
pub fn iso_day(t: Option<SystemTime>) -> Option<String> {
    Some(
        chrono::DateTime::<chrono::Local>::from(t?)
            .format("%Y-%m-%d")
            .to_string(),
    )
}
