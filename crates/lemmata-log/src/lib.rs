//! Turns a TeX `.log` file into a list of diagnostics with file, line,
//! a plain-language message and, where we know one, a hint for fixing it.
//!
//! Assumes the log was produced with `-file-line-error`, which is how
//! Lemmata always invokes latexmk. Logs without it still parse, but errors
//! fall back to the file stack for their location.

pub mod launcher;
pub mod packages;
mod rules;

use regex::Regex;
use rules::{Engine, Source};
use serde::Serialize;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Kind {
    MissingFile { name: String },
    UndefinedCitation { key: String },
    UndefinedReference { key: String },
    UndefinedControlSequence,
    BadBox,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub kind: Kind,
    /// Path as written in the log, relative to the project root when TeX printed it that way.
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
    pub hint: Option<String>,
    /// How to fix it on this system, filled in by the build for missing files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fix {
    pub explanation: String,
    /// A shell command the user can run, if there is one.
    pub command: Option<String>,
    /// A LaTeX package the document should load (`\usepackage{…}`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

/// TeX hard-wraps log lines at this many characters (`max_print_line`).
const WRAP: usize = 79;

static FILE_LINE_ERROR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(.+?\.(?:tex|sty|cls|ltx|dtx|bbl|def|cfg|clo|ins)):(\d+): (.*)$").unwrap()
});
static BANG_ERROR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^! (.*)$").unwrap());
static CONTEXT_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^l\.(\d+)").unwrap());
static CONTEXT_TEXT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^l\.\d+ (.*)$").unwrap());
static LATEX_WARNING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:LaTeX|Class \S+|Package \S+) Warning: (.*)$").unwrap());
static PACKAGE_CONT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\(\S+\)\s+(.*)$").unwrap());
static INPUT_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"on input line (\d+)\.?").unwrap());
static BAD_BOX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^((?:Over|Under)full \\[hv]box \([^)]*\)).*?(?:at lines? (\d+)(?:--\d+)?|has occurred while \\output is active)").unwrap()
});
static CITATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Citation [`']([^']+)' (?:on page \d+ )?undefined").unwrap());
static MULTIPLY_DEFINED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Label [`']([^']+)' multiply defined").unwrap());
static REFERENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Reference [`']([^']+)' (?:on page \d+ )?undefined").unwrap());

/// Messages that only restate an earlier error.
const NOISE: &[&str] = &[
    "Emergency stop.",
    "==> Fatal error occurred, no output PDF file produced!",
    "Interruption.",
];

pub fn parse(log: &str) -> Vec<Diagnostic> {
    let lines = unwrap_lines(log);
    let mut out: Vec<Diagnostic> = Vec::new();
    let mut files = FileStack::default();
    let engine = Engine::detect(&lines);
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].as_str();

        if let Some(c) = FILE_LINE_ERROR.captures(line) {
            let msg = c[3].trim().to_string();
            if !NOISE.contains(&msg.as_str()) {
                out.push(error(
                    Some(c[1].to_string()),
                    c[2].parse().ok(),
                    &msg,
                    &lines[i + 1..],
                    &engine,
                ));
            }
            i += 1;
            continue;
        }

        if let Some(c) = BANG_ERROR.captures(line) {
            let msg = c[1].trim().to_string();
            if !NOISE.contains(&msg.as_str()) {
                // Without -file-line-error the line comes from the "l.<n>" context, which
                // TeX may print after a follow-up "file:line:" message.
                let line_no = lines[i + 1..]
                    .iter()
                    .take(20)
                    .take_while(|l| !l.starts_with("! "))
                    .find_map(|l| CONTEXT_LINE.captures(l).and_then(|c| c[1].parse().ok()));
                out.push(error(files.top(), line_no, &msg, &lines[i + 1..], &engine));
            }
            i += 1;
            continue;
        }

        if let Some(c) = LATEX_WARNING.captures(line) {
            // Warnings continue on following lines until a blank line; package
            // warnings prefix continuations with "(pkgname)".
            let mut text = c[1].trim().to_string();
            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim().is_empty() {
                let next = lines[j].as_str();
                if let Some(pc) = PACKAGE_CONT.captures(next) {
                    text.push(' ');
                    text.push_str(pc[1].trim());
                } else if next.starts_with(' ') || !next.contains(':') && !next.starts_with('(') {
                    text.push(' ');
                    text.push_str(next.trim());
                } else {
                    break;
                }
                j += 1;
            }
            out.push(warning(files.top(), text));
            i = j;
            continue;
        }

        if let Some(c) = BAD_BOX.captures(line) {
            out.push(Diagnostic {
                severity: Severity::Info,
                kind: Kind::BadBox,
                file: files.top(),
                line: c.get(2).and_then(|m| m.as_str().parse().ok()),
                message: c[1].to_string(),
                hint: None,
                fix: None,
            });
        }

        files.scan(line);
        i += 1;
    }

    for d in &mut out {
        soften_shell_escape(d);
    }
    dedupe(out)
}

/// MiKTeX (and TeX Live's epstopdf) mention shell escape on documents that
/// merely load graphics. It only matters when an .eps image needs converting
/// or a package such as minted asks for it, so it isn't a problem to fix.
fn soften_shell_escape(d: &mut Diagnostic) {
    if d.message.contains("Shell escape feature is not enabled") {
        d.severity = Severity::Info;
        d.hint = Some(
            "Shell escape is off, which is the safe default. You only need it for packages that run programs (minted, svg) or to convert .eps images; turn it on in Project settings if a build asks for it."
                .into(),
        );
    }
}

/// What follows an error line in the log, up to the next error: the
/// continuation of the message, and TeX's view of the source line.
struct ErrorContext {
    /// Further lines of the message, joined.
    message_rest: String,
    /// The source line up to where TeX stopped, and what it hadn't read yet.
    before: String,
    after: String,
}

impl ErrorContext {
    fn of(after: &[String]) -> ErrorContext {
        let body: Vec<&str> = after
            .iter()
            .take(20)
            .take_while(|l| !l.starts_with("! ") && !FILE_LINE_ERROR.is_match(l))
            .map(String::as_str)
            .collect();
        // Package errors continue on indented or "(package)" lines until a blank one.
        let message_rest = body
            .iter()
            .take_while(|l| {
                !l.trim().is_empty() && (l.starts_with(' ') || PACKAGE_CONT.is_match(l))
            })
            .map(|l| {
                PACKAGE_CONT
                    .captures(l)
                    .map_or(l.trim(), |c| c.get(1).unwrap().as_str().trim())
            })
            .collect::<Vec<_>>()
            .join(" ");
        let at = body.iter().position(|l| CONTEXT_LINE.is_match(l));
        let before = at
            .and_then(|p| CONTEXT_TEXT.captures(body[p]))
            .map_or(String::new(), |c| c[1].to_string());
        let after = at
            .and_then(|p| body.get(p + 1))
            .filter(|l| l.starts_with(' '))
            .map_or(String::new(), |l| l.trim().to_string());
        ErrorContext {
            message_rest,
            before,
            after,
        }
    }
}

fn error(
    file: Option<String>,
    line: Option<u32>,
    message: &str,
    after: &[String],
    engine: &Engine,
) -> Diagnostic {
    // "LaTeX Error:" adds nothing next to the error badge in the UI.
    let message = message.strip_prefix("LaTeX Error: ").unwrap_or(message);
    let ctx = ErrorContext::of(after);
    let text = format!("{message} {}", ctx.message_rest)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let haystack = format!("{text}\n{}\n{}", ctx.before, ctx.after);
    let (kind, message, hint, fix) = match rules::explain(&haystack, engine, Source::Tex) {
        Some(e) => (e.kind, e.message, Some(e.hint), e.fix),
        None => (Kind::Other, text, None, None),
    };
    Diagnostic {
        severity: Severity::Error,
        kind,
        file,
        line,
        message,
        hint,
        fix,
    }
}

fn warning(file: Option<String>, text: String) -> Diagnostic {
    let line = INPUT_LINE.captures(&text).and_then(|c| c[1].parse().ok());
    let message = INPUT_LINE
        .replace(&text, "")
        .trim()
        .trim_end_matches('.')
        .to_string()
        + ".";
    let (kind, hint) = if let Some(c) = CITATION.captures(&text) {
        (
            Kind::UndefinedCitation { key: c[1].to_string() },
            Some("No entry with this key in your .bib files, or the bibliography hasn't been built yet.".into()),
        )
    } else if let Some(c) = REFERENCE.captures(&text) {
        (
            Kind::UndefinedReference {
                key: c[1].to_string(),
            },
            Some(
                "No \\label with this name. A label added just now resolves on the next build."
                    .into(),
            ),
        )
    } else if let Some(c) = MULTIPLY_DEFINED.captures(&text) {
        (
            Kind::Other,
            Some(format!(
                "Two \\label commands are both called \"{}\". Every label must be unique, or references may point to the wrong place.",
                &c[1]
            )),
        )
    } else if text.contains("may have changed") && text.contains("Rerun") {
        (
            Kind::Other,
            Some(
                "The numbering shifted during this build. Recompile once more to settle it.".into(),
            ),
        )
    } else {
        (Kind::Other, None)
    };
    Diagnostic {
        severity: Severity::Warning,
        kind,
        file,
        line,
        message,
        hint,
        fix: None,
    }
}

/// Re-joins lines TeX wrapped at [`WRAP`] characters.
fn unwrap_lines(log: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut continuing = false;
    for raw in log.lines() {
        if continuing {
            out.last_mut().unwrap().push_str(raw);
        } else {
            out.push(raw.to_string());
        }
        continuing = raw.chars().count() == WRAP;
    }
    out
}

/// Tracks which file TeX is reading from the `(./file.tex ... )` markers.
#[derive(Default)]
struct FileStack(Vec<Option<String>>);

impl FileStack {
    fn top(&self) -> Option<String> {
        self.0.iter().rev().find_map(|f| f.clone())
    }

    fn scan(&mut self, line: &str) {
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'(' => {
                    let rest = &line[i + 1..];
                    let end = rest
                        .find(|c: char| c.is_whitespace() || c == '(' || c == ')')
                        .unwrap_or(rest.len());
                    let name = &rest[..end];
                    let is_path = name.contains('.')
                        && (name.starts_with("./")
                            || name.starts_with('/')
                            || name.contains('/')
                            || name.ends_with(".tex"));
                    self.0.push(is_path.then(|| name.to_string()));
                    i += 1 + end;
                }
                b')' => {
                    self.0.pop();
                    i += 1;
                }
                _ => i += 1,
            }
        }
    }
}

static BIBER_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(ERROR|WARN) - (.*)$").unwrap());
static BIBER_SYNTAX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"BibTeX subsystem: .*?([^/\\ ]+?\.bib)_\w+\.utf8, line (\d+), (.*)$").unwrap()
});
static BIBER_ENTRY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"didn't find a database entry for '([^']+)'").unwrap());
static BIBTEX_ENTRY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^Warning--I didn't find a database entry for "([^"]+)""#).unwrap()
});
static BIBTEX_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^---line (\d+) of file (\S+)").unwrap());

fn bib_diag(
    severity: Severity,
    kind: Kind,
    file: Option<String>,
    line: Option<u32>,
    message: String,
    hint: &str,
) -> Diagnostic {
    Diagnostic {
        severity,
        kind,
        file,
        line,
        message,
        hint: Some(hint.into()),
        fix: None,
    }
}

fn blg_rule_diag(e: rules::Explained) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        kind: e.kind,
        file: None,
        line: None,
        message: e.message,
        hint: Some(e.hint),
        fix: e.fix,
    }
}

/// Problems the bibliography tool reported in its `.blg` log (biber or
/// BibTeX): a missing .bib file, a syntax error in an entry, a cited key
/// that isn't in the database.
pub fn parse_blg(blg: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let lines: Vec<&str> = blg.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if let Some(c) = BIBER_LINE.captures(line) {
            let (level, msg) = (&c[1], c[2].trim());
            let severity = if level == "ERROR" {
                Severity::Error
            } else {
                Severity::Warning
            };
            if let Some(m) = BIBER_SYNTAX.captures(msg) {
                out.push(bib_diag(
                    Severity::Error,
                    Kind::Other,
                    Some(m[1].to_string()),
                    m[2].parse().ok(),
                    format!("Bibliography entry could not be read: {}", m[3].trim()),
                    "Check that braces and quotes are balanced and that fields are separated by commas.",
                ));
            } else if let Some(m) = BIBER_ENTRY.captures(msg) {
                out.push(bib_diag(
                    severity,
                    Kind::UndefinedCitation {
                        key: m[1].to_string(),
                    },
                    None,
                    None,
                    format!("Citation '{}' is not in your bibliography.", &m[1]),
                    "No entry with this key in your .bib files. Check the key's spelling.",
                ));
            } else if let Some(e) = rules::explain(msg, &Engine::UNKNOWN, Source::Blg) {
                out.push(blg_rule_diag(e));
            } else if severity == Severity::Error {
                out.push(bib_diag(
                    severity,
                    Kind::Other,
                    None,
                    None,
                    msg.to_string(),
                    "Reported by biber.",
                ));
            }
        } else if let Some(c) = BIBTEX_ENTRY.captures(line) {
            out.push(bib_diag(
                Severity::Warning,
                Kind::UndefinedCitation {
                    key: c[1].to_string(),
                },
                None,
                None,
                format!("Citation '{}' is not in your bibliography.", &c[1]),
                "No entry with this key in your .bib files. Check the key's spelling.",
            ));
        } else if let Some(e) = rules::explain(line, &Engine::UNKNOWN, Source::Blg) {
            out.push(blg_rule_diag(e));
        } else if let Some(c) = BIBTEX_LINE
            .captures(line)
            // "---line 4 of file main.aux" only locates a problem in the .aux file, already reported.
            .filter(|c| !c[2].ends_with(".aux"))
        {
            // The complaint is on the previous line: "I was expecting a `,' or a `}'".
            let what = lines
                .get(i.wrapping_sub(1))
                .map(|l| l.trim())
                .unwrap_or("syntax error");
            out.push(bib_diag(
                Severity::Error,
                Kind::Other,
                Some(c[2].to_string()),
                c[1].parse().ok(),
                format!("Bibliography entry could not be read: {what}."),
                "Check that braces and quotes are balanced and that fields are separated by commas.",
            ));
        }
    }
    dedupe(out)
}

fn dedupe(diags: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = Vec::with_capacity(diags.len());
    for d in diags {
        if !out.contains(&d) {
            out.push(d);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_shell_escape_remark_is_information_not_a_problem() {
        let log = "Package epstopdf-base Warning: Shell escape feature is not enabled.\n\nmain.tex:3: Shell escape feature is not enabled.\n";
        let d = parse(log);
        assert!(!d.is_empty());
        assert!(d.iter().all(|x| x.severity == Severity::Info), "{d:?}");
        assert!(d[0].hint.as_deref().unwrap().contains("Project settings"));
    }

    use super::*;

    const UNDEFINED: &str =
        include_str!("../../../tests/fixtures/logs/undefined-control-sequence.log");
    const MISSING: &str = include_str!("../../../tests/fixtures/logs/missing-package.log");
    const WARNINGS: &str = include_str!("../../../tests/fixtures/logs/warnings.log");

    #[test]
    fn undefined_control_sequence_has_location() {
        let d = parse(UNDEFINED);
        let e = d.iter().find(|d| d.severity == Severity::Error).unwrap();
        assert_eq!(e.kind, Kind::UndefinedControlSequence);
        assert_eq!(e.file.as_deref(), Some("./chapters/method.tex"));
        assert_eq!(e.line, Some(12));
        assert!(e.hint.is_some());
    }

    #[test]
    fn missing_package_is_recognised() {
        let d = parse(MISSING);
        let e = d.iter().find(|d| d.severity == Severity::Error).unwrap();
        assert_eq!(
            e.kind,
            Kind::MissingFile {
                name: "pgfplots.sty".into()
            }
        );
        assert_eq!(e.line, Some(3));
        // "Emergency stop" must not appear as a second error.
        assert_eq!(
            d.iter().filter(|d| d.severity == Severity::Error).count(),
            1
        );
    }

    #[test]
    fn real_texlive_2026_missing_package() {
        // TeX Live prints missing files as "! LaTeX Error" even with -file-line-error,
        // and the line comes from the "l.<n>" context after the help text.
        let d = parse(include_str!(
            "../../../tests/fixtures/logs/missing-package-texlive2026.log"
        ));
        let errors: Vec<_> = d.iter().filter(|d| d.severity == Severity::Error).collect();
        assert_eq!(errors.len(), 1, "{errors:#?}");
        assert_eq!(
            errors[0].kind,
            Kind::MissingFile {
                name: "lemmata-does-not-exist.sty".into()
            }
        );
        assert_eq!(errors[0].file.as_deref(), Some("./main.tex"));
        assert_eq!(errors[0].line, Some(4));
    }

    #[test]
    fn warnings_carry_keys_lines_and_files() {
        let d = parse(WARNINGS);
        let cite = d
            .iter()
            .find(|d| matches!(d.kind, Kind::UndefinedCitation { .. }))
            .unwrap();
        assert_eq!(
            cite.kind,
            Kind::UndefinedCitation {
                key: "schulman2017".into()
            }
        );
        assert_eq!(cite.line, Some(16));
        assert_eq!(cite.file.as_deref(), Some("./chapters/method.tex"));

        let r = d
            .iter()
            .find(|d| matches!(d.kind, Kind::UndefinedReference { .. }))
            .unwrap();
        assert_eq!(
            r.kind,
            Kind::UndefinedReference {
                key: "fig:grid".into()
            }
        );

        let pkg = d
            .iter()
            .find(|d| d.message.contains("hyperref") || d.message.contains("Token not allowed"))
            .unwrap();
        assert!(
            pkg.message.contains("removing"),
            "continuation lines joined: {}",
            pkg.message
        );

        let bad = d.iter().find(|d| d.kind == Kind::BadBox).unwrap();
        assert_eq!(bad.line, Some(20));
        assert_eq!(bad.severity, Severity::Info);
    }

    #[test]
    fn wrapped_lines_are_joined() {
        let long = format!("{}\nrest", "x".repeat(WRAP));
        assert_eq!(
            unwrap_lines(&long),
            vec![format!("{}rest", "x".repeat(WRAP))]
        );
    }

    #[test]
    fn undefined_command_suggests_its_package() {
        let log = "./main.tex:12: Undefined control sequence.\nl.12 Let $x \\in \\mathbb\n                {R}$ be a number.\n";
        let d = parse(log);
        assert_eq!(d.len(), 1);
        let fix = d[0].fix.as_ref().unwrap();
        assert_eq!(fix.package.as_deref(), Some("amssymb"));
        assert!(fix.explanation.contains("\\mathbb"));
    }

    #[test]
    fn unknown_command_has_no_package() {
        let log = "./main.tex:3: Undefined control sequence.\nl.3 \\frobnicate\n";
        assert!(parse(log)[0].fix.is_none());
    }

    #[test]
    fn undefined_environment_suggests_its_package() {
        let log = "./main.tex:5: LaTeX Error: Environment align undefined.\n\nSee the LaTeX manual.\nl.5 \\begin{align}\n";
        let d = parse(log);
        assert_eq!(
            d[0].fix.as_ref().unwrap().package.as_deref(),
            Some("amsmath")
        );
    }

    #[test]
    fn multiply_defined_label_is_explained() {
        let d = parse("LaTeX Warning: Label `eq:a' multiply defined.\n\n");
        assert!(d[0].hint.as_deref().unwrap().contains("eq:a"));
    }

    #[test]
    fn biber_log_problems() {
        let blg = "[12] Utils.pm:1> INFO - Looking for bibtex format file 'refs.bib'\n\
[20] bibtex.pm:2> ERROR - BibTeX subsystem: /tmp/par-x/cache/refs.bib_Ab12.utf8, line 14, syntax error: at line 14, expected a field name\n\
[21] Biber.pm:3> WARN - I didn't find a database entry for 'smith2020' (section 0)\n\
[22] Biber.pm:4> ERROR - Cannot find 'missing.bib'!\n";
        let d = parse_blg(blg);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].file.as_deref(), Some("refs.bib"));
        assert_eq!(d[0].line, Some(14));
        assert_eq!(
            d[1].kind,
            Kind::UndefinedCitation {
                key: "smith2020".into()
            }
        );
        assert_eq!(
            d[2].kind,
            Kind::MissingFile {
                name: "missing.bib".into()
            }
        );
    }

    #[test]
    fn bibtex_log_problems() {
        let blg = "I was expecting a `,' or a `}'\n---line 7 of file refs.bib\nWarning--I didn't find a database entry for \"jones1999\"\nI couldn't open database file nope.bib\n";
        let d = parse_blg(blg);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].line, Some(7));
        assert_eq!(d[0].file.as_deref(), Some("refs.bib"));
        assert_eq!(
            d[1].kind,
            Kind::UndefinedCitation {
                key: "jones1999".into()
            }
        );
        assert_eq!(
            d[2].kind,
            Kind::MissingFile {
                name: "nope.bib".into()
            }
        );
    }

    // Error explanations, one test per rule, on logs from real TeX Live runs.

    macro_rules! fixture {
        ($name:literal) => {
            include_str!(concat!("../../../tests/fixtures/logs/", $name))
        };
    }

    /// The first error in `log` whose message contains `needle`.
    fn error_like(log: &str, needle: &str) -> Diagnostic {
        let d = parse(log);
        d.iter()
            .find(|d| d.severity == Severity::Error && d.message.contains(needle))
            .unwrap_or_else(|| panic!("no error containing {needle:?} in {d:#?}"))
            .clone()
    }

    fn hint(d: &Diagnostic) -> &str {
        d.hint.as_deref().expect("hint")
    }

    fn at_main(d: &Diagnostic, line: u32) {
        assert_eq!(d.file.as_deref(), Some("./main.tex"), "{d:?}");
        assert_eq!(d.line, Some(line), "{d:?}");
    }

    #[test]
    fn unicode_character_in_pdflatex_suggests_the_command_and_package() {
        let d = error_like(fixture!("unicode-character.log"), "character ✓");
        at_main(&d, 3);
        assert!(hint(&d).contains("\\checkmark"), "{d:?}");
        assert!(!hint(&d).contains("inputenc"));
        assert_eq!(d.fix.unwrap().package.as_deref(), Some("amssymb"));
    }

    #[test]
    fn unknown_unicode_character_suggests_another_engine() {
        let log = fixture!("unicode-character.log").replace("✓ (U+2713)", "★ (U+2605)");
        let d = error_like(&log, "character ★");
        assert!(hint(&d).contains("XeLaTeX or LuaLaTeX"), "{d:?}");
        assert!(d.fix.is_none());
    }

    #[test]
    fn unicode_character_with_an_old_latex_suggests_inputenc() {
        let log = fixture!("unicode-character.log")
            .replace("LaTeX2e <2025-11-01>", "LaTeX2e <2017-04-15>");
        let d = error_like(&log, "character ✓");
        assert!(hint(&d).contains("\\usepackage[utf8]{inputenc}"), "{d:?}");
    }

    #[test]
    fn unicode_character_in_xelatex_points_at_the_font() {
        let log = fixture!("unicode-character.log").replace("This is pdfTeX", "This is XeTeX");
        let d = error_like(&log, "character ✓");
        assert!(hint(&d).contains("\\setmainfont"), "{d:?}");
        assert!(!hint(&d).contains("inputenc"));
    }

    #[test]
    fn missing_number() {
        let d = error_like(fixture!("missing-number.log"), "number or length");
        at_main(&d, 3);
        assert!(hint(&d).contains("empty or misspelled"));
        let unit = error_like(fixture!("missing-number.log"), "missing its unit");
        assert!(hint(&unit).contains("2cm"));
    }

    #[test]
    fn misplaced_alignment_tab() {
        let d = error_like(fixture!("misplaced-tab.log"), "& was used outside a table");
        at_main(&d, 3);
        assert!(hint(&d).contains("\\&"));
    }

    #[test]
    fn extra_alignment_tab() {
        let d = error_like(fixture!("extra-tab.log"), "more cells");
        at_main(&d, 4);
        assert!(hint(&d).contains("column specification"));
    }

    #[test]
    fn option_clash_names_the_package() {
        let d = error_like(fixture!("option-clash.log"), "inputenc");
        at_main(&d, 5);
        assert!(d.message.contains("loaded twice"));
        assert!(hint(&d).contains("\\PassOptionsToPackage{...}{inputenc}"));
    }

    #[test]
    fn fontspec_font_not_found() {
        let d = error_like(
            fixture!("fontspec-font-missing.log"),
            "Nonexistent Font XYZ",
        );
        at_main(&d, 4);
        assert!(d.message.contains("is not installed"), "{}", d.message);
        assert!(hint(&d).contains("installed on this computer"));
    }

    #[test]
    fn xetex_font_not_loadable() {
        let d = parse(fixture!("fontspec-font-missing.log"));
        let e = d
            .iter()
            .find(|d| d.message.contains("could not be found"))
            .unwrap();
        assert!(e.message.contains("Nonexistent Font XYZ"));
        assert!(hint(e).contains("XeLaTeX and LuaLaTeX"));
    }

    #[test]
    fn tfm_font_not_found() {
        let d = error_like(fixture!("font-not-loadable.log"), "nonexistentfont");
        at_main(&d, 4);
        assert!(d.message.contains("could not be found"));
        assert!(hint(&d).contains("fontspec"));
    }

    #[test]
    fn fontspec_in_pdflatex_asks_for_another_engine() {
        let d = error_like(fixture!("fontspec-needs-xetex.log"), "fontspec needs");
        assert_eq!(d.line, Some(101));
        assert!(hint(&d).contains("XeLaTeX or LuaLaTeX"));
    }

    #[test]
    fn minted_shell_escape_off_in_version_2() {
        let d = error_like(
            fixture!("minted-shell-escape.log"),
            "minted needs shell escape",
        );
        at_main(&d, 3);
        assert!(hint(&d).contains("shell escape in Project settings"));
    }

    #[test]
    fn minted_shell_escape_off_in_version_3() {
        let d = error_like(
            fixture!("minted-executable-disabled.log"),
            "can't run its highlighter",
        );
        at_main(&d, 6);
        assert!(hint(&d).contains("shell escape in Project settings"));
    }

    #[test]
    fn minted_helper_not_on_path() {
        let d = error_like(fixture!("minted-executable-missing.log"), "latexminted");
        at_main(&d, 6);
        assert!(hint(&d).contains("PATH"));
    }

    #[test]
    fn pygmentize_missing() {
        let d = error_like(fixture!("minted-pygmentize.log"), "pygmentize");
        at_main(&d, 3);
        let fix = d.fix.unwrap();
        assert_eq!(fix.command.as_deref(), Some("pip install Pygments"));
    }

    #[test]
    fn minted_follow_up_errors_point_back_to_the_first() {
        let d = error_like(fixture!("minted-pygmentize.log"), "no highlighted code");
        assert!(hint(&d).contains("first minted error"));
    }

    #[test]
    fn underscore_outside_math() {
        let d = error_like(
            fixture!("missing-dollar-underscore.log"),
            "_ was used outside",
        );
        at_main(&d, 3);
        assert!(hint(&d).contains("\\_"));
    }

    #[test]
    fn math_command_outside_math() {
        let d = error_like(fixture!("missing-dollar.log"), "\\alpha");
        at_main(&d, 3);
        assert!(hint(&d).contains("$\\alpha$"), "{}", hint(&d));
    }

    #[test]
    fn paragraph_ended_before_argument_was_complete() {
        let d = error_like(fixture!("paragraph-ended.log"), "blank line");
        at_main(&d, 5);
        assert!(hint(&d).contains("missing its }"));
    }

    #[test]
    fn too_many_closing_braces() {
        let d = error_like(fixture!("too-many-braces.log"), "without a matching {");
        at_main(&d, 3);
        assert!(hint(&d).contains("closing brace"));
    }

    #[test]
    fn extra_brace_or_forgotten_dollar() {
        let d = error_like(fixture!("extra-brace.log"), "} closes something");
        at_main(&d, 3);
        assert!(hint(&d).contains("every $"));
    }

    #[test]
    fn begin_ended_by_another_end() {
        let d = error_like(fixture!("environment-mismatch.log"), "\\begin{itemize}");
        at_main(&d, 5);
        assert!(d.message.contains("\\end{enumerate}"));
        assert!(hint(&d).contains("\\end{itemize}"));
    }

    #[test]
    fn undefined_environment_from_a_real_log_suggests_amsmath() {
        let d = error_like(
            fixture!("environment-undefined.log"),
            "no environment called align",
        );
        at_main(&d, 3);
        assert_eq!(d.fix.unwrap().package.as_deref(), Some("amsmath"));
    }

    #[test]
    fn undefined_command_is_named() {
        let d = parse(UNDEFINED);
        let e = d.iter().find(|d| d.severity == Severity::Error).unwrap();
        assert!(e.message.contains("\\maximise"), "{}", e.message);
        assert!(hint(e).contains("\\maximise"));
        assert!(e.fix.is_none());
    }

    #[test]
    fn dimension_too_large() {
        let d = error_like(fixture!("dimension-too-large.log"), "bigger than TeX");
        at_main(&d, 3);
        assert!(hint(&d).contains("\\hspace"));
    }

    #[test]
    fn capacity_exceeded_blames_recursion() {
        let d = error_like(fixture!("capacity-exceeded.log"), "ran out of room");
        at_main(&d, 4);
        assert!(d.message.contains("input stack size"));
        assert!(hint(&d).contains("calls itself"));
    }

    #[test]
    fn floats_lost() {
        let d = error_like(fixture!("floats-lost.log"), "was lost");
        at_main(&d, 4);
        assert!(hint(&d).contains("footnote"));
    }

    #[test]
    fn too_many_unprocessed_floats() {
        let d = error_like(fixture!("too-many-floats.log"), "waiting for a place");
        assert!(hint(&d).contains("\\clearpage"));
    }

    #[test]
    fn preamble_only_command_is_named() {
        let d = error_like(fixture!("preamble-only.log"), "only works before");
        at_main(&d, 3);
        assert!(d.message.contains("\\usepackage"));
        assert!(hint(&d).contains("preamble"));
    }

    #[test]
    fn missing_begin_document() {
        let d = error_like(fixture!("missing-begin-document.log"), "starts before");
        at_main(&d, 2);
        assert!(hint(&d).contains("\\begin{document}"));
        let aux = error_like(fixture!("missing-begin-document.log"), "main.aux");
        assert!(hint(&aux).contains("first error"));
    }

    #[test]
    fn float_in_a_box() {
        let d = error_like(fixture!("not-outer-par-mode.log"), "can't float");
        at_main(&d, 3);
        assert!(hint(&d).contains("\\fbox"));
    }

    #[test]
    fn illegal_parameter_number() {
        let d = error_like(fixture!("illegal-parameter.log"), "\\foo");
        at_main(&d, 3);
        assert!(hint(&d).contains("##1"));
    }

    #[test]
    fn biber_cannot_find_the_bib_file() {
        let d = parse_blg(fixture!("biber-missing-bib.blg"));
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(
            d[0].kind,
            Kind::MissingFile {
                name: "nope.bib".into()
            }
        );
        assert!(d[0].message.contains("'nope.bib' was not found"));
        assert!(hint(&d[0]).contains("addbibresource"));
    }

    #[test]
    fn bibtex_cannot_open_the_database() {
        // "---line 4 of file main.aux" only repeats the same problem.
        let d = parse_blg(fixture!("bibtex-missing-bib.blg"));
        assert_eq!(d.len(), 2, "{d:?}");
        assert!(matches!(d[1].kind, Kind::UndefinedCitation { .. }));
        assert_eq!(
            d[0].kind,
            Kind::MissingFile {
                name: "nope.bib".into()
            }
        );
        assert!(hint(&d[0]).contains("\\bibliography"));
    }

    #[test]
    fn package_errors_join_their_continuation_lines() {
        let log = "./main.tex:4: Package foo Error: \n(foo)                Something broke\n(foo)                badly.\n\nl.4 x\n";
        let d = parse(log);
        assert_eq!(d[0].message, "Package foo Error: Something broke badly.");
        assert_eq!(d[0].line, Some(4));
    }
}
