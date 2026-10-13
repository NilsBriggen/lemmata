//! Project file operations: search and replace, create, rename, move to
//! the trash, and a watcher for changes made outside Lemmata.

use crate::{AppState, CmdResult, err};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Emitter, State};

/// Files search and rename-reference updates look into.
const TEXT_EXTS: &[&str] = &[
    "tex", "bib", "sty", "cls", "bst", "txt", "md", "toml", "tikz", "bbx", "cbx",
];
const MAX_MATCHES: usize = 2000;

fn project(state: &State<AppState>) -> CmdResult<lemmata_project::Project> {
    state
        .project
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "No project is open.".into())
}

/// A project-relative path that can't escape the project.
fn safe_rel(rel: &str) -> CmdResult<PathBuf> {
    let p = PathBuf::from(rel);
    if rel.is_empty()
        || p.is_absolute()
        || p.components().any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!("{rel} isn't a path inside the project."));
    }
    Ok(p)
}

fn text_files(project: &lemmata_project::Project) -> Vec<String> {
    fn walk(entries: &[lemmata_project::Entry], out: &mut Vec<String>) {
        for e in entries {
            if e.is_dir {
                walk(&e.children, out);
            } else if Path::new(&e.name)
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| TEXT_EXTS.contains(&x))
            {
                out.push(e.path.clone());
            }
        }
    }
    let mut out = Vec::new();
    walk(
        &lemmata_project::tree(&project.root, &project.config.outdir),
        &mut out,
    );
    out
}

#[derive(Deserialize, Clone)]
pub struct SearchOptions {
    query: String,
    regex: bool,
    case_sensitive: bool,
    whole_word: bool,
}

fn build_regex(o: &SearchOptions) -> CmdResult<Regex> {
    let mut pattern = if o.regex {
        o.query.clone()
    } else {
        regex::escape(&o.query)
    };
    if o.whole_word {
        pattern = format!(r"\b(?:{pattern})\b");
    }
    RegexBuilder::new(&pattern)
        .case_insensitive(!o.case_sensitive)
        .build()
        .map_err(|e| format!("Invalid pattern: {e}"))
}

#[derive(Serialize)]
pub struct LineMatch {
    line: u32,
    text: String,
    /// Character (not byte) ranges within `text`.
    ranges: Vec<(usize, usize)>,
}

#[derive(Serialize)]
pub struct FileMatches {
    path: String,
    matches: Vec<LineMatch>,
}

pub fn search_in(text: &str, re: &Regex) -> Vec<LineMatch> {
    let char_at = |line: &str, byte: usize| line[..byte].chars().count();
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let ranges: Vec<_> = re
                .find_iter(line)
                .filter(|m| !m.is_empty())
                .map(|m| (char_at(line, m.start()), char_at(line, m.end())))
                .collect();
            (!ranges.is_empty()).then(|| LineMatch {
                line: i as u32 + 1,
                text: line.to_string(),
                ranges,
            })
        })
        .collect()
}

#[tauri::command]
pub fn search_project(
    state: State<AppState>,
    options: SearchOptions,
) -> CmdResult<Vec<FileMatches>> {
    if options.query.is_empty() {
        return Ok(Vec::new());
    }
    let p = project(&state)?;
    let re = build_regex(&options)?;
    let mut out = Vec::new();
    let mut total = 0;
    for rel in text_files(&p) {
        let Ok(text) = std::fs::read_to_string(p.root.join(&rel)) else {
            continue;
        };
        let matches = search_in(&text, &re);
        if matches.is_empty() {
            continue;
        }
        total += matches.len();
        out.push(FileMatches { path: rel, matches });
        if total >= MAX_MATCHES {
            break;
        }
    }
    Ok(out)
}

/// Replaces every match in the given files on disk. Files open with unsaved
/// edits are handled by the frontend instead.
#[tauri::command]
pub fn replace_in_files(
    state: State<AppState>,
    options: SearchOptions,
    replacement: String,
    paths: Vec<String>,
) -> CmdResult<usize> {
    let p = project(&state)?;
    let re = build_regex(&options)?;
    let mut count = 0;
    for rel in paths {
        let path = p.root.join(safe_rel(&rel)?);
        let text = std::fs::read_to_string(&path).map_err(err)?;
        count += re.find_iter(&text).count();
        let new = if options.regex {
            re.replace_all(&text, replacement.as_str())
        } else {
            re.replace_all(&text, regex::NoExpand(&replacement))
        };
        std::fs::write(&path, new.as_ref()).map_err(err)?;
    }
    Ok(count)
}

#[tauri::command]
pub fn create_path(state: State<AppState>, path: String, dir: bool) -> CmdResult<()> {
    let p = project(&state)?;
    let full = p.root.join(safe_rel(&path)?);
    if full.exists() {
        return Err(format!("{path} already exists."));
    }
    if dir {
        std::fs::create_dir_all(&full).map_err(err)
    } else {
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(err)?;
        }
        std::fs::write(&full, "").map_err(err)
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

/// Copies a file or folder inside the project.
#[tauri::command]
pub fn copy_path(state: State<AppState>, from: String, to: String) -> CmdResult<()> {
    let p = project(&state)?;
    let (src, dest) = (p.root.join(safe_rel(&from)?), p.root.join(safe_rel(&to)?));
    if dest.exists() {
        return Err(format!("{to} already exists."));
    }
    if src.is_dir() {
        if dest.starts_with(&src) {
            return Err("A folder can't be copied into itself.".into());
        }
        copy_dir(&src, &dest).map_err(err)
    } else {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(err)?;
        }
        std::fs::copy(&src, &dest).map(|_| ()).map_err(err)
    }
}

const MAX_BINARY: u64 = 64 * 1024 * 1024;

/// A project file's bytes, for the image and PDF viewers.
#[tauri::command]
pub fn read_binary(state: State<AppState>, path: String) -> CmdResult<tauri::ipc::Response> {
    let p = project(&state)?;
    let full = p.root.join(safe_rel(&path)?);
    let size = std::fs::metadata(&full)
        .map_err(|e| format!("Could not open {path}: {e}"))?
        .len();
    if size > MAX_BINARY {
        return Err(format!("{path} is too large to preview."));
    }
    std::fs::read(&full)
        .map(tauri::ipc::Response::new)
        .map_err(|e| format!("Could not open {path}: {e}"))
}

/// Moves a file or folder to the system trash, so it can be recovered.
#[tauri::command]
pub fn trash_path(state: State<AppState>, path: String) -> CmdResult<()> {
    let p = project(&state)?;
    let full = p.root.join(safe_rel(&path)?);
    trash::delete(&full).map_err(|e| format!("Could not move {path} to the trash: {e}"))
}

#[derive(Serialize)]
pub struct Reference {
    file: String,
    line: u32,
    text: String,
}

/// How `\input`, `\include`, `\includegraphics` etc. would name `rel`.
fn reference_names(rel: &str) -> Vec<String> {
    let mut names = vec![rel.to_string()];
    if let Some(stem) = rel.strip_suffix(".tex").or_else(|| {
        // \includegraphics often omits the extension too.
        Path::new(rel)
            .extension()
            .and_then(|e| e.to_str())
            .filter(|e| ["png", "jpg", "jpeg", "pdf", "eps", "svg"].contains(e))
            .map(|e| &rel[..rel.len() - e.len() - 1])
    }) {
        names.push(stem.to_string());
    }
    names
}

fn reference_regex(rel: &str) -> Regex {
    let alts: Vec<String> = reference_names(rel)
        .iter()
        .map(|n| regex::escape(n))
        .collect();
    Regex::new(&format!(
        r"(\\(?:input|include|subfile|includegraphics|addbibresource|bibliography|lstinputlisting)\*?(?:\[[^\]]*\])?\{{)(\./)?({})(\}})",
        alts.join("|")
    ))
    .unwrap()
}

/// Lines that refer to `path` by name, so a rename can offer to update them.
#[tauri::command]
pub fn find_references(state: State<AppState>, path: String) -> CmdResult<Vec<Reference>> {
    let p = project(&state)?;
    let re = reference_regex(&path);
    let mut out = Vec::new();
    for rel in text_files(&p) {
        let Ok(text) = std::fs::read_to_string(p.root.join(&rel)) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            if re.is_match(line) {
                out.push(Reference {
                    file: rel.clone(),
                    line: i as u32 + 1,
                    text: line.trim().to_string(),
                });
            }
        }
    }
    Ok(out)
}

pub fn rewrite_references(text: &str, from: &str, to: &str) -> String {
    let re = reference_regex(from);
    re.replace_all(text, |c: &regex::Captures| {
        // Keep the reader's style: with or without the extension.
        let new = if c[3] == *from {
            to.to_string()
        } else {
            strip_ext(to)
        };
        format!(
            "{}{}{}{}",
            &c[1],
            c.get(2).map_or("", |m| m.as_str()),
            new,
            &c[4]
        )
    })
    .into_owned()
}

fn strip_ext(p: &str) -> String {
    match Path::new(p).extension() {
        Some(e) => p[..p.len() - e.len() - 1].to_string(),
        None => p.to_string(),
    }
}

/// Renames a file or folder; with `update_references`, also rewrites
/// `\input{...}` and friends that name it.
#[tauri::command]
pub fn rename_path(
    state: State<AppState>,
    from: String,
    to: String,
    update_references: bool,
) -> CmdResult<usize> {
    let p = project(&state)?;
    let src = p.root.join(safe_rel(&from)?);
    let dst = p.root.join(safe_rel(&to)?);
    if dst.exists() {
        return Err(format!("{to} already exists."));
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(err)?;
    }
    std::fs::rename(&src, &dst).map_err(err)?;
    let mut updated = 0;
    if update_references {
        for rel in text_files(&p) {
            let path = p.root.join(&rel);
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let new = rewrite_references(&text, &from, &to);
            if new != text {
                std::fs::write(&path, new).map_err(err)?;
                updated += 1;
            }
        }
    }
    // Keep lemmata.toml's main file pointing at the right place.
    let mut guard = state.project.lock().unwrap();
    if let Some(proj) = guard.as_mut()
        && proj.main == Path::new(&from)
    {
        proj.main = PathBuf::from(&to);
        proj.config.main = Some(to.clone());
        proj.save_config().map_err(err)?;
    }
    Ok(updated)
}

/// Watches the project folder and emits `files-changed` with the changed
/// project-relative paths (build output and hidden folders ignored).
pub fn watch(
    app: AppHandle,
    root: PathBuf,
    outdir: String,
) -> CmdResult<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>> {
    use notify_debouncer_mini::{DebounceEventResult, new_debouncer};
    let watch_root = root.clone();
    let mut debouncer = new_debouncer(
        std::time::Duration::from_millis(400),
        move |res: DebounceEventResult| {
            let Ok(events) = res else { return };
            let mut paths: Vec<String> = events
                .iter()
                .filter_map(|e| e.path.strip_prefix(&root).ok())
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .filter(|p| {
                    !p.split('/').any(|seg| seg.starts_with('.')) && !p.starts_with(&outdir)
                })
                .collect();
            paths.sort();
            paths.dedup();
            if !paths.is_empty() {
                let _ = app.emit("files-changed", paths);
            }
        },
    )
    .map_err(err)?;
    debouncer
        .watcher()
        .watch(&watch_root, notify::RecursiveMode::Recursive)
        .map_err(err)?;
    Ok(debouncer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_reports_character_ranges() {
        let re = build_regex(&SearchOptions {
            query: "größe".into(),
            regex: false,
            case_sensitive: false,
            whole_word: false,
        })
        .unwrap();
        let m = search_in("Die Größe und GRÖSSE\nnichts", &re);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].ranges, vec![(4, 9)]);
    }

    #[test]
    fn whole_word_and_literal() {
        let re = build_regex(&SearchOptions {
            query: "a.b".into(),
            regex: false,
            case_sensitive: true,
            whole_word: true,
        })
        .unwrap();
        assert_eq!(search_in("a.b axb xa.b", &re)[0].ranges, vec![(0, 3)]);
    }

    #[test]
    fn rewrites_references_keeping_style() {
        let text = "\\input{chapters/intro}\n\\include{chapters/intro.tex}\n\\input{chapters/intro2}\n\\includegraphics[width=3cm]{./figures/a}";
        let out = rewrite_references(text, "chapters/intro.tex", "chapters/introduction.tex");
        assert_eq!(
            out.lines().next().unwrap(),
            "\\input{chapters/introduction}"
        );
        assert_eq!(
            out.lines().nth(1).unwrap(),
            "\\include{chapters/introduction.tex}"
        );
        assert_eq!(out.lines().nth(2).unwrap(), "\\input{chapters/intro2}");
        let fig = rewrite_references(text, "figures/a.png", "figures/b.png");
        assert!(
            fig.ends_with("\\includegraphics[width=3cm]{./figures/b}"),
            "{fig}"
        );
    }

    #[test]
    fn rejects_escaping_paths() {
        assert!(safe_rel("../x").is_err());
        assert!(safe_rel("/etc/passwd").is_err());
        assert!(safe_rel("a/../../b").is_err());
        assert!(safe_rel("chapters/a.tex").is_ok());
    }

    #[test]
    fn copies_folders_recursively() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("a/b")).unwrap();
        std::fs::write(d.path().join("a/b/x.tex"), "x").unwrap();
        copy_dir(&d.path().join("a"), &d.path().join("copy")).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.path().join("copy/b/x.tex")).unwrap(),
            "x"
        );
    }
}
