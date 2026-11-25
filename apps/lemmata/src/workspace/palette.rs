//! The command palette (Ctrl+Shift+P) and the lists that share its look (Palette.svelte): quick
//! open (Ctrl+P), wrap in an environment, go to a section, go to a label or citation. The main
//! menu runs the same commands, so a command has one name wherever it appears.

use super::{Action, Tone, Workspace, panels::SidePanel};
use crate::backend::{Backend, Msg, Save};
use crate::icons::{self, Icon};
use crate::shortcuts::{display, keys_of};
use crate::store::Settings;
use crate::theme::{self, Palette as Colors};
use crate::widgets;
use crate::workspace::tree;
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Key, Margin, Modifiers, Rect, RichText, Sense, Stroke,
    StrokeKind, pos2, vec2,
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
    /// The items matching a query, best first (their indices): scored once per query, not per frame.
    found: std::cell::RefCell<Option<(String, Vec<usize>)>>,
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
    ("reindent", "Reindent lines", "reindent"),
    ("sync", "Show this line in the PDF", "sync"),
    ("focus", "Focus mode", "focus"),
    ("toggle-wrap", "Turn line wrapping on or off", ""),
    ("toggle-dark-pdf", "Turn dark PDF pages on or off", ""),
    ("new-file", "New file…", ""),
    ("goto-heading", "Go to a section…", "goto-heading"),
    ("goto-symbol", "Go to a label or citation…", ""),
    ("rename-symbol", "Rename label or citation key…", "rename"),
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
    ("save", "Save all files", "save"),
    ("close-tab", "Close tab", "close-tab"),
    ("undo", "Undo", "undo"),
    ("redo", "Redo", "redo"),
    ("find", "Find in file", "find"),
    ("goto-line", "Go to line…", "goto-line"),
    (
        "select-all-matches",
        "Select all matches",
        "select-all-matches",
    ),
    ("next-problem", "Next problem", "next-problem"),
    ("prev-problem", "Previous problem", "prev-problem"),
    ("next-todo", "Next TODO", "next-todo"),
    ("prev-todo", "Previous TODO", "prev-todo"),
    ("next-comment", "Next review comment", "next-comment"),
    ("prev-comment", "Previous review comment", "prev-comment"),
    ("definition", "Go to definition", "definition"),
    ("references", "Find references", "references"),
    ("bold", "Bold", "bold"),
    ("italic", "Italic", "italic"),
    (
        "heading-down",
        "Make heading one level deeper",
        "heading-down",
    ),
    ("heading-up", "Make heading one level higher", "heading-up"),
    ("fold-all", "Fold everything", "fold-all"),
    ("unfold-all", "Unfold everything", "unfold-all"),
    ("project-settings", "Project settings", ""),
    ("preferences", "Preferences", ""),
    ("guide", "User guide", ""),
    ("report", "Report a problem…", ""),
    ("close", "All projects", ""),
];

/// The palette's commands the editor runs (`Editor::queued`).
const EDITOR: &[&str] = &[
    "undo",
    "redo",
    "find",
    "goto-line",
    "select-all-matches",
    "next-problem",
    "prev-problem",
    "next-todo",
    "prev-todo",
    "next-comment",
    "prev-comment",
    "definition",
    "references",
    "bold",
    "italic",
    "heading-down",
    "heading-up",
    "fold-all",
    "unfold-all",
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
            found: Default::default(),
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
        let mut found = self.found.borrow_mut();
        if found.as_ref().is_none_or(|(q, _)| *q != self.query) {
            let mut scored: Vec<(f32, usize)> = self
                .items
                .iter()
                .enumerate()
                .filter_map(|(n, i)| {
                    score(&self.query, &format!("{} {}", i.label, i.detail)).map(|s| (s, n))
                })
                .collect();
            // Stable: equal scores keep the list's order.
            scored.sort_by(|a, b| b.0.total_cmp(&a.0));
            let best = scored.into_iter().take(100).map(|(_, n)| n).collect();
            *found = Some((self.query.clone(), best));
        }
        found
            .iter()
            .flat_map(|f| &f.1)
            .map(|&n| &self.items[n])
            .collect()
    }

    fn group_title(&self) -> &'static str {
        match self.kind {
            Kind::Commands => "Commands",
            Kind::Files => "Files",
            Kind::Environments => "Environments",
            Kind::Headings => "Sections",
            Kind::Symbols => "Labels & citations",
        }
    }

    fn item_icon(&self, item: &Item) -> Icon {
        match (&self.kind, &item.cmd) {
            (Kind::Files, _) => icons::FILE_TEXT,
            (Kind::Environments, _) => icons::CODE,
            (Kind::Headings, _) => icons::LIST_TREE,
            (Kind::Symbols, _) => icons::BOOK_OPEN,
            (Kind::Commands, Cmd::Command(id)) => match *id {
                "recompile" => icons::REFRESH_CW,
                "open-file" | "new-file" | "reveal" => icons::FILE_TEXT,
                "search" | "find" => icons::SEARCH,
                "project-settings" | "preferences" => icons::SETTINGS,
                "git" => icons::GIT_BRANCH,
                "history" | "snapshot" => icons::HISTORY,
                "comments" | "comment" => icons::MESSAGE_SQUARE,
                "todos" => icons::CIRCLE_CHECK,
                "cite" | "bib-manager" | "zotero-cite" | "zotero-sync" => icons::BOOK_OPEN,
                "symbols" => icons::SIGMA,
                _ => icons::SPARKLES,
            },
            _ => icons::SPARKLES,
        }
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
            // Home and End stay with the text field's caret.
            if i.consume_key(Modifiers::NONE, Key::PageDown) {
                self.selected = (self.selected + 8).min(n.saturating_sub(1));
                self.scroll = true;
            }
            if i.consume_key(Modifiers::NONE, Key::PageUp) {
                self.selected = self.selected.saturating_sub(8);
                self.scroll = true;
            }
            if i.consume_key(Modifiers::NONE, Key::Escape) {
                picked = Some(None);
            }
            // Options aren't tab stops: Tab / Shift+Tab stay in the input.
            i.consume_key(Modifiers::NONE, Key::Tab);
            i.consume_key(Modifiers::SHIFT, Key::Tab);
        });
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
            picked = Some(self.results().get(self.selected).map(|i| i.cmd.clone()));
        }
        let backdrop = egui::Area::new(egui::Id::unique("palette-backdrop"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, r) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(rect, 0, Color32::from_rgba_unmultiplied(12, 17, 22, 82));
                r
            });
        let width = 500.0f32.min(screen.width() - 32.0).max(280.0);
        let group_title = self.group_title();
        let is_commands = self.kind == Kind::Commands;
        let query_chars: Vec<char> = self
            .query
            .to_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let area = egui::Area::new(egui::Id::unique("palette"))
            .kind(egui::UiKind::Modal)
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::CENTER_TOP)
            .fixed_pos(pos2(
                screen.center().x,
                screen.top() + screen.height() * 0.12,
            ));
        let layer = area.layer();
        ctx.memory_mut(|m| m.set_modal_layer(layer));
        let panel_resp = area
            .show(ctx, |ui| {
                ui.set_width(width);
                egui::Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(theme::R_LG)
                    .shadow(p.shadow_lg)
                    .show(ui, |ui| {
                        ui.set_width(width - 2.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        // 46 px search header with leading search icon and trailing Esc badge.
                        let (head_rect, _) = ui
                            .allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::hover());
                        let mut head = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(head_rect.shrink2(vec2(14.0, 0.0)))
                                .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        );
                        head.spacing_mut().item_spacing.x = 10.0;
                        icons::show(&mut head, icons::SEARCH, 15.0, p.faint);
                        let before = self.query.clone();
                        let placeholder = self.placeholder();
                        let input_w = (head.available_width() - 42.0).max(100.0);
                        let r = egui::TextEdit::singleline(&mut self.query)
                            .hint_text(RichText::new(placeholder).size(14.0).color(p.faint))
                            .font(FontId::proportional(14.0))
                            .text_color(p.ink)
                            .frame(egui::Frame::NONE)
                            .desired_width(input_w)
                            .show(&mut head)
                            .response
                            .response;
                        head.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            widgets::kbd(ui, p, "Esc");
                        });
                        ui.ctx()
                            .accesskit_node_builder(r.id, |n| n.set_label(placeholder));
                        if std::mem::take(&mut self.focus) || !r.has_focus() {
                            r.request_focus();
                        }
                        if self.query != before {
                            self.selected = 0;
                            self.scroll = true;
                        }
                        ui.painter().hline(
                            head_rect.x_range(),
                            head_rect.bottom() - 0.5,
                            Stroke::new(1.0, p.border),
                        );
                        let max_h = (screen.height() - 210.0).clamp(140.0, screen.height() * 0.50);
                        let results: Vec<Item> = self.results().into_iter().cloned().collect();
                        let w = ui.available_width();
                        ui.allocate_ui(vec2(w, max_h + 12.0), |ui| {
                            egui::Frame::new()
                                .inner_margin(Margin::same(6))
                                .show(ui, |ui| {
                                    egui::ScrollArea::vertical()
                                        .max_height(max_h)
                                        .scroll_bar_visibility(
                                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                        )
                                        .auto_shrink([false, true])
                                        .show(ui, |ui| {
                                            ui.spacing_mut().item_spacing.y = 0.0;
                                            if !results.is_empty() {
                                                egui::Frame::new()
                                                    .inner_margin(Margin {
                                                        left: 8,
                                                        right: 8,
                                                        top: 6,
                                                        bottom: 4,
                                                    })
                                                    .show(ui, |ui| {
                                                        ui.label(
                                                            RichText::new(
                                                                group_title.to_uppercase(),
                                                            )
                                                            .font(FontId::new(
                                                                11.0,
                                                                theme::font::sans_600(),
                                                            ))
                                                            .color(p.faint),
                                                        );
                                                    });
                                            } else {
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
                                                    vec2(ui.available_width(), 34.0),
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
                                                let selected = i == self.selected;
                                                if selected {
                                                    ui.ctx().accesskit_node_builder(r.id, |n| {
                                                        n.set_active_descendant(
                                                            egui::accesskit::NodeId::from(
                                                                row.id.value(),
                                                            ),
                                                        )
                                                    });
                                                    ui.painter().rect_filled(
                                                        rect,
                                                        CornerRadius::same(7),
                                                        p.accent_soft,
                                                    );
                                                    if std::mem::take(&mut self.scroll) {
                                                        ui.scroll_to_rect(rect, None);
                                                    }
                                                }
                                                let inner = rect.shrink2(vec2(8.0, 0.0));
                                                let ico_rect = Rect::from_min_size(
                                                    pos2(inner.left(), inner.center().y - 7.5),
                                                    vec2(15.0, 15.0),
                                                );
                                                icons::paint(
                                                    ui.painter(),
                                                    ico_rect,
                                                    self.item_icon(item),
                                                    if selected { p.accent_text } else { p.faint },
                                                );
                                                let label_x = ico_rect.right() + 10.0;
                                                let detail = ui.painter().layout_no_wrap(
                                                    item.detail.clone(),
                                                    FontId::monospace(11.0),
                                                    p.faint,
                                                );
                                                let has_kbd =
                                                    is_commands && !item.detail.is_empty();
                                                let pill_pad = if has_kbd { 12.0 } else { 0.0 };
                                                let dw = (detail.size().x + pill_pad)
                                                    .min(inner.width() * 0.45);
                                                let label_clip = Rect::from_min_max(
                                                    pos2(label_x, inner.top()),
                                                    pos2(
                                                        (inner.right() - dw - 10.0)
                                                            .max(label_x + 20.0),
                                                        inner.bottom(),
                                                    ),
                                                );
                                                let base_ink =
                                                    if selected { p.ink } else { p.ink_2 };
                                                let mut job = egui::text::LayoutJob::default();
                                                let mut qi = 0usize;
                                                for ch in item.label.chars() {
                                                    let hit = qi < query_chars.len()
                                                        && ch.to_lowercase().next().is_some_and(
                                                            |lc| lc == query_chars[qi],
                                                        );
                                                    if hit {
                                                        qi += 1;
                                                    }
                                                    job.append(
                                                        &ch.to_string(),
                                                        0.0,
                                                        egui::TextFormat {
                                                            font_id: if hit {
                                                                FontId::new(
                                                                    13.0,
                                                                    theme::font::sans_600(),
                                                                )
                                                            } else {
                                                                FontId::proportional(13.0)
                                                            },
                                                            color: if hit {
                                                                p.ink
                                                            } else {
                                                                base_ink
                                                            },
                                                            ..Default::default()
                                                        },
                                                    );
                                                }
                                                let lg = ui.fonts_mut(|f| f.layout_job(job));
                                                ui.painter().with_clip_rect(label_clip).galley(
                                                    pos2(
                                                        label_x,
                                                        inner.center().y - lg.size().y / 2.0,
                                                    ),
                                                    lg,
                                                    base_ink,
                                                );
                                                if !item.detail.is_empty() {
                                                    if has_kbd {
                                                        let pr = Rect::from_min_size(
                                                            pos2(
                                                                inner.right() - dw,
                                                                inner.center().y - 10.0,
                                                            ),
                                                            vec2(dw, 20.0),
                                                        );
                                                        ui.painter().rect(
                                                            pr,
                                                            CornerRadius::same(theme::R_SM),
                                                            p.surface_2,
                                                            Stroke::new(1.0, p.border),
                                                            StrokeKind::Inside,
                                                        );
                                                        ui.painter().with_clip_rect(pr).galley(
                                                            pr.center() - detail.size() / 2.0,
                                                            detail,
                                                            p.faint,
                                                        );
                                                    } else {
                                                        let clip = Rect::from_min_max(
                                                            pos2(inner.right() - dw, inner.top()),
                                                            inner.max,
                                                        );
                                                        ui.painter().with_clip_rect(clip).galley(
                                                            pos2(
                                                                inner.right()
                                                                    - detail.size().x.min(dw),
                                                                inner.center().y
                                                                    - detail.size().y / 2.0,
                                                            ),
                                                            detail,
                                                            p.faint,
                                                        );
                                                    }
                                                }
                                                if row.clicked() {
                                                    picked = Some(Some(item.cmd.clone()));
                                                }
                                            }
                                        });
                                });
                        });
                        // Key-hint footer strip.
                        let (foot_rect, _) = ui
                            .allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
                        ui.painter().hline(
                            foot_rect.x_range(),
                            foot_rect.top() + 0.5,
                            Stroke::new(1.0, p.border),
                        );
                        let mut foot = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(foot_rect.shrink2(vec2(14.0, 0.0)))
                                .layout(egui::Layout::left_to_right(egui::Align::Center)),
                        );
                        foot.spacing_mut().item_spacing.x = 6.0;
                        let hint = |ui: &mut egui::Ui, key: &str, what: &str| {
                            widgets::kbd(ui, p, key);
                            ui.label(RichText::new(what).size(11.5).color(p.faint));
                            ui.add_space(6.0);
                        };
                        hint(&mut foot, "↑↓", "move");
                        hint(&mut foot, "Enter", if is_commands { "run" } else { "open" });
                        if is_commands {
                            let qo = display(keys_of("quick-open"));
                            hint(&mut foot, &qo, "quick open");
                        } else if self.kind == Kind::Files {
                            let cp = display(keys_of("palette"));
                            hint(&mut foot, &cp, "commands");
                        }
                    });
            })
            .response;
        panel_resp.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Dialog,
                true,
                if is_commands {
                    "Command palette"
                } else {
                    "Quick open"
                },
            )
        });
        let panel = panel_resp.rect;
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
                let has_pdf = self.pane.view.shown_bytes().is_some();
                // The editor's commands do nothing without an open file: not offered then.
                let has_file = self
                    .active
                    .as_ref()
                    .is_some_and(|a| self.editors.contains_key(a));
                let is_tex = self.active.as_deref().is_some_and(|a| {
                    self.editors.contains_key(a) && a.to_ascii_lowercase().ends_with(".tex")
                });
                COMMANDS
                    .iter()
                    .filter(|c| {
                        has_file
                            || !(EDITOR.contains(&c.0)
                                || matches!(c.0, "reindent" | "close-tab" | "format"))
                    })
                    .filter(|c| c.0 != "format" || is_tex)
                    // PDF commands need a built PDF (and presenting needs the PDF pane visible).
                    .filter(|c| c.0 != "present" || (show_pdf && has_pdf))
                    .filter(|c| !matches!(c.0, "sync" | "export-pdf" | "open-external") || has_pdf)
                    .map(|(id, _, key)| Item {
                        label: label(id, self.focus_mode).to_owned(),
                        detail: if key.is_empty() {
                            String::new()
                        } else {
                            display(keys_of(key))
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
        let mut panel = |ws: &mut Workspace, which: SidePanel, settings: &mut Settings| {
            if ws.open_side(which, backend, settings) {
                action = Some(Action::Settings);
            }
        };
        match id {
            "recompile" => self.recompile(backend, settings),
            "open-file" => self.open_palette(Kind::Files, settings),
            "search" => {
                self.open_search(None);
                settings.sidebar_open = true;
                settings.side_panel = "search".into();
                action = Some(Action::Settings);
            }
            "cite" => self.open_cite(backend, None),
            "figure" => self.open_figure(),
            "table" => self.open_table(),
            "snippets" => return Some(Action::Snippets),
            "symbols" => self.symbols = Some(Default::default()),
            "wrap-env" => self.open_palette(Kind::Environments, settings),
            "words" => self.open_word_count(backend),
            "shortcuts" => self.shortcut_sheet = Some(String::new()),
            "snapshot" => self.history.label = Some(String::new()),
            "history" => panel(self, SidePanel::History, settings),
            "comment" => {
                if self.add_comment(backend, settings) {
                    return Some(Action::Settings);
                }
            }
            "comments" => panel(self, SidePanel::Comments, settings),
            "bib-manager" => self.open_bib_manager(backend, settings),
            "git" => panel(self, SidePanel::Git, settings),
            "todos" => panel(self, SidePanel::Todos, settings),
            "format" => self.format_document(backend),
            "reindent" => {
                let indent = self.indent;
                if let Some(ed) = self.active_editor() {
                    ed.reindent_lines(indent);
                }
            }
            "sync" => self.sync_forward(backend),
            "focus" => self.focus_mode = !self.focus_mode,
            "toggle-wrap" => {
                settings.word_wrap = !settings.word_wrap;
                return Some(Action::Settings);
            }
            "toggle-dark-pdf" => {
                settings.pdf_dark = !settings.pdf_dark;
                return Some(Action::Settings);
            }
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
                    self.side_panel = SidePanel::Files;
                    self.focus_mode = false;
                    settings.sidebar_open = true;
                    settings.side_panel = "files".into();
                    action = Some(Action::Settings);
                }
            }
            "unused" => self.show_unused(backend),
            "undo-files" => self.undo_files(backend, settings),
            "deleted" => self.show_deleted(backend),
            "zotero-cite" => self.cite_from_zotero(backend),
            "zotero-sync" => self.sync_zotero(backend, true),
            "export-pdf" => self.export(backend, settings, Save::Pdf),
            "present" => self.present(ctx),
            "open-external" => self.spawn(backend, |s| Msg::Done(s.open_pdf_externally())),
            "save-template" => {
                self.after_saving(backend, settings, |ws, _| {
                    ws.pending_save_template = true;
                });
            }
            "export-zip" => self.export(backend, settings, Save::Zip),
            "export-arxiv" => self.export(backend, settings, Save::Arxiv { flatten: false }),
            "export-arxiv-flat" => self.export(backend, settings, Save::Arxiv { flatten: true }),
            "project-settings" => self.open_project_settings(),
            "preferences" => return Some(Action::Preferences),
            "guide" => backend.spawn(|s| Msg::Done(s.open_url(GUIDE.into()))),
            "report" => return Some(Action::Report),
            "close" => return Some(Action::Library),
            "save" => {
                if settings.auto_compile {
                    self.save_all(backend, settings);
                } else {
                    self.recompile(backend, settings);
                }
            }
            "close-tab" => {
                if let Some(a) = self.active.clone() {
                    self.close_tab(backend, &a, settings);
                }
            }
            // Shortcuts the editor handles: it runs them as their keys would.
            _ => {
                if let Some(&(cmd, _, _)) =
                    COMMANDS.iter().find(|c| c.0 == id && EDITOR.contains(&id))
                    && let Some(ed) = self.active_editor()
                {
                    ed.queued = Some(cmd);
                    ed.refocus = true;
                }
            }
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_follow_the_query_though_they_are_cached() {
        let item = |l: &str| Item {
            label: l.into(),
            detail: String::new(),
            cmd: Cmd::Open(l.into()),
        };
        let mut p = Palette::new(Kind::Files, vec![item("alpha.tex"), item("beta.tex")]);
        assert_eq!(p.results().len(), 2);
        p.query = "alp".into();
        assert_eq!(p.results().len(), 1);
        assert_eq!(p.results()[0].label, "alpha.tex");
        p.query = "bet".into();
        assert_eq!(p.results()[0].label, "beta.tex");
    }

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
    fn undo_and_redo_are_commands_the_editor_runs() {
        for id in ["undo", "redo"] {
            assert!(COMMANDS.iter().any(|c| c.0 == id) && EDITOR.contains(&id));
        }
        assert!(EDITOR.iter().all(|e| COMMANDS.iter().any(|c| c.0 == *e)));
    }

    #[test]
    fn every_command_has_a_name() {
        for (id, l, _) in COMMANDS {
            assert_eq!(label(id, false), *l);
        }
        assert_eq!(label("focus", true), "Leave focus mode");
    }
}
