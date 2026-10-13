//! Snapshots of a project, stored as commits on the private ref
//! `refs/lemmata/snapshots` in the project's git repository.
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

pub struct History {
    git: PathBuf,
    root: PathBuf,
    git_dir: PathBuf,
}

impl History {
    /// Opens history for a project folder that is (inside) a git repository.
    pub fn open(root: &Path) -> Result<History> {
        let git = which::which("git").map_err(|_| Error::NoGit)?;
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
        };
        h.exclude_build_folder()?;
        Ok(h)
    }

    /// Makes `root` a git repository and opens its history.
    pub fn init(root: &Path) -> Result<History> {
        let git = which::which("git").map_err(|_| Error::NoGit)?;
        check(
            Command::new(&git)
                .arg("-C")
                .arg(root)
                .args(["init", "--quiet"])
                .output()?,
        )?;
        let ignore = root.join(".gitignore");
        if !ignore.exists() {
            std::fs::write(&ignore, "# Lemmata's build output\n.lemmata/\n")?;
        }
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
        c.arg("-C")
            .arg(&self.root)
            // A private index: the user's staging area stays as it is.
            .env("GIT_INDEX_FILE", self.git_dir.join("lemmata-index"))
            .env("GIT_AUTHOR_NAME", "Lemmata")
            .env("GIT_AUTHOR_EMAIL", "snapshots@lemmata.invalid")
            .env("GIT_COMMITTER_NAME", "Lemmata")
            .env("GIT_COMMITTER_EMAIL", "snapshots@lemmata.invalid");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
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
        self.run(&["notes", "--ref", NOTES_REF, "add", "-f", "-m", label.trim(), id])?;
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
        let h = History::init(root).unwrap();
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
        assert_eq!(fs::read(root.join("keep.bin")).unwrap(), [0u8, 159, 146, 150]);

        // The state before restoring was saved, so the restore can be undone.
        let labels: Vec<_> = h.list(10).unwrap().into_iter().map(|s| s.label).collect();
        assert!(labels.contains(&"Before restoring the project".to_string()));
        let saved = h.list(10).unwrap()[0].id.clone();
        assert_eq!(h.file_at(&saved, "main.tex").unwrap().as_deref(), Some("three"));
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
            [("modified", "a.tex"), ("deleted", "b.tex"), ("added", "c.tex")]
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
