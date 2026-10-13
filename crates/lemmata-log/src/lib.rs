//! Turns a TeX `.log` file into a list of diagnostics with file, line,
//! a plain-language message and, where we know one, a hint for fixing it.
//!
//! Assumes the log was produced with `-file-line-error`, which is how
//! Lemmata always invokes latexmk. Logs without it still parse, but errors
//! fall back to the file stack for their location.

pub mod packages;

use regex::Regex;
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
static LATEX_WARNING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:LaTeX|Class \S+|Package \S+) Warning: (.*)$").unwrap());
static PACKAGE_CONT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\(\S+\)\s+(.*)$").unwrap());
static INPUT_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"on input line (\d+)\.?").unwrap());
static BAD_BOX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^((?:Over|Under)full \\[hv]box \([^)]*\)).*?(?:at lines? (\d+)(?:--\d+)?|has occurred while \\output is active)").unwrap()
});
static MISSING_FILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"File [`']([^']+)' not found").unwrap());
static CITATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Citation [`']([^']+)' (?:on page \d+ )?undefined").unwrap());
static CONTEXT_TEXT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^l\.\d+ (.*)$").unwrap());
static LAST_COMMAND: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\([A-Za-z@]+)\*?[^\\]*$").unwrap());
static ENVIRONMENT_UNDEFINED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Environment (\S+) undefined").unwrap());
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
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].as_str();

        if let Some(c) = FILE_LINE_ERROR.captures(line) {
            let msg = c[3].trim().to_string();
            if !NOISE.contains(&msg.as_str()) {
                let mut d = error(Some(c[1].to_string()), c[2].parse().ok(), msg);
                add_package_fix(&mut d, &lines[i + 1..]);
                out.push(d);
            }
            i += 1;
            continue;
        }

        if let Some(c) = BANG_ERROR.captures(line) {
            let msg = c[1].trim().to_string();
            // The "l.<n>" context line follows, after TeX's help text,
            // before the next error starts.
            let line_no = lines[i + 1..]
                .iter()
                .take(20)
                .take_while(|l| !l.starts_with("! "))
                .find_map(|l| CONTEXT_LINE.captures(l).and_then(|c| c[1].parse().ok()));
            if !NOISE.contains(&msg.as_str()) {
                let mut d = error(files.top(), line_no, msg);
                add_package_fix(&mut d, &lines[i + 1..]);
                out.push(d);
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

    dedupe(out)
}

/// For an undefined command or environment we know a package for, suggest
/// loading it. `after` is the log text following the error line.
fn add_package_fix(d: &mut Diagnostic, after: &[String]) {
    let found = match &d.kind {
        Kind::UndefinedControlSequence => after
            .iter()
            .take(20)
            .take_while(|l| !l.starts_with("! ") && !FILE_LINE_ERROR.is_match(l))
            .find_map(|l| CONTEXT_TEXT.captures(l))
            .and_then(|c| LAST_COMMAND.captures(&c[1]).map(|m| m[1].to_string()))
            .and_then(|name| {
                packages::for_command(&name).map(|p| (format!("\\{name}"), p.to_string()))
            }),
        _ => ENVIRONMENT_UNDEFINED
            .captures(&d.message)
            .and_then(|c| {
                let name = c[1].to_string();
                packages::for_environment(&name).map(|p| (format!("The {name} environment"), p.to_string()))
            }),
    };
    if let Some((what, package)) = found {
        d.fix = Some(Fix {
            explanation: format!("{what} comes from the {package} package."),
            command: None,
            package: Some(package),
        });
    }
}

fn error(file: Option<String>, line: Option<u32>, message: String) -> Diagnostic {
    // "LaTeX Error:" adds nothing next to the error badge in the UI.
    let message = message
        .strip_prefix("LaTeX Error: ")
        .map(str::to_string)
        .unwrap_or(message);
    let (kind, hint) = classify_error(&message);
    Diagnostic {
        severity: Severity::Error,
        kind,
        file,
        line,
        message,
        hint,
        fix: None,
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
            Some("The numbering shifted during this build. Recompile once more to settle it.".into()),
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

fn classify_error(msg: &str) -> (Kind, Option<String>) {
    if let Some(c) = MISSING_FILE.captures(msg) {
        let name = c[1].to_string();
        let hint = if name.ends_with(".sty") || name.ends_with(".cls") {
            format!("{name} isn't installed. Install the TeX package that provides it.")
        } else {
            format!("{name} doesn't exist. Check the file name and path.")
        };
        return (Kind::MissingFile { name }, Some(hint));
    }
    let hint = match msg {
        m if m.starts_with("Undefined control sequence") => {
            return (
                Kind::UndefinedControlSequence,
                Some("A command isn't defined. Check its spelling, or load the package that provides it.".into()),
            );
        }
        m if m.starts_with("Missing $ inserted") => {
            "A math command is used outside math mode. Wrap it in $...$."
        }
        m if m.starts_with("Too many }'s") => "There's a closing brace } without a matching {.",
        m if m.starts_with("Missing } inserted") => "A { is never closed.",
        m if m.contains("Environment") && m.contains("undefined") => {
            "This environment doesn't exist. Check the spelling, or load the package that defines it."
        }
        m if m.contains("\\begin{") && m.contains("ended by \\end{") => {
            "\\begin and \\end don't match. Check that every environment is closed in the right order."
        }
        m if m.starts_with("File ended while scanning") => {
            "A brace or argument is never closed before the end of the file."
        }
        m if m.starts_with("Runaway argument") => "An argument is missing its closing brace.",
        _ => return (Kind::Other, None),
    };
    (Kind::Other, Some(hint.into()))
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

static BIBER_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(ERROR|WARN) - (.*)$").unwrap());
static BIBER_SYNTAX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"BibTeX subsystem: .*?([^/\\ ]+?\.bib)_\w+\.utf8, line (\d+), (.*)$").unwrap()
});
static BIBER_ENTRY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"didn't find a database entry for '([^']+)'").unwrap());
static BIBER_MISSING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Cannot find '([^']+)'").unwrap());
static BIBTEX_ENTRY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^Warning--I didn't find a database entry for "([^"]+)""#).unwrap());
static BIBTEX_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^I couldn't open database file (\S+)").unwrap());
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

/// Problems the bibliography tool reported in its `.blg` log (biber or
/// BibTeX): a missing .bib file, a syntax error in an entry, a cited key
/// that isn't in the database.
pub fn parse_blg(blg: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let lines: Vec<&str> = blg.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if let Some(c) = BIBER_LINE.captures(line) {
            let (level, msg) = (&c[1], c[2].trim());
            let severity = if level == "ERROR" { Severity::Error } else { Severity::Warning };
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
                    Kind::UndefinedCitation { key: m[1].to_string() },
                    None,
                    None,
                    format!("Citation '{}' is not in your bibliography.", &m[1]),
                    "No entry with this key in your .bib files. Check the key's spelling.",
                ));
            } else if let Some(m) = BIBER_MISSING.captures(msg) {
                out.push(bib_diag(
                    Severity::Error,
                    Kind::MissingFile { name: m[1].to_string() },
                    None,
                    None,
                    format!("Bibliography file '{}' was not found.", &m[1]),
                    "Check the name in \\addbibresource{} or \\bibliography{}, and that the file is in the project.",
                ));
            } else if severity == Severity::Error {
                out.push(bib_diag(severity, Kind::Other, None, None, msg.to_string(), "Reported by biber."));
            }
        } else if let Some(c) = BIBTEX_ENTRY.captures(line) {
            out.push(bib_diag(
                Severity::Warning,
                Kind::UndefinedCitation { key: c[1].to_string() },
                None,
                None,
                format!("Citation '{}' is not in your bibliography.", &c[1]),
                "No entry with this key in your .bib files. Check the key's spelling.",
            ));
        } else if let Some(c) = BIBTEX_OPEN.captures(line) {
            out.push(bib_diag(
                Severity::Error,
                Kind::MissingFile { name: c[1].to_string() },
                None,
                None,
                format!("Bibliography file '{}' was not found.", &c[1]),
                "Check the name in \\bibliography{}, and that the file is in the project.",
            ));
        } else if let Some(c) = BIBTEX_LINE.captures(line) {
            // The complaint is on the previous line: "I was expecting a `,' or a `}'".
            let what = lines.get(i.wrapping_sub(1)).map(|l| l.trim()).unwrap_or("syntax error");
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
        assert_eq!(d[0].fix.as_ref().unwrap().package.as_deref(), Some("amsmath"));
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
        assert_eq!(d[1].kind, Kind::UndefinedCitation { key: "smith2020".into() });
        assert_eq!(d[2].kind, Kind::MissingFile { name: "missing.bib".into() });
    }

    #[test]
    fn bibtex_log_problems() {
        let blg = "I was expecting a `,' or a `}'\n---line 7 of file refs.bib\nWarning--I didn't find a database entry for \"jones1999\"\nI couldn't open database file nope.bib\n";
        let d = parse_blg(blg);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0].line, Some(7));
        assert_eq!(d[0].file.as_deref(), Some("refs.bib"));
        assert_eq!(d[1].kind, Kind::UndefinedCitation { key: "jones1999".into() });
        assert_eq!(d[2].kind, Kind::MissingFile { name: "nope.bib".into() });
    }
}
