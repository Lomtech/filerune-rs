//! Die Oberfläche. Aufbau, Metrik und Beschriftungen wie ContentView.swift:
//! Seitenleiste | Pfadleiste · Filterleiste · Spaltenkopf · Liste · Statuszeile.
//! Icons sind echte SF Symbols und Finder-Dateisymbole (siehe icons.rs).

use crate::entry::{format_date, format_size, Entry};
use crate::platform::{sc, IconCache, ICON_POINTS};
use crate::state::{AppState, SortKey};
use egui::{Align, Color32, Key, Layout, Modifiers, Rect, RichText, Sense, Ui, Vec2};
use std::path::PathBuf;

/// Spaltenbreiten — dieselben Werte wie FileColumns in der SwiftUI-Fassung.
const COL_DATE: f32 = 132.0;
const COL_SIZE: f32 = 76.0;
const COL_KIND: f32 = 84.0;
const SIDEBAR_WIDTH: f32 = 192.0;
/// Zeilenhöhe: 16 pt Icon + 3 pt Polsterung oben und unten, wie EntryRow.
const ROW_HEIGHT: f32 = 22.0;
/// Höher, sobald Suchtreffer ihren Ordner als zweite Zeile zeigen.
const ROW_HEIGHT_WITH_SUBTITLE: f32 = 34.0;
/// Sprungweite von Bild auf/ab — wie moveSelection(by: ±15).
const PAGE_JUMP: i64 = 15;
/// ToolButton: 30 × 26 mit Radius 7.
const TOOL_SIZE: Vec2 = Vec2::new(30.0, 26.0);
const TOOL_RADIUS: f32 = 7.0;
const TOOL_GLYPH: f32 = 15.0;

/// Welches modale Blatt gerade offen ist — nur eines zur Zeit, wie die
/// `.sheet`-Modifier der SwiftUI-Fassung.
#[derive(PartialEq)]
pub enum Sheet {
    None,
    Rename { path: PathBuf, text: String },
    GoTo { text: String },
    NewFolder { text: String },
}

pub struct FileRuneApp {
    pub state: AppState,
    pub sheet: Sheet,
    icons: IconCache,
    accent: Color32,
    filter_text: String,
    /// Zeile, zu der die Liste beim nächsten Aufbau scrollen soll.
    scroll_to: Option<usize>,
    focus_filter: bool,
    /// Einmalige Prüfung nach dem Start, ob das Fenster im Bild liegt.
    checked_on_screen: bool,
}

impl FileRuneApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::platform::install_fonts(&cc.egui_ctx);
        let accent = crate::platform::system_accent();
        install_style(&cc.egui_ctx, accent);
        FileRuneApp {
            state: AppState::new(),
            sheet: Sheet::None,
            icons: IconCache::new(),
            accent,
            filter_text: String::new(),
            scroll_to: None,
            focus_filter: true,
            checked_on_screen: false,
        }
    }

    /// Akzentfarbe mit Deckkraft — ersetzt `Color.accentColor.opacity(…)`.
    fn accent_alpha(&self, opacity: f32) -> Color32 {
        Color32::from_rgba_unmultiplied(
            self.accent.r(),
            self.accent.g(),
            self.accent.b(),
            (opacity * 255.0).round() as u8,
        )
    }
}

impl eframe::App for FileRuneApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Fenstergröße und -position mitschreiben, damit beides beim nächsten
        // Start wieder so ist.
        // Eine wiederhergestellte Position kann ins Leere zeigen, wenn seither
        // ein Bildschirm abgesteckt oder die Anordnung geändert wurde. Dann
        // stünde die App unsichtbar irgendwo — der schlimmste Fehler, den ein
        // Fenster haben kann. Also einmal nach dem Start zurückholen.
        if !self.checked_on_screen {
            self.checked_on_screen = true;
            let rescue = ctx.input(|i| {
                let vp = i.viewport();
                let (Some(outer), Some(monitor)) = (vp.outer_rect, vp.monitor_size) else {
                    return None;
                };
                // Mindestens 80 × 80 Punkte müssen sichtbar bleiben.
                let visible = outer.left() < monitor.x - 80.0
                    && outer.top() < monitor.y - 80.0
                    && outer.right() > 80.0
                    && outer.bottom() > 80.0;
                (!visible).then_some(egui::pos2(80.0, 80.0))
            });
            if let Some(pos) = rescue {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
            }
        }

        let (size, pos, special) = ctx.input(|i| {
            let vp = i.viewport();
            (
                vp.inner_rect.map(|r| [r.width(), r.height()]),
                vp.outer_rect.map(|r| [r.min.x, r.min.y]),
                vp.fullscreen.unwrap_or(false)
                    || vp.maximized.unwrap_or(false)
                    || vp.minimized.unwrap_or(false),
            )
        });
        self.state.note_window_geometry(size, pos, special);

        if self.state.poll_search() {
            self.scroll_to = Some(self.state.selected_index);
        }
        if self.state.is_searching || self.state.has_pending_flash() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }

        if self.sheet == Sheet::None {
            self.handle_keys(&ctx);
        } else {
            self.handle_sheet_keys(&ctx);
        }

        // Reihenfolge zählt: zuerst hinzugefügte Panels liegen außen, das
        // CentralPanel muss zuletzt kommen.
        if self.state.show_sidebar {
            egui::Panel::left("sidebar")
                .exact_size(SIDEBAR_WIDTH)
                .resizable(false)
                .show(ui, |ui| self.sidebar(ui));
        }

        egui::Panel::top("pathbar").show(ui, |ui| self.path_bar(ui));
        egui::Panel::top("filterbar").show(ui, |ui| self.filter_bar(ui));
        egui::Panel::top("listheader").show(ui, |ui| self.list_header(ui));
        egui::Panel::bottom("statusbar").show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default().show(ui, |ui| self.list(ui));

        self.show_sheet(&ctx);

    }
}

// MARK: - Tastatur

impl FileRuneApp {
    /// Alle Navigationstasten werden VOR dem Textfeld abgefangen. Das ist die
    /// Signatur der App: das Filterfeld hat immer den Fokus, die Pfeiltasten
    /// steuern trotzdem die Liste statt der Schreibmarke — genau wie das
    /// `doCommandBySelector`-Routing in FilterField.swift.
    fn handle_keys(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        let shift = Modifiers::SHIFT;
        let cmd_shift = Modifiers::COMMAND.plus(Modifiers::SHIFT);
        let cmd_alt = Modifiers::COMMAND.plus(Modifiers::ALT);

        // Befehle mit Zusatztaste zuerst: sie sind spezifischer als die blanken
        // Pfeiltasten und müssen die Ereignisse vorher greifen.
        if consume(ctx, cmd, Key::ArrowLeft) {
            self.state.go_back();
            self.scroll_to = Some(0);
        }
        if consume(ctx, cmd, Key::ArrowRight) {
            self.state.go_forward();
            self.scroll_to = Some(0);
        }
        if consume(ctx, cmd, Key::ArrowUp) {
            self.state.go_up();
            self.scroll_to = Some(0);
        }
        if consume(ctx, cmd_shift, Key::H) {
            self.state.go_home();
            self.scroll_to = Some(0);
        }
        if consume(ctx, cmd_shift, Key::G) {
            self.sheet = Sheet::GoTo { text: String::new() };
        }
        if consume(ctx, cmd_shift, Key::N) {
            self.sheet = Sheet::NewFolder { text: String::new() };
        }
        if consume(ctx, cmd_shift, Key::D) {
            self.state.add_favorite(self.state.current_dir.clone());
        }
        if consume(ctx, cmd_shift, Key::R) {
            self.state.reveal_in_finder();
        }
        if consume(ctx, cmd_shift, Key::T) {
            self.state.open_in_terminal();
        }
        if consume(ctx, cmd_shift, Key::E) {
            self.state.open_in_zed();
        }
        if consume(ctx, cmd_shift, Key::Period) {
            self.state.toggle_hidden();
        }
        if consume(ctx, cmd_alt, Key::C) {
            self.state.copy_path();
        }
        if consume(ctx, cmd_alt, Key::S) {
            self.state.toggle_sort_direction();
        }
        if consume(ctx, cmd_alt, Key::Num1) {
            self.state.sort_by(SortKey::Name);
        }
        if consume(ctx, cmd_alt, Key::Num2) {
            self.state.sort_by(SortKey::Date);
        }
        if consume(ctx, cmd_alt, Key::Num3) {
            self.state.sort_by(SortKey::Size);
        }
        if consume(ctx, Modifiers::COMMAND.plus(Modifiers::CTRL), Key::S) {
            self.state.toggle_sidebar();
        }
        if consume(ctx, cmd, Key::R) {
            self.state.load();
        }
        if consume(ctx, cmd, Key::D) {
            self.state.duplicate_selection();
            self.scroll_to = Some(self.state.selected_index);
        }
        if consume(ctx, cmd, Key::Backspace) || consume(ctx, cmd, Key::Delete) {
            self.state.move_selection_to_trash();
        }
        if consume(ctx, cmd, Key::Enter) {
            if let Some(e) = self.state.selected_entry() {
                self.sheet = Sheet::Rename {
                    path: e.path.clone(),
                    text: e.name.clone(),
                };
            }
        }
        if consume(ctx, cmd, Key::C) {
            self.state.copy_selection();
        }
        if consume(ctx, cmd, Key::V) {
            self.state.paste();
            self.scroll_to = Some(self.state.selected_index);
        }
        if consume(ctx, cmd, Key::O) {
            self.state.activate_selection();
        }
        // ⌘A markiert alle Dateien, solange das Filterfeld leer ist; steht dort
        // Text, gehört die Taste der normalen Textauswahl.
        if self.filter_text.is_empty() && consume(ctx, cmd, Key::A) {
            self.state.select_all();
        }

        // Listennavigation ohne Zusatztaste.
        if consume(ctx, shift, Key::ArrowUp) {
            self.state.extend_selection(-1);
            self.scroll_to = Some(self.state.selected_index);
        }
        if consume(ctx, shift, Key::ArrowDown) {
            self.state.extend_selection(1);
            self.scroll_to = Some(self.state.selected_index);
        }
        if consume(ctx, Modifiers::NONE, Key::ArrowUp) {
            self.state.move_selection(-1);
            self.scroll_to = Some(self.state.selected_index);
        }
        if consume(ctx, Modifiers::NONE, Key::ArrowDown) {
            self.state.move_selection(1);
            self.scroll_to = Some(self.state.selected_index);
        }
        if consume(ctx, Modifiers::NONE, Key::PageUp) {
            self.state.move_selection(-PAGE_JUMP);
            self.scroll_to = Some(self.state.selected_index);
        }
        if consume(ctx, Modifiers::NONE, Key::PageDown) {
            self.state.move_selection(PAGE_JUMP);
            self.scroll_to = Some(self.state.selected_index);
        }
        if consume(ctx, Modifiers::NONE, Key::Home) {
            self.state.select_edge(true);
            self.scroll_to = Some(0);
        }
        if consume(ctx, Modifiers::NONE, Key::End) {
            self.state.select_edge(false);
            self.scroll_to = Some(self.state.selected_index);
        }
        // ← übergeordneter Ordner, → in den markierten Ordner. Bewusst auch bei
        // Text im Feld: die Schreibmarke bewegt sich in dieser App nie.
        if consume(ctx, Modifiers::NONE, Key::ArrowLeft) {
            self.state.go_up();
            self.scroll_to = Some(0);
        }
        if consume(ctx, Modifiers::NONE, Key::ArrowRight) {
            if let Some(e) = self.state.selected_entry().cloned() {
                if e.is_dir {
                    self.state.go_into(&e);
                    self.scroll_to = Some(0);
                }
            }
        }
        if consume(ctx, Modifiers::NONE, Key::Enter) {
            self.state.activate_selection();
            self.scroll_to = Some(0);
        }
        if consume(ctx, Modifiers::NONE, Key::Escape) && !self.filter_text.is_empty() {
            self.filter_text.clear();
            self.state.set_filter(String::new());
            self.scroll_to = Some(0);
        }

        // Navigation leert den Filter — das Feld muss das mitbekommen.
        self.filter_text = self.state.filter.clone();
    }

    /// Im Blatt gilt nur Abbrechen — alles andere ist Texteingabe.
    fn handle_sheet_keys(&mut self, ctx: &egui::Context) {
        if consume(ctx, Modifiers::NONE, Key::Escape) {
            self.sheet = Sheet::None;
            self.focus_filter = true;
        }
    }
}

/// Nimmt die Taste aus der Ereignisschlange, damit kein Textfeld sie mehr sieht.
fn consume(ctx: &egui::Context, modifiers: Modifiers, key: Key) -> bool {
    ctx.input_mut(|i| i.consume_key(modifiers, key))
}

// MARK: - Leisten

impl FileRuneApp {
    fn path_bar(&mut self, ui: &mut Ui) {
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            ui.add_space(5.0);
            let sidebar_on = self.state.show_sidebar;
            if self.tool(ui, "sidebar.left", &format!("Seitenleiste ein/aus ({})", sc::SIDEBAR), true, sidebar_on) {
                self.state.toggle_sidebar();
            }
            let back = self.state.can_go_back();
            if self.tool(ui, "chevron.left", &format!("Zurück ({})", sc::BACK), back, false) {
                self.state.go_back();
            }
            let fwd = self.state.can_go_forward();
            if self.tool(ui, "chevron.right", &format!("Vorwärts ({})", sc::FORWARD), fwd, false) {
                self.state.go_forward();
            }
            if self.tool(ui, "arrow.up", &format!("Übergeordneter Ordner ({})", sc::UP), true, false) {
                self.state.go_up();
            }
            if self.tool(ui, "house", &format!("Persönlicher Ordner ({})", sc::HOME), true, false) {
                self.state.go_home();
            }
            if self.tool(ui, "arrow.clockwise", &format!("Neu laden ({})", sc::RELOAD), true, false) {
                self.state.load();
            }

            // Rechte Knöpfe zuerst setzen, damit sie bei schmalem Fenster
            // stehen bleiben und stattdessen der Pfad gekürzt wird.
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let hidden = self.state.show_hidden;
                let eye = if hidden { "eye.fill" } else { "eye.slash" };
                if self.tool(ui, eye, &format!("Versteckte Dateien ({})", sc::HIDDEN), true, hidden) {
                    self.state.toggle_hidden();
                }
                if self.tool(
                    ui,
                    "chevron.left.forwardslash.chevron.right",
                    &format!("In Zed öffnen ({})", sc::ZED),
                    true,
                    false,
                ) {
                    self.state.open_in_zed();
                }
                if self.tool(ui, "doc.on.doc", &format!("Pfad kopieren ({})", sc::COPY_PATH), true, false) {
                    self.state.copy_path();
                }
                ui.add_space(4.0);
                let path = self.state.current_dir.to_string_lossy().into_owned();
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(path).monospace().size(13.0))
                            .truncate()
                            .selectable(true),
                    );
                });
            });
        });
        ui.add_space(3.0);
    }

    /// Das Suchfeld. Nachgebaut wie ein NSSearchField: Lupe links, Löschknopf
    /// rechts sobald Text drinsteht, gerahmt und dauerhaft fokussiert.
    fn filter_bar(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);

            let height = 24.0;
            let width = ui.available_width() - 10.0;
            let outer = Rect::from_min_size(
                ui.cursor().min,
                Vec2::new(width, height),
            );

            let magnifier = Rect::from_min_size(
                egui::pos2(outer.left() + 7.0, outer.center().y - 6.5),
                Vec2::splat(13.0),
            );
            let has_text = !self.filter_text.is_empty();
            let clear = Rect::from_min_size(
                egui::pos2(outer.right() - 21.0, outer.center().y - 6.5),
                Vec2::splat(13.0),
            );

            // Rahmen zuerst, damit Feld und Symbole darauf liegen.
            ui.painter().rect(
                outer,
                6.0,
                ui.visuals().extreme_bg_color,
                ui.visuals().widgets.inactive.bg_stroke,
                egui::StrokeKind::Inside,
            );
            self.draw_symbol(ui, "magnifyingglass", magnifier, ui.visuals().weak_text_color());

            let text_rect = Rect::from_min_max(
                egui::pos2(magnifier.right() + 6.0, outer.top()),
                egui::pos2(if has_text { clear.left() - 4.0 } else { outer.right() - 6.0 }, outer.bottom()),
            );
            let field = ui
                .scope_builder(egui::UiBuilder::new().max_rect(text_rect), |ui| {
                    ui.add_sized(
                        text_rect.size(),
                        egui::TextEdit::singleline(&mut self.filter_text)
                            // Der Platzhalter des Originals las sich als
                            // „filtern: art:csv" — Leute tippten das „filtern:"
                            // mit ab. Jetzt steht dort die Schreibweise selbst.
                            .hint_text(
                                "Suchen (auch Unterordner) · .pdf · >1mb · datum:2026-05",
                            )
                            .font(egui::FontId::proportional(13.0))
                            .frame(egui::Frame::NONE)
                            .desired_width(f32::INFINITY),
                    )
                })
                .inner;

            // Das Filterfeld ist die Haupteingabe und hält den Fokus dauerhaft,
            // solange kein Blatt offen ist (FocusSearchField in der Swift-Fassung).
            if self.focus_filter || (!field.has_focus() && self.sheet == Sheet::None) {
                field.request_focus();
                self.focus_filter = false;
            }
            if field.changed() {
                self.state.set_filter(self.filter_text.clone());
                self.scroll_to = Some(0);
            }

            if has_text {
                let response = ui.interact(
                    clear,
                    egui::Id::new("filter-clear"),
                    Sense::click(),
                );
                let color = if response.hovered() {
                    ui.visuals().text_color()
                } else {
                    ui.visuals().weak_text_color()
                };
                self.draw_symbol(ui, "xmark.circle.fill", clear, color);
                if response.on_hover_text("Filter löschen (Esc)").clicked() {
                    self.filter_text.clear();
                    self.state.set_filter(String::new());
                    self.scroll_to = Some(0);
                    self.focus_filter = true;
                }
            }

            ui.advance_cursor_after_rect(outer);
        });
        ui.add_space(4.0);
    }

    fn list_header(&mut self, ui: &mut Ui) {
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.allocate_space(Vec2::new(ICON_POINTS, 1.0)); // fluchtet mit dem Zeilensymbol
            let name_width =
                (ui.available_width() - COL_DATE - COL_SIZE - COL_KIND - 32.0).max(80.0);
            self.header_cell(ui, SortKey::Name, name_width, Align::LEFT);
            self.header_cell(ui, SortKey::Date, COL_DATE, Align::LEFT);
            self.header_cell(ui, SortKey::Size, COL_SIZE, Align::RIGHT);
            self.header_cell(ui, SortKey::Kind, COL_KIND, Align::LEFT);
        });
        ui.add_space(3.0);
    }

    fn header_cell(&mut self, ui: &mut Ui, key: SortKey, width: f32, align: Align) {
        let active = self.state.sort_key() == key;
        let color = if active {
            self.accent
        } else {
            ui.visuals().weak_text_color()
        };
        // Rang im Sortierstapel: Gleichstand-Brecher bekommen eine kleine Ziffer.
        let rank = self
            .state
            .sort_stack
            .iter()
            .position(|c| c.key == key)
            .filter(|i| *i > 0)
            .map(|i| format!(" {}", i + 1))
            .unwrap_or_default();
        let chevron = if self.state.sort_ascending() {
            "chevron.up"
        } else {
            "chevron.down"
        };

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, 15.0), Sense::click());
        let label = format!("{}{rank}", key.label());
        let galley = ui.painter().layout_no_wrap(
            label,
            egui::FontId::proportional(11.0),
            color,
        );
        let arrow = 9.0;
        let total = galley.size().x + if active { arrow + 3.0 } else { 0.0 };
        let start_x = if align == Align::RIGHT {
            rect.right() - total
        } else {
            rect.left()
        };
        let text_pos = egui::pos2(start_x, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(text_pos, galley.clone(), color);
        if active {
            let icon_rect = Rect::from_min_size(
                egui::pos2(start_x + galley.size().x + 3.0, rect.center().y - arrow / 2.0),
                Vec2::splat(arrow),
            );
            self.draw_symbol(ui, chevron, icon_rect, color);
        }
        if response.clicked() {
            self.state.sort_by(key);
            self.focus_filter = true;
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let shown = self.state.view.len();
            let total = self.state.total_count();
            let folder = self
                .state
                .current_dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();

            if let Some(msg) = self.state.transient_message() {
                let msg = msg.to_string();
                let green = Color32::from_rgb(52, 168, 83);
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(11.0), Sense::hover());
                self.draw_symbol(ui, "checkmark.circle.fill", rect, green);
                ui.add_space(3.0);
                ui.label(RichText::new(msg).size(11.0).color(green));
            } else if let Some(err) = &self.state.error {
                ui.label(
                    RichText::new(err.clone())
                        .size(11.0)
                        .color(Color32::from_rgb(220, 76, 62)),
                );
            } else if !self.state.filter.is_empty() {
                let suffix = if self.state.is_searching {
                    if self.state.searching_contents {
                        " · suche Name und Inhalt…"
                    } else {
                        " · suche…"
                    }
                } else {
                    ""
                };
                let text = if self.state.column_filtered {
                    format!("{shown} gefiltert{suffix} · rekursiv ab {folder}")
                } else {
                    let inhalt = self.state.content_hits();
                    let davon = if inhalt > 0 {
                        format!(" ({inhalt} im Inhalt)")
                    } else {
                        String::new()
                    };
                    format!("{shown} Treffer{davon}{suffix} · rekursiv ab {folder}")
                };
                if self.state.is_searching {
                    ui.add(egui::Spinner::new().size(11.0));
                    ui.add_space(3.0);
                }
                ui.label(RichText::new(text).size(11.0).weak());
            } else {
                let picked = self.state.selection.len();
                let prefix = if picked > 1 {
                    format!("{picked} ausgewählt · ")
                } else {
                    String::new()
                };
                ui.label(
                    RichText::new(format!(
                        "{prefix}{shown} / {total} · {:.1} ms",
                        self.state.load_ms
                    ))
                    .size(11.0)
                    .weak(),
                );
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(12.0);
                ui.label(
                    RichText::new(sc::HINTS)
                        .size(10.0)
                        .color(ui.visuals().weak_text_color()),
                );
            });
        });
        ui.add_space(3.0);
    }
}

// MARK: - Seitenleiste

impl FileRuneApp {
    fn sidebar(&mut self, ui: &mut Ui) {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(
                RichText::new("FAVORITEN")
                    .size(10.0)
                    .strong()
                    .color(ui.visuals().weak_text_color()),
            );
        });
        ui.add_space(4.0);

        let mut navigate_to: Option<PathBuf> = None;
        let mut remove: Option<PathBuf> = None;

        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - 34.0)
            .show(ui, |ui| {
                for fav in self.state.favorites.clone() {
                    let name = fav
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| fav.to_string_lossy().into_owned());
                    let current = fav == self.state.current_dir;

                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), 24.0),
                        Sense::click(),
                    );
                    if current || response.hovered() {
                        ui.painter().rect_filled(
                            rect.shrink2(Vec2::new(6.0, 1.0)),
                            5.0,
                            self.accent_alpha(if current { 0.16 } else { 0.07 }),
                        );
                    }
                    let icon_rect = Rect::from_min_size(
                        egui::pos2(rect.left() + 12.0, rect.center().y - ICON_POINTS / 2.0),
                        Vec2::splat(ICON_POINTS),
                    );
                    self.icons.draw_file_icon(ui, &fav, true, icon_rect);
                    let galley = galley_one_line(
                        ui.painter(),
                        &name,
                        egui::FontId::proportional(12.0),
                        if current {
                            self.accent
                        } else {
                            ui.visuals().text_color()
                        },
                        rect.width() - 40.0,
                        Trim::Middle,
                    );
                    ui.painter().galley(
                        egui::pos2(icon_rect.right() + 6.0, rect.center().y - galley.size().y / 2.0),
                        galley,
                        ui.visuals().text_color(),
                    );

                    if response.clicked() {
                        navigate_to = Some(fav.clone());
                    }
                    response.context_menu(|ui| {
                        if ui.button("Aus Favoriten entfernen").clicked() {
                            remove = Some(fav.clone());
                            ui.close();
                        }
                    });
                }
            });

        if let Some(p) = navigate_to {
            self.state.navigate(p, true);
            self.scroll_to = Some(0);
            self.focus_filter = true;
        }
        if let Some(p) = remove {
            self.state.remove_favorite(&p);
        }

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let response = ui.add(
                    egui::Label::new(
                        RichText::new("⊕  Aktuellen Ordner sichern")
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                    )
                    .sense(Sense::click()),
                );
                if response.on_hover_text(format!("Aktuellen Ordner zu den Favoriten ({})", sc::FAVORITE)).clicked() {
                    self.state.add_favorite(self.state.current_dir.clone());
                }
            });
        });
    }
}

// MARK: - Liste

impl FileRuneApp {
    fn list(&mut self, ui: &mut Ui) {
        let row_count = self.state.view.len();
        if row_count == 0 {
            ui.centered_and_justified(|ui| {
                let msg = if self.state.is_searching {
                    "Suche läuft …"
                } else if self.state.filter.is_empty() {
                    "Dieser Ordner ist leer."
                } else {
                    "Keine Treffer."
                };
                ui.label(RichText::new(msg).weak());
            });
            return;
        }

        // Suchtreffer zeigen ihren Ordner als zweite Zeile — dann brauchen ALLE
        // Zeilen dieselbe größere Höhe, sonst kann die Liste nicht virtualisieren.
        let has_subtitles = self.state.view.iter().any(|e| !e.rel_parent.is_empty());
        let row_height = if has_subtitles {
            ROW_HEIGHT_WITH_SUBTITLE
        } else {
            ROW_HEIGHT
        };

        let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if let Some(target) = self.scroll_to.take() {
            let y = target as f32 * row_height;
            scroll =
                scroll.vertical_scroll_offset((y - ui.available_height() / 2.0).max(0.0));
        }

        let mut action: Option<RowAction> = None;

        scroll.show_rows(ui, row_height, row_count, |ui, range| {
            for i in range {
                if let Some(a) = self.row(ui, i, row_height) {
                    action = Some(a);
                }
            }
        });

        self.apply_row_action(action);
    }

    fn row(&mut self, ui: &mut Ui, index: usize, row_height: f32) -> Option<RowAction> {
        let entry: Entry = self.state.view[index].clone();
        let selected = self.state.selection.contains(&entry.path);
        let is_cursor = index == self.state.selected_index;

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), row_height), Sense::click());

        if selected {
            // Der Cursor ist kräftiger getönt als die übrige Mehrfachauswahl —
            // 0.28 gegen 0.16, wie in EntryRow.
            ui.painter().rect_filled(
                rect.shrink2(Vec2::new(8.0, 1.0)),
                4.0,
                self.accent_alpha(if is_cursor { 0.28 } else { 0.16 }),
            );
        }

        let icon_rect = Rect::from_min_size(
            egui::pos2(rect.left() + 16.0, rect.center().y - ICON_POINTS / 2.0),
            Vec2::splat(ICON_POINTS),
        );
        self.icons.draw_file_icon(ui, &entry.path, entry.is_dir, icon_rect);

        let name_left = icon_rect.right() + 8.0;
        let kind_left = rect.right() - 16.0 - COL_KIND;
        let size_right = kind_left - 8.0;
        let date_left = size_right - COL_SIZE - 8.0 - COL_DATE;
        let name_width = (date_left - name_left - 8.0).max(60.0);

        // Zweite Zeile: bei Inhaltstreffern die Fundstelle, sonst der Ordner.
        // Bei Inhaltstreffern beides, denn wo die Datei liegt, will man auch wissen.
        let subtitle = match (&entry.matched_line, entry.rel_parent.as_str()) {
            (Some(line), "") => Some(line.clone()),
            (Some(line), folder) => Some(format!("{folder}  ·  {line}")),
            (None, "") => None,
            (None, folder) => Some(format!("↳ {folder}")),
        };
        let has_subtitle = subtitle.is_some();
        let painter = ui.painter();
        let text_color = ui.visuals().text_color();
        let weak = ui.visuals().weak_text_color();

        // Name (bei Suchtreffern oben, darunter der Ordner).
        let name_galley = galley_one_line(
            painter,
            &entry.name,
            egui::FontId::proportional(13.0),
            text_color,
            name_width,
            Trim::Middle,
        );
        let name_y = if has_subtitle {
            rect.center().y - name_galley.size().y - 1.0
        } else {
            rect.center().y - name_galley.size().y / 2.0
        };
        painter.galley(egui::pos2(name_left, name_y), name_galley.clone(), text_color);

        if let Some(text) = &subtitle {
            // Inhaltstreffer vorne kürzen wäre falsch — dort steht die Zeilennummer.
            let trim = if entry.matched_line.is_some() { Trim::Middle } else { Trim::Head };
            let sub = galley_one_line(
                painter,
                text,
                egui::FontId::proportional(10.0),
                if entry.matched_line.is_some() { self.accent } else { weak },
                name_width,
                trim,
            );
            painter.galley(
                egui::pos2(name_left, name_y + name_galley.size().y + 2.0),
                sub,
                weak,
            );
        }

        // Datum, Größe, Art — Ziffern in gleicher Breite wie .monospacedDigit().
        let mono = egui::FontId::monospace(11.0);
        let date = painter.layout_no_wrap(format_date(entry.modified), mono.clone(), weak);
        painter.galley(
            egui::pos2(date_left, rect.center().y - date.size().y / 2.0),
            date,
            weak,
        );

        let size_text = if entry.is_dir {
            "--".to_string()
        } else {
            format_size(entry.size)
        };
        let size = painter.layout_no_wrap(size_text, mono, weak);
        painter.galley(
            egui::pos2(size_right - size.size().x, rect.center().y - size.size().y / 2.0),
            size,
            weak,
        );

        let kind = galley_one_line(
            painter,
            &entry.kind(),
            egui::FontId::proportional(11.0),
            weak,
            COL_KIND,
            Trim::Middle,
        );
        painter.galley(
            egui::pos2(kind_left, rect.center().y - kind.size().y / 2.0),
            kind,
            weak,
        );

        let mut action = None;
        if response.double_clicked() {
            action = Some(RowAction::DoubleClick { index });
        } else if response.clicked() {
            let modifiers = ui.input(|i| i.modifiers);
            action = Some(RowAction::Click { index, modifiers });
        }

        response.context_menu(|ui| {
            if ui.button("Öffnen").clicked() {
                action = Some(RowAction::DoubleClick { index });
                ui.close();
            }
            if ui.button("Umbenennen…").clicked() {
                action = Some(RowAction::Rename { index });
                ui.close();
            }
            if ui.button("Pfad kopieren").clicked() {
                action = Some(RowAction::CopyPath { index });
                ui.close();
            }
            if has_subtitle && ui.button("Im übergeordneten Ordner zeigen").clicked() {
                action = Some(RowAction::RevealParent { index });
                ui.close();
            }
            if entry.is_dir && ui.button("Zu Favoriten").clicked() {
                action = Some(RowAction::AddFavorite { index });
                ui.close();
            }
            ui.separator();
            if ui.button("In den Papierkorb").clicked() {
                action = Some(RowAction::Trash { index });
                ui.close();
            }
        });

        action
    }

    fn apply_row_action(&mut self, action: Option<RowAction>) {
        match action {
            Some(RowAction::Click { index, modifiers }) => {
                if modifiers.shift {
                    self.state.range_select(index);
                } else if modifiers.command {
                    self.state.toggle_select(index);
                } else {
                    self.state.select_only(index);
                }
                self.focus_filter = true;
            }
            Some(RowAction::DoubleClick { index }) => {
                if let Some(e) = self.state.view.get(index).cloned() {
                    self.state.go_into(&e);
                    self.scroll_to = Some(0);
                }
                self.focus_filter = true;
            }
            Some(RowAction::Rename { index }) => {
                if let Some(e) = self.state.view.get(index) {
                    self.sheet = Sheet::Rename {
                        path: e.path.clone(),
                        text: e.name.clone(),
                    };
                }
            }
            Some(RowAction::CopyPath { index }) => {
                if let Some(e) = self.state.view.get(index).cloned() {
                    self.state.copy_path_of(&e);
                }
            }
            Some(RowAction::RevealParent { index }) => {
                if let Some(e) = self.state.view.get(index).cloned() {
                    self.state.reveal_in_parent(&e);
                    self.scroll_to = Some(self.state.selected_index);
                }
            }
            Some(RowAction::AddFavorite { index }) => {
                if let Some(e) = self.state.view.get(index).cloned() {
                    self.state.add_favorite(e.path);
                }
            }
            Some(RowAction::Trash { index }) => {
                self.state.select_only(index);
                self.state.move_selection_to_trash();
            }
            None => {}
        }
    }
}

enum RowAction {
    Click { index: usize, modifiers: Modifiers },
    DoubleClick { index: usize },
    Rename { index: usize },
    CopyPath { index: usize },
    RevealParent { index: usize },
    AddFavorite { index: usize },
    Trash { index: usize },
}

// MARK: - Modale Blätter

impl FileRuneApp {
    fn show_sheet(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut commit: Option<Sheet> = None;

        match &mut self.sheet {
            Sheet::None => return,
            Sheet::Rename { path, text } => {
                let path = path.clone();
                let mut buffer = text.clone();
                let submitted = sheet_window(ctx, "Umbenennen", None, "Umbenennen", &mut buffer, &mut close);
                if submitted {
                    commit = Some(Sheet::Rename { path, text: buffer.clone() });
                }
                *text = buffer;
            }
            Sheet::GoTo { text } => {
                let mut buffer = text.clone();
                let submitted = sheet_window(
                    ctx, "Gehe zu Ordner", Some("~/Developer"), "Öffnen", &mut buffer, &mut close,
                );
                if submitted {
                    commit = Some(Sheet::GoTo { text: buffer.clone() });
                }
                *text = buffer;
            }
            Sheet::NewFolder { text } => {
                let mut buffer = text.clone();
                let submitted = sheet_window(
                    ctx, "Neuer Ordner", Some("Neuer Ordner"), "Anlegen", &mut buffer, &mut close,
                );
                if submitted {
                    commit = Some(Sheet::NewFolder { text: buffer.clone() });
                }
                *text = buffer;
            }
        }

        match commit {
            Some(Sheet::Rename { path, text }) => {
                self.state.rename(&path, &text);
                close = true;
            }
            Some(Sheet::GoTo { text }) => {
                self.state.go_to_path(&text);
                self.scroll_to = Some(0);
                close = true;
            }
            Some(Sheet::NewFolder { text }) => {
                self.state.new_folder(&text);
                self.scroll_to = Some(self.state.selected_index);
                close = true;
            }
            _ => {}
        }

        if close {
            self.sheet = Sheet::None;
            self.focus_filter = true;
        }
    }
}

/// Ein modales Blatt mit einem Textfeld. Gibt `true` zurück, wenn bestätigt wurde.
fn sheet_window(
    ctx: &egui::Context,
    title: &str,
    hint: Option<&str>,
    confirm: &str,
    buffer: &mut String,
    close: &mut bool,
) -> bool {
    let mut submitted = false;
    egui::Modal::new(egui::Id::new(title)).show(ctx, |ui| {
        ui.set_width(320.0);
        ui.label(RichText::new(title).size(13.0).strong());
        ui.add_space(8.0);
        let mut edit = egui::TextEdit::singleline(buffer).desired_width(f32::INFINITY);
        if let Some(h) = hint {
            edit = edit.hint_text(h);
        }
        let field = ui.add(edit);
        field.request_focus();
        if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            submitted = true;
        }
        ui.add_space(10.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button(confirm).clicked() {
                submitted = true;
            }
            if ui.button("Abbrechen").clicked() {
                *close = true;
            }
        });
    });
    submitted
}

// MARK: - Icons und Aussehen

impl FileRuneApp {
    /// Ein Werkzeugknopf wie ToolButton.swift: 30 × 26, Radius 7, Akzenttönung
    /// wenn aktiv, dezente Tönung beim Überfahren.
    fn tool(&mut self, ui: &mut Ui, symbol: &str, help: &str, enabled: bool, active: bool) -> bool {
        let (rect, response) = ui.allocate_exact_size(TOOL_SIZE, Sense::click());
        let response = response.on_hover_text(help);

        let hovering = response.hovered() && enabled;
        if active {
            ui.painter().rect_filled(rect, TOOL_RADIUS, self.accent_alpha(0.14));
        } else if hovering {
            let base = ui.visuals().text_color();
            ui.painter().rect_filled(
                rect,
                TOOL_RADIUS,
                Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 20),
            );
        }

        let color = if !enabled {
            ui.visuals().weak_text_color()
        } else if active {
            self.accent
        } else {
            ui.visuals().text_color()
        };
        let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(TOOL_GLYPH));
        self.draw_symbol(ui, symbol, icon_rect, color);

        enabled && response.clicked()
    }

    /// Zeichnet ein Symbol — je nach System aus AppKit oder selbst gemalt.
    fn draw_symbol(&mut self, ui: &Ui, name: &str, rect: Rect, color: Color32) {
        self.icons.draw_symbol(ui, name, rect, color);
    }
}

/// Wo gekürzt wird, wenn der Text nicht passt.
#[derive(Clone, Copy, PartialEq)]
enum Trim {
    /// Mitte weg — für Dateinamen, damit Anfang UND Endung lesbar bleiben.
    /// Entspricht `.truncationMode(.middle)` der SwiftUI-Zeile.
    Middle,
    /// Anfang weg — für Pfade, wo der hintere Teil zählt (`.head`).
    Head,
}

/// Text auf eine Zeile kürzen. Ohne das umbricht egui lange Namen auf zwei
/// Zeilen, die dann in die nächste Tabellenzeile ragen — die Zeilenhöhe ist fest.
fn galley_one_line(
    painter: &egui::Painter,
    text: &str,
    font: egui::FontId,
    color: Color32,
    max_width: f32,
    trim: Trim,
) -> std::sync::Arc<egui::Galley> {
    let full = painter.layout_no_wrap(text.to_string(), font.clone(), color);
    if full.size().x <= max_width || max_width <= 0.0 {
        return full;
    }

    let chars: Vec<char> = text.chars().collect();
    let build = |keep: usize| -> String {
        match trim {
            Trim::Head => format!("…{}", chars[chars.len() - keep..].iter().collect::<String>()),
            Trim::Middle => {
                let front = keep.div_ceil(2);
                let back = keep - front;
                let mut out: String = chars[..front].iter().collect();
                out.push('…');
                out.extend(&chars[chars.len() - back..]);
                out
            }
        }
    };

    // Binäre Suche über die Zahl behaltener Zeichen — bei ~30 sichtbaren
    // Zeilen ist das billiger als zeichenweises Abschneiden.
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let candidate = painter.layout_no_wrap(build(mid), font.clone(), color);
        if candidate.size().x <= max_width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    painter.layout_no_wrap(build(lo), font, color)
}

fn install_style(ctx: &egui::Context, accent: Color32) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = Vec2::new(6.0, 3.0);
        style.spacing.button_padding = Vec2::new(8.0, 4.0);
        style.visuals.striped = false;
        style.visuals.selection.bg_fill = accent;
        style.visuals.hyperlink_color = accent;
        // macOS-Textgrößen: body 13, caption 11, caption2 10.
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::proportional(13.0),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::proportional(13.0),
        );
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::monospace(12.0),
        );
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::proportional(10.0),
        );
    });
}
