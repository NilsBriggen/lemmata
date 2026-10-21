//! Git panel commands: thin async wrappers around lemmata-git for the open project.
//! Each runs on a blocking thread, so a slow network never freezes the window.

use crate::{CmdResult, LockSafe, Shell, err};
use lemmata_git as g;
use std::path::PathBuf;
use std::sync::Mutex;

/// Two git commands on one repository at once fail on its index lock.
static GIT: Mutex<()> = Mutex::new(());

fn root(state: &Shell) -> CmdResult<PathBuf> {
    let guard = state.project.lock_safe();
    Ok(guard.as_ref().ok_or("No project is open.")?.root.clone())
}

/// Runs `f` on the project's repository, one git operation at a time.
async fn with<T: Send + 'static>(
    state: &Shell,
    f: impl FnOnce(PathBuf) -> g::Result<T> + Send + 'static,
) -> CmdResult<T> {
    let root = root(state)?;
    tokio::task::spawn_blocking(move || {
        let _one_at_a_time = GIT.lock_safe();
        f(root).map_err(err)
    })
    .await
    .map_err(err)?
}

impl Shell {
    pub async fn git_overview(&self) -> CmdResult<g::Overview> {
        with(self, |r| Ok(g::overview(&r))).await
    }

    pub async fn git_init(&self) -> CmdResult<()> {
        with(self, |r| g::init(&r)).await
    }

    pub async fn git_stage(&self, paths: Vec<String>) -> CmdResult<()> {
        with(self, move |r| g::stage(&r, &paths)).await
    }

    pub async fn git_unstage(&self, paths: Vec<String>) -> CmdResult<()> {
        with(self, move |r| g::unstage(&r, &paths)).await
    }

    pub async fn git_stage_all(&self) -> CmdResult<()> {
        with(self, |r| g::stage_all(&r)).await
    }

    pub async fn git_commit(&self, message: String) -> CmdResult<()> {
        with(self, move |r| g::commit(&r, &message)).await
    }

    pub async fn git_fetch(&self) -> CmdResult<String> {
        with(self, |r| g::fetch(&r)).await
    }

    pub async fn git_pull(&self) -> CmdResult<String> {
        with(self, |r| g::pull(&r)).await
    }

    pub async fn git_push(&self) -> CmdResult<String> {
        with(self, |r| g::push(&r)).await
    }

    pub async fn git_branches(&self) -> CmdResult<Vec<String>> {
        with(self, |r| g::branches(&r)).await
    }

    pub async fn git_switch(&self, name: String) -> CmdResult<()> {
        with(self, move |r| g::switch_branch(&r, &name)).await
    }

    pub async fn git_create_branch(&self, name: String) -> CmdResult<()> {
        with(self, move |r| g::create_branch(&r, &name)).await
    }

    pub async fn git_log(&self) -> CmdResult<Vec<g::Commit>> {
        with(self, |r| g::log(&r)).await
    }

    /// Clones into `parent/name` (the name defaults to the URL's last part) and returns the new folder.
    /// Needs no open project: it runs from the start screen.
    pub async fn git_clone(&self, url: String, parent: String, name: String) -> CmdResult<String> {
        tokio::task::spawn_blocking(move || g::clone(&url, parent.as_ref(), &name))
            .await
            .map_err(err)?
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(err)
    }
}
