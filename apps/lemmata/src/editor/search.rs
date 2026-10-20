//! CodeMirror's search panel (@codemirror/search) as Editor.svelte used it: at the bottom of the
//! editor, on `--surface-2`, with find, replace and their options, plus the Go to line panel.

use super::buffer::{Change, ChangeSet, EditKind, Range, Selection};
use super::view::Editor;
use crate::theme::{self, Palette};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};

pub struct Search {
    pub query: String,
    pub replace: String,
    pub case_sensitive: bool,
    pub regexp: bool,
    pub whole_word: bool,
    /// The Go to line panel instead of search.
    pub goto: Option<String>,
    /// Matches as char ranges, for the text version and query they were found for.
    matches: Vec<(usize, usize)>,
    found_for: Option<(u64, String, bool, bool, bool)>,
    current: Option<(usize, usize)>,
    focus_query: bool,
}

impl Search {
    /// Mod-f: opens (or refocuses) search; a one-line selection becomes the query.
    pub fn open(previous: Option<Search>, selected: &str, _regexp: bool) -> Self {
        let mut s = previous.unwrap_or_else(|| Self {
            query: String::new(),
            replace: String::new(),
            case_sensitive: false,
            regexp: false,
            whole_word: false,
            goto: None,
            matches: Vec::new(),
            found_for: None,
            current: None,
            focus_query: true,
        });
        if !selected.is_empty() && !selected.contains('\n') {
            s.query = selected.into();
        }
        s.goto = None;
        s.focus_query = true;
        s
    }

    /// Mod-Alt-g: the Go to line panel.
    pub fn goto_line(previous: Option<Search>) -> Self {
        let mut s = Self::open(previous, "", false);
        s.goto = Some(String::new());
        s
    }

    pub fn current(&self) -> Option<(usize, usize)> {
        self.current
    }

    /// The matches inside `from..to`.
    pub fn matches_between(
        &self,
        from: usize,
        to: usize,
    ) -> impl Iterator<Item = (usize, usize)> + '_ {
        let start = self.matches.partition_point(|m| m.1 < from);
        self.matches[start..]
            .iter()
            .copied()
            .take_while(move |m| m.0 <= to)
            .filter(move |m| m.1 <= to && m.0 >= from)
    }

    fn refresh(&mut self, ed_text: &str, version: u64) {
        let key = (
            version,
            self.query.clone(),
            self.case_sensitive,
            self.regexp,
            self.whole_word,
        );
        if self.found_for.as_ref() == Some(&key) {
            return;
        }
        self.matches = find_all(
            ed_text,
            &self.query,
            self.case_sensitive,
            self.regexp,
            self.whole_word,
        );
        self.found_for = Some(key);
        if self.current.is_some_and(|c| !self.matches.contains(&c)) {
            self.current = None;
        }
    }
}

/// Every match of the query, as char ranges.
pub fn find_all(
    text: &str,
    query: &str,
    case_sensitive: bool,
    regexp: bool,
    whole_word: bool,
) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let pattern = if regexp {
        query.to_owned()
    } else {
        regex::escape(query)
    };
    let pattern = if case_sensitive {
        pattern
    } else {
        format!("(?i){pattern}")
    };
    let Ok(re) = regex::Regex::new(&pattern) else {
        return Vec::new();
    };
    // Byte offsets to char offsets.
    let mut byte_to_char = Vec::with_capacity(text.len() + 1);
    for (i, ch) in text.chars().enumerate() {
        for _ in 0..ch.len_utf8() {
            byte_to_char.push(i);
        }
    }
    let n = text.chars().count();
    byte_to_char.push(n);
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    re.find_iter(text)
        .filter(|m| !m.as_str().is_empty())
        .filter(|m| {
            !whole_word
                || (!word(text[..m.start()].chars().next_back())
                    && !word(text[m.end()..].chars().next()))
        })
        .map(|m| (byte_to_char[m.start()], byte_to_char[m.end()]))
        .collect()
}

/// Mod-g / Enter: selects the next match after the selection (wrapping around); Shift goes back.
pub fn find_next(ed: &mut Editor, forward: bool) {
    let Some(s) = ed.search.as_mut() else { return };
    s.refresh(&ed.doc.text(), ed.doc.version);
    if s.matches.is_empty() {
        return;
    }
    let main = ed.doc.selection.main();
    let m = if forward {
        s.matches
            .iter()
            .find(|m| m.0 >= main.to() && (m.0, m.1) != (main.from(), main.to()))
            .or(s.matches.first())
    } else {
        s.matches
            .iter()
            .rev()
            .find(|m| m.1 <= main.from())
            .or(s.matches.last())
    };
    if let Some(&(a, b)) = m {
        s.current = Some((a, b));
        ed.doc.selection = Selection::single(Range::new(a, b));
        ed.reveal = true;
    }
}

/// Mod-Shift-l / "all": selects every match.
pub fn select_all_matches(ed: &mut Editor) {
    let text = ed.doc.text();
    // Without a search open, the main selection is what to look for (selectSelectionMatches).
    let matches = match ed.search.as_mut() {
        Some(s) => {
            s.refresh(&text, ed.doc.version);
            s.matches.clone()
        }
        None => {
            let sel = ed.selected_text();
            if sel.is_empty() {
                return;
            }
            find_all(&text, &sel, true, false, false)
        }
    };
    if matches.is_empty() {
        return;
    }
    let ranges = matches.iter().map(|&(a, b)| Range::new(a, b)).collect();
    ed.doc.selection = Selection::normalized(ranges, 0);
}

/// "replace": the current match (if selected) becomes the replacement, then the next one is selected.
fn replace_next(ed: &mut Editor) {
    let Some(s) = ed.search.as_mut() else { return };
    s.refresh(&ed.doc.text(), ed.doc.version);
    let main = ed.doc.selection.main();
    let replacement = s.replace.clone();
    if s.matches.contains(&(main.from(), main.to())) {
        let n = replacement.chars().count();
        let cs = ChangeSet::new(vec![Change {
            from: main.from(),
            to: main.to(),
            insert: replacement,
        }]);
        ed.doc
            .apply(cs, Selection::cursor(main.from() + n), EditKind::Other);
    }
    find_next(ed, true);
}

fn replace_all(ed: &mut Editor) {
    let Some(s) = ed.search.as_mut() else { return };
    s.refresh(&ed.doc.text(), ed.doc.version);
    if s.matches.is_empty() {
        return;
    }
    let changes = s
        .matches
        .iter()
        .map(|&(a, b)| Change {
            from: a,
            to: b,
            insert: s.replace.clone(),
        })
        .collect();
    let cs = ChangeSet::new(changes);
    let head = cs.map(ed.doc.selection.main().head, 1);
    ed.doc.apply(cs, Selection::cursor(head), EditKind::Other);
}

/// "line", "line:col", "+n"/"-n" (relative) or "n%" (of the document), as CodeMirror's gotoLine.
pub fn parse_goto(
    input: &str,
    current_line: usize,
    lines: usize,
) -> Option<(usize, Option<usize>)> {
    let t = input.trim();
    if let Some(pct) = t.strip_suffix('%') {
        let v: f64 = pct.parse().ok()?;
        return Some((((v / 100.0) * lines as f64).round().max(1.0) as usize, None));
    }
    let (line, col) = match t.split_once(':') {
        Some((l, c)) => (l, Some(c.parse::<usize>().ok()?)),
        None => (t, None),
    };
    let n = if let Some(r) = line.strip_prefix('+') {
        current_line + r.parse::<usize>().ok()?
    } else if let Some(r) = line.strip_prefix('-') {
        current_line.saturating_sub(r.parse::<usize>().ok()?)
    } else {
        line.parse().ok()?
    };
    Some((n.clamp(1, lines), col))
}

/// CodeMirror's `.cm-button`: a small gradient button at 70 % of the editor's font size.
fn cm_button(ui: &mut Ui, p: &Palette, label: &str, size: f32) -> egui::Response {
    let font = FontId::proportional(size * 0.7);
    let g = ui.painter().layout_no_wrap(label.into(), font, p.ink);
    let (rect, r) = ui.allocate_exact_size(
        vec2(
            g.size().x + 2.0 * size * 0.7,
            g.size().y + 0.4 * size * 0.7 + 2.0,
        ),
        Sense::click(),
    );
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    let (top, bottom) = if p.dark {
        (
            Color32::from_rgb(0x39, 0x39, 0x39),
            Color32::from_rgb(0x11, 0x11, 0x11),
        )
    } else {
        (
            Color32::from_rgb(0xef, 0xf1, 0xf5),
            Color32::from_rgb(0xd9, 0xd9, 0xdf),
        )
    };
    let (top, bottom) = if r.is_pointer_button_down_on() {
        (bottom, top)
    } else {
        (top, bottom)
    };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    ui.painter().add(egui::Shape::mesh(mesh));
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(1),
        Stroke::new(1.0, Color32::from_rgb(0x88, 0x88, 0x88)),
        StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - g.size() / 2.0, g, p.ink);
    r
}

/// `.cm-textfield`: 70 % font, a silver border, white (or the panel's colour when dark).
fn cm_field(
    ui: &mut Ui,
    p: &Palette,
    text: &mut String,
    hint: &str,
    size: f32,
    width: f32,
) -> egui::Response {
    let font = FontId::proportional(size * 0.7);
    let fill = if p.dark { p.surface_2 } else { Color32::WHITE };
    let border = if p.dark {
        Color32::from_rgb(0x55, 0x55, 0x55)
    } else {
        Color32::from_rgb(0xc0, 0xc0, 0xc0)
    };
    let out = egui::TextEdit::singleline(text)
        .font(font)
        .hint_text(hint)
        .text_color(p.ink)
        .desired_width(width)
        .frame(
            egui::Frame::NONE
                .fill(fill)
                .stroke(Stroke::new(1.0, border))
                .inner_margin(egui::Margin::symmetric(
                    (size * 0.7 * 0.5) as i8,
                    (size * 0.7 * 0.2) as i8,
                )),
        )
        .show(ui);
    let r = out.response.response;
    ui.ctx().accesskit_node_builder(r.id, |n| n.set_label(hint));
    r
}

fn cm_check(ui: &mut Ui, p: &Palette, value: &mut bool, label: &str, size: f32) -> bool {
    let font = FontId::proportional(size * 0.8);
    let g = ui.painter().layout_no_wrap(label.into(), font, p.ink);
    let (rect, r) = ui.allocate_exact_size(
        vec2(13.0 + 3.0 + g.size().x, g.size().y.max(13.0)),
        Sense::click(),
    );
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, *value, label));
    let bx = Rect::from_min_size(pos2(rect.left(), rect.center().y - 6.5), Vec2::splat(13.0));
    if *value {
        ui.painter()
            .rect_filled(bx, CornerRadius::same(2), p.accent);
        crate::icons::paint(
            ui.painter(),
            bx.shrink(2.0),
            crate::icons::CHECK,
            p.accent_ink,
        );
    } else {
        ui.painter().rect(
            bx,
            CornerRadius::same(2),
            p.surface,
            Stroke::new(1.0, p.faint),
            StrokeKind::Inside,
        );
    }
    ui.painter().galley(
        pos2(bx.right() + 3.0, rect.center().y - g.size().y / 2.0),
        g,
        p.ink,
    );
    if r.clicked() {
        *value = !*value;
        return true;
    }
    false
}

/// Draws the panel at the bottom of `area`; returns its height.
pub fn panel(ui: &mut Ui, p: &Palette, ed: &mut Editor, area: Rect, id: egui::Id) -> f32 {
    let size = 13.0;
    let goto = ed.search.as_ref().is_some_and(|s| s.goto.is_some());
    let h = if goto { 30.0 } else { 54.0 };
    let rect = Rect::from_min_max(pos2(area.left(), area.bottom() - h), area.max);
    ui.painter().rect_filled(rect, 0, p.surface_2);
    ui.painter()
        .hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p.border));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(6.0, 3.0)))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let ui = &mut child;
    ui.spacing_mut().item_spacing = vec2(0.6 * size * 0.7, 3.0);
    let text = ed.doc.text();
    let version = ed.doc.version;
    let lines = ed.doc.line_count();
    let current_line = ed.cursor_line();
    let mut act: Option<&str> = None;
    {
        let s = ed.search.as_mut().unwrap();
        if let Some(target) = s.goto.as_mut() {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Go to line:")
                        .size(size * 0.8)
                        .color(p.ink),
                );
                let r = cm_field(ui, p, target, "", size, 80.0);
                if s.focus_query {
                    r.request_focus();
                    s.focus_query = false;
                }
                if (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    | cm_button(ui, p, "go", size).clicked()
                {
                    act = Some("goto");
                }
            });
        } else {
            s.refresh(&text, version);
            ui.horizontal(|ui| {
                let r = cm_field(ui, p, &mut s.query, "Find", size, 140.0);
                if s.focus_query {
                    r.request_focus();
                    s.focus_query = false;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    act = Some(if ui.input(|i| i.modifiers.shift) {
                        "prev"
                    } else {
                        "next"
                    });
                    r.request_focus();
                }
                if cm_button(ui, p, "next", size).clicked() {
                    act = Some("next");
                }
                if cm_button(ui, p, "previous", size).clicked() {
                    act = Some("prev");
                }
                if cm_button(ui, p, "all", size).clicked() {
                    act = Some("all");
                }
                cm_check(ui, p, &mut s.case_sensitive, "match case", size);
                cm_check(ui, p, &mut s.regexp, "regexp", size);
                cm_check(ui, p, &mut s.whole_word, "by word", size);
            });
            ui.horizontal(|ui| {
                let r = cm_field(ui, p, &mut s.replace, "Replace", size, 140.0);
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    act = Some("replace");
                    r.request_focus();
                }
                if cm_button(ui, p, "replace", size).clicked() {
                    act = Some("replace");
                }
                if cm_button(ui, p, "replace all", size).clicked() {
                    act = Some("replace-all");
                }
            });
        }
    }
    // The close button, top right.
    let close = Rect::from_min_size(
        pos2(rect.right() - 4.0 - 14.0, rect.top() + 1.0),
        vec2(14.0, 16.0),
    );
    let cr = ui.interact(close, id.with("search-close"), Sense::click());
    cr.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "close"));
    ui.painter().text(
        close.center(),
        egui::Align2::CENTER_CENTER,
        "×",
        FontId::proportional(size * 1.1),
        p.ink,
    );
    if cr.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ed.search = None;
        return h;
    }
    match act {
        Some("next") => find_next(ed, true),
        Some("prev") => find_next(ed, false),
        Some("all") => select_all_matches(ed),
        Some("replace") => replace_next(ed),
        Some("replace-all") => replace_all(ed),
        Some("goto") => {
            let input = ed
                .search
                .as_ref()
                .and_then(|s| s.goto.clone())
                .unwrap_or_default();
            if let Some((line, col)) = parse_goto(&input, current_line, lines) {
                ed.goto_line(line);
                if let Some(c) = col {
                    let start = ed.doc.line_start(line - 1);
                    let len = ed.doc.line(line - 1).chars().count();
                    ed.doc.selection = Selection::cursor(start + (c.saturating_sub(1)).min(len));
                }
                ed.search = None;
                ui.memory_mut(|m| m.request_focus(id));
            }
        }
        _ => {}
    }
    let _ = theme::R;
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_literal_case_regexp_and_whole_words() {
        let t = "Lemma lemma dilemma Grüße";
        assert_eq!(
            find_all(t, "lemma", false, false, false),
            vec![(0, 5), (6, 11), (14, 19)]
        );
        assert_eq!(
            find_all(t, "lemma", true, false, false),
            vec![(6, 11), (14, 19)]
        );
        assert_eq!(
            find_all(t, "lemma", false, false, true),
            vec![(0, 5), (6, 11)]
        );
        assert_eq!(find_all(t, "gr..e", false, true, false), vec![(20, 25)]);
        assert_eq!(
            find_all(t, "(", false, true, false),
            vec![],
            "a broken pattern finds nothing"
        );
        assert_eq!(find_all(t, "(", false, false, false), vec![]);
    }

    #[test]
    fn goto_reads_lines_columns_offsets_and_percentages() {
        assert_eq!(parse_goto("12", 1, 100), Some((12, None)));
        assert_eq!(parse_goto("12:4", 1, 100), Some((12, Some(4))));
        assert_eq!(parse_goto("+3", 10, 100), Some((13, None)));
        assert_eq!(parse_goto("-30", 10, 100), Some((1, None)));
        assert_eq!(parse_goto("50%", 1, 200), Some((100, None)));
        assert_eq!(parse_goto("999", 1, 100), Some((100, None)));
        assert_eq!(parse_goto("x", 1, 100), None);
    }

    #[test]
    fn next_wraps_and_replace_all_replaces() {
        let mut ed = Editor::new("a b a b a", false);
        ed.search = Some(Search::open(None, "", false));
        ed.search.as_mut().unwrap().query = "a".into();
        // From a cursor at 0 the match at 0 comes first, as in CodeMirror.
        find_next(&mut ed, true);
        assert_eq!(ed.doc.selection.main(), Range::new(0, 1));
        find_next(&mut ed, true);
        assert_eq!(ed.doc.selection.main(), Range::new(4, 5));
        find_next(&mut ed, true);
        find_next(&mut ed, true);
        assert_eq!(ed.doc.selection.main(), Range::new(0, 1), "wrapped around");
        find_next(&mut ed, false);
        assert_eq!(
            ed.doc.selection.main(),
            Range::new(8, 9),
            "backwards wraps too"
        );
        ed.search.as_mut().unwrap().replace = "xy".into();
        replace_all(&mut ed);
        assert_eq!(ed.doc.text(), "xy b xy b xy");
    }
}
