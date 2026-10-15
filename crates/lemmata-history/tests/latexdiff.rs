//! The PDF of the changes with the real latexdiff, latexmk and pdflatex. Skipped when they (or the
//! LaTeX packages ulem, color and listings, which latexdiff's markup needs) aren't installed, unless
//! LEMMATA_REQUIRE_TEX is set: CI sets it so a broken TeX install can't pass by skipping.

use lemmata_history::{History, LATEXDIFF_DIR, Tools, find_latexdiff, latexdiff_pdf};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use std::{env, fs};

struct Installed {
    latexdiff: PathBuf,
    latexmk: PathBuf,
}

fn installed() -> Option<Installed> {
    let has = |file: &str| {
        Command::new("kpsewhich")
            .arg(file)
            .output()
            .is_ok_and(|o| !o.stdout.is_empty())
    };
    let found = (|| {
        let latexdiff = find_latexdiff(None)?;
        let latexmk = which::which("latexmk").ok()?;
        which::which("pdflatex").ok()?;
        which::which("biber").ok()?;
        (["ulem.sty", "listings.sty", "color.sty", "biblatex.sty"]
            .into_iter()
            .all(has))
        .then_some(Installed { latexdiff, latexmk })
    })();
    if found.is_none() {
        assert!(
            env::var_os("LEMMATA_REQUIRE_TEX").is_none(),
            "LEMMATA_REQUIRE_TEX is set, but latexdiff, latexmk, pdflatex, biber, biblatex, ulem, listings or color is missing"
        );
        eprintln!(
            "skipped: needs latexdiff, latexmk, pdflatex, biber, biblatex, ulem, listings and color"
        );
    }
    found
}

fn tools(i: &Installed) -> Tools<'_> {
    Tools {
        latexdiff: &i.latexdiff,
        latexmk: &i.latexmk,
        engine_flag: "-pdf",
        installer: None,
        extra_args: Vec::new(),
        path_dirs: Vec::new(),
        timeout: Duration::from_secs(180),
    }
}

/// A 1x1 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

fn write(root: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let to = root.join(path);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::write(to, text).unwrap();
    }
}

/// Snapshots the project as it is, applies `after`, and returns the history and the snapshot's id.
fn two_versions(
    before: &[(&str, &str)],
    after: &[(&str, &str)],
) -> (tempfile::TempDir, History, String) {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    write(root, before);
    fs::create_dir_all(root.join("figures")).unwrap();
    fs::write(root.join("figures/box.png"), PNG).unwrap();
    let h = History::init(root).unwrap();
    let old = h.snapshot("v1").unwrap().unwrap();
    write(root, after);
    (d, h, old)
}

fn make(
    d: &tempfile::TempDir,
    h: &History,
    old: &str,
    main: &str,
    t: &Tools,
) -> lemmata_history::Result<PathBuf> {
    let root = d.path();
    h.extract_tree(old, &root.join(LATEXDIFF_DIR).join("old"))
        .unwrap();
    let pdf = latexdiff_pdf(root, Path::new(main), t);
    // The old tree is cleaned up whether it worked or not.
    assert!(!root.join(LATEXDIFF_DIR).join("old").exists());
    pdf
}

fn diff_tex(root: &Path) -> String {
    fs::read_to_string(root.join(LATEXDIFF_DIR).join("diff.tex")).unwrap()
}

/// A PDF came out, TeX had no errors, and the bibliography tool ran and found the .bib file.
fn assert_pdf(pdf: &Path) {
    let bytes = fs::read(pdf).unwrap();
    assert!(bytes.starts_with(b"%PDF") && bytes.len() > 1000);
    let log = fs::read_to_string(pdf.with_extension("log")).unwrap();
    assert!(!log.lines().any(|l| l.starts_with('!')), "{log}");
    let bbl = fs::read_to_string(pdf.with_extension("bbl")).unwrap();
    assert!(bbl.contains("Alpha"), "{bbl}");
}

const MAIN_BIBLATEX: &str = r"\documentclass{article}
\usepackage{graphicx}
\graphicspath{{figures/}}
\usepackage[backend=biber]{biblatex}
\addbibresource{refs.bib}
\begin{document}
Intro text.
\input{chapters/one}
\include{chapters/two}
\includegraphics[width=1cm]{box}
\printbibliography
\end{document}
";
const REFS_OLD: &str = "@book{a, author={Ann Author}, title={Alpha}, year={2001}}
@book{gone, author={Gil Gone}, title={Removed}, year={1999}}
";
const REFS_NEW: &str = "@book{a, author={Ann Author}, title={Alpha}, year={2001}}
@book{b, author={Bo Writer}, title={Beta}, year={2020}}
";

#[test]
fn input_include_biblatex_and_graphics() {
    let Some(i) = installed() else { return };
    let (d, h, old) = two_versions(
        &[
            ("main.tex", MAIN_BIBLATEX),
            (
                "chapters/one.tex",
                "The first chapter says hello \\cite{gone}.\n",
            ),
            ("chapters/two.tex", "The second chapter stays.\n"),
            ("refs.bib", REFS_OLD),
        ],
        &[
            (
                "chapters/one.tex",
                "The first chapter says goodbye \\cite{a}.\n",
            ),
            ("chapters/two.tex", "The second chapter stays, and grows.\n"),
            ("refs.bib", REFS_NEW),
        ],
    );
    let pdf = make(&d, &h, &old, "main.tex", &tools(&i)).unwrap();
    assert_pdf(&pdf);
    let tex = diff_tex(d.path());
    // \input and \include were flattened and the words that changed are marked.
    assert!(
        tex.contains("\\DIFdel{hello") && tex.contains("\\DIFadd{goodbye"),
        "{tex}"
    );
    assert!(tex.contains("grows") && !tex.contains("\\input{chapters"));
}

#[test]
fn main_file_in_a_subfolder_with_bibtex() {
    let Some(i) = installed() else { return };
    let main = "\\documentclass{article}\n\\begin{document}\nIntro.\n\\input{paper/sec}\n\\bibliographystyle{plain}\n\\bibliography{paper/refs}\n\\end{document}\n";
    let (d, h, old) = two_versions(
        &[
            ("paper/main.tex", main),
            ("paper/sec.tex", "Old words \\cite{a}.\n"),
            ("paper/refs.bib", REFS_OLD),
        ],
        &[
            ("paper/sec.tex", "New words \\cite{a}.\n"),
            ("paper/refs.bib", REFS_NEW),
        ],
    );
    let pdf = make(&d, &h, &old, "paper/main.tex", &tools(&i)).unwrap();
    assert_pdf(&pdf);
    let tex = diff_tex(d.path());
    assert!(
        tex.contains("\\DIFadd{New") && tex.contains("\\DIFdel{Old"),
        "{tex}"
    );
}

#[test]
fn a_package_that_does_not_exist_is_named() {
    let Some(i) = installed() else { return };
    let main = "\\documentclass{article}\n\\begin{document}\nText.\n\\end{document}\n";
    let (d, h, old) = two_versions(
        &[("main.tex", main)],
        &[(
            "main.tex",
            "\\documentclass{article}\n\\usepackage{lemmata-does-not-exist}\n\\begin{document}\nText.\n\\end{document}\n",
        )],
    );
    let e = make(&d, &h, &old, "main.tex", &tools(&i)).unwrap_err();
    assert!(e.to_string().contains("lemmata-does-not-exist.sty"), "{e}");
}

#[test]
fn latexdiff_that_cannot_start_is_explained() {
    let Some(i) = installed() else { return };
    let (d, h, old) = two_versions(
        &[(
            "main.tex",
            "\\documentclass{article}\n\\begin{document}\nA\n\\end{document}\n",
        )],
        &[],
    );
    let missing = d.path().join("no-such-latexdiff");
    let t = Tools {
        latexdiff: &missing,
        ..tools(&i)
    };
    let e = make(&d, &h, &old, "main.tex", &t).unwrap_err();
    assert!(e.to_string().contains("no-such-latexdiff"), "{e}");
}
