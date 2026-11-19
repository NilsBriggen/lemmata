//! Finds the prose words in LaTeX source: text the reader sees, without
//! commands, their technical arguments, comments or maths. Used for word
//! counts; the editor's spell checker follows the same rules.

/// Environments whose content is maths.
const MATH_ENVS: &[&str] = &[
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
];

/// Environments whose content isn't prose.
const VERBATIM_ENVS: &[&str] = &[
    "verbatim",
    "verbatim*",
    "lstlisting",
    "minted",
    "comment",
    "tikzpicture",
];

/// Commands whose braced arguments are names, keys or paths, not prose.
const NON_PROSE_ARGS: &[&str] = &[
    "label",
    "ref",
    "eqref",
    "cref",
    "Cref",
    "autoref",
    "pageref",
    "nameref",
    "cite",
    "citep",
    "citet",
    "parencite",
    "textcite",
    "autocite",
    "footcite",
    "citeauthor",
    "citeyear",
    "nocite",
    "input",
    "include",
    "subfile",
    "usepackage",
    "RequirePackage",
    "documentclass",
    "begin",
    "end",
    "includegraphics",
    "url",
    "href",
    "bibliography",
    "addbibresource",
    "bibliographystyle",
    "newcommand",
    "renewcommand",
    "providecommand",
    "DeclareMathOperator",
    "newenvironment",
    "renewenvironment",
    "setlength",
    "addtolength",
    "hspace",
    "vspace",
    "graphicspath",
    "color",
    "textcolor",
    "definecolor",
    "colorbox",
    "hypersetup",
    "pagestyle",
    "thispagestyle",
    "setcounter",
    "addtocounter",
    "usetheme",
    "usecolortheme",
    "setmainfont",
    "setsansfont",
    "setmonofont",
    "geometry",
    "newpage",
    "clearpage",
    "printbibliography",
    "bibitem",
    "lstinline",
    "texttt",
];

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct State {
    /// Inside a math or verbatim environment, with its name.
    env: Option<String>,
    /// Inside `$$ … $$` or `\[ … \]` that spans lines.
    display: Option<&'static str>,
    /// Inside an `otherlanguage` environment, with its name.
    foreign: Option<String>,
}

/// Byte ranges of prose words in one line, updating `state` for
/// environments that span lines.
pub fn words(line: &str, state: &mut State) -> Vec<(usize, usize)> {
    scan(line, state)
        .into_iter()
        .map(|(a, b, _)| (a, b))
        .collect()
}

/// The words of `words` in the document's own language: those in `\foreignlanguage{…}{…}` or an
/// `otherlanguage` environment are left out, so the spell checker doesn't mark them.
pub fn own_language_words(line: &str, state: &mut State) -> Vec<(usize, usize)> {
    scan(line, state)
        .into_iter()
        .filter(|w| !w.2)
        .map(|(a, b, _)| (a, b))
        .collect()
}

/// Words with whether they are in another language.
fn scan(line: &str, state: &mut State) -> Vec<(usize, usize, bool)> {
    let code = strip_comment(line);
    let b = code.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    // The end of a `\foreignlanguage` text on this line.
    let mut foreign_until = 0;

    while i < b.len() {
        if let Some(env) = &state.env {
            let end = format!("\\end{{{env}}}");
            match code[i..].find(&end) {
                Some(p) => {
                    i += p + end.len();
                    state.env = None;
                    continue;
                }
                None => return out,
            }
        }
        if let Some(close) = state.display {
            match code[i..].find(close) {
                Some(p) => {
                    i += p + close.len();
                    state.display = None;
                    continue;
                }
                None => return out,
            }
        }
        match b[i] {
            b'$' => {
                if code[i..].starts_with("$$") {
                    state.display = Some("$$");
                    i += 2;
                } else {
                    // Inline maths ends on this line in practice.
                    i = code[i + 1..].find('$').map_or(b.len(), |p| i + 1 + p + 1);
                }
            }
            b'\\' => {
                let rest = &code[i + 1..];
                if rest.starts_with('[') {
                    state.display = Some("\\]");
                    i += 2;
                    continue;
                }
                if rest.starts_with('(') {
                    i = rest.find("\\)").map_or(b.len(), |p| i + 1 + p + 2);
                    continue;
                }
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphabetic())
                    .collect();
                if name.is_empty() {
                    // Escaped character like \% or \&.
                    i += 1 + rest.chars().next().map_or(0, char::len_utf8);
                    continue;
                }
                i += 1 + name.len();
                if name == "verb" {
                    // Any character delimits the code, unless no closing one is on the line.
                    if code[i..].starts_with('*') {
                        i += 1;
                    }
                    if let Some(d) = code[i..].chars().next() {
                        let body = i + d.len_utf8();
                        i = code[body..]
                            .find(d)
                            .map_or(b.len(), |p| body + p + d.len_utf8());
                    }
                    continue;
                }
                if name == "begin" {
                    if let Some((env, len)) = braced_at(code, i) {
                        i += len;
                        if MATH_ENVS.contains(&env) || VERBATIM_ENVS.contains(&env) {
                            state.env = Some(env.to_string());
                        } else if env == "otherlanguage" || env == "otherlanguage*" {
                            state.foreign = Some(env.to_string());
                            // The language's name isn't prose.
                            if let Some((_, len)) = braced_at(code, skip_ws(code, i)) {
                                i = skip_ws(code, i) + len;
                            }
                        }
                    }
                    continue;
                }
                if name == "end"
                    && let Some((env, _)) = braced_at(code, skip_ws(code, i))
                    && state.foreign.as_deref() == Some(env)
                {
                    state.foreign = None;
                }
                if name == "foreignlanguage" {
                    // [options]{language}{text}: the language is skipped, the text is marked.
                    let mut at = skip_ws(code, i);
                    if code[at..].starts_with('[') {
                        at = code[at..].find(']').map_or(b.len(), |p| at + p + 1);
                    }
                    if let Some((_, len)) = braced_at(code, skip_ws(code, at)) {
                        i = skip_ws(code, at) + len;
                        let text = skip_ws(code, i);
                        // shortcut: a text that goes on past this line is foreign to the line's end only.
                        foreign_until =
                            braced_at(code, text).map_or(b.len(), |(_, len)| text + len);
                    }
                    continue;
                }
                if NON_PROSE_ARGS.contains(&name.as_str()) {
                    // Skip a star, optional arguments and up to two braced ones.
                    if code[i..].starts_with('*') {
                        i += 1;
                    }
                    for _ in 0..2 {
                        let skipped = skip_ws(code, i);
                        if code[skipped..].starts_with('[') {
                            i = code[skipped..]
                                .find(']')
                                .map_or(b.len(), |p| skipped + p + 1);
                        }
                        let skipped = skip_ws(code, i);
                        match braced_at(code, skipped) {
                            Some((_, len)) => i = skipped + len,
                            None => break,
                        }
                    }
                }
            }
            c if c.is_ascii_alphabetic() || c >= 0x80 => {
                let start = i;
                let mut end = i;
                for (off, ch) in code[i..].char_indices() {
                    if ch.is_alphabetic() || (ch == '\'' || ch == '-') && off > 0 {
                        end = i + off + ch.len_utf8();
                    } else {
                        break;
                    }
                }
                if end == start {
                    // A non-letter multibyte character.
                    i += code[i..].chars().next().map_or(1, char::len_utf8);
                    continue;
                }
                let word = code[start..end].trim_end_matches(['\'', '-']);
                if !word.is_empty() {
                    let foreign = state.foreign.is_some() || start < foreign_until;
                    out.push((start, start + word.len(), foreign));
                }
                i = end;
            }
            _ => i += 1,
        }
    }
    out
}

fn skip_ws(s: &str, i: usize) -> usize {
    i + s[i..].len() - s[i..].trim_start().len()
}

/// `{...}` at byte `i`: (contents, total length including braces).
fn braced_at(s: &str, i: usize) -> Option<(&str, usize)> {
    let rest = s.get(i..)?.strip_prefix('{')?;
    let mut depth = 1;
    for (p, c) in rest.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&rest[..p], p + 2));
                }
            }
            _ => {}
        }
    }
    None
}

fn strip_comment(line: &str) -> &str {
    let b = line.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        if c == b'%' && (i == 0 || b[i - 1] != b'\\') {
            return &line[..i];
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(line: &str, state: &mut State) -> Vec<String> {
        words(line, state)
            .into_iter()
            .map(|(a, b)| line[a..b].to_string())
            .collect()
    }

    #[test]
    fn skips_commands_maths_keys_and_comments() {
        let mut s = State::default();
        assert_eq!(
            texts(
                r"We use \emph{robust} methods~\cite[p.~3]{knuth84} with $x^2$ \% only, see \cref{sec:a}. % not this",
                &mut s
            ),
            ["We", "use", "robust", "methods", "with", "only", "see"]
        );
        assert_eq!(
            texts(r"\section{Über die Größe} don't stop-words", &mut s),
            ["Über", "die", "Größe", "don't", "stop-words"]
        );
        assert_eq!(
            texts(
                r"\includegraphics[width=\linewidth]{figures/plot} Caption",
                &mut s
            ),
            ["Caption"]
        );
    }

    #[test]
    fn other_languages_count_but_are_not_spell_checked() {
        let mut s = State::default();
        let own = |line: &str, s: &mut State| -> Vec<String> {
            own_language_words(line, s)
                .into_iter()
                .map(|(a, b)| line[a..b].to_string())
                .collect()
        };
        let line = r"As \foreignlanguage{ngerman}{man sagt} here";
        assert_eq!(texts(line, &mut s), ["As", "man", "sagt", "here"]);
        assert_eq!(own(line, &mut s), ["As", "here"]);
        assert_eq!(
            own(
                r"\foreignlanguage[variant=swiss]{german}{Grüezi \emph{mitenand}}",
                &mut s
            ),
            Vec::<String>::new()
        );
        assert_eq!(
            own(r"Text \begin{otherlanguage}{french} Bonjour", &mut s),
            ["Text"]
        );
        assert_eq!(
            texts(
                r"\begin{otherlanguage*}{french} Bonjour",
                &mut State::default()
            ),
            ["Bonjour"]
        );
        assert!(own("tout le monde", &mut s).is_empty());
        assert_eq!(own(r"au revoir \end{otherlanguage} back", &mut s), ["back"]);
        assert_eq!(own("home", &mut s), ["home"]);
    }

    #[test]
    fn maths_and_verbatim_span_lines() {
        let mut s = State::default();
        assert_eq!(texts(r"Before \begin{equation}", &mut s), ["Before"]);
        assert!(texts(r"  E = mc^2 \text{energy}", &mut s).is_empty());
        assert_eq!(texts(r"\end{equation} after", &mut s), ["after"]);
        assert_eq!(texts(r"Then \[", &mut s), ["Then"]);
        assert!(texts(r"a + b", &mut s).is_empty());
        assert_eq!(texts(r"\] done", &mut s), ["done"]);
        assert!(texts(r"\begin{verbatim} code here", &mut s).is_empty());
        assert_eq!(texts(r"more code \end{verbatim} text", &mut s), ["text"]);
    }

    /// Shared with the editor's spell checker.
    #[test]
    fn matches_shared_cases() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/prose/cases.json"
        );
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(!cases.is_empty());
        for c in cases {
            let mut s = State::default();
            let got: Vec<String> = c["tex"]
                .as_str()
                .unwrap()
                .split('\n')
                .flat_map(|line| texts(line, &mut s))
                .collect();
            let want: Vec<&str> = c["words"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| w.as_str().unwrap())
                .collect();
            assert_eq!(got, want, "{}", c["name"]);
        }
    }
}
