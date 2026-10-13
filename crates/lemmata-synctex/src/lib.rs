//! Forward search (source → PDF) and backward search (PDF → source) using
//! the `synctex` command that ships with TeX Live and MiKTeX.
//!
//! Coordinates are PDF points (1/72 in) measured from the top-left corner
//! of the page, which is what `synctex` reports.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not run synctex: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("synctex failed: {0}")]
    Failed(String),
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
    let output = format!("{page}:{x:.2}:{y:.2}:{}", pdf.display());
    let out = run(synctex, &["edit", "-o", &output])?;
    Ok(parse_edit(&out))
}

fn run(synctex: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new(synctex).args(args).output()?;
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
    use super::*;

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
}
