//! Farben. Golgari aus der Zed-Theme-Familie „Mana Guilds"
//! (`~/.config/zed/themes/mana-guilds.json`), damit FileRune und der Editor
//! daneben dieselbe Sprache sprechen.
//!
//! In Zed ist Golgari ein Zweiklang: Grün für Funktionen und Titel, Lila für
//! Strings und den Cursor, beides auf neutralem Grund mit mauve-grauem Rahmen.
//! Hier führt das Grün — Auswahl, aktive Knöpfe, Sortierung —, das Lila bleibt
//! für Fundstellen und die Schreibmarke, wie Strings und Cursor in Zed.

use egui::Color32;

pub struct Theme {
    /// Hauptakzent: Auswahl, aktive Knöpfe, Sortierspalte. Zed: `syntax.function`.
    pub accent: Color32,
    /// Dunkles Grün für Schrift auf grüner Tönung. Zed: `syntax.number`.
    pub accent_deep: Color32,
    /// Zweitakzent: Fundstellen der Inhaltssuche, Schreibmarke. Zed: `syntax.string`.
    pub secondary: Color32,

    /// Worauf die Liste steht. Zed: `editor.background` — neutral, wie in allen
    /// zehn Gilden, weil Tönung hinter Schrift stört.
    pub canvas: Color32,
    /// Eingabefeld, eine Spur heller als die Leiste darum.
    pub field: Color32,
    /// Filterleiste. Zed: `toolbar.background`.
    pub toolbar: Color32,
    /// Seitenleiste und Pfadleiste. Zed: `surface.background`.
    pub surface: Color32,
    /// Statusleiste. Zed: `status_bar.background`.
    pub bar: Color32,
    /// Zed: `element.hover`.
    pub hover: Color32,

    pub border: Color32,
    pub border_soft: Color32,

    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    /// Fehler in der Statuszeile. Bewusst NICHT aus dem Theme: Golgari färbt
    /// `error` dort lila, und ein Fehler, der nicht nach Fehler aussieht, wird
    /// übersehen.
    pub error: Color32,
    /// Bestätigungen in der Statuszeile. Das tiefe Grün, nicht der Akzent: die
    /// Statusleiste ist dunkler als die Liste, und dort schafft `#00773a` nur
    /// 4,15:1 statt der nötigen 4,5:1.
    pub success: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const GOLGARI: Theme = Theme {
    accent: rgb(0x00773a),
    accent_deep: rgb(0x00441f),
    secondary: rgb(0x8e1da5),

    canvas: rgb(0xfafafa),
    field: rgb(0xffffff),
    toolbar: rgb(0xfdf8fe),
    surface: rgb(0xeeeaef),
    bar: rgb(0xdfdbe0),
    hover: rgb(0xe4dce6),

    border: rgb(0xccc7cd),
    border_soft: rgb(0xe2dde3),

    text: rgb(0x192025),
    text_muted: rgb(0x47545d),
    text_faint: rgb(0x788c99),
    error: rgb(0xb3261e),
    success: rgb(0x00441f),
};

/// Überträgt das Theme auf egui — Hintergründe, Schrift, Rahmen, Auswahl und
/// Schreibmarke. Die Flächen der einzelnen Leisten setzt ui.rs je Panel.
pub fn apply(ctx: &egui::Context, t: &Theme) {
    ctx.all_styles_mut(|style| {
        let v = &mut style.visuals;
        v.dark_mode = false;
        v.override_text_color = Some(t.text);
        v.weak_text_color = Some(t.text_faint);
        v.panel_fill = t.canvas;
        v.window_fill = t.canvas;
        v.window_stroke = egui::Stroke::new(1.0, t.border);
        v.extreme_bg_color = t.field;
        v.text_edit_bg_color = Some(t.field);
        v.faint_bg_color = t.surface;
        v.code_bg_color = t.surface;
        v.hyperlink_color = t.accent;
        v.error_fg_color = t.error;
        v.warn_fg_color = t.secondary;

        v.selection.bg_fill = with_alpha(t.accent, 0.28);
        v.selection.stroke = egui::Stroke::new(1.0, t.accent_deep);
        // Die Schreibmarke ist in Zed lila — hier auch.
        v.text_cursor.stroke = egui::Stroke::new(2.0, t.secondary);

        let w = &mut v.widgets;
        w.noninteractive.bg_stroke = egui::Stroke::new(1.0, t.border_soft);
        w.noninteractive.fg_stroke = egui::Stroke::new(1.0, t.text);
        w.noninteractive.bg_fill = t.canvas;
        w.inactive.bg_fill = t.surface;
        w.inactive.weak_bg_fill = t.surface;
        w.inactive.bg_stroke = egui::Stroke::new(1.0, t.border);
        w.inactive.fg_stroke = egui::Stroke::new(1.0, t.text);
        w.hovered.bg_fill = t.hover;
        w.hovered.weak_bg_fill = t.hover;
        w.hovered.bg_stroke = egui::Stroke::new(1.0, t.border);
        w.hovered.fg_stroke = egui::Stroke::new(1.0, t.text);
        w.active.bg_fill = with_alpha(t.accent, 0.28);
        w.active.weak_bg_fill = with_alpha(t.accent, 0.28);
        w.active.bg_stroke = egui::Stroke::new(1.0, t.accent);
        w.active.fg_stroke = egui::Stroke::new(1.0, t.accent_deep);
    });
}

pub fn with_alpha(c: Color32, opacity: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (opacity * 255.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG-Kontrast zweier Farben.
    fn contrast(a: Color32, b: Color32) -> f32 {
        fn lum(c: Color32) -> f32 {
            let ch = |v: u8| {
                let s = v as f32 / 255.0;
                if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
        }
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// Jede farbige Schrift muss dort lesbar sein, wo sie tatsächlich steht.
    /// Geprüft werden genau die Paare, die in ui.rs vorkommen — die erste
    /// Fassung prüfte pauschal alles gegen alles und fand so, dass das Grün auf
    /// der dunkleren Statusleiste nur 4,15:1 hatte.
    #[test]
    fn colored_text_is_readable_where_it_is_used() {
        let t = &GOLGARI;
        let pairs = [
            ("Sortierspalte (Grün) auf der Liste", t.accent, t.canvas),
            ("aktueller Ordner (Grün) in der Seitenleiste", t.accent, t.surface),
            ("Fundstelle (Lila) auf der Liste", t.secondary, t.canvas),
            ("Bestätigung (tiefes Grün) in der Statuszeile", t.success, t.bar),
            ("Fehler in der Statuszeile", t.error, t.bar),
            ("gedämpfte Schrift in der Seitenleiste", t.text_muted, t.surface),
            ("gedämpfte Schrift in der Statuszeile", t.text_muted, t.bar),
        ];
        for (what, fg, bg) in pairs {
            let c = contrast(fg, bg);
            assert!(c >= 4.5, "{what}: {c:.2}:1 < 4,5:1");
        }
        assert!(contrast(t.text, t.canvas) >= 7.0, "Grundschrift");
    }
}
