//! The Git panel's backend. Everything goes through the system `git` (like
//! lemmata-history), so the user's credential helper, SSH keys and config
//! apply. Nothing here rewrites history or discards work: no reset --hard, no
//! force push, no clean, no checkout of files.
//!
//! Every command runs with `GIT_TERMINAL_PROMPT=0` and a timeout, so a
//! missing password produces an error instead of a hang.

use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const LOCAL: Duration = Duration::from_secs(30);
const NETWORK: Duration = Duration::from_secs(60);
/// Cloning a large repository legitimately takes longer than a fetch.
const CLONE: Duration = Duration::from_secs(300);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Git isn't installed. Install git to use the Git panel.")]
    NoGit,
    #[error("This folder isn't a git repository.")]
    NotARepo,
    #[error("{0}")]
    Msg(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

fn msg<T>(s: impl Into<String>) -> Result<T> {
    Err(Error::Msg(s.into()))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileChange {
    /// Relative to the repository root, with forward slashes.
    pub path: String,
    /// "M" modified, "A" added, "D" deleted, "R" renamed, "?" untracked, "U" conflicted.
    pub status: String,
    /// The old name of a renamed file.
    pub from: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// None while HEAD is detached.
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub has_commits: bool,
    pub staged: Vec<FileChange>,
    pub unstaged: Vec<FileChange>,
    pub untracked: Vec<FileChange>,
    pub conflicted: Vec<FileChange>,
}

impl Status {
    /// Tracked files differ from HEAD (untracked files don't count).
    pub fn dirty(&self) -> bool {
        !(self.staged.is_empty() && self.unstaged.is_empty() && self.conflicted.is_empty())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Overview {
    NoGit { message: String },
    NotARepo,
    Repo(Box<Status>),
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
    pub author: String,
    /// "3 days ago".
    pub date: String,
}

/// Turns `git status --porcelain=v2 --branch -z` output into a [`Status`].
pub fn parse_status(z: &str) -> Status {
    let mut s = Status::default();
    let mut parts = z.split('\0');
    let letter = |c: char| match c {
        'A' => "A",
        'D' => "D",
        'R' | 'C' => "R",
        _ => "M",
    };
    while let Some(e) = parts.next() {
        if let Some(h) = e.strip_prefix("# ") {
            match h.split_once(' ') {
                Some(("branch.oid", oid)) => s.has_commits = oid != "(initial)",
                Some(("branch.head", b)) => s.branch = (b != "(detached)").then(|| b.to_string()),
                Some(("branch.upstream", u)) => s.upstream = Some(u.to_string()),
                Some(("branch.ab", ab)) => {
                    let n = |t: Option<&str>| t.and_then(|t| t[1..].parse().ok()).unwrap_or(0);
                    let mut it = ab.split(' ');
                    (s.ahead, s.behind) = (n(it.next()), n(it.next()));
                }
                _ => {}
            }
            continue;
        }
        let kind = e.chars().next().unwrap_or(' ');
        let fields = match kind {
            '1' => 9,
            '2' => 10,
            'u' => 11,
            '?' => {
                if let Some(path) = e.strip_prefix("? ") {
                    s.untracked.push(FileChange {
                        path: path.into(),
                        status: "?".into(),
                        from: None,
                    });
                }
                continue;
            }
            _ => continue,
        };
        let f: Vec<&str> = e.splitn(fields, ' ').collect();
        let (Some(xy), Some(path)) = (f.get(1), f.last()) else {
            continue;
        };
        let from = (kind == '2').then(|| parts.next().unwrap_or("").to_string());
        let mut c = xy.chars();
        let (x, y) = (c.next().unwrap_or('.'), c.next().unwrap_or('.'));
        let change = |st: &str| FileChange {
            path: path.to_string(),
            status: st.into(),
            from: from.clone(),
        };
        if kind == 'u' {
            s.conflicted.push(change("U"));
            continue;
        }
        if x != '.' {
            s.staged.push(change(letter(x)));
        }
        if y != '.' {
            s.unstaged.push(change(letter(y)));
        }
    }
    s
}

/// The git program, if there is a usable one. On macOS, /usr/bin/git is a stub
/// that opens an install dialog when the Command Line Tools are missing.
fn find_git() -> Option<PathBuf> {
    let git = which::which("git").ok()?;
    if cfg!(target_os = "macos") && git == Path::new("/usr/bin/git") {
        let tools = Command::new("xcode-select").arg("-p").output();
        return tools.is_ok_and(|o| o.status.success()).then_some(git);
    }
    Some(git)
}

/// No console window flashes up on Windows, and git runs with the user's
/// environment, not an AppImage's (see lemmata-env).
fn no_window(c: &mut Command) {
    lemmata_env::host(c);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let _ = c;
}

struct Out {
    ok: bool,
    stdout: String,
    stderr: String,
}

/// Runs git with no stdin, never prompting, killed after `timeout`.
fn exec(git: &Path, dir: Option<&Path>, args: &[&str], timeout: Duration) -> Result<Out> {
    let mut c = Command::new(git);
    if let Some(d) = dir {
        c.arg("-C").arg(d);
    }
    c.args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_LITERAL_PATHSPECS", "1")
        // English messages, so the error handling below can recognise them.
        .env("LC_MESSAGES", "C")
        .env("LANGUAGE", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // ssh would ask for a passphrase on the terminal; fail instead, unless the user chose their own command.
    let network = matches!(args.first(), Some(&("clone" | "fetch" | "pull" | "push")));
    if network && std::env::var_os("GIT_SSH_COMMAND").is_none() {
        let own = lemmata_env::host(&mut Command::new(git))
            .args(["config", "core.sshCommand"])
            .output();
        if own.is_ok_and(|o| o.stdout.is_empty()) {
            c.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
        }
    }
    no_window(&mut c);
    let mut child = c.spawn()?;
    let drain = |mut r: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut v = Vec::new();
            let _ = r.read_to_end(&mut v);
            String::from_utf8_lossy(&v).into_owned()
        })
    };
    let (so, se) = (
        drain(Box::new(child.stdout.take().unwrap())),
        drain(Box::new(child.stderr.take().unwrap())),
    );
    let start = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if start.elapsed() > timeout {
            // The reader threads are left to finish on their own: a helper process may still hold the pipes.
            let _ = child.kill();
            let _ = child.wait();
            return msg(format!(
                "Git took longer than {} seconds and was stopped. Check your connection and try again.",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(15));
    };
    Ok(Out {
        ok: status.success(),
        stdout: so.join().unwrap_or_default(),
        stderr: se.join().unwrap_or_default(),
    })
}

/// Git's message, trimmed, with passwords in URLs hidden.
fn tidy(s: &str) -> String {
    let mut out = String::new();
    for line in s.trim().lines() {
        let mut rest = line;
        while let Some(i) = rest.find("://") {
            let (head, tail) = rest.split_at(i + 3);
            out.push_str(head);
            let end = tail.find(['/', ' ']).unwrap_or(tail.len());
            // "https://user:secret@host/x" -> "https://***@host/x"
            match tail[..end].rfind('@') {
                Some(at) => {
                    out.push_str("***");
                    rest = &tail[at..];
                }
                None => rest = tail,
            }
        }
        out.push_str(rest);
        out.push('\n');
    }
    let out = out.trim_end();
    match out.char_indices().nth(800) {
        Some((i, _)) => format!("{}…", &out[..i]),
        None => out.to_string(),
    }
}

fn auth_failure(stderr: &str) -> bool {
    [
        "Authentication failed",
        "could not read Username",
        "could not read Password",
        "terminal prompts disabled",
        "Permission denied",
        "Invalid username or password",
        "Host key verification failed",
        "The requested URL returned error: 403",
        "The requested URL returned error: 401",
    ]
    .iter()
    .any(|m| stderr.contains(m))
}

/// A failed network command, verbatim but trimmed, with a hint when it was about signing in.
fn net_error(stderr: &str) -> Error {
    let mut m = tidy(stderr);
    if auth_failure(stderr) {
        m.push_str(
            "\n\nLemmata uses the system git and its saved credentials (the credential manager or your SSH keys). \
             Run `git push` or `git pull` once in a terminal in this folder to sign in, then try again.",
        );
    }
    Error::Msg(m)
}

struct Repo {
    git: PathBuf,
    top: PathBuf,
}

impl Repo {
    fn open(root: &Path) -> Result<Repo> {
        let git = find_git().ok_or(Error::NoGit)?;
        let o = exec(&git, Some(root), &["rev-parse", "--show-toplevel"], LOCAL)?;
        if !o.ok {
            return if o.stderr.contains("not a git repository") {
                Err(Error::NotARepo)
            } else {
                msg(tidy(&o.stderr))
            };
        }
        Ok(Repo {
            git,
            top: PathBuf::from(o.stdout.trim()),
        })
    }

    fn run(&self, args: &[&str], timeout: Duration) -> Result<Out> {
        exec(&self.git, Some(&self.top), args, timeout)
    }

    /// Stdout on success; git's trimmed message otherwise.
    fn ok(&self, args: &[&str]) -> Result<String> {
        let o = self.run(args, LOCAL)?;
        if o.ok {
            Ok(o.stdout)
        } else {
            msg(tidy(&o.stderr))
        }
    }

    fn status(&self) -> Result<Status> {
        let o = self.run(
            &[
                "--no-optional-locks",
                "status",
                "--porcelain=v2",
                "--branch",
                "-z",
            ],
            LOCAL,
        )?;
        if !o.ok {
            return msg(tidy(&o.stderr));
        }
        Ok(parse_status(&o.stdout))
    }

    fn paths(&self, verb: &[&str], paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args: Vec<&str> = verb.to_vec();
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
        self.ok(&args).map(|_| ())
    }
}

const NO_IDENTITY: &str = "Git doesn't know who you are yet, so it can't sign a commit. Run these two commands in a terminal, then try again:\n\n\
    git config --global user.name \"Your Name\"\n\
    git config --global user.email \"you@example.com\"";

fn check_identity(git: &Path, dir: &Path) -> Result<()> {
    // No guessing from the login and computer name: on a Mac that gives something like
    // you@Your-MacBook.local, which git accepts and GitHub or Overleaf can't link to anyone.
    let args = ["-c", "user.useConfigOnly=true", "var", "GIT_AUTHOR_IDENT"];
    let o = exec(git, Some(dir), &args, LOCAL)?;
    if o.ok { Ok(()) } else { msg(NO_IDENTITY) }
}

pub fn overview(root: &Path) -> Overview {
    match Repo::open(root).and_then(|r| r.status()) {
        Ok(s) => Overview::Repo(Box::new(s)),
        Err(Error::NoGit) => Overview::NoGit {
            message: Error::NoGit.to_string(),
        },
        Err(Error::NotARepo) => Overview::NotARepo,
        // A broken repository still gets a panel with the reason.
        Err(e) => Overview::NoGit {
            message: e.to_string(),
        },
    }
}

/// `git init` and a first commit of everything (with a small .gitignore for
/// Lemmata's own folder when the project has none).
pub fn init(root: &Path) -> Result<()> {
    let git = find_git().ok_or(Error::NoGit)?;
    check_identity(&git, root)?;
    let run = |args: &[&str]| -> Result<()> {
        let o = exec(&git, Some(root), args, LOCAL)?;
        if o.ok { Ok(()) } else { msg(tidy(&o.stderr)) }
    };
    run(&["init"])?;
    let ignore = root.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(&ignore, ".lemmata/\n")?;
    }
    run(&["add", "-A"])?;
    run(&["commit", "-m", "Initial commit", "--allow-empty"])
}

pub fn stage(root: &Path, paths: &[String]) -> Result<()> {
    Repo::open(root)?.paths(&["add"], paths)
}

pub fn stage_all(root: &Path) -> Result<()> {
    Repo::open(root)?.ok(&["add", "-A"]).map(|_| ())
}

/// Takes files out of the next commit; their contents stay as they are.
pub fn unstage(root: &Path, paths: &[String]) -> Result<()> {
    let r = Repo::open(root)?;
    if r.status()?.has_commits {
        r.paths(&["restore", "--staged"], paths)
    } else {
        r.paths(&["rm", "--cached", "-r", "-q"], paths)
    }
}

pub fn commit(root: &Path, message: &str) -> Result<()> {
    let r = Repo::open(root)?;
    if message.trim().is_empty() {
        return msg("Write a commit message first.");
    }
    if r.status()?.staged.is_empty() {
        return msg("Nothing is staged. Tick the files to include, or press Stage all.");
    }
    check_identity(&r.git, &r.top)?;
    let o = r.run(&["commit", "-m", message], LOCAL)?;
    if o.ok {
        Ok(())
    } else if o.stderr.contains("Please tell me who you are") || o.stderr.contains("empty ident") {
        msg(NO_IDENTITY)
    } else {
        msg(tidy(&format!("{}\n{}", o.stdout, o.stderr)))
    }
}

pub fn fetch(root: &Path) -> Result<String> {
    let r = Repo::open(root)?;
    let o = r.run(&["fetch"], NETWORK)?;
    if o.ok {
        Ok("Fetched.".into())
    } else {
        Err(net_error(&o.stderr))
    }
}

pub fn pull(root: &Path) -> Result<String> {
    let r = Repo::open(root)?;
    let o = r.run(&["pull", "--ff-only"], NETWORK)?;
    if o.ok {
        return Ok(tidy(&o.stdout));
    }
    let e = &o.stderr;
    if [
        "Not possible to fast-forward",
        "divergent branches",
        "have diverged",
    ]
    .iter()
    .any(|m| e.contains(m))
    {
        msg(
            "Your branch and the remote have both changed. Pull with rebase or merge in a terminal.",
        )
    } else if e.contains("no tracking information") {
        msg(
            "This branch isn't linked to a remote branch yet. Push it first; that sets the link up.",
        )
    } else {
        Err(net_error(e))
    }
}

/// The first push of a branch sets its upstream (`-u`).
pub fn push(root: &Path) -> Result<String> {
    let r = Repo::open(root)?;
    let s = r.status()?;
    let Some(branch) = s.branch else {
        return msg("HEAD isn't on a branch. Switch to a branch before pushing.");
    };
    let o = if s.upstream.is_some() {
        r.run(&["push"], NETWORK)?
    } else {
        let remotes = r.ok(&["remote"])?;
        let mut names = remotes.lines();
        let Some(first) = names.next() else {
            return msg(
                "This repository has no remote yet. Add one in a terminal, for example:\n\ngit remote add origin <url>",
            );
        };
        let remote = if remotes.lines().any(|n| n == "origin") {
            "origin"
        } else {
            first
        };
        r.run(&["push", "-u", remote, &branch], NETWORK)?
    };
    if o.ok {
        Ok("Pushed.".into())
    } else if o.stderr.contains("non-fast-forward") || o.stderr.contains("fetch first") {
        msg("The remote has commits you don't have. Pull first, then push again.")
    } else {
        Err(net_error(&o.stderr))
    }
}

pub fn branches(root: &Path) -> Result<Vec<String>> {
    let out = Repo::open(root)?.ok(&["for-each-ref", "--format=%(refname:short)", "refs/heads"])?;
    Ok(out.lines().map(str::to_string).collect())
}

pub fn switch_branch(root: &Path, name: &str) -> Result<()> {
    let r = Repo::open(root)?;
    if !branches(root)?.iter().any(|b| b == name) {
        return msg(format!("There is no branch called {name}."));
    }
    if r.status()?.dirty() {
        return msg(
            "You have uncommitted changes. Commit them first, so switching branches can't mix up or lose your work.",
        );
    }
    r.ok(&["switch", "--no-guess", name]).map(|_| ())
}

/// A new branch at HEAD, which becomes the current one. Uncommitted work comes along unchanged.
pub fn create_branch(root: &Path, name: &str) -> Result<()> {
    let r = Repo::open(root)?;
    let name = name.trim();
    let valid = !name.is_empty()
        && !name.starts_with('-')
        && r.run(&["check-ref-format", "--branch", name], LOCAL)?.ok;
    if !valid {
        return msg(format!(
            "\"{name}\" can't be a branch name. Use letters, digits, - and /, without spaces."
        ));
    }
    if branches(root)?.iter().any(|b| b == name) {
        return msg(format!("A branch called {name} already exists."));
    }
    r.ok(&["switch", "-c", name]).map(|_| ())
}

/// The last 50 commits of the current branch.
pub fn log(root: &Path) -> Result<Vec<Commit>> {
    let r = Repo::open(root)?;
    if !r.status()?.has_commits {
        return Ok(Vec::new());
    }
    let out = r.ok(&["log", "-50", "--format=%h%x1f%s%x1f%an%x1f%ar%x1e"])?;
    Ok(out
        .split('\x1e')
        .filter_map(|rec| {
            let mut f = rec.trim_start_matches('\n').split('\x1f');
            Some(Commit {
                hash: f.next().filter(|h| !h.is_empty())?.into(),
                subject: f.next()?.into(),
                author: f.next()?.into(),
                date: f.next()?.into(),
            })
        })
        .collect())
}

/// The folder name git would pick for a URL: `https://git.overleaf.com/abc123` gives `abc123`.
pub fn repo_name(url: &str) -> String {
    let u = url.trim().trim_end_matches('/');
    let u = u.strip_suffix(".git").unwrap_or(u);
    let name = u.rsplit(['/', ':', '\\']).next().unwrap_or("");
    if name.is_empty() {
        "repository".into()
    } else {
        name.into()
    }
}

/// Clones `url` into `parent/name` (the name comes from the URL when empty) and returns that folder.
pub fn clone(url: &str, parent: &Path, name: &str) -> Result<PathBuf> {
    let url = url.trim();
    if url.is_empty() || url.starts_with('-') || url.contains("::") {
        return msg(
            "Enter the repository's URL, such as https://github.com/you/thesis.git or git@github.com:you/thesis.git.",
        );
    }
    let git = find_git().ok_or(Error::NoGit)?;
    let name = if name.trim().is_empty() {
        repo_name(url)
    } else {
        name.trim().to_string()
    };
    if name.contains(['/', '\\']) || name == ".." || name == "." {
        return msg("The folder name can't contain slashes.");
    }
    if !parent.is_dir() {
        return msg(format!("{} isn't a folder.", parent.display()));
    }
    let target = parent.join(&name);
    if target.exists() {
        return msg(format!(
            "{} already exists. Pick another folder name.",
            target.display()
        ));
    }
    let target_arg = target.to_string_lossy();
    let o = exec(&git, None, &["clone", "--", url, &target_arg], CLONE);
    let failure = match o {
        Ok(o) if o.ok => return Ok(target),
        Ok(o) => net_error(&o.stderr),
        Err(e) => e,
    };
    // The folder didn't exist before, so a half-finished clone is ours to remove.
    let _ = std::fs::remove_dir_all(&target);
    Err(failure)
}

#[cfg(test)]
mod tests;
