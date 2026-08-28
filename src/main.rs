// Unter Windows kein Konsolenfenster hinter der App öffnen.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! FileRune — tastaturgetriebener Finder-Ersatz.
//!
//! Rust-Fassung der SwiftUI-App: dieselbe Bedienung, dieselben Kürzel, dieselbe
//! Such- und Filterlogik — nur ohne AppKit. Oberfläche: egui/eframe.

mod content;
mod entry;
mod filter;
mod fuzzy;
mod platform;
mod integrate;
mod ops;
mod scanner;
mod search;
mod state;
mod ui;

/// Das App-Icon für Windows und Linux. Ohne gesetztes Icon legt eframe sein
/// eigenes Standardsymbol an (siehe NativeOptions::viewport).
#[cfg(not(target_os = "macos"))]
const APP_ICON: &[u8] = include_bytes!("../icon.png");

fn main() -> eframe::Result<()> {
    // Fenster dort und so groß öffnen, wie es zuletzt stand.
    let (saved_size, saved_pos) = state::saved_window_geometry();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(saved_size.unwrap_or(state::DEFAULT_WINDOW_SIZE))
        .with_min_inner_size(state::MIN_WINDOW_SIZE)
        .with_title("FileRune");
    if let Some(pos) = saved_pos {
        viewport = viewport.with_position(pos);
    }
    // Auf macOS bewusst ein LEERES IconData: das weist eframe an, das
    // Programmsymbol gar nicht anzufassen, sodass `CFBundleIconFile` aus dem
    // Bundle gilt. Nur so legt macOS den üblichen Rand um das Icon — setzt man
    // es programmatisch, zeichnet das Dock es randlos und damit größer als
    // jedes andere Symbol daneben.
    #[cfg(target_os = "macos")]
    {
        viewport = viewport.with_icon(egui::IconData::default());
    }

    #[cfg(not(target_os = "macos"))]
    match eframe::icon_data::from_png_bytes(APP_ICON) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(e) => eprintln!("App-Icon konnte nicht gelesen werden: {e}"),
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "FileRune",
        options,
        Box::new(|cc| Ok(Box::new(ui::FileRuneApp::new(cc)))),
    )
}
