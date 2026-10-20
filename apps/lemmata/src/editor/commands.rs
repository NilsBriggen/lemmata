//! Editing commands that need no layout: CodeMirror's defaultKeymap, closeBrackets, toggleComment
//! and the environment-aware Enter of editor/environments.ts. Moves that depend on how lines are
//! drawn (up/down, page, wrapped line ends) live in the view.

use super::buffer::{Change, ChangeSet, Doc, EditKind, Range, Selection};
use unicode_segmentation::UnicodeSegmentation;

/// One indentation level: spaces or a tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Indent {
    Spaces(usize),
    Tab,
}

impl Indent {
    pub fn unit(self) -> String {
        match self {
            Indent::Spaces(n) => " ".repeat(n),
            Indent::Tab => "\t".into(),
        }
    }
    /// How many columns one level is (a tab is drawn 4 wide).
    pub fn width(self) -> usize {
        match self {
            Indent::Spaces(n) => n,
            Indent::Tab => 4,
        }
    }
    pub fn tab_size(self) -> usize {
        self.width()
    }
    /// From the settings value: 2, 4, 8 or "tab".
    pub fn from_setting(v: &serde_json::Value) -> Self {
        match v {
            serde_json::Value::String(s) if s == "tab" => Indent::Tab,
            v => Indent::Spaces(v.as_u64().filter(|n| [2, 4, 8].contains(n)).unwrap_or(4) as usize),
        }
    }
}

/// Columns of leading whitespace, tabs to the next stop.
pub fn indent_columns(text: &str, tab: usize) -> usize {
    let mut col = 0;
    for c in text.chars() {
        match c {
            ' ' => col += 1,
            '\t' => col += tab - col % tab,
            _ => break,
        }
    }
    col
}

/// Whitespace that reaches `cols` columns in this indentation style.
fn indent_string(cols: usize, indent: Indent) -> String {
    match indent {
        Indent::Tab => "\t".repeat(cols / 4) + &" ".repeat(cols % 4),
        Indent::Spaces(_) => " ".repeat(cols),
    }
}

fn leading_ws(text: &str) -> &str {
    &text[..text.len() - text.trim_start_matches([' ', '\t']).len()]
}

// ── Characters and groups ──────────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy)]
enum Cat {
    Word,
    Space,
    Other,
}

fn cat(c: char) -> Cat {
    if c.is_alphanumeric() || c == '_' {
        Cat::Word
    } else if c.is_whitespace() {
        Cat::Space
    } else {
        Cat::Other
    }
}

/// The next grapheme boundary after (or before) `pos`, staying within its line; at a line's end or
/// start it crosses the line break.
fn by_char(d: &Doc, pos: usize, forward: bool) -> usize {
    let line = d.line_of(pos);
    let start = d.line_start(line);
    let text = d.line(line);
    let col = pos - start;
    let chars: Vec<(usize, &str)> = text.grapheme_indices(true).collect();
    // Char offsets of grapheme starts.
    let mut offsets = Vec::with_capacity(chars.len() + 1);
    let mut acc = 0;
    for (_, g) in &chars {
        offsets.push(acc);
        acc += g.chars().count();
    }
    offsets.push(acc);
    if forward {
        match offsets.iter().find(|&&o| o > col) {
            Some(&o) => start + o,
            None if line + 1 < d.line_count() => d.line_start(line + 1),
            None => pos,
        }
    } else {
        match offsets.iter().rev().find(|&&o| o < col) {
            Some(&o) => start + o,
            None if line > 0 => d.line_end(line - 1),
            None => pos,
        }
    }
}

/// CodeMirror's byGroup: across one run of word or punctuation characters, with spaces before it.
fn by_group(d: &Doc, pos: usize, forward: bool) -> usize {
    let line = d.line_of(pos);
    let start = d.line_start(line);
    let text: Vec<char> = d.line(line).chars().collect();
    let mut col = pos - start;
    if forward {
        if col >= text.len() {
            return by_char(d, pos, true);
        }
        let mut c = cat(text[col]);
        while col < text.len() {
            let n = cat(text[col]);
            if c == Cat::Space {
                c = n;
            }
            if n != c {
                break;
            }
            col += 1;
        }
    } else {
        if col == 0 {
            return by_char(d, pos, false);
        }
        let mut c = cat(text[col - 1]);
        while col > 0 {
            let n = cat(text[col - 1]);
            if c == Cat::Space {
                c = n;
            }
            if n != c {
                break;
            }
            col -= 1;
        }
    }
    start + col
}

/// Moves every range: `extend` keeps the anchor (Shift).
pub fn move_ranges(d: &mut Doc, extend: bool, mut to: impl FnMut(&Doc, Range) -> usize) {
    let ranges = d.selection.ranges.clone();
    let moved = ranges
        .into_iter()
        .map(|r| {
            let head = to(d, r);
            if extend {
                Range::new(r.anchor, head)
            } else {
                Range::cursor(head)
            }
        })
        .collect();
    d.selection = Selection::normalized(moved, d.selection.main);
    d.break_group();
}

pub fn char_left(d: &mut Doc, extend: bool) {
    // Without Shift a selection collapses to its start, as browsers do.
    move_ranges(d, extend, |d, r| {
        if !extend && !r.is_empty() {
            r.from()
        } else {
            by_char(d, r.head, false)
        }
    });
}

pub fn char_right(d: &mut Doc, extend: bool) {
    move_ranges(d, extend, |d, r| {
        if !extend && !r.is_empty() {
            r.to()
        } else {
            by_char(d, r.head, true)
        }
    });
}

pub fn group_left(d: &mut Doc, extend: bool) {
    move_ranges(d, extend, |d, r| by_group(d, r.head, false));
}

pub fn group_right(d: &mut Doc, extend: bool) {
    move_ranges(d, extend, |d, r| by_group(d, r.head, true));
}

/// Home: to the first non-blank character, or the line start when already there.
pub fn line_boundary_backward(d: &mut Doc, extend: bool) {
    move_ranges(d, extend, |d, r| {
        let line = d.line_of(r.head);
        let start = d.line_start(line);
        let text = d.line(line);
        let first = start + leading_ws(&text).chars().count();
        if r.head == first { start } else { first }
    });
}

pub fn line_boundary_forward(d: &mut Doc, extend: bool) {
    move_ranges(d, extend, |d, r| d.line_end(d.line_of(r.head)));
}

pub fn doc_start(d: &mut Doc, extend: bool) {
    move_ranges(d, extend, |_, _| 0);
}

pub fn doc_end(d: &mut Doc, extend: bool) {
    move_ranges(d, extend, |d, _| d.len());
}

pub fn select_all(d: &mut Doc) {
    d.selection = Selection::single(Range::new(0, d.len()));
}

/// Escape: one cursor, the main one (a selection stays).
pub fn simplify_selection(d: &mut Doc) -> bool {
    if d.selection.ranges.len() > 1 {
        d.selection = Selection::single(d.selection.main());
        return true;
    }
    false
}

/// Alt-l: the whole lines of each range, including the line break.
pub fn select_line(d: &mut Doc) {
    let ranges = d
        .selection
        .ranges
        .iter()
        .map(|r| {
            let first = d.line_start(d.line_of(r.from()));
            let last = d.line_of(r.to());
            let end = if last + 1 < d.line_count() {
                d.line_start(last + 1)
            } else {
                d.len()
            };
            Range::new(first, end)
        })
        .collect();
    d.selection = Selection::normalized(ranges, d.selection.main);
}

/// Mod-d: adds the next occurrence of the main selection (or selects the word at the cursor).
pub fn select_next_occurrence(d: &mut Doc) -> bool {
    let main = d.selection.main();
    if main.is_empty() {
        let (from, to) = word_at(d, main.head);
        if from == to {
            return false;
        }
        let mut ranges = d.selection.ranges.clone();
        ranges[d.selection.main] = Range::new(from, to);
        d.selection = Selection::normalized(ranges, d.selection.main);
        return true;
    }
    let needle: Vec<char> = d.slice(main.from(), main.to()).chars().collect();
    let hay: Vec<char> = d.text().chars().collect();
    let last = d.selection.ranges.iter().map(|r| r.to()).max().unwrap_or(0);
    let find = |from: usize| {
        (from..=hay.len().saturating_sub(needle.len()))
            .find(|&i| hay[i..i + needle.len()] == needle[..])
    };
    let hit = find(last)
        .or_else(|| find(0))
        .filter(|&i| !d.selection.ranges.iter().any(|r| r.from() == i));
    match hit {
        Some(i) => {
            let mut ranges = d.selection.ranges.clone();
            ranges.push(Range::new(i, i + needle.len()));
            let n = ranges.len() - 1;
            d.selection = Selection::normalized(ranges, n);
            true
        }
        None => false,
    }
}

/// The word (letters, digits, `_`) touching `pos`.
pub fn word_at(d: &Doc, pos: usize) -> (usize, usize) {
    let line = d.line_of(pos);
    let start = d.line_start(line);
    let text: Vec<char> = d.line(line).chars().collect();
    let col = pos - start;
    let is_word = |c: char| cat(c) == Cat::Word;
    let mut a = col;
    while a > 0 && is_word(text[a - 1]) {
        a -= 1;
    }
    let mut b = col;
    while b < text.len() && is_word(text[b]) {
        b += 1;
    }
    (start + a, start + b)
}

// ── Inserting and deleting ─────────────────────────────────────────────────

/// Typed text replaces each range.
pub fn insert_text(d: &mut Doc, text: &str) {
    let n = text.chars().count();
    let (c, s) = d.change_by_range(|_, r| {
        (
            vec![Change {
                from: r.from(),
                to: r.to(),
                insert: text.into(),
            }],
            Range::cursor(r.from() + n),
        )
    });
    d.apply(c, s, EditKind::Input);
}

/// Pasted text: one line per cursor when the line count matches the cursors, as CodeMirror does.
pub fn paste(d: &mut Doc, text: &str) {
    let text = text.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let per_range = d.selection.ranges.len() > 1 && lines.len() == d.selection.ranges.len();
    let mut i = 0;
    let (c, s) = d.change_by_range(|_, r| {
        let piece = if per_range { lines[i] } else { text.as_str() };
        i += 1;
        let n = piece.chars().count();
        (
            vec![Change {
                from: r.from(),
                to: r.to(),
                insert: piece.into(),
            }],
            Range::cursor(r.from() + n),
        )
    });
    d.apply(c, s, EditKind::Other);
}

/// Brackets that close themselves when typed (closeBrackets with the TeX language data).
pub const AUTO_CLOSE: [(char, char); 4] = [('(', ')'), ('[', ']'), ('{', '}'), ('$', '$')];

/// Typing one character with closeBrackets: an opening bracket adds its partner, a closing one
/// steps over the one already there. True when it handled the character.
pub fn close_brackets_input(d: &mut Doc, ch: char) -> bool {
    let closing = AUTO_CLOSE.iter().any(|&(_, c)| c == ch);
    let opening = AUTO_CLOSE.iter().find(|&&(o, _)| o == ch).map(|&(_, c)| c);
    // Step over: every cursor sits right before this closing character.
    if closing
        && d.selection
            .ranges
            .iter()
            .all(|r| r.is_empty() && d.slice(r.head, r.head + 1).starts_with(ch))
    {
        move_ranges(d, false, |_, r| r.head + 1);
        return true;
    }
    let Some(close) = opening else { return false };
    let (c, s) = d.change_by_range(|d, r| {
        if r.is_empty() {
            // Only before whitespace, a closing bracket or the line end, as CodeMirror's `before` set.
            let next = d.slice(r.head, r.head + 1).chars().next();
            if next.is_some_and(|n| !n.is_whitespace() && !")]}:;>$".contains(n)) {
                return (
                    vec![Change::insert(r.head, ch.to_string())],
                    Range::cursor(r.head + 1),
                );
            }
            (
                vec![Change::insert(r.head, format!("{ch}{close}"))],
                Range::cursor(r.head + 1),
            )
        } else {
            // Around a selection: wrap it.
            (
                vec![
                    Change::insert(r.from(), ch.to_string()),
                    Change::insert(r.to(), close.to_string()),
                ],
                Range::new(r.anchor + 1, r.head + 1),
            )
        }
    });
    d.apply(c, s, EditKind::Input);
    true
}

/// Backspace between an empty pair `()` deletes both.
fn delete_pair(d: &Doc, r: Range) -> Option<Change> {
    if !r.is_empty() || r.head == 0 {
        return None;
    }
    let around = d.slice(r.head - 1, r.head + 1);
    AUTO_CLOSE
        .iter()
        .any(|&(o, c)| around == format!("{o}{c}"))
        .then(|| Change::delete(r.head - 1, r.head + 1))
}

/// Backspace: the selection, or one character; in leading whitespace back to the previous indent
/// column (CodeMirror's deleteCharBackward); an empty auto-closed pair when `pairs`.
pub fn delete_char_backward(d: &mut Doc, indent: Indent, pairs: bool) {
    let (c, s) = d.change_by_range(|d, r| {
        if !r.is_empty() {
            return (
                vec![Change::delete(r.from(), r.to())],
                Range::cursor(r.from()),
            );
        }
        if pairs && let Some(c) = delete_pair(d, r) {
            return (vec![c], Range::cursor(r.head - 1));
        }
        let line = d.line_of(r.head);
        let start = d.line_start(line);
        let before = d.slice(start, r.head);
        if !before.is_empty() && before.chars().all(|c| c == ' ' || c == '\t') {
            let cols = indent_columns(&before, indent.tab_size());
            let target = (cols - 1) / indent.width() * indent.width();
            // Walk back over whitespace until the column drops to the target.
            let chars: Vec<char> = before.chars().collect();
            let mut keep = chars.len();
            while keep > 0
                && indent_columns(
                    &chars[..keep - 1].iter().collect::<String>(),
                    indent.tab_size(),
                ) >= target
            {
                keep -= 1;
                if indent_columns(&chars[..keep].iter().collect::<String>(), indent.tab_size())
                    <= target
                {
                    break;
                }
            }
            return (
                vec![Change::delete(start + keep, r.head)],
                Range::cursor(start + keep),
            );
        }
        let to = by_char(d, r.head, false);
        (vec![Change::delete(to, r.head)], Range::cursor(to))
    });
    d.apply(c, s, EditKind::Delete);
}

pub fn delete_char_forward(d: &mut Doc) {
    let (c, s) = d.change_by_range(|d, r| {
        if !r.is_empty() {
            return (
                vec![Change::delete(r.from(), r.to())],
                Range::cursor(r.from()),
            );
        }
        let to = by_char(d, r.head, true);
        (vec![Change::delete(r.head, to)], Range::cursor(r.head))
    });
    d.apply(c, s, EditKind::Delete);
}

pub fn delete_group_backward(d: &mut Doc) {
    let (c, s) = d.change_by_range(|d, r| {
        if !r.is_empty() {
            return (
                vec![Change::delete(r.from(), r.to())],
                Range::cursor(r.from()),
            );
        }
        let to = by_group(d, r.head, false);
        (vec![Change::delete(to, r.head)], Range::cursor(to))
    });
    d.apply(c, s, EditKind::Delete);
}

pub fn delete_group_forward(d: &mut Doc) {
    let (c, s) = d.change_by_range(|d, r| {
        if !r.is_empty() {
            return (
                vec![Change::delete(r.from(), r.to())],
                Range::cursor(r.from()),
            );
        }
        let to = by_group(d, r.head, true);
        (vec![Change::delete(r.head, to)], Range::cursor(r.head))
    });
    d.apply(c, s, EditKind::Delete);
}

/// The 0-based line spans the ranges cover, merged when they touch.
fn line_blocks(d: &Doc) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for r in &d.selection.ranges {
        let first = d.line_of(r.from());
        // A selection ending at a line's start doesn't include that line.
        let mut last = d.line_of(r.to());
        if !r.is_empty() && last > first && r.to() == d.line_start(last) {
            last -= 1;
        }
        match out.last_mut() {
            Some(b) if first <= b.1 + 1 => b.1 = b.1.max(last),
            _ => out.push((first, last)),
        }
    }
    out
}

/// Shift-Mod-k: removes the selected lines.
pub fn delete_line(d: &mut Doc) {
    let blocks = line_blocks(d);
    let mut changes = Vec::new();
    for &(a, b) in &blocks {
        let from = d.line_start(a);
        let to = if b + 1 < d.line_count() {
            d.line_start(b + 1)
        } else {
            d.len()
        };
        // The last line takes the break before it instead.
        let from = if b + 1 >= d.line_count() && a > 0 {
            d.line_end(a - 1)
        } else {
            from
        };
        changes.push(Change::delete(from, to));
    }
    let cs = ChangeSet::new(changes);
    let ranges = d
        .selection
        .ranges
        .iter()
        .map(|r| Range::cursor(cs.map(r.head, -1)))
        .collect();
    let s = Selection::normalized(ranges, d.selection.main);
    d.apply(cs, s, EditKind::Other);
}

/// Alt-Up / Alt-Down: moves the selected lines past their neighbour.
pub fn move_lines(d: &mut Doc, down: bool) -> bool {
    let blocks = line_blocks(d);
    let n = d.line_count();
    if blocks
        .iter()
        .any(|&(a, b)| if down { b + 1 >= n } else { a == 0 })
    {
        return false;
    }
    let mut changes = Vec::new();
    let mut shifts = Vec::new();
    for &(a, b) in &blocks {
        let block = (a..=b).map(|l| d.line(l)).collect::<Vec<_>>().join("\n");
        let (from, to, text, delta) = if down {
            let other = d.line(b + 1);
            (
                d.line_start(a),
                d.line_end(b + 1),
                format!("{other}\n{block}"),
                other.chars().count() as isize + 1,
            )
        } else {
            let other = d.line(a - 1);
            (
                d.line_start(a - 1),
                d.line_end(b),
                format!("{block}\n{other}"),
                -(other.chars().count() as isize + 1),
            )
        };
        changes.push(Change {
            from,
            to,
            insert: text,
        });
        shifts.push((d.line_start(a), d.line_end(b), delta));
    }
    let ranges = d
        .selection
        .ranges
        .iter()
        .map(|r| {
            let delta = shifts
                .iter()
                .find(|(f, t, _)| r.from() >= *f && r.from() <= *t)
                .map_or(0, |s| s.2);
            Range::new(
                (r.anchor as isize + delta) as usize,
                (r.head as isize + delta) as usize,
            )
        })
        .collect();
    let s = Selection::normalized(ranges, d.selection.main);
    d.apply(ChangeSet::new(changes), s, EditKind::Other);
    true
}

/// Shift-Alt-Up / Shift-Alt-Down: duplicates the selected lines; the selection moves with the copy below.
pub fn copy_lines(d: &mut Doc, down: bool) {
    let blocks = line_blocks(d);
    let mut changes = Vec::new();
    let mut total = 0isize;
    let mut shift_for = Vec::new();
    for &(a, b) in &blocks {
        let block = (a..=b).map(|l| d.line(l)).collect::<Vec<_>>().join("\n");
        let len = block.chars().count() as isize + 1;
        changes.push(Change::insert(d.line_end(b), format!("\n{block}")));
        if down {
            shift_for.push((d.line_start(a), d.line_end(b), total + len));
        } else {
            shift_for.push((d.line_start(a), d.line_end(b), total));
        }
        total += len;
    }
    let ranges = d
        .selection
        .ranges
        .iter()
        .map(|r| {
            let delta = shift_for
                .iter()
                .find(|(f, t, _)| r.from() >= *f && r.from() <= *t)
                .map_or(0, |s| s.2);
            Range::new(
                (r.anchor as isize + delta) as usize,
                (r.head as isize + delta) as usize,
            )
        })
        .collect();
    let s = Selection::normalized(ranges, d.selection.main);
    d.apply(ChangeSet::new(changes), s, EditKind::Other);
}

/// Mod-] / Tab: one level more on every selected line.
pub fn indent_more(d: &mut Doc, indent: Indent) {
    let unit = indent.unit();
    let mut changes = Vec::new();
    for (a, b) in line_blocks(d) {
        for l in a..=b {
            if !d.line(l).trim().is_empty() || a == b {
                changes.push(Change::insert(d.line_start(l), unit.clone()));
            }
        }
    }
    finish_line_edit(d, changes);
}

/// Mod-[ / Shift-Tab: one level less.
pub fn indent_less(d: &mut Doc, indent: Indent) {
    let mut changes = Vec::new();
    for (a, b) in line_blocks(d) {
        for l in a..=b {
            let text = d.line(l);
            let ws = leading_ws(&text);
            let cols = indent_columns(ws, indent.tab_size());
            if cols == 0 {
                continue;
            }
            let target = (cols - 1) / indent.width() * indent.width();
            let new = indent_string(target, indent);
            changes.push(Change {
                from: d.line_start(l),
                to: d.line_start(l) + ws.chars().count(),
                insert: new,
            });
        }
    }
    finish_line_edit(d, changes);
}

fn finish_line_edit(d: &mut Doc, changes: Vec<Change>) {
    let cs = ChangeSet::new(changes);
    let ranges = d
        .selection
        .ranges
        .iter()
        .map(|r| Range::new(cs.map(r.anchor, 1), cs.map(r.head, 1)))
        .collect();
    let s = Selection::normalized(ranges, d.selection.main);
    d.apply(cs, s, EditKind::Other);
}

/// Mod-/: comments the selected lines with `%`, or uncomments them when all are commented.
pub fn toggle_comment(d: &mut Doc) {
    let mut lines = Vec::new();
    for (a, b) in line_blocks(d) {
        lines.extend(a..=b);
    }
    let texts: Vec<String> = lines.iter().map(|&l| d.line(l)).collect();
    let nonblank: Vec<usize> = (0..lines.len())
        .filter(|&i| !texts[i].trim().is_empty())
        .collect();
    let all_commented = !nonblank.is_empty()
        && nonblank
            .iter()
            .all(|&i| texts[i].trim_start().starts_with('%'));
    let mut changes = Vec::new();
    if all_commented {
        for &i in &nonblank {
            let ws = leading_ws(&texts[i]).chars().count();
            let at = d.line_start(lines[i]) + ws;
            let rest = &texts[i][leading_ws(&texts[i]).len()..];
            let n = if rest.starts_with("% ") { 2 } else { 1 };
            changes.push(Change::delete(at, at + n));
        }
    } else {
        // At the smallest indentation, like CodeMirror's line comments.
        let col = nonblank
            .iter()
            .map(|&i| leading_ws(&texts[i]).chars().count())
            .min()
            .unwrap_or(0);
        let targets: Vec<usize> = if nonblank.is_empty() {
            (0..lines.len()).collect()
        } else {
            nonblank
        };
        for i in targets {
            changes.push(Change::insert(d.line_start(lines[i]) + col, "% "));
        }
    }
    finish_line_edit(d, changes);
}

/// Environments by nesting: `(begin?, name, position after the brace)`, comments skipped.
fn env_tokens(d: &Doc, upto: usize) -> Vec<(bool, String, usize, usize)> {
    let mut out = Vec::new();
    let last = d.line_of(upto);
    for l in 0..=last {
        let text = d.line(l);
        if !text.contains('\\') {
            continue;
        }
        let comment = comment_start(&text);
        let start = d.line_start(l);
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if comment.is_some_and(|c| i >= c) {
                break;
            }
            let rest: String = chars[i..].iter().collect();
            for (kw, begin) in [("\\begin{", true), ("\\end{", false)] {
                if rest.starts_with(kw) {
                    let name_from = i + kw.chars().count();
                    if let Some(close) = chars[name_from..].iter().position(|&c| c == '}') {
                        let name: String = chars[name_from..name_from + close].iter().collect();
                        if !name.contains('{') {
                            out.push((begin, name, start + i, l));
                        }
                    }
                }
            }
            i += 1;
        }
    }
    out
}

/// Index of the first unescaped `%`.
pub fn comment_start(text: &str) -> Option<usize> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' {
            i += 2;
            continue;
        }
        if chars[i] == '%' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Their lines are content, not code: indenting them would change it.
const VERBATIM: [&str; 6] = [
    "verbatim",
    "verbatim*",
    "Verbatim",
    "lstlisting",
    "minted",
    "comment",
];

/// The indentation (in columns) the environment rules give a new line at `pos`, or None to keep the
/// line's own (editor/environments.ts `environmentIndent`).
pub fn environment_indent(
    d: &Doc,
    pos: usize,
    rest_of_line: &str,
    indent: Indent,
) -> Option<usize> {
    let mut open: Vec<(String, usize)> = Vec::new();
    for (begin, name, at, line) in env_tokens(d, pos) {
        if at >= pos {
            break;
        }
        if begin {
            open.push((name, line));
        } else {
            open.pop();
        }
    }
    let (name, line) = open.last()?;
    if VERBATIM.contains(&name.as_str()) {
        return None;
    }
    let closing = rest_of_line.trim_start().starts_with("\\end");
    if name == "document" && !closing {
        return None;
    }
    let base = indent_columns(&d.line(*line), indent.tab_size());
    Some(if closing { base } else { base + indent.width() })
}

/// Enter: a line break, the new line indented by the environment rules or like the current line.
pub fn insert_newline_and_indent(d: &mut Doc, indent: Indent) {
    let (c, s) = d.change_by_range(|d, r| {
        let line = d.line_of(r.from());
        let text = d.line(line);
        let start = d.line_start(line);
        let after: String = d.slice(r.to(), d.line_end(d.line_of(r.to())));
        let cols = environment_indent(d, r.from(), &after, indent).unwrap_or_else(|| {
            // An empty line keeps only what is before the cursor.
            let before = d.slice(start, r.from());
            indent_columns(
                if text.trim().is_empty() {
                    &before
                } else {
                    leading_ws(&text)
                },
                indent.tab_size(),
            )
        });
        let ws = indent_string(cols, indent);
        // Trailing blanks before the cursor go (CodeMirror trims the line it leaves).
        let before = d.slice(start, r.from());
        let trimmed = before.trim_end_matches([' ', '\t']).chars().count();
        let from = if before.trim().is_empty() {
            r.from()
        } else {
            start + trimmed
        };
        // Blanks after the cursor go too; the new line gets its own indentation.
        let skip = after
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .count();
        let insert = format!("\n{ws}");
        let n = insert.chars().count();
        (
            vec![Change {
                from,
                to: r.to() + skip,
                insert,
            }],
            Range::cursor(from + n),
        )
    });
    d.apply(c, s, EditKind::Other);
}

// ── Formatting (editor/shortcuts.ts) ──────────────────────────────────────

/// Wraps each range in `\cmd{…}`, or unwraps it when it already is (Ctrl+B on bold text removes it).
pub fn toggle_wrap(d: &mut Doc, command: &str) {
    let open = format!("\\{command}{{");
    let n = open.chars().count();
    let (c, s) = d.change_by_range(|d, r| {
        let before = d.slice(r.from().saturating_sub(n), r.from());
        let after = d.slice(r.to(), r.to() + 1);
        if before == open && after == "}" {
            return (
                vec![
                    Change::delete(r.from() - n, r.from()),
                    Change::delete(r.to(), r.to() + 1),
                ],
                Range::new(r.anchor - n, r.head - n),
            );
        }
        let text = d.slice(r.from(), r.to());
        if text.starts_with(&open) && text.ends_with('}') && text.chars().count() > n {
            let inner: String = text
                .chars()
                .skip(n)
                .take(text.chars().count() - n - 1)
                .collect();
            let len = inner.chars().count();
            return (
                vec![Change {
                    from: r.from(),
                    to: r.to(),
                    insert: inner,
                }],
                Range::new(r.from(), r.from() + len),
            );
        }
        let range = if r.is_empty() {
            Range::cursor(r.from() + n)
        } else {
            Range::new(r.from() + n, r.to() + n)
        };
        (
            vec![
                Change::insert(r.from(), open.clone()),
                Change::insert(r.to(), "}"),
            ],
            range,
        )
    });
    d.apply(c, s, EditKind::Other);
}

const HEADINGS: [&str; 6] = [
    "part",
    "chapter",
    "section",
    "subsection",
    "subsubsection",
    "paragraph",
];

/// Moves the heading on the cursor's line one level deeper (1) or higher (-1). False when the line
/// has no heading, so the key does something else.
pub fn shift_heading(d: &mut Doc, delta: isize) -> bool {
    let n = d.line_of(d.selection.main().head);
    let text = d.line(n);
    let ws = leading_ws(&text).chars().count();
    let rest: String = text.chars().skip(ws).collect();
    let Some(after_slash) = rest.strip_prefix('\\') else {
        return false;
    };
    let Some((i, name)) = HEADINGS.iter().enumerate().find(|(_, h)| {
        after_slash.starts_with(**h)
            && after_slash[h.len()..]
                .trim_start_matches('*')
                .starts_with(['[', '{'])
    }) else {
        return false;
    };
    let j = i as isize + delta;
    if j < 0 || j >= HEADINGS.len() as isize {
        return true;
    }
    let from = d.line_start(n) + ws + 1;
    let sel = d.selection.clone();
    let change = Change {
        from,
        to: from + name.len(),
        insert: HEADINGS[j as usize].into(),
    };
    let cs = ChangeSet::new(vec![change]);
    let ranges = sel
        .ranges
        .iter()
        .map(|r| Range::new(cs.map(r.anchor, 1), cs.map(r.head, 1)))
        .collect();
    let s = Selection::normalized(ranges, sel.main);
    d.apply(cs, s, EditKind::Other);
    true
}

// ── Templates ──────────────────────────────────────────────────────────────

/// Inserts a snippet template at the main selection (Editor.svelte insertTemplate): a multi-line
/// block after text starts on a new line; `\t` and two-space levels become the indentation unit;
/// later lines take the current line's indentation; `${}` and `${text}` mark fields and the first
/// one is selected.
pub fn insert_template(d: &mut Doc, template: &str, indent: Indent) {
    let r = d.selection.main();
    let line = d.line_of(r.from());
    let line_text = d.line(line);
    let col = r.from() - d.line_start(line);
    let before: String = line_text.chars().take(col).collect();
    let mut prefix = String::new();
    let mut cur_indent = leading_ws(&line_text).to_owned();
    if template.contains('\n') && r.is_empty() {
        if before.trim().is_empty() {
            cur_indent = before.clone();
        } else {
            prefix = format!("\n{cur_indent}");
        }
    }
    let unit = indent.unit();
    // Two spaces per level in generated blocks, and tabs, become the chosen unit.
    let lines: Vec<String> = template
        .split('\n')
        .enumerate()
        .map(|(i, l)| {
            let lead = l.chars().take_while(|c| *c == '\t').count();
            let rest = &l[lead..];
            let spaces = rest.chars().take_while(|c| *c == ' ').count() / 2 * 2;
            let body = &rest[spaces..];
            let levels = lead + spaces / 2;
            let ind = if i == 0 {
                String::new()
            } else {
                cur_indent.clone()
            };
            format!("{ind}{}{body}", unit.repeat(levels))
        })
        .collect();
    let text = lines.join("\n");
    // Fields: the first one's text is selected; the markers go.
    let mut out = String::new();
    let mut first: Option<(usize, usize)> = None;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$'
            && chars.get(i + 1) == Some(&'{')
            && let Some(close) = chars[i + 2..].iter().position(|c| *c == '}')
        {
            let inner: String = chars[i + 2..i + 2 + close].iter().collect();
            let start = out.chars().count();
            out.push_str(&inner);
            if first.is_none() {
                first = Some((start, start + inner.chars().count()));
            }
            i += 3 + close;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    let at = r.from() + prefix.chars().count();
    let insert = format!("{prefix}{out}");
    let n = insert.chars().count();
    let sel = match first {
        Some((a, b)) => Range::new(at + a, at + b),
        None => Range::cursor(r.from() + n),
    };
    d.apply(
        ChangeSet::new(vec![Change {
            from: r.from(),
            to: r.to(),
            insert,
        }]),
        Selection::single(sel),
        EditKind::Other,
    );
}

// ── Brackets ───────────────────────────────────────────────────────────────

const BRACKETS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

/// The bracket at or before `pos` and its partner, for highlighting and Shift-Mod-\.
pub fn matching_bracket(d: &Doc, pos: usize) -> Option<(usize, usize)> {
    let at = |p: usize| d.slice(p, p + 1).chars().next();
    for p in [pos, pos.wrapping_sub(1)] {
        if p >= d.len() {
            continue;
        }
        let Some(c) = at(p) else { continue };
        for &(o, cl) in &BRACKETS {
            if c == o {
                return scan(d, p, o, cl, true).map(|m| (p, m));
            }
            if c == cl {
                return scan(d, p, o, cl, false).map(|m| (p, m));
            }
        }
    }
    None
}

fn scan(d: &Doc, from: usize, open: char, close: char, forward: bool) -> Option<usize> {
    // Like CodeMirror, look at most 10000 characters away.
    let mut depth = 0i32;
    let range: Box<dyn Iterator<Item = usize>> = if forward {
        Box::new(from..d.len().min(from + 10_000))
    } else {
        Box::new((from.saturating_sub(10_000)..=from).rev())
    };
    let text = d.slice(
        if forward {
            from
        } else {
            from.saturating_sub(10_000)
        },
        if forward {
            d.len().min(from + 10_000)
        } else {
            from + 1
        },
    );
    let chars: Vec<char> = text.chars().collect();
    let base = if forward {
        from
    } else {
        from.saturating_sub(10_000)
    };
    for p in range {
        let c = chars[p - base];
        if c == open {
            depth += if forward { 1 } else { -1 };
        } else if c == close {
            depth += if forward { -1 } else { 1 };
        }
        if depth == 0 {
            return Some(p);
        }
    }
    None
}

pub fn cursor_matching_bracket(d: &mut Doc, extend: bool) -> bool {
    let head = d.selection.main().head;
    let Some((_, m)) = matching_bracket(d, head) else {
        return false;
    };
    move_ranges(d, extend, |_, _| m);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str, at: usize) -> Doc {
        let mut d = Doc::new(text);
        d.selection = Selection::cursor(at);
        d
    }

    #[test]
    fn groups_move_over_words_and_punctuation() {
        let mut d = doc("\\section{Intro text}", 0);
        group_right(&mut d, false);
        assert_eq!(d.selection.main().head, 1);
        group_right(&mut d, false);
        assert_eq!(d.selection.main().head, 8);
        group_right(&mut d, false);
        group_right(&mut d, false);
        assert_eq!(d.selection.main().head, 14);
        group_left(&mut d, false);
        assert_eq!(d.selection.main().head, 9);
    }

    #[test]
    fn home_toggles_between_indentation_and_line_start() {
        let mut d = doc("    \\item x", 8);
        line_boundary_backward(&mut d, false);
        assert_eq!(d.selection.main().head, 4);
        line_boundary_backward(&mut d, false);
        assert_eq!(d.selection.main().head, 0);
        line_boundary_backward(&mut d, false);
        assert_eq!(d.selection.main().head, 4);
    }

    #[test]
    fn backspace_in_indentation_goes_back_a_level() {
        let mut d = doc("      x", 6);
        delete_char_backward(&mut d, Indent::Spaces(4), true);
        assert_eq!(d.text(), "    x");
        delete_char_backward(&mut d, Indent::Spaces(4), true);
        assert_eq!(d.text(), "x");
    }

    #[test]
    fn brackets_close_and_step_over_and_delete_in_pairs() {
        let mut d = doc("", 0);
        assert!(close_brackets_input(&mut d, '{'));
        assert_eq!((d.text().as_str(), d.selection.main().head), ("{}", 1));
        insert_text(&mut d, "a");
        assert!(close_brackets_input(&mut d, '}'));
        assert_eq!((d.text().as_str(), d.selection.main().head), ("{a}", 3));
        let mut d = doc("()", 1);
        delete_char_backward(&mut d, Indent::Spaces(4), true);
        assert_eq!(d.text(), "");
        // Before a word, only the opening bracket.
        let mut d = doc("word", 0);
        close_brackets_input(&mut d, '(');
        assert_eq!(d.text(), "(word");
    }

    #[test]
    fn enter_keeps_indentation_and_indents_inside_environments() {
        let mut d = doc("\\begin{itemize}\n  \\item a", 25);
        insert_newline_and_indent(&mut d, Indent::Spaces(2));
        assert_eq!(d.text(), "\\begin{itemize}\n  \\item a\n  ");
        // Directly in document the line keeps its own.
        let mut d = doc("\\begin{document}\nText", 21);
        insert_newline_and_indent(&mut d, Indent::Spaces(4));
        assert_eq!(d.text(), "\\begin{document}\nText\n");
        // \end lines up with its \begin.
        let mut d = doc("  \\begin{figure}\n      \\end{figure}", 23);
        insert_newline_and_indent(&mut d, Indent::Spaces(4));
        assert_eq!(d.text(), "  \\begin{figure}\n      \n  \\end{figure}");
    }

    #[test]
    fn lines_move_copy_and_delete() {
        let mut d = doc("a\nb\nc", 2);
        assert!(move_lines(&mut d, true));
        assert_eq!((d.text().as_str(), d.selection.main().head), ("a\nc\nb", 4));
        assert!(!move_lines(&mut d, true));
        copy_lines(&mut d, true);
        assert_eq!(
            (d.text().as_str(), d.selection.main().head),
            ("a\nc\nb\nb", 6)
        );
        delete_line(&mut d);
        assert_eq!(d.text(), "a\nc\nb");
    }

    #[test]
    fn indent_and_comment_toggle() {
        let mut d = doc("a\nb", 0);
        d.selection = Selection::single(Range::new(0, 3));
        indent_more(&mut d, Indent::Spaces(2));
        assert_eq!(d.text(), "  a\n  b");
        indent_less(&mut d, Indent::Spaces(2));
        assert_eq!(d.text(), "a\nb");
        toggle_comment(&mut d);
        assert_eq!(d.text(), "% a\n% b");
        toggle_comment(&mut d);
        assert_eq!(d.text(), "a\nb");
    }

    #[test]
    fn next_occurrence_adds_ranges() {
        let mut d = doc("foo bar foo foo", 1);
        assert!(select_next_occurrence(&mut d));
        assert_eq!(d.selection.main(), Range::new(0, 3));
        assert!(select_next_occurrence(&mut d));
        assert!(select_next_occurrence(&mut d));
        assert_eq!(d.selection.ranges.len(), 3);
        assert!(!select_next_occurrence(&mut d));
        insert_text(&mut d, "x");
        assert_eq!(d.text(), "x bar x x");
    }

    #[test]
    fn bold_toggles_and_headings_shift() {
        let mut d = doc("word", 0);
        d.selection = Selection::single(Range::new(0, 4));
        toggle_wrap(&mut d, "textbf");
        assert_eq!(d.text(), "\\textbf{word}");
        assert_eq!(
            d.slice(d.selection.main().from(), d.selection.main().to()),
            "word"
        );
        toggle_wrap(&mut d, "textbf");
        assert_eq!(d.text(), "word");
        let mut d = doc("  \\section{Intro}", 5);
        assert!(shift_heading(&mut d, 1));
        assert_eq!(d.text(), "  \\subsection{Intro}");
        assert!(shift_heading(&mut d, -1));
        assert!(shift_heading(&mut d, -1));
        assert_eq!(d.text(), "  \\chapter{Intro}");
        let mut d = doc("plain", 0);
        assert!(!shift_heading(&mut d, 1));
    }

    #[test]
    fn templates_indent_and_select_their_first_field() {
        let mut d = doc("Some text", 9);
        insert_template(
            &mut d,
            "\\begin{itemize}\n\t\\item ${}\n\\end{itemize}",
            Indent::Spaces(2),
        );
        assert_eq!(
            d.text(),
            "Some text\n\\begin{itemize}\n  \\item \n\\end{itemize}"
        );
        assert_eq!(d.selection.main(), Range::cursor(34));
        let mut d = doc("    ", 4);
        insert_template(&mut d, "\\href{${https://}}{${}}", Indent::Spaces(4));
        assert_eq!(d.text(), "    \\href{https://}{}");
        assert_eq!(
            d.slice(d.selection.main().from(), d.selection.main().to()),
            "https://"
        );
    }

    #[test]
    fn brackets_match_across_nesting() {
        let d = doc("\\frac{a}{\\sqrt{b}}", 8);
        assert_eq!(matching_bracket(&d, 8), Some((8, 17)));
        assert_eq!(matching_bracket(&d, 18), Some((17, 8)));
    }
}
