//! Writing user files so a crash, a full disk or a link planted by a project can't damage them,
//! and checking names that come from snapshots, archives and documents before they become paths.

use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The most of a project's text file that is read: more is no LaTeX source.
pub const TEXT_LIMIT: u64 = 64 << 20;
/// The most of any other project file (a PDF, an image) that is read.
pub const FILE_LIMIT: u64 = 1 << 30;

/// The bytes of `path`, a file a project may have planted, at most `limit` of them. Only a
/// regular file is read: a link to /dev/zero never ends and one to a named pipe never answers.
pub fn read_limited(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    if !std::fs::metadata(path)?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(std::io::Error::other(format!(
            "larger than {} MB",
            limit >> 20
        )));
    }
    Ok(bytes)
}

/// `std::fs::read` for files a project may have planted (see `read_limited`).
pub fn read(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
    read_limited(path.as_ref(), FILE_LIMIT)
}

/// `std::fs::read_to_string` for files a project may have planted (see `read_limited`).
pub fn read_to_string(path: impl AsRef<Path>) -> std::io::Result<String> {
    String::from_utf8(read_limited(path.as_ref(), TEXT_LIMIT)?)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// `std::fs::copy` for files a project may have planted: only a regular file, as copying a link to
/// /dev/zero would fill the disk and one to a named pipe would wait forever.
pub fn copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> std::io::Result<u64> {
    if !std::fs::metadata(from.as_ref())?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} is not a regular file", from.as_ref().display()),
        ));
    }
    std::fs::copy(from, to)
}

/// `rel` as a path inside a folder: relative, made only of plain names (no `..`, no `.`, nothing
/// empty), and without a backslash or a colon, which are a separator and a drive letter on
/// Windows. Names from snapshots, archives and documents go through this before they are joined
/// onto a project folder.
pub fn inside_rel(rel: &str) -> Option<PathBuf> {
    let p = Path::new(rel);
    let plain = !rel.is_empty()
        && !rel.contains(['\\', ':'])
        && !rel.chars().any(char::is_control)
        && !p.is_absolute()
        && p.components().all(|c| matches!(c, Component::Normal(_)));
    plain.then(|| p.to_path_buf())
}

/// `path` with links resolved, as far as it exists; on Windows without the `\\?\` prefix
/// `canonicalize` adds.
fn resolved(path: &Path) -> PathBuf {
    let Ok(real) = path.canonicalize() else {
        return path.to_path_buf();
    };
    #[cfg(windows)]
    {
        let s = real.to_string_lossy();
        if let Some(unc) = s.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        if let Some(rest) = s.strip_prefix(r"\\?\")
            && rest.as_bytes().get(1) == Some(&b':')
        {
            return PathBuf::from(rest);
        }
    }
    real
}

/// Whether `path`, a file of the project in `root`, may be written. A link there is followed by
/// `stage`; one that leads out of the project only in a project the user trusts, so a project
/// can't point its `refs.bib` at `~/.bashrc` and have an edit or a sync overwrite it.
pub fn may_write(root: &Path, trusted: bool, path: &Path) -> std::io::Result<()> {
    // A file not written yet is resolved through its folder, which may be reached by a link
    // (macOS's /var is /private/var).
    // A link that leads nowhere would be written through to wherever it names.
    let target = match (path.symlink_metadata(), path.parent(), path.file_name()) {
        (Err(_), Some(dir), Some(name)) => Some(resolved(dir).join(name)),
        (Ok(m), ..) if m.file_type().is_symlink() => {
            path.canonicalize().ok().map(|_| resolved(path))
        }
        _ => Some(resolved(path)),
    };
    if trusted || target.is_some_and(|t| t.starts_with(resolved(root))) {
        return Ok(());
    }
    let name = path.strip_prefix(root).unwrap_or(path).display();
    Err(std::io::Error::other(format!(
        "{name} links to a file outside the project, and Lemmata writes through such a link only in a project you trust"
    )))
}

/// New text for one file, written beside it and not yet in place. Dropped without `commit`, it
/// leaves the file as it was.
pub struct Staged {
    tmp: PathBuf,
    target: PathBuf,
    bytes: Vec<u8>,
    done: bool,
}

/// Writes `bytes` beside `path` and waits until they are on the disk, so the rename in `commit`
/// can't leave an empty file after a power cut. A link at `path` is followed: the file it names
/// gets the text, the link stays (a bibliography shared between projects). The temporary file is
/// created anew under a fresh name, so a link a project planted there can't redirect the write.
pub fn stage(path: &Path, bytes: &[u8]) -> std::io::Result<Staged> {
    static N: AtomicU64 = AtomicU64::new(0);
    let target = resolved(path);
    let dir = target.parent().unwrap_or(Path::new("."));
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let old = std::fs::metadata(&target).ok();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    for _ in 0..16 {
        let n = N.fetch_add(1, Ordering::Relaxed);
        // Hidden, so file watchers and the tree ignore it.
        let tmp = dir.join(format!(
            ".{name}.{}-{stamp:x}-{n}.lemmata-tmp",
            std::process::id()
        ));
        let mut f = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let staged = Staged {
            tmp,
            target,
            bytes: bytes.to_vec(),
            done: false,
        };
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        // The file keeps its permissions (an edited script stays executable).
        if let Some(m) = old {
            let _ = std::fs::set_permissions(&staged.tmp, m.permissions());
        }
        return Ok(staged);
    }
    Err(std::io::Error::other(
        "no free name for a temporary file beside it",
    ))
}

impl Staged {
    /// The file the text goes to: the one a link at the given path names.
    pub fn target(&self) -> &Path {
        &self.target
    }

    /// Puts the text in place. Windows refuses to rename over a file another program holds open
    /// (TeX keeps its input open while it runs); then the file is overwritten in place.
    pub fn commit(mut self) -> std::io::Result<PathBuf> {
        let result = std::fs::rename(&self.tmp, &self.target).or_else(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => std::fs::write(&self.target, &self.bytes),
            _ => Err(e),
        });
        self.done = result.is_ok();
        let _ = std::fs::remove_file(&self.tmp);
        result.map(|()| std::mem::take(&mut self.target))
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        if !self.done {
            let _ = std::fs::remove_file(&self.tmp);
        }
    }
}

/// `stage` and `commit` in one go. Returns the file written, as `Staged::target`.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<PathBuf> {
    stage(path, bytes)?.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_names_inside_the_folder_pass() {
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a/../../x",
            "./a",
            "a\\..\\b",
            "C:x",
            "a\nb",
        ] {
            assert!(inside_rel(bad).is_none(), "{bad:?}");
        }
        for good in [
            "main.tex",
            "ch/Einführung.tex",
            "my chapter.tex",
            ".lemmata/x",
        ] {
            assert_eq!(inside_rel(good), Some(PathBuf::from(good)), "{good:?}");
        }
    }

    #[test]
    fn a_write_replaces_the_file_and_leaves_nothing_beside_it() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a.tex");
        std::fs::write(&f, "old").unwrap();
        write_atomic(&f, b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "new");
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
        // Dropped before commit: the file stays as it was.
        drop(stage(&f, b"never").unwrap());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "new");
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    }

    /// Windows refuses to rename over a file TeX holds open; a read-only folder refuses it here too,
    /// while the file itself stays writable.
    #[cfg(unix)]
    #[test]
    fn a_file_that_cant_be_replaced_is_written_in_place() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("main.tex");
        std::fs::write(&f, "old").unwrap();
        let staged = stage(&f, b"new").unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = staged.commit();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        result.unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "new");
    }

    /// A link to an endless device or a pipe is refused at once, and a file over the limit too.
    #[cfg(unix)]
    #[test]
    fn only_regular_files_up_to_the_limit_are_read() {
        let d = tempfile::tempdir().unwrap();
        let zero = d.path().join("zero.tex");
        std::os::unix::fs::symlink("/dev/zero", &zero).unwrap();
        assert!(read_to_string(&zero).is_err());
        assert!(read(&zero).is_err());
        let fifo = d.path().join("pipe.tex");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        assert!(read_to_string(&fifo).is_err(), "a pipe must not block");
        let f = d.path().join("a.tex");
        std::fs::write(&f, "0123456789").unwrap();
        assert_eq!(read_to_string(&f).unwrap(), "0123456789");
        assert!(read_limited(&f, 9).is_err());
        assert_eq!(read_limited(&f, 10).unwrap().len(), 10);
        assert!(copy(&zero, d.path().join("out")).is_err());
        assert!(copy(&fifo, d.path().join("out")).is_err());
        assert_eq!(copy(&f, d.path().join("out")).unwrap(), 10);
    }

    #[cfg(unix)]
    #[test]
    fn links_at_the_file_are_followed_and_planted_temp_links_are_not() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let shared = tempfile::tempdir().unwrap();
        let library = shared.path().join("library.bib");
        std::fs::write(&library, "old").unwrap();
        std::fs::set_permissions(&library, std::fs::Permissions::from_mode(0o640)).unwrap();
        let link = d.path().join("refs.bib");
        std::os::unix::fs::symlink(&library, &link).unwrap();
        // The temp names the writers before 3.2 used, planted by a project to point elsewhere.
        let victim = shared.path().join("victim");
        std::fs::write(&victim, "keep").unwrap();
        for name in [
            ".library.bib.0.lemmata-tmp",
            "library.bib.lemmata-tmp",
            ".library.bib.lemmata-tmp",
        ] {
            std::os::unix::fs::symlink(&victim, shared.path().join(name)).unwrap();
        }
        // A link out of the project is written through only when the project is trusted; one
        // that stays inside it always.
        assert!(may_write(d.path(), false, &link).is_err());
        assert!(may_write(d.path(), true, &link).is_ok());
        std::fs::write(d.path().join("real.bib"), "").unwrap();
        std::os::unix::fs::symlink("real.bib", d.path().join("inner.bib")).unwrap();
        assert!(may_write(d.path(), false, &d.path().join("inner.bib")).is_ok());
        assert!(may_write(d.path(), false, &d.path().join("new.bib")).is_ok());
        // The project reached through a link, as macOS's temp folder always is.
        let via = shared.path().join("via");
        std::os::unix::fs::symlink(d.path(), &via).unwrap();
        assert!(may_write(&via, false, &via.join("new.bib")).is_ok());
        assert!(may_write(&via, false, &via.join("inner.bib")).is_ok());
        assert!(may_write(&via, false, &via.join("refs.bib")).is_err());
        std::os::unix::fs::symlink(shared.path().join("nowhere"), d.path().join("dangling.png"))
            .unwrap();
        assert!(may_write(d.path(), false, &d.path().join("dangling.png")).is_err());
        let written = write_atomic(&link, b"new").unwrap();
        assert_eq!(written, library.canonicalize().unwrap());
        assert!(
            link.symlink_metadata().unwrap().file_type().is_symlink(),
            "the link stays"
        );
        assert_eq!(std::fs::read_to_string(&library).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
        let mode = std::fs::metadata(&library).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640, "keeps its permissions");
    }
}
