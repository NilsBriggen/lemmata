//! An open file's text, selections and undo history: what CodeMirror's EditorState held.
//!
//! Positions are char offsets into the rope. A selection is one or more ranges (anchor, head); every
//! edit is a set of non-overlapping changes in the coordinates of the text before it, so it can be
//! inverted for undo and positions can be mapped across it.

use ropey::Rope;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub anchor: usize,
    pub head: usize,
}

impl Range {
    pub fn cursor(at: usize) -> Self {
        Self {
            anchor: at,
            head: at,
        }
    }
    pub fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }
    pub fn from(&self) -> usize {
        self.anchor.min(self.head)
    }
    pub fn to(&self) -> usize {
        self.anchor.max(self.head)
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

/// One or more ranges, sorted and not overlapping; `main` is the one that scrolls into view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub ranges: Vec<Range>,
    pub main: usize,
}

impl Selection {
    pub fn single(r: Range) -> Self {
        Self {
            ranges: vec![r],
            main: 0,
        }
    }
    pub fn cursor(at: usize) -> Self {
        Self::single(Range::cursor(at))
    }
    pub fn main(&self) -> Range {
        self.ranges[self.main]
    }

    /// Sorts the ranges and merges the ones that touch or overlap, as CodeMirror's EditorSelection.create.
    pub fn normalized(ranges: Vec<Range>, main: usize) -> Self {
        let main_range = ranges[main.min(ranges.len() - 1)];
        let mut sorted = ranges;
        sorted.sort_by_key(|r| (r.from(), r.to()));
        let mut out: Vec<Range> = Vec::with_capacity(sorted.len());
        for r in sorted {
            if let Some(last) = out.last_mut()
                && (r.from() < last.to()
                    || (r.from() == last.to() && (r.is_empty() || last.is_empty())))
            {
                // Merged ranges keep the direction of the later one.
                let (from, to) = (last.from(), last.to().max(r.to()));
                *last = if r.head < r.anchor {
                    Range::new(to, from)
                } else {
                    Range::new(from, to)
                };
                continue;
            }
            out.push(r);
        }
        let main = out
            .iter()
            .position(|r| r.from() <= main_range.from() && main_range.to() <= r.to())
            .unwrap_or(0);
        Self { ranges: out, main }
    }
}

/// Replace `from..to` (in the text before the edit) with `insert`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

impl Change {
    pub fn insert(at: usize, text: impl Into<String>) -> Self {
        Self {
            from: at,
            to: at,
            insert: text.into(),
        }
    }
    pub fn delete(from: usize, to: usize) -> Self {
        Self {
            from,
            to,
            insert: String::new(),
        }
    }
    fn delta(&self) -> isize {
        self.insert.chars().count() as isize - (self.to - self.from) as isize
    }
}

/// Changes sorted by position, not overlapping, all in the coordinates of the text before them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeSet(pub Vec<Change>);

impl ChangeSet {
    pub fn new(mut changes: Vec<Change>) -> Self {
        changes.retain(|c| c.from != c.to || !c.insert.is_empty());
        changes.sort_by_key(|c| (c.from, c.to));
        Self(changes)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Where `pos` ends up. Inside a replaced range it goes to the range's start (`assoc < 0`) or
    /// past the inserted text (`assoc > 0`); at an insertion point, likewise.
    pub fn map(&self, pos: usize, assoc: i8) -> usize {
        let mut shift: isize = 0;
        for c in &self.0 {
            let start = (c.from as isize + shift) as usize;
            let inserted = c.insert.chars().count();
            if c.from == c.to {
                if pos < c.from || (pos == c.from && assoc < 0) {
                    break;
                }
                if pos == c.from {
                    return start + inserted;
                }
            } else {
                if pos <= c.from {
                    break;
                }
                if pos < c.to {
                    return if assoc < 0 { start } else { start + inserted };
                }
            }
            shift += c.delta();
        }
        (pos as isize + shift) as usize
    }

    /// Applies the changes, returning the set that undoes them.
    fn apply(&self, rope: &mut Rope) -> ChangeSet {
        let mut inverse = Vec::with_capacity(self.0.len());
        let mut shift: isize = 0;
        for c in &self.0 {
            let removed: String = rope.slice(c.from..c.to).to_string();
            let from = (c.from as isize + shift) as usize;
            inverse.push(Change {
                from,
                to: from + c.insert.chars().count(),
                insert: removed,
            });
            shift += c.delta();
        }
        // Back to front, so earlier positions stay valid.
        for c in self.0.iter().rev() {
            rope.remove(c.from..c.to);
            rope.insert(c.from, &c.insert);
        }
        ChangeSet(inverse)
    }
}

/// What kind of edit a change was, for grouping undo steps like CodeMirror's history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    /// Typing: consecutive ones merge into one undo step.
    Input,
    /// Deleting with Backspace/Delete: also merge.
    Delete,
    /// Everything else is its own step.
    Other,
}

struct Step {
    /// Applied in order to undo; each undoes the transaction after it.
    inverses: Vec<ChangeSet>,
    redos: Vec<ChangeSet>,
    before: Selection,
    after: Selection,
}

/// CodeMirror's `newGroupDelay`.
const GROUP_DELAY: Duration = Duration::from_millis(500);

pub struct Doc {
    pub rope: Rope,
    pub selection: Selection,
    undo: Vec<Step>,
    redo: Vec<Step>,
    last: Option<(Instant, EditKind)>,
    /// Bumped on every change, so views know to re-highlight and re-measure.
    pub version: u64,
    /// The first line touched by the last change, for incremental work.
    pub changed_from_line: usize,
    /// Every change since the view last looked, in order (undo and redo too), so positions it
    /// keeps (folds, snippet fields, problems) can follow the text.
    pub journal: Vec<ChangeSet>,
}

impl Doc {
    pub fn new(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            selection: Selection::cursor(0),
            undo: Vec::new(),
            redo: Vec::new(),
            last: None,
            version: 0,
            changed_from_line: 0,
            journal: Vec::new(),
        }
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn len(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Line `n` (0-based) without its line break.
    pub fn line(&self, n: usize) -> String {
        let s = self.rope.line(n);
        let mut t = s.to_string();
        while t.ends_with('\n') || t.ends_with('\r') {
            t.pop();
        }
        t
    }

    pub fn line_start(&self, n: usize) -> usize {
        self.rope.line_to_char(n)
    }

    /// The end of line `n`, before its line break.
    pub fn line_end(&self, n: usize) -> usize {
        self.line_start(n) + self.line(n).chars().count()
    }

    pub fn line_of(&self, pos: usize) -> usize {
        self.rope.char_to_line(pos.min(self.len()))
    }

    pub fn slice(&self, from: usize, to: usize) -> String {
        self.rope
            .slice(from.min(self.len())..to.min(self.len()))
            .to_string()
    }

    /// Applies `changes` and moves the selection to `selection` (given in the coordinates after the change).
    pub fn apply(&mut self, changes: ChangeSet, selection: Selection, kind: EditKind) {
        let before = self.selection.clone();
        if changes.is_empty() {
            self.selection = selection;
            return;
        }
        self.changed_from_line = changes.0.first().map_or(0, |c| self.line_of(c.from));
        let inverse = changes.apply(&mut self.rope);
        self.journal.push(changes.clone());
        self.selection = selection;
        self.version += 1;
        self.redo.clear();
        let now = Instant::now();
        let merge = kind != EditKind::Other
            && self
                .last
                .is_some_and(|(t, k)| k == kind && now.duration_since(t) < GROUP_DELAY)
            && !self.undo.is_empty();
        if merge {
            let step = self.undo.last_mut().unwrap();
            step.inverses.insert(0, inverse);
            step.redos.push(changes);
            step.after = self.selection.clone();
        } else {
            self.undo.push(Step {
                inverses: vec![inverse],
                redos: vec![changes],
                before,
                after: self.selection.clone(),
            });
        }
        self.last = Some((now, kind));
    }

    /// Maps a position through the changes in `journal` (as `ChangeSet::map`).
    pub fn map_through(journal: &[ChangeSet], pos: usize, assoc: i8) -> usize {
        journal.iter().fold(pos, |p, c| c.map(p, assoc))
    }

    /// Applies `changes` as part of the last undo step (an edit that follows another, like the
    /// renamed `\end` of a renamed `\begin`), keeping the selection mapped.
    pub fn apply_merged(&mut self, changes: ChangeSet) {
        if changes.is_empty() {
            return;
        }
        let ranges = self
            .selection
            .ranges
            .iter()
            .map(|r| Range::new(changes.map(r.anchor, -1), changes.map(r.head, -1)))
            .collect();
        let sel = Selection::normalized(ranges, self.selection.main);
        let inverse = changes.apply(&mut self.rope);
        self.journal.push(changes.clone());
        self.selection = sel;
        self.version += 1;
        match self.undo.last_mut() {
            Some(step) => {
                step.inverses.insert(0, inverse);
                step.redos.push(changes);
                step.after = self.selection.clone();
            }
            None => self.undo.push(Step {
                inverses: vec![inverse],
                redos: vec![changes],
                before: self.selection.clone(),
                after: self.selection.clone(),
            }),
        }
    }

    /// Ends the current undo group, so the next edit starts a new one.
    pub fn break_group(&mut self) {
        self.last = None;
    }

    pub fn undo(&mut self) -> bool {
        let Some(step) = self.undo.pop() else {
            return false;
        };
        for inv in &step.inverses {
            inv.apply(&mut self.rope);
            self.journal.push(inv.clone());
        }
        self.changed_from_line = 0;
        self.selection = clamp(step.before.clone(), self.len());
        self.version += 1;
        self.last = None;
        self.redo.push(step);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(step) = self.redo.pop() else {
            return false;
        };
        for c in &step.redos {
            c.apply(&mut self.rope);
            self.journal.push(c.clone());
        }
        self.changed_from_line = 0;
        self.selection = clamp(step.after.clone(), self.len());
        self.version += 1;
        self.last = None;
        self.undo.push(step);
        true
    }

    /// Replaces the whole text, keeping the history (an outside reload of an open file is one undo step).
    pub fn replace_all(&mut self, text: &str) {
        let len = self.len();
        let main = self.selection.main().head.min(text.chars().count());
        self.apply(
            ChangeSet::new(vec![Change {
                from: 0,
                to: len,
                insert: text.into(),
            }]),
            Selection::cursor(main),
            EditKind::Other,
        );
    }

    /// CodeMirror's changeByRange: `f` sees each range and returns its changes (in the coordinates
    /// before the edit) and its new range (in the coordinates after its own changes only).
    pub fn change_by_range(
        &self,
        mut f: impl FnMut(&Doc, Range) -> (Vec<Change>, Range),
    ) -> (ChangeSet, Selection) {
        let mut all = Vec::new();
        let mut ranges = Vec::with_capacity(self.selection.ranges.len());
        let mut shift: isize = 0;
        for &r in &self.selection.ranges {
            let (changes, nr) = f(self, r);
            let own: isize = changes.iter().map(Change::delta).sum();
            // Changes of earlier ranges moved this range by `shift`.
            let mv = |p: usize| (p as isize + shift).max(0) as usize;
            ranges.push(Range::new(mv(nr.anchor), mv(nr.head)));
            shift += own;
            all.extend(changes);
        }
        (
            ChangeSet::new(all),
            Selection::normalized(ranges, self.selection.main),
        )
    }
}

fn clamp(mut s: Selection, len: usize) -> Selection {
    for r in &mut s.ranges {
        r.anchor = r.anchor.min(len);
        r.head = r.head.min(len);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str, ranges: &[(usize, usize)]) -> Doc {
        let mut d = Doc::new(text);
        d.selection =
            Selection::normalized(ranges.iter().map(|&(a, h)| Range::new(a, h)).collect(), 0);
        d
    }

    fn type_text(d: &mut Doc, text: &str) {
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

    #[test]
    fn typing_at_several_cursors_moves_each() {
        let mut d = doc("ab\ncd", &[(1, 1), (4, 4)]);
        type_text(&mut d, "XY");
        assert_eq!(d.text(), "aXYb\ncXYd");
        assert_eq!(d.selection.ranges, vec![Range::cursor(3), Range::cursor(8)]);
    }

    #[test]
    fn typing_replaces_selections() {
        let mut d = doc("hello world", &[(0, 5)]);
        type_text(&mut d, "bye");
        assert_eq!(d.text(), "bye world");
        assert_eq!(d.selection.main(), Range::cursor(3));
    }

    #[test]
    fn undo_groups_quick_typing_and_redo_restores() {
        let mut d = doc("", &[(0, 0)]);
        type_text(&mut d, "a");
        type_text(&mut d, "b");
        d.break_group();
        type_text(&mut d, "c");
        assert_eq!(d.text(), "abc");
        assert!(d.undo());
        assert_eq!(d.text(), "ab");
        assert!(d.undo());
        assert_eq!(d.text(), "");
        assert_eq!(d.selection.main(), Range::cursor(0));
        assert!(!d.undo());
        assert!(d.redo());
        assert_eq!(d.text(), "ab");
        assert!(d.redo());
        assert_eq!(
            (d.text().as_str(), d.selection.main()),
            ("abc", Range::cursor(3))
        );
    }

    #[test]
    fn a_new_edit_drops_the_redo_steps() {
        let mut d = doc("", &[(0, 0)]);
        type_text(&mut d, "a");
        d.undo();
        type_text(&mut d, "b");
        assert!(!d.redo());
        assert_eq!(d.text(), "b");
    }

    #[test]
    fn changes_map_positions() {
        let cs = ChangeSet::new(vec![Change::insert(2, "xx"), Change::delete(5, 7)]);
        assert_eq!(cs.map(0, 1), 0);
        assert_eq!(cs.map(2, -1), 2);
        assert_eq!(cs.map(2, 1), 4);
        assert_eq!(cs.map(4, 1), 6);
        assert_eq!(cs.map(6, -1), 7);
        assert_eq!(cs.map(9, 1), 9);
    }

    #[test]
    fn overlapping_ranges_merge() {
        let s = Selection::normalized(
            vec![
                Range::new(5, 2),
                Range::new(4, 8),
                Range::cursor(10),
                Range::cursor(10),
            ],
            2,
        );
        assert_eq!(s.ranges, vec![Range::new(2, 8), Range::cursor(10)]);
        assert_eq!(s.main, 1);
    }

    #[test]
    fn unicode_counts_chars_not_bytes() {
        let mut d = doc("Grüße\n", &[(5, 5)]);
        type_text(&mut d, "!");
        assert_eq!(d.text(), "Grüße!\n");
        assert_eq!(d.line(0), "Grüße!");
        assert_eq!(d.line_end(0), 6);
    }
}
