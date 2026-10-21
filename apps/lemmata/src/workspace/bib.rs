//! The citation picker (CitePicker.svelte), the bibliography manager (BibManager.svelte with
//! bib/manager.ts) and the Zotero flows of App.svelte.

use super::{Notice, NoticeAction, Tone, Workspace};
use crate::backend::{Backend, Msg};
use crate::icons;
use crate::store::Settings;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind, Size};
use eframe::egui::{
    self, CornerRadius, FontId, Key, Margin, Modifiers, Rect, RichText, Sense, Stroke, StrokeKind,
    Ui, pos2, vec2,
};
use lemmata_bib::Entry;
use lemmata_bib::manage::{Duplicate, Library, ManagedEntry};
use lemmata_bib::search::Hit;
use lemmata_bib::zotero::Status;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

// ── bib/manager.ts ─────────────────────────────────────────────────────────────

/// Entries containing every search word in `hay`.
fn matches(hay: &str, query: &str) -> bool {
    let hay = hay.to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|t| hay.contains(t))
}

/// Non-empty before empty, then without case.
fn text_cmp(a: &str, b: &str) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (false, true) => Ordering::Less,
        (true, false) => Ordering::Greater,
        _ => a.to_lowercase().cmp(&b.to_lowercase()),
    }
}

/// Entry indices sorted by key, author or year (newest first); missing values last.
fn sort_entries(entries: &[ManagedEntry], idx: &mut [usize], by: &str) {
    idx.sort_by(|&a, &b| {
        let (a, b) = (&entries[a].entry, &entries[b].entry);
        let first = match by {
            "author" => text_cmp(&a.authors, &b.authors),
            "year" => match (a.year.is_empty(), b.year.is_empty()) {
                (false, true) => Ordering::Less,
                (true, false) => Ordering::Greater,
                _ => b.year.cmp(&a.year),
            },
            _ => Ordering::Equal,
        };
        first.then_with(|| text_cmp(&a.key, &b.key))
    });
}

/// For each entry index, the duplicate groups it is in: the kind and the other members.
fn warnings_by_entry(duplicates: &[Duplicate]) -> HashMap<usize, Vec<(&'static str, Vec<usize>)>> {
    let mut out: HashMap<usize, Vec<(&'static str, Vec<usize>)>> = HashMap::new();
    for d in duplicates {
        for &i in &d.entries {
            out.entry(i).or_default().push((
                d.kind,
                d.entries.iter().copied().filter(|&j| j != i).collect(),
            ));
        }
    }
    out
}

/// A key not in `taken` (without case): `key`, `keya`, `keyb`, …
fn free_key(base: &str, taken: &[&str]) -> String {
    let lower: HashSet<String> = taken.iter().map(|k| k.to_lowercase()).collect();
    let mut key = base.to_owned();
    for c in b'a'..=b'z' {
        if !lower.contains(&key.to_lowercase()) {
            break;
        }
        key = format!("{base}{}", c as char);
    }
    key
}

/// Whether BibTeX would accept the key (`valid_key` in lemmata-bib).
fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && !key
            .chars()
            .any(|c| c.is_whitespace() || ",{}\"#%'()=\\".contains(c))
}

// ── The citation picker ────────────────────────────────────────────────────────

#[derive(Default)]
pub struct CitePicker {
    entries: Vec<Entry>,
    files: Vec<String>,
    loading: bool,
    query: String,
    selected: usize,
    picked: Vec<String>,
    add_id: String,
    add_file: String,
    adding: bool,
    /// Whether it worked, and what to say.
    add_msg: Option<(bool, String)>,
    online: Option<Vec<Hit>>,
    searching: bool,
    online_error: String,
    adding_doi: String,
    zotero: Option<Status>,
    search_id: Option<egui::Id>,
    scroll: bool,
}

enum Cite {
    Insert(Vec<String>),
    SearchOnline,
    AddHit(String),
    AddReference,
    Zotero,
}

impl CitePicker {
    fn results(&self) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|e| {
                matches(
                    &format!("{} {} {} {}", e.key, e.authors, e.title, e.year),
                    &self.query,
                )
            })
            .collect()
    }

    fn toggle(&mut self, key: &str) {
        match self.picked.iter().position(|k| k == key) {
            Some(i) => {
                self.picked.remove(i);
            }
            None => self.picked.push(key.to_owned()),
        }
    }

    fn show(&mut self, ctx: &egui::Context, p: &Palette) -> (bool, Option<Cite>) {
        let mut out = None;
        let r = widgets::modal_placed(
            ctx,
            p,
            "Insert citation",
            Some("Search your bibliography by author, title, year or key."),
            700.0,
            true,
            |ui| self.body(ui, p, &mut out),
            None,
        );
        (r.closed, out)
    }

    fn body(&mut self, ui: &mut Ui, p: &Palette, out: &mut Option<Cite>) {
        ui.spacing_mut().item_spacing.y = 10.0;
        if let Some(z) = self.zotero.filter(|z| *z != Status::Unavailable) {
            ui.add_space(-6.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let starting = z == Status::Starting;
                let label = if starting {
                    "Zotero is starting…"
                } else {
                    "Cite from Zotero…"
                };
                if Button::new(label)
                    .with_icon(icons::BOOK_OPEN)
                    .size(Size::Sm)
                    .enabled(!starting)
                    .show(ui, p)
                    .on_hover_text("Opens Zotero's citation picker")
                    .clicked()
                {
                    *out = Some(Cite::Zotero);
                }
            });
        }
        let results_len = self.results().len();
        // The list keys, while the search box has the focus.
        if self
            .search_id
            .is_some_and(|id| ui.memory(|m| m.has_focus(id)))
        {
            ui.input_mut(|i| {
                if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                    self.selected = (self.selected + 1).min(results_len.saturating_sub(1));
                    self.scroll = true;
                }
                if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                    self.selected = self.selected.saturating_sub(1);
                    self.scroll = true;
                }
            });
            // Ctrl+Enter adds to the selection; Enter inserts.
            let ctrl = ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Enter));
            let enter = !ctrl && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
            if let Some(key) = self.results().get(self.selected).map(|e| e.key.clone()) {
                if ctrl {
                    self.toggle(&key);
                } else if enter {
                    *out = Some(Cite::Insert(if self.picked.is_empty() {
                        vec![key]
                    } else {
                        self.picked.clone()
                    }));
                }
            }
        }
        let before = self.query.clone();
        let r = widgets::text_input(
            ui,
            p,
            &mut self.query,
            "Search by author, title, year or key",
            false,
        );
        ui.ctx()
            .accesskit_node_builder(r.id, |n| n.set_label("Search the bibliography"));
        if self.search_id.is_none() {
            r.request_focus();
        }
        self.search_id = Some(r.id);
        if self.query != before {
            self.selected = 0;
            self.online = None;
            self.online_error.clear();
        }
        self.list(ui, p, out);
        if !self.query.trim().is_empty() && !self.loading {
            self.online_part(ui, p, out);
        }
        // .hint
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let muted = |ui: &mut Ui, s: &str| {
                ui.label(RichText::new(s).size(12.0).color(p.muted));
            };
            widgets::kbd(ui, p, "Enter");
            muted(ui, "inserts ·");
            widgets::kbd(ui, p, "Ctrl");
            muted(ui, "+");
            widgets::kbd(ui, p, "Enter");
            muted(ui, "or the checkbox selects several");
            if !self.picked.is_empty() {
                muted(ui, "·");
                let l = ui.add(
                    egui::Label::new(
                        RichText::new(format!("Insert {} selected", self.picked.len()))
                            .size(12.0)
                            .underline()
                            .color(p.accent),
                    )
                    .sense(Sense::click()),
                );
                if l.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    *out = Some(Cite::Insert(self.picked.clone()));
                }
            }
        });
        self.add_form(ui, p, out);
    }

    fn list(&mut self, ui: &mut Ui, p: &Palette, out: &mut Option<Cite>) {
        let max_h = ui.ctx().content_rect().height() * 0.42;
        let shown: Vec<Entry> = self.results().into_iter().take(200).cloned().collect();
        list_frame(ui, p, max_h, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("cite-list")
                .max_height(max_h)
                .min_scrolled_height(80.0f32.min(max_h))
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let note = if self.loading {
                        Some("Reading the bibliography…".to_owned())
                    } else if self.entries.is_empty() {
                        Some("No entries yet. Add one by DOI or arXiv ID below.".to_owned())
                    } else if shown.is_empty() {
                        Some(format!("Nothing matches “{}”.", self.query))
                    } else {
                        None
                    };
                    if let Some(n) = note {
                        egui::Frame::new()
                            .inner_margin(Margin::symmetric(12, 10))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.set_min_height(58.0);
                                ui.label(RichText::new(n).color(p.muted));
                            });
                    }
                    for (i, e) in shown.iter().enumerate() {
                        let (rect, row) = ui
                            .allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::click());
                        row.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, &e.key)
                        });
                        if row.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                            self.selected = i;
                        }
                        if i == self.selected && std::mem::take(&mut self.scroll) {
                            ui.scroll_to_rect(rect, None);
                        }
                        let pt = ui.painter();
                        if i == self.selected {
                            pt.rect_filled(rect, 0, p.accent_soft);
                        }
                        pt.hline(
                            rect.x_range(),
                            rect.bottom() - 0.5,
                            Stroke::new(1.0, p.border),
                        );
                        // The checkbox, then authors, year and the key at the right.
                        let bx = Rect::from_min_size(
                            pos2(rect.left() + 12.0, rect.top() + 9.0),
                            vec2(13.0, 13.0),
                        );
                        let on = self.picked.contains(&e.key);
                        let check = ui.interact(
                            bx.expand(3.0),
                            ui.scope_id().with(("pick", &e.key)),
                            Sense::click(),
                        );
                        check.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::Role::CheckBox,
                                true,
                                on,
                                format!("Select {}", e.key),
                            )
                        });
                        if on {
                            pt.rect_filled(bx, CornerRadius::same(3), p.accent);
                            icons::paint(pt, bx.shrink(2.0), icons::CHECK, p.accent_ink);
                        } else {
                            pt.rect(
                                bx,
                                CornerRadius::same(3),
                                p.surface,
                                Stroke::new(1.0, p.faint),
                                StrokeKind::Inside,
                            );
                        }
                        let mut x = bx.right() + 8.0;
                        let y = rect.top() + 7.0;
                        let who = if e.authors.is_empty() {
                            "Unknown author"
                        } else {
                            &e.authors
                        };
                        let key_g =
                            pt.layout_no_wrap(e.key.clone(), FontId::monospace(11.0), p.muted);
                        let key_x = rect.right() - 12.0 - key_g.size().x;
                        let g = pt.layout(
                            who.to_owned(),
                            FontId::new(13.0, theme::font::sans_600()),
                            p.ink,
                            key_x - x - 60.0,
                        );
                        let w = g.size().x;
                        pt.galley(pos2(x, y), g, p.ink);
                        x += w + 8.0;
                        if !e.year.is_empty() {
                            pt.text(
                                pos2(x, y),
                                egui::Align2::LEFT_TOP,
                                &e.year,
                                FontId::proportional(13.0),
                                p.muted,
                            );
                        }
                        pt.galley(pos2(key_x, y + 3.0), key_g, p.muted);
                        let title = if e.title.is_empty() {
                            "(no title)"
                        } else {
                            &e.title
                        };
                        let tg = pt.layout(
                            title.to_owned(),
                            FontId::proportional(12.0),
                            p.muted,
                            f32::INFINITY,
                        );
                        let clip = pt.with_clip_rect(Rect::from_min_max(
                            pos2(rect.left(), rect.top()),
                            pos2(rect.right() - 12.0, rect.bottom()),
                        ));
                        clip.galley(pos2(rect.left() + 12.0, rect.top() + 28.0), tg, p.muted);
                        if check.clicked() {
                            self.toggle(&e.key);
                        } else if row.clicked() {
                            *out = Some(Cite::Insert(vec![e.key.clone()]));
                        }
                    }
                });
        });
    }

    fn online_part(&mut self, ui: &mut Ui, p: &Palette, out: &mut Option<Cite>) {
        let few = self.results().len() < 3;
        let q = self.query.trim().to_owned();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let label = if self.searching {
                "Searching…"
            } else {
                "Search online"
            };
            if Button::new(label)
                .size(Size::Sm)
                .kind(if few { Kind::Primary } else { Kind::Default })
                .enabled(!self.searching)
                .show(ui, p)
                .clicked()
            {
                *out = Some(Cite::SearchOnline);
            }
            ui.label(
                RichText::new(format!(
                    "{}Sends “{q}” to Crossref, only when you click.",
                    if few { "Few matches here. " } else { "" }
                ))
                .size(11.0)
                .color(p.muted),
            );
        });
        if !self.online_error.is_empty() {
            ui.label(RichText::new(&self.online_error).color(p.error));
        }
        let Some(hits) = &self.online else { return };
        if hits.is_empty() {
            ui.label(RichText::new(format!("Crossref found nothing for “{q}”.")).color(p.muted));
            return;
        }
        let max_h = ui.ctx().content_rect().height() * 0.28;
        let busy = !self.adding_doi.is_empty();
        list_frame(ui, p, max_h, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("cite-online")
                .max_height(max_h)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for h in hits {
                        let r = egui::Frame::new()
                            .inner_margin(Margin::symmetric(12, 6))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let w = ui.available_width() - 70.0;
                                    ui.allocate_ui_with_layout(
                                        vec2(w, 0.0),
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| {
                                            ui.spacing_mut().item_spacing.y = 2.0;
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 8.0;
                                                ui.label(
                                                    RichText::new(if h.authors.is_empty() {
                                                        "Unknown author"
                                                    } else {
                                                        &h.authors
                                                    })
                                                    .font(FontId::new(
                                                        13.0,
                                                        theme::font::sans_600(),
                                                    ))
                                                    .color(p.ink),
                                                );
                                                if !h.year.is_empty() {
                                                    ui.label(RichText::new(&h.year).color(p.muted));
                                                }
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        ui.label(
                                                            RichText::new(&h.doi)
                                                                .font(FontId::monospace(11.0))
                                                                .color(p.muted),
                                                        );
                                                    },
                                                );
                                            });
                                            let mut t = if h.title.is_empty() {
                                                "(no title)".to_owned()
                                            } else {
                                                h.title.clone()
                                            };
                                            if !h.venue.is_empty() {
                                                t += &format!(" · {}", h.venue);
                                            }
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(t).size(12.0).color(p.muted),
                                                )
                                                .wrap(),
                                            );
                                        },
                                    );
                                    let label = if self.adding_doi == h.doi {
                                        "Adding…"
                                    } else {
                                        "Add"
                                    };
                                    if Button::new(label)
                                        .size(Size::Sm)
                                        .enabled(!busy)
                                        .show(ui, p)
                                        .on_hover_text(format!(
                                            "Adds it to {} and inserts the citation",
                                            self.add_file
                                        ))
                                        .clicked()
                                    {
                                        *out = Some(Cite::AddHit(h.doi.clone()));
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
                });
        });
        ui.label(
            RichText::new(format!(
                "Add puts the reference in {} (change the file below) and inserts the citation.",
                self.add_file
            ))
            .size(11.0)
            .color(p.muted),
        );
    }

    fn add_form(&mut self, ui: &mut Ui, p: &Palette, out: &mut Option<Cite>) {
        let top = ui.cursor().top();
        ui.painter().hline(
            ui.max_rect().x_range(),
            top - 0.5,
            Stroke::new(1.0, p.border),
        );
        ui.add_space(10.0);
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.label(
            RichText::new("ADD A REFERENCE BY DOI OR ARXIV ID")
                .font(FontId::new(11.0, theme::font::mono_500()))
                .color(p.muted),
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let files = if self.files.is_empty() {
                vec!["refs.bib".to_owned()]
            } else {
                self.files.clone()
            };
            let add_w = 60.0;
            let sel_w = 150.0;
            let w = ui.available_width() - add_w - sel_w - 16.0;
            let mut submit = false;
            ui.allocate_ui(vec2(w, 32.0), |ui| {
                let r = widgets::text_input(
                    ui,
                    p,
                    &mut self.add_id,
                    "10.1038/nature14539 or arXiv:1707.06347",
                    false,
                );
                ui.ctx().accesskit_node_builder(r.id, |n| {
                    n.set_label("Add a reference by DOI or arXiv ID")
                });
                submit = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            });
            let opts: Vec<(&str, &str)> = files.iter().map(|f| (f.as_str(), f.as_str())).collect();
            ui.allocate_ui(vec2(sel_w, 32.0), |ui| {
                widgets::select(ui, p, "Bibliography file", &mut self.add_file, &opts)
            });
            let label = if self.adding { "Looking up…" } else { "Add" };
            let ok = !self.adding && !self.add_id.trim().is_empty();
            if (Button::new(label).enabled(ok).show(ui, p).clicked() || submit) && ok {
                *out = Some(Cite::AddReference);
            }
        });
        if let Some((ok, text)) = &self.add_msg {
            ui.label(RichText::new(text).color(if *ok { p.ok } else { p.error }));
        }
        ui.label(
            RichText::new("Looks the reference up on doi.org. This is the only time Lemmata goes online for references.")
                .size(11.0)
                .color(p.muted),
        );
    }
}

/// A bordered list box (`ul` with --border and --r-lg), up to `max_h` tall. The room is given
/// explicitly: a modal's area only offers the height it had last frame, so the list could never grow.
fn list_frame(ui: &mut Ui, p: &Palette, max_h: f32, add: impl FnOnce(&mut Ui)) {
    let w = ui.available_width();
    ui.allocate_ui(vec2(w, max_h + 2.0), |ui| {
        egui::Frame::new()
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(theme::R_LG)
            .inner_margin(Margin::same(1))
            .show(ui, |ui| {
                ui.set_width(w - 2.0);
                add(ui);
            });
    });
}

// ── The bibliography manager ───────────────────────────────────────────────────

pub struct BibManager {
    lib: Option<Library>,
    loading: bool,
    busy: bool,
    error: String,
    query: String,
    sort: String,
    show: String,
    /// `file:line` of the entry asking "Delete entry / Keep".
    confirming: String,
    renaming: Option<(String, String)>,
    expanded: HashSet<String>,
}

enum Manage {
    Open(String, usize),
    Remove(usize),
    Rename(usize, String),
}

fn entry_id(e: &ManagedEntry) -> String {
    format!("{}:{}", e.entry.file, e.line)
}

impl BibManager {
    fn show(&mut self, ctx: &egui::Context, p: &Palette) -> (bool, Option<Manage>) {
        let mut out = None;
        let r = widgets::modal_placed(
            ctx,
            p,
            "Bibliography",
            Some("Every reference in your .bib files, with duplicates and unused entries."),
            880.0,
            false,
            |ui| self.body(ui, p, &mut out),
            None,
        );
        (r.closed, out)
    }

    fn describe(lib: &Library, kind: &str, others: &[usize]) -> String {
        let names = others
            .iter()
            .map(|&j| &lib.entries[j])
            .map(|o| format!("{} ({}:{})", o.entry.key, o.entry.file, o.line))
            .collect::<Vec<_>>()
            .join(", ");
        match kind {
            "key" => format!("Same key as {names}"),
            "doi" => format!("Same DOI as {names}"),
            _ => format!("Same title as {names}"),
        }
    }

    fn body(&mut self, ui: &mut Ui, p: &Palette, out: &mut Option<Manage>) {
        ui.spacing_mut().item_spacing.y = 10.0;
        let Some(lib) = self.lib.take() else {
            if self.loading {
                ui.label(RichText::new("Reading the bibliography…").color(p.muted));
            } else if !self.error.is_empty() {
                ui.label(RichText::new(&self.error).color(p.error));
            }
            return;
        };
        let n = lib.entries.len();
        let uncited = lib.entries.iter().filter(|e| !e.cited).count();
        let plural = |n: usize, one: &str, many: &str| {
            if n == 1 {
                one.to_owned()
            } else {
                many.to_owned()
            }
        };
        ui.label(
            RichText::new(format!(
                "{n} {} in {} {} · {} possible {} · {uncited} not cited",
                plural(n, "entry", "entries"),
                lib.files.len(),
                plural(lib.files.len(), "file", "files"),
                lib.duplicates.len(),
                plural(lib.duplicates.len(), "duplicate", "duplicates"),
            ))
            .color(p.muted),
        );
        if !lib.missing.is_empty() {
            widgets::Rich::new()
                .add(
                    "Cited but not in any .bib file: ",
                    FontId::proportional(13.0),
                    p.error,
                )
                .add(&lib.missing.join(", "), FontId::monospace(12.0), p.error)
                .show(ui);
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let w = ui.available_width() - 2.0 * 150.0 - 16.0;
            ui.allocate_ui(vec2(w, 32.0), |ui| {
                let r = widgets::text_input(
                    ui,
                    p,
                    &mut self.query,
                    "Search by author, title, year, key or DOI",
                    false,
                );
                ui.ctx()
                    .accesskit_node_builder(r.id, |n| n.set_label("Search the bibliography"));
            });
            ui.allocate_ui(vec2(150.0, 32.0), |ui| {
                widgets::select(
                    ui,
                    p,
                    "Show",
                    &mut self.show,
                    &[
                        ("all", "All entries"),
                        ("duplicates", "Duplicates"),
                        ("uncited", "Not cited"),
                    ],
                )
            });
            ui.allocate_ui(vec2(150.0, 32.0), |ui| {
                widgets::select(
                    ui,
                    p,
                    "Sort by",
                    &mut self.sort,
                    &[
                        ("key", "Sort by key"),
                        ("author", "Sort by author"),
                        ("year", "Sort by year"),
                    ],
                )
            });
        });
        if !self.error.is_empty() {
            ui.label(RichText::new(&self.error).color(p.error));
        }
        let warnings = warnings_by_entry(&lib.duplicates);
        let mut rows: Vec<usize> = (0..n)
            .filter(|&i| {
                let e = &lib.entries[i];
                matches(
                    &format!(
                        "{} {} {} {} {}",
                        e.entry.key, e.entry.authors, e.entry.title, e.entry.year, e.doi
                    ),
                    &self.query,
                ) && match self.show.as_str() {
                    "duplicates" => warnings.contains_key(&i),
                    "uncited" => !e.cited,
                    _ => true,
                }
            })
            .collect();
        sort_entries(&lib.entries, &mut rows, &self.sort);
        let max_h = ui.ctx().content_rect().height() * 0.52;
        list_frame(ui, p, max_h, |ui| {
            egui::ScrollArea::vertical().id_salt("bib-list").max_height(max_h).min_scrolled_height(80.0f32.min(max_h)).auto_shrink([false, true]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let empty = if n == 0 {
                    Some("No entries yet. Add references from the citation picker.")
                } else if rows.is_empty() {
                    Some("Nothing matches.")
                } else {
                    None
                };
                if let Some(t) = empty {
                    egui::Frame::new().inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.set_min_height(58.0);
                        ui.label(RichText::new(t).color(p.muted));
                    });
                }
                for (k, &i) in rows.iter().enumerate() {
                    let e = &lib.entries[i];
                    let id = entry_id(e);
                    let ws = warnings.get(&i).map(Vec::as_slice).unwrap_or(&[]);
                    let r = egui::Frame::new().inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;
                            let main_w = ui.available_width() - 270.0 - 12.0;
                            ui.allocate_ui_with_layout(vec2(main_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.set_width(main_w);
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    let who = if e.entry.authors.is_empty() { "Unknown author" } else { &e.entry.authors };
                                    ui.label(RichText::new(who).font(FontId::new(13.0, theme::font::sans_600())).color(p.ink));
                                    if !e.entry.year.is_empty() {
                                        ui.label(RichText::new(&e.entry.year).color(p.muted));
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(RichText::new(&e.entry.key).font(FontId::monospace(11.0)).color(p.muted));
                                    });
                                });
                                let title = if e.entry.title.is_empty() { "(no title)" } else { &e.entry.title };
                                ui.add(egui::Label::new(RichText::new(title).size(12.0).color(p.muted)).truncate());
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    ui.label(RichText::new(format!("{}:{} · {}", e.entry.file, e.line, e.entry.kind)).size(11.0).color(p.muted));
                                    if !e.cited {
                                        tag(ui, p, "Not cited", false);
                                    }
                                    for (kind, others) in ws {
                                        let t = match *kind {
                                            "key" => "Duplicate key",
                                            "doi" => "Same DOI",
                                            _ => "Same title",
                                        };
                                        tag(ui, p, t, true).on_hover_text(Self::describe(&lib, kind, others));
                                    }
                                });
                                for (kind, others) in ws {
                                    ui.label(RichText::new(Self::describe(&lib, kind, others)).size(11.0).color(p.error));
                                }
                                if self.expanded.contains(&id) {
                                    egui::Grid::new(("fields", &id)).num_columns(2).spacing(vec2(10.0, 2.0)).show(ui, |ui| {
                                        for (name, value) in &e.fields {
                                            ui.label(RichText::new(name).font(FontId::monospace(12.0)).color(p.muted));
                                            ui.add(egui::Label::new(RichText::new(value).size(12.0).color(p.ink)).wrap());
                                            ui.end_row();
                                        }
                                    });
                                }
                                if let Some((rid, value)) = &mut self.renaming
                                    && *rid == id
                                {
                                    let mut cancel = false;
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        let mut go = false;
                                        ui.allocate_ui(vec2(180.0, 28.0), |ui| {
                                            let r = widgets::text_input(ui, p, value, "", true);
                                            ui.ctx().accesskit_node_builder(r.id, |n| n.set_label("New key"));
                                            go = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                                        });
                                        if Button::new("Rename").size(Size::Sm).enabled(!self.busy).show(ui, p).clicked() || go {
                                            *out = Some(Manage::Rename(i, value.trim().to_owned()));
                                        }
                                        if Button::new("Cancel").size(Size::Sm).kind(Kind::Ghost).show(ui, p).clicked() {
                                            cancel = true;
                                        }
                                        ui.label(RichText::new("Citations in your .tex files are not changed.").size(11.0).color(p.muted));
                                    });
                                    if cancel {
                                        self.renaming = None;
                                    }
                                }
                            });
                            // .actions: at most 270 px, wrapping.
                            ui.allocate_ui_with_layout(vec2(270.0, 0.0), egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true), |ui| {
                                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                                if Button::new("Open").size(Size::Sm).show(ui, p).clicked() {
                                    *out = Some(Manage::Open(e.entry.file.clone(), e.line));
                                }
                                let open = self.expanded.contains(&id);
                                if Button::new(if open { "Hide fields" } else { "Fields" }).size(Size::Sm).kind(Kind::Ghost).show(ui, p).clicked() {
                                    if open {
                                        self.expanded.remove(&id);
                                    } else {
                                        self.expanded.insert(id.clone());
                                    }
                                }
                                if ws.iter().any(|w| w.0 == "key")
                                    && Button::new("Rename key…").size(Size::Sm).enabled(!self.busy).show(ui, p).clicked()
                                {
                                    let taken: Vec<&str> = lib.entries.iter().map(|x| x.entry.key.as_str()).collect();
                                    self.renaming = Some((id.clone(), free_key(&e.entry.key, &taken)));
                                    self.confirming.clear();
                                }
                                if self.confirming == id {
                                    if Button::new("Delete entry").size(Size::Sm).kind(Kind::Danger).enabled(!self.busy).show(ui, p).clicked() {
                                        *out = Some(Manage::Remove(i));
                                    }
                                    if Button::new("Keep").size(Size::Sm).kind(Kind::Ghost).show(ui, p).clicked() {
                                        self.confirming.clear();
                                    }
                                } else if Button::new("Delete…").size(Size::Sm).kind(Kind::Ghost).enabled(!self.busy).show(ui, p).clicked() {
                                    self.confirming = id.clone();
                                }
                            });
                        });
                    });
                    if k + 1 < rows.len() {
                        let b = r.response.rect;
                        ui.painter().hline(b.x_range(), b.bottom() - 0.5, Stroke::new(1.0, p.border));
                    }
                }
            });
        });
        self.lib = Some(lib);
    }
}

/// `.tag`: a pill with a border; red for a warning.
fn tag(ui: &mut Ui, p: &Palette, text: &str, warn: bool) -> egui::Response {
    let c = if warn { p.error } else { p.muted };
    let g = ui
        .painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(11.0), c);
    let (rect, r) = ui.allocate_exact_size(g.size() + vec2(14.0, 2.0), Sense::hover());
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(255),
        Stroke::new(1.0, if warn { p.error } else { p.border_strong }),
        StrokeKind::Inside,
    );
    ui.painter().galley(rect.min + vec2(7.0, 1.0), g, c);
    r
}

// ── Wiring ─────────────────────────────────────────────────────────────────────

impl Workspace {
    /// Opens the citation picker; `search_for` (from a hover card) is looked up online at once.
    pub fn open_cite(&mut self, backend: &Backend, search_for: Option<String>) {
        let mut c = CitePicker {
            loading: true,
            add_file: "refs.bib".into(),
            ..Default::default()
        };
        let online = search_for.is_some();
        c.query = search_for.unwrap_or_default();
        self.cite = Some(c);
        self.load_cite(backend);
        Workspace::job_async(
            backend,
            |s| async move { s.zotero_status().await },
            |ws, _, z| {
                if let Some(c) = &mut ws.cite {
                    c.zotero = Some(z);
                }
            },
        );
        if online {
            self.cite_online(backend);
        }
    }

    fn load_cite(&self, backend: &Backend) {
        Workspace::job(
            backend,
            |s| s.bib_entries(),
            |ws, _, r| {
                let Some(c) = &mut ws.cite else { return };
                c.loading = false;
                match r {
                    Ok(b) => {
                        if let Some(first) = b.files.first()
                            && !b.files.contains(&c.add_file)
                        {
                            c.add_file = first.clone();
                        }
                        c.entries = b.entries;
                        c.files = b.files;
                    }
                    Err(e) => c.add_msg = Some((false, e)),
                }
            },
        );
    }

    /// Goes online only on this click, never while typing.
    fn cite_online(&mut self, backend: &Backend) {
        let Some(c) = &mut self.cite else { return };
        c.searching = true;
        c.online_error.clear();
        c.online = None;
        let q = c.query.clone();
        Workspace::job_async(
            backend,
            move |s| async move { s.bib_search_online(q).await },
            |ws, _, r| {
                let Some(c) = &mut ws.cite else { return };
                c.searching = false;
                match r {
                    Ok(h) => c.online = Some(h),
                    Err(e) => c.online_error = e,
                }
            },
        );
    }

    /// Saves unsaved .bib files, then runs `then` when the writes are done (beforeBibChange), so a
    /// change on disk doesn't lose them.
    fn after_bib_saved(
        &mut self,
        backend: &Backend,
        settings: &Settings,
        then: impl FnOnce(&mut Workspace, &Backend) + 'static,
    ) {
        for p in self
            .buffers
            .unsaved()
            .into_iter()
            .filter(|p| p.ends_with(".bib"))
        {
            self.no_build.insert(p.clone());
            self.write(backend, &p, settings);
        }
        self.after_writes.push(Box::new(then));
    }

    /// A .bib file changed on disk (onBibChanged): read it again, and the citation keys.
    fn bib_changed(&mut self, backend: &Backend, files: &[String]) {
        for f in files {
            self.reload(backend, f);
        }
        backend.spawn(|shell| Msg::Info(shell.project_info_cmd()));
    }

    fn insert_citation(&mut self, keys: &[String]) {
        if let Some(ed) = self.active_editor() {
            ed.insert_citation(keys);
        }
    }

    pub(super) fn bib_dialogs(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        backend: &Backend,
        settings: &Settings,
    ) {
        if let Some(c) = &mut self.cite {
            let (closed, act) = c.show(ctx, p);
            if closed {
                self.cite = None;
                if let Some(ed) = self.active_editor() {
                    ed.refocus = true;
                }
            }
            match act {
                None => {}
                Some(Cite::Insert(keys)) => {
                    self.cite = None;
                    self.insert_citation(&keys);
                }
                Some(Cite::SearchOnline) => self.cite_online(backend),
                Some(Cite::AddHit(doi)) => {
                    let Some(c) = &mut self.cite else { return };
                    c.adding_doi = doi.clone();
                    c.online_error.clear();
                    let file = c.add_file.clone();
                    self.after_bib_saved(backend, settings, move |ws, b| {
                        Workspace::job_async(
                            b,
                            move |s| async move { s.bib_add_reference(doi, file).await },
                            |ws, b, r| {
                                let Some(c) = &mut ws.cite else { return };
                                c.adding_doi.clear();
                                match r {
                                    Ok(a) => {
                                        ws.cite = None;
                                        ws.bib_changed(b, std::slice::from_ref(&a.file));
                                        ws.insert_citation(&[a.key]);
                                    }
                                    Err(e) => c.online_error = e,
                                }
                            },
                        );
                        let _ = ws;
                    });
                }
                Some(Cite::AddReference) => {
                    let Some(c) = &mut self.cite else { return };
                    c.adding = true;
                    c.add_msg = None;
                    let (id, file) = (c.add_id.trim().to_owned(), c.add_file.clone());
                    self.after_bib_saved(backend, settings, move |_, b| {
                        Workspace::job_async(
                            b,
                            move |s| async move { s.bib_add_reference(id, file).await },
                            |ws, b, r| {
                                let Some(c) = &mut ws.cite else { return };
                                c.adding = false;
                                match r {
                                    Ok(a) => {
                                        c.add_msg =
                                            Some((true, format!("Added {} to {}.", a.key, a.file)));
                                        c.add_id.clear();
                                        c.query = a.key;
                                        c.selected = 0;
                                        c.search_id = None;
                                        ws.bib_changed(b, std::slice::from_ref(&a.file));
                                        ws.load_cite(b);
                                    }
                                    Err(e) => c.add_msg = Some((false, e)),
                                }
                            },
                        );
                    });
                }
                Some(Cite::Zotero) => {
                    self.cite = None;
                    self.cite_from_zotero(backend);
                }
            }
        }
        let Some(m) = &mut self.bib_manager else {
            return;
        };
        let (closed, act) = m.show(ctx, p);
        let entry = match &act {
            Some(Manage::Remove(i) | Manage::Rename(i, _)) => {
                m.lib.as_ref().map(|l| l.entries[*i].clone())
            }
            _ => None,
        };
        if let Some(Manage::Rename(_, value)) = &act
            && !valid_key(value)
        {
            m.error = "A key can't be empty or contain spaces, commas, braces or quotes.".into();
            return;
        }
        if closed {
            self.bib_manager = None;
        }
        match (act, entry) {
            (Some(Manage::Open(file, line)), _) => {
                self.bib_manager = None;
                self.goto(backend, &file, line);
            }
            (Some(Manage::Remove(_)), Some(e)) => {
                self.bib_change(backend, settings, move |s| {
                    s.bib_remove_entry(e.entry.file, e.entry.key, e.line)
                });
            }
            (Some(Manage::Rename(_, value)), Some(e)) => {
                self.bib_change(backend, settings, move |s| {
                    s.bib_rename_key(e.entry.file, e.entry.key, e.line, value)
                });
            }
            _ => {}
        }
    }

    pub fn open_bib_manager(&mut self, backend: &Backend, settings: &Settings) {
        self.bib_manager = Some(BibManager {
            lib: None,
            loading: true,
            busy: false,
            error: String::new(),
            query: String::new(),
            sort: "key".into(),
            show: "all".into(),
            confirming: String::new(),
            renaming: None,
            expanded: HashSet::new(),
        });
        // Line numbers must match what is on disk.
        self.after_bib_saved(backend, settings, |ws, b| ws.load_library(b));
    }

    fn load_library(&self, backend: &Backend) {
        Workspace::job(
            backend,
            |s| s.bib_library(),
            |ws, _, r| {
                let Some(m) = &mut ws.bib_manager else { return };
                m.loading = false;
                m.busy = false;
                match r {
                    Ok(l) => m.lib = Some(l),
                    Err(e) => m.error = e,
                }
            },
        );
    }

    fn bib_change(
        &mut self,
        backend: &Backend,
        settings: &Settings,
        run: impl FnOnce(&lemmata_shell::Shell) -> Result<Vec<String>, String> + Send + 'static,
    ) {
        let Some(m) = &mut self.bib_manager else {
            return;
        };
        m.busy = true;
        m.error.clear();
        self.after_bib_saved(backend, settings, move |_, b| {
            Workspace::job(b, run, |ws, b, r| {
                match r {
                    Ok(paths) => {
                        ws.bib_changed(b, &paths);
                        if let Some(m) = &mut ws.bib_manager {
                            m.confirming.clear();
                            m.renaming = None;
                        }
                    }
                    Err(e) => {
                        if let Some(m) = &mut ws.bib_manager {
                            m.error = e;
                        }
                    }
                }
                // Lines moved (or the entry was stale): show what is on disk now.
                ws.load_library(b);
            });
        });
    }

    pub(super) fn cite_from_zotero(&mut self, backend: &Backend) {
        self.notice("Choose references in Zotero's picker…", Tone::Info);
        Workspace::job_async(
            backend,
            |s| async move { s.zotero_pick().await },
            |ws, b, r| match r {
                Ok(keys) if keys.is_empty() => {}
                Ok(keys) => {
                    ws.insert_citation(&keys);
                    // The citation must be on disk before zotero.bib is written for it.
                    for p in ws.buffers.unsaved() {
                        ws.no_build.insert(p.clone());
                        // The default settings only set the retry delay of a write already under way.
                        ws.write(b, &p, &Settings::default());
                    }
                    ws.after_writes
                        .push(Box::new(|ws, b| ws.sync_zotero(b, false)));
                }
                Err(e) => ws.notice(e, Tone::Error),
            },
        );
    }

    /// Writes zotero.bib for the cited keys; `report` shows a summary even when all went well.
    pub fn sync_zotero(&mut self, backend: &Backend, report: bool) {
        Workspace::job_async(
            backend,
            |s| async move { s.zotero_sync().await },
            move |ws, b, r| match r {
                Ok(r) => {
                    ws.bib_changed(b, &[lemmata_shell::bib::ZOTERO_BIB.to_owned()]);
                    if !r.linked {
                        ws.notices.push(
                            Notice::new("Your document doesn't load zotero.bib yet.", Tone::Info)
                                .action(NoticeAction::LinkZotero),
                        );
                    } else if !r.missing.is_empty() {
                        ws.notice(
                            format!("Not in Zotero: {}.", r.missing.join(", ")),
                            Tone::Info,
                        );
                    } else if report {
                        let s = if r.written == 1 { "" } else { "s" };
                        ws.notice(
                            format!("zotero.bib has {} reference{s}.", r.written),
                            Tone::Info,
                        );
                    }
                    ws.build_soon(false, false);
                }
                Err(e) => ws.notice(e, Tone::Error),
            },
        );
    }

    /// "Add it": loads zotero.bib in the main file.
    pub(super) fn link_zotero(&mut self, backend: &Backend, settings: &Settings) {
        let main = self.info.main.clone();
        if self.buffers.unsaved().contains(&main) {
            self.no_build.insert(main.clone());
            self.write(backend, &main, settings);
        }
        self.after_writes.push(Box::new(move |_, b| {
            Workspace::job(
                b,
                |s| s.zotero_link(),
                move |ws, b, r| match r {
                    Ok(()) => {
                        ws.reload(b, &main);
                        ws.build_soon(false, false);
                    }
                    Err(e) => ws.notice(e, Tone::Error),
                },
            );
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(key: &str, authors: &str, year: &str, title: &str) -> ManagedEntry {
        ManagedEntry {
            entry: Entry {
                key: key.into(),
                kind: "misc".into(),
                title: title.into(),
                authors: authors.into(),
                year: year.into(),
                file: "refs.bib".into(),
            },
            line: 1,
            doi: String::new(),
            fields: Default::default(),
            cited: false,
        }
    }

    // bib/manager.test.ts
    #[test]
    fn filters_sorts_and_finds_keys() {
        let list = [
            e("b", "Zed", "2001", "Graphs"),
            e("a", "", "", "Nothing"),
            e("c", "Abe", "2020", "Proofs"),
        ];
        let hay = |x: &ManagedEntry| {
            format!(
                "{} {} {} {}",
                x.entry.key, x.entry.authors, x.entry.title, x.entry.year
            )
        };
        assert_eq!(
            list.iter().filter(|x| matches(&hay(x), "abe 2020")).count(),
            1
        );
        assert!(list.iter().all(|x| matches(&hay(x), "")));
        let sorted = |by: &str| {
            let mut idx = vec![0, 1, 2];
            sort_entries(&list, &mut idx, by);
            idx.iter()
                .map(|&i| list[i].entry.key.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(sorted("key"), ["a", "b", "c"]);
        assert_eq!(sorted("author"), ["c", "b", "a"]);
        assert_eq!(sorted("year"), ["c", "b", "a"]);
        let w = warnings_by_entry(&[Duplicate {
            kind: "doi",
            value: "x".into(),
            entries: vec![0, 2],
        }]);
        assert_eq!(w[&0], [("doi", vec![2])]);
        assert!(!w.contains_key(&1));
        assert_eq!(free_key("k", &["K", "ka"]), "kb");
        assert!(valid_key("smith2020a"));
        assert!(!valid_key("bad key") && !valid_key(""));
    }
}
