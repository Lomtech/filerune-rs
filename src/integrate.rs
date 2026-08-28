//! Fremde Apps öffnen und die Zwischenablage — der Teil, der pro System
//! abweicht. macOS geht über `open`, Windows über `explorer`/`start`, Linux
//! über `xdg-open` und die üblichen Terminal-Namen.

use std::path::Path;
use std::process::Command;

/// Name des Dateimanagers für Hilfetexte und Fehlermeldungen.
#[cfg(target_os = "macos")]
pub const FILE_MANAGER: &str = "Finder";
#[cfg(target_os = "windows")]
pub const FILE_MANAGER: &str = "Explorer";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const FILE_MANAGER: &str = "Dateimanager";

// MARK: - Zed

/// Öffnet den Pfad als Arbeitsbereich in Zed.
pub fn open_in_zed(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // Bundle-IDs in Vorzugsreihenfolge — stabil, dann Preview/Nightly/Dev.
        const IDS: [&str; 4] = [
            "dev.zed.Zed",
            "dev.zed.Zed-Preview",
            "dev.zed.Zed-Nightly",
            "dev.zed.Zed-Dev",
        ];
        for id in IDS {
            if run(Command::new("open").args(["-b", id]).arg(path)).is_ok() {
                return Ok(());
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Erst der Name im PATH, dann die Stellen, an denen die Installer landen.
        // `zeditor` ist der Name, den einige Linux-Distributionen vergeben.
        for exe in ["zed", "zeditor"] {
            if run(Command::new(exe).arg(path)).is_ok() {
                return Ok(());
            }
        }
        for candidate in zed_install_paths() {
            if candidate.exists() && run(Command::new(&candidate).arg(path)).is_ok() {
                return Ok(());
            }
        }
        #[cfg(target_os = "linux")]
        if run(Command::new("flatpak").args(["run", "dev.zed.Zed"]).arg(path)).is_ok() {
            return Ok(());
        }
    }

    Err("Zed wurde nicht gefunden.".into())
}

#[cfg(target_os = "windows")]
fn zed_install_paths() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        out.push(Path::new(&local).join(r"Programs\Zed\Zed.exe"));
    }
    if let Some(files) = std::env::var_os("ProgramFiles") {
        out.push(Path::new(&files).join(r"Zed\Zed.exe"));
    }
    out
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn zed_install_paths() -> Vec<std::path::PathBuf> {
    ["/usr/bin/zed", "/usr/local/bin/zed", "/snap/bin/zed"]
        .iter()
        .map(std::path::PathBuf::from)
        .collect()
}

// MARK: - Terminal

pub fn open_in_terminal(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return run(Command::new("open").args(["-b", "com.apple.Terminal"]).arg(path))
            .map_err(|_| "Terminal wurde nicht gefunden.".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        // Windows Terminal, wenn vorhanden; sonst die klassische Eingabeaufforderung.
        if run(Command::new("wt.exe").arg("-d").arg(path)).is_ok() {
            return Ok(());
        }
        return run(Command::new("cmd").args(["/c", "start", "cmd"]).current_dir(path))
            .map_err(|_| "Terminal wurde nicht gefunden.".to_string());
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        // x-terminal-emulator ist die Debian-Abstraktion; der Rest sind die
        // Namen der verbreiteten Desktop-Umgebungen.
        const TERMINALS: [&str; 7] = [
            "x-terminal-emulator",
            "gnome-terminal",
            "konsole",
            "xfce4-terminal",
            "alacritty",
            "kitty",
            "xterm",
        ];
        for term in TERMINALS {
            if run(Command::new(term).current_dir(path)).is_ok() {
                return Ok(());
            }
        }
        Err("Kein Terminal gefunden.".into())
    }
}

// MARK: - Dateimanager

/// Zeigt die Einträge im Dateimanager und markiert sie dort, wo das geht.
pub fn reveal_in_file_manager(paths: &[&Path]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    reveal(paths).map_err(|e| format!("{FILE_MANAGER} konnte nicht geöffnet werden: {e}"))
}

#[cfg(target_os = "macos")]
fn reveal(paths: &[&Path]) -> Result<(), String> {
    let mut cmd = Command::new("open");
    cmd.arg("-R");
    for p in paths {
        cmd.arg(p);
    }
    run(&mut cmd)
}

#[cfg(target_os = "windows")]
fn reveal(paths: &[&Path]) -> Result<(), String> {
    // `/select,<pfad>` muss ein einziges Argument sein, sonst markiert der
    // Explorer nichts und öffnet stattdessen den Ordner.
    let arg = format!("/select,{}", paths[0].display());
    run(Command::new("explorer").arg(arg))
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn reveal(paths: &[&Path]) -> Result<(), String> {
    // Der Standardweg markiert keine Datei, also den Elternordner öffnen.
    let target = paths[0].parent().unwrap_or(paths[0]);
    run(Command::new("xdg-open").arg(target))
}

// MARK: - Standard-App

/// Öffnet die Datei in ihrer Standard-App.
pub fn open_default(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg(path);
        c
    };

    #[cfg(target_os = "windows")]
    let mut cmd = {
        // Das leere Argument ist der Fenstertitel — ohne das deutet `start`
        // einen Pfad in Anführungszeichen als Titel und öffnet nichts.
        let mut c = Command::new("cmd");
        c.args(["/c", "start", ""]).arg(path);
        c
    };

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut cmd = {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };

    run(&mut cmd).map_err(|e| format!("Öffnen fehlgeschlagen: {e}"))
}

fn run(cmd: &mut Command) -> Result<(), String> {
    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("Exit-Code {}", s.code().unwrap_or(-1))),
        Err(e) => Err(e.to_string()),
    }
}

/// Text in die System-Zwischenablage legen.
pub fn copy_text(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        .map_err(|e| format!("Zwischenablage: {e}"))
}
