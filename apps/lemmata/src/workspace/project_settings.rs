//! Project settings (ProjectSettings.svelte): lemmata.toml for the open project, edited on a
//! working copy that is written on Save.

use super::{Action, Tone, Workspace};
use crate::backend::Backend;
use crate::icons;
use crate::store::Settings;
use crate::theme::Palette;
use crate::widgets::{self, Button, Kind, Size};
use eframe::egui::{self, Margin, RichText, Stroke, Ui, vec2};
use lemmata_project::{Bib, Config, Engine, Target};

pub struct ProjectSettings {
    config: Config,
    timeout: String,
    words_open: bool,
    rules_open: bool,
    busy: bool,
}

const ENGINES: [(Engine, &str, &str); 4] = [
    (Engine::Pdflatex, "pdflatex", "pdfLaTeX"),
    (Engine::Xelatex, "xelatex", "XeLaTeX"),
    (Engine::Lualatex, "lualatex", "LuaLaTeX"),
    (Engine::Tectonic, "tectonic", "Tectonic (no TeX install)"),
];

/// Mirrors valid_outdir in lemmata-project: the build folder must stay inside the project.
fn outdir_error(d: &str) -> Option<&'static str> {
    let parts: Vec<&str> = d.split(['/', '\\']).collect();
    let b = d.as_bytes();
    if d.trim().is_empty() {
        Some("Enter a folder name.")
    } else if d.starts_with(['/', '\\'])
        || (b.len() > 1 && b[0].is_ascii_alphabetic() && b[1] == b':')
    {
        Some("Use a folder inside the project, not a full path.")
    } else if parts.contains(&"..") {
        Some("The folder must stay inside the project, so remove the \"..\".")
    } else if !parts.iter().any(|s| !s.is_empty() && *s != ".") {
        Some("Name a folder, not \".\".")
    } else {
        None
    }
}

fn timeout_ok(t: &str) -> Option<u64> {
    t.trim()
        .parse::<u64>()
        .ok()
        .filter(|n| (10..=3600).contains(n))
}

impl Workspace {
    pub fn open_project_settings(&mut self) {
        let mut config = self.info.config.clone();
        config.main.get_or_insert_with(|| self.info.main.clone());
        self.project_settings = Some(ProjectSettings {
            timeout: config.timeout_secs.to_string(),
            config,
            words_open: false,
            rules_open: false,
            busy: false,
        });
    }

    pub(super) fn project_settings_dialog(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        backend: &Backend,
        settings: &mut Settings,
        action: &mut Option<Action>,
    ) {
        let Some(d) = &mut self.project_settings else {
            return;
        };
        let mut candidates = vec![self.info.main.clone()];
        for c in &self.info.main_candidates {
            if !candidates.contains(c) {
                candidates.push(c.clone());
            }
        }
        let trusted = settings.shell_escape_projects.contains(&self.root);
        let was_on = self.info.config.shell_escape;
        let invalid = outdir_error(&d.config.outdir).is_some() || timeout_ok(&d.timeout).is_none();
        let (mut save, mut cancel) = (false, false);
        let r = widgets::modal_with(
            ctx,
            p,
            "Project settings",
            Some("Saved in lemmata.toml in the project folder, so they travel with it."),
            540.0,
            |ui| {
                let max_h = ui.ctx().content_rect().height() * 0.62;
                let w = ui.available_width();
                ui.allocate_ui(vec2(w, max_h), |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(max_h)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.set_width(w - 12.0);
                            ui.spacing_mut().item_spacing.y = 14.0;
                            body(ui, p, d, &candidates, trusted, was_on);
                        });
                });
            },
            Some(&mut |ui: &mut Ui| {
                if Button::new("Save")
                    .kind(Kind::Primary)
                    .enabled(!invalid)
                    .show(ui, p)
                    .clicked()
                {
                    save = true;
                }
                if Button::new("Cancel").show(ui, p).clicked() {
                    cancel = true;
                }
            }),
        );
        if r.closed || cancel {
            self.project_settings = None;
            return;
        }
        if !save || invalid || d.busy {
            return;
        }
        d.busy = true;
        let mut config = d.config.clone();
        config.timeout_secs =
            timeout_ok(&d.timeout).unwrap_or(lemmata_project::DEFAULT_TIMEOUT_SECS);
        if config.shell_escape && !trusted {
            settings.shell_escape_projects.push(self.root.clone());
            *action = Some(Action::Settings);
        }
        let main_changed = config.main.as_deref() != Some(self.info.main.as_str());
        self.save_all_quietly(backend, settings);
        Workspace::job(
            backend,
            move |s| s.set_config(config),
            move |ws, b, r| match r {
                Ok(info) => {
                    ws.project_settings = None;
                    ws.checks.grammar.ignored =
                        info.config.grammar_ignore.iter().cloned().collect();
                    ws.checks.grammar.generation += 1;
                    ws.info = info;
                    ws.tree_rows = None;
                    ws.refresh_lookups(b, false);
                    if main_changed {
                        let main = ws.info.main.clone();
                        ws.open_file(b, &main);
                    }
                    ws.refresh_outline(b);
                    ws.build_soon(false, false);
                }
                Err(e) => {
                    // The dialog stays open, so the choices aren't lost.
                    if let Some(d) = &mut ws.project_settings {
                        d.busy = false;
                    }
                    ws.notice(
                        format!("Could not change the project settings: {e}"),
                        Tone::Error,
                    );
                }
            },
        );
    }
}

fn hint(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).size(12.0).color(p.muted)).wrap());
}

fn err(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(p.error));
}

fn body(
    ui: &mut Ui,
    p: &Palette,
    d: &mut ProjectSettings,
    candidates: &[String],
    trusted: bool,
    was_on: bool,
) {
    let c = &mut d.config;
    let opts: Vec<(&str, &str)> = candidates
        .iter()
        .map(|x| (x.as_str(), x.as_str()))
        .collect();
    widgets::field(ui, p, "Main document", |ui| {
        let mut main = c.main.clone().unwrap_or_default();
        if widgets::select(ui, p, "Main document", &mut main, &opts) {
            c.main = Some(main);
        }
    });
    ui.columns(2, |cols| {
        widgets::field(&mut cols[0], p, "Compiler", |ui| {
            let mut v = ENGINES
                .iter()
                .find(|e| e.0 == c.engine)
                .map_or("pdflatex", |e| e.1)
                .to_owned();
            let o: Vec<(&str, &str)> = ENGINES.iter().map(|e| (e.1, e.2)).collect();
            if widgets::select(ui, p, "Compiler", &mut v, &o) {
                c.engine = ENGINES
                    .iter()
                    .find(|e| e.1 == v)
                    .map_or(Engine::Pdflatex, |e| e.0);
            }
        });
        widgets::field(&mut cols[1], p, "Bibliography", |ui| {
            let mut v = if c.bib == Bib::Bibtex {
                "bibtex"
            } else {
                "biber"
            }
            .to_owned();
            if widgets::select(
                ui,
                p,
                "Bibliography",
                &mut v,
                &[
                    ("biber", "Biber (biblatex)"),
                    ("bibtex", "BibTeX (natbib, .bst)"),
                ],
            ) {
                c.bib = if v == "bibtex" {
                    Bib::Bibtex
                } else {
                    Bib::Biber
                };
            }
        });
    });
    ui.add_space(-8.0);
    hint(
        ui,
        p,
        "pdfLaTeX is fastest. XeLaTeX and LuaLaTeX can use the fonts installed on your computer.",
    );
    if c.engine == Engine::Tectonic {
        hint(
            ui,
            p,
            "Tectonic downloads the packages a document needs on first use, so it needs an internet connection then. It runs BibTeX itself; biblatex with biber is not supported, so choose BibTeX above.",
        );
    }
    ui.columns(2, |cols| {
        widgets::field(&mut cols[0], p, "Stop a build after N seconds", |ui| {
            widgets::text_input(ui, p, &mut d.timeout, "", false);
            if timeout_ok(&d.timeout).is_none() {
                err(ui, p, "Enter a whole number from 10 to 3600.");
            }
        });
        widgets::field(&mut cols[1], p, "Build folder", |ui| {
            widgets::text_input(ui, p, &mut c.outdir, "", true);
            if let Some(e) = outdir_error(&c.outdir) {
                err(ui, p, e);
            }
            hint(
                ui,
                p,
                "Relative to the project, e.g. .lemmata/build. Must stay inside the project.",
            );
        });
    });
    widgets::field(ui, p, "Spell check", |ui| {
        let mut langs: Vec<(String, String)> = vec![(String::new(), "Use my default".into())];
        langs.extend(
            crate::writing::spell::LANGUAGES
                .iter()
                .map(|(a, b)| ((*a).to_owned(), (*b).to_owned())),
        );
        langs.push(("off".into(), "Off".into()));
        let o: Vec<(&str, &str)> = langs
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let mut v = c.spellcheck.clone().unwrap_or_default();
        if widgets::select(ui, p, "Spell check", &mut v, &o) {
            c.spellcheck = (!v.is_empty()).then_some(v);
        }
        if !c.words.is_empty() {
            let n = c.words.len();
            let summary = format!(
                "{n} word{} added to this project's dictionary",
                if n == 1 { "" } else { "s" }
            );
            if disclosure(ui, p, &summary, &mut d.words_open) {
                let mut remove = None;
                chips(ui, p, |ui| {
                    for w in &c.words {
                        chip(ui, p, |ui| {
                            ui.label(RichText::new(w).size(12.5).color(p.ink));
                            if Button::icon(icons::X, &format!("Remove {w}"))
                                .kind(Kind::Ghost)
                                .size(Size::Sm)
                                .icon_size(13.0)
                                .show_icon(ui, p)
                                .on_hover_text("Remove from the dictionary")
                                .clicked()
                            {
                                remove = Some(w.clone());
                            }
                        });
                    }
                });
                if let Some(w) = remove {
                    c.words.retain(|x| *x != w);
                }
            }
        }
        if !c.grammar_ignore.is_empty() {
            let n = c.grammar_ignore.len();
            let summary = format!("{n} grammar rule{} ignored", if n == 1 { "" } else { "s" });
            if disclosure(ui, p, &summary, &mut d.rules_open) {
                let mut remove = None;
                chips(ui, p, |ui| {
                    for r in &c.grammar_ignore {
                        chip(ui, p, |ui| {
                            ui.label(
                                RichText::new(r)
                                    .font(egui::FontId::monospace(12.0))
                                    .color(p.ink),
                            );
                            if Button::new("Check again")
                                .kind(Kind::Ghost)
                                .size(Size::Sm)
                                .show(ui, p)
                                .on_hover_text("Underline mistakes of this kind again")
                                .clicked()
                            {
                                remove = Some(r.clone());
                            }
                        });
                    }
                });
                if let Some(r) = remove {
                    c.grammar_ignore.retain(|x| *x != r);
                }
            }
        }
    });
    widgets::field(ui, p, "Build targets", |ui| {
        hint(
            ui,
            p,
            "Optional. Name more than one main file (a paper and its slides, say) to get a picker in the toolbar.",
        );
        let mut remove = None;
        for (i, t) in c.targets.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let w = (ui.available_width() - 38.0 - 18.0) / 3.3;
                ui.allocate_ui(vec2(w, 32.0), |ui| {
                    let r = widgets::text_input(ui, p, &mut t.name, "Name", false);
                    ui.ctx()
                        .accesskit_node_builder(r.id, |n| n.set_label("Target name"));
                });
                ui.allocate_ui(vec2(w * 1.3, 32.0), |ui| {
                    widgets::select(
                        ui,
                        p,
                        &format!("Main file of {}", t.name),
                        &mut t.main,
                        &opts,
                    )
                });
                ui.allocate_ui(vec2(w, 32.0), |ui| {
                    let mut v = t
                        .engine
                        .and_then(|e| ENGINES.iter().find(|x| x.0 == e))
                        .map_or("", |x| x.1)
                        .to_owned();
                    let o = [
                        ("", "Keep compiler"),
                        ("pdflatex", "pdfLaTeX"),
                        ("xelatex", "XeLaTeX"),
                        ("lualatex", "LuaLaTeX"),
                        ("tectonic", "Tectonic"),
                    ];
                    if widgets::select(ui, p, &format!("Compiler of {}", t.name), &mut v, &o) {
                        t.engine = ENGINES.iter().find(|x| x.1 == v).map(|x| x.0);
                    }
                });
                if Button::icon(icons::TRASH, &format!("Remove {}", t.name))
                    .show_icon(ui, p)
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            c.targets.remove(i);
        }
        if Button::new("Add target")
            .with_icon(icons::PLUS)
            .size(Size::Sm)
            .show(ui, p)
            .clicked()
        {
            let main = c.main.clone().unwrap_or_else(|| candidates[0].clone());
            c.targets.push(Target {
                name: format!("Target {}", c.targets.len() + 1),
                main,
                engine: None,
            });
        }
    });
    widgets::checkbox(
        ui,
        p,
        &mut c.shell_escape,
        "Allow the document to run programs (shell escape)",
    );
    if c.shell_escape && was_on && !trusted {
        ui.add_space(-8.0);
        hint(
            ui,
            p,
            "The project's lemmata.toml turns this on, but it isn't allowed on this computer yet, so builds run without it. Save to allow it.",
        );
    }
    if c.shell_escape {
        egui::Frame::new().fill(p.warn_soft).corner_radius(crate::theme::R).inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                icons::show(ui, icons::TRIANGLE_ALERT, 16.0, p.warn);
                ui.add(
                    egui::Label::new(
                        RichText::new("Only for projects you trust: with shell escape, the document can run any command on your computer. Packages like minted and svg need it.")
                            .color(p.ink_2),
                    )
                    .wrap(),
                );
            });
        });
    }
}

/// `<details>`: a muted summary that opens its contents. Returns whether it is open.
fn disclosure(ui: &mut Ui, p: &Palette, summary: &str, open: &mut bool) -> bool {
    let arrow = if *open { "▾" } else { "▸" };
    let r = ui.add(
        egui::Label::new(
            RichText::new(format!("{arrow} {summary}"))
                .size(12.0)
                .color(p.muted),
        )
        .sense(egui::Sense::click()),
    );
    if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        *open = !*open;
    }
    *open
}

fn chips(ui: &mut Ui, _p: &Palette, add: impl FnOnce(&mut Ui)) {
    let w = ui.available_width();
    ui.allocate_ui(vec2(w, 140.0), |ui| {
        egui::ScrollArea::vertical()
            .max_height(140.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                    add(ui);
                });
            });
    });
}

fn chip(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(crate::theme::R_SM)
        .inner_margin(Margin {
            left: 8,
            right: 0,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                add(ui);
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_build_folder_stays_inside_the_project() {
        assert_eq!(outdir_error(".lemmata/build"), None);
        assert!(outdir_error("").is_some());
        assert!(outdir_error("/tmp/x").is_some() && outdir_error("C:\\x").is_some());
        assert!(outdir_error("a/../../b").is_some());
        assert!(outdir_error("./.").is_some());
        assert_eq!(timeout_ok("300"), Some(300));
        assert_eq!(timeout_ok("9"), None);
        assert_eq!(timeout_ok("x"), None);
    }
}
