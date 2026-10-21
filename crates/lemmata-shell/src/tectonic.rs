//! The optional Tectonic engine: a TeX compiler Lemmata downloads on request,
//! for people without TeX Live or MiKTeX.

use crate::{LockSafe, Shell};
use lemmata_tex::tectonic as tex;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct TectonicStatus {
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    /// False on platforms the app has no pinned download for.
    pub supported: bool,
    /// Size of the download, in megabytes.
    pub download_mb: Option<u64>,
}

fn install_dir(state: &Shell) -> Result<PathBuf, String> {
    Ok(state.dirs.data.join("tectonic"))
}

/// Tells detection where the downloaded binary lives. Call before detection starts.
pub(crate) fn init(state: &Shell) {
    if let Ok(dir) = install_dir(state) {
        tex::set_install_dir(dir);
    }
}

fn status() -> TectonicStatus {
    let path = tex::find();
    TectonicStatus {
        installed: path.is_some(),
        version: path.as_deref().and_then(tex::version),
        path: path.map(|p| p.to_string_lossy().into_owned()),
        supported: tex::release().is_some(),
        download_mb: tex::release().map(|r| r.size.div_ceil(1_000_000)),
    }
}

impl Shell {
    pub fn tectonic_status(&self) -> TectonicStatus {
        status()
    }

    /// Downloads and verifies the pinned Tectonic. Blocks until done (tens of MB).
    pub fn tectonic_install(&self) -> Result<TectonicStatus, String> {
        let dir = install_dir(self)?;
        tex::set_install_dir(dir.clone());
        tex::install(&dir).map_err(crate::err)?;
        // Builds use the cached detection: refresh it so they find Tectonic now.
        *self.startup_detection.lock_safe() = None;
        crate::run_detection(self);
        Ok(status())
    }
}

/// What the build status says while Tectonic downloads the packages a document needs.
pub(crate) fn fetch_message(files: usize) -> String {
    format!(
        "Tectonic is fetching the packages this document needs (first build only): {files} file{} so far",
        if files == 1 { "" } else { "s" }
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn fetch_message_counts_files() {
        assert!(super::fetch_message(1).ends_with("1 file so far"));
        assert!(super::fetch_message(37).ends_with("37 files so far"));
    }
}
