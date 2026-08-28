//! Icons und Schriften für Windows und Linux. Die Symbole werden gezeichnet,
//! nicht geladen: kein Icon-Theme, keine mitgelieferten Bilddateien, keine
//! Lizenzfrage — und bei jeder Bildschirmauflösung scharf. Die Formen folgen
//! den SF Symbols der macOS-Fassung, damit beide Fassungen gleich zu lesen sind.

use egui::{Color32, Context, Pos2, Rect, Stroke, Ui, Vec2};

/// Der Cache ist hier leer: es gibt nichts zu rastern.
pub struct IconCache;

impl IconCache {
    pub fn new() -> Self {
        IconCache
    }

    /// Ordner blau gefüllt, Datei als Blatt mit umgeknickter Ecke — die beiden
    /// Fälle, die Windows und Linux ohne Systemabfrage sicher hergeben.
    pub fn draw_file_icon(&mut self, ui: &Ui, path: &std::path::Path, is_dir: bool, rect: Rect) {
        let p = ui.painter();
        if is_dir {
            let blue = Color32::from_rgb(88, 160, 236);
            let r = rect.shrink2(Vec2::new(rect.width() * 0.06, rect.height() * 0.16));
            // Reiter
            let tab = Rect::from_min_size(
                r.min,
                Vec2::new(r.width() * 0.45, r.height() * 0.22),
            );
            p.rect_filled(tab, 1.5, blue);
            let body = Rect::from_min_max(
                Pos2::new(r.left(), r.top() + r.height() * 0.14),
                r.max,
            );
            p.rect_filled(body, 2.0, blue);
        } else {
            let fill = Color32::from_rgb(236, 238, 242);
            let line = Color32::from_rgb(150, 156, 168);
            let r = rect.shrink2(Vec2::new(rect.width() * 0.16, rect.height() * 0.06));
            let fold = r.width() * 0.34;
            // Blatt mit abgeschnittener Ecke oben rechts
            let outline = vec![
                Pos2::new(r.left(), r.top()),
                Pos2::new(r.right() - fold, r.top()),
                Pos2::new(r.right(), r.top() + fold),
                Pos2::new(r.right(), r.bottom()),
                Pos2::new(r.left(), r.bottom()),
            ];
            p.add(egui::Shape::convex_polygon(
                outline,
                fill,
                Stroke::new(1.0, line),
            ));
            // Der umgeknickte Zipfel
            p.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(r.right() - fold, r.top()),
                    Pos2::new(r.right(), r.top() + fold),
                    Pos2::new(r.right() - fold, r.top() + fold),
                ],
                line.gamma_multiply(0.45),
                Stroke::new(1.0, line),
            ));
            // Ein farbiger Streifen nach Dateiendung, damit sich Typen
            // unterscheiden lassen wie bei den echten Finder-Icons.
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let tint = ext_color(&ext.to_lowercase());
                let bar = Rect::from_min_max(
                    Pos2::new(r.left() + 1.0, r.bottom() - r.height() * 0.30),
                    Pos2::new(r.right() - 1.0, r.bottom() - 1.0),
                );
                p.rect_filled(bar, 1.0, tint);
            }
        }
    }

    /// Zeichnet dasselbe Symbol wie das gleichnamige SF Symbol auf macOS.
    pub fn draw_symbol(&mut self, ui: &Ui, name: &str, rect: Rect, color: Color32) {
        let p = ui.painter();
        // Strichstärke mitskalieren, damit kleine Symbole nicht zulaufen.
        let w = (rect.width() * 0.11).clamp(1.0, 2.0);
        let stroke = Stroke::new(w, color);
        let c = rect.center();
        let r = rect.shrink(rect.width() * 0.14);

        match name {
            "sidebar.left" => {
                p.rect_stroke(r, 2.0, stroke, egui::StrokeKind::Inside);
                let x = r.left() + r.width() * 0.36;
                p.line_segment([Pos2::new(x, r.top()), Pos2::new(x, r.bottom())], stroke);
            }
            "chevron.left" => chevron(p, c, r.width() * 0.28, stroke, Dir::Left),
            "chevron.right" => chevron(p, c, r.width() * 0.28, stroke, Dir::Right),
            "chevron.up" => chevron(p, c, r.width() * 0.30, stroke, Dir::Up),
            "chevron.down" => chevron(p, c, r.width() * 0.30, stroke, Dir::Down),
            "arrow.up" => {
                p.line_segment([Pos2::new(c.x, r.bottom()), Pos2::new(c.x, r.top())], stroke);
                let h = r.width() * 0.28;
                p.line_segment([Pos2::new(c.x - h, r.top() + h), Pos2::new(c.x, r.top())], stroke);
                p.line_segment([Pos2::new(c.x + h, r.top() + h), Pos2::new(c.x, r.top())], stroke);
            }
            "house" => {
                let eaves = r.top() + r.height() * 0.42;
                p.line_segment([Pos2::new(r.left(), eaves), Pos2::new(c.x, r.top())], stroke);
                p.line_segment([Pos2::new(c.x, r.top()), Pos2::new(r.right(), eaves)], stroke);
                p.line_segment([Pos2::new(r.left() + w, eaves), Pos2::new(r.left() + w, r.bottom())], stroke);
                p.line_segment([Pos2::new(r.right() - w, eaves), Pos2::new(r.right() - w, r.bottom())], stroke);
                p.line_segment([Pos2::new(r.left() + w, r.bottom()), Pos2::new(r.right() - w, r.bottom())], stroke);
            }
            "arrow.clockwise" => {
                // Offener Kreis mit Pfeilspitze — als Polygonzug, egui hat keine Bögen.
                let radius = r.width() * 0.40;
                let pts: Vec<Pos2> = (0..=26)
                    .map(|i| {
                        let a = -1.2 + (i as f32 / 26.0) * 5.4;
                        Pos2::new(c.x + radius * a.cos(), c.y + radius * a.sin())
                    })
                    .collect();
                p.add(egui::Shape::line(pts, stroke));
                let tip = Pos2::new(c.x + radius * (-1.2f32).cos(), c.y + radius * (-1.2f32).sin());
                let h = r.width() * 0.22;
                p.line_segment([tip, Pos2::new(tip.x - h, tip.y - h * 0.2)], stroke);
                p.line_segment([tip, Pos2::new(tip.x + h * 0.2, tip.y + h)], stroke);
            }
            "doc.on.doc" => {
                let a = Rect::from_min_size(
                    r.min,
                    Vec2::new(r.width() * 0.66, r.height() * 0.66),
                );
                let b = Rect::from_min_size(
                    Pos2::new(r.left() + r.width() * 0.34, r.top() + r.height() * 0.34),
                    Vec2::new(r.width() * 0.66, r.height() * 0.66),
                );
                p.rect_stroke(a, 1.5, stroke, egui::StrokeKind::Inside);
                p.rect_stroke(b, 1.5, stroke, egui::StrokeKind::Inside);
            }
            "chevron.left.forwardslash.chevron.right" => {
                let d = r.width() * 0.20;
                let m = r.width() * 0.30;
                // < Schrägstrich >
                p.line_segment([Pos2::new(c.x - m, c.y - d), Pos2::new(c.x - m - d, c.y)], stroke);
                p.line_segment([Pos2::new(c.x - m - d, c.y), Pos2::new(c.x - m, c.y + d)], stroke);
                p.line_segment([Pos2::new(c.x + d * 0.4, r.top()), Pos2::new(c.x - d * 0.4, r.bottom())], stroke);
                p.line_segment([Pos2::new(c.x + m, c.y - d), Pos2::new(c.x + m + d, c.y)], stroke);
                p.line_segment([Pos2::new(c.x + m + d, c.y), Pos2::new(c.x + m, c.y + d)], stroke);
            }
            "eye.fill" | "eye.slash" => {
                let rx = r.width() * 0.46;
                let ry = r.height() * 0.28;
                // Augenlider als zwei gespiegelte Polygonzüge
                for sign in [-1.0f32, 1.0] {
                    let pts: Vec<Pos2> = (0..=16)
                        .map(|i| {
                            let t = -1.0 + (i as f32 / 16.0) * 2.0;
                            Pos2::new(c.x + t * rx, c.y + sign * ry * (1.0 - t * t))
                        })
                        .collect();
                    p.add(egui::Shape::line(pts, stroke));
                }
                if name == "eye.fill" {
                    p.circle_filled(c, r.width() * 0.15, color);
                } else {
                    p.circle_stroke(c, r.width() * 0.15, stroke);
                    p.line_segment(
                        [Pos2::new(r.left(), r.bottom()), Pos2::new(r.right(), r.top())],
                        stroke,
                    );
                }
            }
            "magnifyingglass" => {
                let radius = r.width() * 0.32;
                let centre = Pos2::new(c.x - r.width() * 0.08, c.y - r.height() * 0.08);
                p.circle_stroke(centre, radius, stroke);
                let off = radius * 0.71;
                p.line_segment(
                    [
                        Pos2::new(centre.x + off, centre.y + off),
                        Pos2::new(r.right(), r.bottom()),
                    ],
                    stroke,
                );
            }
            "xmark.circle.fill" => {
                p.circle_filled(c, r.width() * 0.5, color);
                let d = r.width() * 0.19;
                let cut = Stroke::new(w, ui.visuals().extreme_bg_color);
                p.line_segment([Pos2::new(c.x - d, c.y - d), Pos2::new(c.x + d, c.y + d)], cut);
                p.line_segment([Pos2::new(c.x + d, c.y - d), Pos2::new(c.x - d, c.y + d)], cut);
            }
            "checkmark.circle.fill" => {
                p.circle_filled(c, r.width() * 0.5, color);
                let tick = Stroke::new(w, ui.visuals().extreme_bg_color);
                p.line_segment(
                    [
                        Pos2::new(c.x - r.width() * 0.22, c.y),
                        Pos2::new(c.x - r.width() * 0.05, c.y + r.height() * 0.18),
                    ],
                    tick,
                );
                p.line_segment(
                    [
                        Pos2::new(c.x - r.width() * 0.05, c.y + r.height() * 0.18),
                        Pos2::new(c.x + r.width() * 0.24, c.y - r.height() * 0.18),
                    ],
                    tick,
                );
            }
            // Unbekanntes Symbol: kleiner Kreis statt stillem Nichts, damit ein
            // vergessener Name beim Ausprobieren auffällt.
            _ => {
                p.circle_stroke(c, r.width() * 0.35, stroke);
            }
        }
    }
}

enum Dir {
    Left,
    Right,
    Up,
    Down,
}

fn chevron(p: &egui::Painter, c: Pos2, size: f32, stroke: Stroke, dir: Dir) {
    let (a, b) = match dir {
        Dir::Left => (
            [Pos2::new(c.x + size * 0.5, c.y - size), Pos2::new(c.x - size * 0.5, c.y)],
            [Pos2::new(c.x - size * 0.5, c.y), Pos2::new(c.x + size * 0.5, c.y + size)],
        ),
        Dir::Right => (
            [Pos2::new(c.x - size * 0.5, c.y - size), Pos2::new(c.x + size * 0.5, c.y)],
            [Pos2::new(c.x + size * 0.5, c.y), Pos2::new(c.x - size * 0.5, c.y + size)],
        ),
        Dir::Up => (
            [Pos2::new(c.x - size, c.y + size * 0.5), Pos2::new(c.x, c.y - size * 0.5)],
            [Pos2::new(c.x, c.y - size * 0.5), Pos2::new(c.x + size, c.y + size * 0.5)],
        ),
        Dir::Down => (
            [Pos2::new(c.x - size, c.y - size * 0.5), Pos2::new(c.x, c.y + size * 0.5)],
            [Pos2::new(c.x, c.y + size * 0.5), Pos2::new(c.x + size, c.y - size * 0.5)],
        ),
    };
    p.line_segment(a, stroke);
    p.line_segment(b, stroke);
}

/// Grobe Einfärbung nach Dateiendung — dieselbe Idee wie die farbigen
/// Finder-Icons, nur ohne System-Datenbank.
fn ext_color(ext: &str) -> Color32 {
    match ext {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "svg" | "tiff" | "bmp" => {
            Color32::from_rgb(126, 187, 122)
        }
        "pdf" => Color32::from_rgb(214, 94, 84),
        "zip" | "gz" | "tar" | "7z" | "rar" | "xz" | "zst" => Color32::from_rgb(196, 160, 92),
        "mp4" | "mov" | "mkv" | "avi" | "webm" => Color32::from_rgb(150, 122, 200),
        "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" => Color32::from_rgb(212, 140, 190),
        "rs" | "py" | "js" | "ts" | "tsx" | "go" | "c" | "h" | "cpp" | "swift" | "java" | "rb"
        | "sh" | "abap" | "ex" | "exs" => Color32::from_rgb(96, 150, 214),
        "json" | "toml" | "yaml" | "yml" | "xml" | "csv" | "ini" | "cfg" => {
            Color32::from_rgb(140, 150, 166)
        }
        "md" | "txt" | "rst" | "log" => Color32::from_rgb(160, 168, 180),
        _ => Color32::from_rgb(176, 182, 194),
    }
}

/// Systemschrift laden, wenn eine der bekannten Stellen etwas hergibt; sonst
/// bleibt es bei den Schriften, die egui selbst mitbringt.
pub fn install_fonts(ctx: &Context) {
    use std::sync::Arc;

    const CANDIDATES: &[(&str, &str)] = &[
        // Windows
        ("segoe", r"C:\Windows\Fonts\segoeui.ttf"),
        ("consola", r"C:\Windows\Fonts\consola.ttf"),
        // Linux, verbreitete Pfade
        ("dejavu", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        ("dejavu-mono", "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
        ("noto", "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf"),
        ("noto2", "/usr/share/fonts/noto/NotoSans-Regular.ttf"),
        ("liberation", "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf"),
        ("liberation-mono", "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf"),
    ];

    let mut fonts = egui::FontDefinitions::default();
    let mut any = false;
    for (name, path) in CANDIDATES {
        let Ok(bytes) = std::fs::read(path) else { continue };
        fonts
            .font_data
            .insert((*name).to_string(), Arc::new(egui::FontData::from_owned(bytes)));
        let family = if name.contains("mono") || *name == "consola" {
            egui::FontFamily::Monospace
        } else {
            egui::FontFamily::Proportional
        };
        fonts.families.entry(family).or_default().insert(0, (*name).to_string());
        any = true;
    }
    if any {
        ctx.set_fonts(fonts);
    }
}

/// Ohne Systemquelle das Blau, das auch macOS voreingestellt hat — so sehen
/// beide Fassungen gleich aus, solange der Nutzer nichts anderes eingestellt hat.
pub fn system_accent() -> Color32 {
    Color32::from_rgb(0, 122, 255)
}
