//! Project-wide editor tools: the TODO list and whole-document formatting.

use crate::{AppState, CmdResult, LockSafe, project};
use regex::Regex;
use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tauri::State;

const MAX_TEXT: usize = 120;
const FORMAT_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Todo {
    /// Project-relative, forward slashes.
    pub file: String,
    pub line: usize,
    /// "TODO", "FIXME" or "XXX" for comments; "TODO" for `\todo` notes.
    pub kind: String,
    pub text: String,
}

/// Everything before the first unescaped `%`.
fn code_part(line: &str) -> &str {
    let b = line.as_bytes();
    (0..b.len())
        .find(|&i| b[i] == b'%' && (i == 0 || b[i - 1] != b'\\'))
        .map_or(line, |i| &line[..i])
}

/// The text up to the `}` that closes an already-opened brace, or the end of the line.
fn braced(s: &str) -> &str {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' if depth == 0 => return &s[..i],
            '}' => depth -= 1,
            _ => {}
        }
    }
    s
}

fn clip(text: &str) -> String {
    let t = text.trim();
    if t.chars().count() <= MAX_TEXT {
        return t.to_string();
    }
    format!(
        "{}…",
        t.chars().take(MAX_TEXT).collect::<String>().trim_end()
    )
}

/// The todos in one file's text. Line numbers start at 1.
pub fn scan_text(file: &str, text: &str) -> Vec<Todo> {
    let comment = Regex::new(r"(?i)(?:^|[^\\])%\s*(todo|fixme|xxx)\b:?\s*(.*)$").unwrap();
    let note = Regex::new(r"\\todo(?:\[[^\]]*\])?\s*\{").unwrap();
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line_no = i + 1;
        if let Some(c) = comment.captures(line) {
            out.push(Todo {
                file: file.to_string(),
                line: line_no,
                kind: c[1].to_uppercase(),
                text: clip(&c[2]),
            });
        }
        let code = code_part(line);
        for m in note.find_iter(code) {
            out.push(Todo {
                file: file.to_string(),
                line: line_no,
                kind: "TODO".into(),
                text: clip(braced(&code[m.end()..])),
            });
        }
    }
    out
}

fn scan_entries(root: &Path, entries: &[lemmata_project::Entry], out: &mut Vec<Todo>) {
    for e in entries {
        if e.is_dir {
            scan_entries(root, &e.children, out);
        } else if Path::new(&e.name).extension().is_some_and(|x| x == "tex")
            && let Ok(text) = std::fs::read_to_string(root.join(&e.path))
        {
            out.extend(scan_text(&e.path, &text));
        }
    }
}

/// Every TODO, FIXME, XXX comment and `\todo` note in the project's `.tex`
/// files, sorted by file and line. The build folder is skipped.
pub fn scan_project(root: &Path, outdir: &str) -> Vec<Todo> {
    let mut out = Vec::new();
    scan_entries(root, &lemmata_project::tree(root, outdir), &mut out);
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[tauri::command(async)]
pub(crate) fn todos(state: State<AppState>) -> CmdResult<Vec<Todo>> {
    let p = project(&state)?;
    Ok(scan_project(&p.root, &p.config.outdir))
}

/// `indent` is the indent unit as typed: a tab, or 1 to 8 spaces.
fn check_indent(indent: &str) -> CmdResult<()> {
    let spaces = !indent.is_empty() && indent.len() <= 8 && indent.bytes().all(|b| b == b' ');
    if indent == "\t" || spaces {
        Ok(())
    } else {
        Err("The indent must be a tab or 1 to 8 spaces.".into())
    }
}

/// `latexindent - ` reads stdin, writes stdout. `-g` keeps it from writing indent.log.
fn latexindent_args(indent: &str) -> Vec<String> {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    // The YAML value: a double-quoted escape for a tab (the value in latexindent's own defaults), quoted spaces otherwise.
    let unit = if indent == "\t" {
        "\"\\t\"".to_string()
    } else {
        format!("'{indent}'")
    };
    vec![
        "-g".into(),
        null.into(),
        format!("-y=defaultIndent: {unit}"),
        "-".into(),
    ]
}

/// `tex-fmt --stdin` reads stdin, writes stdout; `--tabsize` sets the indent width in spaces.
fn tex_fmt_args(width: usize) -> Vec<String> {
    vec!["--stdin".into(), "--tabsize".into(), width.to_string()]
}

/// A tool from the detected TeX folder, else from PATH.
fn find_tool(name: &str, bin_dir: Option<&Path>) -> Option<PathBuf> {
    bin_dir
        .map(|d| d.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
        .filter(|p| p.is_file())
        .or_else(|| which::which(name).ok())
}

/// Runs `cmd` with `input` on stdin and returns stdout, or stderr as the error.
fn run(mut cmd: Command, input: &str, timeout: Duration) -> Result<String, String> {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start the formatter: {e}"))?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    // Each pipe gets its own thread so a full pipe can't stall the child, and the timeout can still fire.
    let text = input.to_owned();
    std::thread::spawn(move || stdin.write_all(text.as_bytes()));
    let out = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stdout.read_to_end(&mut b);
        b
    });
    let err = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stderr.read_to_end(&mut b);
        b
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "Formatting took longer than {} s and was stopped.",
                    timeout.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("Formatting failed: {e}")),
        }
    };
    let stdout = out.join().unwrap_or_default();
    if status.success() {
        return Ok(String::from_utf8_lossy(&stdout).into_owned());
    }
    let stderr = err.join().unwrap_or_default();
    let msg = String::from_utf8_lossy(&stderr);
    Err(match msg.trim() {
        "" => format!("The formatter failed ({status})."),
        m => format!("The formatter failed: {m}"),
    })
}

/// tex-fmt if present (and the indent is spaces), else latexindent.
fn format_with(bin_dir: Option<&Path>, text: &str, indent: &str) -> CmdResult<String> {
    check_indent(indent)?;
    if indent != "\t"
        && let Some(tex_fmt) = find_tool("tex-fmt", bin_dir)
    {
        let mut cmd = Command::new(tex_fmt);
        cmd.args(tex_fmt_args(indent.len()));
        return run(cmd, text, FORMAT_TIMEOUT);
    }
    if let Some(latexindent) = find_tool("latexindent", bin_dir) {
        let mut cmd = Command::new(latexindent);
        cmd.args(latexindent_args(indent));
        return run(cmd, text, FORMAT_TIMEOUT);
    }
    Err("Formatting needs latexindent (part of TeX Live) or tex-fmt.".into())
}

#[tauri::command(async)]
pub(crate) fn format_tex(
    state: State<AppState>,
    text: String,
    indent: String,
) -> CmdResult<String> {
    let bin_dir = state
        .detection
        .lock_safe()
        .as_ref()
        .and_then(|d| d.bin_dir.clone());
    format_with(bin_dir.as_deref(), &text, &indent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(usize, String, String)> {
        scan_text("a.tex", text)
            .into_iter()
            .map(|t| (t.line, t.kind, t.text))
            .collect()
    }

    #[test]
    fn finds_comment_markers_case_insensitively() {
        assert_eq!(
            kinds("x\n% TODO: fix this\n%fixme later\n  % xxx\n"),
            vec![
                (2, "TODO".into(), "fix this".into()),
                (3, "FIXME".into(), "later".into()),
                (4, "XXX".into(), "".into()),
            ]
        );
    }

    #[test]
    fn ignores_escaped_percent_longer_words_and_commented_notes() {
        assert!(kinds(r"100\% TODO not a comment").is_empty());
        assert!(kinds("% TODOS are a list").is_empty());
        assert!(kinds(r"% \todo{commented out}").is_empty());
    }

    #[test]
    fn finds_todonotes_commands() {
        assert_eq!(
            kinds(r"\todo[inline]{Write intro} and \todo{a {nested} b}"),
            vec![
                (1, "TODO".into(), "Write intro".into()),
                (1, "TODO".into(), "a {nested} b".into()),
            ]
        );
        assert!(kinds(r"\todonotes is not a note").is_empty());
    }

    #[test]
    fn clips_long_text() {
        let long = format!("% TODO {}", "x".repeat(300));
        let t = &scan_text("a.tex", &long)[0].text;
        assert_eq!(t.chars().count(), MAX_TEXT + 1);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn project_scan_skips_build_and_hidden_folders() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        for (rel, text) in [
            ("main.tex", "\\documentclass{article}\n% TODO one\n"),
            ("sections/b.tex", "\\todo{two}\n"),
            (".lemmata/build/c.tex", "% TODO build\n"),
            (".hidden.tex", "% TODO hidden\n"),
            ("notes.txt", "% TODO not tex\n"),
        ] {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        let found: Vec<(String, usize)> = scan_project(root, ".lemmata/build")
            .into_iter()
            .map(|t| (t.file, t.line))
            .collect();
        assert_eq!(
            found,
            vec![("main.tex".into(), 2), ("sections/b.tex".into(), 1)]
        );
    }

    #[test]
    fn validates_indent() {
        assert!(check_indent("\t").is_ok());
        assert!(check_indent("    ").is_ok());
        assert!(check_indent("").is_err());
        assert!(check_indent("'; x").is_err());
        assert!(check_indent("         ").is_err());
    }

    #[test]
    fn builds_latexindent_and_tex_fmt_arguments() {
        let args = latexindent_args("    ");
        assert_eq!(args.last().map(String::as_str), Some("-"));
        assert!(args.contains(&"-y=defaultIndent: '    '".to_string()));
        assert!(latexindent_args("\t").contains(&"-y=defaultIndent: \"\\t\"".to_string()));
        assert_eq!(tex_fmt_args(2), ["--stdin", "--tabsize", "2"]);
    }

    /// Stdin in, stdout out, on a unix `cat`. Skipped elsewhere.
    #[cfg(unix)]
    #[test]
    fn run_pipes_stdin_to_stdout() {
        let out = run(Command::new("cat"), "a\nb\n", FORMAT_TIMEOUT).unwrap();
        assert_eq!(out, "a\nb\n");
    }

    #[cfg(unix)]
    #[test]
    fn run_stops_a_slow_formatter() {
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        let started = Instant::now();
        let err = run(cmd, "", Duration::from_millis(100)).unwrap_err();
        assert!(err.contains("stopped"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    /// Formats a badly indented document with the installed latexindent. Skipped when it
    /// isn't installed or can't run (e.g. its Perl modules are missing).
    #[test]
    fn latexindent_formats_when_installed() {
        let Some(bin) = find_tool("latexindent", None) else {
            return;
        };
        if !Command::new(&bin)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
        {
            return;
        }
        let src = "\\begin{itemize}\n\\item a\n  \\item b\n\\end{itemize}\n";
        let out = format_with(None, src, "    ").unwrap();
        assert!(out.contains("    \\item a"), "{out}");
    }
}
