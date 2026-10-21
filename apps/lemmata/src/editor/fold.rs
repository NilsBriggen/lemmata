//! Folding of sections, environments and comment blocks (editor/folding.ts). A fold hides the
//! lines after its first one; it is kept as the character range from the end of that first line
//! to the end of the last hidden line, so it follows edits like CodeMirror's.

use super::buffer::Doc;
use regex::Regex;
use std::sync::LazyLock;

static HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*\\(part|chapter|section|subsection|subsubsection|paragraph)\*?\s*[\[{]")
        .unwrap()
});
static BEGIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\\begin\{([^{}]+)\}").unwrap());
static TOKEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\(begin|end)\{([^{}]+)\}").unwrap());
static END_DOCUMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\\end\{document\}").unwrap());

fn level(cmd: &str) -> usize {
    [
        "part",
        "chapter",
        "section",
        "subsection",
        "subsubsection",
        "paragraph",
    ]
    .iter()
    .position(|c| *c == cmd)
    .unwrap_or(5)
}

fn is_comment(t: &str) -> bool {
    t.trim_start().starts_with('%')
}

/// The lines (0-based, inclusive) a fold starting on line `n` hides, if it can fold.
pub fn fold_lines(d: &Doc, n: usize) -> Option<(usize, usize)> {
    let lines = d.line_count();
    let text = d.line(n);
    if let Some(h) = HEADING.captures(&text) {
        let lv = level(&h[1]);
        let mut end = lines - 1;
        for k in n + 1..lines {
            let t = d.line(k);
            let stop = HEADING.captures(&t).is_some_and(|x| level(&x[1]) <= lv)
                || END_DOCUMENT.is_match(&t);
            if stop {
                end = k - 1;
                break;
            }
        }
        // Trailing blank lines stay out, so the fold ends at real content.
        while end > n && d.line(end).trim().is_empty() {
            end -= 1;
        }
        return (end > n).then_some((n + 1, end));
    }
    if let Some(b) = BEGIN.captures(&text)
        && &b[1] != "document"
    {
        let name = b[1].to_owned();
        let mut depth = 0i32;
        for k in n..lines {
            let t = d.line(k);
            for m in TOKEN.captures_iter(&t) {
                if m[2] != name {
                    continue;
                }
                depth += if &m[1] == "begin" { 1 } else { -1 };
                if depth == 0 {
                    // The \end line stays visible.
                    return (k > n + 1).then_some((n + 1, k - 1));
                }
            }
        }
        return None;
    }
    if is_comment(&text) && (n == 0 || !is_comment(&d.line(n - 1))) {
        let mut last = n;
        while last + 1 < lines && is_comment(&d.line(last + 1)) {
            last += 1;
        }
        return (last > n).then_some((n + 1, last));
    }
    None
}

/// Folded ranges of one file, as characters `(end of the first line, end of the last hidden line)`.
#[derive(Default, Clone)]
pub struct Folds(pub Vec<(usize, usize)>);

impl Folds {
    /// Moves the folds with the text; a fold whose range collapsed is dropped.
    pub fn map(&mut self, journal: &[super::buffer::ChangeSet]) {
        for f in &mut self.0 {
            *f = (
                Doc::map_through(journal, f.0, 1),
                Doc::map_through(journal, f.1, -1),
            );
        }
        self.0.retain(|f| f.1 > f.0);
    }

    /// Folds the block starting on line `n`; false when it can't fold or already is.
    pub fn fold(&mut self, d: &Doc, n: usize) -> bool {
        let Some((_, last)) = fold_lines(d, n) else {
            return false;
        };
        let range = (d.line_end(n), d.line_end(last));
        if self.0.iter().any(|f| f.0 == range.0) {
            return false;
        }
        self.0.push(range);
        self.0.sort();
        true
    }

    /// Unfolds what starts on line `n`; false when nothing does.
    pub fn unfold_line(&mut self, d: &Doc, n: usize) -> bool {
        let at = d.line_end(n);
        let before = self.0.len();
        self.0.retain(|f| f.0 != at);
        self.0.len() != before
    }

    /// The fold starting at the end of line `n`.
    pub fn starting(&self, d: &Doc, n: usize) -> Option<(usize, usize)> {
        let at = d.line_end(n);
        self.0.iter().copied().find(|f| f.0 == at)
    }

    /// Opens every fold that hides `pos`, so a cursor put there (a search hit, go to line) shows.
    pub fn reveal(&mut self, pos: usize) {
        self.0.retain(|f| !(pos > f.0 && pos <= f.1));
    }

    /// For each line, whether a fold hides it.
    pub fn hidden(&self, d: &Doc) -> Vec<bool> {
        let mut h = vec![false; d.line_count()];
        for &(from, to) in &self.0 {
            let (a, b) = (d.line_of(from) + 1, d.line_of(to));
            for x in h.iter_mut().take(b + 1).skip(a) {
                *x = true;
            }
        }
        h
    }

    /// The first lines of the folds, 1-based, for the session.
    pub fn lines(&self, d: &Doc) -> Vec<usize> {
        self.0.iter().map(|f| d.line_of(f.0) + 1).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hidden(text: &str, line: usize) -> Option<String> {
        let d = Doc::new(text);
        fold_lines(&d, line - 1)
            .map(|(a, b)| (a..=b).map(|n| d.line(n)).collect::<Vec<_>>().join("\n"))
    }

    // folding.test.ts
    #[test]
    fn folds_sections_environments_and_comments() {
        let doc = "\\section{A}\ntext\n\\subsection{B}\nmore\n\n\\section{C}\nend";
        assert_eq!(
            hidden(doc, 1).as_deref(),
            Some("text\n\\subsection{B}\nmore")
        );
        assert_eq!(hidden(doc, 3).as_deref(), Some("more"));
        let nested = "\\begin{a}\n\\begin{a}\nx\n\\end{a}\n\\end{a}";
        assert_eq!(
            hidden(nested, 1).as_deref(),
            Some("\\begin{a}\nx\n\\end{a}")
        );
        assert_eq!(
            hidden("% a\n% b\n% c\ntext", 1).as_deref(),
            Some("% b\n% c")
        );
        assert_eq!(hidden("\\begin{document}\nx\n\\end{document}", 1), None);
    }

    #[test]
    fn folds_follow_edits_and_hide_lines() {
        let mut d = Doc::new("x\n\\section{A}\na\nb\n\\section{B}");
        let mut f = Folds::default();
        assert!(f.fold(&d, 1));
        assert_eq!(f.hidden(&d), [false, false, true, true, false]);
        // A line typed above moves the fold down.
        let journal_from = d.journal.len();
        d.apply(
            super::super::buffer::ChangeSet::new(vec![super::super::buffer::Change::insert(
                0, "new\n",
            )]),
            super::super::buffer::Selection::cursor(0),
            super::super::buffer::EditKind::Other,
        );
        f.map(&d.journal[journal_from..]);
        assert_eq!(f.lines(&d), [3]);
        assert!(f.unfold_line(&d, 2));
        assert!(f.0.is_empty());
    }
}
