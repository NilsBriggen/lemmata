//! Keeps `\begin{X}` and `\end{X}` in step, like Overleaf (editor/environments.ts): rename one and
//! the other follows, Enter after an unclosed `\begin{X}` adds the `\end{X}`, and a `\begin` or
//! `\end` without a partner gets a dotted underline.

use super::buffer::{Change, ChangeSet, Doc, EditKind, Range, Selection};
use super::commands::{Indent, comment_start};
use regex::Regex;
use std::sync::LazyLock;

static TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\(begin|end)\{([^{}\n]*)\}").unwrap());
static NEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\(?:begin|end)\s*\{").unwrap());
static OPENING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)\\begin\{([^{}]+)\}$").unwrap());

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub begin: bool,
    pub name: String,
    /// The name between the braces, in chars.
    pub from: usize,
    pub to: usize,
    /// Index of the partner token, if paired.
    pub partner: Option<usize>,
}

/// All `\begin`/`\end` tokens, paired by nesting. Commented parts are skipped.
pub fn tokens(d: &Doc) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for n in 0..d.line_count() {
        let line = d.line(n);
        if !line.contains('\\') {
            continue;
        }
        let comment =
            comment_start(&line).map(|c| line.char_indices().nth(c).map_or(line.len(), |(b, _)| b));
        let start = d.line_start(n);
        for m in TOKEN.captures_iter(&line) {
            let whole = m.get(0).unwrap();
            if comment.is_some_and(|c| whole.start() >= c) {
                break;
            }
            let name = m.get(2).unwrap();
            let from = start + line[..name.start()].chars().count();
            let t = Token {
                begin: &m[1] == "begin",
                name: name.as_str().into(),
                from,
                to: from + name.as_str().chars().count(),
                partner: None,
            };
            let i = out.len();
            let begin = t.begin;
            out.push(t);
            if begin {
                stack.push(i);
            } else if let Some(b) = stack.pop() {
                out[i].partner = Some(b);
                out[b].partner = Some(i);
            }
        }
    }
    out
}

/// Whether line `n` could hold a token, so an edit there is worth checking.
pub fn near_token(d: &Doc, n: usize) -> bool {
    NEAR.is_match(&d.line(n))
}

/// The partner edit after `change` (one change, applied to the text `before` was scanned from)
/// renamed one half of a pair that matched.
pub fn paired_edit(before: &[Token], change: &ChangeSet, after: &Doc) -> Option<ChangeSet> {
    let [c] = change.0.as_slice() else {
        return None;
    };
    let i = before
        .iter()
        .position(|t| c.from >= t.from && c.to <= t.to)?;
    let tok = &before[i];
    let other = &before[tok.partner?];
    // Only follow when they matched before this edit.
    if other.name != tok.name {
        return None;
    }
    let (from, to) = (change.map(tok.from, -1), change.map(tok.to, 1));
    let name = after.slice(from, to);
    if name.contains(['{', '}', '\n']) {
        return None;
    }
    let (pf, pt) = (change.map(other.from, -1), change.map(other.to, 1));
    if after.slice(pf, pt) == name {
        return None;
    }
    Some(ChangeSet::new(vec![Change {
        from: pf,
        to: pt,
        insert: name,
    }]))
}

/// Enter right after an unclosed `\begin{X}`: adds the `\end{X}` below, the cursor in between.
pub fn close_environment(d: &mut Doc, indent: Indent) -> bool {
    let sel = d.selection.main();
    if !sel.is_empty() || d.selection.ranges.len() > 1 {
        return false;
    }
    let n = d.line_of(sel.head);
    let line = d.line(n);
    let Some(m) = OPENING.captures(&line) else {
        return false;
    };
    if sel.head != d.line_end(n) {
        return false;
    }
    let start = d.line_start(n);
    let toks = tokens(d);
    let Some(here) = toks
        .iter()
        .find(|t| t.begin && t.from >= start && t.to <= sel.head)
    else {
        return false;
    };
    if here.partner.is_some() {
        return false;
    }
    let lead = &m[1];
    let body = format!("\n{lead}{}", indent.unit());
    let insert = format!("{body}\n{lead}\\end{{{}}}", &m[2]);
    let at = sel.head + body.chars().count();
    d.apply(
        ChangeSet::new(vec![Change::insert(sel.head, insert)]),
        Selection::single(Range::cursor(at)),
        EditKind::Other,
    );
    true
}

/// The `\begin{x}` / `\end{x}` spans (backslash to closing brace) that have no partner.
pub fn unmatched(d: &Doc) -> Vec<(usize, usize)> {
    tokens(d)
        .into_iter()
        .filter(|t| t.partner.is_none())
        .map(|t| (t.from - if t.begin { 7 } else { 5 }, t.to + 1))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(text: &str, c: Change) -> String {
        let mut d = Doc::new(text);
        let before = tokens(&d);
        let set = ChangeSet::new(vec![c]);
        d.apply(set.clone(), Selection::cursor(0), EditKind::Input);
        if let Some(p) = paired_edit(&before, &set, &d) {
            d.apply_merged(p);
        }
        d.text()
    }

    // environments.test.ts
    #[test]
    fn pairs_nested_environments_and_skips_comments() {
        let t = tokens(&Doc::new("\\begin{a}\\begin{b}\\end{b}\\end{a}"));
        assert_eq!(
            t.iter().map(|x| x.partner).collect::<Vec<_>>(),
            [Some(3), Some(2), Some(1), Some(0)]
        );
        assert_eq!(
            tokens(&Doc::new("% \\begin{a}\n\\begin{b}\n\\end{b}")).len(),
            2
        );
    }

    #[test]
    fn renaming_one_half_renames_the_other() {
        assert_eq!(
            edit(
                "\\begin{itemize}\n\\item x\n\\end{itemize}",
                Change::insert(14, "s")
            ),
            "\\begin{itemizes}\n\\item x\n\\end{itemizes}"
        );
        let t = "\\begin{a}\nx\n\\end{a}";
        let pos = t.rfind("a}").unwrap();
        assert_eq!(
            edit(
                t,
                Change {
                    from: pos,
                    to: pos + 1,
                    insert: "equation".into()
                }
            ),
            "\\begin{equation}\nx\n\\end{equation}"
        );
        assert_eq!(
            edit("\\begin{a}\n\\end{b}", Change::insert(7, "z")),
            "\\begin{za}\n\\end{b}"
        );
    }

    #[test]
    fn one_undo_reverts_both_halves() {
        let mut d = Doc::new("\\begin{a}\n\\end{a}");
        let before = tokens(&d);
        let set = ChangeSet::new(vec![Change::insert(8, "b")]);
        d.apply(set.clone(), Selection::cursor(9), EditKind::Input);
        d.apply_merged(paired_edit(&before, &set, &d).unwrap());
        assert_eq!(d.text(), "\\begin{ab}\n\\end{ab}");
        d.undo();
        assert_eq!(d.text(), "\\begin{a}\n\\end{a}");
    }

    #[test]
    fn enter_closes_an_open_environment_once() {
        let mut d = Doc::new("\\begin{itemize}");
        d.selection = Selection::cursor(15);
        assert!(close_environment(&mut d, Indent::Spaces(2)));
        assert_eq!(d.text(), "\\begin{itemize}\n  \n\\end{itemize}");
        assert_eq!(d.selection.main().head, 18);
        let mut d = Doc::new("\\begin{itemize}");
        d.selection = Selection::cursor(15);
        assert!(close_environment(&mut d, Indent::Tab));
        assert_eq!(d.text(), "\\begin{itemize}\n\t\n\\end{itemize}");
        let mut d = Doc::new("\\begin{itemize}\n\\end{itemize}");
        d.selection = Selection::cursor(15);
        assert!(!close_environment(&mut d, Indent::Spaces(2)));
        assert_eq!(
            unmatched(&Doc::new("\\begin{a}\n\\begin{b}\n\\end{b}")),
            [(0, 9)]
        );
    }
}
