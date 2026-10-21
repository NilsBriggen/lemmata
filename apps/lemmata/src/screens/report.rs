//! "Report a problem" (report.ts): a GitHub issue prefilled with what helps diagnose it, shown to
//! the user first. File paths are removed, since they often contain names.

use crate::theme::Palette;
use crate::widgets::{self, Button, Kind};
use eframe::egui::{self, FontId, Margin, RichText, Stroke, Ui, vec2};
use lemmata_log::{Diagnostic, Severity};
use lemmata_tex::detect::Distribution;
use regex::Regex;
use std::sync::LazyLock;

pub const ISSUES: &str = "https://github.com/NilsBriggen/lemmata/issues/new";

/// Replaces absolute paths (Unix, Windows, ~) with <path>.
pub fn scrub_paths(text: &str) -> String {
    static WIN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?:[A-Za-z]:\\|\\\\)[^\s'"`]+"#).unwrap());
    static UNIX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?:~|/(?:home|Users|tmp|var|usr|opt|mnt|media|Volumes))/[^\s'"`]+"#).unwrap()
    });
    UNIX.replace_all(&WIN.replace_all(text, "<path>"), "<path>")
        .into_owned()
}

pub fn system_summary(version: &str, tex: Option<&lemmata_shell::TexStatus>) -> String {
    let dist = match tex.map(|t| &t.detection.distribution) {
        Some(Distribution::TexLive { year }) => {
            format!("TeX Live {}", year.map_or("?".into(), |y| y.to_string()))
        }
        Some(Distribution::Miktex { version }) => {
            format!("MiKTeX {}", version.as_deref().unwrap_or("?"))
        }
        _ => "unknown TeX".into(),
    };
    let missing: Vec<&str> = tex
        .map(|t| {
            t.detection
                .tools
                .iter()
                .filter(|x| x.path.is_none())
                .map(|x| x.name.as_str())
                .collect()
        })
        .unwrap_or_default();
    let os = tex.map_or(std::env::consts::OS.to_owned(), |t| t.os.name.clone());
    let missing = if missing.is_empty() {
        String::new()
    } else {
        format!(" (missing: {})", missing.join(", "))
    };
    format!("Lemmata {version}\nSystem: {os}\nTeX: {dist}{missing}")
}

/// The first five errors, with project-relative places.
pub fn error_summary(diagnostics: &[Diagnostic]) -> String {
    let lines: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .take(5)
        .map(|d| {
            let place = match (&d.file, d.line) {
                (Some(f), l) => format!(
                    "{}{}: ",
                    f.strip_prefix("./").unwrap_or(f),
                    l.map(|l| format!(":{l}")).unwrap_or_default()
                ),
                (None, _) => String::new(),
            };
            format!("{place}{}", d.message)
        })
        .collect();
    scrub_paths(&lines.join("\n"))
}

pub fn issue_url(system: &str, log: &str) -> String {
    url::Url::parse_with_params(
        ISSUES,
        &[("template", "bug.yml"), ("system", system), ("log", log)],
    )
    .map(|u| u.to_string())
    .unwrap_or_else(|_| ISSUES.into())
}

pub enum Out {
    Close,
    Open(String),
}

/// The dialog: the details in a box, then "Open GitHub".
pub fn show(ctx: &egui::Context, p: &Palette, system: &str, log: &str) -> Option<Out> {
    let (mut open, mut cancel) = (false, false);
    let r = widgets::modal_with(
        ctx,
        p,
        "Report a problem",
        Some(
            "Opens a new GitHub issue with these details. You can edit everything before submitting; file paths are left out.",
        ),
        560.0,
        |ui| {
            egui::Frame::new()
                .fill(p.surface_2)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(crate::theme::R)
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let text = if log.is_empty() {
                        system.to_owned()
                    } else {
                        format!("{system}\n\n{log}")
                    };
                    let w = ui.available_width();
                    ui.allocate_ui(vec2(w, 260.0), |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(260.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(text)
                                            .font(FontId::monospace(12.0))
                                            .color(p.ink),
                                    )
                                    .wrap()
                                    .selectable(true),
                                );
                            });
                    });
                });
        },
        Some(&mut |ui: &mut Ui| {
            if Button::new("Open GitHub")
                .kind(Kind::Primary)
                .show(ui, p)
                .clicked()
            {
                open = true;
            }
            if Button::new("Cancel").show(ui, p).clicked() {
                cancel = true;
            }
        }),
    );
    if open {
        Some(Out::Open(issue_url(system, log)))
    } else if r.closed || cancel {
        Some(Out::Close)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // report.test.ts
    #[test]
    fn reports_leave_paths_out() {
        assert_eq!(
            scrub_paths("File /home/nils/thesis/main.tex not found"),
            "File <path> not found"
        );
        assert_eq!(scrub_paths("at C:\\Users\\Nils\\paper\\a.tex"), "at <path>");
        assert_eq!(
            scrub_paths("see ~/texlive/2026 and ./chapters/a.tex"),
            "see <path> and ./chapters/a.tex"
        );
        let d = |severity, file: Option<&str>, line, message: &str| Diagnostic {
            severity,
            kind: lemmata_log::Kind::Other,
            file: file.map(String::from),
            line,
            message: message.into(),
            hint: None,
            fix: None,
        };
        let s = error_summary(&[
            d(
                Severity::Error,
                Some("./main.tex"),
                Some(4),
                "Undefined control sequence.",
            ),
            d(Severity::Warning, None, None, "ignored"),
        ]);
        assert_eq!(s, "main.tex:4: Undefined control sequence.");
        let u = url::Url::parse(&issue_url("Lemmata 0.1.0", "x")).unwrap();
        let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
        assert_eq!(q["template"], "bug.yml");
        assert_eq!(q["system"], "Lemmata 0.1.0");
    }
}
