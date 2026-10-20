//! App-wide preferences (Preferences.svelte) and the user's snippets (SnippetsDialog.svelte);
//! lemmata.toml holds the per-project ones.

use crate::icons;
use crate::store::{Settings, Snippet};
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind};
use eframe::egui::{self, FontId, Margin, RichText, Stroke, Ui, vec2};
use lemmata_spell::Available;

#[derive(Default)]
pub struct Prefs {
    /// Spell-check languages; the ones not installed are fetched when chosen.
    pub dicts: Vec<Available>,
    pub downloading: Option<String>,
    pub spell_error: Option<String>,
    pub grammar_status: Option<String>,
    pub snippets: Option<Vec<Snippet>>,
    /// Opened for the snippets alone (the palette's "Edit snippets…"): closing them closes all.
    pub only_snippets: bool,
}

pub enum Out {
    /// A setting changed: save it.
    Changed,
    Close,
    ChooseTex,
    AutoTex,
    Download(String),
    TestGrammar,
}

const INDENTS: [(&str, &str); 4] = [
    ("2", "2 spaces"),
    ("4", "4 spaces"),
    ("8", "8 spaces"),
    ("tab", "Tab"),
];

/// `section h3`: the uppercase group title.
fn heading(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add_space(4.0);
    widgets::label(ui, p, text);
}

/// `.row`: a 140 px label, then the control (at most 300 px wide).
fn row<R>(ui: &mut Ui, p: &Palette, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.allocate_ui_with_layout(
            vec2(140.0, 32.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(140.0);
                ui.label(RichText::new(label).color(p.ink));
            },
        );
        let w = ui.available_width().min(300.0);
        ui.allocate_ui_with_layout(
            vec2(w, 32.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| add(ui),
        )
        .inner
    })
    .inner
}

/// A select in a row, named by the row's label.
fn select(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    value: &mut String,
    options: &[(&str, &str)],
) -> bool {
    row(ui, p, label, |ui| {
        widgets::select(ui, p, label, value, options)
    })
}

fn section_end(ui: &mut Ui, p: &Palette) {
    ui.add_space(4.0);
    let y = ui.cursor().top();
    ui.painter()
        .hline(ui.max_rect().x_range(), y, Stroke::new(1.0, p.border));
    ui.add_space(4.0);
}

pub fn show(
    ctx: &egui::Context,
    p: &Palette,
    st: &mut Prefs,
    s: &mut Settings,
    tex: Option<&lemmata_shell::TexStatus>,
    system_dark: bool,
) -> Option<Out> {
    if st.snippets.is_some() {
        return snippets(ctx, p, st, s);
    }
    if st.only_snippets {
        return Some(Out::Close);
    }
    let mut out = None;
    let mut done = false;
    let r = widgets::modal_with(
        ctx,
        p,
        "Preferences",
        Some("These apply to every project."),
        600.0,
        |ui| {
            let max_h = ui.ctx().content_rect().height() * 0.62;
            let w = ui.available_width();
            ui.allocate_ui(vec2(w, max_h), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(max_h)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_width(w - 12.0);
                        ui.spacing_mut().item_spacing.y = 10.0;
                        body(ui, p, st, s, tex, system_dark, &mut out);
                    });
            });
        },
        Some(&mut |ui: &mut Ui| {
            if Button::new("Done")
                .kind(Kind::Primary)
                .show(ui, p)
                .clicked()
            {
                done = true;
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("Lemmata {}", env!("CARGO_PKG_VERSION")))
                        .size(12.0)
                        .color(p.faint),
                );
            });
        }),
    );
    if r.closed || done {
        return Some(Out::Close);
    }
    out
}

fn body(
    ui: &mut Ui,
    p: &Palette,
    st: &mut Prefs,
    s: &mut Settings,
    tex: Option<&lemmata_shell::TexStatus>,
    system_dark: bool,
    out: &mut Option<Out>,
) {
    let mut changed = false;
    heading(ui, p, "Appearance");
    row(ui, p, "Theme", |ui| {
        let opts = [
            (Some(icons::MONITOR), "System"),
            (Some(icons::SUN), "Light"),
            (Some(icons::MOON), "Dark"),
        ];
        let sel = ["system", "light", "dark"]
            .iter()
            .position(|t| *t == s.theme)
            .unwrap_or(0);
        if let Some(i) = widgets::segmented(ui, p, &opts, sel, false, false) {
            s.theme = ["system", "light", "dark"][i].into();
            changed = true;
        }
    });
    let mut fonts: Vec<(String, String)> = vec![
        ("plex".into(), "IBM Plex Mono".into()),
        ("system".into(), "System monospace".into()),
    ];
    if !["plex", "system"].contains(&s.font_family.as_str()) {
        fonts.push((s.font_family.clone(), s.font_family.clone()));
    }
    for f in [
        "JetBrains Mono",
        "Fira Code",
        "Cascadia Code",
        "Source Code Pro",
        "Menlo",
        "Consolas",
    ] {
        if f != s.font_family {
            fonts.push((f.into(), format!("{f} (if installed)")));
        }
    }
    let opts: Vec<(&str, &str)> = fonts
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    changed |= select(ui, p, "Font", &mut s.font_family, &opts);
    let mut size = format!("{}", s.font_size);
    let sizes: Vec<(String, String)> = [11, 12, 13, 14, 15, 16, 18, 20]
        .iter()
        .map(|n| (n.to_string(), format!("{n} px")))
        .collect();
    let opts: Vec<(&str, &str)> = sizes
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    if select(ui, p, "Editor text size", &mut size, &opts) {
        s.font_size = size.parse().unwrap_or(13.0);
        changed = true;
    }
    let mut lh = format!("{}", s.line_height);
    if select(
        ui,
        p,
        "Line height",
        &mut lh,
        &[
            ("1.4", "1.4"),
            ("1.5", "1.5"),
            ("1.65", "1.65"),
            ("1.8", "1.8"),
            ("2", "2"),
        ],
    ) {
        s.line_height = lh.parse().unwrap_or(1.65);
        changed = true;
    }
    let dark = s.dark(system_dark);
    ui.add_enabled_ui(dark, |ui| {
        let mut on = s.pdf_dark;
        let text = if dark {
            "Dark PDF pages"
        } else {
            "Dark PDF pages (only in a dark theme)"
        };
        if widgets::checkbox(ui, p, &mut on, text).clicked() {
            s.pdf_dark = on;
            changed = true;
        }
    });
    section_end(ui, p);

    heading(ui, p, "Editing");
    row(ui, p, "Save", |ui| {
        let sel = usize::from(s.autosave_ms == 0.0);
        if let Some(i) = widgets::segmented(
            ui,
            p,
            &[(None, "As I type"), (None, "With Ctrl+S")],
            sel,
            false,
            false,
        ) {
            s.autosave_ms = if i == 0 {
                Settings::default().autosave_ms
            } else {
                0.0
            };
            changed = true;
        }
    });
    changed |= widgets::checkbox(ui, p, &mut s.auto_compile, "Compile after every save").clicked();
    changed |= widgets::checkbox(
        ui,
        p,
        &mut s.autocomplete,
        "Suggest commands, environments, labels and citations while typing",
    )
    .clicked();
    row(ui, p, "Snippets", |ui| {
        let label = if s.snippets.is_empty() {
            "Add…".to_owned()
        } else {
            format!("Edit {}…", s.snippets.len())
        };
        let b = Button::new(&label).enabled(s.autocomplete).show(ui, p);
        let b = if s.autocomplete {
            b
        } else {
            b.on_disabled_hover_text("Snippets are offered by the suggestions, which are off")
        };
        if b.clicked() {
            st.snippets = Some(s.snippets.clone());
        }
    });
    changed |= widgets::checkbox(
        ui,
        p,
        &mut s.env_sync,
        "Rename \\end{…} when \\begin{…} changes, and the other way round",
    )
    .clicked();
    changed |= widgets::checkbox(
        ui,
        p,
        &mut s.auto_close,
        "Close brackets and dollar signs automatically",
    )
    .clicked();
    changed |= widgets::checkbox(
        ui,
        p,
        &mut s.folding,
        "Code folding for sections and environments",
    )
    .clicked();
    let mut indent = match &s.indent {
        serde_json::Value::String(t) if t == "tab" => "tab".to_owned(),
        v => v.as_u64().unwrap_or(2).to_string(),
    };
    if select(ui, p, "Indentation", &mut indent, &INDENTS) {
        s.indent = if indent == "tab" {
            serde_json::Value::from("tab")
        } else {
            serde_json::Value::from(indent.parse::<u64>().unwrap_or(2))
        };
        changed = true;
    }
    changed |= widgets::checkbox(ui, p, &mut s.word_wrap, "Wrap long lines").clicked();
    section_end(ui, p);

    heading(ui, p, "Language");
    let mut spell: Vec<(String, String)> =
        vec![("auto".into(), "Match the document's language".into())];
    for (code, label) in crate::writing::spell::LANGUAGES {
        let note = st
            .dicts
            .iter()
            .find(|d| d.code == *code && !d.installed)
            .map(|d| {
                format!(
                    " (downloads {} MB)",
                    (d.size_kb as f32 / 1024.0).round().max(1.0)
                )
            })
            .unwrap_or_default();
        spell.push(((*code).into(), format!("{label}{note}")));
    }
    spell.push(("off".into(), "Off".into()));
    let opts: Vec<(&str, &str)> = spell
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let mut lang = s.spell_default.clone();
    let picked = ui
        .add_enabled_ui(st.downloading.is_none(), |ui| {
            select(ui, p, "Spell check", &mut lang, &opts)
        })
        .inner;
    if picked {
        st.spell_error = None;
        match st.dicts.iter().find(|d| d.code == lang) {
            Some(d) if !d.installed => {
                st.downloading = Some(lang.clone());
                *out = Some(Out::Download(lang));
            }
            _ => {
                s.spell_default = lang;
                changed = true;
            }
        }
    }
    if let Some(d) = &st.downloading {
        ui.label(
            RichText::new(format!(
                "Downloading the {} dictionary…",
                crate::writing::spell::label(d)
            ))
            .size(12.5)
            .color(p.muted),
        );
    } else if let Some(e) = &st.spell_error {
        ui.label(RichText::new(e).size(12.5).color(p.error));
    }
    if select(
        ui,
        p,
        "Grammar check",
        &mut s.grammar,
        &[
            ("off", "Off"),
            ("local", "LanguageTool on this computer"),
            ("custom", "LanguageTool at an address…"),
        ],
    ) {
        st.grammar_status = None;
        changed = true;
    }
    if s.grammar == "custom" {
        row(ui, p, "Server address", |ui| {
            let r = widgets::text_input(ui, p, &mut s.grammar_url, "", true);
            if r.lost_focus() {
                s.grammar_url = s.grammar_url.trim().to_owned();
                changed = true;
            }
        });
    }
    if s.grammar != "off" {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let to = if s.grammar == "local" {
                "the LanguageTool server on this computer (http://localhost:8081)"
            } else {
                "this server"
            };
            ui.label(
                RichText::new(format!("Your text goes to {to} and nowhere else. "))
                    .size(12.5)
                    .color(p.muted),
            );
            let link = ui.add(
                egui::Label::new(
                    RichText::new("Test the connection")
                        .size(12.5)
                        .underline()
                        .color(p.accent),
                )
                .sense(egui::Sense::click()),
            );
            if link
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                st.grammar_status = Some("checking…".into());
                *out = Some(Out::TestGrammar);
            }
            if let Some(g) = &st.grammar_status {
                ui.label(RichText::new(format!(" — {g}")).size(12.5).color(p.muted));
            }
        });
    }
    section_end(ui, p);

    heading(ui, p, "Files");
    row(ui, p, "Deleted files", |ui| {
        let sel = usize::from(s.trash_mode == "system");
        let r = widgets::segmented(
            ui,
            p,
            &[(None, "Keep for Undo"), (None, "System trash")],
            sel,
            false,
            false,
        );
        if let Some(i) = r {
            s.trash_mode = if i == 0 { "project" } else { "system" }.into();
            changed = true;
        }
    });
    section_end(ui, p);

    heading(ui, p, "TeX");
    let det = tex.map(|t| &t.detection);
    let dist = match det.map(|d| &d.distribution) {
        Some(lemmata_tex::detect::Distribution::TexLive { year }) => format!(
            "TeX Live {}",
            year.map(|y| y.to_string()).unwrap_or_default()
        ),
        Some(lemmata_tex::detect::Distribution::Miktex { version }) => {
            format!("MiKTeX {}", version.clone().unwrap_or_default())
        }
        _ => "No TeX distribution found".into(),
    };
    let dir = det
        .and_then(|d| d.bin_dir.as_ref())
        .map(|b| format!(" in {}", b.display()))
        .unwrap_or_default();
    ui.label(RichText::new(format!("{}{dir}", dist.trim_end())).color(p.muted));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if Button::new("Choose TeX folder…")
            .with_icon(icons::FOLDER_OPEN)
            .show(ui, p)
            .clicked()
        {
            *out = Some(Out::ChooseTex);
        }
        if s.tex_dir.is_some()
            && Button::new("Find automatically")
                .kind(Kind::Ghost)
                .show(ui, p)
                .clicked()
        {
            *out = Some(Out::AutoTex);
        }
    });
    section_end(ui, p);

    heading(ui, p, "Updates");
    // shortcut: "Check now" comes with the self-updater (M9); the setting is kept already.
    changed |= widgets::checkbox(
        ui,
        p,
        &mut s.check_for_updates,
        "Check for updates automatically",
    )
    .clicked();
    if changed && out.is_none() {
        *out = Some(Out::Changed);
    }
}

/// SnippetsDialog.svelte: a trigger, what it inserts and a note, per snippet.
fn snippets(ctx: &egui::Context, p: &Palette, st: &mut Prefs, s: &mut Settings) -> Option<Out> {
    let list = st.snippets.as_mut()?;
    let invalid = |list: &[Snippet], i: usize| {
        let sn = &list[i];
        !crate::editor::complete::valid_trigger(&sn.trigger)
            || sn.body.trim().is_empty()
            || list.iter().position(|o| o.trigger == sn.trigger) != Some(i)
    };
    let problems = (0..list.len()).filter(|&i| invalid(list, i)).count();
    let (mut save, mut cancel) = (false, false);
    let r = widgets::modal_with(
        ctx,
        p,
        "Snippets",
        Some("Type a trigger in the editor and pick it from the completion list."),
        640.0,
        |ui| {
            let code = |t: &str| {
                RichText::new(t)
                    .font(FontId::monospace(12.0))
                    .background_color(p.surface_2)
                    .color(p.ink)
            };
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let t = |s: &str| RichText::new(s).color(p.muted);
                ui.label(t("Use "));
                ui.label(code("${}"));
                ui.label(t(
                    " for places the cursor stops at (Tab jumps to the next one), for example ",
                ));
                ui.label(code("\\begin{theorem}${}\\end{theorem}"));
                ui.label(t(". Triggers are two or more letters, digits or dashes."));
            });
            if list.is_empty() {
                ui.label(RichText::new("No snippets yet.").color(p.muted));
            }
            let max_h = ui.ctx().content_rect().height() * 0.5;
            let w = ui.available_width();
            let mut remove = None;
            let bad: Vec<bool> = (0..list.len()).map(|i| invalid(list, i)).collect();
            ui.allocate_ui(vec2(w, max_h), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(max_h)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 12.0;
                        for (i, sn) in list.iter_mut().enumerate() {
                            egui::Frame::new()
                                .stroke(Stroke::new(1.0, if bad[i] { p.warn } else { p.border }))
                                .corner_radius(theme::R)
                                .inner_margin(Margin::same(10))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.spacing_mut().item_spacing.y = 6.0;
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        ui.allocate_ui(vec2(130.0, 32.0), |ui| {
                                            let r = widgets::text_input(
                                                ui,
                                                p,
                                                &mut sn.trigger,
                                                "trigger",
                                                true,
                                            );
                                            ui.ctx().accesskit_node_builder(r.id, |n| {
                                                n.set_label("Trigger")
                                            });
                                        });
                                        ui.allocate_ui(
                                            vec2(ui.available_width() - 38.0, 32.0),
                                            |ui| {
                                                let r = widgets::text_input(
                                                    ui,
                                                    p,
                                                    &mut sn.description,
                                                    "Description (optional)",
                                                    false,
                                                );
                                                ui.ctx().accesskit_node_builder(r.id, |n| {
                                                    n.set_label("Description")
                                                });
                                            },
                                        );
                                        if Button::icon(icons::TRASH, "Delete snippet")
                                            .show_icon(ui, p)
                                            .clicked()
                                        {
                                            remove = Some(i);
                                        }
                                    });
                                    let r = widgets::text_area(
                                        ui,
                                        p,
                                        &mut sn.body,
                                        "\\begin{figure}[t]\n  ${}\n\\end{figure}",
                                        3,
                                        true,
                                    );
                                    ui.ctx().accesskit_node_builder(r.id, |n| {
                                        n.set_label("Text to insert")
                                    });
                                });
                        }
                    });
            });
            if let Some(i) = remove {
                list.remove(i);
            }
            if Button::new("New snippet")
                .with_icon(icons::PLUS)
                .show(ui, p)
                .clicked()
            {
                list.push(Snippet {
                    trigger: String::new(),
                    body: String::new(),
                    description: String::new(),
                });
            }
        },
        Some(&mut |ui: &mut Ui| {
            if Button::new("Save")
                .kind(Kind::Primary)
                .show(ui, p)
                .clicked()
            {
                save = true;
            }
            if Button::new("Cancel").show(ui, p).clicked() {
                cancel = true;
            }
            if problems > 0 {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let t = format!(
                        "{problems} snippet{} incomplete or repeated and will be skipped.",
                        if problems == 1 { " is" } else { "s are" }
                    );
                    ui.label(RichText::new(t).size(12.5).color(p.warn));
                });
            }
        }),
    );
    if save {
        let list = st.snippets.take().unwrap_or_default();
        s.snippets = (0..list.len())
            .filter(|&i| !invalid(&list, i))
            .map(|i| Snippet {
                trigger: list[i].trigger.trim().to_owned(),
                ..list[i].clone()
            })
            .collect();
        return Some(Out::Changed);
    }
    if r.closed || cancel {
        st.snippets = None;
    }
    None
}
