//! Grammar checking through LanguageTool (grammar/text.ts and service.ts): LaTeX becomes plain
//! paragraphs with a map from every character back to the source, so a finding is underlined (and
//! fixed) where it is. Findings are kept per paragraph text, so an unchanged paragraph is asked
//! about once. Text goes only to the server the user chose in Preferences.

use super::prose::{State, line_words};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

pub const LOCAL_URL: &str = "http://localhost:8081";

#[derive(Clone, Debug, PartialEq)]
pub struct Paragraph {
    /// Plain text sent to LanguageTool.
    pub text: Vec<char>,
    /// Source position (chars) of each text character; None for ones made up here.
    pub map: Vec<Option<usize>>,
    pub from: usize,
    pub to: usize,
}

static BREAK_BEFORE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*\\(?:begin|end|item|part|chapter|(?:sub)*section|(?:sub)*paragraph|caption|title|author|date|maketitle|\\)(?:[^A-Za-z]|$)").unwrap()
});
static BREAK_AFTER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*\\(?:begin|end|part|chapter|(?:sub)*section|(?:sub)*paragraph|title|author|date|maketitle)(?:[^A-Za-z]|$)").unwrap()
});
static INLINE_MATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$[^$]*\$|\\\([^)]*\\\)").unwrap());
static COMMAND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\[A-Za-z]+\*?(?:\[[^\]]*\])*(?:\{[^{}]*\})*").unwrap());
static COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|[^\\])%.*$").unwrap());

fn punct(c: char) -> bool {
    ".,;:!?)]”’\"".contains(c)
}
fn open(c: char) -> bool {
    "([“‘\"".contains(c)
}

/// What remains of the source between two words: a space, punctuation that followed a command,
/// and X for inline maths.
fn gap_text(gap: &str) -> String {
    let math = INLINE_MATH.is_match(gap);
    let g = INLINE_MATH.replace_all(gap, " ");
    let g = COMMAND.replace_all(&g, " ");
    let g: String = g
        .chars()
        .map(|c| if "{}~".contains(c) { ' ' } else { c })
        .collect();
    let p: String = g.chars().filter(|c| punct(*c) || open(*c)).collect();
    // A placeholder keeps "Let $x$ be" a sentence; punctuation after it ("$x$,") stays attached.
    format!(
        "{}{p}{}",
        if math { " X" } else { "" },
        if g.contains(' ') || p.is_empty() {
            " "
        } else {
            ""
        }
    )
}

pub fn paragraphs(source: &str) -> Vec<Paragraph> {
    let mut out = Vec::new();
    let mut state = State::default();
    let mut cur: Option<Paragraph> = None;
    let mut offset = 0usize;
    let flush = |cur: &mut Option<Paragraph>, out: &mut Vec<Paragraph>| {
        if let Some(mut p) = cur.take()
            && p.text.iter().any(|c| c.is_alphabetic())
        {
            while p.text.last() == Some(&' ') {
                p.text.pop();
            }
            p.map.truncate(p.text.len());
            out.push(p);
        }
    };
    let push = |p: &mut Paragraph, s: &str, src: Option<usize>| {
        for (i, c) in s.chars().enumerate() {
            if src.is_none() && c == ' ' && (p.text.is_empty() || p.text.last() == Some(&' ')) {
                continue;
            }
            p.text.push(c);
            p.map.push(src.map(|s| s + i));
        }
    };
    for line in source.split('\n') {
        let start = offset;
        let n = line.chars().count();
        offset += n + 1;
        let math_at_start = state != State::default();
        let words = line_words(line, &mut state, start);
        let math_at_end = state != State::default();
        if line.trim().is_empty() {
            flush(&mut cur, &mut out);
            continue;
        }
        if BREAK_BEFORE.is_match(line) {
            flush(&mut cur, &mut out);
        }
        if words.is_empty() {
            if BREAK_AFTER.is_match(line) {
                flush(&mut cur, &mut out);
            }
            continue;
        }
        let p = cur.get_or_insert_with(|| Paragraph {
            text: Vec::new(),
            map: Vec::new(),
            from: words[0].from,
            to: words[0].to,
        });
        push(p, " ", None);
        // Comments are not text: gaps come from the line without one.
        let code: Vec<char> = COMMENT.replace(line, "$1").chars().collect();
        let slice = |a: usize, b: usize| {
            code[a.min(code.len())..b.min(code.len())]
                .iter()
                .collect::<String>()
        };
        let mut at = 0;
        for (k, w) in words.iter().enumerate() {
            let (ws, we) = (w.from - start, w.to - start);
            let gap = slice(at, ws);
            if k == 0 && math_at_start {
                push(p, " ", None);
            } else if !gap.is_empty() {
                push(p, &gap_text(&gap), None);
            }
            push(p, &w.text, Some(w.from));
            p.to = w.to;
            // Punctuation right behind the word, even past a closing brace: "\emph{word}."
            let mut q = we;
            while q < code.len() && (code[q] == '}' || punct(code[q])) {
                if code[q] != '}' {
                    push(p, &code[q].to_string(), Some(start + q));
                    p.to = start + q + 1;
                }
                q += 1;
            }
            at = q;
        }
        let tail = slice(at, code.len());
        let t = if math_at_end || tail.trim().is_empty() {
            " ".to_owned()
        } else {
            gap_text(&tail)
        };
        push(p, &t, None);
        if BREAK_AFTER.is_match(line) {
            flush(&mut cur, &mut out);
        }
    }
    flush(&mut cur, &mut out);
    out
}

/// Where a finding (chars `offset..offset+length` of the paragraph) is in the source, and whether
/// the source there is exactly the flagged text (so a replacement can be applied).
pub fn to_source(
    p: &Paragraph,
    offset: usize,
    length: usize,
    source: &[char],
) -> Option<(usize, usize, bool)> {
    let mut a = offset;
    let mut b = (offset + length).min(p.text.len()).checked_sub(1)?;
    while a <= b && p.map[a].is_none() {
        a += 1;
    }
    while b >= a && p.map[b].is_none() {
        if b == 0 {
            return None;
        }
        b -= 1;
    }
    if a > b {
        return None;
    }
    let (from, to) = (p.map[a]?, p.map[b]? + 1);
    let flagged: String = p.text[offset..(offset + length).min(p.text.len())]
        .iter()
        .collect();
    let src: String = source[from..to.min(source.len())].iter().collect();
    Some((from, to, src == flagged))
}

/// LanguageTool's code for a spell-check language.
pub fn lt_language(spell: &str) -> Option<&'static str> {
    Some(match spell {
        "en" => "en-US",
        "en-GB" => "en-GB",
        "de" => "de-DE",
        "de-CH" => "de-CH",
        "fr" => "fr",
        _ => return None,
    })
}

/// Rules that misfire on text reduced from LaTeX.
fn noisy(m: &lemmata_grammar::Match) -> bool {
    const NOISY: &[&str] = &[
        "WHITESPACE_RULE",
        "COMMA_PARENTHESIS_WHITESPACE",
        "UPPERCASE_SENTENCE_START",
        "EN_QUOTES",
        "UNPAIRED_BRACKETS",
        "EN_UNPAIRED_BRACKETS",
        "DOUBLE_PUNCTUATION",
    ];
    NOISY.contains(&m.rule_id.as_str())
        || m.category == "TYPOS"
        || m.rule_id.starts_with("MORFOLOGIK")
}

/// The service's state: what was found per paragraph text, what's asked, what's ignored.
#[derive(Default)]
pub struct Grammar {
    /// The server and language while checking is on.
    pub target: Option<(String, String)>,
    found: HashMap<String, Vec<lemmata_grammar::Match>>,
    pub asked: HashSet<String>,
    pub ignored: HashSet<String>,
    /// Why the last request failed.
    pub error: Option<String>,
    /// Bumped when findings change, so editors redraw their marks.
    pub generation: u64,
}

impl Grammar {
    /// Applies the settings; another server or language forgets what was found.
    pub fn configure(&mut self, mode: &str, custom_url: &str, spell_lang: &str) {
        let url = if mode == "custom" {
            custom_url.trim().to_owned()
        } else {
            LOCAL_URL.to_owned()
        };
        let target = (mode != "off")
            .then(|| lt_language(spell_lang))
            .flatten()
            .map(|l| (url, l.to_owned()));
        if target != self.target {
            self.target = target;
            self.found.clear();
            self.asked.clear();
            self.error = None;
            self.generation += 1;
        }
    }

    pub fn set_ignored(&mut self, ids: &[String]) {
        let set: HashSet<String> = ids.iter().cloned().collect();
        if set != self.ignored {
            self.ignored = set;
            self.generation += 1;
        }
    }

    /// Findings for a paragraph, without noisy or ignored rules; None when not checked yet.
    pub fn found(&self, text: &str) -> Option<Vec<&lemmata_grammar::Match>> {
        self.found.get(text).map(|v| {
            v.iter()
                .filter(|m| !noisy(m) && !self.ignored.contains(&m.rule_id))
                .collect()
        })
    }

    pub fn store(&mut self, text: String, matches: Vec<lemmata_grammar::Match>) {
        if self.found.len() >= 2000 {
            self.found.clear();
        }
        self.found.insert(text, matches);
        self.error = None;
        self.generation += 1;
    }
}

/// LanguageTool counts UTF-16 units; the text here counts chars.
pub fn utf16_to_char(text: &[char], units: usize) -> usize {
    let mut u = 0;
    for (i, c) in text.iter().enumerate() {
        if u >= units {
            return i;
        }
        u += c.len_utf16();
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(p: &Paragraph) -> String {
        p.text.iter().collect()
    }

    // grammar/text.test.ts
    #[test]
    fn plain_text_without_commands_maths_and_comments() {
        let p = &paragraphs(
            "We use \\emph{robust} methods~\\cite[p.~3]{knuth84} with $x^2$ \\% only, see \\cref{sec:a}. % not this",
        )[0];
        assert_eq!(text(p), "We use robust methods with X only, see.");
        let p =
            &paragraphs("This is shown in~\\cite{a}. Let $x$ be small, and\n\\textbf{bold}.")[0];
        assert_eq!(text(p), "This is shown in. Let X be small, and bold.");
        let src = "\\section{Intro}\nFirst line\nsecond line.\n\nNext paragraph.\n\\begin{equation}\na = b.\n\\end{equation}\nAfter it.\n\\item Listed";
        assert_eq!(
            paragraphs(src).iter().map(text).collect::<Vec<_>>(),
            [
                "Intro",
                "First line second line.",
                "Next paragraph.",
                "After it.",
                "Listed"
            ]
        );
    }

    #[test]
    fn maps_back_to_the_source() {
        let src = "We use \\emph{robust} methods~\\cite{k} with $x$ here.\nSecond line.";
        let chars: Vec<char> = src.chars().collect();
        for p in paragraphs(src) {
            for (i, m) in p.map.iter().enumerate() {
                if let Some(s) = m {
                    assert_eq!(chars[*s], p.text[i]);
                }
            }
        }
        let src = "Intro text.\n\nWe has \\emph{robust} methods~\\cite{k} here.";
        let chars: Vec<char> = src.chars().collect();
        let p = &paragraphs(src)[1];
        let t = text(p);
        let (a, b, ok) = to_source(p, t.find("We has").unwrap(), 6, &chars).unwrap();
        assert_eq!(
            (chars[a..b].iter().collect::<String>(), ok),
            ("We has".into(), true)
        );
        let i = t.find("has robust").unwrap();
        let (a, b, ok) = to_source(p, i, "has robust".len(), &chars).unwrap();
        assert_eq!(
            (chars[a..b].iter().collect::<String>(), ok),
            ("has \\emph{robust".into(), false)
        );
        let q = &paragraphs("Let $x$ be.")[0];
        assert_eq!(
            to_source(
                q,
                text(q).find('X').unwrap(),
                1,
                &"Let $x$ be.".chars().collect::<Vec<_>>()
            ),
            None
        );
    }
}
