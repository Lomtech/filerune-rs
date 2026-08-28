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
    let Some(t) = t else { return String::new() };
    let Ok(dur) = t.duration_since(SystemTime::UNIX_EPOCH) else {
        return String::new();
    };
    let (y, mo, d, h, mi) = civil_local(dur.as_secs() as i64);
    format!("{d:02}.{mo:02}.{:02}, {h:02}:{mi:02}", y % 100)
}

/// Änderungsdatum als ISO-Tag (yyyy-MM-dd) — Vergleichsbasis des `datum:`-Filters.
pub fn iso_day(t: Option<SystemTime>) -> Option<String> {
    let dur = t?.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    let (y, mo, d, _, _) = civil_local(dur.as_secs() as i64);
    Some(format!("{y:04}-{mo:02}-{d:02}"))
}

/// Unix-Sekunden → lokale Kalenderzeit. Der Offset kommt einmalig aus libc
/// (`localtime_r`), damit Sommerzeit und Zone stimmen, ohne chrono/time-Crate.
fn civil_local(unix: i64) -> (i64, u32, u32, u32, u32) {
    let secs = unix + local_offset_seconds(unix);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, mo, d) = civil_from_days(days);
    (y, mo, d, (rem / 3600) as u32, ((rem % 3600) / 60) as u32)
}

fn local_offset_seconds(unix: i64) -> i64 {
    // `struct tm` ist plattformabhängig groß; 64 Zeiger-große Felder sind reichlich
    // Platz, und wir lesen nur tm_gmtoff über die dokumentierte Feldposition aus.
    extern "C" {
        fn localtime_r(time: *const i64, tm: *mut Tm) -> *mut Tm;
    }
    #[repr(C)]
    #[derive(Default)]
    struct Tm {
        sec: i32,
        min: i32,
        hour: i32,
        mday: i32,
        mon: i32,
        year: i32,
        wday: i32,
        yday: i32,
        isdst: i32,
        gmtoff: i64,
        zone: *const i8,
    }
    let mut tm = Tm {
        zone: std::ptr::null(),
        ..Default::default()
    };
    let t = unix;
    unsafe {
        if localtime_r(&t, &mut tm).is_null() {
            return 0;
        }
    }
    tm.gmtoff
}

/// Howard Hinnants `civil_from_days` — Tage seit 1970-01-01 → (Jahr, Monat, Tag).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
