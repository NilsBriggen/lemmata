//! The editor widget: what CodeMirror's EditorView drew for Editor.svelte. Lines wrap with a
//! hanging indent and show indent guides (editor/indentGuides.ts), the gutter has line numbers and
//! the lint and fold columns, and the colours are Editor.svelte's theme.

use super::buffer::{Doc, Range, Selection};
use super::commands::{self as cmd, Indent};
use super::highlight::{Highlighter, Token};
use super::search::{self, Search};
use crate::shortcuts;
use crate::theme::Palette;
use eframe::egui::{
    self, Color32, Event, FontId, Galley, Key, Modifiers, Pos2, Rect, Sense, Stroke, TextFormat,
    Ui, Vec2, epaint::text::cursor::CCursor, pos2, text::LayoutJob, vec2,
};
use std::collections::HashMap;
use std::sync::Arc;

/// `.cm-line` padding: 10 px left, 16 px right.
const PAD: f32 = 10.0;
const PAD_RIGHT: f32 = 16.0;
/// `.cm-content` padding: 12 px above the first line; 40 % of the height below the last.
const PAD_TOP: f32 = 12.0;
/// The lint gutter is 1.4em wide.
const LINT_EM: f32 = 1.4;
const MAC: bool = cfg!(target_os = "macos");

/// How the editor looks and behaves, from the settings.
#[derive(Clone, PartialEq)]
pub struct Style {
    pub font: FontId,
    /// Pixels per line: font size × the line-height setting.
    pub line_height: f32,
    pub indent: Indent,
    pub wrap: bool,
    pub auto_close: bool,
    /// Focus mode: the text in a centred column at most 88 characters wide.
    pub focus: bool,
}

/// What happened in the editor this frame, for the app.
#[derive(Default, Debug)]
pub struct Output {
    /// The text changed.
    pub changed: bool,
    /// The main cursor is on another line now (1-based).
    pub cursor_line: Option<usize>,
    /// The selection changed; its text ("" when empty).
    pub selection: Option<String>,
    /// Mod-j: show this line (1-based) and column in the PDF.
    pub sync: Option<(usize, usize)>,
    /// Mod-Shift-e: ask which environment to wrap the selection in.
    pub wrap_environment: bool,
    /// F8 / Shift-F8: next or previous problem.
    pub next_problem: Option<i8>,
}

struct Drag {
    /// The press position in the document.
    origin: usize,
    /// Rectangular (Alt) selection: the press's line and x on screen.
    rect: Option<(usize, f32)>,
    unit: DragUnit,
    /// The ranges there were before a Ctrl-click added one.
    keep: Vec<Range>,
}

#[derive(Clone, Copy, PartialEq)]
enum DragUnit {
    Char,
    Word,
    Line,
}

/// One open file: its text, history and how it is shown.
pub struct Editor {
    pub doc: Doc,
    /// TeX files get highlighting, environment indentation and the formatting keys.
    pub tex: bool,
    hl: Highlighter,
    hl_version: u64,
    heights: Vec<f32>,
    heights_key: (u64, u32, u32, bool, u32),
    /// Heights by line text, so an edit lays out only the lines it changed.
    height_cache: HashMap<u64, f32>,
    cache_key: (u32, u32, bool, u32),
    /// The x a run of Up/Down presses aims for.
    goal_x: Option<f32>,
    /// Scroll so the main cursor shows (after typing, a jump, a search).
    pub reveal: bool,
    /// Scroll so this line (0-based) is in the middle (go to line, a jump from the PDF).
    pub center_line: Option<usize>,
    last_activity: f64,
    drag: Option<Drag>,
    preedit: Option<String>,
    pub search: Option<Search>,
    reported_version: u64,
    last_cursor_line: usize,
    last_selection: Selection,
    /// Where the view is scrolled.
    pub scroll_y: f32,
}

impl Editor {
    pub fn new(text: &str, tex: bool) -> Self {
        Self {
            doc: Doc::new(text),
            tex,
            hl: Highlighter::default(),
            hl_version: u64::MAX,
            heights: Vec::new(),
            heights_key: (u64::MAX, 0, 0, false, 0),
            height_cache: HashMap::new(),
            cache_key: (0, 0, false, 0),
            goal_x: None,
            reveal: false,
            center_line: None,
            last_activity: 0.0,
            drag: None,
            preedit: None,
            search: None,
            reported_version: 0,
            last_cursor_line: 1,
            last_selection: Selection::cursor(0),
            scroll_y: 0.0,
        }
    }

    /// The main cursor's line, 1-based.
    pub fn cursor_line(&self) -> usize {
        self.doc.line_of(self.doc.selection.main().head) + 1
    }

    /// Puts the cursor at the start of `line` (1-based) and scrolls it to the middle.
    pub fn goto_line(&mut self, line: usize) {
        let n = line.clamp(1, self.doc.line_count()) - 1;
        self.doc.selection = Selection::cursor(self.doc.line_start(n));
        self.center_line = Some(n);
    }

    /// The text of the main selection.
    pub fn selected_text(&self) -> String {
        let r = self.doc.selection.main();
        self.doc.slice(r.from(), r.to())
    }
}

/// A laid-out line: the text after its leading whitespace in a galley that starts `hang` in.
struct LineLayout {
    galley: Arc<Galley>,
    ws: Vec<char>,
    ws_width: f32,
    hang: f32,
    height: f32,
}

fn hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// Editor.svelte's HighlightStyle: colour, and italic for comments.
fn token_color(t: Token, p: &Palette) -> Option<(Color32, bool)> {
    Some(match t {
        Token::Tag | Token::Keyword => (p.accent, false),
        Token::Comment => (p.muted, true),
        Token::Atom => (p.atom, false),
        Token::Bracket => (p.muted, false),
        Token::Number => (p.ok, false),
        Token::Error => (p.error, false),
        Token::Builtin => return None,
    })
}

/// What laying out lines needs.
struct Lay<'a> {
    p: &'a Palette,
    style: &'a Style,
    /// One monospace column.
    cw: f32,
    /// Width for text, inside the line padding.
    text_w: f32,
}

impl Lay<'_> {
    fn ws_width(&self, ws: &[char]) -> f32 {
        cmd::indent_columns(&ws.iter().collect::<String>(), self.style.indent.tab_size()) as f32
            * self.cw
    }

    fn layout(&self, ui: &Ui, text: &str, tokens: &[(usize, usize, Token)]) -> LineLayout {
        let chars: Vec<char> = text.chars().collect();
        let ws_len = chars
            .iter()
            .take_while(|c| **c == ' ' || **c == '\t')
            .count();
        let ws: Vec<char> = chars[..ws_len].to_vec();
        let ws_width = self.ws_width(&ws);
        // Wrapped rows continue under the indentation; a very deep line keeps half the width.
        let hang = if self.style.wrap {
            ws_width.min((self.text_w / 2.0).max(0.0))
        } else {
            ws_width
        };
        let mut job = LayoutJob::default();
        job.wrap.max_width = if self.style.wrap {
            (self.text_w - hang).max(self.cw * 4.0)
        } else {
            f32::INFINITY
        };
        let base = TextFormat {
            font_id: self.style.font.clone(),
            color: self.p.ink,
            line_height: Some(self.style.line_height),
            ..Default::default()
        };
        let lead = ws_width - hang;
        let mut col = ws_len;
        let mut first = true;
        let mut push = |job: &mut LayoutJob, from: usize, to: usize, fmt: TextFormat| {
            if from >= to {
                return;
            }
            let s: String = chars[from..to].iter().collect();
            job.append(&s, if first { lead } else { 0.0 }, fmt);
            first = false;
        };
        for &(a, b, t) in tokens {
            let (a, b) = (a.max(col), b.max(ws_len));
            if a > col {
                push(&mut job, col, a, base.clone());
            }
            let mut fmt = base.clone();
            if let Some((c, italic)) = token_color(t, self.p) {
                fmt.color = c;
                fmt.italics = italic;
            }
            push(&mut job, a, b, fmt);
            col = col.max(b);
        }
        if col < chars.len() {
            push(&mut job, col, chars.len(), base.clone());
        }
        if first {
            job.append("", lead, base);
        }
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let height = galley.size().y.max(self.style.line_height);
        LineLayout {
            galley,
            ws,
            ws_width,
            hang,
            height,
        }
    }
}

/// Where column `col` of a line is drawn, relative to its top-left at the line padding, and its row.
fn col_pos(l: &LineLayout, col: usize, cw: f32, tab: usize) -> (Pos2, usize) {
    if col < l.ws.len() || (col == l.ws.len() && l.galley.is_empty()) {
        let w = cmd::indent_columns(&l.ws[..col.min(l.ws.len())].iter().collect::<String>(), tab)
            as f32
            * cw;
        return (pos2(w, 0.0), 0);
    }
    let r = l.galley.pos_from_cursor(CCursor::new(col - l.ws.len()));
    let row = l
        .galley
        .rows
        .iter()
        .position(|row| r.center().y < row.pos.y + row.row.size.y)
        .unwrap_or(l.galley.rows.len().saturating_sub(1));
    (pos2(l.hang + r.min.x, r.min.y), row)
}

/// The column nearest to a point relative to the line's top-left.
fn col_at(l: &LineLayout, at: Vec2, cw: f32, tab: usize) -> usize {
    let first_row_h = l.galley.rows.first().map_or(l.height, |r| r.row.size.y);
    if !l.ws.is_empty() && at.y < first_row_h && at.x < l.ws_width {
        let mut best = 0;
        for i in 0..=l.ws.len() {
            let w = cmd::indent_columns(&l.ws[..i].iter().collect::<String>(), tab) as f32 * cw;
            if w <= at.x + cw / 2.0 {
                best = i;
            }
        }
        return best;
    }
    l.ws.len() + l.galley.cursor_from_pos(vec2(at.x - l.hang, at.y)).index.0
}

/// Indentation level of line `n`; a blank line takes the smaller level of the nearest text above and below.
fn indent_level(d: &Doc, n: usize, indent: Indent) -> usize {
    let cols = |i: usize| {
        let t = d.line(i);
        (!t.trim().is_empty()).then(|| cmd::indent_columns(&t, indent.tab_size()))
    };
    let unit = indent.width();
    if let Some(c) = cols(n) {
        return c / unit;
    }
    let near = |up: bool| {
        let mut i = n;
        for _ in 0..100 {
            if up {
                if i == 0 {
                    return 0;
                }
                i -= 1;
            } else {
                i += 1;
                if i >= d.line_count() {
                    return 0;
                }
            }
            if let Some(c) = cols(i) {
                return c / unit;
            }
        }
        0
    };
    near(true).min(near(false))
}

/// The geometry the keyboard and mouse need: laid-out lines on screen, others on demand.
struct Geo<'a> {
    lay: &'a Lay<'a>,
    layouts: HashMap<usize, LineLayout>,
    tab: usize,
}

impl Geo<'_> {
    fn line(&mut self, ui: &Ui, d: &Doc, n: usize) -> &LineLayout {
        let lay = self.lay;
        self.layouts
            .entry(n)
            .or_insert_with(|| lay.layout(ui, &d.line(n), &[]))
    }

    /// (row, x) of a position.
    fn place(&mut self, ui: &Ui, d: &Doc, pos: usize) -> (usize, usize, f32) {
        let n = d.line_of(pos);
        let col = pos - d.line_start(n);
        let (cw, tab) = (self.lay.cw, self.tab);
        let (pt, row) = col_pos(self.line(ui, d, n), col, cw, tab);
        (n, row, pt.x)
    }

    /// The position at x on a visual row of line n.
    fn at(&mut self, ui: &Ui, d: &Doc, n: usize, row: usize, x: f32) -> usize {
        let (cw, tab, lh) = (self.lay.cw, self.tab, self.lay.style.line_height);
        let col = col_at(
            self.line(ui, d, n),
            vec2(x, row as f32 * lh + lh / 2.0),
            cw,
            tab,
        );
        d.line_start(n) + col.min(d.line(n).chars().count())
    }

    fn rows(&mut self, ui: &Ui, d: &Doc, n: usize) -> usize {
        self.line(ui, d, n).galley.rows.len().max(1)
    }

    /// One visual row up or down from `pos`, aiming for `goal_x`.
    fn vertical(
        &mut self,
        ui: &Ui,
        d: &Doc,
        pos: usize,
        down: bool,
        goal_x: Option<f32>,
    ) -> (usize, f32) {
        let (n, row, x) = self.place(ui, d, pos);
        let x = goal_x.unwrap_or(x);
        let target = if down {
            if row + 1 < self.rows(ui, d, n) {
                Some((n, row + 1))
            } else if n + 1 < d.line_count() {
                Some((n + 1, 0))
            } else {
                None
            }
        } else if row > 0 {
            Some((n, row - 1))
        } else if n > 0 {
            let r = self.rows(ui, d, n - 1);
            Some((n - 1, r - 1))
        } else {
            None
        };
        match target {
            Some((n, row)) => (self.at(ui, d, n, row, x), x),
            // Past the first or last line: to the document's start or end.
            None => (if down { d.len() } else { 0 }, x),
        }
    }
}

pub fn show(ui: &mut Ui, p: &Palette, ed: &mut Editor, style: &Style, id: egui::Id) -> Output {
    let mut out = Output::default();
    let mut area = ui.available_rect_before_wrap();
    ui.painter().rect_filled(area, 0, p.code_bg);
    ui.allocate_rect(area, Sense::hover());

    // In-file search sits under the text, like CodeMirror's default panel.
    if ed.search.is_some() {
        let h = search::panel(ui, p, ed, area, id);
        area.max.y -= h;
    }

    let cw = ui.fonts_mut(|f| f.glyph_width(&style.font, '0'));
    let digits = ed.doc.line_count().to_string().len();
    let numbers_w = (24.0 + digits as f32 * cw).max(40.0);
    let em = style.font.size;
    let gutter_w = (numbers_w + LINT_EM * em + 0.6 * em + 6.0 + 1.0).round();
    let full_text_w = area.width() - gutter_w - PAD - PAD_RIGHT;
    let (text_w, focus_offset) = if style.focus {
        let w = full_text_w.min(88.0 * cw);
        (w, ((full_text_w - w) / 2.0).max(0.0))
    } else {
        (full_text_w, 0.0)
    };
    let lay = Lay {
        p,
        style,
        cw,
        text_w,
    };
    let tab = style.indent.tab_size();

    // Highlighting starts over from the first changed line.
    if ed.hl_version != ed.doc.version {
        ed.hl.invalidate_from(if ed.hl_version == u64::MAX {
            0
        } else {
            ed.doc.changed_from_line
        });
        ed.hl_version = ed.doc.version;
    }

    // Line heights, from the cache by text; a line is laid out only when it is new.
    let key = (
        text_w.to_bits(),
        style.font.size.to_bits(),
        style.wrap,
        style.line_height.to_bits(),
    );
    if ed.cache_key != key {
        ed.height_cache.clear();
        ed.cache_key = key;
    }
    let hkey = (ed.doc.version, key.0, key.1, key.2, key.3);
    if ed.heights_key != hkey || ed.heights.len() != ed.doc.line_count() {
        let n = ed.doc.line_count();
        ed.heights.clear();
        for i in 0..n {
            let text = ed.doc.line(i);
            let h = *ed.height_cache.entry(hash(&text)).or_insert_with(|| {
                // Short lines can't wrap: no layout needed.
                if !style.wrap || text.chars().count() as f32 * cw <= text_w * 0.9 {
                    style.line_height
                } else {
                    lay.layout(ui, &text, &[]).height
                }
            });
            ed.heights.push(h);
        }
        ed.heights_key = hkey;
    }
    let mut tops = Vec::with_capacity(ed.heights.len());
    let mut y = PAD_TOP;
    for h in &ed.heights {
        tops.push(y);
        y += h;
    }
    let total = y + area.height() * 0.4;

    let main = ed.doc.selection.main();
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt(id.with("scroll"))
        .auto_shrink(false);
    if let Some(n) = ed.center_line.take() {
        let n = n.min(tops.len() - 1);
        scroll = scroll.vertical_scroll_offset((tops[n] - area.height() / 2.0).max(0.0));
    } else if ed.reveal {
        let line = ed.doc.line_of(main.head);
        let (top, bottom) = (tops[line], tops[line] + ed.heights[line]);
        // CodeMirror keeps 5 px between the cursor and the edge.
        if top < ed.scroll_y + 5.0 {
            scroll = scroll.vertical_scroll_offset((top - 5.0).max(0.0));
        } else if bottom > ed.scroll_y + area.height() - 5.0 {
            scroll = scroll.vertical_scroll_offset(bottom - area.height() + 5.0);
        }
    }
    ed.reveal = false;

    let mut geo = Geo {
        lay: &lay,
        layouts: HashMap::new(),
        tab,
    };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
    let now = ui.input(|i| i.time);
    let mods = ui.input(|i| i.modifiers);

    let scrolled = scroll.show_viewport(&mut child, |ui, viewport| {
        ui.set_min_size(vec2(area.width(), total));
        let origin = ui.min_rect().min;
        ed.scroll_y = viewport.min.y;
        let screen = |x: f32, y: f32| origin + vec2(x, y);
        let visible = Rect::from_min_size(screen(0.0, viewport.min.y), viewport.size());
        let painter = ui.painter_at(visible);
        let text_x = gutter_w + PAD + focus_offset;

        let first = tops
            .partition_point(|&t| t <= viewport.min.y)
            .saturating_sub(1);
        let last = tops
            .partition_point(|&t| t < viewport.max.y)
            .min(tops.len());

        // Gutter: --gutter with a 1 px --border on its right.
        let gutter = Rect::from_min_size(visible.min, vec2(gutter_w, visible.height()));
        painter.rect_filled(gutter, 0, p.gutter);
        painter.vline(
            gutter.right() - 0.5,
            gutter.y_range(),
            Stroke::new(1.0, p.border),
        );

        let focused = ui.memory(|m| m.has_focus(id));
        let head_lines: Vec<usize> = ed
            .doc
            .selection
            .ranges
            .iter()
            .map(|r| ed.doc.line_of(r.head))
            .collect();
        let bracket = if main.is_empty() {
            cmd::matching_bracket(&ed.doc, main.head)
        } else {
            None
        };
        // highlightSelectionMatches: other copies of a short single-line selection.
        let sel = if !main.is_empty() && main.to() - main.from() <= 200 {
            ed.doc.slice(main.from(), main.to())
        } else {
            String::new()
        };
        let needle: Vec<char> = if !sel.contains('\n') && !sel.trim().is_empty() {
            sel.chars().collect()
        } else {
            Vec::new()
        };
        let lh = style.line_height;
        let right_edge = screen(text_x + text_w + 6.0, 0.0).x;

        #[allow(clippy::needless_range_loop)] // n is also the line number.
        for n in first..last {
            let text = ed.doc.line(n);
            let tokens = if ed.tex {
                ed.hl.tokens(n, |k| ed.doc.line(k))
            } else {
                Vec::new()
            };
            let l = lay.layout(ui, &text, &tokens);
            if (l.height - ed.heights[n]).abs() > 0.5 {
                // A guessed height was off; it is right from the next frame on.
                ed.height_cache.insert(hash(&text), l.height);
                ed.heights_key.0 = u64::MAX;
                ui.ctx().request_repaint();
            }
            let top = tops[n];
            let line_rect = Rect::from_min_size(
                screen(gutter_w, top),
                vec2(area.width() - gutter_w, l.height),
            );
            let start = ed.doc.line_start(n);
            let len = text.chars().count();
            let base = screen(text_x, top);
            let at = |col: usize| base + col_pos(&l, col, cw, tab).0.to_vec2();

            // .cm-activeLine: 6 % accent on every line with a cursor.
            if head_lines.contains(&n) {
                painter.rect_filled(line_rect, 0, Palette::mix(p.accent, 6.0));
            }
            // Indent guides under each level but the first.
            let unit = style.indent.width() as f32 * cw;
            for k in 1..indent_level(&ed.doc, n, style.indent) {
                painter.vline(
                    screen(text_x + k as f32 * unit, 0.0).x,
                    line_rect.y_range(),
                    Stroke::new(1.0, p.indent_guide()),
                );
            }
            let span = |a: usize, b: usize, fill: Color32| {
                let (pa, pb) = (at(a), at(b));
                if (pa.y - pb.y).abs() < 1.0 {
                    painter.rect_filled(
                        Rect::from_min_max(pa, pos2(pb.x.max(pa.x + 1.0), pa.y + lh)),
                        0,
                        fill,
                    );
                }
            };
            if !needle.is_empty() {
                // CodeMirror's .cm-selectionMatch colours.
                let fill = if p.dark {
                    Color32::from_rgba_unmultiplied(0xaa, 0xfe, 0x66, 0x1a)
                } else {
                    Color32::from_rgba_unmultiplied(0x99, 0xff, 0x77, 0x80)
                };
                let chars: Vec<char> = text.chars().collect();
                let mut i = 0;
                while i + needle.len() <= chars.len() {
                    if chars[i..i + needle.len()] == needle[..] {
                        if start + i != main.from() {
                            span(i, i + needle.len(), fill);
                        }
                        i += needle.len();
                    } else {
                        i += 1;
                    }
                }
            }
            if let Some(s) = &ed.search {
                for (a, b) in s.matches_between(start, start + len) {
                    let fill = if s.current() == Some((a, b)) {
                        Palette::mix(p.accent, 40.0)
                    } else {
                        Palette::mix(p.warn, 30.0)
                    };
                    span(a - start, b - start, fill);
                }
            }
            if let Some((a, b)) = bracket {
                for q in [a, b] {
                    if q >= start && q < start + len {
                        span(q - start, q - start + 1, Palette::mix(p.accent, 18.0));
                    }
                }
            }
            // Selections: 24 % accent; one running past the line's end fills to the right.
            for r in &ed.doc.selection.ranges {
                if r.is_empty() || r.to() < start || r.from() > start + len {
                    continue;
                }
                let a = r.from().max(start) - start;
                let b = r.to().min(start + len) - start;
                let past_end = r.to() > start + len;
                let (pa, ra) = col_pos(&l, a, cw, tab);
                let (pb, rb) = col_pos(&l, b, cw, tab);
                let fill = Palette::mix(p.accent, 24.0);
                for row in ra..=rb {
                    let y = base.y + row as f32 * lh;
                    let x0 = if row == ra {
                        base.x + pa.x
                    } else {
                        base.x + l.hang
                    };
                    let x1 = if row == rb && !past_end {
                        base.x + pb.x
                    } else {
                        right_edge
                    };
                    if x1 > x0 {
                        painter.rect_filled(
                            Rect::from_min_max(pos2(x0, y), pos2(x1, y + lh)),
                            0,
                            fill,
                        );
                    }
                }
            }
            painter.galley(screen(text_x + l.hang, top), l.galley.clone(), p.ink);
            // The line number, right-aligned; --ink-2 on the cursor's line.
            let color = if head_lines.contains(&n) && focused {
                p.ink_2
            } else {
                p.faint
            };
            painter.text(
                pos2(gutter.left() + numbers_w - 10.0, line_rect.top() + lh / 2.0),
                egui::Align2::RIGHT_CENTER,
                (n + 1).to_string(),
                style.font.clone(),
                color,
            );
            geo.layouts.insert(n, l);
        }

        // Cursors: 2 px accent, blinking at CodeMirror's 1.2 s, while focused.
        if focused {
            let since = now - ed.last_activity;
            if since < 0.6 || ((since / 0.6) as u64).is_multiple_of(2) {
                for r in &ed.doc.selection.ranges {
                    let n = ed.doc.line_of(r.head);
                    if let Some(l) = geo.layouts.get(&n) {
                        let pt = col_pos(l, r.head - ed.doc.line_start(n), cw, tab).0;
                        let at = screen(text_x, tops[n]) + pt.to_vec2();
                        painter.rect_filled(
                            Rect::from_min_size(pos2(at.x - 1.0, at.y), vec2(2.0, lh)),
                            0,
                            p.accent,
                        );
                    }
                }
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(
                    (0.6 - since % 0.6).max(0.05),
                ));
            // Where an input method shows its candidates, and what it is composing.
            let n = ed.doc.line_of(main.head);
            if let Some(l) = geo.layouts.get(&n) {
                let pt = col_pos(l, main.head - ed.doc.line_start(n), cw, tab).0;
                let cursor =
                    Rect::from_min_size(screen(text_x, tops[n]) + pt.to_vec2(), vec2(2.0, lh));
                if let Some(pre) = &ed.preedit {
                    let g =
                        ui.fonts_mut(|f| f.layout_no_wrap(pre.clone(), style.font.clone(), p.ink));
                    let w = g.size().x;
                    painter.rect_filled(Rect::from_min_size(cursor.min, vec2(w, lh)), 0, p.code_bg);
                    painter.galley(cursor.min, g, p.ink);
                    painter.hline(
                        cursor.min.x..=cursor.min.x + w,
                        cursor.min.y + lh - 2.0,
                        Stroke::new(1.0, p.ink),
                    );
                }
                ui.ctx().output_mut(|o| {
                    o.ime = Some(egui::output::IMEOutput {
                        purpose: egui::IMEPurpose::Normal,
                        rect: visible,
                        cursor_rect: cursor,
                        should_interrupt_composition: false,
                    })
                });
            }
        }

        // The pointer.
        let response = ui.interact(visible, id, Sense::click_and_drag());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::MultilineTextInput, true, "Editor")
        });
        if response.hovered() {
            ui.ctx().set_cursor_icon(if mods.alt {
                egui::CursorIcon::Crosshair
            } else {
                egui::CursorIcon::Text
            });
        }
        let hit = |geo: &mut Geo, ui: &Ui, d: &Doc, pos: Pos2| -> (usize, usize, f32) {
            let y = pos.y - origin.y;
            let n = tops
                .partition_point(|&t| t <= y)
                .saturating_sub(1)
                .min(d.line_count() - 1);
            let local = vec2(pos.x - screen(text_x, 0.0).x, (y - tops[n]).max(0.0));
            let (cw, tab) = (geo.lay.cw, geo.tab);
            let col = col_at(geo.line(ui, d, n), local, cw, tab);
            (
                d.line_start(n) + col.min(d.line(n).chars().count()),
                n,
                local.x,
            )
        };
        if response.is_pointer_button_down_on()
            && ed.drag.is_none()
            && ui.input(|i| i.pointer.any_pressed())
            && let Some(pos) = response.interact_pointer_pos()
        {
            ui.memory_mut(|m| m.request_focus(id));
            ed.last_activity = now;
            ed.goal_x = None;
            ed.doc.break_group();
            let (at, line, x) = hit(&mut geo, ui, &ed.doc, pos);
            let unit = if response.triple_clicked() {
                DragUnit::Line
            } else if response.double_clicked() {
                DragUnit::Word
            } else {
                DragUnit::Char
            };
            if mods.shift && !mods.command {
                let m = ed.doc.selection.main();
                ed.drag = Some(Drag {
                    origin: m.anchor,
                    rect: None,
                    unit: DragUnit::Char,
                    keep: Vec::new(),
                });
            } else {
                // Ctrl-click (⌘ on a Mac) adds a cursor; Alt-drag selects a rectangle.
                let keep = if mods.command {
                    ed.doc.selection.ranges.clone()
                } else {
                    Vec::new()
                };
                ed.drag = Some(Drag {
                    origin: at,
                    rect: mods.alt.then_some((line, x)),
                    unit,
                    keep,
                });
            }
            drag_to(ed, &mut geo, ui, at, line, x);
        } else if let Some(pos) = response
            .interact_pointer_pos()
            .filter(|_| ed.drag.is_some() && response.is_pointer_button_down_on())
        {
            let (at, line, x) = hit(&mut geo, ui, &ed.doc, pos);
            drag_to(ed, &mut geo, ui, at, line, x);
            ed.last_activity = now;
            // Dragging past an edge scrolls.
            if pos.y < visible.top() {
                ui.scroll_with_delta(vec2(0.0, lh));
            } else if pos.y > visible.bottom() {
                ui.scroll_with_delta(vec2(0.0, -lh));
            }
        }
        // A double or triple click arrives after the press: widen the selection then.
        if let Some(d) = &mut ed.drag {
            let unit = if response.triple_clicked() {
                Some(DragUnit::Line)
            } else if response.double_clicked() {
                Some(DragUnit::Word)
            } else {
                None
            };
            if let (Some(u), Some(pos)) = (unit, response.interact_pointer_pos()) {
                d.unit = u;
                let (at, line, x) = hit(&mut geo, ui, &ed.doc, pos);
                drag_to(ed, &mut geo, ui, at, line, x);
            }
        }
        if !ui.input(|i| i.pointer.any_down()) {
            ed.drag = None;
        }
        visible
    });
    let visible = scrolled.inner;

    // The keyboard.
    if ui.memory(|m| m.has_focus(id)) {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                id,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            )
        });
        let events = ui.input(|i| i.events.clone());
        let page = (visible.height() / style.line_height).floor().max(1.0) as usize;
        for e in &events {
            if handle_event(ui, ed, style, e, &mut geo, page, &mut out) {
                ed.last_activity = now;
            }
        }
    }

    // What changed this frame.
    if ed.doc.version != ed.reported_version {
        ed.reported_version = ed.doc.version;
        out.changed = true;
    }
    let line = ed.cursor_line();
    if line != ed.last_cursor_line {
        ed.last_cursor_line = line;
        out.cursor_line = Some(line);
    }
    if ed.doc.selection != ed.last_selection {
        ed.last_selection = ed.doc.selection.clone();
        out.selection = Some(ed.selected_text());
    }
    out
}

/// Moves the dragged end of the selection.
fn drag_to(ed: &mut Editor, geo: &mut Geo, ui: &Ui, at: usize, line: usize, x: f32) {
    let Some(d) = &ed.drag else { return };
    let mut ranges = d.keep.clone();
    match (d.rect, d.unit) {
        (Some((l0, x0)), _) => {
            // One range per line between the press and here, from x0 to x.
            let (a, b) = (l0.min(line), l0.max(line));
            for n in a..=b {
                let from = geo.at(ui, &ed.doc, n, 0, x0.min(x));
                let to = geo.at(ui, &ed.doc, n, 0, x0.max(x));
                ranges.push(if x < x0 {
                    Range::new(to, from)
                } else {
                    Range::new(from, to)
                });
            }
        }
        (None, DragUnit::Char) => ranges.push(Range::new(d.origin, at)),
        (None, DragUnit::Word) => {
            let (a0, b0) = cmd::word_at(&ed.doc, d.origin);
            let (a1, b1) = cmd::word_at(&ed.doc, at);
            ranges.push(if at < d.origin {
                Range::new(b0, a1)
            } else {
                Range::new(a0, b1.max(b0))
            });
        }
        (None, DragUnit::Line) => {
            let span = |p: usize| {
                let n = ed.doc.line_of(p);
                let end = if n + 1 < ed.doc.line_count() {
                    ed.doc.line_start(n + 1)
                } else {
                    ed.doc.len()
                };
                (ed.doc.line_start(n), end)
            };
            let (a0, b0) = span(d.origin);
            let (a1, b1) = span(at);
            ranges.push(if at < d.origin {
                Range::new(b0, a1)
            } else {
                Range::new(a0, b1)
            });
        }
    }
    let main = ranges.len() - 1;
    ed.doc.selection = Selection::normalized(ranges, main);
}

/// CodeMirror's Mod: Ctrl, or ⌘ on a Mac.
fn cmd_key(m: &Modifiers) -> bool {
    if MAC { m.mac_cmd } else { m.ctrl }
}

/// One input event; true when it did something.
fn handle_event(
    ui: &Ui,
    ed: &mut Editor,
    style: &Style,
    e: &Event,
    geo: &mut Geo,
    page: usize,
    out: &mut Output,
) -> bool {
    let indent = style.indent;
    match e {
        Event::Text(t) => {
            if ed.preedit.is_some() {
                return false;
            }
            let mut chars = t.chars();
            if let (Some(ch), None) = (chars.next(), chars.next())
                && style.auto_close
                && ed.tex
                && cmd::close_brackets_input(&mut ed.doc, ch)
            {
            } else {
                cmd::insert_text(&mut ed.doc, t);
            }
            ed.goal_x = None;
            ed.reveal = true;
            true
        }
        Event::Paste(t) => {
            cmd::paste(&mut ed.doc, t);
            ed.reveal = true;
            true
        }
        Event::Copy | Event::Cut => {
            let cut = matches!(e, Event::Cut);
            // An empty selection copies (or cuts) its whole line, as CodeMirror does.
            let lines = ed.doc.selection.ranges.iter().all(|r| r.is_empty());
            let text = if lines {
                ed.doc
                    .selection
                    .ranges
                    .iter()
                    .map(|r| ed.doc.line(ed.doc.line_of(r.head)) + "\n")
                    .collect::<String>()
            } else {
                ed.doc
                    .selection
                    .ranges
                    .iter()
                    .map(|r| ed.doc.slice(r.from(), r.to()))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            ui.ctx().copy_text(text);
            if cut {
                if lines {
                    cmd::delete_line(&mut ed.doc);
                } else {
                    cmd::delete_char_forward(&mut ed.doc);
                }
                ed.reveal = true;
            }
            true
        }
        Event::Ime(ime) => {
            match ime {
                egui::ImeEvent::Preedit { text, .. } => {
                    ed.preedit = (!text.is_empty()).then(|| text.clone())
                }
                egui::ImeEvent::Commit(text) => {
                    ed.preedit = None;
                    if !text.is_empty() {
                        cmd::insert_text(&mut ed.doc, text);
                        ed.reveal = true;
                    }
                }
                egui::ImeEvent::DeleteSurrounding {
                    before_chars,
                    after_chars,
                } => {
                    let (b, a) = (*before_chars, *after_chars);
                    let (c, s) = ed.doc.change_by_range(|d, r| {
                        let from = r.head.saturating_sub(b);
                        let to = (r.head + a).min(d.len());
                        (
                            vec![super::buffer::Change::delete(from, to)],
                            Range::cursor(from),
                        )
                    });
                    ed.doc.apply(c, s, super::buffer::EditKind::Delete);
                }
                #[allow(deprecated)]
                egui::ImeEvent::Enabled | egui::ImeEvent::Disabled => ed.preedit = None,
            }
            true
        }
        Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => key_press(ui, ed, style, *key, *modifiers, geo, page, out, indent),
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn key_press(
    ui: &Ui,
    ed: &mut Editor,
    style: &Style,
    key: Key,
    m: Modifiers,
    geo: &mut Geo,
    page: usize,
    out: &mut Output,
    indent: Indent,
) -> bool {
    let d = &mut ed.doc;
    let shift = m.shift;
    let modk = cmd_key(&m);
    let is = |spec: &str| shortcuts::matches_spec(spec, m, key);
    let keep_goal = matches!(
        key,
        Key::ArrowUp | Key::ArrowDown | Key::PageUp | Key::PageDown
    ) && !m.alt;
    if !keep_goal {
        ed.goal_x = None;
    }
    // The formatting keys of TeX files come first (editor/shortcuts.ts formattingKeymap).
    if ed.tex {
        if is(shortcuts::keys_of("bold")) {
            cmd::toggle_wrap(d, "textbf");
            return reveal(ed);
        }
        if is(shortcuts::keys_of("italic")) {
            cmd::toggle_wrap(d, "textit");
            return reveal(ed);
        }
        if is(shortcuts::keys_of("heading-down")) {
            return cmd::shift_heading(d, 1);
        }
        if is(shortcuts::keys_of("heading-up")) {
            return cmd::shift_heading(d, -1);
        }
        if is(shortcuts::keys_of("wrap-env")) {
            out.wrap_environment = true;
            return true;
        }
        if is(shortcuts::keys_of("next-problem")) {
            out.next_problem = Some(1);
            return true;
        }
        if is(shortcuts::keys_of("prev-problem")) {
            out.next_problem = Some(-1);
            return true;
        }
    }
    if is(shortcuts::keys_of("comment")) {
        cmd::toggle_comment(d);
        return reveal(ed);
    }
    if is(shortcuts::keys_of("sync")) {
        let h = d.selection.main().head;
        let n = d.line_of(h);
        out.sync = Some((n + 1, h - d.line_start(n)));
        return true;
    }
    // The search keys (searchKeymap).
    if is("Mod-f") {
        ed.search = Some(Search::open(ed.search.take(), &ed.selected_text(), false));
        return true;
    }
    if is("Mod-Alt-g") {
        ed.search = Some(Search::goto_line(ed.search.take()));
        return true;
    }
    if is("Mod-g") || is("F3") {
        search::find_next(ed, true);
        return true;
    }
    if is("Shift-Mod-g") || is("Shift-F3") {
        search::find_next(ed, false);
        return true;
    }
    if is("Mod-Shift-l") {
        search::select_all_matches(ed);
        return true;
    }
    if is("Mod-d") {
        cmd::select_next_occurrence(d);
        return reveal(ed);
    }
    // History.
    if is("Mod-z") {
        d.undo();
        return reveal(ed);
    }
    if is("Mod-y") || is("Mod-Shift-z") {
        d.redo();
        return reveal(ed);
    }
    match key {
        Key::ArrowLeft | Key::ArrowRight => {
            let right = key == Key::ArrowRight;
            if MAC && m.mac_cmd {
                vertical_boundary(ed, geo, ui, right, shift);
            } else if (modk && !MAC) || (MAC && m.alt) || (!MAC && m.alt) {
                if right {
                    cmd::group_right(&mut ed.doc, shift)
                } else {
                    cmd::group_left(&mut ed.doc, shift)
                }
            } else if right {
                cmd::char_right(&mut ed.doc, shift);
            } else {
                cmd::char_left(&mut ed.doc, shift);
            }
            reveal(ed)
        }
        Key::ArrowUp | Key::ArrowDown => {
            let down = key == Key::ArrowDown;
            if m.alt && modk {
                add_cursor_vertical(ed, geo, ui, down);
            } else if m.alt && shift {
                cmd::copy_lines(&mut ed.doc, down);
            } else if m.alt {
                cmd::move_lines(&mut ed.doc, down);
            } else if MAC && m.mac_cmd {
                if down {
                    cmd::doc_end(&mut ed.doc, shift)
                } else {
                    cmd::doc_start(&mut ed.doc, shift)
                }
            } else {
                move_vertical(ed, geo, ui, down, shift, 1);
            }
            reveal(ed)
        }
        Key::PageUp | Key::PageDown => {
            move_vertical(ed, geo, ui, key == Key::PageDown, shift, page);
            reveal(ed)
        }
        Key::Home if modk => {
            cmd::doc_start(&mut ed.doc, shift);
            reveal(ed)
        }
        Key::End if modk => {
            cmd::doc_end(&mut ed.doc, shift);
            reveal(ed)
        }
        Key::Home | Key::End => {
            vertical_boundary(ed, geo, ui, key == Key::End, shift);
            reveal(ed)
        }
        Key::Enter if !modk && !m.alt => {
            cmd::insert_newline_and_indent(&mut ed.doc, indent);
            reveal(ed)
        }
        Key::Backspace => {
            if modk || (MAC && m.alt) {
                cmd::delete_group_backward(&mut ed.doc);
            } else {
                cmd::delete_char_backward(&mut ed.doc, indent, style.auto_close && ed.tex);
            }
            reveal(ed)
        }
        Key::Delete => {
            if modk || (MAC && m.alt) {
                cmd::delete_group_forward(&mut ed.doc);
            } else {
                cmd::delete_char_forward(&mut ed.doc);
            }
            reveal(ed)
        }
        Key::A if modk && !shift && !m.alt => {
            cmd::select_all(&mut ed.doc);
            true
        }
        Key::Escape => {
            if ed.search.is_some() {
                ed.search = None;
            } else {
                cmd::simplify_selection(&mut ed.doc);
            }
            true
        }
        Key::L if m.alt && !modk => {
            cmd::select_line(&mut ed.doc);
            true
        }
        Key::K if modk && shift => {
            cmd::delete_line(&mut ed.doc);
            reveal(ed)
        }
        Key::Backslash if modk && shift => {
            cmd::cursor_matching_bracket(&mut ed.doc, false);
            reveal(ed)
        }
        Key::OpenBracket | Key::CloseBracket if modk && !shift => {
            // Outside TeX files (where they change heading levels): indentation.
            if key == Key::CloseBracket {
                cmd::indent_more(&mut ed.doc, indent)
            } else {
                cmd::indent_less(&mut ed.doc, indent)
            }
            reveal(ed)
        }
        Key::Tab if !modk && !m.alt => {
            // indentWithTab: Tab indents the lines, Shift-Tab outdents them.
            if shift {
                cmd::indent_less(&mut ed.doc, indent)
            } else {
                cmd::indent_more(&mut ed.doc, indent)
            }
            reveal(ed)
        }
        _ => false,
    }
}

fn reveal(ed: &mut Editor) -> bool {
    ed.reveal = true;
    true
}

/// Up/Down by visual rows, keeping the goal x across presses.
fn move_vertical(ed: &mut Editor, geo: &mut Geo, ui: &Ui, down: bool, extend: bool, times: usize) {
    let mut goal = ed.goal_x;
    let ranges: Vec<Range> = ed
        .doc
        .selection
        .ranges
        .iter()
        .map(|r| {
            // Without Shift, a selection first collapses to its edge in the direction of travel.
            let mut head = if !extend && !r.is_empty() {
                if down { r.to() } else { r.from() }
            } else {
                r.head
            };
            let mut g = goal;
            for _ in 0..times {
                let (h, x) = geo.vertical(ui, &ed.doc, head, down, g);
                head = h;
                g = Some(x);
            }
            goal = g;
            if extend {
                Range::new(r.anchor, head)
            } else {
                Range::cursor(head)
            }
        })
        .collect();
    ed.goal_x = goal;
    ed.doc.selection = Selection::normalized(ranges, ed.doc.selection.main);
    ed.doc.break_group();
}

/// Home/End: to the start or end of the visual row, then of the line (Home: the indentation first).
fn vertical_boundary(ed: &mut Editor, geo: &mut Geo, ui: &Ui, forward: bool, extend: bool) {
    let main = ed.doc.selection.main();
    let (n, row, _) = geo.place(ui, &ed.doc, main.head);
    let rows = geo.rows(ui, &ed.doc, n);
    let row_start = geo.at(ui, &ed.doc, n, row, -1e6);
    let row_end = geo.at(ui, &ed.doc, n, row, 1e6);
    if rows > 1 {
        let target = if forward { row_end } else { row_start };
        if target != main.head && !(forward && row + 1 == rows) && !(!forward && row == 0) {
            cmd::move_ranges(&mut ed.doc, extend, |_, _| target);
            return;
        }
    }
    if forward {
        cmd::line_boundary_forward(&mut ed.doc, extend)
    } else {
        cmd::line_boundary_backward(&mut ed.doc, extend)
    }
}

/// Mod-Alt-Up/Down: another cursor a row above or below the main one.
fn add_cursor_vertical(ed: &mut Editor, geo: &mut Geo, ui: &Ui, down: bool) {
    let main = ed.doc.selection.main();
    let (h, _) = geo.vertical(ui, &ed.doc, main.head, down, None);
    if h == main.head {
        return;
    }
    let mut ranges = ed.doc.selection.ranges.clone();
    ranges.push(Range::cursor(h));
    let n = ranges.len() - 1;
    ed.doc.selection = Selection::normalized(ranges, n);
}

/// Marks an edit made from outside (a toolbar button) as its own undo step and shows its cursor.
pub fn after_command(ed: &mut Editor) {
    ed.doc.break_group();
    ed.reveal = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;
    use crate::theme;
    use egui_kittest::kittest::Queryable;

    pub fn style() -> Style {
        Style {
            font: FontId::monospace(13.0),
            line_height: 13.0 * 1.65,
            indent: Indent::Spaces(4),
            wrap: true,
            auto_close: true,
            focus: false,
        }
    }

    #[test]
    fn draws_and_takes_typing() {
        let text = include_str!("../../../../tests/fixtures/article/main.tex");
        let mut ed = Editor::new(text, true);
        ed.doc.selection = Selection::cursor(0);
        let p = theme::LIGHT;
        let mut h = egui_kittest::Harness::builder()
            .with_size([620.0, 420.0])
            .with_context_setup(move |ctx| {
                theme::install_fonts(ctx);
                theme::apply(ctx, &p);
            })
            .wgpu()
            .build_ui_state(
                move |ui, ed: &mut Editor| {
                    let rect = ui.ctx().content_rect();
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                    show(&mut child, &p, ed, &style(), egui::Id::unique("ed"));
                },
                ed,
            );
        h.run();
        let _ = testing::WINDOW;
        h.get_by_label("Editor").click();
        h.run();
        h.get_by_label("Editor").type_text("% hello");
        h.run();
        assert!(h.state().doc.text().contains("% hello"));
        h.snapshot("editor-alone");
    }
}
