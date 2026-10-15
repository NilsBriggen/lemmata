//! The table that turns a TeX error into plain English.
//!
//! Each [`Rule`] recognises an error with a regex, then supplies a message,
//! a hint and optionally a fix. The first matching rule wins, so specific
//! rules come before general ones.
//!
//! The regex runs on a small text built by the parser: the error message
//! (continuation lines joined into one), a newline, the part of the source
//! line TeX had read when it stopped (`l.12 <this part>`), a newline, and the
//! rest of that line. `$1`, `$2` ... in message, hint and fix refer to
//! capture groups.

use crate::{Fix, Kind};
use regex::{Captures, Regex};
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    PdfTex,
    XeTex,
    LuaTex,
    Unknown,
}

/// Which TeX engine and LaTeX release produced the log; some advice
/// depends on it.
#[derive(Debug, Clone, Copy)]
pub struct Engine {
    pub kind: EngineKind,
    pub latex_year: Option<u32>,
}

impl Engine {
    pub const UNKNOWN: Engine = Engine {
        kind: EngineKind::Unknown,
        latex_year: None,
    };

    pub fn detect(lines: &[String]) -> Engine {
        let mut e = Engine::UNKNOWN;
        for l in lines.iter().take(60) {
            if let Some(rest) = l.strip_prefix("This is ") {
                e.kind = if rest.starts_with("pdfTeX") {
                    EngineKind::PdfTex
                } else if rest.starts_with("XeTeX") {
                    EngineKind::XeTex
                } else if rest.starts_with("LuaTeX") || rest.starts_with("LuaHBTeX") {
                    EngineKind::LuaTex
                } else {
                    EngineKind::Unknown
                };
            } else if let Some(rest) = l.strip_prefix("LaTeX2e <") {
                e.latex_year = rest.get(..4).and_then(|y| y.parse().ok());
                break;
            }
        }
        e
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Tex,
    Blg,
}

struct RuleFix {
    explanation: &'static str,
    command: Option<&'static str>,
    package: Option<&'static str>,
}

pub struct Explained {
    pub kind: Kind,
    pub message: String,
    pub hint: String,
    pub fix: Option<Fix>,
}

struct Rule {
    pattern: &'static str,
    message: &'static str,
    hint: &'static str,
    kind: fn(&Captures) -> Kind,
    fix: Option<RuleFix>,
    /// For advice that needs code: looking up a package, or the engine.
    refine: Option<fn(&Captures, &Engine, &mut Explained)>,
    source: Source,
}

fn other(_: &Captures) -> Kind {
    Kind::Other
}

fn missing_file(c: &Captures) -> Kind {
    Kind::MissingFile {
        name: c[1].to_string(),
    }
}

fn undefined_cs(_: &Captures) -> Kind {
    Kind::UndefinedControlSequence
}

impl Rule {
    const fn new(pattern: &'static str, message: &'static str, hint: &'static str) -> Rule {
        Rule {
            pattern,
            message,
            hint,
            kind: other,
            fix: None,
            refine: None,
            source: Source::Tex,
        }
    }
    const fn kind(mut self, kind: fn(&Captures) -> Kind) -> Rule {
        self.kind = kind;
        self
    }
    const fn fix(
        mut self,
        explanation: &'static str,
        command: Option<&'static str>,
        package: Option<&'static str>,
    ) -> Rule {
        self.fix = Some(RuleFix {
            explanation,
            command,
            package,
        });
        self
    }
    const fn refine(mut self, f: fn(&Captures, &Engine, &mut Explained)) -> Rule {
        self.refine = Some(f);
        self
    }
    const fn blg(mut self) -> Rule {
        self.source = Source::Blg;
        self
    }
}

static RULES: &[Rule] = &[
    // Missing files.
    Rule::new(
        r"^[^\n]*File [`']([^'\n]+\.(?:sty|cls))' not found",
        "The package file $1 was not found.",
        "$1 isn't installed. Install the TeX package that provides it.",
    )
    .kind(missing_file),
    Rule::new(
        r"^[^\n]*File [`']([^'\n]+)' not found",
        "The file $1 was not found.",
        "$1 doesn't exist. Check the file name and path.",
    )
    .kind(missing_file),
    Rule::new(
        r"^Cannot find '([^'\n]+)'",
        "Bibliography file '$1' was not found.",
        r"Check the name in \addbibresource{} or \bibliography{}, and that the file is in the project.",
    )
    .kind(missing_file)
    .blg(),
    Rule::new(
        r"^I couldn't open database file (\S+)",
        "Bibliography file '$1' was not found.",
        r"Check the name in \bibliography{}, and that the file is in the project.",
    )
    .kind(missing_file)
    .blg(),
    // Commands and environments.
    Rule::new(
        r"^Undefined control sequence\.?\n[^\n]*\\([A-Za-z@]+)\*?[^\\\n]*\n",
        r"LaTeX doesn't know the command \$1.",
        r"Check the spelling of \$1, or load the package that provides it.",
    )
    .kind(undefined_cs)
    .refine(command_package),
    Rule::new(
        r"^Undefined control sequence",
        "LaTeX doesn't know one of the commands used here.",
        "A command isn't defined. Check its spelling, or load the package that provides it.",
    )
    .kind(undefined_cs),
    Rule::new(
        r"^Environment (\S+?)\.? undefined",
        "There is no environment called $1.",
        "This environment doesn't exist. Check the spelling, or load the package that defines it.",
    )
    .refine(environment_package),
    Rule::new(
        r"^\\begin\{([^}]+)\}(?: on input line \d+)? ended by \\end\{([^}]+)\}",
        r"\begin{$1} is closed by \end{$2}.",
        r"\begin and \end don't match. Change one of them, or add the missing \end{$1}; environments must close in the reverse order they were opened.",
    ),
    Rule::new(
        r"^Can be used only in preamble\.?\n[^\n]*\\([A-Za-z@]+)[^\\\n]*\n",
        r"\$1 only works before \begin{document}.",
        r"Move this line up into the preamble, above \begin{document}.",
    ),
    Rule::new(
        r"^Can be used only in preamble",
        r"This command only works before \begin{document}.",
        r"Move this line up into the preamble, above \begin{document}.",
    ),
    Rule::new(
        r"^Missing \\begin\{document\}",
        r"The text starts before \begin{document}.",
        r"Add \begin{document} after the preamble. Otherwise something printable is in the preamble, often a typo or a missing \ or } just above.",
    ),
    // Characters and fonts.
    Rule::new(
        r"^(?:Package inputenc Error: )?Unicode char(?:acter)? (.+?)(?: \(U\+([0-9A-Fa-f]+)\))? not set up for use with LaTeX",
        "LaTeX can't typeset the character $1.",
        "This character isn't available in the current font.",
    )
    .refine(unicode_hint),
    Rule::new(
        r"^(?:Fatal )?Package fontspec Error: The fontspec package requires either XeTeX or LuaTeX",
        "fontspec needs the XeLaTeX or LuaLaTeX engine.",
        "This document uses fontspec (system fonts), which pdfLaTeX can't run. Switch the engine to XeLaTeX or LuaLaTeX in Project settings.",
    ),
    Rule::new(
        r#"^Package fontspec Error: The font "([^"\n]+)" cannot be found"#,
        "The font '$1' is not installed.",
        "Check the font name exactly as your system lists it, or install the font. fontspec only finds fonts installed on this computer.",
    ),
    Rule::new(
        r#"^Font [^\n]*="([^"\n]+)"[^\n]* not loadable"#,
        "The font '$1' could not be found.",
        "Check the font name exactly as your system lists it, or install the font. XeLaTeX and LuaLaTeX only find fonts installed on this computer.",
    ),
    Rule::new(
        r"^Font (\S+?)=(\S+) not loadable: Metric \(TFM\) file not found",
        "The font $2 could not be found.",
        "Check the font name, or install the TeX package that provides it. To use fonts installed on your computer, switch to XeLaTeX or LuaLaTeX and load them with fontspec.",
    ),
    Rule::new(
        r"^Font (\S+) not loadable",
        "The font $1 could not be loaded.",
        "Check the font name, or install the TeX package that provides it. To use fonts installed on your computer, switch to XeLaTeX or LuaLaTeX and load them with fontspec.",
    ),
    Rule::new(
        r"^Option clash for package (\S+?)\.?(?:\s|$)",
        "Package $1 is loaded twice with different options.",
        r"Load $1 once, with all its options in one \usepackage[...]{$1}. Another package or the class may load it already: pass options early with \PassOptionsToPackage{...}{$1} before \documentclass.",
    ),
    // minted.
    Rule::new(
        r"^Package minted Error: minted v3\+ executable is not installed",
        "minted can't find its helper program, latexminted.",
        "minted 3 runs latexminted (it needs Python). Install it with the minted TeX package, make sure it is on your PATH, and restart Lemmata.",
    ),
    Rule::new(
        r"^Package minted Error: [^\n]*minted executable is unavailable or disabled",
        "minted can't run its highlighter.",
        "minted runs a helper program, which needs shell escape. Turn on shell escape in Project settings, then compile again.",
    ),
    Rule::new(
        r"^Package minted Error: You must invoke LaTeX with the -shell-escape flag",
        "minted needs shell escape, which is turned off.",
        "minted runs a helper program, which needs shell escape. Turn on shell escape in Project settings, then compile again.",
    ),
    Rule::new(
        r"^Package minted Error: You must have .pygmentize. installed",
        "minted needs the program pygmentize, and it wasn't found.",
        "Install Pygments so that pygmentize is on your PATH, then restart Lemmata.",
    )
    .fix(
        "Install Pygments, which provides pygmentize.",
        Some("pip install Pygments"),
        None,
    ),
    Rule::new(
        r"^Package (?:minted Error: (?:Missing Pygments output|Cannot find Pygments style)|catchfile Error: File [^\n]*_minted[^\n]* not found)",
        "minted has no highlighted code to show.",
        "This follows another minted error, usually shell escape being off or pygmentize missing. Fix the first minted error and this one goes away.",
    ),
    // Maths and alignment.
    Rule::new(
        r"^Missing \$ inserted\.?\n[^\n]*([_^])\n",
        "A $1 was used outside math mode.",
        r"To print an underscore write \_ (for a caret \^{}). Subscripts and superscripts only work in math, between $...$.",
    ),
    Rule::new(
        r"^Missing \$ inserted\.?\n[^\n]*\\([A-Za-z@]+)\n",
        r"The command \$1 only works in math mode.",
        r"Put it between dollar signs, for example $\$1$, or inside an equation environment.",
    ),
    Rule::new(
        r"^Missing \$ inserted",
        "A math-only command or symbol was used outside math mode.",
        "A math command is used outside math mode. Wrap it in $...$, and check for a blank line in the middle of a formula.",
    ),
    Rule::new(
        r"^Extra \}, or forgotten \$",
        "A } closes something that isn't open, or a $ is missing.",
        r"Check that every $ has a partner and every { a matching }. The cause is usually just above, often in the formula before this line.",
    ),
    Rule::new(
        r"^Too many \}'s",
        "There is a } without a matching {.",
        "There's a closing brace } without a matching {. Remove it, or add the missing {.",
    ),
    Rule::new(
        r"^Missing \} inserted",
        "A { is never closed.",
        "A { is never closed.",
    ),
    Rule::new(
        r"^Misplaced alignment tab character &",
        "An & was used outside a table.",
        r"& only separates cells in tabular, align and similar environments. To print an ampersand, write \&.",
    ),
    Rule::new(
        r"^Extra alignment tab has been changed to \\cr",
        "This table row has more cells than the table has columns.",
        r"Add a column to the column specification (like {lll}), remove the extra &, or write \& for a literal ampersand.",
    ),
    Rule::new(
        r"^I can't find file .([^'\n]+)'",
        "The file $1 could not be read.",
        r"For an \input file, check the name and path. For the .aux file, an earlier error stopped the build: fix the first error.",
    ),
    // Numbers and sizes.
    Rule::new(
        r"^Missing number, treated as zero",
        "A number or length was expected here.",
        r"Look at the command just before this point: a value is empty or misspelled (an empty width=, \hspace{}), or a line starts with [ right after \\.",
    ),
    Rule::new(
        r"^Illegal unit of measure",
        "A length is missing its unit.",
        r"Write a unit after the number, such as 2cm, 10pt or 0.5\textwidth.",
    ),
    Rule::new(
        r"^Dimension too large",
        "A length is bigger than TeX can handle (about 5.7 m).",
        r"A width, height or space is huge, or a calculation overflowed. Check \hspace, \rule, \includegraphics sizes and lengths computed from others.",
    ),
    // Definitions and arguments.
    Rule::new(
        r"^Illegal parameter number in definition of (\\\S+?)\.?(?:\s|$)",
        "A # in the definition of $1 isn't allowed here.",
        r"Parameters are #1 to #9, up to the number declared (\newcommand{...}[2] allows #1 and #2). In a definition inside a definition, double the hash: ##1.",
    ),
    Rule::new(
        r"^Paragraph ended before \S+ was complete",
        "A blank line ended a command's argument too early.",
        "A { is probably missing its }, or a blank line sits inside an argument. The problem is usually just above this line.",
    ),
    Rule::new(
        r"^File ended while scanning",
        "The file ended while something was still open.",
        "A brace or argument is never closed before the end of the file.",
    ),
    Rule::new(
        r"^Runaway argument",
        "An argument is never closed.",
        "An argument is missing its closing brace.",
    ),
    // Memory and floats.
    Rule::new(
        r"^TeX capacity exceeded, sorry \[([^\]=\n]+?)(?:=\d+)?\]",
        "TeX ran out of room ($1).",
        r"Usually a command that calls itself forever (such as \newcommand{\a}{\a x}) or a loop that never ends, rather than a document that is too big. Check macros you defined recently, and the line shown.",
    ),
    Rule::new(
        r"^Float\(s\) lost",
        "A figure or table (float) was lost.",
        "A float was placed where it can't float: inside a box, minipage, footnote or margin note. Move the figure or table out into the main text.",
    ),
    Rule::new(
        r"^Too many unprocessed floats",
        "Too many figures or tables are waiting for a place.",
        r"LaTeX holds back at most 18. Add \clearpage to place them, use placement [!htbp], make them smaller, or use [H] from the float package.",
    ),
    Rule::new(
        r"^Not in outer par mode",
        "A figure or table was used where it can't float.",
        r"figure and table can't go inside boxes such as \fbox, \parbox, minipage or another float. Move it into the main text.",
    ),
];

static COMPILED: LazyLock<Vec<(Regex, &'static Rule)>> = LazyLock::new(|| {
    RULES
        .iter()
        .map(|r| (Regex::new(r.pattern).expect("rule regex"), r))
        .collect()
});

/// Replaces `$1`..`$9` with capture groups; other text, `$` included, is kept.
fn expand(template: &str, c: &Captures) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        match chars.peek().and_then(|d| d.to_digit(10)) {
            Some(n) if ch == '$' => {
                chars.next();
                out.push_str(c.get(n as usize).map_or("", |m| m.as_str()));
            }
            _ => out.push(ch),
        }
    }
    out
}

/// The first rule for `text` from the given log, if there is one.
pub fn explain(text: &str, engine: &Engine, source: Source) -> Option<Explained> {
    COMPILED
        .iter()
        .filter(|(_, r)| r.source == source)
        .find_map(|(re, r)| {
            let c = re.captures(text)?;
            let mut e = Explained {
                kind: (r.kind)(&c),
                message: expand(r.message, &c),
                hint: expand(r.hint, &c),
                fix: r.fix.as_ref().map(|f| Fix {
                    explanation: expand(f.explanation, &c),
                    command: f.command.map(|s| expand(s, &c)),
                    package: f.package.map(|s| expand(s, &c)),
                }),
            };
            if let Some(refine) = r.refine {
                refine(&c, engine, &mut e);
            }
            Some(e)
        })
}

fn package_fix(what: String, package: &str) -> Fix {
    Fix {
        explanation: format!("{what} comes from the {package} package."),
        command: None,
        package: Some(package.to_string()),
    }
}

fn command_package(c: &Captures, _: &Engine, e: &mut Explained) {
    let name = &c[1];
    if let Some(p) = crate::packages::for_command(name) {
        e.hint =
            format!(r"\{name} comes from the {p} package: add \usepackage{{{p}}} to the preamble.");
        e.fix = Some(package_fix(format!(r"\{name}"), p));
    }
}

fn environment_package(c: &Captures, _: &Engine, e: &mut Explained) {
    let name = &c[1];
    if let Some(p) = crate::packages::for_environment(name) {
        e.hint = format!(
            r"The {name} environment comes from the {p} package: add \usepackage{{{p}}} to the preamble."
        );
        e.fix = Some(package_fix(format!("The {name} environment"), p));
    }
}

/// Characters people paste into pdfLaTeX documents, with what to write
/// instead and the package that needs, if any.
const REPLACEMENTS: &[(char, &str, Option<&str>)] = &[
    ('\u{2713}', r"$\checkmark$", Some("amssymb")),
    ('\u{2192}', r"$\rightarrow$", None),
    ('\u{2190}', r"$\leftarrow$", None),
    ('\u{21D2}', r"$\Rightarrow$", None),
    ('\u{2264}', r"$\leq$", None),
    ('\u{2265}', r"$\geq$", None),
    ('\u{2260}', r"$\neq$", None),
    ('\u{2248}', r"$\approx$", None),
    ('\u{221E}', r"$\infty$", None),
    ('\u{2212}', "$-$", None),
    ('\u{00D7}', r"$\times$", None),
    ('\u{03B1}', r"$\alpha$", None),
    ('\u{03B2}', r"$\beta$", None),
    ('\u{03BB}', r"$\lambda$", None),
    ('\u{03BC}', r"$\mu$", None),
    ('\u{03C0}', r"$\pi$", None),
    ('\u{00A0}', "~ (a non-breaking space)", None),
    ('\u{202F}', r"\, (a thin space)", None),
    ('\u{2009}', r"\, (a thin space)", None),
    (
        '\u{200B}',
        "nothing: delete it (it is an invisible space)",
        None,
    ),
    (
        '\u{FEFF}',
        "nothing: delete it (it is an invisible marker)",
        None,
    ),
    ('\u{20AC}', r"\texteuro", None),
    ('\u{00B0}', r"\textdegree", None),
];

fn unicode_hint(c: &Captures, engine: &Engine, e: &mut Explained) {
    let shown = &c[1];
    let ch = c
        .get(2)
        .and_then(|m| u32::from_str_radix(m.as_str(), 16).ok())
        .and_then(char::from_u32);
    let old = engine.latex_year.is_some_and(|y| y < 2018);
    e.hint = match engine.kind {
        EngineKind::XeTex | EngineKind::LuaTex => format!(
            r"The current font has no glyph for {shown}. Choose a font that has it with \setmainfont{{...}}, or replace the character."
        ),
        _ if old => r"This LaTeX is older than 2018 and doesn't read UTF-8 by default. Add \usepackage[utf8]{inputenc} and \usepackage[T1]{fontenc} to the preamble; if the character is still unknown, switch to XeLaTeX or LuaLaTeX.".into(),
        _ => match ch.and_then(|ch| REPLACEMENTS.iter().find(|r| r.0 == ch)) {
            Some((_, with, pkg)) => {
                if let Some(p) = pkg {
                    e.fix = Some(Fix {
                        explanation: format!("{with} needs the {p} package."),
                        command: None,
                        package: Some(p.to_string()),
                    });
                }
                format!("pdfLaTeX doesn't know {shown}. Write {with} instead (or switch to XeLaTeX or LuaLaTeX, which accept any character).")
            }
            None => format!("pdfLaTeX only knows a limited set of characters, and {shown} isn't one. Switch the engine to XeLaTeX or LuaLaTeX in Project settings, or replace the character with a LaTeX command."),
        },
    };
}
