//! Word count (WordCount.svelte) and the keyboard shortcuts sheet (Shortcuts.svelte).

use super::Workspace;
use crate::backend::Backend;
use crate::shortcuts::{self, SHORTCUTS};
use crate::store::Settings;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind};
use eframe::egui::{self, CornerRadius, FontId, Margin, RichText, Stroke, Ui, vec2};
use lemmata_project::extras::Counts;

/// The word count's state: counting, the counts, or why it failed.
pub enum Words {
    Counting,
    Done(Counts),
    Failed(String),
}

/// 12345 → "12,345": thousands separated, as Intl.NumberFormat() in English.
pub fn group_digits(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

const GROUPS: [&str; 8] = [
    "Project",
    "Navigation",
    "Files",
    "Writing",
    "Editing",
    "PDF",
    "Presenting",
    "Mouse",
];

impl Workspace {
    pub fn open_word_count(&mut self, backend: &Backend) {
        self.words = Some(Words::Counting);
        Workspace::job(
            backend,
            |s| s.word_count(),
            |ws, _, r| {
                if ws.words.is_some() {
                    ws.words = Some(match r {
                        Ok(c) => Words::Done(c),
                        Err(e) => Words::Failed(e),
                    });
                }
            },
        );
    }

    pub(super) fn sheets(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        settings: &mut Settings,
        action: &mut Option<super::Action>,
    ) {
        if let Some(w) = &self.words {
            let mut done = false;
            let r = widgets::modal_with(
                ctx,
                p,
                "Word count",
                Some("Text the reader sees: no commands, maths or comments."),
                460.0,
                |ui| word_count(ui, p, w),
                Some(&mut |ui: &mut Ui| {
                    if Button::new("Done")
                        .kind(Kind::Primary)
                        .show(ui, p)
                        .clicked()
                    {
                        done = true;
                    }
                }),
            );
            if r.closed || done {
                self.words = None;
            }
        }
        let mut told: Option<String> = None;
        if let Some(query) = &mut self.shortcut_sheet {
            // The next key press while a shortcut is recording is its new key; Escape stops,
            // before the dialog takes Escape for closing.
            if let Some(id) = recording(ctx) {
                let press = ctx.input_mut(|i| {
                    let at = i
                        .events
                        .iter()
                        .position(|e| matches!(e, egui::Event::Key { pressed: true, .. }))?;
                    let press = match i.events.remove(at) {
                        egui::Event::Key { key, modifiers, .. } => Some((key, modifiers)),
                        _ => None,
                    };
                    // Nor does it type, paste, copy, or cut inside the search box.
                    i.events.retain(|e| {
                        !matches!(
                            e,
                            egui::Event::Text(_)
                                | egui::Event::Paste(_)
                                | egui::Event::Copy
                                | egui::Event::Cut
                        )
                    });
                    press
                });
                if let Some((key, mods)) = press {
                    set_recording(ctx, None);
                    if key != egui::Key::Escape {
                        match shortcuts::spec_of(mods, key) {
                            Some(spec) => match assign(settings, id, &spec) {
                                Ok(note) => {
                                    *action = Some(super::Action::Settings);
                                    told = note;
                                }
                                Err(why) => told = Some(why),
                            },
                            None => {
                                told = Some(
                                    "Shortcuts need Ctrl, Alt, or a function key so normal typing still works."
                                        .into(),
                                );
                            }
                        }
                    }
                }
            }
            let mut done = false;
            let mut reset_all = false;
            let has_custom = !settings.keybindings.is_empty();
            let r = widgets::modal_with(
                ctx,
                p,
                "Keyboard shortcuts",
                None,
                660.0,
                |ui| {
                    if shortcut_sheet(ui, p, query, settings) {
                        *action = Some(super::Action::Settings);
                    }
                },
                Some(&mut |ui: &mut Ui| {
                    if Button::new("Done")
                        .kind(Kind::Primary)
                        .show(ui, p)
                        .clicked()
                    {
                        done = true;
                    }
                    if has_custom
                        && Button::new("Reset all")
                            .with_icon(crate::icons::ROTATE_CCW)
                            .kind(Kind::Ghost)
                            .show(ui, p)
                            .clicked()
                    {
                        reset_all = true;
                    }
                }),
            );
            if reset_all {
                settings.keybindings.clear();
                shortcuts::choose(&settings.keybindings);
                *action = Some(super::Action::Settings);
            }
            if r.closed || done {
                self.shortcut_sheet = None;
                set_recording(ctx, None);
            }
        }
        if let Some(text) = told {
            self.notice(text, super::Tone::Info);
        }
    }
}

fn word_count(ui: &mut Ui, p: &Palette, w: &Words) {
    let c = match w {
        Words::Counting => {
            ui.label(RichText::new("Counting…").color(p.muted));
            return;
        }
        Words::Failed(e) => {
            ui.label(RichText::new(e).color(p.error));
            return;
        }
        Words::Done(c) => c,
    };
    ui.spacing_mut().item_spacing.y = 14.0;
    // Hero total in Young Serif 40 px, paired with estimated reading & speaking times.
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 4.0);
        ui.label(
            RichText::new(group_digits(c.total_words))
                .font(FontId::new(40.0, theme::font::display()))
                .color(p.ink),
        );
        ui.vertical(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new("words").size(15.0).color(p.muted));
            if c.total_words > 0 {
                let read_min = (c.total_words as f32 / 200.0).ceil().max(1.0) as u32;
                let speak_min = (c.total_words as f32 / 130.0).ceil().max(1.0) as u32;
                ui.label(
                    RichText::new(format!("~{read_min} min read · ~{speak_min} min spoken"))
                        .size(11.5)
                        .color(p.faint),
                );
            }
        });
    });
    let stats = [
        ("Headings", c.headings),
        ("Inline maths", c.inline_math),
        ("Display maths", c.display_math),
        ("Figures & tables", c.floats),
        ("Citations", c.citations),
    ];
    let col_w = ((ui.available_width() - 10.0) / 2.0).max(120.0);
    egui::Grid::new("word-stats")
        .num_columns(2)
        .spacing(vec2(10.0, 8.0))
        .min_col_width(col_w)
        .show(ui, |ui| {
            for (i, (name, n)) in stats.iter().enumerate() {
                egui::Frame::new()
                    .fill(p.surface_2)
                    .stroke(Stroke::new(1.0, p.card_hairline))
                    .corner_radius(theme::R_MD)
                    .inner_margin(Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_width((col_w - 24.0).max(80.0));
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(*name).size(12.5).color(p.muted));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(group_digits(*n))
                                            .font(FontId::new(13.0, theme::font::mono_500()))
                                            .color(p.ink),
                                    );
                                },
                            );
                        });
                    });
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    if c.files.len() > 1 {
        ui.add_space(2.0);
        widgets::label(ui, p, "By file");
        let max_file_words = c.files.iter().map(|f| f.words).max().unwrap_or(1).max(1);
        let w = ui.available_width();
        ui.allocate_ui(vec2(w, 190.0), |ui| {
            egui::ScrollArea::vertical()
                .max_height(190.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    for f in &c.files {
                        let frac = (f.words as f32 / max_file_words as f32).clamp(0.0, 1.0);
                        let (row_r, _) = ui.allocate_exact_size(
                            vec2(ui.available_width(), 24.0),
                            egui::Sense::hover(),
                        );
                        if frac > 0.0 {
                            let bar = egui::Rect::from_min_size(
                                row_r.min,
                                vec2(row_r.width() * frac, row_r.height()),
                            );
                            ui.painter().rect_filled(
                                bar,
                                CornerRadius::same(5),
                                p.accent_soft.gamma_multiply(0.55),
                            );
                        }
                        ui.painter().text(
                            row_r.left_center() + vec2(8.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            &f.file,
                            FontId::monospace(12.0),
                            p.ink,
                        );
                        ui.painter().text(
                            row_r.right_center() - vec2(8.0, 0.0),
                            egui::Align2::RIGHT_CENTER,
                            group_digits(f.words),
                            FontId::new(12.0, theme::font::mono_500()),
                            p.ink_2,
                        );
                    }
                });
        });
    }
}

fn recording_id() -> egui::Id {
    egui::Id::unique("shortcut-recording")
}

/// The shortcut waiting for its new key, if one is.
fn recording(ctx: &egui::Context) -> Option<&'static str> {
    ctx.data(|d| d.get_temp::<&'static str>(recording_id()))
}

fn set_recording(ctx: &egui::Context, id: Option<&'static str>) {
    ctx.data_mut(|d| match id {
        Some(id) => {
            d.insert_temp(recording_id(), id);
        }
        None => d.remove::<&'static str>(recording_id()),
    });
}

/// Whether two groups of shortcuts can be pressed in the same place: the project and navigation
/// ones work everywhere, the others where their group does (writing and editing, in the editor).
fn overlap(a: &str, b: &str) -> bool {
    fn scope(g: &str) -> &str {
        match g {
            "Writing" | "Editing" => "editor",
            g => g,
        }
    }
    let global = |g: &str| matches!(g, "Project" | "Navigation");
    global(a) || global(b) || scope(a) == scope(b)
}

/// Gives `id` the keys `spec`. Another shortcut that could be pressed in the same place with the
/// same keys loses them, so one key press never means two things; one that can't be changed
/// (Move line up), or a key every text field uses (copy, paste, undo), is refused.
/// Ok holds what the user is told when another shortcut lost its keys.
fn assign(settings: &mut Settings, id: &str, spec: &str) -> Result<Option<String>, String> {
    let Some(me) = SHORTCUTS.iter().find(|s| s.id == id) else {
        return Ok(None);
    };
    if [
        "Mod-c",
        "Mod-x",
        "Mod-v",
        "Mod-a",
        "Mod-z",
        "Mod-y",
        "Mod-Shift-z",
    ]
    .contains(&spec)
    {
        return Err(format!(
            "{spec} is for copy, paste, undo and redo in every text field."
        ));
    }
    let clash = |other: &&shortcuts::Shortcut| {
        other.id != id
            && overlap(other.group, me.group)
            && shortcuts::specs(other)
                .iter()
                .any(|keys| shortcuts::specs_clash(keys, spec))
    };
    if let Some(fixed) = SHORTCUTS
        .iter()
        .filter(|s| !shortcuts::editable(s))
        .find(|s| clash(s))
    {
        return Err(format!(
            "{spec} is used by \"{}\", which can't be changed.",
            fixed.label
        ));
    }
    let mut note = None;
    for other in SHORTCUTS.iter().filter(clash) {
        settings.keybindings.insert(other.id.into(), String::new());
        note = Some(format!(
            "{spec} was used by \"{}\": that has no keys now.",
            other.label
        ));
    }
    if spec == me.keys {
        settings.keybindings.remove(id);
    } else {
        settings.keybindings.insert(id.into(), spec.into());
    }
    shortcuts::choose(&settings.keybindings);
    Ok(note)
}

/// The sheet; true when a key was reset.
fn shortcut_sheet(ui: &mut Ui, p: &Palette, query: &mut String, settings: &mut Settings) -> bool {
    let mut changed = false;
    let recording = recording(ui.ctx());
    let r = widgets::text_input(ui, p, query, "Search shortcuts", false);
    ui.ctx()
        .accesskit_node_builder(r.id, |n| n.set_label("Search shortcuts"));
    if r.ctx.memory(|m| m.focused().is_none()) {
        r.request_focus();
    }
    let shown: Vec<_> = SHORTCUTS
        .iter()
        .filter(|s| shortcuts::matches_query(s, query))
        .collect();
    if shown.is_empty() {
        ui.label(RichText::new("No shortcut matches").color(p.muted));
        return false;
    }
    ui.label(
        RichText::new("Click a key to change it, then press the new keys. Escape keeps it.")
            .size(12.0)
            .color(p.muted),
    );
    ui.add_space(6.0);
    // `columns: 2`: groups flow down the first column, then the second, without splitting.
    let groups: Vec<(&str, Vec<_>)> = GROUPS
        .iter()
        .map(|g| {
            (
                *g,
                shown.iter().filter(|s| s.group == *g).collect::<Vec<_>>(),
            )
        })
        .filter(|(_, l)| !l.is_empty())
        .collect();
    let rows: usize = groups.iter().map(|(_, l)| l.len() + 2).sum();
    let mut split = 0;
    let mut acc = 0;
    for (i, (_, l)) in groups.iter().enumerate() {
        if acc * 2 >= rows {
            break;
        }
        acc += l.len() + 2;
        split = i + 1;
    }
    let vh = ui.ctx().content_rect().height();
    let max_h = (vh - 240.0).clamp(160.0, vh * 0.6);
    let w = ui.available_width();
    ui.allocate_ui(vec2(w, max_h), |ui| {
        egui::ScrollArea::vertical()
            .max_height(max_h)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.columns(2, |cols| {
                    for (c, part) in [&groups[..split], &groups[split..]].into_iter().enumerate() {
                        let ui = &mut cols[c];
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (g, list) in part {
                            widgets::label(ui, p, g);
                            ui.add_space(6.0);
                            for s in list {
                                let r = egui::Frame::new()
                                    .inner_margin(Margin::symmetric(0, 4))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        let editable = shortcuts::editable(s);
                                        let k = if recording == Some(s.id) {
                                            "Press keys…".to_owned()
                                        } else {
                                            match shortcuts::key_label(s) {
                                                k if k.is_empty() && editable => "None".into(),
                                                k => k,
                                            }
                                        };
                                        ui.horizontal(|ui| {
                                            ui.set_width(ui.available_width());
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if !k.is_empty() {
                                                        let is_rec = recording == Some(s.id);
                                                        let chip = egui::Frame::new()
                                                            .fill(if is_rec {
                                                                p.accent_soft
                                                            } else {
                                                                p.surface_2
                                                            })
                                                            .stroke(Stroke::new(
                                                                1.0,
                                                                if is_rec {
                                                                    p.accent
                                                                } else {
                                                                    p.border_strong
                                                                },
                                                            ))
                                                            .corner_radius(CornerRadius::same(5))
                                                            .inner_margin(Margin::symmetric(7, 2))
                                                            .show(ui, |ui| {
                                                                ui.label(
                                                                    RichText::new(&k)
                                                                        .font(FontId::new(
                                                                            11.5,
                                                                            theme::font::mono_500(),
                                                                        ))
                                                                        .color(if is_rec {
                                                                            p.accent
                                                                        } else {
                                                                            p.ink_2
                                                                        }),
                                                                )
                                                            });
                                                        if editable {
                                                            let r = ui
                                                                .interact(
                                                                    chip.response.rect,
                                                                    ui.scope_id().with(s.id),
                                                                    egui::Sense::click(),
                                                                )
                                                                .on_hover_cursor(
                                                                    egui::CursorIcon::PointingHand,
                                                                )
                                                                .on_hover_text("Change the keys");
                                                            ui.ctx().accesskit_node_builder(
                                                                r.id,
                                                                |n| {
                                                                    n.set_label(format!(
                                                                        "Keys for {}",
                                                                        s.label
                                                                    ))
                                                                },
                                                            );
                                                            if r.clicked() {
                                                                set_recording(ui.ctx(), Some(s.id));
                                                            }
                                                        }
                                                        if editable
                                                            && !shortcuts::specs(s).is_empty()
                                                            && Button::icon(
                                                                crate::icons::X,
                                                                "Clear shortcut",
                                                            )
                                                            .size(widgets::Size::Sm)
                                                            .icon_size(12.0)
                                                            .show_icon(ui, p)
                                                            .on_hover_text("Clear shortcut")
                                                            .clicked()
                                                        {
                                                            settings
                                                                .keybindings
                                                                .insert(s.id.into(), String::new());
                                                            shortcuts::choose(
                                                                &settings.keybindings,
                                                            );
                                                            changed = true;
                                                        }
                                                        if settings.keybindings.contains_key(s.id)
                                                            && Button::icon(
                                                                crate::icons::ROTATE_CCW,
                                                                "Reset shortcut",
                                                            )
                                                            .size(widgets::Size::Sm)
                                                            .icon_size(12.0)
                                                            .show_icon(ui, p)
                                                            .on_hover_text("Reset to default")
                                                            .clicked()
                                                        {
                                                            settings.keybindings.remove(s.id);
                                                            shortcuts::choose(
                                                                &settings.keybindings,
                                                            );
                                                            changed = true;
                                                        }
                                                    }
                                                    ui.with_layout(
                                                        egui::Layout::left_to_right(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            ui.add(
                                                                egui::Label::new(
                                                                    RichText::new(s.label)
                                                                        .size(13.0)
                                                                        .color(p.ink),
                                                                )
                                                                .truncate(),
                                                            );
                                                        },
                                                    );
                                                },
                                            );
                                        });
                                    });
                                let b = r.response.rect;
                                ui.painter().hline(
                                    b.x_range(),
                                    b.bottom() - 0.5,
                                    Stroke::new(1.0, p.border),
                                );
                            }
                            ui.add_space(16.0);
                        }
                    }
                });
            });
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_in_use_in_the_same_place_is_taken_over_or_refused() {
        let mut s = Settings::default();
        // Save all files uses Mod-s, and works everywhere: "Find in file" can't have it too.
        let note = assign(&mut s, "find", "Mod-s").unwrap();
        assert!(note.is_some());
        assert_eq!(s.keybindings.get("save").map(String::as_str), Some(""));
        // Move line up can't be changed, and its key isn't handed to another shortcut.
        assert!(assign(&mut s, "select-line", "Alt-ArrowUp").is_err());
        assert!(!s.keybindings.contains_key("move-line-up"));
        assert!(assign(&mut s, "find", "Mod-v").is_err());
        // The keys in effect are shared by the whole process.
        shortcuts::choose(&Default::default());
    }

    #[test]
    fn digits_are_grouped() {
        assert_eq!(super::group_digits(0), "0");
        assert_eq!(super::group_digits(999), "999");
        assert_eq!(super::group_digits(12345), "12,345");
        assert_eq!(super::group_digits(1234567), "1,234,567");
    }
}
