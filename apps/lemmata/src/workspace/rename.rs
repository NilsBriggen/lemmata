//! Rename a label or citation key everywhere (RenameSymbol.svelte): the `\label` or .bib entry and
//! every `\ref` or `\cite` that uses it. F2 opens it on the name under the cursor (symbolAt.ts).

use super::{Tone, Workspace};
use crate::backend::Backend;
use crate::store::Settings;
use crate::theme::Palette;
use crate::widgets::{self, Button, Kind};
use eframe::egui::{self, FontId, Margin, RichText, Stroke, Ui};
use lemmata_project::symbols::{Kind as SymbolKind, Occurrence};
use regex::Regex;
use std::sync::LazyLock;

static LABEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\label\s*\{([^{}]*)\}").unwrap());
static REF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\([a-zA-Z]*ref)\*?(?:\[[^\]]*\])?\s*\{([^{}]*)\}").unwrap());
static CITE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\[a-zA-Z]*cite[a-zA-Z]*\*?(?:\[[^\]]*\]){0,2}\s*\{([^{}]*)\}").unwrap()
});
static BIB: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*@[A-Za-z]+\s*[{(]\s*([^,\s{}()]+)\s*,").unwrap());

/// The label or citation key at character `col` of `line`, if any.
pub fn symbol_at(line: &str, col: usize, is_bib: bool) -> Option<(SymbolKind, String)> {
    // Regex offsets are bytes; the cursor is in characters.
    let col = line.char_indices().nth(col).map_or(line.len(), |(b, _)| b);
    if is_bib {
        let c = BIB.captures(line)?.get(1)?;
        return (col >= c.start() && col <= c.end())
            .then(|| (SymbolKind::Cite, c.as_str().to_owned()));
    }
    let before = &line[..col];
    if before
        .char_indices()
        .any(|(i, c)| c == '%' && !before[..i].ends_with('\\'))
    {
        return None;
    }
    let scan =
        |re: &Regex, kind: SymbolKind, group: usize, skip: &dyn Fn(&regex::Captures) -> bool| {
            for m in re.captures_iter(line) {
                if skip(&m) {
                    continue;
                }
                let g = m.get(group)?;
                if col < g.start() || col > g.end() {
                    continue;
                }
                // The name of the comma-separated part that holds the cursor.
                let mut at = g.start();
                for part in g.as_str().split(',') {
                    let lead = part.len() - part.trim_start().len();
                    let name = part.trim();
                    let start = at + lead;
                    if !name.is_empty() && col >= start && col <= start + name.len() {
                        return Some((kind, name.to_owned()));
                    }
                    at += part.len() + 1;
                }
            }
            None
        };
    scan(&LABEL, SymbolKind::Label, 1, &|_| false)
        .or_else(|| {
            scan(&REF, SymbolKind::Label, 2, &|m| {
                matches!(&m[1], "href" | "hyperref")
            })
        })
        .or_else(|| scan(&CITE, SymbolKind::Cite, 1, &|_| false))
}

/// Whether BibTeX and LaTeX accept the name.
fn valid(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|c| c.is_whitespace() || ",{}()\\%#~".contains(c))
}

pub struct RenameSymbol {
    kind: SymbolKind,
    old: String,
    new: String,
    /// The uses of `old` as last looked up, for the name they were looked up for.
    uses: Vec<Occurrence>,
    looked_up: Option<(SymbolKind, String)>,
    known: Vec<String>,
    error: Option<String>,
    busy: bool,
    focus: bool,
}

impl Workspace {
    /// F2: the dialog, filled in with the label or key under the cursor.
    pub fn open_rename(&mut self, backend: &Backend) {
        let at = self.active.clone().and_then(|path| {
            let ed = self.editors.get(&path)?;
            let head = ed.doc.selection.main().head;
            let n = ed.doc.line_of(head);
            symbol_at(
                &ed.doc.line(n),
                head - ed.doc.line_start(n),
                path.ends_with(".bib"),
            )
        });
        let (kind, name) = at.unwrap_or((SymbolKind::Label, String::new()));
        self.rename = Some(RenameSymbol {
            kind,
            old: name.clone(),
            new: name,
            uses: Vec::new(),
            looked_up: None,
            known: Vec::new(),
            error: None,
            busy: false,
            focus: true,
        });
        Workspace::job(
            backend,
            |s| s.symbol_defs(),
            |ws, _, r| {
                if let (Some(d), Ok(defs)) = (&mut ws.rename, r) {
                    let mut names: Vec<String> = defs
                        .into_iter()
                        .map(|o| format!("{}\u{0}{}", o.kind == SymbolKind::Cite, o.name))
                        .collect();
                    names.sort();
                    names.dedup();
                    d.known = names;
                }
            },
        );
    }

    fn lookup_uses(&mut self, backend: &Backend) {
        let Some(d) = &mut self.rename else { return };
        let key = (d.kind, d.old.trim().to_owned());
        if d.looked_up.as_ref() == Some(&key) {
            return;
        }
        d.looked_up = Some(key.clone());
        if key.1.is_empty() {
            d.uses.clear();
            return;
        }
        Workspace::job(
            backend,
            move |s| s.symbol_uses(key.0, key.1.clone()).map(|u| (key, u)),
            |ws, _, r| {
                let Some(d) = &mut ws.rename else { return };
                match r {
                    Ok((key, u)) if d.looked_up.as_ref() == Some(&key) => d.uses = u,
                    Ok(_) => {}
                    Err(_) => d.uses.clear(),
                }
            },
        );
    }

    fn apply_rename(&mut self, backend: &Backend, settings: &Settings) {
        let Some(d) = &mut self.rename else { return };
        d.busy = true;
        d.error = None;
        let (kind, old, new) = (d.kind, d.old.trim().to_owned(), d.new.trim().to_owned());
        self.save_all_quietly(backend, settings);
        self.after_writes.push(Box::new(move |_, b| {
            Workspace::job(
                b,
                move |s| {
                    let _ = s.history_snapshot("Before rename".into());
                    s.rename_symbol(kind, old.clone(), new.clone())
                        .map(|files| (files, old, new))
                },
                |ws, b, r| match r {
                    Ok((files, old, new)) => {
                        ws.rename = None;
                        for f in &files {
                            ws.reload(b, f);
                        }
                        let n = files.len();
                        ws.notice(
                            format!(
                                "Renamed {old} to {new} in {n} file{}.",
                                if n == 1 { "" } else { "s" }
                            ),
                            Tone::Ok,
                        );
                        ws.refresh_lookups(b, false);
                        ws.build_soon(false, false);
                    }
                    Err(e) => {
                        if let Some(d) = &mut ws.rename {
                            d.error = Some(e);
                            d.busy = false;
                        }
                    }
                },
            );
        }));
    }

    pub(super) fn rename_dialog(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        backend: &Backend,
        settings: &Settings,
    ) {
        if self.rename.is_none() {
            return;
        }
        self.lookup_uses(backend);
        let history_on = self.history.on;
        let Some(d) = &mut self.rename else { return };
        let files = {
            let mut f: Vec<&str> = d.uses.iter().map(|u| u.file.as_str()).collect();
            f.sort();
            f.dedup();
            f.len()
        };
        let ok = valid(d.new.trim()) && d.new != d.old && !d.uses.is_empty() && !d.busy;
        let (mut apply, mut enter, mut cancel) = (false, false, false);
        let subtitle = if history_on {
            "Changes every use in the project. A snapshot is saved first."
        } else {
            "Changes every use in the project. History is off (git isn't installed), so this can't be undone."
        };
        let r = widgets::modal_with(
            ctx,
            p,
            "Rename label or citation key",
            Some(subtitle),
            560.0,
            |ui| {
                ui.spacing_mut().item_spacing.y = 12.0;
                let sel = usize::from(d.kind == SymbolKind::Cite);
                if let Some(i) = widgets::segmented(
                    ui,
                    p,
                    &[(None, "Label"), (None, "Citation key")],
                    sel,
                    false,
                    false,
                ) {
                    d.kind = if i == 0 {
                        SymbolKind::Label
                    } else {
                        SymbolKind::Cite
                    };
                }
                widgets::field(ui, p, "Current name", |ui| {
                    widgets::text_input(ui, p, &mut d.old, "", true)
                });
                // The datalist: names of this kind starting with what is typed.
                let cite = d.kind == SymbolKind::Cite;
                let typed = d.old.trim().to_owned();
                let hints: Vec<&str> = d
                    .known
                    .iter()
                    .filter_map(|k| k.split_once('\u{0}'))
                    .filter(|(c, n)| (*c == "true") == cite && n.starts_with(&typed) && *n != typed)
                    .map(|(_, n)| n)
                    .take(8)
                    .collect();
                if !typed.is_empty() && !hints.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for h in hints {
                            if Button::new(h)
                                .kind(Kind::Ghost)
                                .size(widgets::Size::Sm)
                                .show(ui, p)
                                .clicked()
                            {
                                d.old = h.to_owned();
                                d.new = h.to_owned();
                            }
                        }
                    });
                }
                let r = widgets::field(ui, p, "New name", |ui| {
                    widgets::text_input(ui, p, &mut d.new, "", true)
                });
                if std::mem::take(&mut d.focus) {
                    r.request_focus();
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    enter = true;
                }
                let old = d.old.trim();
                if !old.is_empty() {
                    if d.uses.is_empty() {
                        ui.label(
                            RichText::new(format!("Nothing in the project uses \"{old}\"."))
                                .color(p.muted),
                        );
                    } else {
                        let n = d.uses.len();
                        ui.label(
                            RichText::new(format!(
                                "{n} place{} in {files} file{}:",
                                if n == 1 { "" } else { "s" },
                                if files == 1 { "" } else { "s" }
                            ))
                            .color(p.muted),
                        );
                        refs(ui, p, &d.uses);
                    }
                }
                if let Some(e) = &d.error {
                    ui.label(RichText::new(e).color(p.error));
                }
            },
            Some(&mut |ui: &mut Ui| {
                if Button::new("Rename")
                    .kind(Kind::Primary)
                    .enabled(ok)
                    .show(ui, p)
                    .clicked()
                {
                    apply = true;
                }
                if Button::new("Cancel").show(ui, p).clicked() {
                    cancel = true;
                }
            }),
        );
        if r.closed || cancel {
            self.rename = None;
        } else if (apply || enter) && ok {
            self.apply_rename(backend, settings);
        }
    }
}

/// `.refs`: the places, mono 11.5 on --surface-2, at most 190 px.
fn refs(ui: &mut Ui, p: &Palette, uses: &[Occurrence]) {
    egui::Frame::new()
        .fill(p.surface_2)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(crate::theme::R)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let w = ui.available_width();
            ui.allocate_ui(egui::vec2(w, 190.0), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(174.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        let font = FontId::monospace(11.5);
                        for u in uses.iter().take(60) {
                            let job = widgets::Rich::new()
                                .add(&format!("{}:{}", u.file, u.line), font.clone(), p.muted)
                                .add(&format!(" {}", u.text), font.clone(), p.ink);
                            ui.add(egui::Label::new(job.0).truncate());
                        }
                        if uses.len() > 60 {
                            ui.label(
                                RichText::new(format!("… and {} more", uses.len() - 60))
                                    .font(font)
                                    .color(p.ink),
                            );
                        }
                    });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    // symbolAt.test.ts
    #[test]
    fn finds_the_name_under_the_cursor() {
        let at = |line: &str, needle: &str, bib: bool| {
            let col = line[..line.find(needle).unwrap()].chars().count() + 1;
            symbol_at(line, col, bib).map(|(k, n)| (k == SymbolKind::Cite, n))
        };
        assert_eq!(
            at("\\section{A}\\label{sec:a}", "sec:a", false),
            Some((false, "sec:a".into()))
        );
        assert_eq!(
            at("see \\cref{sec:a, eq:b}", "eq:b", false),
            Some((false, "eq:b".into()))
        );
        assert_eq!(
            at("by \\citep[p.~2]{knuth84,lamport94}", "lamport", false),
            Some((true, "lamport94".into()))
        );
        assert_eq!(at("\\href{http://x.org}{text}", "http", false), None);
        assert_eq!(at("% \\ref{sec:a}", "sec:a", false), None);
        assert_eq!(at("\\ref{sec:a}", "ref", false), None);
        assert_eq!(
            at("@article{knuth84,", "knuth", true),
            Some((true, "knuth84".into()))
        );
        assert_eq!(at("@article{knuth84,", "article", true), None);
        assert!(valid("sec:a") && !valid("a b") && !valid(""));
    }
}
