//! Hover cards for `\ref`, `\cite`, `\includegraphics` and `\input` (editor/hover.ts): what the
//! label, key or file points at, without leaving the line.

use super::complete::Lookups;
use crate::theme::Palette;
use crate::writing::render;
use eframe::egui::{self, Color32, FontId, Ui};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Ref,
    Cite,
    Graphic,
    Input,
}

/// What is under the pointer: chars `from..to` of the line.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub kind: Kind,
    pub name: String,
    pub from: usize,
    pub to: usize,
}

static COMMAND: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\([A-Za-z]+)\*?").unwrap());
static REF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[A-Za-z]*ref|eqref|cref|Cref|autoref|pageref|vref|Vref|nameref|Nameref)$")
        .unwrap()
});
static CITE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z]*cite[A-Za-z]*$").unwrap());

/// Index just after the group opened at `i` (`{…}` nested, or `[…]` with braces inside), if closed.
fn skip_group(s: &[char], i: usize, open: char, close: char) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut p = i;
    while p < s.len() {
        let c = s[p];
        if c == '\\' {
            p += 2;
            continue;
        }
        if open == '{' {
            if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth -= 1;
                if depth == 0 {
                    return Some(p + 1);
                }
            }
        } else if c == '{' {
            depth += 1;
        } else if c == '}' {
            depth -= 1;
        } else if c == close && depth <= 0 && p > i {
            return Some(p + 1);
        }
        p += 1;
    }
    None
}

fn comment_start(s: &[char]) -> usize {
    let mut i = 0;
    while i < s.len() {
        if s[i] == '\\' {
            i += 2;
            continue;
        }
        if s[i] == '%' {
            return i;
        }
        i += 1;
    }
    s.len()
}

/// The reference, citation key, image or input file at char `offset` of one line. In a list
/// (`\cite{a,b}`) it is the key under `offset`; on the command itself, the only key.
pub fn target_at(line: &str, offset: usize) -> Option<Target> {
    let all: Vec<char> = line.chars().collect();
    let end = comment_start(&all);
    if offset > end {
        return None;
    }
    let text = &all[..end];
    let s: String = text.iter().collect();
    for m in COMMAND.captures_iter(&s) {
        let cmd = &m[1];
        let kind = if REF.is_match(cmd) && !matches!(cmd, "hyperref" | "href") {
            Kind::Ref
        } else if CITE.is_match(cmd) {
            Kind::Cite
        } else if cmd == "includegraphics" {
            Kind::Graphic
        } else if ["input", "include", "subfile"].contains(&cmd) {
            Kind::Input
        } else {
            continue;
        };
        let start = s[..m.get(0).unwrap().start()].chars().count();
        if offset < start {
            break;
        }
        let mut i = start + m[0].chars().count();
        // Optional arguments.
        loop {
            let ws = text[i..].iter().take_while(|c| c.is_whitespace()).count();
            if text.get(i + ws) != Some(&'[') {
                i += ws;
                break;
            }
            let Some(next) = skip_group(text, i + ws, '[', ']') else {
                break;
            };
            i = next;
        }
        if text.get(i) != Some(&'{') {
            continue;
        }
        let Some(close) = skip_group(text, i, '{', '}') else {
            continue;
        };
        if offset > close {
            continue;
        }
        let inner: String = text[i + 1..close - 1].iter().collect();
        if matches!(kind, Kind::Graphic | Kind::Input) {
            let name = inner.trim().to_owned();
            return (!name.is_empty()).then_some(Target {
                kind,
                name,
                from: start,
                to: close,
            });
        }
        // The items between top-level commas, with where each sits.
        let ic: Vec<char> = inner.chars().collect();
        let mut items: Vec<(String, usize, usize)> = Vec::new();
        let (mut depth, mut from) = (0i32, 0usize);
        for p in 0..=ic.len() {
            let c = ic.get(p).copied();
            if c == Some('{') {
                depth += 1;
            } else if c == Some('}') {
                depth -= 1;
            }
            if p == ic.len() || (c == Some(',') && depth == 0) {
                let raw: String = ic[from..p].iter().collect();
                let name = raw.trim().to_owned();
                if !name.is_empty() {
                    let lead = raw.chars().count() - raw.trim_start().chars().count();
                    let a = i + 1 + from + lead;
                    items.push((name.clone(), a, a + name.chars().count()));
                }
                from = p + 1;
            }
        }
        let hit = items
            .iter()
            .find(|(_, a, b)| offset >= *a && offset <= *b)
            .or_else(|| (offset < i + 1 && items.len() == 1).then(|| &items[0]));
        return hit.map(|(name, a, b)| Target {
            kind,
            name: name.clone(),
            from: *a,
            to: *b,
        });
    }
    None
}

/// The project file a path names: as written, with one of `exts`, or as a suffix.
pub fn resolve_file(path: &str, files: &[String], exts: &[&str]) -> Option<String> {
    let p = path.strip_prefix("./").unwrap_or(path);
    for e in exts {
        let want = format!("{p}{e}");
        if let Some(f) = files
            .iter()
            .find(|f| **f == want)
            .or_else(|| files.iter().find(|f| f.ends_with(&format!("/{want}"))))
        {
            return Some(f.clone());
        }
    }
    None
}

const GRAPHIC_EXTS: &[&str] = &["", ".png", ".jpg", ".jpeg", ".svg", ".pdf", ".gif", ".webp"];
static IMAGE_EXT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\.(png|jpe?g|gif|webp|svg)$").unwrap());
static MATH_ENV: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(equation|displaymath|math|align|alignat|flalign|gather|multline|eqnarray)\*?$")
        .unwrap()
});
static FIGURE_ENV: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(figure|subfigure|wrapfigure|sidewaysfigure)\*?$").unwrap());
static TABLE_ENV: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(table|subtable|sidewaystable)\*?$").unwrap());
static HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^((sub)*section|chapter|part|paragraph)$").unwrap());
static LABELS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\(?:label\{[^}]*\}|nonumber|notag)").unwrap());

/// What to render for an equation environment's body.
pub fn math_source(env: &str, body: &str) -> String {
    let base = env.trim_end_matches('*');
    let mut tex = LABELS.replace_all(body, "").trim().to_owned();
    if base == "alignat" {
        tex = Regex::new(r"^\{\d+\}")
            .unwrap()
            .replace(&tex, "")
            .into_owned();
    }
    match base {
        "align" | "alignat" | "flalign" | "eqnarray" => {
            format!("\\begin{{aligned}}{tex}\\end{{aligned}}")
        }
        "gather" | "multline" => format!("\\begin{{gathered}}{tex}\\end{{gathered}}"),
        _ => tex,
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// A button on a card for a name the project doesn't define.
#[derive(Clone, Debug, PartialEq)]
pub enum Search {
    /// "Search the project" for a missing label.
    Project(String),
    /// "Search online" for a missing citation key.
    Online(String),
}

/// The note for a missing name, with its search button.
fn missing(ui: &mut Ui, p: &Palette, text: String, button: &str, search: Search) -> Option<Search> {
    ui.label(egui::RichText::new(text).size(12.5).color(p.muted));
    ui.add_space(4.0);
    crate::widgets::Button::new(button)
        .kind(crate::widgets::Kind::Ghost)
        .size(crate::widgets::Size::Sm)
        .show(ui, p)
        .clicked()
        .then_some(search)
}

/// Whether any async preview for `t` has settled (loaded or failed), so the hover card's `Area`
/// can re-measure once the full preview replaces the `"Loading preview…"` placeholder.
pub fn preview_ready(
    ctx: &egui::Context,
    t: &Target,
    look: &Lookups,
    root: &std::path::Path,
) -> bool {
    match t.kind {
        Kind::Input => {
            let Some(f) = resolve_file(&t.name, &look.files, &["", ".tex"]) else {
                return true;
            };
            !matches!(input_preview(ctx, &root.join(&f)), Preview::Loading)
        }
        Kind::Graphic => {
            let Some(f) = resolve_file(&t.name, &look.files, GRAPHIC_EXTS) else {
                return true;
            };
            if !IMAGE_EXT.is_match(&f) {
                return true;
            }
            !matches!(
                crate::writing::images::state(ctx, &root.join(&f)),
                crate::writing::images::State::Loading
            )
        }
        Kind::Ref => {
            let Some(l) = look.labels.iter().find(|l| l.label == t.name) else {
                return true;
            };
            if FIGURE_ENV.is_match(&l.env)
                && !l.graphic.is_empty()
                && let Some(f) = resolve_file(&l.graphic, &look.files, GRAPHIC_EXTS)
                && IMAGE_EXT.is_match(&f)
            {
                return !matches!(
                    crate::writing::images::state(ctx, &root.join(&f)),
                    crate::writing::images::State::Loading
                );
            }
            true
        }
        Kind::Cite => true,
    }
}

/// The card's contents, drawn into the tooltip (`.cm-hover-card`).
pub fn card(
    ui: &mut Ui,
    p: &Palette,
    t: &Target,
    look: &Lookups,
    root: &std::path::Path,
) -> Option<Search> {
    ui.spacing_mut().item_spacing.y = 6.0;
    let head = |ui: &mut Ui, icon: crate::icons::Icon, s: String| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            crate::icons::show(ui, icon, 13.0, p.faint);
            ui.label(egui::RichText::new(s).size(12.0).color(p.faint));
        });
    };
    let note = |ui: &mut Ui, s: String| {
        ui.label(egui::RichText::new(s).size(12.5).color(p.muted));
    };
    let file_line = |ui: &mut Ui, s: &str| {
        ui.label(
            egui::RichText::new(s)
                .font(FontId::monospace(12.0))
                .color(p.faint),
        );
    };
    match t.kind {
        Kind::Ref => {
            let Some(l) = look.labels.iter().find(|l| l.label == t.name) else {
                let text = format!("No \\label{{{}}} in the project.", t.name);
                return missing(
                    ui,
                    p,
                    text,
                    "Search the project",
                    Search::Project(t.name.clone()),
                );
            };
            let env = l.env.as_str();
            let (icon, kind) = if MATH_ENV.is_match(env) {
                (crate::icons::SIGMA, "Equation".into())
            } else if FIGURE_ENV.is_match(env) {
                (crate::icons::IMAGE, "Figure".into())
            } else if TABLE_ENV.is_match(env) {
                (crate::icons::TABLE, "Table".into())
            } else if HEADING.is_match(env) || !env.is_empty() {
                (crate::icons::LIST_TREE, cap(env.trim_end_matches('*')))
            } else {
                (crate::icons::LIST_TREE, "Label".into())
            };
            head(ui, icon, format!("{kind} · {}:{}", l.file, l.line));
            if MATH_ENV.is_match(env) && !l.snippet.is_empty() {
                egui::Frame::new()
                    .fill(p.surface_2)
                    .stroke(egui::Stroke::new(1.0, p.border))
                    .corner_radius(crate::theme::R_MD)
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical_centered(|ui| {
                            match render::render(
                                ui.ctx(),
                                &math_source(env, &l.snippet),
                                true,
                                13.0 * 1.1 * 1.21,
                                p.ink,
                            ) {
                                Ok(m) => {
                                    ui.image((m.texture.id(), m.size));
                                }
                                Err(_) => {
                                    ui.label(
                                        egui::RichText::new(&l.snippet)
                                            .font(FontId::monospace(11.5))
                                            .color(p.ink),
                                    );
                                }
                            }
                        });
                    });
            } else if HEADING.is_match(env) {
                let s = if l.snippet.is_empty() {
                    &l.context
                } else {
                    &l.snippet
                };
                ui.label(
                    egui::RichText::new(s)
                        .font(FontId::new(13.0, crate::theme::font::sans_600()))
                        .color(p.ink),
                );
            } else {
                if FIGURE_ENV.is_match(env) && !l.graphic.is_empty() {
                    match resolve_file(&l.graphic, &look.files, GRAPHIC_EXTS) {
                        Some(f) if IMAGE_EXT.is_match(&f) => image(ui, p, &root.join(&f)),
                        f => file_line(ui, &f.unwrap_or_else(|| l.graphic.clone())),
                    }
                }
                let s = if l.snippet.is_empty() {
                    &l.context
                } else {
                    &l.snippet
                };
                if !s.is_empty() {
                    ui.label(egui::RichText::new(s).color(p.ink));
                }
            }
        }
        Kind::Cite => {
            let Some(e) = look.cites.iter().find(|e| e.key == t.name) else {
                return missing(
                    ui,
                    p,
                    "Not in any .bib file.".into(),
                    "Search online",
                    Search::Online(t.name.clone()),
                );
            };
            head(
                ui,
                crate::icons::BOOK_OPEN,
                format!("{} · {}", e.key, e.file),
            );
            ui.label(
                egui::RichText::new(if e.title.is_empty() { &e.key } else { &e.title })
                    .font(FontId::new(13.0, crate::theme::font::sans_600()))
                    .color(p.ink),
            );
            let who = [e.authors.as_str(), e.year.as_str()]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" · ");
            if !who.is_empty() {
                ui.label(egui::RichText::new(who).size(12.5).color(p.muted));
            }
            if !e.kind.is_empty() {
                file_line(ui, &e.kind);
            }
        }
        Kind::Graphic | Kind::Input => {
            let graphic = t.kind == Kind::Graphic;
            let exts: &[&str] = if graphic { GRAPHIC_EXTS } else { &["", ".tex"] };
            let Some(f) = resolve_file(&t.name, &look.files, exts) else {
                note(ui, format!("No file {} in the project.", t.name));
                return None;
            };
            head(
                ui,
                if graphic {
                    crate::icons::IMAGE
                } else {
                    crate::icons::FILE_TEXT
                },
                f.clone(),
            );
            if graphic {
                if IMAGE_EXT.is_match(&f) {
                    image(ui, p, &root.join(&f));
                }
            } else {
                match input_preview(ui.ctx(), &root.join(&f)) {
                    Preview::Ready(first) if !first.is_empty() => {
                        ui.label(
                            egui::RichText::new(first)
                                .font(FontId::monospace(11.5))
                                .color(p.ink),
                        );
                    }
                    Preview::Loading => note(ui, "Loading preview…".into()),
                    Preview::Ready(_) | Preview::Failed => {}
                }
            }
        }
    }
    None
}

enum Preview {
    Loading,
    Ready(String),
    Failed,
}

enum PreviewSlot {
    Loading(
        std::sync::Arc<std::sync::Mutex<Option<Option<String>>>>,
        Option<std::time::SystemTime>,
    ),
    Ready(Option<String>, Option<std::time::SystemTime>),
}

thread_local! {
    static INPUT_CACHE: std::cell::RefCell<std::collections::HashMap<std::path::PathBuf, PreviewSlot>> =
        Default::default();
}

/// Reads the first 8 lines of an `\input` target off the UI thread (UTF-8 or Latin-1) and caches
/// the result keyed by modified time.
fn input_preview(ctx: &egui::Context, path: &std::path::Path) -> Preview {
    let meta = std::fs::metadata(path).ok();
    if !meta.as_ref().is_some_and(|m| m.is_file()) {
        return Preview::Failed;
    }
    let stamp = meta.and_then(|m| m.modified().ok());
    INPUT_CACHE.with_borrow_mut(|cache| {
        if cache.len() > 64 {
            cache.retain(|_, s| matches!(s, PreviewSlot::Loading(..)));
        }
        if let Some(PreviewSlot::Ready(_, then)) = cache.get(path)
            && *then != stamp
        {
            cache.remove(path);
        }
        let slot = cache.entry(path.to_owned()).or_insert_with(|| {
            let cell = std::sync::Arc::new(std::sync::Mutex::new(None));
            let (c, p, ctx) = (cell.clone(), path.to_owned(), ctx.clone());
            std::thread::spawn(move || {
                // Regular file already checked; read a bounded prefix and decode Latin-1 cleanly.
                let text = std::fs::File::open(&p).ok().map(|file| {
                    let mut head = Vec::new();
                    let _ =
                        std::io::Read::read_to_end(&mut std::io::Read::take(file, 4096), &mut head);
                    let src = match std::str::from_utf8(&head) {
                        Ok(s) => s.to_owned(),
                        Err(e) if e.error_len().is_none() => {
                            // Truncated multi-byte UTF-8 sequence at the 4096-byte boundary.
                            String::from_utf8_lossy(&head[..e.valid_up_to()]).into_owned()
                        }
                        Err(_) => head.into_iter().map(|b| b as char).collect(),
                    };
                    src.replace("\r\n", "\n")
                        .split('\n')
                        .take(8)
                        .collect::<Vec<_>>()
                        .join("\n")
                });
                *c.lock().unwrap_or_else(|e| e.into_inner()) = Some(text);
                ctx.request_repaint();
            });
            PreviewSlot::Loading(cell, stamp)
        });
        if let PreviewSlot::Loading(cell, then) = slot {
            let done = cell.lock().unwrap_or_else(|e| e.into_inner()).take();
            match done {
                Some(text) => *slot = PreviewSlot::Ready(text, *then),
                None => return Preview::Loading,
            }
        }
        match slot {
            PreviewSlot::Ready(Some(t), _) => Preview::Ready(t.clone()),
            PreviewSlot::Ready(None, _) => Preview::Failed,
            PreviewSlot::Loading(..) => Preview::Loading,
        }
    })
}

/// `.cm-hover-img`: at most 240 px, on white, read and decoded off the UI thread.
fn image(ui: &mut Ui, p: &Palette, path: &std::path::Path) {
    match crate::writing::images::state(ui.ctx(), path) {
        crate::writing::images::State::Ready(tex) => {
            let size = tex.size_vec2() / ui.ctx().pixels_per_point();
            let k = (240.0 / size.x).min(240.0 / size.y).min(1.0);
            ui.add_space(2.0);
            let r = ui
                .add(egui::Image::new((tex.id(), size * k)).bg_fill(Color32::WHITE))
                .rect;
            ui.painter().rect_stroke(
                r,
                4,
                egui::Stroke::new(1.0, p.border),
                egui::StrokeKind::Outside,
            );
        }
        crate::writing::images::State::Loading => {
            ui.label(egui::RichText::new("Loading preview…").color(p.muted));
        }
        crate::writing::images::State::Failed => {
            ui.label(egui::RichText::new("Could not load image.").color(p.muted));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mouse sits at `|`.
    fn at(src: &str) -> Option<Target> {
        let i = src.chars().position(|c| c == '|').unwrap();
        target_at(&src.replacen('|', "", 1), i)
    }
    fn name(t: Option<Target>) -> Option<String> {
        t.map(|t| t.name)
    }

    // hover.test.ts
    #[test]
    fn finds_references_and_keys() {
        assert_eq!(
            at("see \\eq|ref{eq:a} here").map(|t| (t.kind, t.name)),
            Some((Kind::Ref, "eq:a".into()))
        );
        assert_eq!(
            at("see \\ref{eq|:a} here").map(|t| (t.from, t.to)),
            Some((9, 13))
        );
        assert_eq!(name(at("see \\Cref*{a|}")), Some("a".into()));
        assert_eq!(name(at("\\cite{alpha, be|ta,gamma}")), Some("beta".into()));
        assert_eq!(name(at("\\cite{alpha, beta,ga|mma}")), Some("gamma".into()));
        assert_eq!(name(at("\\cite{|alpha,beta}")), Some("alpha".into()));
        assert_eq!(
            at("\\cref{a,b|}").map(|t| (t.kind, t.name)),
            Some((Kind::Ref, "b".into()))
        );
        assert_eq!(at("\\ci|te{a,b}"), None);
        assert_eq!(name(at("\\ci|te{a}")), Some("a".into()));
    }

    #[test]
    fn skips_optional_arguments_and_finds_nested_commands() {
        assert_eq!(
            name(at("\\textcite[see {\\S 3}][p. 4]{k|ey}")),
            Some("key".into())
        );
        assert_eq!(name(at("\\autocite[{[x]}]{k|}")), Some("k".into()));
        assert_eq!(name(at("\\textbf{see \\ref{x|}} and")), Some("x".into()));
        assert_eq!(
            name(at("\\emph{a \\cite{one}, \\cite{tw|o}}")),
            Some("two".into())
        );
    }

    #[test]
    fn reads_images_and_inputs_whole_and_ignores_the_rest() {
        assert!(IMAGE_EXT.is_match("figure.svg"));
        assert_eq!(
            at("\\includegraphics[width=.5\\textwidth]{fi|gs/a.png}").map(|t| (t.kind, t.name)),
            Some((Kind::Graphic, "figs/a.png".into()))
        );
        assert_eq!(
            name(at("\\includegraphics{a_{1}|.png}")),
            Some("a_{1}.png".into())
        );
        assert_eq!(
            at("\\inp|ut{chapters/intro}").map(|t| (t.kind, t.name)),
            Some((Kind::Input, "chapters/intro".into()))
        );
        assert_eq!(name(at("\\include{ch|2}")), Some("ch2".into()));
        for s in [
            "plain| text",
            "\\label{a|}",
            "\\hyperref[a]{te|xt}",
            "\\ref{a} aft|er",
            "\\ref{unclosed|",
            "text % \\ref{a|}",
        ] {
            assert_eq!(at(s), None, "{s}");
        }
        assert_eq!(name(at("50\\% \\ref{a|}")), Some("a".into()));
    }

    #[test]
    fn resolves_files_and_math_sources() {
        let files: Vec<String> = ["main.tex", "figs/a.png", "figs/b.pdf", "chapters/intro.tex"]
            .map(String::from)
            .to_vec();
        assert_eq!(
            resolve_file("figs/a.png", &files, &[""]).as_deref(),
            Some("figs/a.png")
        );
        assert_eq!(
            resolve_file("a", &files, &["", ".png"]).as_deref(),
            Some("figs/a.png")
        );
        assert_eq!(
            resolve_file("./chapters/intro", &files, &["", ".tex"]).as_deref(),
            Some("chapters/intro.tex")
        );
        assert_eq!(resolve_file("nope", &files, &["", ".tex"]), None);
        assert_eq!(
            math_source("align*", "a &= b \\label{x} \\\\ c &= d \\nonumber"),
            "\\begin{aligned}a &= b  \\\\ c &= d\\end{aligned}"
        );
        assert_eq!(math_source("equation", "E=mc^2"), "E=mc^2");
        assert_eq!(
            math_source("alignat", "{2}a&=b"),
            "\\begin{aligned}a&=b\\end{aligned}"
        );
    }
}
