//! Forward search (source → PDF) and backward search (PDF → source).
//!
//! The `.synctex.gz` file next to the PDF is parsed in process and cached
//! until it changes. The `synctex` command that ships with TeX Live and
//! MiKTeX is only run when that file cannot be read or parsed.
//!
//! Coordinates are PDF points (1/72 in) measured from the top-left corner
//! of the page, which is what `synctex` reports.

mod index;

use index::Index;
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not run synctex: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("synctex failed: {0}")]
    Failed(String),
    /// pdfLaTeX writes the position data at the end of a build, so it is missing while one runs.
    #[error(
        "The PDF has no position data yet. It is written at the end of a build: wait for the build to finish, or compile again."
    )]
    NoData,
}

pub type Result<T> = std::result::Result<T, Error>;

/// A box in the PDF that corresponds to a source position.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PdfBox {
    pub page: u32,
    pub x: f64,
    pub y: f64,
    /// Left edge.
    pub h: f64,
    /// Baseline.
    pub v: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceLocation {
    pub file: PathBuf,
    pub line: u32,
    pub column: Option<u32>,
}

/// Where in the PDF a source line ended up.
pub fn forward(
    synctex: &Path,
    pdf: &Path,
    tex_file: &Path,
    line: u32,
    column: u32,
) -> Result<Vec<PdfBox>> {
    if let Some(idx) = load(pdf) {
        return Ok(idx.forward(tex_file, line));
    }
    if !has_data(pdf) {
        return Err(Error::NoData);
    }
    let input = format!("{line}:{column}:{}", tex_file.display());
    let out = run(
        synctex,
        &["view", "-i", &input, "-o", &pdf.to_string_lossy()],
    )?;
    Ok(parse_view(&out))
}

/// Which source line produced the point clicked in the PDF.
pub fn backward(
    synctex: &Path,
    pdf: &Path,
    page: u32,
    x: f64,
    y: f64,
) -> Result<Option<SourceLocation>> {
    if let Some(idx) = load(pdf) {
        return Ok(idx.backward(page, x, y));
    }
    // Without the file the synctex tool can't help either; it would only print its usage.
    if !has_data(pdf) {
        return Err(Error::NoData);
    }
    let output = format!("{page}:{x:.2}:{y:.2}:{}", pdf.display());
    let out = run(synctex, &["edit", "-o", &output])?;
    Ok(parse_edit(&out))
}

/// Whether the synctex file next to `pdf` exists (compressed or not).
fn has_data(pdf: &Path) -> bool {
    pdf.with_extension("synctex.gz").is_file() || pdf.with_extension("synctex").is_file()
}

type Cache = Option<(PathBuf, SystemTime, Arc<Index>)>;
static CACHE: Mutex<Cache> = Mutex::new(None);

/// The parsed synctex file of `pdf`, re-read only when its mtime changes.
fn load(pdf: &Path) -> Option<Arc<Index>> {
    let (path, mtime) = [
        pdf.with_extension("synctex.gz"),
        pdf.with_extension("synctex"),
    ]
    .into_iter()
    .find_map(|p| Some((p.clone(), std::fs::metadata(&p).ok()?.modified().ok()?)))?;
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((p, m, idx)) = cache.as_ref()
        && *p == path
        && *m == mtime
    {
        return Some(idx.clone());
    }
    let mut bytes = std::fs::read(&path).ok()?;
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut text = Vec::new();
        flate2::read::GzDecoder::new(&bytes[..])
            .read_to_end(&mut text)
            .ok()?;
        bytes = text;
    }
    let idx = Arc::new(Index::parse(
        &String::from_utf8_lossy(&bytes),
        path.parent()?,
    )?);
    *cache = Some((path, mtime, idx.clone()));
    Some(idx)
}

fn run(synctex: &Path, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new(synctex);
    lemmata_env::host(&mut cmd).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output()?;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() && !stdout.contains("SyncTeX result begin") {
        return Err(Error::Failed(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    Ok(stdout)
}

fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.strip_prefix(key)?.strip_prefix(':')
}

fn parse_view(out: &str) -> Vec<PdfBox> {
    let mut boxes = Vec::new();
    let mut cur: Option<PdfBox> = None;
    for line in out.lines() {
        if let Some(v) = field(line, "Page") {
            if let Some(b) = cur.take() {
                boxes.push(b);
            }
            cur = Some(PdfBox {
                page: v.parse().unwrap_or(1),
                x: 0.0,
                y: 0.0,
                h: 0.0,
                v: 0.0,
                width: 0.0,
                height: 0.0,
            });
            continue;
        }
        let Some(b) = cur.as_mut() else { continue };
        let num = |s: &str| s.parse::<f64>().unwrap_or(0.0);
        if let Some(v) = field(line, "x") {
            b.x = num(v);
        } else if let Some(v) = field(line, "y") {
            b.y = num(v);
        } else if let Some(v) = field(line, "h") {
            b.h = num(v);
        } else if let Some(v) = field(line, "v") {
            b.v = num(v);
        } else if let Some(v) = field(line, "W") {
            b.width = num(v);
        } else if let Some(v) = field(line, "H") {
            b.height = num(v);
        }
    }
    boxes.extend(cur);
    boxes
}

fn parse_edit(out: &str) -> Option<SourceLocation> {
    let mut file = None;
    let mut line_no = None;
    let mut column = None;
    for line in out.lines() {
        if let Some(v) = field(line, "Input") {
            file.get_or_insert_with(|| normalize(v));
        } else if let Some(v) = field(line, "Line") {
            line_no.get_or_insert(v.parse().ok()?);
        } else if let Some(v) = field(line, "Column") {
            column.get_or_insert(v.parse::<i64>().ok().filter(|c| *c >= 0).map(|c| c as u32));
        }
    }
    Some(SourceLocation {
        file: file?,
        line: line_no?,
        column: column.flatten(),
    })
}

/// TeX records inputs like `/home/x/thesis/./chapters/a.tex`.
fn normalize(p: &str) -> PathBuf {
    PathBuf::from(p.replace("/./", "/").replace("\\.\\", "\\"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_pdf_without_position_data_says_so() {
        let d = tempfile::tempdir().unwrap();
        let pdf = d.path().join("main.pdf");
        std::fs::write(&pdf, "%PDF").unwrap();
        let e = backward(Path::new("synctex"), &pdf, 1, 10.0, 10.0).unwrap_err();
        assert!(matches!(e, Error::NoData), "{e}");
        assert!(forward(Path::new("synctex"), &pdf, Path::new("main.tex"), 1, 0).is_err());
    }

    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn parses_view_output() {
        let out = "This is SyncTeX command line utility, version 1.5\nSyncTeX result begin\nOutput:/p/main.pdf\nPage:2\nx:133.7\ny:250.1\nh:72.0\nv:252.0\nW:451.2\nH:9.5\nbefore:\noffset:0\nmiddle:\nafter:\nSyncTeX result end\n";
        let b = parse_view(out);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].page, 2);
        assert_eq!(b[0].v, 252.0);
        assert_eq!(b[0].width, 451.2);
    }

    #[test]
    fn parses_edit_output() {
        let out = "SyncTeX result begin\nOutput:/p/main.pdf\nInput:/p/./chapters/method.tex\nLine:16\nColumn:-1\nOffset:0\nContext:\nSyncTeX result end\n";
        let loc = parse_edit(out).unwrap();
        assert_eq!(loc.file, PathBuf::from("/p/chapters/method.tex"));
        assert_eq!(loc.line, 16);
        assert_eq!(loc.column, None);
    }

    #[test]
    fn edit_without_result_is_none() {
        assert_eq!(
            parse_edit("SyncTeX result begin\nSyncTeX result end\n"),
            None
        );
    }

    fn tex_available() -> bool {
        Command::new("pdflatex").arg("--version").output().is_ok()
            && Command::new("synctex").arg("help").output().is_ok()
    }

    const MAIN: &str = "\\documentclass{article}
\\usepackage{amsmath}
\\begin{document}
\\section{Intro}
W W
W

\\input{chapters/a}

\\begin{equation}
 x^2 + y^2 = \\frac{a}{b}
\\end{equation}
Final text\\footnote{a note} here.
\\begin{align}
 a &= b \\\\
 c &= d
\\end{align}
\\clearpage
W W W
W

% comment line
W
\\end{document}
";

    const CHAPTER: &str = "\\subsection{A}
W W W
W

Inline $a+b=c$ math, W
\\begin{itemize}
\\item one
\\item two W W
\\end{itemize}
\\begin{tabular}{ll}
a & b \\\\
c & d
\\end{tabular}
";

    /// A two page, two file document built with the real pdflatex.
    fn build_doc(dir: &Path) -> PathBuf {
        let w = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua";
        std::fs::create_dir_all(dir.join("chapters")).unwrap();
        std::fs::write(dir.join("main.tex"), MAIN.replace('W', w)).unwrap();
        std::fs::write(dir.join("chapters/a.tex"), CHAPTER.replace('W', w)).unwrap();
        let out = Command::new("pdflatex")
            .args(["-synctex=1", "-interaction=nonstopmode", "main.tex"])
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "pdflatex failed");
        dir.join("main.pdf")
    }

    fn forward_cached(pdf: &Path, tex: &Path, line: u32) -> Vec<PdfBox> {
        forward(Path::new("synctex-missing"), pdf, tex, line, 1).unwrap()
    }

    fn close(a: &PdfBox, b: &PdfBox) -> bool {
        let c = |x: f64, y: f64| (x - y).abs() < 1.0;
        a.page == b.page
            && c(a.x, b.x)
            && c(a.y, b.y)
            && c(a.h, b.h)
            && c(a.v, b.v)
            && c(a.width, b.width)
            && c(a.height, b.height)
    }

    /// Lines of plain text, headings and paragraph ends, where the answer is
    /// unambiguous; displayed maths, lists and tables are only checked loosely.
    const PLAIN: &[(&str, u32)] = &[
        ("main.tex", 1),
        ("main.tex", 4),
        ("main.tex", 5),
        ("main.tex", 6),
        ("main.tex", 7),
        ("main.tex", 8),
        ("main.tex", 19),
        ("main.tex", 20),
        ("main.tex", 21),
        ("main.tex", 22),
        ("main.tex", 23),
        ("chapters/a.tex", 1),
        ("chapters/a.tex", 2),
        ("chapters/a.tex", 3),
        ("chapters/a.tex", 4),
        ("chapters/a.tex", 5),
    ];

    #[test]
    fn agrees_with_the_synctex_command() {
        if !tex_available() {
            eprintln!("skipped: pdflatex or synctex not installed");
            return;
        }
        let dir = std::env::temp_dir().join(format!("lemmata-synctex-{}", std::process::id()));
        let pdf = build_doc(&dir);
        let cli = Path::new("synctex");
        let t = Instant::now();
        let idx = load(&pdf).expect("the synctex file should parse");
        let first_load = t.elapsed();

        let (mut cli_time, mut new_time, mut n) = (Duration::ZERO, Duration::ZERO, 0u32);
        let mut loose_exact = 0;
        let mut loose_total = 0;
        for (file, last) in [("main.tex", 25), ("chapters/a.tex", 14)] {
            let tex = dir.join(file);
            for line in (1..=last).chain([500]) {
                let input = format!("{line}:1:{}", tex.display());
                let t = Instant::now();
                let out = run(cli, &["view", "-i", &input, "-o", &pdf.to_string_lossy()]).unwrap();
                cli_time += t.elapsed();
                let want = parse_view(&out);
                let t = Instant::now();
                let got = forward_cached(&pdf, &tex, line);
                new_time += t.elapsed();
                n += 1;
                assert!(!got.is_empty(), "{file}:{line} found nothing");
                let same =
                    got.len() == want.len() && want.iter().all(|w| got.iter().any(|g| close(g, w)));
                if PLAIN.contains(&(file, line)) {
                    assert!(same, "{file}:{line}\n  got  {got:?}\n  want {want:?}");
                } else {
                    // Loosely: the same pages, and a box on a line the CLI also marks.
                    let pages = |v: &[PdfBox]| {
                        let mut p: Vec<_> = v.iter().map(|b| b.page).collect();
                        p.dedup();
                        p
                    };
                    assert_eq!(pages(&got), pages(&want), "{file}:{line}");
                    loose_total += 1;
                    loose_exact += same as u32;
                }
            }
        }

        let (mut back_cli, mut back_new, mut back_n) = (Duration::ZERO, Duration::ZERO, 0u32);
        let (mut over_text, mut off_text, mut off_text_same) = (0, 0, 0);
        for page in 1..=2 {
            for xi in 0..60 {
                for yi in 0..80 {
                    let (x, y) = (xi as f64 * 10.0 + 3.3, yi as f64 * 10.0 + 1.7);
                    let out = format!("{page}:{x:.2}:{y:.2}:{}", pdf.display());
                    let t = Instant::now();
                    let want = parse_edit(&run(cli, &["edit", "-o", &out]).unwrap());
                    back_cli += t.elapsed();
                    let t = Instant::now();
                    let got = idx.backward(page, x, y);
                    back_new += t.elapsed();
                    back_n += 1;
                    if idx.over_text(page, x as f32, y as f32) {
                        over_text += 1;
                        assert_eq!(got, want, "page {page} at {x},{y}");
                    } else {
                        off_text += 1;
                        off_text_same += (got == want) as u32;
                        assert!(got.is_some());
                    }
                }
            }
        }
        eprintln!("first parse {first_load:?}");
        eprintln!(
            "forward x{n}: CLI {:?}/call, parsed {:?}/call; {} plain lines exact, {loose_exact}/{loose_total} other lines exact",
            cli_time / n,
            new_time / n,
            PLAIN.len()
        );
        eprintln!(
            "backward x{back_n}: CLI {:?}/call, parsed {:?}/call; {over_text} points over text exact, {off_text_same}/{off_text} off the text",
            back_cli / back_n,
            back_new / back_n
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A tiny hand written file: no TeX needed.
    #[test]
    fn answers_from_a_synctex_file() {
        let dir = std::env::temp_dir().join(format!("lemmata-synctex-hand-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = format!(
            "SyncTeX Version:1\nInput:1:{d}/./main.tex\nInput:2:{d}/./b.tex\nOutput:pdf\nMagnification:1000\nUnit:1\nX Offset:0\nY Offset:0\nContent:\n!1\n{{1\n[1,9:4736286,46220574:26673152,41484288,0\n(1,8:5000000,10000000:20000000,500000,100000\nk1,3:6000000,10000000:131072\nx1,3:9000000,10000000\n)\n(2,9:5000000,20000000:20000000,500000,100000\nx2,5:6000000,20000000\ng2,5:8000000,20000000\n)\n]\n}}1\nPostamble:\nCount:1\n",
            d = dir.display()
        );
        let pdf = dir.join("main.pdf");
        std::fs::write(&pdf, "").unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gz, text.as_bytes()).unwrap();
        std::fs::write(dir.join("main.synctex.gz"), gz.finish().unwrap()).unwrap();

        let idx = load(&pdf).unwrap();
        // Line 4 has nothing: the next line (5) answers. Units are bp: sp / 65781.76.
        let b = idx.forward(&dir.join("b.tex"), 4);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].page, 1);
        assert!((b[0].h - 5000000.0 / 65781.76).abs() < 0.01);
        assert!((b[0].width - 20000000.0 / 65781.76).abs() < 0.01);
        // A kern is shown from where it starts.
        let b = idx.forward(&dir.join("main.tex"), 3);
        assert!((b[0].x - (6000000.0 - 131072.0) / 65781.76).abs() < 0.01);
        // Clicking on the second box finds b.tex, with the `/./` removed.
        let hit = idx.backward(1, 100.0, 305.0).unwrap();
        assert_eq!(hit.file, dir.join("b.tex"));
        assert_eq!(hit.line, 5);
        assert!(idx.forward(&dir.join("other.tex"), 1).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Numbers for the doc comment above `load`: parse once, then look up.
    #[test]
    fn timing_on_a_longer_document() {
        if !tex_available() {
            eprintln!("skipped: pdflatex or synctex not installed");
            return;
        }
        let dir = std::env::temp_dir().join(format!("lemmata-synctex-long-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let para = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua ut enim ad minim veniam quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat";
        let mut tex = String::from("\\documentclass{article}\n\\begin{document}\n");
        for i in 0..600 {
            tex += &format!("{para} {para} $x_{i}^2$ {para}\n\n");
        }
        tex += "\\end{document}\n";
        std::fs::write(dir.join("main.tex"), tex).unwrap();
        let out = Command::new("pdflatex")
            .args(["-synctex=1", "-interaction=nonstopmode", "main.tex"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(out.status.success());
        let (pdf, tex) = (dir.join("main.pdf"), dir.join("main.tex"));
        let size = std::fs::metadata(dir.join("main.synctex.gz"))
            .unwrap()
            .len();

        let t = Instant::now();
        let idx = load(&pdf).unwrap();
        let parse = t.elapsed();
        let t = Instant::now();
        for _ in 0..100 {
            load(&pdf).unwrap();
        }
        let cached_load = t.elapsed() / 100;

        let lines = [3, 300, 900, 1500, 2200];
        let t = Instant::now();
        for l in lines {
            let input = format!("{l}:1:{}", tex.display());
            run(
                Path::new("synctex"),
                &["view", "-i", &input, "-o", &pdf.to_string_lossy()],
            )
            .unwrap();
        }
        let cli_fwd = t.elapsed() / lines.len() as u32;
        let t = Instant::now();
        for l in lines {
            assert!(!idx.forward(&tex, l).is_empty());
        }
        let new_fwd = t.elapsed() / lines.len() as u32;
        let t = Instant::now();
        for _ in 0..5 {
            run(
                Path::new("synctex"),
                &["edit", "-o", &format!("20:200:300:{}", pdf.display())],
            )
            .unwrap();
        }
        let cli_back = t.elapsed() / 5;
        let t = Instant::now();
        for _ in 0..100 {
            assert!(idx.backward(20, 200.0, 300.0).is_some());
        }
        let new_back = t.elapsed() / 100;
        eprintln!(
            "long document ({} KB synctex.gz): parse {parse:?}, cache hit {cached_load:?}\n  forward  CLI {cli_fwd:?}, parsed {new_fwd:?}\n  backward CLI {cli_back:?}, parsed {new_back:?}",
            size / 1024
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unreadable_synctex_file_falls_back_to_the_command() {
        let dir = std::env::temp_dir().join(format!("lemmata-synctex-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("main.pdf");
        std::fs::write(dir.join("main.synctex.gz"), b"not gzip").unwrap();
        // The command is missing too, so the fallback shows up as its spawn error.
        let err = forward(
            Path::new("synctex-missing"),
            &pdf,
            &dir.join("main.tex"),
            1,
            1,
        );
        assert!(matches!(err, Err(Error::Spawn(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
