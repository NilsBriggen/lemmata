//! Review comments (comments/model.ts and ReviewPanel.svelte): threads anchored to a quoted piece
//! of text in a file, kept in `.lemmata/comments.json`, and the panel that lists them.

use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind, Size};
use eframe::egui::{
    self, CornerRadius, FontId, Margin, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const COMMENTS_FILE: &str = ".lemmata/comments.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub body: String,
    #[serde(default)]
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

/// Threads from the file, and whether damaged ones were skipped (writing back would lose them).
/// None when the file isn't JSON at all.
pub fn parse(json: &str) -> Option<(Vec<Thread>, bool)> {
    let data = serde_json::from_str::<Value>(json).ok()?;
    let list = match &data {
        Value::Array(a) => a.clone(),
        v => v
            .get("threads")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    };
    let threads: Vec<Thread> = list
        .iter()
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
        .collect();
    let skipped = threads.len() < list.len();
    Some((threads, skipped))
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
    // Asked again at every key while a file has comments: no copy of the text, and the
    // standard library's search instead of a comparison at every char.
    if !quote.is_empty() {
        let len = quote.chars().count();
        let (mut best, mut seen, mut lines, mut chars) = (None, 0, 1, 0);
        let mut from = 0;
        while let Some(i) = text[from..].find(quote) {
            let at = from + i;
            lines += text.as_bytes()[seen..at]
                .iter()
                .filter(|&&b| b == b'\n')
                .count();
            chars += text[seen..at].chars().count();
            seen = at;
            // The nearest copy to its line, the first on a tie; overlapping copies count.
            if best.is_none_or(|(_, _, l): (usize, usize, usize)| {
                lines.abs_diff(line) < l.abs_diff(line)
            }) {
                best = Some((chars, chars + len, lines));
            }
            from = at + text[at..].chars().next().map_or(1, char::len_utf8);
        }
        if best.is_some() {
            return best;
        }
    }
    let mut start = 0;
    for (n, l) in text.split('\n').enumerate() {
        let len = l.chars().count();
        if n + 1 == line {
            return Some((start, start + len, line));
        }
        start += len + 1;
    }
    None
}

/// What the panel asks of the workspace.
pub enum Action {
    Add(String),
    CancelDraft,
    Select(Thread),
    Reply(String, String),
    Resolve(String, bool),
    Delete(String),
}

#[derive(Default)]
pub struct Review {
    show_resolved: bool,
    text: String,
    draft_focused: bool,
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
    /// A new draft starts empty, whatever was typed into the last one.
    pub fn clear_draft(&mut self) {
        self.text.clear();
        self.draft_focused = false;
    }

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
        ui.painter().rect_filled(rect, 0, p.app);
        let open_count = threads.iter().filter(|t| !t.resolved).count();

        // 32 px quiet sidebar header with compact Open / Resolved pill switch.
        let head = Rect::from_min_size(rect.min, vec2(rect.width(), 32.0));
        let mut hjob = egui::text::LayoutJob::default();
        hjob.append(
            "COMMENTS",
            0.0,
            egui::TextFormat {
                font_id: FontId::new(11.0, theme::font::sans_600()),
                color: p.faint,
                extra_letter_spacing: 0.77,
                ..Default::default()
            },
        );
        let mut hleft = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    pos2(head.left() + 12.0, head.top()),
                    pos2(head.left() + 92.0, head.bottom()),
                ))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        hleft.label(hjob);
        let mut hright = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    pos2(head.left() + 92.0, head.top()),
                    pos2(head.right() - 8.0, head.bottom()),
                ))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        // Compact pill switch: "Open N" | "Resolved".
        let open_lbl = format!("Open {open_count}");
        let res_lbl = "Resolved";
        let font = FontId::new(11.5, theme::font::sans_500());
        let ow = hright
            .painter()
            .layout_no_wrap(open_lbl.clone(), font.clone(), p.ink)
            .size()
            .x
            + 16.0;
        let rw = hright
            .painter()
            .layout_no_wrap(res_lbl.into(), font.clone(), p.ink)
            .size()
            .x
            + 16.0;
        let (track, _) = hright.allocate_exact_size(vec2(ow + rw + 4.0, 24.0), Sense::hover());
        hright
            .painter()
            .rect_filled(track, CornerRadius::same(7), p.surface_3);
        let open_rect = Rect::from_min_size(track.min + vec2(2.0, 2.0), vec2(ow, 20.0));
        let res_rect =
            Rect::from_min_size(pos2(open_rect.right(), track.top() + 2.0), vec2(rw, 20.0));
        for (i, (r, lbl, on)) in [
            (open_rect, open_lbl.as_str(), !self.show_resolved),
            (res_rect, res_lbl, self.show_resolved),
        ]
        .into_iter()
        .enumerate()
        {
            let resp = hright
                .interact(r, hright.scope_id().with(("cm-tab", i)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, lbl));
            if on {
                hright
                    .painter()
                    .add(p.shadow_card.as_shape(r, CornerRadius::same(5)));
                hright.painter().rect(
                    r,
                    CornerRadius::same(5),
                    p.surface,
                    Stroke::new(1.0, p.card_hairline),
                    StrokeKind::Inside,
                );
            }
            hright.painter().text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                lbl,
                font.clone(),
                if on { p.ink } else { p.muted },
            );
            widgets::focus_ring(&hright, &resp, r, p, 5);
            if resp.clicked() {
                self.show_resolved = i == 1;
            }
        }

        if draft.is_none() {
            self.draft_focused = false;
        }
        let body_rect = Rect::from_min_max(pos2(rect.left(), head.bottom()), rect.max);
        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(body_rect));
        let mut shown: Vec<&Thread> = threads
            .iter()
            .filter(|t| {
                if self.show_resolved {
                    t.resolved
                } else {
                    !t.resolved
                }
            })
            .collect();
        shown.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
        egui::ScrollArea::vertical()
            .id_salt("review")
            .auto_shrink([false, false])
            .show(&mut c, |ui| {
                egui::Frame::NONE
                    .inner_margin(Margin {
                        left: 10,
                        right: 10,
                        top: 6,
                        bottom: 14,
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 8.0;
                        let card =
                            |ui: &mut Ui, is_active: bool, add: &mut dyn FnMut(&mut Ui)| {
                                let stroke = if is_active {
                                    Stroke::new(2.0, p.accent)
                                } else {
                                    Stroke::new(1.0, p.card_hairline)
                                };
                                egui::Frame::new()
                                    .fill(p.surface)
                                    .stroke(stroke)
                                    .corner_radius(theme::R_MD)
                                    .shadow(p.shadow_card)
                                    .inner_margin(Margin::symmetric(11, 10))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.spacing_mut().item_spacing.y = 6.0;
                                        add(ui);
                                    });
                            };
                        let quote = |ui: &mut Ui, q: &str| {
                            let r = egui::Frame::NONE
                                .inner_margin(Margin {
                                    left: 9,
                                    right: 4,
                                    top: 1,
                                    bottom: 1,
                                })
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(excerpt(q))
                                                .size(12.0)
                                                .italics()
                                                .color(p.muted),
                                        )
                                        .wrap(),
                                    );
                                });
                            let b = r.response.rect;
                            ui.painter().vline(
                                b.left() + 1.0,
                                b.y_range(),
                                Stroke::new(2.0, p.warn),
                            );
                        };
                        if let Some((file, line, q)) = draft {
                            let mut add = false;
                            let mut cancel = false;
                            card(ui, true, &mut |ui| {
                                ui.label(
                                    egui::RichText::new(format!("{file}:{line}"))
                                        .font(FontId::monospace(11.0))
                                        .color(p.faint),
                                );
                                quote(ui, q);
                                let r = widgets::text_area(
                                    ui,
                                    p,
                                    &mut self.text,
                                    "Write a comment",
                                    3,
                                    true,
                                );
                                if !self.draft_focused {
                                    r.request_focus();
                                    self.draft_focused = true;
                                }
                                if r.has_focus()
                                    && ui.input(|i| {
                                        i.key_pressed(egui::Key::Enter) && i.modifiers.command
                                    })
                                {
                                    add = true;
                                }
                                if (r.has_focus() || r.lost_focus())
                                    && ui.input_mut(|i| {
                                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                                    })
                                {
                                    cancel = true;
                                }
                                ui.allocate_ui_with_layout(
                                    vec2(ui.available_width(), 28.0),
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        if Button::new("Comment")
                                            .kind(Kind::Primary)
                                            .size(Size::Sm)
                                            .enabled(!self.text.trim().is_empty())
                                            .show(ui, p)
                                            .clicked()
                                        {
                                            add = true;
                                        }
                                        if Button::new("Cancel").size(Size::Sm).show(ui, p).clicked()
                                        {
                                            cancel = true;
                                        }
                                    },
                                );
                            });
                            if add && !self.text.trim().is_empty() {
                                out.push(Action::Add(self.text.trim().to_owned()));
                                self.text.clear();
                            }
                            if cancel {
                                self.text.clear();
                                out.push(Action::CancelDraft);
                            }
                        }
                        if shown.is_empty() && draft.is_none() {
                            egui::Frame::NONE
                                .inner_margin(Margin::symmetric(4, 10))
                                .show(ui, |ui| {
                                    let t = if threads.is_empty() {
                                        "No comments yet. Select some text and choose Add comment (in the editor toolbar or the command palette)."
                                    } else if self.show_resolved {
                                        "No resolved comments."
                                    } else {
                                        "No open comments."
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(t).size(12.5).color(p.muted),
                                        )
                                        .wrap(),
                                    );
                                });
                        }
                        for t in shown {
                            let is_active = active == Some(t.id.as_str());
                            card(ui, is_active, &mut |ui| {
                                if t.resolved {
                                    ui.set_opacity(0.75);
                                }
                                let head = ui
                                    .scope(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{}:{}",
                                                    t.file, t.line
                                                ))
                                                .font(FontId::monospace(11.0))
                                                .color(p.faint),
                                            );
                                            if t.created > 0.0 {
                                                ui.with_layout(
                                                    egui::Layout::right_to_left(
                                                        egui::Align::Center,
                                                    ),
                                                    |ui| {
                                                        ui.label(
                                                            egui::RichText::new(
                                                                crate::when::medium_ms(t.created),
                                                            )
                                                            .size(11.0)
                                                            .color(p.faint),
                                                        );
                                                    },
                                                );
                                            }
                                        });
                                        quote(ui, &t.quote);
                                    })
                                    .response;
                                let head = ui
                                    .interact(
                                        head.rect,
                                        ui.scope_id().with(("thread", &t.id)),
                                        Sense::click(),
                                    )
                                    .on_hover_text("Show in the editor")
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                                if head.clicked() {
                                    self.confirm_delete = None;
                                    out.push(Action::Select(t.clone()));
                                }
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&t.body).size(12.5).color(p.ink),
                                    )
                                    .wrap(),
                                );
                                for r in &t.replies {
                                    let fr = egui::Frame::NONE
                                        .inner_margin(Margin {
                                            left: 12,
                                            right: 0,
                                            top: 2,
                                            bottom: 2,
                                        })
                                        .show(ui, |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&r.body)
                                                        .size(12.5)
                                                        .color(p.ink),
                                                )
                                                .wrap(),
                                            );
                                            if r.created > 0.0 {
                                                ui.label(
                                                    egui::RichText::new(crate::when::medium_ms(
                                                        r.created,
                                                    ))
                                                    .size(11.0)
                                                    .color(p.faint),
                                                );
                                            }
                                        });
                                    let b = fr.response.rect;
                                    ui.painter().vline(
                                        b.left() + 4.0,
                                        b.y_range(),
                                        Stroke::new(2.0, p.border),
                                    );
                                }
                                if is_active && !t.resolved {
                                    let reply = self.replies.entry(t.id.clone()).or_default();
                                    let mut submit_reply = false;
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        let bw = 56.0;
                                        let iw = (ui.available_width() - bw - 6.0).max(80.0);
                                        let r = ui
                                            .allocate_ui(vec2(iw, 30.0), |ui| {
                                                widgets::text_input(ui, p, reply, "Reply", false)
                                            })
                                            .inner;
                                        ui.ctx().accesskit_node_builder(r.id, |n| {
                                            n.set_label("Reply")
                                        });
                                        if r.lost_focus()
                                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                            && !reply.trim().is_empty()
                                        {
                                            submit_reply = true;
                                        }
                                        if Button::new("Reply")
                                            .kind(Kind::Primary)
                                            .size(Size::Sm)
                                            .enabled(!reply.trim().is_empty())
                                            .show(ui, p)
                                            .clicked()
                                        {
                                            submit_reply = true;
                                        }
                                    });
                                    if submit_reply {
                                        out.push(Action::Reply(
                                            t.id.clone(),
                                            reply.trim().to_owned(),
                                        ));
                                        reply.clear();
                                    }
                                }
                                ui.allocate_ui_with_layout(
                                    vec2(ui.available_width(), 28.0),
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        if self.confirm_delete.as_deref() == Some(t.id.as_str()) {
                                            if Button::new("Delete for good")
                                                .size(Size::Sm)
                                                .fill(p.error)
                                                .ink(p.accent_ink)
                                                .show(ui, p)
                                                .clicked()
                                            {
                                                self.confirm_delete = None;
                                                out.push(Action::Delete(t.id.clone()));
                                            }
                                            if Button::new("Cancel")
                                                .size(Size::Sm)
                                                .show(ui, p)
                                                .clicked()
                                            {
                                                self.confirm_delete = None;
                                            }
                                        } else if Button::new("Delete")
                                            .kind(Kind::Ghost)
                                            .size(Size::Sm)
                                            .show(ui, p)
                                            .clicked()
                                        {
                                            self.confirm_delete = Some(t.id.clone());
                                        }
                                        if Button::new(if t.resolved {
                                            "Reopen"
                                        } else {
                                            "Resolve"
                                        })
                                        .size(Size::Sm)
                                        .show(ui, p)
                                        .clicked()
                                        {
                                            out.push(Action::Resolve(t.id.clone(), !t.resolved));
                                        }
                                    },
                                );
                            });
                        }
                    });
            });
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
        assert_eq!(
            parse(&serialize(std::slice::from_ref(&t))),
            Some((vec![t], false))
        );
        assert_eq!(
            parse(r#"{"threads":[{"id":1},{"id":"b","file":"x.tex","body":"b","line":-3}]}"#),
            Some((vec![thread("b", "x.tex", 1, "b", 0.0)], true))
        );
        // A reply without a time still reads.
        let (t, skipped) =
            parse(r#"[{"id":"a","file":"x.tex","body":"b","replies":[{"body":"r"}]}]"#).unwrap();
        assert!(!skipped && t[0].replies[0].created == 0.0);
        assert!(
            parse("not json").is_none(),
            "a broken file is told apart from an empty one"
        );
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
        assert_eq!(locate(text, 0, ""), None);
        // Overlapping copies (they share the middle ä), in chars, not bytes.
        assert_eq!(locate("ää\nä\nä", 2, "ä\nä"), Some((3, 6, 2)));
        assert_eq!(locate("ää\nä\nä", 1, "ä\nä"), Some((1, 4, 1)));
    }
}
