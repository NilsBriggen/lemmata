//! Review comments (comments/model.ts and ReviewPanel.svelte): threads anchored to a quoted piece
//! of text in a file, kept in `.lemmata/comments.json`, and the panel that lists them.

use super::bar;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind, Size};
use eframe::egui::{self, FontId, Margin, Stroke, Ui, vec2};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const COMMENTS_FILE: &str = ".lemmata/comments.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub body: String,
    pub created: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub file: String,
    /// 1-based line where the quote started when last seen.
    pub line: usize,
    /// The commented text; empty means the whole line.
    pub quote: String,
    pub body: String,
    pub created: f64,
    pub resolved: bool,
    pub replies: Vec<Reply>,
}

/// Threads from the file, skipping damaged entries.
pub fn parse(json: &str) -> Vec<Thread> {
    let Ok(data) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let list = match &data {
        Value::Array(a) => a.clone(),
        v => v
            .get("threads")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };
    list.iter()
        .filter_map(|t| {
            Some(Thread {
                id: t.get("id")?.as_str()?.to_owned(),
                file: t.get("file")?.as_str()?.to_owned(),
                body: t.get("body")?.as_str()?.to_owned(),
                line: t
                    .get("line")
                    .and_then(Value::as_i64)
                    .filter(|l| *l > 0)
                    .unwrap_or(1) as usize,
                quote: t
                    .get("quote")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                created: t.get("created").and_then(Value::as_f64).unwrap_or(0.0),
                resolved: t.get("resolved").and_then(Value::as_bool) == Some(true),
                replies: t
                    .get("replies")
                    .and_then(Value::as_array)
                    .map(|r| {
                        r.iter()
                            .filter_map(|r| serde_json::from_value(r.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default(),
            })
        })
        .collect()
}

pub fn serialize(threads: &[Thread]) -> String {
    serde_json::to_string_pretty(&serde_json::json!({ "version": 1, "threads": threads }))
        .unwrap_or_default()
        + "\n"
}

pub fn new_id(now_ms: f64) -> String {
    let rnd = std::collections::hash_map::RandomState::new();
    let r = std::hash::BuildHasher::hash_one(&rnd, now_ms.to_bits()) % 2_176_782_336; // 36^6
    format!("{}-{:0>6}", radix36(now_ms as u64), radix36(r))
}

fn radix36(mut n: u64) -> String {
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    loop {
        out.push(digits[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

/// Where a thread's quote is in `text` now (chars): the copy nearest its last known line; a quote
/// that's gone falls back to its line, if the file still has it.
pub fn locate(text: &str, line: usize, quote: &str) -> Option<(usize, usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut starts = vec![0usize];
    for (i, c) in chars.iter().enumerate() {
        if *c == '\n' {
            starts.push(i + 1);
        }
    }
    let line_of = |pos: usize| starts.partition_point(|&s| s <= pos);
    if !quote.is_empty() {
        let q: Vec<char> = quote.chars().collect();
        let mut best: Option<usize> = None;
        for at in 0..chars.len().saturating_sub(q.len() - 1) {
            if chars[at..at + q.len()] == q[..] {
                let d = |p: usize| (line_of(p) as isize - line as isize).abs();
                if best.is_none_or(|b| d(at) < d(b)) {
                    best = Some(at);
                }
            }
        }
        if let Some(b) = best {
            return Some((b, b + q.len(), line_of(b)));
        }
    }
    if line > starts.len() {
        return None;
    }
    let from = starts[line - 1];
    let end = if line < starts.len() {
        starts[line] - 1
    } else {
        chars.len()
    };
    Some((from, end.max(from), line))
}

/// What the panel asks of the workspace.
pub enum Action {
    Add(String),
    CancelDraft,
    Select(Thread),
    Reply(String, String),
    Resolve(String, bool),
    Delete(String),
    Close,
}

#[derive(Default)]
pub struct Review {
    show_resolved: bool,
    text: String,
    replies: std::collections::HashMap<String, String>,
    confirm_delete: Option<String>,
}

fn excerpt(q: &str) -> String {
    if q.chars().count() > 160 {
        q.chars().take(160).collect::<String>() + "…"
    } else {
        q.to_owned()
    }
}

impl Review {
    pub fn show(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        threads: &[Thread],
        active: Option<&str>,
        draft: Option<(&str, usize, &str)>,
    ) -> Vec<Action> {
        let mut out = Vec::new();
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, p.surface_2);
        let open = threads.iter().filter(|t| !t.resolved).count();
        let body = bar(ui, p, "Comments", "Close comments", &mut |ui| {
            ui.label(
                egui::RichText::new(format!("{open} open"))
                    .size(12.0)
                    .color(p.muted),
            );
            ui.add_space(10.0);
            widgets::checkbox(ui, p, &mut self.show_resolved, "Show resolved");
        });
        if body.closed {
            out.push(Action::Close);
        }
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(body.rest));
        let mut shown: Vec<&Thread> = threads
            .iter()
            .filter(|t| self.show_resolved || !t.resolved)
            .collect();
        shown.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
        egui::ScrollArea::vertical().id_salt("review").auto_shrink([false, false]).show(&mut c, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let item = |ui: &mut Ui, fill: egui::Color32, add: &mut dyn FnMut(&mut Ui)| {
                let r = egui::Frame::new().fill(fill).inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 4.0;
                    add(ui);
                });
                ui.painter().hline(r.response.rect.x_range(), r.response.rect.bottom() - 0.5, Stroke::new(1.0, p.border));
            };
            let quote = |ui: &mut Ui, q: &str| {
                let r = egui::Frame::NONE.inner_margin(Margin { left: 11, right: 8, top: 2, bottom: 2 }).show(ui, |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(excerpt(q)).font(FontId::monospace(12.0)).color(p.ink)).wrap());
                });
                let b = r.response.rect;
                ui.painter().vline(b.left() + 1.5, b.y_range(), Stroke::new(3.0, egui::Color32::from_rgba_unmultiplied(220, 160, 20, 179)));
            };
            if let Some((file, line, q)) = draft {
                let mut add = false;
                let mut cancel = false;
                item(ui, p.surface, &mut |ui| {
                    ui.label(egui::RichText::new(format!("{file}:{line}")).font(FontId::monospace(11.5)).color(p.muted));
                    quote(ui, q);
                    let r = widgets::text_area(ui, p, &mut self.text, "Write a comment", 3, true);
                    if !r.has_focus() && self.text.is_empty() {
                        r.request_focus();
                    }
                    if r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command) {
                        add = true;
                    }
                    if r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if Button::new("Comment").kind(Kind::Primary).size(Size::Sm).enabled(!self.text.trim().is_empty()).show(ui, p).clicked() {
                            add = true;
                        }
                        if Button::new("Cancel").size(Size::Sm).show(ui, p).clicked() {
                            cancel = true;
                        }
                    });
                });
                if add && !self.text.trim().is_empty() {
                    out.push(Action::Add(self.text.trim().to_owned()));
                    self.text.clear();
                }
                if cancel {
                    out.push(Action::CancelDraft);
                }
            }
            if shown.is_empty() && draft.is_none() {
                egui::Frame::NONE.inner_margin(Margin::symmetric(14, 12)).show(ui, |ui| {
                    let t = if threads.is_empty() {
                        "No comments yet. Select some text and choose Add comment (in the editor toolbar or the command palette)."
                    } else {
                        "No open comments."
                    };
                    ui.add(egui::Label::new(egui::RichText::new(t).color(p.muted)).wrap());
                });
            }
            for t in shown {
                let is_active = active == Some(t.id.as_str());
                let fill = if is_active { p.accent_soft } else { p.surface };
                item(ui, fill, &mut |ui| {
                    if t.resolved {
                        ui.set_opacity(0.7);
                    }
                    let head = ui
                        .scope(|ui| {
                            ui.label(egui::RichText::new(format!("{}:{}", t.file, t.line)).font(FontId::monospace(11.5)).color(p.muted));
                            quote(ui, &t.quote);
                        })
                        .response;
                    let head = ui.interact(head.rect, ui.scope_id().with(("thread", &t.id)), egui::Sense::click()).on_hover_text("Show in the editor");
                    if head.clicked() {
                        out.push(Action::Select(t.clone()));
                    }
                    ui.add(egui::Label::new(egui::RichText::new(&t.body).color(p.ink)).wrap());
                    ui.label(egui::RichText::new(crate::when::medium_ms(t.created)).size(11.5).color(p.muted));
                    for r in &t.replies {
                        let fr = egui::Frame::NONE.inner_margin(Margin { left: 20, right: 0, top: 0, bottom: 0 }).show(ui, |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(&r.body).color(p.ink)).wrap());
                            ui.label(egui::RichText::new(crate::when::medium_ms(r.created)).size(11.5).color(p.muted));
                        });
                        let b = fr.response.rect;
                        ui.painter().vline(b.left() + 13.0, b.y_range(), Stroke::new(2.0, p.border));
                    }
                    if is_active && !t.resolved {
                        let reply = self.replies.entry(t.id.clone()).or_default();
                        let r = widgets::text_input(ui, p, reply, "Reply", false);
                        ui.ctx().accesskit_node_builder(r.id, |n| n.set_label("Reply"));
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !reply.trim().is_empty() {
                            out.push(Action::Reply(t.id.clone(), reply.trim().to_owned()));
                            reply.clear();
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if self.confirm_delete.as_deref() == Some(t.id.as_str()) {
                            if Button::new("Delete for good").size(Size::Sm).fill(p.error).ink(egui::Color32::WHITE).show(ui, p).clicked() {
                                self.confirm_delete = None;
                                out.push(Action::Delete(t.id.clone()));
                            }
                        } else if Button::new("Delete").kind(Kind::Ghost).size(Size::Sm).show(ui, p).clicked() {
                            self.confirm_delete = Some(t.id.clone());
                        }
                        if Button::new(if t.resolved { "Reopen" } else { "Resolve" }).size(Size::Sm).show(ui, p).clicked() {
                            out.push(Action::Resolve(t.id.clone(), !t.resolved));
                        }
                    });
                });
            }
            ui.add_space(vec2(0.0, 1.0).y);
        });
        let _ = theme::R;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(id: &str, file: &str, line: usize, body: &str, created: f64) -> Thread {
        Thread {
            id: id.into(),
            file: file.into(),
            line,
            quote: String::new(),
            body: body.into(),
            created,
            resolved: false,
            replies: vec![],
        }
    }

    // comments/model.test.ts
    #[test]
    fn round_trips_and_ignores_damaged_entries() {
        let mut t = thread("a", "main.tex", 1, "Check this", 1.0);
        t.replies.push(Reply {
            body: "ok".into(),
            created: 2.0,
        });
        assert_eq!(parse(&serialize(std::slice::from_ref(&t))), vec![t]);
        assert_eq!(
            parse(r#"{"threads":[{"id":1},{"id":"b","file":"x.tex","body":"b","line":-3}]}"#),
            vec![thread("b", "x.tex", 1, "b", 0.0)]
        );
        assert!(parse("not json").is_empty());
        assert!(new_id(1.7e12).contains('-'));
    }

    #[test]
    fn locates_quotes_near_their_line() {
        let text = "intro\nthe result\nmore\nmore\nthe result\nend";
        assert_eq!(locate(text, 4, "the result"), Some((27, 37, 5)));
        assert_eq!(locate(text, 1, "the result").map(|l| l.2), Some(2));
        let text = "one\ntwo\nthree";
        assert_eq!(locate(text, 2, "vanished"), Some((4, 7, 2)));
        assert_eq!(locate(text, 3, ""), Some((8, 13, 3)));
        assert_eq!(locate(text, 9, ""), None);
    }
}
