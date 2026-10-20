//! The command palette (Ctrl+Shift+P) and the lists that share its look (Palette.svelte): quick
//! open (Ctrl+P), wrap in an environment, go to a section, go to a label or citation. The main
//! menu runs the same commands, so a command has one name wherever it appears.

use super::{Action, Tone, Workspace, panels::Panel};
use crate::backend::{Backend, Msg, Save};
use crate::shortcuts::{display, keys_of};
use crate::store::Settings;
use crate::theme::{self, Palette as Colors};
use crate::workspace::tree;
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Key, Margin, Modifiers, Rect, RichText, Sense, Stroke,
    pos2, vec2,
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Commands,
    Files,
    Environments,
    Headings,
    Symbols,
}

/// What picking an item does.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Command(&'static str),
    Open(String),
    Goto(String, usize),
    Wrap(&'static str),
}

#[derive(Clone)]
pub struct Item {
    pub label: String,
    /// Shown on the right: a shortcut or a folder.
    pub detail: String,
    pub cmd: Cmd,
}

pub struct Palette {
    pub kind: Kind,
    items: Vec<Item>,
    query: String,
    selected: usize,
    scroll: bool,
    focus: bool,
}

/// fuzzy.ts: every query character must appear in order; consecutive and word-start hits score
/// higher, and shorter texts win ties.
pub fn score(query: &str, text: &str) -> Option<f32> {
    let q: String = query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if q.is_empty() {
        return Some(0.0);
    }
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut score, mut ti, mut last) = (0.0f32, 0usize, -2isize);
    for ch in q.chars() {
        let found = (ti..t.len()).find(|&i| t[i] == ch)?;
        let word_start = found == 0 || " \t/_.-".contains(t[found - 1]);
        score +=
            if found as isize == last + 1 { 5.0 } else { 1.0 } + if word_start { 3.0 } else { 0.0 };
        last = found as isize;
        ti = found + 1;
    }
    Some(score - t.len() as f32 * 0.01)
}

/// The command, its label and its shortcut (if it has one), in the palette's order.
const COMMANDS: &[(&str, &str, &str)] = &[
    ("recompile", "Recompile", "recompile"),
    ("open-file", "Open file…", "quick-open"),
    ("search", "Search in project", "search"),
    ("cite", "Insert citation", "cite"),
    ("figure", "Insert figure…", ""),
    ("table", "Insert table…", ""),
    ("snippets", "Edit snippets…", ""),
    ("symbols", "Insert symbol…", "symbols"),
    ("wrap-env", "Wrap selection in an environment…", "wrap-env"),
    ("words", "Word count", ""),
    ("shortcuts", "Keyboard shortcuts", "shortcuts"),
    ("snapshot", "Save a snapshot", "snapshot"),
    ("history", "History", ""),
    ("comment", "Add a comment", "add-comment"),
    ("comments", "Comments", ""),
    ("bib-manager", "Bibliography manager…", ""),
    ("git", "Git", ""),
    ("todos", "TODOs", ""),
    ("format", "Format document", ""),
    ("reindent", "Reindent lines", ""),
    ("sync", "Show this line in the PDF", "sync"),
    ("focus", "Focus mode", "focus"),
    ("new-file", "New file…", ""),
    ("goto-heading", "Go to a section…", "goto-heading"),
    ("goto-symbol", "Go to a label or citation…", ""),
    ("rename-symbol", "Rename label or citation key…", ""),
    ("reopen-tab", "Reopen the last closed tab", "reopen-tab"),
    ("reveal", "Show the open file in the file tree", ""),
    ("unused", "Find unused files…", ""),
    ("undo-files", "Undo the last file operation", ""),
    ("deleted", "Recently deleted files…", ""),
    ("zotero-cite", "Cite from Zotero…", ""),
    ("zotero-sync", "Sync zotero.bib from Zotero", ""),
    ("export-pdf", "Export PDF…", ""),
    ("present", "Present full screen", ""),
    ("open-external", "Open the PDF in an external viewer", ""),
    ("save-template", "Save as template…", ""),
    ("export-zip", "Export project (.zip)…", ""),
    ("export-arxiv", "Export for arXiv (.zip)…", ""),
    (
        "export-arxiv-flat",
        "Export for arXiv as one .tex file (.zip)…",
        "",
    ),
    ("project-settings", "Project settings", ""),
    ("preferences", "Preferences", ""),
    ("guide", "User guide", ""),
    ("report", "Report a problem…", ""),
    ("close", "All projects", ""),
];

/// A command's name, the same in the palette and the menus.
pub fn label(id: &str, focus_mode: bool) -> &'static str {
    if id == "focus" && focus_mode {
        return "Leave focus mode";
    }
    COMMANDS.iter().find(|c| c.0 == id).map_or("", |c| c.1)
}

pub const GUIDE: &str = "https://github.com/NilsBriggen/lemmata/blob/main/docs/guide.md";

impl Palette {
    fn new(kind: Kind, items: Vec<Item>) -> Self {
        Palette {
            kind,
            items,
            query: String::new(),
            selected: 0,
            scroll: false,
            focus: true,
        }
    }

    fn placeholder(&self) -> &'static str {
        match self.kind {
            Kind::Files => "Open a file by name",
            Kind::Environments => "Wrap the selection in…",
            Kind::Headings => "Go to a section",
            Kind::Symbols => "Go to a label or citation key",
            Kind::Commands => "Type a command",
        }
    }

    fn results(&self) -> Vec<&Item> {
        let mut scored: Vec<(f32, &Item)> = self
            .items
            .iter()
            .filter_map(|i| {
                score(&self.query, &format!("{} {}", i.label, i.detail)).map(|s| (s, i))
            })
            .collect();
        // Stable: equal scores keep the list's order.
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        scored.into_iter().take(100).map(|(_, i)| i).collect()
    }

    /// None while open; Some(None) when closed, Some(Some(cmd)) when an item was picked.
    fn show(&mut self, ctx: &egui::Context, p: &Colors) -> Option<Option<Cmd>> {
        let screen = ctx.content_rect();
        let n = self.results().len();
        let mut picked: Option<Option<Cmd>> = None;
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                self.selected = (self.selected + 1).min(n.saturating_sub(1));
                self.scroll = true;
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                self.selected = self.selected.saturating_sub(1);
                self.scroll = true;
            }
            if i.consume_key(Modifiers::NONE, Key::Escape) {
                picked = Some(None);
            }
            // Options aren't tab stops: Tab stays in the input.
            i.consume_key(Modifiers::NONE, Key::Tab);
        });
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
            picked = Some(self.results().get(self.selected).map(|i| i.cmd.clone()));
        }
        // .backdrop: rgba(12, 17, 22, .32), the panel 12vh from the top.
        let backdrop = egui::Area::new(egui::Id::unique("palette-backdrop"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, r) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(rect, 0, Color32::from_rgba_unmultiplied(12, 17, 22, 82));
                r
            });
        let width = 620.0f32.min(screen.width() - 48.0);
        let panel = egui::Area::new(egui::Id::unique("palette"))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::CENTER_TOP)
            .fixed_pos(pos2(
                screen.center().x,
                screen.top() + screen.height() * 0.12,
            ))
            .show(ctx, |ui| {
                ui.set_width(width);
                egui::Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(12)
                    .shadow(p.shadow_lg)
                    .show(ui, |ui| {
                        ui.set_width(width - 2.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let before = self.query.clone();
                        let placeholder = self.placeholder();
                        let r = egui::TextEdit::singleline(&mut self.query)
                            .hint_text(RichText::new(placeholder).size(15.0).color(p.faint))
                            .font(FontId::proportional(15.0))
                            .text_color(p.ink)
                            .frame(egui::Frame::NONE.inner_margin(Margin::symmetric(16, 14)))
                            .desired_width(width - 2.0)
                            .show(ui)
                            .response
                            .response;
                        ui.ctx()
                            .accesskit_node_builder(r.id, |n| n.set_label(placeholder));
                        if std::mem::take(&mut self.focus) || !r.has_focus() {
                            r.request_focus();
                        }
                        if self.query != before {
                            self.selected = 0;
                        }
                        ui.painter().hline(
                            r.rect.x_range(),
                            r.rect.bottom(),
                            Stroke::new(1.0, p.border),
                        );
                        let max_h = screen.height() * 0.52;
                        let results: Vec<Item> = self.results().into_iter().cloned().collect();
                        let w = ui.available_width();
                        ui.allocate_ui(vec2(w, max_h + 12.0), |ui| {
                            egui::Frame::new()
                                .inner_margin(Margin::same(6))
                                .show(ui, |ui| {
                                    egui::ScrollArea::vertical()
                                        .max_height(max_h)
                                        .auto_shrink([false, true])
                                        .show(ui, |ui| {
                                            ui.spacing_mut().item_spacing.y = 0.0;
                                            if results.is_empty() {
                                                egui::Frame::new()
                                                    .inner_margin(Margin::same(12))
                                                    .show(ui, |ui| {
                                                        ui.label(
                                                            RichText::new("Nothing matches.")
                                                                .color(p.muted),
                                                        );
                                                    });
                                            }
                                            for (i, item) in results.iter().enumerate() {
                                                let (rect, row) = ui.allocate_exact_size(
                                                    vec2(ui.available_width(), 35.0),
                                                    Sense::click(),
                                                );
                                                row.widget_info(|| {
                                                    egui::WidgetInfo::selected(
                                                        egui::Role::ListBoxOption,
                                                        true,
                                                        i == self.selected,
                                                        &item.label,
                                                    )
                                                });
                                                if row.hovered()
                                                    && ui.input(|i| {
                                                        i.pointer.delta() != egui::Vec2::ZERO
                                                    })
                                                {
                                                    self.selected = i;
                                                }
                                                if i == self.selected {
                                                    ui.painter().rect_filled(
                                                        rect,
                                                        CornerRadius::same(theme::R),
                                                        p.accent_soft,
                                                    );
                                                    if std::mem::take(&mut self.scroll) {
                                                        ui.scroll_to_rect(rect, None);
                                                    }
                                                }
                                                let inner = rect.shrink2(vec2(10.0, 0.0));
                                                let detail = ui.painter().layout_no_wrap(
                                                    item.detail.clone(),
                                                    FontId::monospace(11.0),
                                                    p.muted,
                                                );
                                                let dw = detail.size().x.min(inner.width() * 0.5);
                                                let label_clip = Rect::from_min_max(
                                                    inner.min,
                                                    pos2(inner.right() - dw - 12.0, inner.bottom()),
                                                );
                                                ui.painter().with_clip_rect(label_clip).text(
                                                    pos2(inner.left(), inner.center().y),
                                                    egui::Align2::LEFT_CENTER,
                                                    &item.label,
                                                    FontId::proportional(13.0),
                                                    p.ink,
                                                );
                                                if !item.detail.is_empty() {
                                                    let clip = Rect::from_min_max(
                                                        pos2(inner.right() - dw, inner.top()),
                                                        inner.max,
                                                    );
                                                    ui.painter().with_clip_rect(clip).galley(
                                                        pos2(
                                                            inner.right() - detail.size().x.min(dw),
                                                            inner.center().y
                                                                - detail.size().y / 2.0,
                                                        ),
                                                        detail,
                                                        p.muted,
                                                    );
                                                }
                                                if row.clicked() {
                                                    picked = Some(Some(item.cmd.clone()));
                                                }
                                            }
                                        });
                                });
                        });
                    });
            })
            .response
            .rect;
        if backdrop.inner.clicked()
            && !panel.contains(backdrop.inner.interact_pointer_pos().unwrap_or_default())
        {
            picked = Some(None);
        }
        picked
    }
}

impl Workspace {
    /// Opens one of the lists, built from what the project has now.
    pub fn open_palette(&mut self, kind: Kind, settings: &Settings) {
        let items = match kind {
            Kind::Commands => {
                let show_pdf = settings.layout != "editor" && !self.focus_mode;
                COMMANDS
                    .iter()
                    // Only while the PDF pane is there: presenting needs its loaded document.
                    .filter(|c| c.0 != "present" || show_pdf)
                    .map(|(id, _, key)| Item {
                        label: label(id, self.focus_mode).to_owned(),
                        detail: match *id {
                            "reindent" => "the selection, or the whole file".to_owned(),
                            _ if key.is_empty() => String::new(),
                            _ => display(keys_of(key)),
                        },
                        cmd: Cmd::Command(id),
                    })
                    .collect()
            }
            Kind::Files => {
                fn files(entries: &[lemmata_project::Entry], out: &mut Vec<String>) {
                    for e in entries {
                        if e.is_dir {
                            files(&e.children, out);
                        } else {
                            out.push(e.path.clone());
                        }
                    }
                }
                let mut all = Vec::new();
                files(&self.info.tree, &mut all);
                all.into_iter()
                    .filter(|p| tree::is_text_file(p))
                    .map(|p| Item {
                        label: tree::base_name(&p).to_owned(),
                        detail: p.rfind('/').map_or(String::new(), |i| p[..i].to_owned()),
                        cmd: Cmd::Open(p),
                    })
                    .collect()
            }
            Kind::Environments => crate::editor::complete::environments()
                .iter()
                .map(|e| Item {
                    label: e.name.to_owned(),
                    detail: e.detail.to_owned(),
                    cmd: Cmd::Wrap(e.name),
                })
                .collect(),
            Kind::Headings => self
                .headings
                .iter()
                .map(|h| Item {
                    label: format!(
                        "{}{}",
                        "\u{2003}".repeat(h.level.saturating_sub(1) as usize),
                        if h.title.is_empty() {
                            "(untitled)"
                        } else {
                            &h.title
                        }
                    ),
                    detail: format!("{}:{}", tree::base_name(&h.file), h.line),
                    cmd: Cmd::Goto(h.file.clone(), h.line as usize),
                })
                .collect(),
            Kind::Symbols => Vec::new(),
        };
        self.palette = Some(Palette::new(kind, items));
    }

    /// "Go to a label or citation…": the project's labels and keys, read on the worker.
    fn open_symbols(&mut self, backend: &Backend) {
        Workspace::job(
            backend,
            |s| s.symbol_defs(),
            |ws, _, r| match r {
                Ok(defs) => {
                    let items = defs
                        .into_iter()
                        .map(|d| Item {
                            detail: format!(
                                "{} · {}:{}",
                                if d.kind == lemmata_project::symbols::Kind::Label {
                                    "label"
                                } else {
                                    "citation"
                                },
                                d.file,
                                d.line
                            ),
                            label: d.name,
                            cmd: Cmd::Goto(d.file, d.line as usize),
                        })
                        .collect();
                    ws.palette = Some(Palette::new(Kind::Symbols, items));
                }
                Err(e) => ws.notice(e, Tone::Error),
            },
        );
    }

    pub(super) fn palette_ui(
        &mut self,
        ctx: &egui::Context,
        p: &Colors,
        backend: &Backend,
        settings: &mut Settings,
    ) -> Option<Action> {
        let picked = self.palette.as_mut()?.show(ctx, p)?;
        self.palette = None;
        let refocus = |ws: &mut Workspace| {
            if let Some(ed) = ws.active_editor() {
                ed.refocus = true;
            }
        };
        match picked {
            None => {
                refocus(self);
                None
            }
            Some(Cmd::Open(path)) => {
                self.open_file(backend, &path);
                None
            }
            Some(Cmd::Goto(file, line)) => {
                self.goto(backend, &file, line);
                None
            }
            Some(Cmd::Wrap(name)) => {
                let indent = self.indent;
                if let Some(ed) = self.active_editor() {
                    crate::editor::commands::wrap_in_environment(&mut ed.doc, name, indent);
                    crate::editor::view::after_command(ed);
                    ed.refocus = true;
                }
                None
            }
            Some(Cmd::Command(id)) => self.run_command(ctx, id, backend, settings),
        }
    }

    /// Runs a palette or menu command by its id.
    pub fn run_command(
        &mut self,
        ctx: &egui::Context,
        id: &str,
        backend: &Backend,
        settings: &mut Settings,
    ) -> Option<Action> {
        let mut action = None;
        let mut panel = |ws: &mut Workspace, which: Panel, settings: &mut Settings| {
            if ws.open_panel(which, backend, settings) {
                action = Some(Action::Settings);
            }
        };
        match id {
            "recompile" => self.recompile(backend, settings),
            "open-file" => self.open_palette(Kind::Files, settings),
            "search" => self.open_search(None),
            "cite" => self.open_cite(backend, None),
            "figure" => self.open_figure(),
            "table" => self.open_table(),
            "snippets" => return Some(Action::Snippets),
            "symbols" => self.symbols = Some(Default::default()),
            "wrap-env" => self.open_palette(Kind::Environments, settings),
            "words" => self.open_word_count(backend),
            "shortcuts" => self.shortcut_sheet = Some(String::new()),
            "snapshot" => self.history.label = Some(String::new()),
            "history" => panel(self, Panel::History, settings),
            "comment" => {
                if self.add_comment(backend, settings) {
                    return Some(Action::Settings);
                }
            }
            "comments" => panel(self, Panel::Review, settings),
            "bib-manager" => self.open_bib_manager(backend, settings),
            "git" => panel(self, Panel::Git, settings),
            "todos" => panel(self, Panel::Todos, settings),
            "format" => self.format_document(backend),
            "reindent" => {
                let indent = self.indent;
                if let Some(ed) = self.active_editor() {
                    ed.reindent_lines(indent);
                }
            }
            "sync" => self.sync_forward(backend),
            "focus" => self.focus_mode = !self.focus_mode,
            "new-file" => self.new_item(false, String::new()),
            "goto-heading" => self.open_palette(Kind::Headings, settings),
            "goto-symbol" => self.open_symbols(backend),
            "rename-symbol" => self.open_rename(backend),
            "reopen-tab" => self.reopen_closed(backend),
            "reveal" => {
                if let Some(a) = self.active.clone() {
                    // Every folder above it opens, and the sidebar shows.
                    let mut at = a.as_str();
                    while let Some(i) = at.rfind('/') {
                        at = &at[..i];
                        self.collapsed.remove(at);
                    }
                    self.sel.selected = vec![a.clone()];
                    self.tree_cursor = Some(a);
                    self.search = None;
                    self.focus_mode = false;
                    if !settings.sidebar_open || settings.layout == "pdf" {
                        settings.sidebar_open = true;
                        if settings.layout == "pdf" {
                            settings.layout = "split".into();
                        }
                        action = Some(Action::Settings);
                    }
                }
            }
            "unused" => self.show_unused(backend),
            "undo-files" => self.undo_files(backend),
            "deleted" => self.show_deleted(backend),
            "zotero-cite" => self.cite_from_zotero(backend),
            "zotero-sync" => self.sync_zotero(backend, true),
            "export-pdf" => self.export(backend, settings, Save::Pdf),
            "present" => self.present(ctx),
            "open-external" => backend.spawn(|s| Msg::Done(s.open_pdf_externally())),
            "save-template" => {
                self.save_all_quietly(backend, settings);
                return Some(Action::SaveTemplate(self.root.clone()));
            }
            "export-zip" => self.export(backend, settings, Save::Zip),
            "export-arxiv" => self.export(backend, settings, Save::Arxiv { flatten: false }),
            "export-arxiv-flat" => self.export(backend, settings, Save::Arxiv { flatten: true }),
            "project-settings" => self.open_project_settings(),
            "preferences" => return Some(Action::Preferences),
            "guide" => backend.spawn(|s| Msg::Done(s.open_url(GUIDE.into()))),
            "report" => return Some(Action::Report),
            "close" => return Some(Action::Library),
            _ => {}
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // fuzzy.test.ts
    #[test]
    fn fuzzy_needs_the_order_and_ranks_word_starts() {
        assert!(score("cmt", "chapters/method.tex").is_some());
        assert!(score("tmc", "chapters/method.tex").is_none());
        let files = [
            "chapters/introduction.tex",
            "main.tex",
            "figures/intro-plot.png",
        ];
        let rank = |q: &str| {
            let mut s: Vec<(f32, &str)> = files
                .iter()
                .filter_map(|f| score(q, f).map(|x| (x, *f)))
                .collect();
            s.sort_by(|a, b| b.0.total_cmp(&a.0));
            s.into_iter().map(|x| x.1).collect::<Vec<_>>()
        };
        assert_eq!(rank("main")[0], "main.tex");
        assert_eq!(rank("intro").len(), 2);
        assert_eq!(score("", "anything"), Some(0.0));
    }

    #[test]
    fn every_command_has_a_name() {
        for (id, l, _) in COMMANDS {
            assert_eq!(label(id, false), *l);
        }
        assert_eq!(label("focus", true), "Leave focus mode");
    }
}
