//! Exporting the PDF, and a source zip. The zip logic is shared with the
//! `lemmata` command, in `lemmata_project::export`.

use crate::{CmdResult, Shell, project};
use lemmata_project::export;
use std::path::Path;

impl Shell {
    pub fn export_pdf(&self, dest: String) -> CmdResult<()> {
        let p = project(self)?;
        if !p.pdf_path().is_file() {
            return Err("Build the project first; there's no PDF yet.".into());
        }
        export::atomic_copy(&p.pdf_path(), Path::new(&dest))
    }

    /// Zips the whole project (not hidden files or the build folder), for a
    /// backup or to share it. With `with_pdf`, the latest PDF is added too.
    pub fn export_zip(&self, dest: String, with_pdf: bool) -> CmdResult<usize> {
        export::zip_project(&project(self)?, Path::new(&dest), with_pdf)
    }

    /// Zips the files the last build used, plus the .bbl (arXiv doesn't run
    /// biber), ready to upload to arXiv. With `flatten`, the main file has its
    /// inputs inlined, and the files inlined are left out of the zip.
    pub fn export_arxiv(
        &self,
        dest: String,
        strip: bool,
        flatten: Option<bool>,
    ) -> CmdResult<usize> {
        export::zip_arxiv(
            &project(self)?,
            Path::new(&dest),
            strip,
            flatten.unwrap_or(false),
        )
    }
}
