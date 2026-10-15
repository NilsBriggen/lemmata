//! Snapshots of a project, stored as commits on the private ref
//! `refs/lemmata/snapshots`: in the project's own git repository when it is
//! one, otherwise in a repository of Lemmata's own at `.lemmata/history`
//! (the project folder stays a plain folder).
//!
//! Every git call uses a separate index file, so the user's HEAD, branches,
//! staging area and stash are never touched. Snapshots respect .gitignore.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const SNAPSHOT_REF: &str = "refs/lemmata/snapshots";
/// Renamed snapshots keep their new label as a git note under this ref.
pub const NOTES_REF: &str = "refs/notes/lemmata";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "Git isn't installed, so history is off. Install git to keep snapshots of your project."
    )]
    NoGit,
    #[error("{0} isn't a git repository yet.")]
    NotARepo(PathBuf),
    #[error("git failed: {0}")]
    Git(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Snapshot {
    pub id: String,
    /// Seconds since the Unix epoch.
    pub time: i64,
    pub label: String,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Change {
    /// "added", "modified" or "deleted".
    pub status: &'static str,
    pub path: String,
}

/// A file's state in the user's own working tree, as `git status` sees it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileStatus {
    /// Relative to the project folder, with forward slashes.
    pub path: String,
    /// "M" modified, "A" added, "D" deleted, "R" renamed, "?" untracked,
    /// "U" conflicted.
    pub status: String,
}

/// Turns `git status --porcelain=v1 -z` output into statuses for files
/// inside `prefix` (the project's folder relative to the repository root,
/// "" or ending in "/").
pub fn parse_status(z: &str, prefix: &str) -> Vec<FileStatus> {
    let mut out = Vec::new();
    let mut parts = z.split('\0');
    while let Some(entry) = parts.next() {
        // "XY path": two status letters, a space, then the path.
        if entry.len() <= 3 || !entry.is_char_boundary(3) {
            continue;
        }
        let (xy, path) = (&entry[..2], &entry[3..]);
        let mut chars = xy.chars();
        let (x, y) = (chars.next().unwrap_or(' '), chars.next().unwrap_or(' '));
        // A rename lists the old name as the next entry.
        if x == 'R' || x == 'C' {
            parts.next();
        }
        let status = match (x, y) {
            ('?', '?') => "?",
            ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D') => "U",
            ('R', _) | ('C', _) => "R",
            ('A', _) => "A",
            ('D', _) | (_, 'D') => "D",
            _ => "M",
        };
        if let Some(rel) = path.strip_prefix(prefix) {
            out.push(FileStatus {
                path: rel.to_string(),
                status: status.into(),
            });
        }
    }
    out
}

/// No console window flashes up on Windows.
fn no_window(c: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let _ = c;
}

/// Which files of the project differ from the last commit. Empty when the
/// folder isn't in a git repository (or git isn't installed). Never changes
/// anything in the repository.
pub fn working_status(root: &Path) -> Vec<FileStatus> {
    let Ok(git) = which::which("git") else {
        return Vec::new();
    };
    let run = |args: &[&str]| {
        let mut c = Command::new(&git);
        c.arg("-C")
            .arg(root)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .args(args);
        no_window(&mut c);
        c.output().ok().filter(|o| o.status.success())
    };
    let Some(prefix) = run(&["rev-parse", "--show-prefix"]) else {
        return Vec::new();
    };
    let prefix = String::from_utf8_lossy(&prefix.stdout).trim().to_string();
    let Some(out) = run(&[
        "status",
        "--porcelain=v1",
        "-z",
        "-uall",
        "--ignore-submodules",
    ]) else {
        return Vec::new();
    };
    parse_status(&String::from_utf8_lossy(&out.stdout), &prefix)
}

/// Lemmata's own snapshot store for projects that aren't git repositories.
pub const STORE: &str = ".lemmata/history";

pub struct History {
    git: PathBuf,
    root: PathBuf,
    git_dir: PathBuf,
    /// The store is `STORE`, not the user's repository: git is told where both live.
    separate: bool,
}

impl History {
    /// Opens history for a project folder: Lemmata's own store if it has one,
    /// else the git repository the folder is (inside of).
    pub fn open(root: &Path) -> Result<History> {
        let git = which::which("git").map_err(|_| Error::NoGit)?;
        let store = root.join(STORE);
        if store.join("HEAD").is_file() {
            let h = History {
                git,
                root: root.to_path_buf(),
                git_dir: store,
                separate: true,
            };
            h.exclude_build_folder()?;
            return Ok(h);
        }
        let out = Command::new(&git)
            .arg("-C")
            .arg(root)
            .args(["rev-parse", "--absolute-git-dir"])
            .output()?;
        if !out.status.success() {
            return Err(Error::NotARepo(root.to_path_buf()));
        }
        let git_dir = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
        let h = History {
            git,
            root: root.to_path_buf(),
            git_dir,
            separate: false,
        };
        h.exclude_build_folder()?;
        Ok(h)
    }

    /// Creates Lemmata's own snapshot store in `.lemmata/history` and opens
    /// it. Nothing else in the project folder changes.
    pub fn init(root: &Path) -> Result<History> {
        let git = which::which("git").map_err(|_| Error::NoGit)?;
        let store = root.join(STORE);
        std::fs::create_dir_all(&store)?;
        let mut init = Command::new(&git);
        init.arg("--git-dir").arg(&store).args(["init", "--quiet"]);
        no_window(&mut init);
        check(init.output()?)?;
        // The work tree is passed on every call, so a moved project keeps its history.
        let mut unset = Command::new(&git);
        unset
            .arg("--git-dir")
            .arg(&store)
            .args(["config", "--unset", "core.worktree"]);
        no_window(&mut unset);
        let _ = unset.output();
        History::open(root)
    }

    /// Keeps `.lemmata/` out of snapshots without editing the user's .gitignore.
    fn exclude_build_folder(&self) -> Result<()> {
        let info = self.git_dir.join("info");
        let exclude = info.join("exclude");
        let current = std::fs::read_to_string(&exclude).unwrap_or_default();
        if !current.lines().any(|l| l.trim() == ".lemmata/") {
            std::fs::create_dir_all(&info)?;
            let sep = if current.is_empty() || current.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            std::fs::write(&exclude, format!("{current}{sep}.lemmata/\n"))?;
        }
        Ok(())
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new(&self.git);
        if self.separate {
            c.env("GIT_DIR", &self.git_dir)
                .env("GIT_WORK_TREE", &self.root);
        }
        c.arg("-C")
            .arg(&self.root)
            // A private index: the user's staging area stays as it is.
            .env("GIT_INDEX_FILE", self.git_dir.join("lemmata-index"))
            .env("GIT_AUTHOR_NAME", "Lemmata")
            .env("GIT_AUTHOR_EMAIL", "snapshots@lemmata.invalid")
            .env("GIT_COMMITTER_NAME", "Lemmata")
            .env("GIT_COMMITTER_EMAIL", "snapshots@lemmata.invalid");
        no_window(&mut c);
        c
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        let out = check(self.cmd().args(args).output()?)?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn latest(&self) -> Option<String> {
        self.run(&["rev-parse", "--verify", "--quiet", SNAPSHOT_REF])
            .ok()
            .map(|s| s.trim().to_string())
    }

    /// Records the current state of the project folder. Returns `None` when
    /// nothing changed since the last snapshot.
    pub fn snapshot(&self, label: &str) -> Result<Option<String>> {
        // Snapshot only this project's folder, even inside a bigger repo.
        self.run(&["add", "--all", "--", "."])?;
        let tree = self.run(&["write-tree"])?.trim().to_string();
        let parent = self.latest();
        if let Some(p) = &parent {
            let parent_tree = self.run(&["rev-parse", &format!("{p}^{{tree}}")])?;
            if parent_tree.trim() == tree {
                return Ok(None);
            }
        }
        let message = if label.trim().is_empty() {
            "Snapshot"
        } else {
            label.trim()
        };
        let mut args = vec!["commit-tree", tree.as_str(), "-m", message];
        if let Some(p) = &parent {
            args.extend(["-p", p.as_str()]);
        }
        let id = self.run(&args)?.trim().to_string();
        let mut update = vec!["update-ref", SNAPSHOT_REF, id.as_str()];
        if let Some(p) = &parent {
            update.push(p.as_str());
        }
        self.run(&update)?;
        Ok(Some(id))
    }

    /// Snapshots, newest first.
    pub fn list(&self, limit: usize) -> Result<Vec<Snapshot>> {
        if self.latest().is_none() {
            return Ok(Vec::new());
        }
        let log = self.run(&[
            "log",
            &format!("--max-count={limit}"),
            &format!("--notes={NOTES_REF}"),
            "--format=%x1e%H%x1f%ct%x1f%s%x1f%N",
            "--name-status",
            "--no-renames",
            "--root",
            SNAPSHOT_REF,
            "--",
            ".",
        ])?;
        Ok(log.split('\x1e').filter_map(parse_entry).collect())
    }

    /// A file's bytes in a snapshot, or `None` if it didn't exist then.
    fn bytes_at(&self, id: &str, path: &str) -> Result<Option<Vec<u8>>> {
        let prefix = self.run(&["rev-parse", "--show-prefix"])?;
        let spec = format!("{id}:{}{}", prefix.trim(), path);
        match self.cmd().args(["show", &spec]).output() {
            Ok(o) if o.status.success() => Ok(Some(o.stdout)),
            _ => Ok(None),
        }
    }

    /// A file's contents in a snapshot, or `None` if it didn't exist then.
    pub fn file_at(&self, id: &str, path: &str) -> Result<Option<String>> {
        Ok(self
            .bytes_at(id, path)?
            .map(|b| String::from_utf8_lossy(&b).into_owned()))
    }

    fn write_back(&self, id: &str, path: &str) -> Result<()> {
        match self.bytes_at(id, path)? {
            Some(bytes) => {
                let dest = self.root.join(path);
                if let Some(dir) = dest.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(dest, bytes)?;
            }
            None => {
                let _ = std::fs::remove_file(self.root.join(path));
            }
        }
        Ok(())
    }

    /// Puts one file back as it was in a snapshot, after snapshotting the
    /// current state so the restore can be undone.
    pub fn restore_file(&self, id: &str, path: &str) -> Result<()> {
        self.snapshot("Before restoring a file")?;
        self.write_back(id, path)
    }

    /// Puts the whole project back as it was in a snapshot: files from the
    /// snapshot are rewritten, files that didn't exist then are removed. The
    /// current state is snapshotted first, so this can be undone.
    pub fn restore_all(&self, id: &str) -> Result<usize> {
        self.snapshot("Before restoring the project")?;
        let wanted = self.paths(&["ls-tree", "-r", "-z", "--name-only", id, "--", "."])?;
        let current = self.paths(&["ls-files", "-z", "--", "."])?;
        let mut changed = 0;
        for path in &wanted {
            let now = std::fs::read(self.root.join(path)).ok();
            if now != self.bytes_at(id, path)? {
                self.write_back(id, path)?;
                changed += 1;
            }
        }
        for path in current.iter().filter(|p| !wanted.contains(p)) {
            self.write_back(id, path)?;
            changed += 1;
        }
        Ok(changed)
    }

    fn paths(&self, args: &[&str]) -> Result<Vec<String>> {
        let out = self.run(args)?;
        Ok(out
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(String::from)
            .collect())
    }

    /// What changed going from snapshot `a` to snapshot `b`.
    pub fn compare(&self, a: &str, b: &str) -> Result<Vec<Change>> {
        let out = self.run(&[
            "diff-tree",
            "-r",
            "--no-renames",
            "--name-status",
            a,
            b,
            "--",
            ".",
        ])?;
        Ok(out
            .lines()
            .filter_map(|l| {
                let (status, path) = l.split_once('\t')?;
                Some(Change {
                    status: match status {
                        "A" => "added",
                        "D" => "deleted",
                        _ => "modified",
                    },
                    path: path.to_string(),
                })
            })
            .collect())
    }

    /// Gives a snapshot a new label. Snapshots themselves never change; the
    /// label is kept as a git note.
    pub fn relabel(&self, id: &str, label: &str) -> Result<()> {
        self.run(&[
            "notes",
            "--ref",
            NOTES_REF,
            "add",
            "-f",
            "-m",
            label.trim(),
            id,
        ])?;
        Ok(())
    }
}

fn check(out: Output) -> Result<Output> {
    if out.status.success() {
        Ok(out)
    } else {
        Err(Error::Git(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
}

fn parse_entry(entry: &str) -> Option<Snapshot> {
    let mut lines = entry.lines().filter(|l| !l.is_empty());
    let mut head = lines.next()?.split('\x1f');
    let id = head.next()?.to_string();
    let time = head.next()?.parse().ok()?;
    let subject = head.next().unwrap_or("").to_string();
    let note = head.next().unwrap_or("").trim().to_string();
    let label = if note.is_empty() { subject } else { note };
    let changes = lines
        .filter_map(|l| {
            let (status, path) = l.split_once('\t')?;
            let status = match status {
                "A" => "added",
                "D" => "deleted",
                _ => "modified",
            };
            Some(Change {
                status,
                path: path.to_string(),
            })
        })
        .collect();
    Some(Snapshot {
        id,
        time,
        label,
        changes,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn working_status_sees_real_changes() {
        let Ok(git) = which::which("git") else { return };
        let d = tempfile::tempdir().unwrap();
        let g = |args: &[&str]| {
            let ok = Command::new(&git)
                .arg("-C")
                .arg(d.path())
                .args([
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t.t",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        g(&["init", "-q"]);
        std::fs::write(d.path().join("main.tex"), "a").unwrap();
        std::fs::write(d.path().join("keep.tex"), "a").unwrap();
        g(&["add", "."]);
        g(&["commit", "-q", "-m", "first"]);
        std::fs::write(d.path().join("main.tex"), "b").unwrap();
        std::fs::write(d.path().join("new.tex"), "n").unwrap();
        let mut got: Vec<_> = working_status(d.path())
            .into_iter()
            .map(|f| (f.path, f.status))
            .collect();
        got.sort();
        assert_eq!(
            got,
            [
                ("main.tex".to_string(), "M".to_string()),
                ("new.tex".to_string(), "?".to_string())
            ]
        );
        // Outside a repository there is simply nothing to show.
        let plain = tempfile::tempdir().unwrap();
        assert!(working_status(plain.path()).is_empty());
    }

    #[test]
    fn parses_porcelain_status() {
        let z = " M main.tex\0?? notes.txt\0A  chapters/new.tex\0 D old.tex\0R  renamed.tex\0was.tex\0UU conflict.tex\0";
        let got: Vec<_> = parse_status(z, "")
            .into_iter()
            .map(|f| (f.path, f.status))
            .collect();
        assert_eq!(
            got,
            [
                ("main.tex", "M"),
                ("notes.txt", "?"),
                ("chapters/new.tex", "A"),
                ("old.tex", "D"),
                ("renamed.tex", "R"),
                ("conflict.tex", "U"),
            ]
            .map(|(a, b)| (a.to_string(), b.to_string()))
        );
    }

    #[test]
    fn status_paths_are_relative_to_the_project_folder() {
        let z = " M thesis/main.tex\0 M other/readme.md\0";
        let got = parse_status(z, "thesis/");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, "main.tex");
    }

    use super::*;
    use std::fs;

    fn git(root: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn snapshots_without_touching_the_users_git_state() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::write(root.join("main.tex"), "one").unwrap();
        fs::write(root.join(".gitignore"), "*.aux\n").unwrap();
        git(root, &["init", "-q"]);
        let h = History::open(root).unwrap();
        // The user has a staged change of their own.
        fs::write(root.join("notes.txt"), "mine").unwrap();
        git(root, &["add", "notes.txt"]);

        let first = h.snapshot("First").unwrap().unwrap();
        assert_eq!(h.snapshot("Nothing changed").unwrap(), None);
        fs::write(root.join("main.tex"), "two").unwrap();
        fs::create_dir_all(root.join(".lemmata/build")).unwrap();
        fs::write(root.join(".lemmata/build/main.pdf"), "pdf").unwrap();
        h.snapshot("").unwrap().unwrap();

        // User's index and HEAD are as they left them.
        assert_eq!(git(root, &["diff", "--cached", "--name-only"]), "notes.txt");
        assert_eq!(git(root, &["rev-parse", "--verify", "--quiet", "HEAD"]), "");

        let list = h.list(10).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].label, "Snapshot");
        assert_eq!(
            list[0].changes,
            vec![Change {
                status: "modified",
                path: "main.tex".into()
            }]
        );
        assert_eq!(list[1].id, first);
        assert!(
            list[1]
                .changes
                .iter()
                .any(|c| c.path == ".gitignore" && c.status == "added")
        );
        assert!(
            !list
                .iter()
                .flat_map(|s| &s.changes)
                .any(|c| c.path.starts_with(".lemmata"))
        );

        assert_eq!(
            h.file_at(&first, "main.tex").unwrap().as_deref(),
            Some("one")
        );
        assert_eq!(h.file_at(&first, "missing.tex").unwrap(), None);
    }

    #[test]
    fn a_plain_folder_gets_a_store_of_its_own() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("thesis");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("main.tex"), "one").unwrap();
        assert!(matches!(History::open(&root), Err(Error::NotARepo(_))));
        let h = History::init(&root).unwrap();
        let first = h.snapshot("First").unwrap().unwrap();
        // The folder itself is still not a repository, and nothing was added to it.
        assert_eq!(git(&root, &["rev-parse", "--is-inside-work-tree"]), "");
        assert!(!root.join(".git").exists() && !root.join(".gitignore").exists());
        assert!(working_status(&root).is_empty());
        // The history moves with the folder.
        let moved = d.path().join("moved");
        fs::rename(&root, &moved).unwrap();
        fs::write(moved.join("main.tex"), "two").unwrap();
        let h = History::open(&moved).unwrap();
        h.snapshot("Second").unwrap().unwrap();
        let list = h.list(10).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].id, first);
        assert_eq!(
            h.file_at(&first, "main.tex").unwrap().as_deref(),
            Some("one")
        );
        assert!(
            list.iter()
                .flat_map(|s| &s.changes)
                .all(|c| !c.path.starts_with(".lemmata"))
        );
    }

    #[test]
    fn restore_is_undoable() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::write(root.join("main.tex"), "draft 1").unwrap();
        let h = History::init(root).unwrap();
        let first = h.snapshot("v1").unwrap().unwrap();
        fs::write(root.join("main.tex"), "draft 2").unwrap();

        h.restore_file(&first, "main.tex").unwrap();
        assert_eq!(
            fs::read_to_string(root.join("main.tex")).unwrap(),
            "draft 1"
        );
        let list = h.list(10).unwrap();
        assert_eq!(list[0].label, "Before restoring a file");
        assert_eq!(
            h.file_at(&list[0].id, "main.tex").unwrap().as_deref(),
            Some("draft 2")
        );
    }

    #[test]
    fn project_in_a_subfolder_of_a_repo() {
        let d = tempfile::tempdir().unwrap();
        git(d.path(), &["init", "--quiet"]);
        let root = d.path().join("papers/thesis");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("main.tex"), "x").unwrap();
        fs::write(d.path().join("unrelated.txt"), "y").unwrap();
        let h = History::open(&root).unwrap();
        let id = h.snapshot("s").unwrap().unwrap();
        assert_eq!(h.file_at(&id, "main.tex").unwrap().as_deref(), Some("x"));
        let list = h.list(5).unwrap();
        assert_eq!(list[0].changes.len(), 1, "{:?}", list[0].changes);
    }

    #[test]
    fn not_a_repo() {
        let d = tempfile::tempdir().unwrap();
        // tempdir might sit inside a repo on some machines; only check the error type when it isn't.
        if Command::new("git")
            .arg("-C")
            .arg(d.path())
            .args(["rev-parse"])
            .status()
            .unwrap()
            .success()
        {
            return;
        }
        assert!(matches!(History::open(d.path()), Err(Error::NotARepo(_))));
    }

    #[test]
    fn restores_the_whole_project_and_keeps_a_way_back() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::write(root.join("main.tex"), "one").unwrap();
        fs::write(root.join("keep.bin"), [0u8, 159, 146, 150]).unwrap();
        let h = History::init(root).unwrap();
        let first = h.snapshot("First").unwrap().unwrap();

        fs::write(root.join("main.tex"), "two").unwrap();
        fs::write(root.join("new.tex"), "extra").unwrap();
        fs::remove_file(root.join("keep.bin")).unwrap();
        let second = h.snapshot("Second").unwrap().unwrap();
        // Unsnapshotted work in progress must survive too.
        fs::write(root.join("main.tex"), "three").unwrap();

        let changed = h.restore_all(&first).unwrap();
        assert!(changed >= 3);
        assert_eq!(fs::read_to_string(root.join("main.tex")).unwrap(), "one");
        assert!(!root.join("new.tex").exists());
        assert_eq!(
            fs::read(root.join("keep.bin")).unwrap(),
            [0u8, 159, 146, 150]
        );

        // The state before restoring was saved, so the restore can be undone.
        let labels: Vec<_> = h.list(10).unwrap().into_iter().map(|s| s.label).collect();
        assert!(labels.contains(&"Before restoring the project".to_string()));
        let saved = h.list(10).unwrap()[0].id.clone();
        assert_eq!(
            h.file_at(&saved, "main.tex").unwrap().as_deref(),
            Some("three")
        );
        h.restore_all(&second).unwrap();
        assert_eq!(fs::read_to_string(root.join("main.tex")).unwrap(), "two");
        assert!(root.join("new.tex").exists());
    }

    #[test]
    fn compares_two_snapshots() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::write(root.join("a.tex"), "1").unwrap();
        fs::write(root.join("b.tex"), "1").unwrap();
        let h = History::init(root).unwrap();
        let first = h.snapshot("First").unwrap().unwrap();
        fs::write(root.join("a.tex"), "2").unwrap();
        fs::remove_file(root.join("b.tex")).unwrap();
        fs::write(root.join("c.tex"), "3").unwrap();
        let second = h.snapshot("Second").unwrap().unwrap();
        let mut got = h.compare(&first, &second).unwrap();
        got.sort_by(|x, y| x.path.cmp(&y.path));
        let got: Vec<_> = got.iter().map(|c| (c.status, c.path.as_str())).collect();
        assert_eq!(
            got,
            [
                ("modified", "a.tex"),
                ("deleted", "b.tex"),
                ("added", "c.tex")
            ]
        );
    }

    #[test]
    fn relabels_a_snapshot() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::write(root.join("a.tex"), "1").unwrap();
        let h = History::init(root).unwrap();
        let id = h.snapshot("Automatic snapshot").unwrap().unwrap();
        h.relabel(&id, "Sent to supervisor").unwrap();
        assert_eq!(h.list(5).unwrap()[0].label, "Sent to supervisor");
    }
}
