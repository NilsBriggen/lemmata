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

const GROUPS: [&str; 7] = [
    "Project",
    "Navigation",
    "Files",
    "Writing",
    "Editing",
    "PDF",
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
                    // Nor does it type into the search box.
                    i.events.retain(|e| !matches!(e, egui::Event::Text(_)));
                    press
                });
                if let Some((key, mods)) = press {
                    set_recording(ctx, None);
                    if key != egui::Key::Escape
                        && let Some(spec) = shortcuts::spec_of(mods, key)
                    {
                        assign(settings, id, &spec);
                        *action = Some(super::Action::Settings);
                    }
                }
            }
            let mut done = false;
            let r = widgets::modal_with(
                ctx,
                p,
                "Keyboard shortcuts",
                None,
                620.0,
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
                }),
            );
            if r.closed || done {
                self.shortcut_sheet = None;
                set_recording(ctx, None);
            }
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
    ui.spacing_mut().item_spacing.y = 12.0;
    // .total: the number in Young Serif 40 px, then "words".
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            RichText::new(group_digits(c.total_words))
                .font(FontId::new(40.0, theme::font::display()))
                .color(p.ink),
        );
        ui.label(RichText::new("words").size(15.0).color(p.muted));
    });
    let stats = [
        ("Headings", c.headings),
        ("Inline maths", c.inline_math),
        ("Display maths", c.display_math),
        ("Figures and tables", c.floats),
        ("Citations", c.citations),
    ];
    egui::Grid::new("word-stats")
        .num_columns(2)
        .spacing(vec2(24.0, 8.0))
        .min_col_width((ui.available_width() - 24.0) / 2.0)
        .show(ui, |ui| {
            for (i, (name, n)) in stats.iter().enumerate() {
                let r = ui.horizontal(|ui| {
                    ui.set_width((ui.available_width()).max(0.0));
                    ui.label(RichText::new(*name).color(p.muted));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(group_digits(*n)).color(p.ink));
                    });
                });
                let b = r.response.rect;
                ui.painter()
                    .hline(b.x_range(), b.bottom() + 4.0, Stroke::new(1.0, p.border));
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    if c.files.len() > 1 {
        ui.add_space(4.0);
        widgets::label(ui, p, "By file");
        let w = ui.available_width();
        ui.allocate_ui(vec2(w, 200.0), |ui| {
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    for f in &c.files {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&f.file)
                                    .font(FontId::monospace(12.0))
                                    .color(p.ink),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(RichText::new(group_digits(f.words)).color(p.ink));
                                },
                            );
                        });
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

/// Gives `id` the keys `spec`. Another shortcut of its group with the same keys loses them, so
/// one key press never means two things there.
fn assign(settings: &mut Settings, id: &str, spec: &str) {
    let Some(me) = SHORTCUTS.iter().find(|s| s.id == id) else {
        return;
    };
    for other in SHORTCUTS
        .iter()
        .filter(|s| s.group == me.group && s.id != id)
    {
        if shortcuts::specs(other).contains(&spec) {
            settings.keybindings.insert(other.id.into(), String::new());
        }
    }
    if spec == me.keys {
        settings.keybindings.remove(id);
    } else {
        settings.keybindings.insert(id.into(), spec.into());
    }
    shortcuts::choose(&settings.keybindings);
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
    let max_h = ui.ctx().content_rect().height() * 0.6;
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
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(s.label).size(13.0).color(p.ink),
                                            );
                                            let editable = shortcuts::editable(s);
                                            let k = if recording == Some(s.id) {
                                                "Press keys…".to_owned()
                                            } else {
                                                match shortcuts::key_label(s) {
                                                    k if k.is_empty() && editable => "None".into(),
                                                    k => k,
                                                }
                                            };
                                            if !k.is_empty() {
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        let chip = egui::Frame::new()
                                                            .fill(p.surface_2)
                                                            .stroke(Stroke::new(1.0, p.border))
                                                            .corner_radius(CornerRadius::same(4))
                                                            .inner_margin(Margin::symmetric(6, 1))
                                                            .show(ui, |ui| {
                                                                ui.label(
                                                                    RichText::new(&k)
                                                                        .size(11.5)
                                                                        .color(p.ink_2),
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
                                                        if settings.keybindings.contains_key(s.id)
                                                            && ui
                                                                .link(
                                                                    RichText::new("Reset")
                                                                        .size(11.5)
                                                                        .color(p.muted),
                                                                )
                                                                .clicked()
                                                        {
                                                            settings.keybindings.remove(s.id);
                                                            shortcuts::choose(
                                                                &settings.keybindings,
                                                            );
                                                            changed = true;
                                                        }
                                                    },
                                                );
                                            }
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
    #[test]
    fn digits_are_grouped() {
        assert_eq!(super::group_digits(0), "0");
        assert_eq!(super::group_digits(999), "999");
        assert_eq!(super::group_digits(12345), "12,345");
        assert_eq!(super::group_digits(1234567), "1,234,567");
    }
}
