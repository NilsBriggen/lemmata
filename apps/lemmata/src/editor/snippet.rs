//! CodeMirror's snippets: `${}` and `${name}` mark fields that Tab and Shift-Tab step through.
//! Fields with the same name are one field with several copies (`\begin{${name}}…\end{${name}}`),
//! edited with a cursor in each; leaving the snippet drops those extra cursors
//! (editor/snippetCursors.ts).

use super::buffer::{Change, ChangeSet, Doc, EditKind, Range, Selection};
use super::commands::Indent;

/// The text without markers, and each field's copies as char ranges in it, in field order.
pub fn parse(template: &str) -> (String, Vec<Vec<(usize, usize)>>) {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut len = 0;
    let mut names: Vec<Option<String>> = Vec::new();
    let mut fields: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$'
            && chars.get(i + 1) == Some(&'{')
            && let Some(close) = chars[i + 2..].iter().position(|c| *c == '}')
        {
            let inner: String = chars[i + 2..i + 2 + close].iter().collect();
            let n = inner.chars().count();
            let range = (len, len + n);
            // `${}` is always a field of its own; a name links its copies.
            let at = (!inner.is_empty())
                .then(|| names.iter().position(|x| x.as_deref() == Some(&inner)))
                .flatten();
            match at {
                Some(k) => fields[k].push(range),
                None => {
                    names.push((!inner.is_empty()).then(|| inner.clone()));
                    fields.push(vec![range]);
                }
            }
            out.push_str(&inner);
            len += n;
            i += 3 + close;
            continue;
        }
        out.push(chars[i]);
        len += 1;
        i += 1;
    }
    (out, fields)
}

/// CodeMirror's `snippet(template)` applied over `from..to`: later lines get the line's
/// indentation, and leading tabs become the indentation unit. Returns the fields.
pub fn insert(
    d: &mut Doc,
    from: usize,
    to: usize,
    template: &str,
    indent: Indent,
) -> Vec<Vec<(usize, usize)>> {
    let line = d.line(d.line_of(from));
    let lead: String = line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let text = template
        .split('\n')
        .enumerate()
        .map(|(i, l)| {
            let tabs = l.chars().take_while(|c| *c == '\t').count();
            let body: String = l.chars().skip(tabs).collect();
            let ind = if i == 0 { String::new() } else { lead.clone() };
            format!("{ind}{}{body}", indent.unit().repeat(tabs))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let (out, fields) = parse(&text);
    let fields: Vec<Vec<(usize, usize)>> = fields
        .into_iter()
        .map(|f| f.into_iter().map(|(a, b)| (from + a, from + b)).collect())
        .collect();
    let end = from + out.chars().count();
    let sel = match fields.first() {
        Some(f) => Selection::normalized(f.iter().map(|&(a, b)| Range::new(a, b)).collect(), 0),
        None => Selection::cursor(end),
    };
    d.apply(
        ChangeSet::new(vec![Change {
            from,
            to,
            insert: out,
        }]),
        sel,
        EditKind::Other,
    );
    fields
}

/// A snippet being filled in.
pub struct Active {
    pub fields: Vec<Vec<(usize, usize)>>,
    pub current: usize,
}

impl Active {
    /// Starts stepping through `fields`; a snippet with one field (or none) has nothing to step to.
    pub fn start(fields: Vec<Vec<(usize, usize)>>) -> Option<Self> {
        (fields.len() > 1).then_some(Self { fields, current: 0 })
    }

    /// Fields grow with what is typed at their edges.
    pub fn map(&mut self, journal: &[ChangeSet]) {
        for f in &mut self.fields {
            for r in f.iter_mut() {
                *r = (
                    Doc::map_through(journal, r.0, -1),
                    Doc::map_through(journal, r.1, 1),
                );
            }
        }
    }

    /// Every cursor and selection is inside some field.
    pub fn contains(&self, s: &Selection) -> bool {
        s.ranges.iter().all(|r| {
            self.fields
                .iter()
                .flatten()
                .any(|&(a, b)| r.from() >= a && r.to() <= b)
        })
    }

    /// Selects field `k` in all its copies.
    pub fn select(&self, k: usize) -> Selection {
        Selection::normalized(
            self.fields[k]
                .iter()
                .map(|&(a, b)| Range::new(a, b))
                .collect(),
            0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_fields_link_and_empty_ones_stand_alone() {
        let (text, fields) = parse("\\begin{${name}}\n${}\n\\end{${name}} ${}");
        assert_eq!(text, "\\begin{name}\n\n\\end{name} ");
        assert_eq!(
            fields,
            vec![vec![(7, 11), (19, 23)], vec![(13, 13)], vec![(25, 25)]]
        );
    }

    #[test]
    fn inserts_with_the_line_indentation_and_selects_the_first_field() {
        let mut d = Doc::new("  x");
        let fields = insert(
            &mut d,
            3,
            3,
            "\\begin{a}\n\t\\item ${}\n\\end{a}",
            Indent::Spaces(2),
        );
        assert_eq!(d.text(), "  x\\begin{a}\n    \\item \n  \\end{a}");
        assert_eq!(fields.len(), 1);
        assert_eq!(d.selection.main().head, fields[0][0].0);
        assert!(
            Active::start(fields).is_none(),
            "one field: nothing to step through"
        );
    }
}
