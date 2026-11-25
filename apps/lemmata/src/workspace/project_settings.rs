//! Project settings (`lemmata.toml` for the open project), edited on a working copy inside the
//! unified Settings window and written only on Save.

use super::{Tone, Workspace};
use crate::backend::Backend;
use crate::icons;
use crate::store::Settings;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind, Size};
use eframe::egui::{self, FontId, Margin, RichText, Stroke, Ui, vec2};
use lemmata_project::{Bib, Config, Engine, Target};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ProjectSection {
    #[default]
    Build,
    Trust,
}

pub struct ProjectSettings {
    pub config: Config,
    pub timeout: String,
    pub words_open: bool,
    pub rules_open: bool,
    /// The chapters the main file `\include`s, for "Build only these chapters".
    pub chapters: Vec<String>,
    /// "Trust this project", kept on this computer rather than in lemmata.toml.
    pub trusted: bool,
    pub(crate) busy: bool,
    pub initial_section: ProjectSection,
    /// One-shot signal so the unified Settings window focuses Project scope when reopened.
    pub focus_project_scope: bool,
    orig_config: Config,
    orig_timeout: String,
    orig_trusted: bool,
}

impl ProjectSettings {
    pub fn is_valid(&self) -> bool {
        outdir_error(&self.config.outdir).is_none() && timeout_ok(&self.timeout).is_some()
    }

    pub fn is_dirty(&self) -> bool {
        self.config != self.orig_config
            || self.timeout != self.orig_timeout
            || self.trusted != self.orig_trusted
    }
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

/// What the build accepts: 0 (no limit, a day for a trusted project) or up to a day.
fn timeout_ok(t: &str) -> Option<u64> {
    t.trim()
        .parse::<u64>()
        .ok()
        .filter(|n| *n == 0 || (10..=86400).contains(n))
}

fn grants_shell_escape_trust(config: &Config, trusted: bool, was_on: bool) -> bool {
    config.shell_escape && !trusted && !was_on
}

impl Workspace {
    pub fn open_project_settings(&mut self) {
        self.open_project_settings_on(ProjectSection::Build);
    }

    pub fn open_project_trust_settings(&mut self) {
        self.open_project_settings_on(ProjectSection::Trust);
    }

    pub fn open_project_settings_on(&mut self, section: ProjectSection) {
        let mut config = self.info.config.clone();
        config.main.get_or_insert_with(|| self.info.main.clone());
        let timeout = config.timeout_secs.to_string();
        let chapters = lemmata_env::fs::read_text(self.info.root.join(&self.info.main))
            .map(|t| lemmata_project::included_chapters(&t))
            .unwrap_or_default();
        self.project_settings = Some(ProjectSettings {
            chapters,
            orig_config: config.clone(),
            orig_timeout: timeout.clone(),
            orig_trusted: self.trusted,
            timeout,
            config,
            words_open: false,
            rules_open: false,
            trusted: self.trusted,
            busy: false,
            initial_section: section,
            focus_project_scope: true,
        });
    }

    pub fn main_candidates(&self) -> Vec<String> {
        let mut candidates = vec![self.info.main.clone()];
        for c in &self.info.main_candidates {
            if !candidates.contains(c) {
                candidates.push(c.clone());
            }
        }
        candidates
    }

    /// Commits the open project's Settings working copy to `lemmata.toml` (and trust store).
    pub fn save_project_settings(&mut self, backend: &Backend, settings: &mut Settings) {
        let Some(d) = &mut self.project_settings else {
            return;
        };
        if !d.is_valid() || d.busy {
            return;
        }
        let trusted = self.trusted;
        let was_on = self.info.config.shell_escape;
        d.busy = true;
        let mut config = d.config.clone();
        config.timeout_secs =
            timeout_ok(&d.timeout).unwrap_or(lemmata_project::DEFAULT_TIMEOUT_SECS);
        // Turning shell escape on here is the user allowing programs: that trusts the project.
        // Trust changes are applied only once saves and `set_config` succeed so a failed save
        // never silently promotes a restricted project.
        let trust = d.trusted || grants_shell_escape_trust(&config, trusted, was_on);
        let trust_changed = trust != trusted;
        let main_changed = config.main.as_deref() != Some(self.info.main.as_str());
        self.after_saving(backend, settings, move |_, b| {
            Workspace::job(
                b,
                move |s| s.set_config(config),
                move |ws, b, r| match r {
                    Ok(info) => {
                        ws.project_settings = None;
                        ws.project_settings_saved = true;
                        ws.checks.grammar.ignored =
                            info.config.grammar_ignore.iter().cloned().collect();
                        ws.checks.grammar.generation += 1;
                        ws.info = info;
                        ws.tree_rows = None;
                        ws.refresh_analysis(b);
                        if main_changed {
                            let main = ws.info.main.clone();
                            ws.open_file(b, &main);
                        }
                        if trust_changed {
                            // `Workspace::show` applies `set_trusted` (which also triggers a rebuild).
                            ws.pending_trust = Some(trust);
                        } else {
                            ws.build_soon(false, false);
                        }
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
        });
    }
}

fn hint(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).size(12.0).color(p.muted)).wrap());
}

fn err(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(p.error));
}

pub fn show_build_section(
    ui: &mut Ui,
    p: &Palette,
    d: &mut ProjectSettings,
    candidates: &[String],
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
    if c.engine == Engine::Tectonic || c.targets.iter().any(|t| t.engine == Some(Engine::Tectonic))
    {
        c.bib = Bib::Bibtex;
    }
    ui.add_space(-6.0);
    hint(
        ui,
        p,
        "pdfLaTeX is fastest. XeLaTeX and LuaLaTeX can use the fonts installed on your computer.",
    );
    if c.engine == Engine::Tectonic {
        hint(
            ui,
            p,
            "Tectonic downloads the packages a document needs on first use, so it needs an internet connection then. It runs BibTeX itself; biblatex with biber is not supported, so BibTeX is used.",
        );
    }
    ui.columns(2, |cols| {
        widgets::field(&mut cols[0], p, "Stop a build after N seconds", |ui| {
            widgets::text_input(ui, p, &mut d.timeout, "", false);
            if timeout_ok(&d.timeout).is_none() {
                err(
                    ui,
                    p,
                    "Enter a whole number of seconds, 10 to 86400, or 0 for no limit.",
                );
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
                            ui.label(RichText::new(r).font(FontId::monospace(12.0)).color(p.ink));
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
    if !d.chapters.is_empty() {
        widgets::field(ui, p, "Build only these chapters", |ui| {
            hint(
                ui,
                p,
                "Optional. Tick the chapters you are working on to build just those (\\includeonly); the others keep their numbers. Tick none to build everything.",
            );
            for ch in &d.chapters {
                let mut on = c.include_only.contains(ch);
                if widgets::checkbox(ui, p, &mut on, ch).clicked() {
                    c.include_only.retain(|x| x != ch);
                    if on {
                        c.include_only.push(ch.clone());
                    }
                }
            }
        });
    }
}

pub fn show_trust_section(
    ui: &mut Ui,
    p: &Palette,
    d: &mut ProjectSettings,
    trusted: bool,
    was_on: bool,
) {
    let c = &mut d.config;
    // Trust toggle row.
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 48.0).max(160.0);
        ui.vertical(|ui| {
            ui.set_width(w);
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.label(
                RichText::new("Trust this project")
                    .font(FontId::new(13.5, theme::font::sans_500()))
                    .color(p.ink),
            );
            hint(
                ui,
                p,
                if d.trusted {
                    "It may run programs (its latexmkrc, Git hooks). Kept on this computer only. Turn off if you don't know where the project comes from."
                } else {
                    "Restricted: it builds and shows its PDF, but nothing of its own runs, and the Git panel is off."
                },
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            widgets::switch(
                ui,
                p,
                &mut d.trusted,
                "Trust this project: it may run programs (its latexmkrc, Git hooks)",
            );
        });
    });
    ui.add_space(6.0);
    let y = ui.cursor().top();
    ui.painter()
        .hline(ui.max_rect().x_range(), y, Stroke::new(1.0, p.border));
    ui.add_space(10.0);

    // Shell escape toggle row.
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 48.0).max(160.0);
        ui.vertical(|ui| {
            ui.set_width(w);
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.label(
                RichText::new("Shell escape")
                    .font(FontId::new(13.5, theme::font::sans_500()))
                    .color(p.ink),
            );
            hint(
                ui,
                p,
                "Allow the document to run external programs during compilation (needed by packages like minted and svg).",
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            widgets::switch(
                ui,
                p,
                &mut c.shell_escape,
                "Allow the document to run programs (shell escape)",
            );
        });
    });
    if c.shell_escape && was_on && !trusted {
        hint(
            ui,
            p,
            "The project's lemmata.toml turns this on, but the project isn't trusted, so builds run without it. Turn on \"Trust this project\" above to use it.",
        );
    }
    if c.shell_escape {
        egui::Frame::new()
            .fill(p.warn_soft)
            .stroke(Stroke::new(1.0, p.warn.gamma_multiply(0.35)))
            .corner_radius(theme::R_MD)
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    icons::show(ui, icons::TRIANGLE_ALERT, 16.0, p.warn);
                    ui.add(
                        egui::Label::new(
                            RichText::new("Only for projects you trust: with shell escape, the document can run any command on your computer. Packages like minted and svg need it.")
                                .size(12.5)
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
    fn a_preexisting_shell_escape_config_does_not_grant_trust() {
        let config = Config {
            shell_escape: true,
            ..Config::default()
        };
        assert!(!grants_shell_escape_trust(&config, false, true));
        assert!(grants_shell_escape_trust(&config, false, false));
        assert!(!grants_shell_escape_trust(&config, true, false));
    }

    #[test]
    fn the_build_folder_stays_inside_the_project() {
        assert_eq!(outdir_error(".lemmata/build"), None);
        assert!(outdir_error("").is_some());
        assert!(outdir_error("/tmp/x").is_some() && outdir_error("C:\\x").is_some());
        assert!(outdir_error("a/../../b").is_some());
        assert!(outdir_error("./.").is_some());
        assert_eq!(timeout_ok("300"), Some(300));
        assert_eq!(timeout_ok("9"), None);
        assert_eq!((timeout_ok("0"), timeout_ok("7200")), (Some(0), Some(7200)));
        assert_eq!(timeout_ok("86401"), None);
        assert_eq!(timeout_ok("x"), None);
    }
}
