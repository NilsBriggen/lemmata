//! The editor widget: what CodeMirror's EditorView drew for Editor.svelte. Lines wrap with a
//! hanging indent and show indent guides (editor/indentGuides.ts), the gutter has line numbers and
//! the lint and fold columns, and the colours are Editor.svelte's theme.

use super::buffer::{Doc, Range, Selection};
use super::commands::{self as cmd, Indent};
use super::complete::{self, Apply, Lookups};
use super::envs;
use super::fold::{self, Folds};
use super::highlight::{Highlighter, Token};
use super::popup::{self, Completion};
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
    /// Sections, environments and comment blocks can fold.
    pub folding: bool,
    /// Renaming `\begin{x}` renames its `\end{x}`; Enter closes an open environment.
    pub env_sync: bool,
    /// The completion list opens while typing.
    pub autocomplete: bool,
    /// The maths under the cursor is previewed above it.
    pub math_preview: bool,
    /// Focus mode: the text in a centred column at most 88 characters wide.
    pub focus: bool,
}

/// A problem from the last build, shown in the text and the lint gutter.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub from: usize,
    pub to: usize,
    pub severity: Severity,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Info,
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
    /// A completion inserted something that needs this package.
    pub needs_package: Option<String>,
    /// A screenshot was pasted, to save in the project and insert as a figure.
    pub pasted_image: Option<std::sync::Arc<egui::ColorImage>>,
    /// Pasted cells became a table: where, the table, and the text as it was pasted.
    pub pasted_table: Option<(usize, String, String)>,
    /// Ask texlab for commands at this cursor, the name starting at `from`.
    pub server_complete: Option<(usize, usize)>,
    /// Ask texlab what is at this line and column (0-based chars).
    pub server_hover: Option<(usize, usize)>,
    /// F12 / Shift-F12 at the cursor: (line, column), and whether it's references.
    pub server_goto: Option<(usize, usize, bool)>,
    /// "Add to dictionary" on a misspelled word.
    pub add_word: Option<String>,
    /// Paragraph texts to send to LanguageTool.
    pub grammar_check: Vec<String>,
    /// "Ignore this rule" on a grammar finding.
    pub ignore_rule: Option<String>,
    /// Commented text was clicked: the thread's id.
    pub comment_click: Option<String>,
    /// A search button on a hover card for a name the project doesn't define.
    pub hover_search: Option<super::hover::Search>,
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
    /// The file, relative to the project; matches texlab's answers to this editor.
    pub path: String,
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
    /// Take the keyboard focus on the next frame (after a dialog that inserted text closes).
    pub refocus: bool,
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
    pub folds: Folds,
    /// Bumped when folds change, so heights are measured again.
    folds_gen: u64,
    heights_folds: u64,
    hidden: Vec<bool>,
    /// Which lines can fold, per document version.
    foldable: (u64, HashMap<usize, bool>),
    /// A snippet whose fields Tab steps through.
    pub snippet: Option<super::snippet::Active>,
    pub completion: Option<Completion>,
    /// Typing (false) or Ctrl-Space (true) asked for completion this frame.
    complete_wanted: Option<bool>,
    /// `\begin`/`\end` spans without a partner, and when they were last computed.
    unmatched: (u64, Vec<(usize, usize)>),
    pub problems: Vec<Problem>,
    /// Which build's problems these are.
    pub problems_gen: u64,
    /// The last maths preview that rendered, shown faded under an error while typing.
    last_math: Option<crate::writing::render::Math>,
    /// The prose scanner's state at the start of each line, valid for the first `.1` lines.
    prose: (Vec<crate::writing::prose::State>, usize),
    /// The suggestions menu of a misspelled word.
    spell_menu: Option<SpellMenu>,
    /// The grammar tooltip on screen and where it hangs.
    grammar_tip: Option<(GrammarMark, Pos2)>,
    /// Paragraphs for grammar checking, for a document version, and when to ask about them.
    grammar_paras: (u64, Vec<crate::writing::grammar::Paragraph>),
    grammar_due: f64,
    /// Grammar findings in the source, for a (document version, findings generation).
    grammar_marks: ((u64, u64), Vec<GrammarMark>),
    /// A completion asked of texlab: the cursor and whether it was Ctrl-Space; its answer opens
    /// the list even when nothing local matched.
    awaiting_server: Option<(usize, bool)>,
    /// Review comments on this file: thread id, char range, whether it's the active one.
    pub comments: Vec<(String, usize, usize, bool)>,
    /// What the comment ranges were computed for (comments generation, document version).
    pub comments_key: (u64, u64),
    /// The hover card on screen: its line, what it shows, and where the card was drawn.
    hover: Option<(usize, super::hover::Target, Rect)>,
}

impl Editor {
    pub fn new(text: &str, tex: bool) -> Self {
        Self {
            path: String::new(),
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
            refocus: false,
            center_line: None,
            last_activity: 0.0,
            drag: None,
            preedit: None,
            search: None,
            reported_version: 0,
            last_cursor_line: 1,
            last_selection: Selection::cursor(0),
            scroll_y: 0.0,
            folds: Folds::default(),
            folds_gen: 0,
            heights_folds: u64::MAX,
            hidden: Vec::new(),
            foldable: (u64::MAX, HashMap::new()),
            snippet: None,
            completion: None,
            complete_wanted: None,
            unmatched: (u64::MAX, Vec::new()),
            problems: Vec::new(),
            problems_gen: u64::MAX,
            last_math: None,
            hover: None,
            comments: Vec::new(),
            comments_key: (u64::MAX, u64::MAX),
            awaiting_server: None,
            prose: (Vec::new(), 0),
            spell_menu: None,
            grammar_tip: None,
            grammar_paras: (u64::MAX, Vec::new()),
            grammar_due: 0.0,
            grammar_marks: ((u64::MAX, u64::MAX), Vec::new()),
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

    /// Inserts a template at the selection (a toolbar button, a dialog), as a snippet.
    pub fn insert_template(&mut self, template: &str, indent: Indent) {
        let fields = cmd::insert_template(&mut self.doc, template, indent);
        self.snippet = super::snippet::Active::start(fields);
        after_command(self);
    }

    /// A figure or table from a dialog, on its own lines, as it is: the cursor goes after it.
    pub fn insert_literal_block(&mut self, text: &str, indent: Indent) {
        cmd::insert_block(&mut self.doc, text, indent, false);
        after_command(self);
        self.refocus = true;
    }

    /// Replaces `from..to` (chars) with `text`, the cursor after it.
    pub fn replace_range(&mut self, from: usize, to: usize, text: &str) {
        let end = from + text.chars().count();
        self.doc.apply(
            super::buffer::ChangeSet::new(vec![super::buffer::Change {
                from,
                to,
                insert: text.into(),
            }]),
            super::buffer::Selection::cursor(end),
            super::buffer::EditKind::Other,
        );
        after_command(self);
    }

    /// Reindent lines: the selection, or the whole file.
    pub fn reindent_lines(&mut self, indent: Indent) {
        cmd::reindent_lines(&mut self.doc, indent);
        after_command(self);
        self.refocus = true;
    }

    /// The citation picker's keys, at the cursor.
    pub fn insert_citation(&mut self, keys: &[String]) {
        cmd::insert_citation(&mut self.doc, keys);
        after_command(self);
        self.refocus = true;
    }

    /// Replaces `from..to` with a snippet (a completion).
    pub fn apply_snippet(&mut self, from: usize, to: usize, template: &str, indent: Indent) {
        let fields = super::snippet::insert(&mut self.doc, from, to, template, indent);
        self.snippet = super::snippet::Active::start(fields);
        self.reveal = true;
    }

    /// Ends the snippet; extra cursors from linked fields go with it.
    fn end_snippet(&mut self) {
        if self.snippet.take().is_some() && self.doc.selection.ranges.len() > 1 {
            let m = self.doc.selection.main();
            self.doc.selection = Selection::single(m);
        }
    }

    /// Shows a build's problems: each on its whole line (1-based), like Editor.svelte showDiagnostics.
    pub fn set_problems(&mut self, generation: u64, list: &[(usize, Severity, String)]) {
        self.problems_gen = generation;
        let lines = self.doc.line_count();
        self.problems = list
            .iter()
            .map(|(line, severity, message)| {
                let n = line.clamp(&1, &lines) - 1;
                Problem {
                    from: self.doc.line_start(n),
                    to: self.doc.line_end(n),
                    severity: *severity,
                    message: message.clone(),
                }
            })
            .collect();
    }

    /// F8 / Shift-F8: selects the next (or previous) problem, wrapping around.
    fn next_problem(&mut self, forward: bool) -> bool {
        if self.problems.is_empty() {
            return false;
        }
        let head = self.doc.selection.main().head;
        let mut sorted: Vec<&Problem> = self.problems.iter().collect();
        sorted.sort_by_key(|p| p.from);
        let pick = if forward {
            sorted.iter().find(|p| p.from > head).or(sorted.first())
        } else {
            sorted.iter().rev().find(|p| p.to < head).or(sorted.last())
        };
        if let Some(p) = pick {
            self.doc.selection = Selection::single(Range::new(p.from, p.to));
            self.reveal = true;
        }
        true
    }

    /// Folds the block starting on line `n` (0-based), or unfolds it when it is folded.
    pub fn toggle_fold(&mut self, n: usize) {
        if !self.folds.unfold_line(&self.doc, n) {
            self.folds.fold(&self.doc, n);
            // A cursor inside goes to the fold's first line.
            if let Some((from, to)) = self.folds.starting(&self.doc, n) {
                let main = self.doc.selection.main();
                if main.head > from && main.head <= to {
                    self.doc.selection = Selection::cursor(from);
                }
            }
        }
        self.folds_gen += 1;
    }

    /// Folds these lines (1-based), as the session remembered them.
    pub fn set_folds(&mut self, lines: &[usize]) {
        for &l in lines {
            if l >= 1 && l <= self.doc.line_count() {
                self.folds.fold(&self.doc, l - 1);
            }
        }
        self.folds_gen += 1;
    }

    fn can_fold(&mut self, n: usize) -> bool {
        if self.foldable.0 != self.doc.version {
            self.foldable = (self.doc.version, HashMap::new());
        }
        let doc = &self.doc;
        *self
            .foldable
            .1
            .entry(n)
            .or_insert_with(|| fold::fold_lines(doc, n).is_some())
    }

    /// The text of the main selection.
    pub fn selected_text(&self) -> String {
        let r = self.doc.selection.main();
        self.doc.slice(r.from(), r.to())
    }
}

/// The suggestions menu of a misspelled word (`.lm-spell-menu`).
struct SpellMenu {
    word: String,
    from: usize,
    to: usize,
    at: Pos2,
    /// Filled in by a thread: suggestions take a moment.
    suggestions: std::sync::Arc<std::sync::Mutex<Option<Vec<String>>>>,
}

/// A grammar finding where it is in the source.
#[derive(Clone)]
struct GrammarMark {
    from: usize,
    to: usize,
    /// The source is exactly the flagged text, so replacements apply.
    editable: bool,
    flagged: String,
    message: String,
    replacements: Vec<String>,
    rule: String,
}

impl Editor {
    /// The prose scanner's state at the start of line `n`, scanning forward as needed.
    fn prose_state(&mut self, n: usize) -> crate::writing::prose::State {
        let (states, valid) = &mut self.prose;
        if states.is_empty() {
            states.push(Default::default());
            *valid = 1;
        }
        states.truncate((*valid).max(1));
        while states.len() <= n {
            let k = states.len() - 1;
            let mut st = states[k].clone();
            crate::writing::prose::line_words(&self.doc.line(k), &mut st, 0);
            states.push(st);
        }
        *valid = states.len();
        states[n].clone()
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
    /// Lines a fold hides; Up and Down step over them.
    hidden: &'a [bool],
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
        let shown = |k: usize| !self.hidden.get(k).copied().unwrap_or(false);
        let next = (n + 1..d.line_count()).find(|&k| shown(k));
        let prev = (0..n).rev().find(|&k| shown(k));
        let target = if down {
            if row + 1 < self.rows(ui, d, n) {
                Some((n, row + 1))
            } else {
                next.map(|k| (k, 0))
            }
        } else if row > 0 {
            Some((n, row - 1))
        } else if let Some(k) = prev {
            let r = self.rows(ui, d, k);
            Some((k, r - 1))
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

/// `label` names the editor for screen readers ("Text of main.tex").
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    p: &Palette,
    ed: &mut Editor,
    style: &Style,
    id: egui::Id,
    label: &str,
    look: &Lookups,
    checks: &mut crate::writing::Checks,
) -> Output {
    let mut out = Output::default();
    let journal = std::mem::take(&mut ed.doc.journal);
    if !journal.is_empty() {
        ed.folds.map(&journal);
        ed.folds_gen += 1;
        if let Some(s) = &mut ed.snippet {
            s.map(&journal);
        }
        let first_line = journal
            .iter()
            .filter_map(|c| c.0.first())
            .map(|c| ed.doc.line_of(c.from.min(ed.doc.len())))
            .min()
            .unwrap_or(0);
        ed.prose.1 = ed.prose.1.min(first_line + 1);
        for u in &mut ed.unmatched.1 {
            *u = (
                super::buffer::Doc::map_through(&journal, u.0, -1),
                super::buffer::Doc::map_through(&journal, u.1, 1),
            );
        }
        for pr in &mut ed.problems {
            pr.from = super::buffer::Doc::map_through(&journal, pr.from, -1);
            pr.to = super::buffer::Doc::map_through(&journal, pr.to, 1).max(pr.from);
        }
    }
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
    if ed.heights_key != hkey
        || ed.heights.len() != ed.doc.line_count()
        || ed.heights_folds != ed.folds_gen
    {
        let n = ed.doc.line_count();
        ed.hidden = ed.folds.hidden(&ed.doc);
        ed.heights_folds = ed.folds_gen;
        ed.heights.clear();
        for i in 0..n {
            if ed.hidden[i] {
                ed.heights.push(0.0);
                continue;
            }
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

    let hidden = ed.hidden.clone();
    let mut geo = Geo {
        lay: &lay,
        layouts: HashMap::new(),
        tab,
        hidden: &hidden,
    };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
    let now = ui.input(|i| i.time);
    let mods = ui.input(|i| i.modifiers);

    // Unpaired \begin/\end: looked for again half a second after typing stops.
    if ed.tex && style.env_sync && ed.unmatched.0 != ed.doc.version {
        let idle = ui.input(|i| i.time) - ed.last_activity;
        if idle > 0.5 {
            ed.unmatched = (ed.doc.version, envs::unmatched(&ed.doc));
        } else {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(0.5 - idle + 0.01));
        }
    }
    // Grammar: paragraphs near the screen are sent 1.5 s after typing stops (TYPING_DELAY).
    let now_t = ui.input(|i| i.time);
    if ed.tex && checks.grammar.target.is_some() {
        if ed.grammar_paras.0 != ed.doc.version {
            if ed.grammar_paras.0 != u64::MAX && now_t - ed.last_activity < 1.5 {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs_f64(
                        1.5 - (now_t - ed.last_activity) + 0.01,
                    ));
            } else {
                ed.grammar_paras = (
                    ed.doc.version,
                    crate::writing::grammar::paragraphs(&ed.doc.text()),
                );
                ed.grammar_due = now_t;
            }
        }
        let key = (ed.grammar_paras.0, checks.grammar.generation);
        if ed.grammar_marks.0 != key {
            let src: Vec<char> = ed.doc.text().chars().collect();
            let mut marks = Vec::new();
            for para in &ed.grammar_paras.1 {
                let text: String = para.text.iter().collect();
                for m in checks.grammar.found(&text).unwrap_or_default() {
                    let off = crate::writing::grammar::utf16_to_char(&para.text, m.offset);
                    let end =
                        crate::writing::grammar::utf16_to_char(&para.text, m.offset + m.length);
                    if let Some((from, to, editable)) =
                        crate::writing::grammar::to_source(para, off, end - off, &src)
                    {
                        marks.push(GrammarMark {
                            from,
                            to,
                            editable,
                            flagged: src[from..to.min(src.len())].iter().collect(),
                            message: m.message.clone(),
                            replacements: m.replacements.clone(),
                            rule: m.rule_id.clone(),
                        });
                    }
                }
            }
            ed.grammar_marks = (key, marks);
        }
    } else if !ed.grammar_marks.1.is_empty() {
        ed.grammar_marks = ((u64::MAX, u64::MAX), Vec::new());
        ed.grammar_paras = (u64::MAX, Vec::new());
    }
    let mut popup_anchor: Option<Pos2> = None;
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
        let fold_on = style.folding;
        let mut fold_clicks: Vec<(Rect, usize)> = Vec::new();
        let mut lint_marks: Vec<(Rect, usize)> = Vec::new();
        let mut spell_words: Vec<(usize, usize, String)> = Vec::new();
        // Paragraphs near the screen (40 lines around it) go to LanguageTool once.
        if ed.grammar_due > 0.0 && checks.grammar.target.is_some() {
            ed.grammar_due = 0.0;
            let lo = ed.doc.line_start(first.saturating_sub(40));
            let hi = ed.doc.line_end((last + 40).min(ed.doc.line_count()) - 1);
            for para in ed
                .grammar_paras
                .1
                .iter()
                .filter(|pp| pp.from <= hi && pp.to >= lo)
            {
                let t: String = para.text.iter().collect();
                if !checks.grammar.asked.contains(&t) {
                    out.grammar_check.push(t);
                }
            }
        }

        #[allow(clippy::needless_range_loop)] // n is also the line number.
        for n in first..last {
            if ed.heights[n] == 0.0 {
                continue;
            }
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
            // .cm-comment: commented text on a yellow ground with a darker line under it.
            for (_, a, b, active) in ed
                .comments
                .iter()
                .filter(|c| c.2 > start && c.1 < start + len)
            {
                let (pa, ra) = col_pos(&l, a.max(&start) - start, cw, tab);
                let (pb, rb) = col_pos(&l, (*b).min(start + len) - start, cw, tab);
                let fill =
                    Color32::from_rgba_unmultiplied(240, 190, 40, if *active { 107 } else { 56 });
                for row in ra..=rb {
                    let y = base.y + row as f32 * lh;
                    let x0 = if row == ra {
                        base.x + pa.x
                    } else {
                        base.x + l.hang
                    };
                    let x1 = if row == rb { base.x + pb.x } else { right_edge };
                    let r = Rect::from_min_max(pos2(x0, y), pos2(x1, y + lh));
                    painter.rect_filled(r, 0, fill);
                    painter.hline(
                        r.x_range(),
                        r.bottom() - 1.0,
                        Stroke::new(2.0, Color32::from_rgba_unmultiplied(220, 160, 20, 179)),
                    );
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
            // .cm-lintRange: a wavy underline under each problem on this line.
            for pr in ed
                .problems
                .iter()
                .filter(|pr| pr.to >= start && pr.from <= start + len)
            {
                let a = pr.from.max(start) - start;
                let b = (pr.to.min(start + len) - start).max(a);
                let (pa, ra) = col_pos(&l, a, cw, tab);
                let (pb, rb) = col_pos(&l, b, cw, tab);
                for row in ra..=rb {
                    let y = base.y + row as f32 * lh + lh / 2.0 + style.font.size * 0.62;
                    let x0 = if row == ra {
                        base.x + pa.x
                    } else {
                        base.x + l.hang
                    };
                    let x1 = if row == rb {
                        base.x + pb.x
                    } else {
                        right_edge - 6.0
                    };
                    wavy(
                        &painter,
                        x0,
                        x1.max(x0 + 6.0),
                        y,
                        severity_color(pr.severity, p),
                    );
                }
            }
            // .cm-unmatched-env: a dotted --warn underline.
            if style.env_sync {
                for &(a, b) in ed
                    .unmatched
                    .1
                    .iter()
                    .filter(|u| u.1 > start && u.0 < start + len)
                {
                    let (pa, _) = col_pos(&l, a.max(start) - start, cw, tab);
                    let (pb, _) = col_pos(&l, b.min(start + len) - start, cw, tab);
                    let y = base.y + lh / 2.0 + style.font.size * 0.62 + 1.0;
                    let mut x = base.x + pa.x;
                    while x < base.x + pb.x {
                        painter.hline(
                            x..=(x + 1.0).min(base.x + pb.x),
                            y,
                            Stroke::new(1.0, p.warn),
                        );
                        x += 2.5;
                    }
                }
            }
            // .cm-misspelled: a wavy --error line under each misspelled prose word.
            if ed.tex && checks.spell.lang.is_some() {
                let mut st = ed.prose_state(n);
                for w in crate::writing::prose::line_words(&text, &mut st, start) {
                    if checks.spell.bad(&w.text) {
                        let (pa, ra) = col_pos(&l, w.from - start, cw, tab);
                        let (pb, rb) = col_pos(&l, w.to - start, cw, tab);
                        if ra == rb {
                            let y = base.y + ra as f32 * lh + lh / 2.0 + style.font.size * 0.62;
                            wavy(&painter, base.x + pa.x, base.x + pb.x, y, p.error);
                        }
                        spell_words.push((w.from, w.to, w.text));
                    }
                }
            }
            // .cm-grammar: a dotted 2 px accent line under grammar findings.
            for g in ed
                .grammar_marks
                .1
                .iter()
                .filter(|g| g.to > start && g.from < start + len)
            {
                let (pa, ra) = col_pos(&l, g.from.max(start) - start, cw, tab);
                let (pb, rb) = col_pos(&l, g.to.min(start + len) - start, cw, tab);
                for row in ra..=rb {
                    let y = base.y + row as f32 * lh + lh / 2.0 + style.font.size * 0.62 + 1.0;
                    let x0 = if row == ra {
                        base.x + pa.x
                    } else {
                        base.x + l.hang
                    };
                    let x1 = if row == rb {
                        base.x + pb.x
                    } else {
                        right_edge - 6.0
                    };
                    let mut x = x0;
                    while x < x1 {
                        painter.rect_filled(
                            Rect::from_min_size(pos2(x, y - 1.0), vec2(2.0, 2.0)),
                            0,
                            p.accent,
                        );
                        x += 4.0;
                    }
                }
            }
            // .cm-lint-marker in the gutter, the worst problem of the line.
            if let Some(worst) = ed
                .problems
                .iter()
                .filter(|pr| ed.doc.line_of(pr.from) == n)
                .map(|pr| pr.severity)
                .min_by_key(|s| *s as u8)
            {
                let r = Rect::from_center_size(
                    pos2(
                        gutter.left() + numbers_w + LINT_EM * em / 2.0,
                        line_rect.top() + lh / 2.0,
                    ),
                    vec2(0.8 * em, 0.8 * em),
                );
                lint_marker(&painter, r, worst);
                lint_marks.push((r.expand(2.0), n));
            }
            // .cm-snippetFieldPosition: a dotted line where an empty field waits.
            if let Some(sn) = &ed.snippet {
                for &(a, b) in sn.fields.iter().flatten() {
                    if a == b && a >= start && a <= start + len {
                        let pt = at(a - start);
                        let mut y = pt.y + 2.0;
                        while y < pt.y + lh - 2.0 {
                            painter.vline(
                                pt.x,
                                y..=(y + 1.4).min(pt.y + lh - 2.0),
                                Stroke::new(1.4, Color32::from_gray(0x88)),
                            );
                            y += 2.8;
                        }
                    }
                }
            }
            // .cm-foldPlaceholder: "…" in an accent pill after a folded line; a click unfolds.
            if ed.folds.starting(&ed.doc, n).is_some() {
                let end = at(len);
                let g =
                    ui.fonts_mut(|f| f.layout_no_wrap("…".into(), style.font.clone(), p.accent));
                let pill = Rect::from_min_size(
                    pos2(end.x + 2.0, end.y + (lh - em * 1.25) / 2.0),
                    vec2(g.size().x + 12.0, em * 1.25),
                );
                painter.rect_filled(pill, 4, p.accent_soft);
                painter.galley(pill.center() - g.size() / 2.0, g, p.accent);
                fold_clicks.push((pill, n));
            }
            // The fold gutter: ▾ on lines that can fold, ▸ on folded ones.
            if ed.tex && fold_on {
                let folded = ed.folds.starting(&ed.doc, n).is_some();
                if folded || ed.can_fold(n) {
                    let r = Rect::from_min_size(
                        pos2(gutter.left() + numbers_w + LINT_EM * em, line_rect.top()),
                        vec2(0.6 * em + 6.0, lh),
                    );
                    let hover = ui.rect_contains_pointer(r);
                    // ▸ / ▾ at 12 px, drawn as triangles (the UI fonts don't have them).
                    let c = r.center();
                    let (w, hh) = (3.0, 4.5);
                    let pts = if folded {
                        vec![
                            pos2(c.x - w + 1.0, c.y - hh + 1.0),
                            pos2(c.x + w, c.y),
                            pos2(c.x - w + 1.0, c.y + hh - 1.0),
                        ]
                    } else {
                        vec![
                            pos2(c.x - hh + 1.0, c.y - w + 1.0),
                            pos2(c.x + hh - 1.0, c.y - w + 1.0),
                            pos2(c.x, c.y + w),
                        ]
                    };
                    painter.add(egui::Shape::convex_polygon(
                        pts,
                        if hover { p.ink } else { p.faint },
                        Stroke::NONE,
                    ));
                    fold_clicks.push((r, n));
                }
            }
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

        // The problems of a line, on hovering its marker.
        if let Some(pos) = ui.input(|i| i.pointer.hover_pos())
            && let Some(&(r, n)) = lint_marks.iter().find(|(r, _)| r.contains(pos))
        {
            problem_tooltip(
                ui,
                p,
                r.right_bottom(),
                ed.problems.iter().filter(|pr| ed.doc.line_of(pr.from) == n),
            );
        }

        // The maths under the cursor, previewed above it (writing/math.ts).
        if ed.tex
            && focused
            && main.is_empty()
            && style.math_preview
            && let Some(span) = crate::writing::math::math_at(&ed.doc, main.head)
            && !span.tex.trim().is_empty()
        {
            // Inline maths is anchored at its start; tall display maths at the cursor.
            let anchor_pos = if span.display { main.head } else { span.from };
            let n = ed.doc.line_of(anchor_pos);
            if let Some(l) = geo.layouts.get(&n) {
                let pt = col_pos(l, anchor_pos - ed.doc.line_start(n), cw, tab).0;
                let at = screen(text_x, tops[n]) + pt.to_vec2();
                math_preview(ui, p, ed, &span.tex, span.display, at, style.font.size, lh);
            }
        }

        // Where the completion list hangs: under the start of the word being completed.
        if let Some(c) = &ed.completion
            && let Some((it, _)) = c.items.first()
        {
            let n = ed.doc.line_of(it.from);
            if let Some(l) = geo.layouts.get(&n) {
                let pt = col_pos(l, it.from - ed.doc.line_start(n), cw, tab).0;
                popup_anchor = Some(screen(text_x, tops[n]) + pt.to_vec2());
            }
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

        // The pointer: fold markers and placeholders first.
        let mut fold_hit = false;
        if ui.input(|i| i.pointer.primary_pressed())
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
            && let Some(&(_, n)) = fold_clicks.iter().find(|(r, _)| r.contains(pos))
        {
            ed.toggle_fold(n);
            fold_hit = true;
            ui.ctx().request_repaint();
        }
        if fold_clicks
            .iter()
            .any(|(r, _)| ui.rect_contains_pointer(*r))
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let response = ui.interact(visible, id, Sense::click_and_drag());
        if std::mem::take(&mut ed.refocus) {
            ui.memory_mut(|m| m.request_focus(id));
        }
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::MultilineTextInput, true, label));
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
        // Hovering a problem's text shows its message too (after CodeMirror's 300 ms).
        if response.hovered()
            && !ed.problems.is_empty()
            && ui.input(|i| i.pointer.time_since_last_movement()) > 0.3
            && let Some(pos) = response.hover_pos()
        {
            let (at, _, _) = hit(&mut geo, ui, &ed.doc, pos);
            let here: Vec<&Problem> = ed
                .problems
                .iter()
                .filter(|pr| at >= pr.from && at <= pr.to && pr.to > pr.from)
                .collect();
            if !here.is_empty() {
                problem_tooltip(ui, p, pos + vec2(0.0, 12.0), here.into_iter());
            }
        } else if response.hovered() && !ed.problems.is_empty() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(320));
        }
        // Hover cards (editor/hover.ts): after resting 300 ms on a \ref, \cite, image or input.
        if ed.tex {
            let rested = ui.input(|i| i.pointer.time_since_last_movement()) > 0.3;
            let pointer = ui.input(|i| i.pointer.hover_pos());
            let on_card = pointer
                .zip(ed.hover.as_ref())
                .is_some_and(|(pt, h)| h.2.expand(4.0).contains(pt));
            let found = response.hover_pos().and_then(|pos| {
                let (at, n, _) = hit(&mut geo, ui, &ed.doc, pos);
                super::hover::target_at(&ed.doc.line(n), at - ed.doc.line_start(n)).map(|t| (n, t))
            });
            match found {
                Some((n, t))
                    if rested || ed.hover.as_ref().is_some_and(|h| h.0 == n && h.1 == t) =>
                {
                    let rect = ed
                        .hover
                        .as_ref()
                        .filter(|h| h.0 == n && h.1 == t)
                        .map_or(Rect::NOTHING, |h| h.2);
                    ed.hover = Some((n, t, rect));
                }
                Some(_) => ui
                    .ctx()
                    .request_repaint_after(std::time::Duration::from_millis(320)),
                None if on_card => {}
                None => {
                    ed.hover = None;
                    // Anything else texlab may know about (a package, a command): ask it.
                    if rested && let Some(pos) = response.hover_pos() {
                        let (at, n, _) = hit(&mut geo, ui, &ed.doc, pos);
                        let col = at - ed.doc.line_start(n);
                        let line_text = ed.doc.line(n);
                        let on_word = line_text
                            .chars()
                            .nth(col)
                            .is_some_and(|c| c.is_alphanumeric() || c == '\\');
                        match &look.hover {
                            Some((path, l, c, text))
                                if *path == ed.path && *l == n && *c == col =>
                            {
                                if !text.is_empty() {
                                    problem_tooltip_text(ui, p, pos + vec2(0.0, 14.0), text);
                                }
                            }
                            _ if on_word => out.server_hover = Some((n, col)),
                            _ => {}
                        }
                    }
                }
            }
            if response.is_pointer_button_down_on() {
                ed.hover = None;
            }
            if let Some((n, t, _)) = ed.hover.clone()
                && let Some(l) = geo.layouts.get(&n)
            {
                let pt = col_pos(l, t.from.min(ed.doc.line(n).chars().count()), cw, tab).0;
                let at = screen(text_x, tops[n]) + pt.to_vec2() + vec2(0.0, lh);
                let r = egui::Area::new(egui::Id::unique("hover-card"))
                    .order(egui::Order::Tooltip)
                    .fixed_pos(at)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::new()
                            .fill(p.surface)
                            .stroke(Stroke::new(1.0, p.border))
                            .corner_radius(8)
                            .shadow(p.shadow)
                            .inner_margin(egui::Margin::symmetric(12, 8))
                            .show(ui, |ui| {
                                let screen_r = ui.ctx().content_rect();
                                ui.set_max_width(460.0f32.min(screen_r.width() * 0.7) - 24.0);
                                egui::ScrollArea::vertical()
                                    .max_height(screen_r.height() * 0.5)
                                    .show(ui, |ui| {
                                        if let Some(s) =
                                            super::hover::card(ui, p, &t, look, &look.root)
                                        {
                                            out.hover_search = Some(s);
                                        }
                                    });
                            });
                    })
                    .response
                    .rect;
                if let Some(h) = &mut ed.hover {
                    h.2 = r;
                }
                if out.hover_search.is_some() {
                    ed.hover = None;
                }
            }
        }
        // A click on commented text shows its thread.
        if response.clicked()
            && !ed.comments.is_empty()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let (at, _, _) = hit(&mut geo, ui, &ed.doc, pos);
            out.comment_click = ed
                .comments
                .iter()
                .find(|c| at >= c.1 && at < c.2)
                .map(|c| c.0.clone());
        }
        // Right-click on a misspelled word: suggestions and "Add to dictionary".
        if response.secondary_clicked()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let (at, _, _) = hit(&mut geo, ui, &ed.doc, pos);
            ed.spell_menu =
                spell_words
                    .iter()
                    .find(|w| at >= w.0 && at <= w.1)
                    .map(|(from, to, word)| {
                        let cell = std::sync::Arc::new(std::sync::Mutex::new(None));
                        if let Some(shell) = checks.spell.shell.clone() {
                            let (c, w, ctx) = (cell.clone(), word.clone(), ui.ctx().clone());
                            std::thread::spawn(move || {
                                *c.lock().unwrap() = Some(shell.spell_suggest(w));
                                ctx.request_repaint();
                            });
                        } else {
                            *cell.lock().unwrap() = Some(Vec::new());
                        }
                        SpellMenu {
                            word: word.clone(),
                            from: *from,
                            to: *to,
                            at: pos,
                            suggestions: cell,
                        }
                    });
        }
        // Hovering a grammar finding: its message, the fixes, and "Ignore this rule".
        if ed.spell_menu.is_none()
            && let Some(pos) = response.hover_pos()
            && ui.input(|i| i.pointer.time_since_last_movement()) > 0.3
        {
            let (at, n, _) = hit(&mut geo, ui, &ed.doc, pos);
            if let Some(g) = ed
                .grammar_marks
                .1
                .iter()
                .find(|g| at >= g.from && at < g.to)
                .cloned()
                && let Some(l) = geo.layouts.get(&n)
            {
                let pt = col_pos(
                    l,
                    g.from.max(ed.doc.line_start(n)) - ed.doc.line_start(n),
                    cw,
                    tab,
                )
                .0;
                ed.grammar_tip = Some((g, screen(text_x, tops[n]) + pt.to_vec2()));
            }
        }
        if let Some((g, at)) = ed.grammar_tip.clone() {
            let mut keep = false;
            let r = egui::Area::new(egui::Id::unique("grammar-tip"))
                .order(egui::Order::Tooltip)
                .pivot(egui::Align2::LEFT_BOTTOM)
                .fixed_pos(at - vec2(0.0, 2.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::new()
                        .fill(p.surface)
                        .stroke(Stroke::new(1.0, p.border))
                        .corner_radius(8)
                        .shadow(p.shadow)
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.set_max_width(340.0);
                            ui.spacing_mut().item_spacing = vec2(4.0, 6.0);
                            ui.label(egui::RichText::new(&g.message).size(13.0).color(p.ink));
                            ui.horizontal_wrapped(|ui| {
                                let button = |ui: &mut Ui, label: &str| {
                                    let galley = ui.painter().layout_no_wrap(
                                        label.into(),
                                        FontId::proportional(12.0),
                                        p.ink,
                                    );
                                    let (r, resp) = ui.allocate_exact_size(
                                        galley.size() + vec2(16.0, 6.0),
                                        Sense::click(),
                                    );
                                    let fill = if resp.hovered() {
                                        p.surface_3
                                    } else {
                                        p.surface_2
                                    };
                                    ui.painter().rect(
                                        r,
                                        crate::theme::R_SM,
                                        fill,
                                        Stroke::new(1.0, p.border),
                                        egui::StrokeKind::Inside,
                                    );
                                    ui.painter().galley(r.min + vec2(8.0, 3.0), galley, p.ink);
                                    resp.widget_info(|| {
                                        egui::WidgetInfo::labeled(egui::Role::Button, true, label)
                                    });
                                    resp.clicked()
                                };
                                if g.editable {
                                    for rep_ in &g.replacements {
                                        if button(
                                            ui,
                                            if rep_.is_empty() { "(delete)" } else { rep_ },
                                        ) && ed.doc.slice(g.from, g.to) == g.flagged
                                        {
                                            ed.doc.apply(
                                                super::buffer::ChangeSet::new(vec![
                                                    super::buffer::Change {
                                                        from: g.from,
                                                        to: g.to,
                                                        insert: rep_.clone(),
                                                    },
                                                ]),
                                                ed.doc.selection.clone(),
                                                super::buffer::EditKind::Other,
                                            );
                                            keep = false;
                                            ed.grammar_tip = None;
                                            return;
                                        }
                                    }
                                }
                                if button(ui, "Ignore this rule") {
                                    out.ignore_rule = Some(g.rule.clone());
                                    ed.grammar_tip = None;
                                }
                            });
                        });
                })
                .response
                .rect;
            // The tip stays while the pointer is on the finding or on the tip itself.
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                let (at, _, _) = hit(&mut geo, ui, &ed.doc, pos);
                keep = r.expand(6.0).contains(pos)
                    || (at >= g.from && at < g.to && response.hovered());
            }
            if !keep {
                ed.grammar_tip = None;
            }
        }
        if response.is_pointer_button_down_on()
            && !fold_hit
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
            // An edit inside a \begin or \end name renames its partner in the same undo step.
            let sync = ed.tex
                && style.env_sync
                && matches!(
                    e,
                    Event::Text(_)
                        | Event::Paste(_)
                        | Event::Key {
                            key: Key::Backspace | Key::Delete,
                            pressed: true,
                            ..
                        }
                )
                && cmd_is_plain(e)
                && envs::near_token(&ed.doc, ed.doc.line_of(ed.doc.selection.main().head));
            let before = sync.then(|| (envs::tokens(&ed.doc), ed.doc.journal.len()));
            if handle_event(ui, ed, style, e, &mut geo, page, &mut out) {
                ed.last_activity = now;
            }
            if let Some((toks, len)) = before
                && ed.doc.journal.len() == len + 1
                && let Some(p) = envs::paired_edit(&toks, &ed.doc.journal[len].clone(), &ed.doc)
            {
                ed.doc.apply_merged(p);
            }
        }
    }

    // The completion list: computed again when the text, the cursor or the lookups change.
    let main = ed.doc.selection.main();
    let key = (ed.doc.version, main.head, look.generation);
    let wanted = ed.complete_wanted.take();
    let focused = ui.memory(|m| m.has_focus(id));
    if !focused || !main.is_empty() || ed.doc.selection.ranges.len() > 1 {
        ed.completion = None;
    } else if let Some(explicit) = wanted
        .or_else(|| {
            ed.completion
                .as_ref()
                .filter(|c| c.key != key)
                .map(|c| c.explicit)
        })
        .or_else(|| {
            // texlab answered for this cursor after the list closed (or never opened).
            ed.awaiting_server
                .filter(|(h, _)| {
                    *h == main.head
                        && look
                            .server
                            .as_ref()
                            .is_some_and(|s| s.0 == ed.path && s.1 == main.head)
                })
                .map(|(_, e)| e)
        })
    {
        ed.awaiting_server = None;
        let mut found: Vec<complete::Found> = [
            complete::latex(&ed.doc, main.head, look, explicit),
            complete::snippets(&ed.doc, main.head, look),
        ]
        .into_iter()
        .flatten()
        .collect();
        // texlab's extra commands arrive on their own and join the list (lspCommandCompletions).
        if complete::wants_server(&ed.doc, main.head, explicit) {
            match &look.server {
                Some((path, head, from, names)) if *path == ed.path && *head == main.head => {
                    found.push(complete::Found {
                        items: complete::server_items(names, *from, main.head),
                        filter: true,
                        from: *from,
                    });
                }
                _ => {
                    let n = ed.doc.line_of(main.head);
                    let before = ed.doc.slice(ed.doc.line_start(n), main.head);
                    let name = before
                        .chars()
                        .rev()
                        .take_while(|c| c.is_ascii_alphabetic())
                        .count();
                    out.server_complete = Some((main.head, main.head - name));
                    ed.awaiting_server = Some((main.head, explicit));
                }
            }
        }
        let items = complete::rank(&ed.doc, found);
        ed.completion = (!items.is_empty()).then_some(Completion {
            items,
            selected: 0,
            explicit,
            key,
            top: 0,
        });
    }
    if let (Some(anchor), Some(c)) = (popup_anchor, &mut ed.completion) {
        // A small rendered glyph beside each maths item (.cm-math-glyph, KaTeX's 1.21 em).
        let mut glyph =
            |ui: &Ui, tex: &str, at: Pos2, size: f32| match crate::writing::render::render(
                ui.ctx(),
                tex,
                false,
                size * 1.21,
                p.ink,
            ) {
                Ok(m) => {
                    let r = Rect::from_min_size(pos2(at.x, at.y - m.size.y / 2.0), m.size);
                    ui.painter().image(
                        m.texture.id(),
                        r,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                    m.size.x
                }
                Err(_) => 0.0,
            };
        let mut info = |ui: &mut Ui, it: &complete::Item| {
            if let Some(t) = &it.info {
                ui.label(egui::RichText::new(t).color(p.ink));
            } else if let Some(t) = &it.preview {
                // .cm-math-info: the symbol large, the code it inserts under it.
                ui.spacing_mut().item_spacing.y = 6.0;
                if let Ok(m) = crate::writing::render::render(ui.ctx(), t, true, 13.0 * 1.21, p.ink)
                {
                    ui.image((m.texture.id(), m.size));
                }
                let code = match &it.apply {
                    Apply::Snippet(a) => a.replace("${}", "▢"),
                    _ => t.clone(),
                };
                ui.label(
                    egui::RichText::new(code)
                        .font(FontId::monospace(11.0))
                        .color(p.muted),
                );
            }
        };
        if let popup::Picked::Item(i) = popup::show(
            ui.ctx(),
            p,
            &style.font,
            anchor,
            style.line_height,
            c,
            &mut glyph,
            &mut info,
        ) {
            accept(ed, i, style.indent, &mut out);
            ui.memory_mut(|m| m.request_focus(id));
        }
    }

    if let Some(menu) = &ed.spell_menu {
        let suggestions = menu.suggestions.lock().unwrap().clone();
        let (word, from, to) = (menu.word.clone(), menu.from, menu.to);
        let mut close = ui.input(|i| i.key_pressed(Key::Escape));
        let r = egui::Area::new(egui::Id::unique("spell-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu.at)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(crate::theme::R_LG)
                    .shadow(p.shadow)
                    .inner_margin(egui::Margin::same(5))
                    .show(ui, |ui| {
                        ui.set_min_width(200.0);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let muted = |ui: &mut Ui, t: &str| {
                            egui::Frame::NONE
                                .inner_margin(egui::Margin::symmetric(10, 6))
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new(t).size(12.0).color(p.muted));
                                });
                        };
                        let item = |ui: &mut Ui, t: &str, color: Color32| {
                            let w = ui.available_width().max(200.0);
                            let (r, resp) = ui.allocate_exact_size(vec2(w, 30.0), Sense::click());
                            if resp.hovered() {
                                ui.painter().rect_filled(r, crate::theme::R_SM, p.surface_3);
                            }
                            ui.painter().text(
                                pos2(r.left() + 10.0, r.center().y),
                                egui::Align2::LEFT_CENTER,
                                t,
                                FontId::proportional(13.0),
                                color,
                            );
                            resp.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::Role::MenuItem, true, t)
                            });
                            resp.clicked()
                        };
                        match &suggestions {
                            None => muted(ui, "Finding suggestions…"),
                            Some(list) if list.is_empty() => muted(ui, "No suggestions"),
                            Some(list) => {
                                for sgn in list {
                                    if item(ui, sgn, p.ink) {
                                        // The word may have moved if the user typed meanwhile.
                                        if ed.doc.slice(from, to) == word {
                                            ed.doc.apply(
                                                super::buffer::ChangeSet::new(vec![
                                                    super::buffer::Change {
                                                        from,
                                                        to,
                                                        insert: sgn.clone(),
                                                    },
                                                ]),
                                                Selection::cursor(from + sgn.chars().count()),
                                                super::buffer::EditKind::Other,
                                            );
                                        }
                                        close = true;
                                    }
                                }
                            }
                        }
                        ui.add_space(4.0);
                        let line_y = ui.cursor().top();
                        ui.painter().hline(
                            ui.min_rect().x_range(),
                            line_y,
                            Stroke::new(1.0, p.border),
                        );
                        ui.add_space(4.0);
                        if item(ui, &format!("Add “{word}” to dictionary"), p.muted) {
                            checks.spell.add(&word);
                            out.add_word = Some(word.clone());
                            close = true;
                        }
                    });
            })
            .response
            .rect;
        if ui.input(|i| i.pointer.any_pressed())
            && ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|pt| !r.contains(pt))
        {
            close = true;
        }
        if close {
            ed.spell_menu = None;
        }
    }

    // Leaving the snippet's fields ends it.
    if ed
        .snippet
        .as_ref()
        .is_some_and(|s| !s.contains(&ed.doc.selection))
    {
        ed.end_snippet();
    }
    // A cursor put inside a fold (a search hit, go to line, typing) opens it.
    let head = ed.doc.selection.main().head;
    if ed.folds.0.iter().any(|f| head > f.0 && head <= f.1) {
        ed.folds.reveal(head);
        ed.folds_gen += 1;
        ui.ctx().request_repaint();
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
            if ed.tex {
                cmd::indent_on_input(&mut ed.doc, style.indent);
            }
            if ed.tex && style.autocomplete {
                ed.complete_wanted = Some(ed.complete_wanted.unwrap_or(false));
            }
            true
        }
        Event::PasteImage(img) if ed.tex => {
            out.pasted_image = Some(img.clone());
            false
        }
        Event::Paste(t) => {
            // Cells copied from a spreadsheet become a table, which a toast can turn back.
            match ed
                .tex
                .then(|| crate::writing::table::tsv_to_tabular(t))
                .flatten()
            {
                Some(table) => {
                    let unit = indent.unit();
                    let table: String = table
                        .split('\n')
                        .map(|l| {
                            let n = l.chars().take_while(|c| *c == ' ').count() / 2;
                            format!("{}{}", unit.repeat(n), &l[n * 2..])
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let from = ed.doc.selection.main().from();
                    cmd::insert_text(&mut ed.doc, &table);
                    out.needs_package = Some("booktabs".into());
                    out.pasted_table = Some((from, table, t.clone()));
                }
                None => cmd::paste(&mut ed.doc, t),
            }
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
    let shift = m.shift;
    let modk = cmd_key(&m);
    let is = |spec: &str| shortcuts::matches_spec(spec, m, key);
    // completionKeymap: the open list takes the arrows, Enter and Escape.
    if let Some(c) = &mut ed.completion
        && !modk
        && !m.alt
    {
        match key {
            Key::ArrowDown => {
                return {
                    c.step(1);
                    true
                };
            }
            Key::ArrowUp => {
                return {
                    c.step(-1);
                    true
                };
            }
            Key::PageDown => {
                return {
                    c.step(6);
                    true
                };
            }
            Key::PageUp => {
                return {
                    c.step(-6);
                    true
                };
            }
            Key::Enter => {
                let i = c.selected;
                accept(ed, i, style.indent, out);
                return true;
            }
            Key::Escape => {
                ed.completion = None;
                return true;
            }
            _ => {}
        }
    }
    if ed.tex && m.ctrl && !m.alt && !shift && key == Key::Space {
        ed.complete_wanted = Some(true);
        return true;
    }
    // Enter after an unclosed \begin{X} adds its \end (environmentSync's keymap, Prec.high).
    if ed.tex
        && style.env_sync
        && key == Key::Enter
        && !modk
        && !m.alt
        && !shift
        && envs::close_environment(&mut ed.doc, style.indent)
    {
        return reveal(ed);
    }
    let d = &mut ed.doc;
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
            return ed.next_problem(true);
        }
        if is(shortcuts::keys_of("prev-problem")) {
            out.next_problem = Some(-1);
            return ed.next_problem(false);
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
    // texlab: go to definition, find references.
    if ed.tex && (is(shortcuts::keys_of("definition")) || is(shortcuts::keys_of("references"))) {
        let h = ed.doc.selection.main().head;
        let n = ed.doc.line_of(h);
        out.server_goto = Some((
            n,
            h - ed.doc.line_start(n),
            is(shortcuts::keys_of("references")),
        ));
        return true;
    }
    // foldKeymap: fold or unfold at the cursor, and everything.
    if ed.tex && style.folding {
        if is(shortcuts::keys_of("fold")) {
            fold_at_cursor(ed);
            return reveal(ed);
        }
        if is(shortcuts::keys_of("unfold")) {
            let n = ed.doc.line_of(ed.doc.selection.main().head);
            if ed.folds.unfold_line(&ed.doc, n) {
                ed.folds_gen += 1;
            }
            return true;
        }
        if is("Ctrl-Alt-[") {
            for n in 0..ed.doc.line_count() {
                if fold::fold_lines(&ed.doc, n).is_some() && ed.folds.starting(&ed.doc, n).is_none()
                {
                    ed.folds.fold(&ed.doc, n);
                }
            }
            // Nested folds inside an outer one stay folded; the cursor's own block opens.
            let h = ed.doc.selection.main().head;
            ed.folds.reveal(h);
            ed.folds_gen += 1;
            return true;
        }
        if is("Ctrl-Alt-]") {
            ed.folds.0.clear();
            ed.folds_gen += 1;
            return true;
        }
    }
    let d = &mut ed.doc;
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
            skip_folds(ed, right);
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
        Key::Escape if ed.hover.is_some() => {
            ed.hover = None;
            true
        }
        Key::Escape if ed.snippet.is_some() => {
            ed.end_snippet();
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
        Key::Tab if !modk && !m.alt && ed.snippet.is_some() => {
            // nextSnippetField / prevSnippetField; reaching the last field ends the snippet.
            let s = ed.snippet.as_mut().unwrap();
            let next = if shift {
                s.current.checked_sub(1)
            } else {
                Some(s.current + 1)
            };
            if let Some(k) = next.filter(|k| *k < s.fields.len()) {
                s.current = k;
                let last = k + 1 == s.fields.len();
                ed.doc.selection = s.select(k);
                ed.doc.break_group();
                if last {
                    ed.snippet = None;
                }
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

/// Ctrl-Shift-[: folds the innermost block that contains the cursor's line.
fn fold_at_cursor(ed: &mut Editor) {
    let n = ed.doc.line_of(ed.doc.selection.main().head);
    for k in (0..=n).rev() {
        if ed.folds.starting(&ed.doc, k).is_some() {
            continue;
        }
        if let Some((_, last)) = fold::fold_lines(&ed.doc, k)
            && (k == n || last >= n)
        {
            ed.toggle_fold(k);
            return;
        }
    }
}

/// Arrow keys step over a folded block instead of into it, like CodeMirror's atomic folds.
fn skip_folds(ed: &mut Editor, forward: bool) {
    let folds = ed.folds.0.clone();
    if folds.is_empty() {
        return;
    }
    let ranges: Vec<Range> = ed
        .doc
        .selection
        .ranges
        .iter()
        .map(
            |r| match folds.iter().find(|f| r.head > f.0 && r.head <= f.1) {
                Some(f) => {
                    // Forward lands at the start of the line after the fold.
                    let head = if forward {
                        (f.1 + 1).min(ed.doc.len())
                    } else {
                        f.0
                    };
                    if r.is_empty() {
                        Range::cursor(head)
                    } else {
                        Range::new(r.anchor, head)
                    }
                }
                None => *r,
            },
        )
        .collect();
    ed.doc.selection = Selection::normalized(ranges, ed.doc.selection.main);
}

/// `.cm-math-preview`: the formula above `at`; on an error, the last good one faded with the
/// message under it.
#[allow(clippy::too_many_arguments)]
fn math_preview(
    ui: &Ui,
    p: &Palette,
    ed: &mut Editor,
    tex: &str,
    display: bool,
    at: Pos2,
    font_size: f32,
    line_h: f32,
) {
    use crate::writing::render;
    let size = font_size * 1.1 * 1.21;
    let result = render::render(ui.ctx(), tex, display, size, p.ink);
    if let Ok(m) = &result {
        ed.last_math = Some(m.clone());
    }
    // Above the line, or below it when there is no room (strictSide: false).
    let h = result.as_ref().map_or(60.0, |m| m.size.y) + 22.0;
    let above = at.y - 4.0 - h >= ui.ctx().content_rect().top();
    let (pivot, pos) = if above {
        (egui::Align2::LEFT_BOTTOM, at - vec2(14.0, 4.0))
    } else {
        (egui::Align2::LEFT_TOP, at + vec2(-14.0, line_h + 4.0))
    };
    egui::Area::new(egui::Id::unique("math-preview"))
        .order(egui::Order::Tooltip)
        .pivot(pivot)
        .fixed_pos(pos)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(crate::theme::R_LG)
                .shadow(p.shadow)
                .inner_margin(egui::Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.set_max_width(ui.ctx().content_rect().width() * 0.7);
                    match &result {
                        Ok(m) => {
                            ui.image((m.texture.id(), m.size));
                        }
                        Err(e) => {
                            if let Some(old) = &ed.last_math {
                                ui.add(
                                    egui::Image::new((old.texture.id(), old.size))
                                        .tint(Color32::from_white_alpha(115)),
                                );
                            }
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(e)
                                    .font(FontId::monospace(12.0))
                                    .color(p.error),
                            );
                        }
                    }
                });
        });
}

fn severity_color(s: Severity, p: &Palette) -> Color32 {
    match s {
        Severity::Error => p.error,
        Severity::Warning => p.warn,
        Severity::Info => Color32::from_gray(0x99),
    }
}

/// A wavy line from x0 to x1 around y, like `text-decoration-style: wavy`.
fn wavy(painter: &egui::Painter, x0: f32, x1: f32, y: f32, color: Color32) {
    let mut pts = Vec::new();
    let mut x = x0;
    let mut up = true;
    while x <= x1 {
        pts.push(pos2(x, if up { y - 1.2 } else { y + 1.2 }));
        x += 2.0;
        up = !up;
    }
    painter.add(egui::Shape::line(pts, Stroke::new(1.0, color)));
}

/// CodeMirror's lint markers: a red circle, a yellow triangle, a blue square.
fn lint_marker(painter: &egui::Painter, r: Rect, s: Severity) {
    let k = r.width() / 40.0;
    let at = |x: f32, y: f32| r.min + vec2(x * k, y * k);
    let stroke = |c: u32| {
        Stroke::new(
            6.0 * k,
            Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8),
        )
    };
    let fill = |c: u32| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8);
    match s {
        Severity::Error => {
            painter.circle(r.center(), 15.0 * k, fill(0xff8877), stroke(0xff4433));
        }
        Severity::Warning => {
            painter.add(egui::Shape::convex_polygon(
                vec![at(20.0, 6.0), at(37.0, 35.0), at(3.0, 35.0)],
                fill(0xffee88),
                stroke(0xffdd77),
            ));
        }
        Severity::Info => {
            painter.add(egui::Shape::convex_polygon(
                vec![at(5.0, 5.0), at(35.0, 5.0), at(35.0, 35.0), at(5.0, 35.0)],
                fill(0xaaaaff),
                stroke(0x7777ee),
            ));
        }
    }
}

/// texlab's hover note (`.cm-lsp-hover-tooltip`) as plain text.
fn problem_tooltip_text(ui: &Ui, p: &Palette, at: Pos2, text: &str) {
    egui::Area::new(egui::Id::unique("lsp-hover"))
        .order(egui::Order::Tooltip)
        .fixed_pos(at)
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(8)
                .shadow(p.shadow)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.set_max_width(460.0);
                    ui.label(
                        egui::RichText::new(text.replace("```latex", "").replace("```", ""))
                            .color(p.ink),
                    );
                });
        });
}

/// `.cm-tooltip-lint`: each problem with a coloured bar on its left.
fn problem_tooltip<'a>(ui: &Ui, p: &Palette, at: Pos2, list: impl Iterator<Item = &'a Problem>) {
    let list: Vec<&Problem> = list.collect();
    egui::Area::new(egui::Id::unique("lint-tooltip"))
        .order(egui::Order::Tooltip)
        .fixed_pos(at)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(8)
                .shadow(p.shadow)
                .show(ui, |ui| {
                    ui.set_max_width(480.0);
                    for pr in list {
                        let resp = egui::Frame::NONE
                            .inner_margin(egui::Margin {
                                left: 13,
                                right: 6,
                                top: 3,
                                bottom: 3,
                            })
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(&pr.message).color(p.ink))
                            })
                            .response;
                        let bar = Rect::from_min_size(resp.rect.min, vec2(5.0, resp.rect.height()));
                        let c = match pr.severity {
                            Severity::Error => Color32::from_rgb(0xdd, 0x11, 0x11),
                            Severity::Warning => Color32::from_rgb(0xff, 0xa5, 0x00),
                            Severity::Info => Color32::from_gray(0x99),
                        };
                        ui.painter().rect_filled(bar, 0, c);
                    }
                });
        });
}

/// Inserts completion `i` and closes the list (`\begin` opens the list of environments next).
fn accept(ed: &mut Editor, i: usize, indent: Indent, out: &mut Output) {
    let Some(c) = ed.completion.take() else {
        return;
    };
    let Some((it, _)) = c.items.into_iter().nth(i) else {
        return;
    };
    let head = ed.doc.selection.main().head;
    let to = it.to.max(head);
    match &it.apply {
        Apply::Snippet(t) => ed.apply_snippet(it.from, to, t, indent),
        Apply::Text(t) => {
            let at = it.from + t.chars().count();
            ed.doc.apply(
                super::buffer::ChangeSet::new(vec![super::buffer::Change {
                    from: it.from,
                    to,
                    insert: t.clone(),
                }]),
                Selection::cursor(at),
                super::buffer::EditKind::Other,
            );
        }
        Apply::Begin => {
            let t = "\\begin{";
            ed.doc.apply(
                super::buffer::ChangeSet::new(vec![super::buffer::Change {
                    from: it.from,
                    to,
                    insert: t.into(),
                }]),
                Selection::cursor(it.from + t.chars().count()),
                super::buffer::EditKind::Other,
            );
            ed.complete_wanted = Some(true);
        }
    }
    ed.doc.break_group();
    ed.reveal = true;
    if it.pkg.is_some() {
        out.needs_package = it.pkg;
    }
}

/// A text edit without Ctrl/Alt (those are commands, not typing).
fn cmd_is_plain(e: &Event) -> bool {
    match e {
        Event::Key { modifiers, .. } => !modifiers.ctrl && !modifiers.alt && !modifiers.mac_cmd,
        _ => true,
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
            folding: true,
            env_sync: true,
            autocomplete: true,
            math_preview: true,
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
                    show(
                        &mut child,
                        &p,
                        ed,
                        &style(),
                        egui::Id::unique("ed"),
                        "Editor",
                        &Lookups::default(),
                        &mut crate::writing::Checks::default(),
                    );
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

    #[test]
    fn folded_blocks_hide_their_lines_and_arrows_step_over_them() {
        let text = "\\section{One}\nfirst\nsecond\n\\section{Two}\n\\begin{itemize}\n  \\item a\n  \\item b\n\\end{itemize}\n% a\n% b";
        let mut ed = Editor::new(text, true);
        ed.toggle_fold(0);
        assert_eq!(ed.folds.lines(&ed.doc), [1]);
        let p = theme::LIGHT;
        let mut h = egui_kittest::Harness::builder()
            .with_size([520.0, 300.0])
            .with_context_setup(move |ctx| {
                theme::install_fonts(ctx);
                theme::apply(ctx, &p);
            })
            .wgpu()
            .build_ui_state(
                move |ui, ed: &mut Editor| {
                    let rect = ui.ctx().content_rect();
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                    show(
                        &mut child,
                        &p,
                        ed,
                        &style(),
                        egui::Id::unique("ed"),
                        "Editor",
                        &Lookups::default(),
                        &mut crate::writing::Checks::default(),
                    );
                },
                ed,
            );
        h.run();
        h.get_by_label("Editor").click();
        h.run();
        // From the end of line 1, Right goes past the folded lines to the next section.
        h.state_mut().doc.selection = Selection::cursor(13);
        h.key_press(egui::Key::ArrowRight);
        h.run();
        let head = h.state().doc.selection.main().head;
        assert_eq!(
            h.state().doc.line_of(head),
            3,
            "the cursor skips the hidden lines"
        );
        h.key_press(egui::Key::ArrowUp);
        h.run();
        assert_eq!(
            h.state().doc.line_of(h.state().doc.selection.main().head),
            0,
            "Up steps over them too"
        );
        assert_eq!(h.state().folds.lines(&h.state().doc), [1], "still folded");
        h.snapshot("editor-folded");
    }

    fn harness(ed: Editor, size: [f32; 2]) -> egui_kittest::Harness<'static, Editor> {
        harness_with(ed, size, Lookups::default())
    }

    fn harness_with(
        ed: Editor,
        size: [f32; 2],
        look: Lookups,
    ) -> egui_kittest::Harness<'static, Editor> {
        let p = theme::LIGHT;
        egui_kittest::Harness::builder()
            .with_size(size)
            .with_context_setup(move |ctx| {
                theme::install_fonts(ctx);
                theme::apply(ctx, &p);
            })
            .wgpu()
            .build_ui_state(
                move |ui, ed: &mut Editor| {
                    let rect = ui.ctx().content_rect();
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                    show(
                        &mut child,
                        &p,
                        ed,
                        &style(),
                        egui::Id::unique("ed"),
                        "Editor",
                        &look,
                        &mut crate::writing::Checks::default(),
                    );
                },
                ed,
            )
    }

    #[test]
    fn typing_a_command_offers_completions_and_enter_inserts_a_snippet() {
        let mut h = harness(
            Editor::new("\\begin{document}\n\n\\end{document}", true),
            [560.0, 320.0],
        );
        h.run();
        h.get_by_label("Editor").click();
        h.run();
        h.state_mut().doc.selection = Selection::cursor(17);
        h.get_by_label("Editor").type_text("\\subsec");
        h.run();
        let first = h
            .state()
            .completion
            .as_ref()
            .map(|c| c.items[0].0.label.clone());
        assert_eq!(first.as_deref(), Some("\\subsection"));
        h.snapshot("completion");
        h.key_press(egui::Key::Enter);
        h.run();
        let ed = h.state();
        assert_eq!(ed.doc.line(1), "\\subsection{}");
        assert_eq!(
            ed.doc.selection.main().head,
            17 + 12,
            "the cursor is in the braces"
        );
        assert!(ed.completion.is_none());
        // \begin{ite + Enter: the whole environment, its \end and the cursor on the item.
        h.state_mut().doc.selection = Selection::cursor(h.state().doc.line_end(1));
        h.key_press(egui::Key::Escape);
        h.run();
        h.key_press(egui::Key::End);
        h.key_press(egui::Key::Enter);
        h.run();
        h.get_by_label("Editor").type_text("\\begin{ite");
        h.run();
        assert_eq!(
            h.state()
                .completion
                .as_ref()
                .map(|c| c.items[0].0.label.clone())
                .as_deref(),
            Some("itemize")
        );
        h.key_press(egui::Key::Enter);
        h.run();
        let text = h.state().doc.text();
        assert!(
            text.contains("\\begin{itemize}\n    \\item \n\\end{itemize}"),
            "{text}"
        );
        // Renaming the \begin renames the \end.
        h.get_by_label("Editor").type_text("x");
        h.run();
        assert!(h.state().doc.text().contains("\\item x"));
    }

    #[test]
    fn maths_is_previewed_and_completed_with_glyphs() {
        crate::writing::render::set_macros(Default::default());
        let text = "Energy $E = \\frac{m}{2} v^2 + \\sqrt{x}$ here.\n\nNext $x = \\";
        let mut h = harness(Editor::new(text, true), [560.0, 320.0]);
        h.run();
        h.get_by_label("Editor").click();
        h.run();
        h.state_mut().doc.selection = Selection::cursor(14);
        h.run();
        h.run();
        h.snapshot("math-preview");
        // \al in maths: \alpha first, with its glyph.
        let end = h.state().doc.len();
        h.state_mut().doc.selection = Selection::cursor(end);
        h.get_by_label("Editor").type_text("al");
        h.run();
        let top: Vec<String> = h
            .state()
            .completion
            .as_ref()
            .unwrap()
            .items
            .iter()
            .take(3)
            .map(|i| i.0.label.clone())
            .collect();
        assert!(top.contains(&"\\alpha".to_owned()), "{top:?}");
        h.run();
        h.snapshot("completion-maths");
    }

    #[test]
    fn resting_on_an_input_shows_its_first_lines() {
        let fixture =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/article");
        let look = Lookups {
            files: vec!["chapters/method.tex".into(), "main.tex".into()],
            root: fixture,
            ..Default::default()
        };
        let mut h = harness_with(
            Editor::new("Intro.\n\\input{chapters/method}\n", true),
            [560.0, 320.0],
            look,
        );
        h.run();
        // Over "method" on line 2.
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(pos2(200.0, 12.0 + 21.45 * 1.5)));
        h.run();
        std::thread::sleep(std::time::Duration::from_millis(350));
        h.run();
        h.run();
        assert!(h.state().hover.is_some(), "the card shows");
        h.snapshot("hover-input");
    }

    #[test]
    fn grammar_findings_are_underlined_where_they_are_in_the_source() {
        let p = theme::LIGHT;
        let text = "Intro.\n\nWe has \\emph{robust} methods.";
        let mut checks = crate::writing::Checks::default();
        checks.grammar.configure("local", "", "en");
        checks.grammar.store(
            "We has robust methods.".into(),
            vec![lemmata_grammar::Match {
                offset: 3,
                length: 3,
                message: "Use “have” with “we”.".into(),
                short_message: String::new(),
                replacements: vec!["have".into()],
                rule_id: "HE_VERB_AGR".into(),
                category: "GRAMMAR".into(),
            }],
        );
        let mut h = egui_kittest::Harness::builder()
            .with_size([520.0, 200.0])
            .with_context_setup(move |ctx| {
                theme::install_fonts(ctx);
                theme::apply(ctx, &p);
            })
            .wgpu()
            .build_ui_state(
                move |ui, ed: &mut Editor| {
                    let rect = ui.ctx().content_rect();
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                    show(
                        &mut child,
                        &p,
                        ed,
                        &style(),
                        egui::Id::unique("ed"),
                        "Editor",
                        &Lookups::default(),
                        &mut checks,
                    );
                },
                Editor::new(text, true),
            );
        h.run();
        h.run();
        let marks: Vec<(usize, usize, bool)> = h
            .state()
            .grammar_marks
            .1
            .iter()
            .map(|g| (g.from, g.to, g.editable))
            .collect();
        assert_eq!(marks, [(11, 14, true)], "\"has\" on line 3");
        h.snapshot("grammar");
    }
}
