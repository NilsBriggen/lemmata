//! Opening a project or file handed to us by the OS: file association, "Open
//! with", a command-line argument, a second launch while running, or (macOS)
//! an Apple Event; and `lemmata://` links from a web page or another program.
//! All of them end up as a [`Request`] that the frontend picks up with
//! `take_open_request` (first load) or the `open-request` event.

use serde::Serialize;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use tauri::Manager;

const LEMMATA_TOML: &str = "lemmata.toml";
const FILE_EXTENSIONS: [&str; 4] = ["tex", "bib", "sty", "cls"];
/// Magic comments only count near the top of the file.
const MAGIC_LINES: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct OpenTarget {
    pub project_root: PathBuf,
    /// Path of the file to open, relative to `project_root` with `/` separators.
    pub file: Option<String>,
}

/// What the app is asked to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Request {
    Open(OpenTarget),
    /// `lemmata://clone?url=…`: the Clone dialog, filled in. Nothing is cloned until the user says so.
    Clone {
        url: String,
    },
    /// `lemmata://new?template=…`: the New project gallery with that template chosen.
    NewProject {
        template: String,
    },
}

const SCHEME: &str = "lemmata";

/// A `lemmata://open|clone|new?…` link. Links come from web pages, so every value is checked.
fn from_link(link: &str) -> Option<Request> {
    let url = tauri::Url::parse(link).ok()?;
    if url.scheme() != SCHEME {
        return None;
    }
    let param = |key: &str| {
        url.query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
    };
    match url.host_str()? {
        "open" => {
            let path = PathBuf::from(param("path")?);
            if !path.is_absolute() {
                return None;
            }
            target_from_path(&normalize(&path)).map(Request::Open)
        }
        "clone" => {
            let repo = param("url")?;
            let ok = repo.len() <= 500
                && tauri::Url::parse(&repo)
                    .is_ok_and(|u| u.scheme() == "https" && u.host_str().is_some());
            ok.then_some(Request::Clone { url: repo })
        }
        "new" => {
            let template = param("template")?;
            let ok = !template.is_empty()
                && template.len() <= 64
                && template
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            ok.then_some(Request::NewProject { template })
        }
        _ => None,
    }
}

/// Lexically resolves `.` and `..` (no symlink or existence handling, so no
/// Windows `\\?\` paths that the rest of the app never sees).
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

/// The main file named by a `% !TeX root = ../main.tex` comment, if it exists.
fn magic_root(file: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(file).ok()?;
    text.lines().take(MAGIC_LINES).find_map(|line| {
        let rest = line
            .trim_start()
            .strip_prefix('%')?
            .trim_start()
            .strip_prefix('!')?;
        let rest = rest.trim_start();
        // ASCII-only prefix, so slicing at its length is safe.
        let rest = rest
            .get(..8)
            .filter(|p| p.eq_ignore_ascii_case("tex root"))
            .map(|_| &rest[8..])?;
        let value = rest.trim_start().strip_prefix('=')?.trim();
        if value.is_empty() {
            return None;
        }
        Some(normalize(&file.parent()?.join(value)))
    })
}

/// Project root for a file: the nearest folder above it with a `lemmata.toml`;
/// else the folder of the main file a `% !TeX root` comment points to, if that
/// main lies in a parent folder; else the file's own folder.
/// shortcut: a file sitting in a folder under a stray lemmata.toml (say in the
/// home folder) opens that folder; fine until someone reports it.
fn project_root_of(file: &Path) -> PathBuf {
    let dir = file.parent().unwrap_or(file);
    if let Some(d) = dir.ancestors().find(|d| d.join(LEMMATA_TOML).is_file()) {
        return d.to_path_buf();
    }
    magic_root(file)
        .filter(|m| m.is_file())
        .and_then(|m| m.parent().map(Path::to_path_buf))
        .filter(|d| dir.starts_with(d))
        .unwrap_or_else(|| dir.to_path_buf())
}

fn target_from_path(path: &Path) -> Option<OpenTarget> {
    if path.is_dir() {
        return Some(OpenTarget {
            project_root: path.to_path_buf(),
            file: None,
        });
    }
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if !path.is_file() || !FILE_EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }
    let project_root = project_root_of(path);
    let rel = path.strip_prefix(&project_root).ok()?;
    let file = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    Some(OpenTarget {
        project_root,
        file: Some(file),
    })
}

/// The first of `args` that is a `lemmata://` link or names a project folder or a
/// .tex/.bib/.sty/.cls file. `args` excludes the program name; relative paths are
/// taken from `cwd`. Flags (`--`, `-psn_0_123`, ...) and anything that isn't there are skipped.
pub(crate) fn request_from_args(args: &[String], cwd: &Path) -> Option<Request> {
    args.iter()
        .filter(|a| !a.starts_with('-') && !a.is_empty())
        .find_map(|a| {
            if a.starts_with(&format!("{SCHEME}:")) {
                from_link(a)
            } else {
                target_from_path(&normalize(&cwd.join(a))).map(Request::Open)
            }
        })
}

/// What the frontend has not picked up yet.
#[derive(Default)]
pub(crate) struct OpenRequest {
    pending: Mutex<Option<Request>>,
    /// Set by the first `take_open_request`: from then on the page listens, so requests are emitted.
    page_ready: Mutex<bool>,
}

/// Call in `.setup()`: remembers the target from this launch's command line.
pub(crate) fn init(app: &tauri::AppHandle) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_default();
    app.manage(OpenRequest {
        pending: Mutex::new(request_from_args(&args, &cwd)),
        ..Default::default()
    });
}

/// Hands a target to the running app: emitted when the page listens, stored otherwise; the window is focused.
// Used by second launches (release builds) and macOS file events.
#[cfg(any(not(debug_assertions), target_os = "macos"))]
fn deliver(app: &tauri::AppHandle, target: Request) {
    let Some(state) = app.try_state::<OpenRequest>() else {
        return;
    };
    let ready = *state.page_ready.lock().unwrap_or_else(|e| e.into_inner());
    if ready {
        let _ = tauri::Emitter::emit(app, "open-request", &target);
    } else {
        *state.pending.lock().unwrap_or_else(|e| e.into_inner()) = Some(target);
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Call from the single-instance callback (`args` still has the program name first). Release builds only.
#[cfg(not(debug_assertions))]
pub(crate) fn on_second_launch(app: &tauri::AppHandle, args: &[String], cwd: &str) {
    if let Some(t) = request_from_args(args.get(1..).unwrap_or_default(), Path::new(cwd)) {
        deliver(app, t);
    }
}

/// Call from `RunEvent::Opened` (macOS Apple Events).
#[cfg(target_os = "macos")]
pub(crate) fn on_opened(app: &tauri::AppHandle, urls: &[tauri::Url]) {
    // file:// URLs for opened files, lemmata:// for links.
    let args: Vec<String> = urls
        .iter()
        .map(|u| match u.to_file_path() {
            Ok(p) if u.scheme() == "file" => p.to_string_lossy().into_owned(),
            _ => u.to_string(),
        })
        .collect();
    if let Some(t) = request_from_args(&args, Path::new("/")) {
        deliver(app, t);
    }
}

/// The page calls this once, after registering its `open-request` listener.
#[tauri::command]
pub(crate) fn take_open_request(state: tauri::State<OpenRequest>) -> Option<Request> {
    *state.page_ready.lock().unwrap_or_else(|e| e.into_inner()) = true;
    state
        .pending
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    fn at(root: &Path, file: Option<&str>) -> Option<Request> {
        Some(Request::Open(OpenTarget {
            project_root: root.to_path_buf(),
            file: file.map(String::from),
        }))
    }

    #[test]
    fn folder_is_a_project() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        assert_eq!(
            request_from_args(&args(&["--", "-psn_0_42", "proj"]), p),
            None
        );
        fs::create_dir(p.join("proj")).unwrap();
        assert_eq!(
            request_from_args(&args(&["--", "-psn_0_42", "proj"]), p),
            at(&p.join("proj"), None)
        );
        assert_eq!(
            request_from_args(&args(&["./proj/../proj/."]), p),
            at(&p.join("proj"), None)
        );
    }

    #[test]
    fn file_without_config_opens_its_folder() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        fs::create_dir(p.join("a")).unwrap();
        fs::write(p.join("a/main.TEX"), "\\documentclass{article}").unwrap();
        fs::write(p.join("a/notes.txt"), "x").unwrap();
        assert_eq!(
            request_from_args(&args(&["a/main.TEX"]), p),
            at(&p.join("a"), Some("main.TEX"))
        );
        assert_eq!(request_from_args(&args(&["a/notes.txt"]), p), None);
        // The first usable argument wins.
        assert_eq!(
            request_from_args(&args(&["a/notes.txt", "a/main.TEX"]), p),
            at(&p.join("a"), Some("main.TEX"))
        );
    }

    #[test]
    fn lemmata_toml_above_decides_the_root() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        fs::create_dir_all(p.join("thesis/chapters")).unwrap();
        fs::write(p.join("thesis/lemmata.toml"), "").unwrap();
        fs::write(p.join("thesis/chapters/one.tex"), "x").unwrap();
        let f = p
            .join("thesis/chapters/one.tex")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            request_from_args(&[f], Path::new("/nonexistent")),
            at(&p.join("thesis"), Some("chapters/one.tex"))
        );
    }

    #[test]
    fn tex_root_comment_points_to_a_parent_main() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        fs::create_dir_all(p.join("book/ch")).unwrap();
        fs::write(p.join("book/main.tex"), "\\documentclass{book}").unwrap();
        fs::write(p.join("book/ch/a.tex"), "% !TEX root = ../main.tex\ntext").unwrap();
        fs::write(p.join("book/ch/b.tex"), "% !TeX root = ../missing.tex\n").unwrap();
        fs::create_dir_all(p.join("other")).unwrap();
        fs::write(p.join("book/ch/c.tex"), "%!TeX root=../../other/x.tex\n").unwrap();
        fs::write(p.join("other/x.tex"), "").unwrap();
        assert_eq!(
            request_from_args(&args(&["book/ch/a.tex"]), p),
            at(&p.join("book"), Some("ch/a.tex"))
        );
        // Missing main, or one that is not in a parent folder: the file's own folder.
        assert_eq!(
            request_from_args(&args(&["book/ch/b.tex"]), p),
            at(&p.join("book/ch"), Some("b.tex"))
        );
        assert_eq!(
            request_from_args(&args(&["book/ch/c.tex"]), p),
            at(&p.join("book/ch"), Some("c.tex"))
        );
    }

    #[test]
    fn links_open_clone_and_start_projects_and_refuse_anything_else() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        fs::create_dir(p.join("proj")).unwrap();
        let open = format!("lemmata://open?path={}", p.join("proj").display());
        assert_eq!(
            request_from_args(&args(&[&open]), p),
            at(&p.join("proj"), None)
        );
        assert_eq!(
            request_from_args(
                &args(&["lemmata://clone?url=https%3A%2F%2Fgithub.com%2Fa%2Fb.git"]),
                p
            ),
            Some(Request::Clone {
                url: "https://github.com/a/b.git".into()
            })
        );
        assert_eq!(
            request_from_args(&args(&["lemmata://new?template=ieee"]), p),
            Some(Request::NewProject {
                template: "ieee".into()
            })
        );
        for bad in [
            "lemmata://open?path=proj",
            "lemmata://clone?url=file:///etc",
            "lemmata://clone?url=ext::sh%20-c%20x",
            "lemmata://new?template=../x",
            "lemmata://run?cmd=x",
            "other://new?template=ieee",
        ] {
            assert_eq!(request_from_args(&args(&[bad]), p), None, "{bad}");
        }
    }
}
