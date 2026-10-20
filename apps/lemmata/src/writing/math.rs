//! Is the cursor in maths, and which maths (editor/mathContext.ts and writing/math.ts)? Used to
//! rank completions and for the live preview. LaTeX maths can't span a blank line, so the scan
//! starts at the paragraph's first line and stays cheap. Positions are chars.

use crate::editor::buffer::Doc;
use regex::Regex;
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// Environments whose content is maths.
pub const MATH_ENVS: &[&str] = &[
    "equation",
    "equation*",
    "align",
    "align*",
    "alignat",
    "alignat*",
    "gather",
    "gather*",
    "multline",
    "multline*",
    "flalign",
    "flalign*",
    "eqnarray",
    "eqnarray*",
    "displaymath",
    "math",
    "split",
    "cases",
    "array",
    "matrix",
    "pmatrix",
    "bmatrix",
    "vmatrix",
    "Vmatrix",
    "Bmatrix",
    "smallmatrix",
];

/// Commands whose braced argument is ordinary text even inside maths.
static TEXT_COMMANDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\\(?:text|textrm|textbf|textit|textsf|texttt|mbox|intertext|label|ref|eqref|cref|cite)\b").unwrap()
});
static BEGIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\\begin\{([A-Za-z]+\*?)\}").unwrap());
static END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\\end\{([A-Za-z]+\*?)\}").unwrap());

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Inline,
    Display,
    Env,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Context {
    pub kind: Kind,
    /// How it was opened: `$`, `$$`, `\(`, `\[` or `begin`.
    pub open: &'static str,
    pub env: Option<String>,
    /// Start of the opening delimiter.
    pub from: usize,
    /// Where the maths itself starts, after the delimiter.
    pub content_from: usize,
}

const MAX_LINES: usize = 120;

/// The line with everything from an unescaped % on blanked out (offsets kept).
fn without_comment(line: &[char]) -> Vec<char> {
    let mut i = 0;
    while i < line.len() {
        if line[i] == '\\' {
            i += 2;
            continue;
        }
        if line[i] == '%' {
            let mut v = line[..i].to_vec();
            v.extend(std::iter::repeat_n(' ', line.len() - i));
            return v;
        }
        i += 1;
    }
    line.to_vec()
}

pub fn context_at(d: &Doc, pos: usize) -> Option<Context> {
    let n = d.line_of(pos);
    let line: Vec<char> = d.line(n).chars().collect();
    let col = pos - d.line_start(n);
    let typed = &line[..col.min(line.len())];
    // A cursor inside a comment is not in maths, whatever the line above says.
    if without_comment(typed) != typed {
        return None;
    }
    let mut first = n;
    while first > 0 && n - first < MAX_LINES && !d.line(first - 1).trim().is_empty() {
        first -= 1;
    }
    let base = d.line_start(first);
    let mut src: Vec<char> = Vec::new();
    for k in first..=n {
        let l: Vec<char> = d.line(k).chars().collect();
        let part = if k == n {
            &l[..col.min(l.len())]
        } else {
            &l[..]
        };
        src.extend(without_comment(part));
        if k < n {
            src.push('\n');
        }
    }

    let mut cur: Option<Context> = None;
    // Brace depth inside a \text{…} within maths; > 0 means "in text".
    let mut text_depth = 0usize;
    let mut pending_text = false;
    let open = |o: &'static str, i: usize, len: usize, env: Option<String>| Context {
        kind: match o {
            "$" | "\\(" => Kind::Inline,
            "begin" => Kind::Env,
            _ => Kind::Display,
        },
        open: o,
        env,
        from: base + i,
        content_from: base + i + len,
    };
    let mut i = 0;
    while i < src.len() {
        let c = src[i];
        if c == '\\' {
            let rest: String = src[i..(i + 40).min(src.len())].iter().collect();
            let next = src.get(i + 1).copied();
            let o = cur.as_ref().map(|c| c.open);
            if next == Some('(') && cur.is_none() {
                cur = Some(open("\\(", i, 2, None));
            } else if next == Some(')') && o == Some("\\(") {
                cur = None;
            } else if next == Some('[') && cur.is_none() {
                cur = Some(open("\\[", i, 2, None));
            } else if next == Some(']') && o == Some("\\[") {
                cur = None;
            } else {
                if let Some(b) = BEGIN.captures(&rest)
                    && MATH_ENVS.contains(&&b[1])
                    && cur.is_none()
                {
                    let len = b[0].chars().count();
                    cur = Some(open("begin", i, len, Some(b[1].to_owned())));
                    i += len;
                    continue;
                }
                if let Some(e) = END.captures(&rest)
                    && cur
                        .as_ref()
                        .is_some_and(|c| c.open == "begin" && c.env.as_deref() == Some(&e[1]))
                {
                    cur = None;
                    i += e[0].chars().count();
                    continue;
                }
                if cur.is_some()
                    && let Some(m) = TEXT_COMMANDS.find(&rest)
                {
                    pending_text = true;
                    i += m.as_str().chars().count();
                    continue;
                }
            }
            i += 2; // also skips \$, \%, \\ and friends
            continue;
        }
        if cur.is_some() && c == '{' {
            if text_depth > 0 {
                text_depth += 1;
            } else if pending_text {
                text_depth = 1;
                pending_text = false;
            }
        } else if cur.is_some() && c == '}' && text_depth > 0 {
            text_depth -= 1;
        } else if c != ' ' && c != '[' && pending_text {
            pending_text = false; // an optional argument may sit between, anything else cancels
        }
        if c == '$' {
            if src.get(i + 1) == Some(&'$') {
                match cur.as_ref().map(|c| c.open) {
                    Some("$$") => cur = None,
                    None => cur = Some(open("$$", i, 2, None)),
                    _ => {}
                }
                i += 2;
                continue;
            }
            match cur.as_ref().map(|c| c.open) {
                Some("$") => cur = None,
                None => cur = Some(open("$", i, 1, None)),
                _ => {}
            }
        }
        i += 1;
    }
    cur.filter(|_| text_depth == 0)
}

pub fn in_math(d: &Doc, pos: usize) -> bool {
    context_at(d, pos).is_some()
}

/// Where the maths that `ctx` opened ends: the start and end of its closing delimiter.
pub fn closing_of(d: &Doc, ctx: &Context, pos: usize) -> Option<(usize, usize)> {
    let closer: Vec<char> = match ctx.open {
        "$" => "$".into(),
        "$$" => "$$".into(),
        "\\(" => "\\)".into(),
        "\\[" => "\\]".into(),
        _ => format!("\\end{{{}}}", ctx.env.clone().unwrap_or_default()),
    }
    .chars()
    .collect();
    let at = d.line_of(pos);
    let mut last = at;
    while last + 1 < d.line_count() && last - at < MAX_LINES && !d.line(last + 1).trim().is_empty()
    {
        last += 1;
    }
    let tail: Vec<char> = d.slice(pos, d.line_end(last)).chars().collect();
    let mut i = 0;
    while i < tail.len() {
        if tail[i..].starts_with(&closer) && !(closer == ['$'] && tail.get(i + 1) == Some(&'$')) {
            return Some((pos + i, pos + i + closer.len()));
        }
        if tail[i] == '\\' {
            i += 1; // \$, \% and other escapes
        } else if tail[i] == '%' {
            match tail[i..].iter().position(|c| *c == '\n') {
                Some(nl) => i += nl,
                None => break,
            }
        }
        i += 1;
    }
    None
}

/// Environments rendered as plain maths, without the environment around them.
const MATH_ONLY: &[&str] = &["equation", "equation*", "displaymath", "math"];

fn katex_env(env: &str) -> Option<&'static str> {
    Some(match env {
        "align" | "align*" | "flalign" | "flalign*" | "alignat" | "alignat*" | "eqnarray"
        | "eqnarray*" => "aligned",
        "gather" | "gather*" | "multline" | "multline*" => "gathered",
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub from: usize,
    pub to: usize,
    pub tex: String,
    pub display: bool,
}

/// Drops `%` comments, which the renderer would choke on.
fn strip_comments(tex: &str) -> String {
    tex.split('\n')
        .map(|l| {
            let c: Vec<char> = l.chars().collect();
            let mut i = 0;
            while i < c.len() {
                if c[i] == '\\' {
                    i += 2;
                    continue;
                }
                if c[i] == '%' {
                    return c[..i].iter().collect();
                }
                i += 1;
            }
            l.to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

static LABELS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\(label|nonumber|notag)\b(\{[^}]*\})?").unwrap());
static TYPING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\[A-Za-z]*$").unwrap());

/// The maths around `pos`, closed or still being typed: inline, `$$`, `\[`, or a maths environment.
pub fn math_at(d: &Doc, pos: usize) -> Option<Span> {
    let ctx = context_at(d, pos)?;
    let closing = closing_of(d, &ctx, pos);
    // Unclosed while typing: what's there up to the end of the line.
    let line_end = d.line_end(d.line_of(pos));
    let content_to = closing.map_or(line_end, |c| c.0);
    let stop = content_to.max(ctx.content_from);
    let mut before = d.slice(ctx.content_from, pos.min(stop));
    let after = d.slice(pos.min(stop), stop);
    // A command still being typed (\alp|) isn't an error yet: leave it out.
    if !after.starts_with(|c: char| c.is_ascii_alphabetic()) {
        before = TYPING.replace(&before, "").into_owned();
    }
    let mut tex = strip_comments(&(before + &after));
    if ctx.open == "begin"
        && let Some(env) = &ctx.env
    {
        tex = LABELS.replace_all(&tex, "").into_owned();
        if let Some(w) = katex_env(env) {
            tex = format!("\\begin{{{w}}}{tex}\\end{{{w}}}");
        } else if !MATH_ONLY.contains(&env.as_str()) {
            tex = format!("\\begin{{{env}}}{tex}\\end{{{env}}}");
        }
    }
    Some(Span {
        from: ctx.from,
        to: closing.map_or(content_to, |c| c.1),
        tex,
        display: ctx.kind != Kind::Inline,
    })
}

static MACRO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\(?:re)?newcommand\*?\s*\{?(\\[A-Za-z]+)\}?\s*(?:\[(\d)\])?(?:\[[^\]]*\])?\s*|\\DeclareMathOperator(\*?)\s*\{(\\[A-Za-z]+)\}\s*").unwrap()
});

/// `\newcommand`, `\renewcommand` and `\DeclareMathOperator` from the preamble, as macros.
pub fn collect_macros(preamble: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let braced = |s: &str, i: usize| -> Option<(String, usize)> {
        if !s[i..].starts_with('{') {
            return None;
        }
        let mut depth = 0;
        for (k, c) in s[i..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((s[i + 1..i + k].to_owned(), i + k + 1));
                    }
                }
                _ => {}
            }
        }
        None
    };
    let mut at = 0;
    while let Some(m) = MACRO.captures_at(preamble, at) {
        let end = m.get(0).unwrap().end();
        let Some((body, next)) = braced(preamble, end) else {
            at = end.max(at + 1);
            continue;
        };
        if let Some(name) = m.get(1) {
            out.insert(name.as_str().to_owned(), body);
        } else if let Some(name) = m.get(4) {
            let star = m.get(3).is_some_and(|s| !s.as_str().is_empty());
            out.insert(
                name.as_str().to_owned(),
                format!("\\operatorname{}{{{body}}}", if star { "*" } else { "" }),
            );
        }
        at = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cursor sits at `|`.
    fn at(src: &str) -> (Doc, usize) {
        let pos = src.chars().position(|c| c == '|').unwrap();
        (Doc::new(&src.replacen('|', "", 1)), pos)
    }
    fn ctx(src: &str) -> Option<Context> {
        let (d, pos) = at(src);
        context_at(&d, pos)
    }

    // mathContext.test.ts
    #[test]
    fn knows_inline_display_and_environments() {
        assert_eq!(ctx("Let $x |+ y$ be").map(|c| c.kind), Some(Kind::Inline));
        assert_eq!(ctx("Let $|").map(|c| c.kind), Some(Kind::Inline));
        assert_eq!(ctx("Let $x$| be"), None);
        assert_eq!(ctx("Let |$x$ be"), None);
        assert_eq!(
            ctx("a $$ x |$$").map(|c| (c.kind, c.open)),
            Some((Kind::Display, "$$"))
        );
        assert_eq!(ctx("a \\( x| \\) b").map(|c| c.open), Some("\\("));
        assert_eq!(ctx("\\[\n x^2 |\n\\]").map(|c| c.open), Some("\\["));
        assert_eq!(ctx("\\[ x \\] a|"), None);
        assert_eq!(
            ctx("\\begin{align}\n a &= b\\\\\n c |= d").and_then(|c| c.env),
            Some("align".into())
        );
        assert_eq!(ctx("\\begin{align}\n a\n\\end{align}\n|"), None);
        assert_eq!(ctx("\\begin{itemize}\n \\item |x"), None);
    }

    #[test]
    fn ignores_escapes_comments_text_and_blank_lines() {
        assert_eq!(ctx("costs \\$5 and |more"), None);
        assert_eq!(ctx("% $ not maths\n text |"), None);
        assert_eq!(ctx("x % $ \n y |"), None);
        assert_eq!(ctx("$x \\text{if |it} y$"), None);
        assert!(ctx("$x \\text{if it} y |$").is_some());
        assert_eq!(ctx("$ unclosed\n\nnew paragraph |"), None);
        let (d, p) = at("$a |+ b$ rest");
        assert_eq!(closing_of(&d, &context_at(&d, p).unwrap(), p), Some((6, 7)));
        let (d, p) = at("\\begin{align}\n a |= b");
        assert_eq!(closing_of(&d, &context_at(&d, p).unwrap(), p), None);
    }

    // math.test.ts
    #[test]
    fn finds_the_maths_under_the_cursor() {
        let d = Doc::new("Energy $E = mc^2$ here, \\(a+b\\) too.");
        assert_eq!(
            math_at(&d, 10).map(|m| (m.tex, m.display)),
            Some(("E = mc^2".into(), false))
        );
        assert_eq!(math_at(&d, 27).map(|m| m.tex), Some("a+b".into()));
        assert_eq!(math_at(&d, 2), None);
        let d = Doc::new(
            "Text\n\\begin{align}\\label{eq:x}\n  a &= b \\\\\n  c &= d\n\\end{align}\nafter",
        );
        let m = math_at(&d, d.line_start(2) + 3).unwrap();
        assert!(m.display && m.tex.contains("\\begin{aligned}") && !m.tex.contains("\\label"));
        assert_eq!(math_at(&d, d.line_start(5) + 2), None);
        let d = Doc::new("\\[\n x^2\n\\]");
        assert_eq!(
            math_at(&d, d.line_start(1) + 1).map(|m| m.tex),
            Some("\n x^2\n".into())
        );
    }

    #[test]
    fn previews_what_is_typed_so_far() {
        assert_eq!(
            math_at(&Doc::new("Let $x^{2} + "), 13).map(|m| m.tex),
            Some("x^{2} + ".into())
        );
        assert_eq!(
            math_at(&Doc::new("Let $x = \\al"), 12).map(|m| m.tex),
            Some("x = ".into())
        );
        assert_eq!(
            math_at(&Doc::new("$x = \\alpha$"), 9).map(|m| m.tex),
            Some("x = \\alpha".into())
        );
        assert_eq!(math_at(&Doc::new("$a % $ oops\n$"), 8), None);
        assert_eq!(
            math_at(&Doc::new("$a % $ oops\n$"), 3).map(|m| m.tex),
            Some("a \n".into())
        );
        let d = Doc::new("\\begin{eqnarray}\n a &=& b % note\n\\end{eqnarray}");
        let m = math_at(&d, d.line_start(1) + 2).unwrap();
        assert!(m.tex.contains("aligned") && !m.tex.contains("note"));
    }

    #[test]
    fn collects_preamble_macros() {
        let m = collect_macros(
            "\\newcommand{\\R}{\\mathbb{R}}\n\\newcommand\\norm[1]{\\left\\lVert #1 \\right\\rVert}\n\\DeclareMathOperator*{\\argmax}{arg\\,max}",
        );
        assert_eq!(m["\\R"], "\\mathbb{R}");
        assert_eq!(m["\\norm"], "\\left\\lVert #1 \\right\\rVert");
        assert_eq!(m["\\argmax"], "\\operatorname*{arg\\,max}");
    }
}
