//! Flaches Einlesen eines Ordners für die Liste.

use crate::entry::Entry;
use std::path::Path;

pub fn scan(dir: &Path, show_hidden: bool) -> std::io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for item in std::fs::read_dir(dir)? {
        let Ok(item) = item else { continue };
        let name = item.file_name().to_string_lossy().into_owned();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        if let Some(e) = Entry::from_path(item.path(), None) {
            entries.push(e);
        }
    }
    sort_default(&mut entries);
    Ok(entries)
}

/// Ordner zuerst, dann Name — der Zustand direkt nach dem Laden, bevor der
/// Sortierstapel des Nutzers greift.
fn sort_default(entries: &mut [Entry]) {
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}
