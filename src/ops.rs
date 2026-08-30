//! Dateioperationen. Jede gibt eine Meldung für die Statuszeile zurück oder
//! einen Fehlertext — der Aufrufer lädt danach neu.

use std::path::{Path, PathBuf};

pub type OpResult = Result<String, String>;

/// Ist das ein schlichter Dateiname — kein Pfad, kein `..`, kein Trenner?
///
/// Absichtlich über `Components` statt über `contains('/')`: auf Windows ist
/// auch `\` ein Trenner, auf Unix dagegen ein erlaubtes Zeichen im Dateinamen.
/// Die Komponentenzerlegung kennt die Regel des jeweiligen Systems.
fn is_plain_file_name(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    matches!(parts.next(), Some(std::path::Component::Normal(_))) && parts.next().is_none()
}

/// Rekursiv kopieren; `std::fs::copy` kann nur Dateien.
fn copy_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    let md = std::fs::symlink_metadata(src)?;
    if md.is_dir() {
        std::fs::create_dir(dst)?;
        for item in std::fs::read_dir(src)? {
            let item = item?;
            copy_recursive(&item.path(), &dst.join(item.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(src, dst).map(|_| ())
    }
}

/// „name Kopie“, „name Kopie 2“, … — der erste freie Name im Zielordner.
pub fn unique_copy_name(dir: &Path, file_name: &str) -> PathBuf {
    let p = Path::new(file_name);
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    let stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);

    let make = |suffix: &str| -> PathBuf {
        let name = if ext.is_empty() {
            format!("{stem} {suffix}")
        } else {
            format!("{stem} {suffix}.{ext}")
        };
        dir.join(name)
    };

    let mut dst = make("Kopie");
    let mut n = 2;
    while dst.exists() {
        dst = make(&format!("Kopie {n}"));
        n += 1;
    }
    dst
}

/// Zählt rekursiv, was in einem Ordner steckt — höchstens `cap` Einträge.
/// Gibt (Anzahl, gedeckelt) zurück. Gedeckelt, weil die Zahl nur dazu dient,
/// vor dem Löschen zu zeigen, wie viel daran hängt; für „mehr als 5000" muss
/// niemand eine Minute warten.
pub fn count_entries(path: &Path, cap: usize) -> (usize, bool) {
    let mut count = 0usize;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else { continue };
        for item in read.flatten() {
            count += 1;
            if count >= cap {
                return (cap, true);
            }
            if item.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                stack.push(item.path());
            }
        }
    }
    (count, false)
}

pub fn move_to_trash(paths: &[PathBuf]) -> OpResult {
    if paths.is_empty() {
        return Ok(String::new());
    }
    trash::delete_all(paths).map_err(|e| format!("Papierkorb: {e}"))?;
    Ok(if paths.len() == 1 {
        format!(
            "In den Papierkorb · {}",
            paths[0].file_name().unwrap_or_default().to_string_lossy()
        )
    } else {
        format!("{} in den Papierkorb gelegt", paths.len())
    })
}

pub fn rename(path: &Path, new_name: &str) -> OpResult {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err("Name darf nicht leer sein.".into());
    }
    let old_name = path.file_name().unwrap_or_default().to_string_lossy();
    if trimmed == old_name {
        return Ok(String::new());
    }
    if !is_plain_file_name(trimmed) {
        return Err("Name darf kein Pfad sein.".into());
    }
    let dst = path.with_file_name(trimmed);
    if dst.exists() {
        return Err(format!("„{trimmed}“ existiert bereits."));
    }
    std::fs::rename(path, &dst).map_err(|e| format!("Umbenennen: {e}"))?;
    Ok(format!("Umbenannt · {trimmed}"))
}

/// Dupliziert einen Eintrag als „name Kopie“. Gibt (Meldung, neuer Name) zurück.
pub fn duplicate(path: &Path) -> Result<(String, String), String> {
    let dir = path.parent().ok_or("Kein übergeordneter Ordner.")?;
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let dst = unique_copy_name(dir, &file_name);
    copy_recursive(path, &dst).map_err(|e| format!("Duplizieren: {e}"))?;
    let created = dst.file_name().unwrap_or_default().to_string_lossy().into_owned();
    Ok((format!("Dupliziert · {created}"), created))
}

/// Legt einen Ordner an und weicht auf „Name 2“, „Name 3“ … aus, wenn belegt.
pub fn new_folder(parent: &Path, name: &str) -> Result<(String, String), String> {
    let base = if name.trim().is_empty() {
        "Neuer Ordner"
    } else {
        name.trim()
    };
    if !is_plain_file_name(base) {
        return Err("Name darf kein Pfad sein.".into());
    }
    let mut candidate = parent.join(base);
    let mut n = 2;
    while candidate.exists() {
        candidate = parent.join(format!("{base} {n}"));
        n += 1;
    }
    std::fs::create_dir(&candidate).map_err(|e| format!("Ordner anlegen: {e}"))?;
    let created = candidate.file_name().unwrap_or_default().to_string_lossy().into_owned();
    Ok((format!("Ordner erstellt · {created}"), created))
}

/// Fügt kopierte Einträge ein — überschreibt nie, hängt bei Kollision
/// „ Kopie“ an. Gibt (Meldung, zuletzt angelegter Name) zurück.
pub fn paste(sources: &[PathBuf], dest_dir: &Path) -> Result<(String, Option<String>), String> {
    let mut pasted = 0usize;
    let mut last_name: Option<String> = None;
    let mut last_err: Option<String> = None;

    for src in sources {
        let Some(file_name) = src.file_name() else { continue };
        let mut dst = dest_dir.join(file_name);
        if dst == *src || dst.exists() {
            dst = unique_copy_name(dest_dir, &file_name.to_string_lossy());
        }
        match copy_recursive(src, &dst) {
            Ok(()) => {
                pasted += 1;
                last_name = dst.file_name().map(|n| n.to_string_lossy().into_owned());
            }
            Err(e) => last_err = Some(format!("Einfügen: {e}")),
        }
    }

    if pasted == 0 {
        return Err(last_err.unwrap_or_else(|| "Nichts eingefügt.".into()));
    }
    let msg = if pasted == 1 {
        format!("Eingefügt · {}", last_name.clone().unwrap_or_default())
    } else {
        format!("{pasted} eingefügt")
    };
    Ok((msg, last_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("filerune-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn counts_entries_recursively_and_stops_at_the_cap() {
        let d = tmpdir("count");
        std::fs::create_dir_all(d.join("a/b")).unwrap();
        std::fs::write(d.join("a/eins.txt"), b"x").unwrap();
        std::fs::write(d.join("a/b/zwei.txt"), b"x").unwrap();
        // a, a/b, a/eins.txt, a/b/zwei.txt
        assert_eq!(count_entries(&d, 100), (4, false));
        // Gedeckelt: meldet den Deckel und dass es mehr sind.
        assert_eq!(count_entries(&d, 2), (2, true));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn unique_copy_name_keeps_extension_and_counts_up() {
        let d = tmpdir("uniq");
        std::fs::write(d.join("a.txt"), b"x").unwrap();
        let first = unique_copy_name(&d, "a.txt");
        assert_eq!(first.file_name().unwrap(), "a Kopie.txt");
        std::fs::write(&first, b"x").unwrap();
        let second = unique_copy_name(&d, "a.txt");
        assert_eq!(second.file_name().unwrap(), "a Kopie 2.txt");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn duplicate_copies_directories_recursively() {
        let d = tmpdir("dup");
        let src = d.join("ordner");
        std::fs::create_dir_all(src.join("tief")).unwrap();
        std::fs::write(src.join("tief/datei.txt"), b"inhalt").unwrap();
        duplicate(&src).unwrap();
        let copied = d.join("ordner Kopie/tief/datei.txt");
        assert_eq!(std::fs::read(copied).unwrap(), b"inhalt");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn rename_refuses_existing_target() {
        let d = tmpdir("ren");
        std::fs::write(d.join("a.txt"), b"a").unwrap();
        std::fs::write(d.join("b.txt"), b"b").unwrap();
        assert!(rename(&d.join("a.txt"), "b.txt").is_err());
        // Die Quelle muss unangetastet bleiben.
        assert_eq!(std::fs::read(d.join("a.txt")).unwrap(), b"a");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn rename_rejects_path_separators() {
        let d = tmpdir("ren2");
        std::fs::write(d.join("a.txt"), b"a").unwrap();
        assert!(rename(&d.join("a.txt"), "../entkommen.txt").is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// Ein Name darf nie aus seinem Ordner herausführen — auch nicht über
    /// Schreibweisen, die nur auf einem der Systeme ein Pfad sind.
    #[test]
    fn plain_file_names_only() {
        assert!(is_plain_file_name("bericht.pdf"));
        assert!(is_plain_file_name("mit leerzeichen.txt"));
        assert!(!is_plain_file_name(".."));
        assert!(!is_plain_file_name("../raus.txt"));
        assert!(!is_plain_file_name("unter/ordner.txt"));
        assert!(!is_plain_file_name("/absolut.txt"));
        assert!(!is_plain_file_name(""));
        // Auf Windows ist der Rückwärtsstrich ein Trenner, auf Unix ein
        // erlaubtes Zeichen — beides ist hier richtig.
        assert_eq!(is_plain_file_name(r"..\raus.txt"), !cfg!(windows));
    }

    #[test]
    fn new_folder_avoids_collision() {
        let d = tmpdir("nf");
        let (_, first) = new_folder(&d, "Test").unwrap();
        assert_eq!(first, "Test");
        let (_, second) = new_folder(&d, "Test").unwrap();
        assert_eq!(second, "Test 2");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn paste_never_overwrites() {
        let d = tmpdir("paste");
        let src = d.join("quelle");
        let dst = d.join("ziel");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(src.join("f.txt"), b"neu").unwrap();
        std::fs::write(dst.join("f.txt"), b"alt").unwrap();
        paste(&[src.join("f.txt")], &dst).unwrap();
        assert_eq!(std::fs::read(dst.join("f.txt")).unwrap(), b"alt");
        assert_eq!(std::fs::read(dst.join("f Kopie.txt")).unwrap(), b"neu");
        std::fs::remove_dir_all(&d).unwrap();
    }
}
