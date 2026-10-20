//! History commands: thin wrappers around lemmata-history for the open project.

use crate::{CmdResult, LockSafe, Shell, err};
use lemmata_history::{Error, History, Snapshot};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;

/// Commands run in parallel now; two git commands on one repository at once fail on its index lock.
static GIT: Mutex<()> = Mutex::new(());

#[derive(Serialize, Clone)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum HistoryStatus {
    On,
    /// The folder isn't a git repository; history can be turned on.
    NotARepo,
    /// Git isn't installed.
    NoGit {
        message: String,
    },
}

fn root(state: &Shell) -> CmdResult<PathBuf> {
    let guard = state.project.lock_safe();
    Ok(guard.as_ref().ok_or("No project is open.")?.root.clone())
}

/// Runs `f` on the project's history, one git operation at a time.
fn with<T>(state: &Shell, f: impl FnOnce(&History) -> lemmata_history::Result<T>) -> CmdResult<T> {
    let root = root(state)?;
    let _one_at_a_time = GIT.lock_safe();
    f(&History::open(&root).map_err(err)?).map_err(err)
}

/// Snapshot ids are commit hashes; anything else (an option such as `--help`) is refused.
fn snapshot_id(id: &str) -> CmdResult<&str> {
    if id.len() >= 4 && id.len() <= 64 && id.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(id)
    } else {
        Err(format!("{id} is not a snapshot."))
    }
}

impl Shell {
    pub fn history_status(&self) -> CmdResult<HistoryStatus> {
        let root = root(self)?;
        let _one_at_a_time = GIT.lock_safe();
        Ok(match History::open(&root) {
            Ok(_) => HistoryStatus::On,
            Err(Error::NotARepo(_)) => HistoryStatus::NotARepo,
            Err(e @ Error::NoGit) => HistoryStatus::NoGit {
                message: e.to_string(),
            },
            Err(e) => return Err(e.to_string()),
        })
    }

    /// Files that differ from the last commit in the user's own git repository.
    pub fn git_status(&self) -> CmdResult<Vec<lemmata_history::FileStatus>> {
        Ok(lemmata_history::working_status(&root(self)?))
    }

    pub fn history_enable(&self) -> CmdResult<()> {
        let root = root(self)?;
        let _one_at_a_time = GIT.lock_safe();
        let h = History::init(&root).map_err(err)?;
        h.snapshot("History turned on").map_err(err)?;
        Ok(())
    }

    pub fn history_snapshot(&self, label: String) -> CmdResult<Option<String>> {
        with(self, |h| h.snapshot(&label))
    }

    pub fn history_list(&self) -> CmdResult<Vec<Snapshot>> {
        with(self, |h| h.list(200))
    }

    pub fn history_file(&self, id: String, path: String) -> CmdResult<Option<String>> {
        crate::files::safe_rel(&path)?;
        let id = snapshot_id(&id)?;
        with(self, |h| h.file_at(id, &path))
    }

    pub fn history_restore(&self, id: String, path: String) -> CmdResult<()> {
        crate::files::safe_rel(&path)?;
        let id = snapshot_id(&id)?;
        with(self, |h| h.restore_file(id, &path))
    }

    pub fn history_restore_all(&self, id: String) -> CmdResult<usize> {
        let id = snapshot_id(&id)?;
        with(self, |h| h.restore_all(id))
    }

    pub fn history_compare(&self, a: String, b: String) -> CmdResult<Vec<lemmata_history::Change>> {
        let (a, b) = (snapshot_id(&a)?, snapshot_id(&b)?);
        with(self, |h| h.compare(a, b))
    }

    pub fn history_relabel(&self, id: String, label: String) -> CmdResult<()> {
        let id = snapshot_id(&id)?;
        with(self, |h| h.relabel(id, &label))
    }

    /// A PDF of the changes since snapshot `id`, made with latexdiff: the old main
    /// file and everything it includes are compared with the current files.
    /// Returns the PDF's path.
    pub fn history_latexdiff(&self, id: String) -> CmdResult<PathBuf> {
        let id = snapshot_id(&id)?.to_string();
        let project = self
            .project
            .lock_safe()
            .clone()
            .ok_or("No project is open.")?;
        let known = self.detection.lock_safe().clone();
        let detection = match known {
            Some(d) => d,
            None => crate::first_or_fresh_detection(self, false),
        };
        let latexdiff = lemmata_history::find_latexdiff(detection.bin_dir.as_deref())
            .ok_or(lemmata_history::MISSING_LATEXDIFF)?;
        let latexmk = detection
            .tool("latexmk")
            .ok_or("latexmk was not found. Install TeX Live first.")?;

        let old = project
            .root
            .join(lemmata_history::LATEXDIFF_DIR)
            .join("old");
        with(self, |h| h.extract_tree(&id, &old))?;

        let miktex = matches!(
            detection.distribution,
            lemmata_tex::Distribution::Miktex { .. }
        );
        let engine = project.config.engine;
        let tools = lemmata_history::Tools {
            latexdiff: &latexdiff,
            latexmk,
            engine_flag: engine.latexmk_flag(),
            installer: Some(detection.installer(lemmata_tex::os::detect().family)),
            extra_args: if miktex {
                vec![format!("-{0}={0} --enable-installer %O %S", engine.binary()).into()]
            } else {
                Vec::new()
            },
            path_dirs: [
                detection.bin_dir.clone(),
                detection
                    .tool("perl")
                    .and_then(|p| p.parent().map(PathBuf::from)),
            ]
            .into_iter()
            .flatten()
            .collect(),
            timeout: std::time::Duration::from_secs(120),
        };
        lemmata_history::latexdiff_pdf(&project.root, &project.main, &tools).map_err(err)
    }
}
