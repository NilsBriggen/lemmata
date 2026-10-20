//! LaTeX highlighting: a port of CodeMirror's legacy `stex` stream mode (@codemirror/legacy-modes
//! mode/stex.js), token for token, so the colours fall where they did. The state at the start of
//! each line is kept, so a change re-tokenizes only from its line until the states agree again.

/// The token kinds stex returns, mapped to the six colours Editor.svelte's HighlightStyle gave them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    /// `tag`: commands. Accent.
    Tag,
    /// `keyword`: maths delimiters. Accent.
    Keyword,
    /// Muted, italic.
    Comment,
    /// `atom`, `string`, `variableName.special`: arguments of \begin, \ref…, maths letters.
    Atom,
    /// Muted.
    Bracket,
    /// `--ok`.
    Number,
    /// `error` (an unmatched closing bracket): `--error`.
    Error,
    /// `builtin`, unstyled in the app.
    Builtin,
}

#[derive(Clone, Debug, PartialEq)]
struct Plug {
    /// None for DEFAULT.
    name: Option<&'static str>,
    bracket_no: usize,
}

impl Plug {
    fn styles(&self) -> &'static [Option<Token>] {
        match self.name {
            Some("importmodule") => &[Some(Token::Atom), Some(Token::Builtin)],
            Some("documentclass") => &[None, Some(Token::Atom)],
            Some(_) => &[Some(Token::Atom)],
            None => &[],
        }
    }
    fn style_identifier(&self) -> Option<Token> {
        self.bracket_no
            .checked_sub(1)
            .and_then(|i| self.styles().get(i).copied().flatten())
    }
}

const PLUGINS: [&str; 12] = [
    "importmodule",
    "documentclass",
    "usepackage",
    "begin",
    "end",
    "label",
    "ref",
    "eqref",
    "cite",
    "bibitem",
    "Bibitem",
    "RBibitem",
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Normal,
    BeginParams,
    /// In maths until this closes it.
    Math(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    mode: Mode,
    cmd: Vec<Plug>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            mode: Mode::Normal,
            cmd: Vec::new(),
        }
    }
}

struct Stream<'a> {
    s: &'a [char],
    pos: usize,
    start: usize,
}

impl Stream<'_> {
    fn eol(&self) -> bool {
        self.pos >= self.s.len()
    }
    fn peek(&self) -> Option<char> {
        self.s.get(self.pos).copied()
    }
    fn next(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }
    fn eat_while(&mut self, f: impl Fn(char) -> bool) {
        while self.peek().is_some_and(&f) {
            self.pos += 1;
        }
    }
    fn eat_space(&mut self) -> bool {
        let start = self.pos;
        self.eat_while(|c| c.is_whitespace());
        self.pos > start
    }
    fn starts_with(&self, lit: &str) -> bool {
        let n = lit.chars().count();
        self.s.len() >= self.pos + n && lit.chars().zip(&self.s[self.pos..]).all(|(a, b)| a == *b)
    }
    fn match_lit(&mut self, lit: &str) -> bool {
        if self.starts_with(lit) {
            self.pos += lit.chars().count();
            true
        } else {
            false
        }
    }
    /// `\` followed by one or more chars matching `f`.
    fn match_cmd(&mut self, f: impl Fn(char) -> bool) -> bool {
        if self.peek() == Some('\\') && self.s.get(self.pos + 1).is_some_and(|c| f(*c)) {
            self.pos += 1;
            self.eat_while(&f);
            true
        } else {
            false
        }
    }
    /// `\` followed by one char of `set`.
    fn match_escape(&mut self, set: &str) -> bool {
        if self.peek() == Some('\\') && self.s.get(self.pos + 1).is_some_and(|c| set.contains(*c)) {
            self.pos += 2;
            true
        } else {
            false
        }
    }
    fn current(&self) -> String {
        self.s[self.start..self.pos].iter().collect()
    }
}

/// `[a-zA-Z@\xc0-῿⁠-￿]`
fn cmd_char(c: char) -> bool {
    c.is_ascii_alphabetic()
        || c == '@'
        || ('\u{c0}'..='\u{1fff}').contains(&c)
        || ('\u{2060}'..='\u{ffff}').contains(&c)
}

fn normal(s: &mut Stream, st: &mut State) -> Option<Token> {
    if s.match_cmd(cmd_char) {
        let name = s.current()[1..].to_owned();
        let plug = Plug {
            name: PLUGINS.iter().find(|p| **p == name).copied(),
            bracket_no: 0,
        };
        st.cmd.push(plug);
        st.mode = Mode::BeginParams;
        return Some(Token::Tag);
    }
    if s.match_escape("$&%#{}_") || s.match_escape(",;!/\\") {
        return Some(Token::Tag);
    }
    for (open, close) in [("\\[", "\\]"), ("\\(", "\\)"), ("$$", "$$"), ("$", "$")] {
        if s.match_lit(open) {
            st.mode = Mode::Math(close);
            return Some(Token::Keyword);
        }
    }
    let ch = s.next()?;
    match ch {
        '%' => {
            s.pos = s.s.len();
            Some(Token::Comment)
        }
        '}' | ']' => {
            if st.cmd.is_empty() {
                return Some(Token::Error);
            }
            st.mode = Mode::BeginParams;
            Some(Token::Bracket)
        }
        '{' | '[' => {
            st.cmd.push(Plug {
                name: None,
                bracket_no: 0,
            });
            Some(Token::Bracket)
        }
        c if c.is_ascii_digit() => {
            s.eat_while(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '%');
            Some(Token::Atom)
        }
        _ => {
            s.eat_while(|c| c.is_alphanumeric() || c == '_' || c == '-');
            // The innermost command that isn't DEFAULT styles its argument.
            st.cmd
                .iter()
                .rev()
                .find(|p| p.name.is_some())
                .and_then(Plug::style_identifier)
        }
    }
}

fn in_math(s: &mut Stream, st: &mut State, end: &'static str) -> Option<Token> {
    if s.eat_space() {
        return None;
    }
    if s.match_lit(end) {
        st.mode = Mode::Normal;
        return Some(Token::Keyword);
    }
    if s.match_cmd(|c| c.is_ascii_alphabetic() || c == '@') {
        return Some(Token::Tag);
    }
    if s.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
        s.eat_while(|c| c.is_ascii_alphabetic());
        return Some(Token::Atom);
    }
    if s.match_escape("$&%#{}_") || s.match_escape(",;!/") {
        return Some(Token::Tag);
    }
    if s.peek().is_some_and(|c| "^_&".contains(c)) {
        s.pos += 1;
        return Some(Token::Tag);
    }
    if s.peek().is_some_and(|c| "+-<>|=,/@!*:;'\"`~#?".contains(c)) {
        s.pos += 1;
        return None;
    }
    // (\d+\.\d*|\d*\.\d+|\d+)
    let digits_from = s.pos;
    s.eat_while(|c| c.is_ascii_digit());
    let int = s.pos > digits_from;
    if s.peek() == Some('.') && (int || s.s.get(s.pos + 1).is_some_and(|c| c.is_ascii_digit())) {
        s.pos += 1;
        s.eat_while(|c| c.is_ascii_digit());
        return Some(Token::Number);
    }
    if int {
        return Some(Token::Number);
    }
    let ch = s.next()?;
    if "{}[]()".contains(ch) {
        return Some(Token::Bracket);
    }
    if ch == '%' {
        s.pos = s.s.len();
        return Some(Token::Comment);
    }
    Some(Token::Error)
}

fn begin_params(s: &mut Stream, st: &mut State) -> Option<Token> {
    match s.peek() {
        Some(c @ ('{' | '[')) => {
            let _ = c;
            if let Some(p) = st.cmd.last_mut() {
                p.bracket_no += 1;
            }
            s.pos += 1;
            st.mode = Mode::Normal;
            Some(Token::Bracket)
        }
        Some(' ' | '\t' | '\r') => {
            s.pos += 1;
            None
        }
        _ => {
            st.mode = Mode::Normal;
            st.cmd.pop();
            normal(s, st)
        }
    }
}

/// The tokens of one line as `(from, to, token)` in char columns; `state` goes from the line's start
/// state to the next line's.
pub fn line(text: &str, state: &mut State) -> Vec<(usize, usize, Token)> {
    let chars: Vec<char> = text.chars().collect();
    // stex's blankLine: an empty line ends maths and open commands.
    if chars.iter().all(|c| c.is_whitespace()) && chars.is_empty() {
        *state = State::default();
        return Vec::new();
    }
    let mut s = Stream {
        s: &chars,
        pos: 0,
        start: 0,
    };
    let mut out: Vec<(usize, usize, Token)> = Vec::new();
    while !s.eol() {
        s.start = s.pos;
        let tok = match state.mode {
            Mode::Normal => normal(&mut s, state),
            Mode::BeginParams => begin_params(&mut s, state),
            Mode::Math(end) => in_math(&mut s, state, end),
        };
        if s.pos == s.start {
            // A mode that consumed nothing must not loop; CodeMirror throws in that case.
            s.pos += 1;
        }
        if let Some(t) = tok {
            match out.last_mut() {
                Some(last) if last.1 == s.start && last.2 == t => last.1 = s.pos,
                _ => out.push((s.start, s.pos, t)),
            }
        }
    }
    out
}

/// Per-line start states of a document, recomputed lazily from the first changed line.
#[derive(Default)]
pub struct Highlighter {
    /// `states[n]` is the state at the start of line n; valid for n < states.len().
    states: Vec<State>,
}

impl Highlighter {
    /// Forgets everything from line `n` on.
    pub fn invalidate_from(&mut self, n: usize) {
        self.states.truncate(n + 1);
    }

    /// The tokens of line `n`, tokenizing earlier lines first if their states aren't known.
    pub fn tokens(
        &mut self,
        n: usize,
        line_text: impl Fn(usize) -> String,
    ) -> Vec<(usize, usize, Token)> {
        if self.states.is_empty() {
            self.states.push(State::default());
        }
        while self.states.len() <= n {
            let k = self.states.len() - 1;
            let mut st = self.states[k].clone();
            line(&line_text(k), &mut st);
            self.states.push(st);
        }
        let mut st = self.states[n].clone();
        line(&line_text(n), &mut st)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(text: &str) -> Vec<(String, Token)> {
        let mut st = State::default();
        let chars: Vec<char> = text.chars().collect();
        line(text, &mut st)
            .into_iter()
            .map(|(a, b, t)| (chars[a..b].iter().collect(), t))
            .collect()
    }

    #[test]
    fn commands_their_arguments_and_brackets() {
        // As in stex, only the first argument of \usepackage takes the atom colour (the reference shows
        // `T1` orange and `fontenc` plain). Neighbouring brackets merge into one span.
        assert_eq!(
            toks("\\usepackage[T1]{fontenc}"),
            vec![
                ("\\usepackage".into(), Token::Tag),
                ("[".into(), Token::Bracket),
                ("T1".into(), Token::Atom),
                ("]{".into(), Token::Bracket),
                ("}".into(), Token::Bracket),
            ]
        );
        assert_eq!(
            toks("\\begin{document}")[2],
            ("document".into(), Token::Atom)
        );
        // \section's argument is plain text: only the command is coloured.
        let t = toks("\\section{Intro}");
        assert_eq!(t[0], ("\\section".into(), Token::Tag));
        assert!(!t.iter().any(|(s, _)| s == "Intro"));
    }

    #[test]
    fn maths_and_comments() {
        let t = toks("$x^2 + 10$ % note");
        assert_eq!(t[0], ("$".into(), Token::Keyword));
        assert_eq!(t[1], ("x".into(), Token::Atom));
        assert_eq!(t[2], ("^".into(), Token::Tag));
        assert_eq!(t[3], ("2".into(), Token::Number));
        assert_eq!(t[4], ("10".into(), Token::Number));
        assert_eq!(t[5], ("$".into(), Token::Keyword));
        assert_eq!(t.last().unwrap(), &("% note".into(), Token::Comment));
        assert_eq!(toks("}")[0].1, Token::Error);
    }

    #[test]
    fn maths_spans_lines_until_an_empty_line() {
        let mut st = State::default();
        line("\\[ a", &mut st);
        assert_eq!(line("b \\]", &mut st)[0].2, Token::Atom);
        let mut st = State::default();
        line("$ open", &mut st);
        line("", &mut st);
        assert_eq!(line("plain", &mut st), vec![]);
    }

    #[test]
    fn the_highlighter_keeps_line_states() {
        let lines = ["\\[", "x", "\\]", "y"];
        let mut h = Highlighter::default();
        assert_eq!(h.tokens(1, |n| lines[n].into())[0].2, Token::Atom);
        assert_eq!(h.tokens(3, |n| lines[n].into()), vec![]);
    }
}
