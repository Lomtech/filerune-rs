//! Alles, was pro Betriebssystem verschieden ist, hinter einer gemeinsamen
//! Schnittstelle. Auf macOS kommen Icons, Schriften und Akzentfarbe echt aus
//! AppKit — dort soll die App eine exakte Kopie der SwiftUI-Fassung sein.
//! Auf Windows und Linux zeichnet die App dieselben Symbole selbst, damit sie
//! ohne Systemabhängigkeit überall gleich aussieht.

#[cfg_attr(target_vendor = "apple", path = "macos.rs")]
#[cfg_attr(not(target_vendor = "apple"), path = "generic.rs")]
mod backend;

pub use backend::{install_fonts, system_accent, IconCache};

/// Kantenlänge der Zeilensymbole in Punkten — das `.frame(width: 16, height: 16)`
/// der SwiftUI-Zeile.
pub const ICON_POINTS: f32 = 16.0;

/// Beschriftung der Tastenkürzel. Auf macOS die Sondertasten wie im Original,
/// sonst ausgeschrieben — „⌘⇧E" heißt unter Windows „Strg+Umschalt+E".
///
/// Die Bindung selbst ist überall dieselbe: egui bildet `Modifiers::COMMAND`
/// auf macOS auf ⌘ ab und sonst auf Strg. Nur bei der Seitenleiste fällt das
/// zusammen (⌃⌘S wird zu Strg+S), deshalb steht dort ein eigener Text.
#[cfg(target_vendor = "apple")]
pub mod sc {
    pub const SIDEBAR: &str = "⌃⌘S";
    pub const BACK: &str = "⌘←";
    pub const FORWARD: &str = "⌘→";
    pub const UP: &str = "⌘↑";
    pub const HOME: &str = "⌘⇧H";
    pub const RELOAD: &str = "⌘R";
    pub const HIDDEN: &str = "⌘⇧.";
    pub const ZED: &str = "⌘⇧E";
    pub const COPY_PATH: &str = "⌘⌥C";
    pub const FAVORITE: &str = "⌘⇧D";
    pub const HINTS: &str = "⏎ öffnen · ⌘⇧E Zed · ⌘⇧T Terminal · ⌘↑ hoch · ⌘⌫ Papierkorb";
}

#[cfg(not(target_vendor = "apple"))]
pub mod sc {
    pub const SIDEBAR: &str = "Strg+S";
    pub const BACK: &str = "Strg+←";
    pub const FORWARD: &str = "Strg+→";
    pub const UP: &str = "Strg+↑";
    pub const HOME: &str = "Strg+Umschalt+H";
    pub const RELOAD: &str = "Strg+R";
    pub const HIDDEN: &str = "Strg+Umschalt+.";
    pub const ZED: &str = "Strg+Umschalt+E";
    pub const COPY_PATH: &str = "Strg+Alt+C";
    pub const FAVORITE: &str = "Strg+Umschalt+D";
    pub const HINTS: &str =
        "Enter öffnen · Strg+Umschalt+E Zed · Strg+Umschalt+T Terminal · Strg+↑ hoch · Strg+Entf Papierkorb";
}
