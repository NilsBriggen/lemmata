//! Completion (editor/completions.ts, data.ts, mathCommands.ts, snippets.ts): environments,
//! commands, maths symbols with previews, labels, citation keys, file paths, packages, the
//! user's snippets and texlab's extra commands, ranked like CodeMirror's autocompletion.

use super::buffer::Doc;
use crate::store::Snippet;
use crate::writing::{math, symbols};
use regex::Regex;
use std::sync::LazyLock;

/// The icon before an item (CodeMirror's completion types).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Function,
    Class,
    Constant,
    Text,
    File,
    Namespace,
    Keyword,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Apply {
    /// A template with `${}` fields.
    Snippet(String),
    Text(String),
    /// `\begin{`, then the list of environment names opens.
    Begin,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub label: String,
    /// Shown instead of the label (texlab's commands come without their backslash).
    pub display: Option<String>,
    pub detail: Option<String>,
    /// A note in the side panel.
    pub info: Option<String>,
    /// Maths to render beside the label and in the side panel.
    pub preview: Option<String>,
    pub kind: Kind,
    pub boost: i32,
    pub apply: Apply,
    /// The document should load this package for what was inserted.
    pub pkg: Option<String>,
    /// The span this item replaces.
    pub from: usize,
    pub to: usize,
}

impl Item {
    fn new(label: impl Into<String>, kind: Kind, apply: Apply) -> Self {
        Self {
            label: label.into(),
            display: None,
            detail: None,
            info: None,
            preview: None,
            kind,
            boost: 0,
            apply,
            pkg: None,
            from: 0,
            to: 0,
        }
    }
    fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = Some(d.into());
        self
    }
    fn boost(mut self, b: i32) -> Self {
        self.boost = b;
        self
    }
}

/// texlab's command names (with their detail) for a file and cursor, and where the name starts.
pub type ServerItems = (String, usize, usize, Vec<(String, Option<String>)>);

/// What completion can look up about the project.
#[derive(Default)]
pub struct Lookups {
    pub labels: Vec<lemmata_project::extras::Label>,
    pub cites: Vec<lemmata_bib::Entry>,
    pub packages: Vec<String>,
    /// Project-relative file paths.
    pub files: Vec<String>,
    pub snippets: Vec<Snippet>,
    /// Bumped when any of these change, so an open list is computed again.
    pub generation: u64,
    /// The project folder, for hover cards that show a file.
    pub root: std::path::PathBuf,
    /// texlab's command completions: file, cursor, where the name starts, and the names.
    pub server: Option<ServerItems>,
    /// texlab's hover note: file, line, column (0-based chars) and text.
    pub hover: Option<(String, usize, usize, String)>,
}

// ── Vocabulary (data.ts) ───────────────────────────────────────────────────

pub struct Env {
    pub name: &'static str,
    pub body: String,
    pub detail: &'static str,
    pub pkg: Option<&'static str>,
    pub args: &'static str,
}

const TABLE_ROWS: &str = "\\toprule\n${Header 1} & ${Header 2} & ${Header 3} \\\\\n\\midrule\n${Cell 1.1} & ${Cell 1.2} & ${Cell 1.3} \\\\\n${Cell 2.1} & ${Cell 2.2} & ${Cell 2.3} \\\\\n\\bottomrule";

fn indented(text: &str) -> String {
    text.split('\n')
        .map(|l| format!("\t{l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn environments() -> &'static [Env] {
    static ALL: LazyLock<Vec<Env>> = LazyLock::new(|| {
        let e = |name, body: &str, detail, pkg| Env {
            name,
            body: body.into(),
            detail,
            pkg,
            args: "",
        };
        let mut v = vec![
            e("itemize", "\\item ${}", "bulleted list", None),
            e("enumerate", "\\item ${}", "numbered list", None),
            e("description", "\\item[${}] ", "term list", None),
            e(
                "figure",
                "\\centering\n\\includegraphics[width=0.8\\linewidth]{${}}\n\\caption{${}}\n\\label{fig:${}}",
                "float with caption",
                Some("graphicx"),
            ),
            Env {
                name: "table",
                args: "[${htbp}]",
                body: format!(
                    "\\centering\n\\begin{{tabular}}{{${{lll}}}}\n{}\n\\end{{tabular}}\n\\caption{{${{Caption}}}}\n\\label{{tab:${{label}}}}",
                    indented(TABLE_ROWS)
                ),
                detail: "float with a sample table",
                pkg: Some("booktabs"),
            },
            Env {
                name: "tabular",
                args: "{${lll}}",
                body: TABLE_ROWS.into(),
                detail: "a sample table",
                pkg: Some("booktabs"),
            },
            e("equation", "${}", "numbered equation", Some("amsmath")),
            e("equation*", "${}", "unnumbered equation", Some("amsmath")),
            e(
                "align",
                "${} &= ${} \\\\",
                "aligned equations",
                Some("amsmath"),
            ),
            e(
                "align*",
                "${} &= ${} \\\\",
                "aligned, unnumbered",
                Some("amsmath"),
            ),
            e("gather", "${}", "centered equations", Some("amsmath")),
            e("multline", "${}", "long equation", Some("amsmath")),
            e(
                "split",
                "${} &= ${} \\\\",
                "split one equation",
                Some("amsmath"),
            ),
            e(
                "cases",
                "${} & \\text{if } ${} \\\\",
                "piecewise",
                Some("amsmath"),
            ),
            e("matrix", "${} & ${} \\\\", "matrix", Some("amsmath")),
            e(
                "pmatrix",
                "${} & ${} \\\\",
                "matrix with ( )",
                Some("amsmath"),
            ),
            e(
                "bmatrix",
                "${} & ${} \\\\",
                "matrix with [ ]",
                Some("amsmath"),
            ),
            e("theorem", "${}", "theorem", Some("amsthm")),
            e("lemma", "${}", "lemma", Some("amsthm")),
            e("proof", "${}", "proof", Some("amsthm")),
            e("definition", "${}", "definition", Some("amsthm")),
            e("abstract", "${}", "abstract", None),
            e("center", "${}", "centered block", None),
            e("quote", "${}", "indented quote", None),
            e("verbatim", "${}", "literal text", None),
            Env {
                name: "minipage",
                args: "{${0.5\\linewidth}}",
                body: "${}".into(),
                detail: "box of fixed width",
                pkg: None,
            },
            e("frame", "\\frametitle{${}}\n${}", "beamer slide", None),
            e(
                "columns",
                "\\column{0.5\\textwidth}\n${}",
                "beamer columns",
                None,
            ),
            e("tikzpicture", "${}", "TikZ drawing", Some("tikz")),
            e("lstlisting", "${}", "code listing", Some("listings")),
            e(
                "thebibliography",
                "\\bibitem{${}} ${}",
                "manual bibliography",
                None,
            ),
            e("appendix", "${}", "appendix", None),
        ];
        v.shrink_to_fit();
        v
    });
    &ALL
}

/// The snippet after `\begin{`: the name, any arguments, the body one level in, and the `\end`.
pub fn env_snippet(e: &Env) -> String {
    format!(
        "{}}}{}\n{}\n\\end{{{}}}",
        e.name,
        e.args,
        indented(&e.body),
        e.name
    )
}

/// (label, template, detail)
const COMMANDS: &[(&str, &str, &str)] = &[
    ("\\begin", "\\begin{", "environment"),
    ("\\section", "\\section{${}}", ""),
    ("\\subsection", "\\subsection{${}}", ""),
    ("\\subsubsection", "\\subsubsection{${}}", ""),
    ("\\paragraph", "\\paragraph{${}}", ""),
    ("\\chapter", "\\chapter{${}}", ""),
    ("\\part", "\\part{${}}", ""),
    ("\\title", "\\title{${}}", ""),
    ("\\author", "\\author{${}}", ""),
    ("\\date", "\\date{${}}", ""),
    ("\\maketitle", "\\maketitle", ""),
    ("\\tableofcontents", "\\tableofcontents", ""),
    ("\\listoffigures", "\\listoffigures", ""),
    ("\\listoftables", "\\listoftables", ""),
    ("\\appendix", "\\appendix", ""),
    ("\\textbf", "\\textbf{${}}", "bold"),
    ("\\textit", "\\textit{${}}", "italic"),
    ("\\texttt", "\\texttt{${}}", "monospace"),
    ("\\textsc", "\\textsc{${}}", "small caps"),
    ("\\textrm", "\\textrm{${}}", "roman"),
    ("\\textsf", "\\textsf{${}}", "sans serif"),
    ("\\emph", "\\emph{${}}", "emphasis"),
    ("\\underline", "\\underline{${}}", ""),
    ("\\textcolor", "\\textcolor{${}}{${}}", "xcolor"),
    ("\\footnote", "\\footnote{${}}", ""),
    ("\\label", "\\label{${}}", ""),
    ("\\ref", "\\ref{${}}", ""),
    ("\\eqref", "\\eqref{${}}", "amsmath"),
    ("\\pageref", "\\pageref{${}}", ""),
    ("\\autoref", "\\autoref{${}}", "hyperref"),
    ("\\cref", "\\cref{${}}", "cleveref"),
    ("\\Cref", "\\Cref{${}}", "cleveref"),
    ("\\cite", "\\cite{${}}", ""),
    ("\\citep", "\\citep{${}}", "natbib"),
    ("\\citet", "\\citet{${}}", "natbib"),
    ("\\parencite", "\\parencite{${}}", "biblatex"),
    ("\\textcite", "\\textcite{${}}", "biblatex"),
    ("\\bibliography", "\\bibliography{${}}", ""),
    ("\\bibliographystyle", "\\bibliographystyle{${}}", ""),
    ("\\addbibresource", "\\addbibresource{${}}", "biblatex"),
    ("\\printbibliography", "\\printbibliography", "biblatex"),
    ("\\usepackage", "\\usepackage{${}}", ""),
    ("\\documentclass", "\\documentclass{${}}", ""),
    ("\\input", "\\input{${}}", ""),
    ("\\include", "\\include{${}}", ""),
    (
        "\\includegraphics",
        "\\includegraphics[width=\\linewidth]{${}}",
        "graphicx",
    ),
    ("\\caption", "\\caption{${}}", ""),
    ("\\centering", "\\centering", ""),
    ("\\item", "\\item ${}", ""),
    ("\\frac", "\\frac{${}}{${}}", "fraction"),
    ("\\dfrac", "\\dfrac{${}}{${}}", "display fraction"),
    ("\\sqrt", "\\sqrt{${}}", ""),
    ("\\sum", "\\sum_{${}}^{${}}", ""),
    ("\\prod", "\\prod_{${}}^{${}}", ""),
    ("\\int", "\\int_{${}}^{${}}", ""),
    ("\\lim", "\\lim_{${} \\to ${}}", ""),
    ("\\partial", "\\partial", ""),
    ("\\infty", "\\infty", ""),
    ("\\mathbb", "\\mathbb{${}}", "amssymb"),
    ("\\mathcal", "\\mathcal{${}}", ""),
    ("\\mathbf", "\\mathbf{${}}", ""),
    ("\\mathrm", "\\mathrm{${}}", ""),
    ("\\text", "\\text{${}}", "amsmath"),
    ("\\left", "\\left${}", ""),
    ("\\right", "\\right${}", ""),
    ("\\vec", "\\vec{${}}", ""),
    ("\\hat", "\\hat{${}}", ""),
    ("\\bar", "\\bar{${}}", ""),
    ("\\tilde", "\\tilde{${}}", ""),
    ("\\overline", "\\overline{${}}", ""),
    ("\\underbrace", "\\underbrace{${}}_{${}}", ""),
    ("\\boxed", "\\boxed{${}}", "amsmath"),
    ("\\operatorname", "\\operatorname{${}}", "amsmath"),
    ("\\newcommand", "\\newcommand{\\${}}[${}]{${}}", ""),
    ("\\renewcommand", "\\renewcommand{\\${}}{${}}", ""),
    ("\\newtheorem", "\\newtheorem{${}}{${}}", "amsthm"),
    (
        "\\DeclareMathOperator",
        "\\DeclareMathOperator{\\${}}{${}}",
        "amsmath",
    ),
    ("\\hspace", "\\hspace{${}}", ""),
    ("\\vspace", "\\vspace{${}}", ""),
    ("\\newpage", "\\newpage", ""),
    ("\\clearpage", "\\clearpage", ""),
    ("\\noindent", "\\noindent", ""),
    ("\\url", "\\url{${}}", "hyperref"),
    ("\\href", "\\href{${}}{${}}", "hyperref"),
    ("\\hline", "\\hline", ""),
    ("\\toprule", "\\toprule", "booktabs"),
    ("\\midrule", "\\midrule", "booktabs"),
    ("\\bottomrule", "\\bottomrule", "booktabs"),
    ("\\multicolumn", "\\multicolumn{${}}{${}}{${}}", ""),
    ("\\SI", "\\SI{${}}{${}}", "siunitx"),
    ("\\num", "\\num{${}}", "siunitx"),
    ("\\qty", "\\qty{${}}{${}}", "siunitx"),
    ("\\todo", "\\todo{${}}", "todonotes"),
    ("\\frametitle", "\\frametitle{${}}", "beamer"),
    ("\\pause", "\\pause", "beamer"),
];

// ── Maths (mathCommands.ts) ────────────────────────────────────────────────

/// (label, template, preview, detail or group)
const STRUCTURES: &[(&str, &str, &str, &str)] = &[
    (
        "\\sum",
        "\\sum_{${}}^{${}} ${}",
        "\\sum_{i=1}^{n}",
        "big operators",
    ),
    (
        "\\prod",
        "\\prod_{${}}^{${}} ${}",
        "\\prod_{i=1}^{n}",
        "big operators",
    ),
    (
        "\\int",
        "\\int_{${}}^{${}} ${} \\,\\mathrm{d}${}",
        "\\int_{a}^{b} f \\,\\mathrm{d}x",
        "big operators",
    ),
    (
        "\\lim",
        "\\lim_{${} \\to ${}} ${}",
        "\\lim_{x \\to 0}",
        "big operators",
    ),
    (
        "\\binom",
        "\\binom{${}}{${}}",
        "\\binom{n}{k}",
        "structures",
    ),
    (
        "\\left(",
        "\\left( ${} \\right)",
        "\\left( x \\right)",
        "structures",
    ),
    (
        "\\left[",
        "\\left[ ${} \\right]",
        "\\left[ x \\right]",
        "structures",
    ),
    (
        "\\left\\{",
        "\\left\\{ ${} \\right\\}",
        "\\left\\{ x \\right\\}",
        "structures",
    ),
    (
        "\\left|",
        "\\left| ${} \\right|",
        "\\left| x \\right|",
        "structures",
    ),
    ("\\sqrt[", "\\sqrt[${}]{${}}", "\\sqrt[n]{x}", "n-th root"),
    (
        "\\xrightarrow",
        "\\xrightarrow{${}}",
        "\\xrightarrow{f}",
        "arrows",
    ),
    ("\\text", "\\text{${}}", "\\text{if } x", "structures"),
];

/// Short forms people type from habit.
const ALIASES: &[&str] = &[
    "\\to",
    "\\gets",
    "\\le",
    "\\ge",
    "\\ne",
    "\\iff",
    "\\implies",
    "\\impliedby",
    "\\land",
    "\\lor",
    "\\lnot",
    "\\cdot",
    "\\dots",
    "\\ldots",
    "\\cdots",
    "\\ell",
];

/// `\name` or `\name[opt]{arg}…` with plain arguments (no scripts, spaces or nesting).
static SIMPLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\\[A-Za-z]+(\[[^\]]*\])?(\{[^{}]*\})*$").unwrap());
static COMMAND_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\\[A-Za-z]+").unwrap());

/// A palette entry as a snippet: each plain argument becomes a field.
pub fn template_of(tex: &str) -> String {
    if !SIMPLE.is_match(tex) {
        return tex.into();
    }
    let opt = Regex::new(r"\[[^\]]*\]")
        .unwrap()
        .replace(tex, "[$${}]")
        .into_owned();
    Regex::new(r"\{[^{}]*\}")
        .unwrap()
        .replace_all(&opt, "{$${}}")
        .into_owned()
}

/// (label, template, preview, detail)
fn math_items() -> &'static [(String, String, String, String)] {
    static ALL: LazyLock<Vec<(String, String, String, String)>> = LazyLock::new(|| {
        let mut out: Vec<(String, String, String, String)> = STRUCTURES
            .iter()
            .map(|(l, a, p, d)| (l.to_string(), a.to_string(), p.to_string(), d.to_string()))
            .collect();
        let mut seen: std::collections::HashSet<String> = out.iter().map(|i| i.0.clone()).collect();
        for s in symbols::symbols() {
            if !SIMPLE.is_match(&s.tex) {
                continue;
            }
            let label = COMMAND_NAME.find(&s.tex).unwrap().as_str().to_owned();
            if !seen.insert(label.clone()) {
                continue;
            }
            let detail = s.pkg.map_or_else(|| s.group.to_lowercase(), str::to_owned);
            out.push((label, template_of(&s.tex), s.tex.clone(), detail));
        }
        for a in ALIASES {
            if seen.insert(a.to_string()) {
                out.push((a.to_string(), a.to_string(), a.to_string(), "misc".into()));
            }
        }
        out
    });
    &ALL
}

fn math_completions(boost: i32) -> Vec<Item> {
    math_items()
        .iter()
        .map(|(l, a, p, d)| {
            let mut it = Item::new(l.clone(), Kind::Keyword, Apply::Snippet(a.clone()))
                .detail(d.clone())
                .boost(boost);
            it.preview = Some(p.clone());
            it
        })
        .collect()
}

fn is_math_label(l: &str) -> bool {
    math_items().iter().any(|i| i.0 == l)
}

fn text_commands(math: bool) -> Vec<Item> {
    COMMANDS
        .iter()
        .filter(|(l, _, _)| !is_math_label(l))
        .map(|&(label, apply, detail)| {
            // \begin leads every list that starts with \b: environments are the commonest thing typed there.
            let boost = if label == "\\begin" {
                20
            } else if math {
                -10
            } else {
                5
            };
            let a = if label == "\\begin" {
                Apply::Begin
            } else {
                Apply::Snippet(apply.into())
            };
            let mut it = Item::new(label, Kind::Function, a).boost(boost);
            if !detail.is_empty() {
                it.detail = Some(detail.into());
            }
            it
        })
        .collect()
}

// ── Sources (completions.ts) ───────────────────────────────────────────────

static REF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\[a-zA-Z]*ref[a-zA-Z]*\*?\{([^{}\n]*)$").unwrap());
static CITE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\[a-zA-Z]*cite[a-zA-Z]*\*?(?:\[[^\]\n]*\]){0,2}\{([^{}\n]*)$").unwrap()
});
static FILE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\(input|include|subfile|includegraphics|bibliography|addbibresource|includepdf)\*?(?:\[[^\]\n]*\])?\{([^{}\n]*)$").unwrap()
});
static PKG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\\(?:usepackage|RequirePackage)(?:\[[^\]\n]*\])?\{([^{}\n]*)$").unwrap()
});
static ENV: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\(begin|end)\{([A-Za-z*]*)$").unwrap());
static CMD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\[A-Za-z]*$").unwrap());
static IMAGES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\.(png|jpe?g|pdf|svg|eps|gif|webp)$").unwrap());
static USED_ENV: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\begin\{([A-Za-z*]+)\}").unwrap());
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z][A-Za-z0-9-]*$").unwrap());

/// The environments most documents use come first.
const COMMON: &[&str] = &[
    "itemize",
    "enumerate",
    "figure",
    "table",
    "equation",
    "align",
    "align*",
    "tabular",
    "theorem",
    "proof",
    "abstract",
];

/// What a source found: its items, and whether what is typed filters them.
pub struct Found {
    pub items: Vec<Item>,
    pub filter: bool,
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

/// The item of a braced list being typed: after the last `,` or `{`, without leading blanks.
fn list_item(inner: &str, pos: usize) -> (usize, String) {
    let word = inner.rsplit(',').next().unwrap_or("");
    let typed = word.trim_start();
    (pos - chars(typed), typed.to_owned())
}

/// The local completions at `pos` (latexCompletions). `explicit` is Ctrl-Space.
pub fn latex(d: &Doc, pos: usize, look: &Lookups, explicit: bool) -> Option<Found> {
    let n = d.line_of(pos);
    let start = d.line_start(n);
    let before = d.slice(start, pos);
    let after = d.slice(pos, d.line_end(n));
    let at = |items: Vec<Item>, from: usize, to: usize| -> Vec<Item> {
        items
            .into_iter()
            .map(|mut i| {
                i.from = from;
                i.to = to;
                i
            })
            .collect()
    };

    if let Some(m) = ENV.captures(&before) {
        let kind = m[1].to_owned();
        let typed = &m[2];
        let from = pos - chars(typed);
        let to = if after.starts_with('}') { pos + 1 } else { pos };
        // Renaming an environment that already has its \end: only the name changes.
        let paired = kind == "begin"
            && super::envs::tokens(d)
                .iter()
                .any(|t| t.begin && t.partner.is_some() && pos >= t.from && pos <= t.to);
        let plain = kind == "end" || paired;
        let mut items: Vec<Item> = environments()
            .iter()
            .map(|e| {
                let apply = if plain {
                    Apply::Text(format!("{}}}", e.name))
                } else {
                    Apply::Snippet(env_snippet(e))
                };
                let mut it = Item::new(e.name, Kind::Class, apply)
                    .detail(e.detail)
                    .boost(if COMMON.contains(&e.name) { 12 } else { 10 });
                if !plain {
                    it.pkg = e.pkg.map(str::to_owned);
                }
                it
            })
            .collect();
        // Also names already used in this document.
        let mut used: Vec<String> = environments().iter().map(|e| e.name.to_owned()).collect();
        for x in USED_ENV.captures_iter(&d.text()) {
            if !used.contains(&x[1].to_owned()) {
                used.push(x[1].to_owned());
                let apply = if plain {
                    Apply::Text(format!("{}}}", &x[1]))
                } else {
                    Apply::Snippet(format!("{0}}}\n\t${{}}\n\\end{{{0}}}", &x[1]))
                };
                items.push(Item::new(&x[1], Kind::Class, apply).detail("used here"));
            }
        }
        return Some(Found {
            items: at(items, from, to),
            filter: true,
        });
    }

    if let Some(m) = REF.captures(&before) {
        let (from, typed) = list_item(&m[1], pos);
        let items = look
            .labels
            .iter()
            .map(|l| {
                let mut it = Item::new(&l.label, Kind::Constant, Apply::Text(l.label.clone()))
                    .detail(format!("{}:{}", l.file, l.line));
                it.info = (!l.context.is_empty()).then(|| l.context.clone());
                it
            })
            .collect();
        return Some(Found {
            items: at(items, from, pos),
            filter: !typed.is_empty() || explicit,
        });
    }

    if let Some(m) = CITE.captures(&before) {
        let (from, _) = list_item(&m[1], pos);
        let items = look
            .cites
            .iter()
            .map(|e| {
                let first = e.authors.split(" and ").next().unwrap_or("");
                let detail = [first, e.year.as_str()]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                let mut it =
                    Item::new(&e.key, Kind::Text, Apply::Text(e.key.clone())).detail(detail);
                it.info = (!e.title.is_empty()).then(|| e.title.clone());
                it
            })
            .collect();
        return Some(Found {
            items: at(items, from, pos),
            filter: true,
        });
    }

    if let Some(m) = FILE.captures(&before) {
        let cmd = m[1].to_owned();
        let from = pos - chars(&m[2]);
        let items = look
            .files
            .iter()
            .filter(|f| match cmd.as_str() {
                "includegraphics" => IMAGES.is_match(f),
                "bibliography" | "addbibresource" => f.ends_with(".bib"),
                "includepdf" => f.ends_with(".pdf"),
                _ => f.ends_with(".tex"),
            })
            .map(|f| {
                let label = match cmd.as_str() {
                    "input" | "include" | "subfile" => f.strip_suffix(".tex").unwrap_or(f),
                    "bibliography" => f.strip_suffix(".bib").unwrap_or(f),
                    _ => f,
                };
                Item::new(label, Kind::File, Apply::Text(label.into()))
            })
            .collect();
        return Some(Found {
            items: at(items, from, pos),
            filter: true,
        });
    }

    if let Some(m) = PKG.captures(&before) {
        let (from, _) = list_item(&m[1], pos);
        let items = look
            .packages
            .iter()
            .map(|n| Item::new(n, Kind::Namespace, Apply::Text(n.clone())))
            .collect();
        return Some(Found {
            items: at(items, from, pos),
            filter: true,
        });
    }

    // Commands.
    if let Some(c) = CMD.find(&before)
        && (chars(c.as_str()) > 1 || explicit)
    {
        let from = pos - chars(c.as_str());
        let in_math = math::in_math(d, pos);
        let mut items = if in_math {
            let mut v = math_completions(20);
            v.extend(text_commands(true));
            v
        } else {
            let mut v = math_completions(-40);
            v.extend(text_commands(false));
            v
        };
        // `\end{name}` for the innermost environment still open before the cursor.
        if let Some(t) = super::envs::tokens(d)
            .iter()
            .rev()
            .find(|t| t.begin && t.partner.is_none() && t.to <= pos)
        {
            let name = d.slice(t.from, t.to);
            items.push(
                Item::new(
                    format!("\\end{{{name}}}"),
                    Kind::Class,
                    Apply::Text(format!("\\end{{{name}}}")),
                )
                .detail("close the open environment")
                .boost(30),
            );
        }
        return Some(Found {
            items: at(items, from, pos),
            filter: true,
        });
    }
    None
}

/// The user's snippets for a typed trigger (snippetCompletions).
pub fn snippets(d: &Doc, pos: usize, look: &Lookups) -> Option<Found> {
    let n = d.line_of(pos);
    let before = d.slice(d.line_start(n), pos);
    let word = WORD.find(&before)?;
    let from = pos - chars(word.as_str());
    // A command name or a braced argument is for the other sources.
    if before[..word.start()].ends_with(['\\', '{']) {
        return None;
    }
    let hits = match_snippets(&look.snippets, word.as_str());
    if hits.is_empty() {
        return None;
    }
    let items = hits
        .into_iter()
        .map(|s| {
            let mut it = Item::new(&s.trigger, Kind::Text, Apply::Snippet(s.body.clone()))
                .detail(if s.description.is_empty() {
                    "snippet".to_owned()
                } else {
                    s.description.clone()
                })
                .boost(99);
            it.info = Some(
                Regex::new(r"\$\{[^}]*\}")
                    .unwrap()
                    .replace_all(&s.body, "…")
                    .into_owned(),
            );
            it.from = from;
            it.to = pos;
            it
        })
        .collect();
    Some(Found {
        items,
        filter: true,
    })
}

/// Triggers are letters, digits and dashes, starting with a letter.
pub fn valid_trigger(t: &str) -> bool {
    static R: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9-]{1,23}$").unwrap());
    R.is_match(t)
}

/// Snippets whose trigger starts with what was typed (two letters at least), shortest first.
pub fn match_snippets<'a>(all: &'a [Snippet], typed: &str) -> Vec<&'a Snippet> {
    if typed.chars().count() < 2 {
        return Vec::new();
    }
    let t = typed.to_lowercase();
    let mut v: Vec<&Snippet> = all
        .iter()
        .filter(|s| {
            valid_trigger(&s.trigger)
                && !s.body.is_empty()
                && s.trigger.to_lowercase().starts_with(&t)
        })
        .collect();
    v.sort_by(|a, b| {
        a.trigger
            .len()
            .cmp(&b.trigger.len())
            .then_with(|| a.trigger.cmp(&b.trigger))
    });
    v
}

/// Whether the language server should be asked for commands here: a command being typed, and none
/// of the contexts the local source answers itself (lspCommandCompletions).
pub fn wants_server(d: &Doc, pos: usize, explicit: bool) -> bool {
    let n = d.line_of(pos);
    let before = d.slice(d.line_start(n), pos);
    let Some(c) = CMD.find(&before) else {
        return false;
    };
    if chars(c.as_str()) < 2 && !explicit {
        return false;
    }
    ![&*ENV, &*REF, &*CITE, &*FILE, &*PKG]
        .iter()
        .any(|r| r.is_match(&before))
}

/// texlab's command items, minus those the local list has (its labels come without the backslash).
pub fn server_items(names: &[(String, Option<String>)], from: usize, pos: usize) -> Vec<Item> {
    let local: std::collections::HashSet<String> = text_commands(false)
        .into_iter()
        .map(|i| i.label)
        .chain(math_items().iter().map(|i| i.0.clone()))
        .chain(
            environments()
                .iter()
                .map(|e| format!("\\begin{{{}}}", e.name)),
        )
        .collect();
    names
        .iter()
        .filter(|(n, _)| !local.contains(&format!("\\{n}")))
        .map(|(n, detail)| {
            let mut it = Item::new(n.clone(), Kind::Function, Apply::Text(n.clone()));
            it.display = Some(format!("\\{n}"));
            it.detail = detail.clone();
            it.from = from;
            it.to = pos;
            it
        })
        .collect()
}

// ── Matching (CodeMirror's FuzzyMatcher) ───────────────────────────────────

const GAP: i32 = -1100;
const NOT_START: i32 = -700;
const CASE_FOLD: i32 = -200;
const BY_WORD: i32 = -100;
const NOT_FULL: i32 = -100;

/// How well `pattern` matches `label`, and which chars matched; None when it doesn't.
pub fn fuzzy(pattern: &str, label: &str) -> Option<(i32, Vec<usize>)> {
    let p: Vec<char> = pattern.chars().collect();
    let l: Vec<char> = label.chars().collect();
    if p.is_empty() {
        return Some((NOT_FULL, Vec::new()));
    }
    if l.len() < p.len() {
        return None;
    }
    let full = |score: i32| {
        if l.len() == p.len() {
            score
        } else {
            score + NOT_FULL
        }
    };
    let lower = |c: &char| c.to_lowercase().next().unwrap_or(*c);
    if l.starts_with(&p) {
        return Some((full(0), (0..p.len()).collect()));
    }
    let pl: Vec<char> = p.iter().map(lower).collect();
    let ll: Vec<char> = l.iter().map(lower).collect();
    if ll.starts_with(&pl) {
        return Some((full(CASE_FOLD), (0..p.len()).collect()));
    }
    // Each pattern char at the start of a word ("tc" in "textcolor" doesn't, "tC" in "textColor" does).
    let word_start = |i: usize| {
        i == 0
            || (l[i].is_uppercase() && !l[i - 1].is_uppercase())
            || (!l[i - 1].is_alphanumeric() && l[i].is_alphanumeric())
    };
    let mut by_word = Vec::new();
    let mut k = 0;
    for (i, c) in ll.iter().enumerate() {
        if k < pl.len() && *c == pl[k] && word_start(i) {
            by_word.push(i);
            k += 1;
        }
    }
    if k == pl.len() {
        let fold = if by_word.iter().zip(&p).all(|(i, c)| l[*i] == *c) {
            0
        } else {
            CASE_FOLD
        };
        return Some((BY_WORD + fold + NOT_FULL, by_word));
    }
    // A substring somewhere.
    if let Some(at) = (0..=ll.len() - pl.len()).find(|&i| ll[i..i + pl.len()] == pl[..]) {
        let fold = if l[at..at + p.len()] == p[..] {
            0
        } else {
            CASE_FOLD
        };
        return Some((NOT_START + fold + NOT_FULL, (at..at + p.len()).collect()));
    }
    // The chars in order, with gaps.
    let mut hits = Vec::new();
    let mut k = 0;
    for (i, c) in ll.iter().enumerate() {
        if k < pl.len() && *c == pl[k] {
            hits.push(i);
            k += 1;
        }
    }
    (k == pl.len()).then_some((GAP + CASE_FOLD + NOT_FULL, hits))
}

/// Filters and ranks what the sources found against what is typed, best first.
pub fn rank(d: &Doc, found: Vec<Found>) -> Vec<(Item, Vec<usize>)> {
    let mut out: Vec<(i32, Item, Vec<usize>)> = Vec::new();
    for f in found {
        for it in f.items {
            let typed = d.slice(it.from, d.selection.main().head);
            if !f.filter {
                out.push((it.boost, it, Vec::new()));
                continue;
            }
            let shown = it.display.clone().unwrap_or_else(|| it.label.clone());
            // texlab's items match from after the backslash.
            let target = if it.display.is_some() {
                &it.label
            } else {
                &shown
            };
            if let Some((score, mut hits)) = fuzzy(&typed, target) {
                if it.display.is_some() {
                    hits.iter_mut().for_each(|h| *h += 1);
                }
                out.push((score + it.boost, it, hits));
            }
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.label.cmp(&b.1.label)));
    out.into_iter().map(|(_, i, h)| (i, h)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look() -> Lookups {
        Lookups {
            labels: vec![lemmata_project::extras::Label {
                label: "sec:intro".into(),
                file: "main.tex".into(),
                line: 3,
                context: "\\section{Intro}".into(),
                env: "section".into(),
                snippet: "Intro".into(),
                graphic: String::new(),
            }],
            cites: vec![lemmata_bib::Entry {
                key: "knuth1984".into(),
                kind: "book".into(),
                title: "The TeXbook".into(),
                authors: "Knuth, Donald and X".into(),
                year: "1984".into(),
                file: "refs.bib".into(),
            }],
            packages: vec!["amsmath".into(), "graphicx".into()],
            files: vec![
                "main.tex".into(),
                "chapters/a.tex".into(),
                "figures/plot.png".into(),
                "refs.bib".into(),
            ],
            snippets: vec![
                Snippet {
                    trigger: "fig".into(),
                    body: "\\begin{figure}\n${}\n\\end{figure}".into(),
                    description: "A figure".into(),
                },
                Snippet {
                    trigger: "figs".into(),
                    body: "x".into(),
                    description: String::new(),
                },
                Snippet {
                    trigger: "thm".into(),
                    body: "\\begin{theorem}${}\\end{theorem}".into(),
                    description: "Theorem".into(),
                },
                Snippet {
                    trigger: "bad trigger".into(),
                    body: "x".into(),
                    description: String::new(),
                },
            ],
            generation: 0,
            root: Default::default(),
            server: None,
            hover: None,
        }
    }

    fn complete(doc: &str, pos: Option<usize>) -> Option<Found> {
        let mut d = Doc::new(doc);
        let pos = pos.unwrap_or(d.len());
        d.selection = super::super::buffer::Selection::cursor(pos);
        latex(&d, pos, &look(), false)
    }
    fn labels(f: &Option<Found>) -> Vec<String> {
        f.as_ref()
            .map(|f| f.items.iter().map(|i| i.label.clone()).collect())
            .unwrap_or_default()
    }

    // completions.test.ts
    #[test]
    fn environments_after_begin() {
        let r = complete("\\begin{ite}", Some(10));
        assert!(labels(&r).contains(&"itemize".into()));
        let it = r
            .as_ref()
            .unwrap()
            .items
            .iter()
            .find(|i| i.label == "itemize")
            .unwrap();
        assert_eq!(
            (it.from, it.to),
            (7, 11),
            "swallows the }} that closeBrackets added"
        );
        let r = complete("\\begin{ite}\nx\n\\end{ite}", Some(10));
        assert_eq!(
            r.unwrap()
                .items
                .iter()
                .find(|i| i.label == "itemize")
                .unwrap()
                .apply,
            Apply::Text("itemize}".into())
        );
        let r = complete("\\end{al", Some(7));
        assert!(matches!(
            r.unwrap()
                .items
                .iter()
                .find(|i| i.label == "align")
                .unwrap()
                .apply,
            Apply::Text(_)
        ));
        assert!(
            labels(&complete(
                "\\begin{myenv}\\end{myenv}\n\\begin{my",
                Some(33)
            ))
            .contains(&"myenv".into())
        );
    }

    #[test]
    fn begin_and_closing_the_open_environment() {
        let r = complete("\\be", None);
        let l = labels(&r);
        assert!(l.contains(&"\\begin".into()) && !l.iter().any(|x| x.starts_with("\\begin{")));
        let r = complete("\\begin{document}\n\\begin{itemize}\n\\item a\n\\en", None);
        assert!(labels(&r).contains(&"\\end{itemize}".into()));
        assert!(!labels(&r).contains(&"\\end{document}".into()));
        assert!(
            !labels(&complete("\\begin{a}\\end{a}\n\\en", None))
                .iter()
                .any(|l| l.starts_with("\\end{"))
        );
    }

    #[test]
    fn tables_start_with_a_header_and_sample_rows() {
        let table = env_snippet(environments().iter().find(|e| e.name == "table").unwrap());
        assert!(table.contains("table}[${htbp}]"));
        assert!(table.contains("\\end{tabular}\n\t\\caption{${Caption}}\n\t\\label{tab:${label}}"));
        assert!(table.contains("\t\t${Header 1} & ${Header 2} & ${Header 3}"));
        assert!(table.ends_with("\\end{table}"));
        let tabular = env_snippet(environments().iter().find(|e| e.name == "tabular").unwrap());
        assert!(
            tabular.contains("tabular}{${lll}}")
                && tabular.contains("\t${Cell 1.1} & ${Cell 1.2} & ${Cell 1.3}")
        );
    }

    #[test]
    fn labels_cites_files_and_packages() {
        assert_eq!(labels(&complete("See \\ref{", None)), ["sec:intro"]);
        let r = complete("\\parencite[p.~3]{kn", None).unwrap();
        assert_eq!(r.items[0].detail.as_deref(), Some("Knuth, Donald 1984"));
        assert_eq!(r.items[0].info.as_deref(), Some("The TeXbook"));
        assert_eq!(labels(&complete("\\input{", None)), ["main", "chapters/a"]);
        assert_eq!(
            labels(&complete("\\includegraphics[width=3cm]{", None)),
            ["figures/plot.png"]
        );
        assert_eq!(labels(&complete("\\addbibresource{", None)), ["refs.bib"]);
        assert_eq!(
            labels(&complete("\\usepackage[utf8]{", None)),
            ["amsmath", "graphicx"]
        );
        assert!(complete("hello wor", None).is_none());
    }

    #[test]
    fn maths_first_in_maths_and_text_commands_first_in_prose() {
        let boost = |f: &Option<Found>, l: &str| {
            f.as_ref()
                .unwrap()
                .items
                .iter()
                .find(|i| i.label == l)
                .unwrap()
                .boost
        };
        let inside = complete("Let $x = \\al", None);
        let prose = complete("Let us write \\al", None);
        assert!(boost(&inside, "\\alpha") > boost(&inside, "\\section"));
        assert!(boost(&prose, "\\alpha") < 0 && boost(&prose, "\\section") > 0);
        let r = complete("$\\fra", None).unwrap();
        let frac = r.items.iter().find(|i| i.label == "\\frac").unwrap();
        assert_eq!(frac.preview.as_deref(), Some("\\frac{a}{b}"));
        assert_eq!(frac.apply, Apply::Snippet("\\frac{${}}{${}}".into()));
        assert!(math_items().len() > 150);
    }

    #[test]
    fn server_commands_skip_local_ones_and_lists() {
        let items = server_items(
            &[("section".into(), None), ("sectionmark".into(), None)],
            1,
            4,
        );
        assert_eq!(
            items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["sectionmark"]
        );
        assert_eq!(items[0].display.as_deref(), Some("\\sectionmark"));
        for doc in ["\\begin{al", "\\ref{a\\b", "hello"] {
            assert!(!wants_server(&Doc::new(doc), doc.len(), false), "{doc}");
        }
        assert!(wants_server(&Doc::new("\\sec"), 4, false));
    }

    // snippets.test.ts
    #[test]
    fn snippets_by_trigger() {
        let l = look();
        assert!(match_snippets(&l.snippets, "f").is_empty());
        assert_eq!(
            match_snippets(&l.snippets, "fi")
                .iter()
                .map(|s| s.trigger.as_str())
                .collect::<Vec<_>>(),
            ["fig", "figs"]
        );
        assert_eq!(match_snippets(&l.snippets, "TH").len(), 1);
        assert!(match_snippets(&l.snippets, "bad").is_empty());
        assert!(!valid_trigger("a") && valid_trigger("eq-1"));
        let d = Doc::new("Some text fig");
        assert!(snippets(&d, 13, &l).is_some());
        assert!(snippets(&Doc::new("\\fig"), 4, &l).is_none());
        assert!(snippets(&Doc::new("\\ref{fig"), 8, &l).is_none());
    }

    #[test]
    fn ranks_prefixes_before_fuzzy_matches() {
        assert!(fuzzy("sec", "\\section").unwrap().0 < fuzzy("\\sec", "\\section").unwrap().0);
        assert_eq!(fuzzy("\\sec", "\\section").map(|s| s.0), Some(NOT_FULL));
        assert!(fuzzy("\\Sec", "\\section").unwrap().0 < fuzzy("\\sec", "\\section").unwrap().0);
        assert!(fuzzy("\\tc", "\\textcolor").is_some());
        assert!(fuzzy("\\sec", "\\subsection").unwrap().0 < fuzzy("\\sec", "\\section").unwrap().0);
        assert_eq!(fuzzy("xyz", "\\alpha"), None);
    }
}
