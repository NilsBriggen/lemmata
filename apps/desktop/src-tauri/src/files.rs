//! Project file operations: search and replace, create, rename, move to
//! the trash, and a watcher for changes made outside Lemmata.

use crate::{AppState, CmdResult, LockSafe, err, project};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Emitter, State};

/// Files search and rename-reference updates look into.
const TEXT_EXTS: &[&str] = &[
    "tex", "bib", "sty", "cls", "bst", "txt", "md", "toml", "tikz", "bbx", "cbx",
];
const MAX_MATCHES: usize = 2000;

/// A project-relative path that can't escape the project.
pub(crate) fn safe_rel(rel: &str) -> CmdResult<PathBuf> {
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
    /// The line as it would read after replacing, when a replacement was given.
    #[serde(skip_serializing_if = "Option::is_none")]
    replaced: Option<String>,
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
                replaced: None,
            })
        })
        .collect()
}

/// The text of one line after replacing every match in it.
fn replace_line(line: &str, re: &Regex, replacement: &str, regex_mode: bool) -> String {
    if regex_mode {
        re.replace_all(line, replacement).into_owned()
    } else {
        re.replace_all(line, regex::NoExpand(replacement))
            .into_owned()
    }
}

/// Replaces matches only on the listed (1-based) lines. Returns the new text
/// and how many matches changed. Line endings are kept as they were.
pub fn replace_on_lines(
    text: &str,
    re: &Regex,
    replacement: &str,
    regex_mode: bool,
    lines: &std::collections::HashSet<u32>,
) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut count = 0;
    for (i, raw) in text.split_inclusive('\n').enumerate() {
        if !lines.contains(&(i as u32 + 1)) {
            out.push_str(raw);
            continue;
        }
        let body = raw.strip_suffix('\n').unwrap_or(raw);
        let (body, eol) = match body.strip_suffix('\r') {
            Some(b) => (b, &raw[b.len()..]),
            None => (body, &raw[body.len()..]),
        };
        count += re.find_iter(body).filter(|m| !m.is_empty()).count();
        out.push_str(&replace_line(body, re, replacement, regex_mode));
        out.push_str(eol);
    }
    (out, count)
}

#[tauri::command(async)]
pub fn search_project(
    state: State<AppState>,
    options: SearchOptions,
    replacement: Option<String>,
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
        let mut matches = search_in(&text, &re);
        if let Some(r) = &replacement {
            for m in &mut matches {
                m.replaced = Some(replace_line(&m.text, &re, r, options.regex));
            }
        }
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

#[derive(Deserialize)]
pub struct ReplaceTarget {
    path: String,
    /// 1-based lines to replace on.
    lines: Vec<u32>,
}

/// Replaces matches on the chosen lines of the given files on disk. Files
/// open with unsaved edits are saved by the frontend first.
#[tauri::command(async)]
pub fn replace_in_files(
    state: State<AppState>,
    options: SearchOptions,
    replacement: String,
    targets: Vec<ReplaceTarget>,
) -> CmdResult<usize> {
    let p = project(&state)?;
    let re = build_regex(&options)?;
    let mut count = 0;
    for t in targets {
        let path = p.root.join(safe_rel(&t.path)?);
        let text = std::fs::read_to_string(&path).map_err(err)?;
        let lines = t.lines.into_iter().collect();
        let (new, n) = replace_on_lines(&text, &re, &replacement, options.regex, &lines);
        if n > 0 {
            std::fs::write(&path, new).map_err(err)?;
        }
        count += n;
    }
    Ok(count)
}

/// Where labels and citation keys are defined, for "Go to label or citation".
#[tauri::command(async)]
pub fn symbol_defs(state: State<AppState>) -> CmdResult<Vec<lemmata_project::symbols::Occurrence>> {
    let p = project(&state)?;
    Ok(lemmata_project::symbols::scan(&p)
        .into_iter()
        .filter(|o| o.role == lemmata_project::symbols::Role::Def)
        .collect())
}

/// Every mention of a label or key, to preview a rename or list its uses.
#[tauri::command(async)]
pub fn symbol_uses(
    state: State<AppState>,
    kind: lemmata_project::symbols::Kind,
    name: String,
) -> CmdResult<Vec<lemmata_project::symbols::Occurrence>> {
    let p = project(&state)?;
    Ok(lemmata_project::symbols::scan(&p)
        .into_iter()
        .filter(|o| o.kind == kind && o.name == name)
        .collect())
}

/// Renames a label or citation key in every .tex and .bib file. Returns the
/// files that changed. The caller saves open files first and reloads after.
#[tauri::command(async)]
pub fn rename_symbol(
    state: State<AppState>,
    kind: lemmata_project::symbols::Kind,
    old: String,
    new: String,
) -> CmdResult<Vec<String>> {
    use lemmata_project::symbols::{Role, rename_in_text, scan, valid_name};
    let p = project(&state)?;
    if !valid_name(&new) {
        return Err(
            "A name can't be empty or contain spaces, commas, braces or backslashes.".into(),
        );
    }
    let all = scan(&p);
    if new != old
        && all
            .iter()
            .any(|o| o.kind == kind && o.role == Role::Def && o.name == new)
    {
        return Err(format!("{new} is already defined."));
    }
    let mut files: Vec<String> = all
        .iter()
        .filter(|o| o.kind == kind && o.name == old)
        .map(|o| o.file.clone())
        .collect();
    files.sort();
    files.dedup();
    let mut changed = Vec::new();
    for f in files {
        let path = p.root.join(safe_rel(&f)?);
        let text = std::fs::read_to_string(&path).map_err(err)?;
        let (out, n) = rename_in_text(&f, &text, f.ends_with(".bib"), kind, &old, &new);
        if n > 0 {
            std::fs::write(&path, out).map_err(err)?;
            changed.push(f);
        }
    }
    Ok(changed)
}

#[tauri::command(async)]
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
#[tauri::command(async)]
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
#[tauri::command(async)]
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
#[tauri::command(async)]
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
#[tauri::command(async)]
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

/// Renames a file or folder inside the project and, with `update_references`,
/// rewrites the `\input{...}` lines that name it.
fn rename_one(
    state: &State<AppState>,
    p: &lemmata_project::Project,
    from: &str,
    to: &str,
    update_references: bool,
) -> CmdResult<usize> {
    let src = p.root.join(safe_rel(from)?);
    let dst = p.root.join(safe_rel(to)?);
    if dst.exists() {
        return Err(format!("{to} already exists."));
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(err)?;
    }
    std::fs::rename(&src, &dst).map_err(err)?;
    let mut updated = 0;
    if update_references {
        for rel in text_files(p) {
            let path = p.root.join(&rel);
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let new = rewrite_references(&text, from, to);
            if new != text {
                std::fs::write(&path, new).map_err(err)?;
                updated += 1;
            }
        }
    }
    // Keep lemmata.toml's main file pointing at the right place.
    let mut guard = state.project.lock_safe();
    if let Some(proj) = guard.as_mut()
        && proj.main == Path::new(from)
    {
        proj.main = PathBuf::from(to);
        proj.config.main = Some(to.to_string());
        proj.save_config().map_err(err)?;
    }
    Ok(updated)
}

/// Renames a file or folder; with `update_references`, also rewrites
/// `\input{...}` and friends that name it.
#[tauri::command(async)]
pub fn rename_path(
    state: State<AppState>,
    from: String,
    to: String,
    update_references: bool,
) -> CmdResult<usize> {
    let p = project(&state)?;
    rename_one(&state, &p, &from, &to, update_references)
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    pub from: String,
    pub to: String,
}

/// Drops entries that sit inside another entry of the list (moving a folder
/// already moves what is in it), and duplicates.
pub fn top_level(paths: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in paths {
        let inside = paths
            .iter()
            .any(|o| o != p && p.starts_with(&format!("{o}/")));
        if !inside && !out.contains(p) {
            out.push(p.clone());
        }
    }
    out
}

fn join_rel(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_string()
    } else {
        format!("{folder}/{name}")
    }
}

/// Moves files and folders into `folder` ("" is the project root), all or
/// nothing: every move is checked before the first one happens.
#[tauri::command(async)]
pub fn move_paths(
    state: State<AppState>,
    paths: Vec<String>,
    folder: String,
    update_references: bool,
) -> CmdResult<Vec<Moved>> {
    let p = project(&state)?;
    let plan = plan_moves(&p.root, &paths, &folder)?;
    for m in &plan {
        rename_one(&state, &p, &m.from, &m.to, update_references)?;
    }
    Ok(plan)
}

/// What moving `paths` into `folder` would do, or why it can't be done.
pub fn plan_moves(root: &Path, paths: &[String], folder: &str) -> CmdResult<Vec<Moved>> {
    if !folder.is_empty() {
        safe_rel(folder)?;
    }
    let mut plan: Vec<Moved> = Vec::new();
    for from in top_level(paths) {
        safe_rel(&from)?;
        let name = from.rsplit('/').next().unwrap_or(&from).to_string();
        let to = join_rel(folder, &name);
        if to == from {
            continue;
        }
        if folder == from || folder.starts_with(&format!("{from}/")) {
            return Err(format!("{from} can't be moved into itself."));
        }
        if root.join(&to).exists() || plan.iter().any(|m| m.to == to) {
            return Err(format!("{to} already exists."));
        }
        plan.push(Moved { from, to });
    }
    Ok(plan)
}

const TRASH_DIR: &str = ".lemmata/trash";
const MANIFEST: &str = "manifest.json";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TrashBatch {
    /// Milliseconds since the epoch; also the folder name.
    pub id: String,
    pub paths: Vec<String>,
}

/// Moves files and folders into `.lemmata/trash/<id>/`, keeping their
/// project-relative paths, so "Undo" can put them back exactly.
#[tauri::command(async)]
pub fn trash_paths(state: State<AppState>, paths: Vec<String>) -> CmdResult<TrashBatch> {
    trash_into(&project(&state)?.root, &paths)
}

pub fn trash_into(root: &Path, paths: &[String]) -> CmdResult<TrashBatch> {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
        .to_string();
    let batch = root.join(TRASH_DIR).join(&id);
    let paths = top_level(paths);
    for rel in &paths {
        safe_rel(rel)?;
        if !root.join(rel).exists() {
            return Err(format!("{rel} no longer exists."));
        }
    }
    for rel in &paths {
        let dest = batch.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(err)?;
        }
        std::fs::rename(root.join(rel), &dest)
            .map_err(|e| format!("Could not move {rel} to the trash: {e}"))?;
    }
    let result = TrashBatch { id, paths };
    std::fs::write(
        batch.join(MANIFEST),
        serde_json::to_vec(&result).map_err(err)?,
    )
    .map_err(err)?;
    Ok(result)
}

/// Puts a trashed batch back where it came from. Nothing is restored if any
/// of the original paths is taken again.
#[tauri::command(async)]
pub fn restore_trash(state: State<AppState>, id: String) -> CmdResult<Vec<String>> {
    restore_from(&project(&state)?.root, &id)
}

pub fn restore_from(root: &Path, id: &str) -> CmdResult<Vec<String>> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        return Err("Unknown trash entry.".into());
    }
    let batch = root.join(TRASH_DIR).join(id);
    let manifest: TrashBatch = serde_json::from_slice(
        &std::fs::read(batch.join(MANIFEST))
            .map_err(|_| "That deletion can't be undone anymore.")?,
    )
    .map_err(err)?;
    for rel in &manifest.paths {
        safe_rel(rel)?;
        if root.join(rel).exists() {
            return Err(format!("{rel} exists again, so it can't be restored."));
        }
    }
    for rel in &manifest.paths {
        let dest = root.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(err)?;
        }
        std::fs::rename(batch.join(rel), dest).map_err(err)?;
    }
    let _ = std::fs::remove_dir_all(&batch);
    Ok(manifest.paths)
}

/// Deletes trash batches older than `days`. Called when a project opens.
#[tauri::command(async)]
pub fn purge_trash(state: State<AppState>, days: u64) -> CmdResult<usize> {
    Ok(purge_in(&project(&state)?.root, days, now_ms()))
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

pub fn purge_in(root: &Path, days: u64, now: u128) -> usize {
    let limit = u128::from(days) * 24 * 60 * 60 * 1000;
    let mut removed = 0;
    let Ok(read) = std::fs::read_dir(root.join(TRASH_DIR)) else {
        return 0;
    };
    for e in read.flatten() {
        let old = e
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u128>().ok())
            .is_some_and(|id| now.saturating_sub(id) > limit);
        if old && std::fs::remove_dir_all(e.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Files nothing in the project refers to by name: images, `.tex` parts,
/// bibliographies and packages left behind. The main file never counts.
#[tauri::command(async)]
pub fn unused_files(state: State<AppState>) -> CmdResult<Vec<String>> {
    let p = project(&state)?;
    let sources: Vec<String> = text_files(&p)
        .iter()
        .filter_map(|rel| std::fs::read_to_string(p.root.join(rel)).ok())
        .collect();
    let main = p.main.to_string_lossy().replace('\\', "/");
    Ok(unused_in(&all_files(&p), &sources, &main))
}

const CANDIDATE_EXTS: &[&str] = &[
    "tex", "bib", "sty", "cls", "png", "jpg", "jpeg", "pdf", "eps", "svg", "tikz",
];

fn all_files(project: &lemmata_project::Project) -> Vec<String> {
    fn walk(entries: &[lemmata_project::Entry], out: &mut Vec<String>) {
        for e in entries {
            if e.is_dir {
                walk(&e.children, out);
            } else {
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

/// A file counts as used when a `{...}` argument in some source names it,
/// by path or by bare name (which also covers `\graphicspath`), with or
/// without the extension.
pub fn unused_in(files: &[String], sources: &[String], main: &str) -> Vec<String> {
    let base_of = |name: &str| -> String {
        let name = name.trim().trim_start_matches("./");
        let stem = strip_ext(name);
        stem.rsplit('/').next().unwrap_or(&stem).to_string()
    };
    let mut names = std::collections::HashSet::new();
    let re = Regex::new(r"\{([^{}]*)\}").unwrap();
    for src in sources {
        for c in re.captures_iter(src) {
            for part in c[1].split(',') {
                let part = part.trim();
                if !part.is_empty() {
                    names.insert(base_of(part));
                }
            }
        }
    }
    files
        .iter()
        .filter(|f| *f != main)
        .filter(|f| {
            Path::new(f)
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| CANDIDATE_EXTS.contains(&x.to_ascii_lowercase().as_str()))
        })
        .filter(|f| !names.contains(&base_of(f)))
        .cloned()
        .collect()
}

/// Hashes of the files the app itself last wrote, so the watcher can tell
/// its own saves from edits made elsewhere.
pub type Written = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<PathBuf, u64>>>;

pub fn hash_bytes(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// What the watcher reports for one batch of events.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// Project-relative paths that exist and differ from what the app wrote.
    pub changed: Vec<String>,
    /// Paths that no longer exist (a deleted folder is reported once, as itself).
    pub removed: Vec<String>,
}

/// Sorts raw event paths into `changed` and `removed`: hidden files and the
/// build folder are ignored, and so are files whose content is exactly what
/// the app last wrote.
pub fn classify(root: &Path, outdir: &str, written: &Written, paths: &[PathBuf]) -> Changes {
    let outdir = outdir.trim_start_matches("./").trim_end_matches('/');
    let mut out = Changes::default();
    for abs in paths {
        let Ok(rel) = abs.strip_prefix(root) else {
            continue;
        };
        let p = rel.to_string_lossy().replace('\\', "/");
        if p.is_empty()
            || p.split('/').any(|seg| seg.starts_with('.'))
            || (!outdir.is_empty() && (p == outdir || p.starts_with(&format!("{outdir}/"))))
        {
            continue;
        }
        if !abs.exists() {
            out.removed.push(p);
            continue;
        }
        if abs.is_file()
            && let Some(&known) = written.lock_safe().get(abs)
            && std::fs::read(abs).is_ok_and(|b| hash_bytes(&b) == known)
        {
            continue;
        }
        out.changed.push(p);
    }
    out.changed.sort();
    out.changed.dedup();
    out.removed.sort();
    out.removed.dedup();
    // Files inside a removed folder add nothing to "the folder is gone".
    let folders = out.removed.clone();
    out.removed
        .retain(|p| !folders.iter().any(|f| p.starts_with(&format!("{f}/"))));
    out
}

/// Watches the project folder and emits `files-changed` with the changes
/// (build output and hidden folders ignored).
pub fn watch(
    app: AppHandle,
    root: PathBuf,
    outdir: String,
    written: Written,
) -> CmdResult<notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>> {
    use notify_debouncer_mini::{DebounceEventResult, new_debouncer};
    let watch_root = root.clone();
    let mut debouncer = new_debouncer(
        std::time::Duration::from_millis(400),
        move |res: DebounceEventResult| {
            let Ok(events) = res else { return };
            let paths: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
            let changes = classify(&root, &outdir, &written, &paths);
            if !changes.changed.is_empty() || !changes.removed.is_empty() {
                let _ = app.emit("files-changed", changes);
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

    fn touch(root: &Path, rel: &str, text: &str) {
        let full = root.join(rel);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }

    #[test]
    fn replaces_only_the_chosen_lines_and_keeps_line_endings() {
        let re = build_regex(&SearchOptions {
            query: "cat".into(),
            regex: false,
            case_sensitive: true,
            whole_word: false,
        })
        .unwrap();
        let text = "cat one\r\ncat two\ncat cat three";
        let lines = [1, 3].into_iter().collect();
        let (new, n) = replace_on_lines(text, &re, "dog", false, &lines);
        assert_eq!(new, "dog one\r\ncat two\ndog dog three");
        assert_eq!(n, 3);
    }

    #[test]
    fn regex_replacements_use_groups_and_literal_ones_do_not() {
        let mk = |regex: bool, q: &str| {
            build_regex(&SearchOptions {
                query: q.into(),
                regex,
                case_sensitive: true,
                whole_word: false,
            })
            .unwrap()
        };
        assert_eq!(
            replace_line("ab12", &mk(true, r"([a-z]+)(\d+)"), "$2$1", true),
            "12ab"
        );
        assert_eq!(replace_line("a.b", &mk(false, "."), "$1", false), "a$1b");
    }

    #[test]
    fn top_level_drops_children_and_duplicates() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            top_level(&v(&["a", "a/b.tex", "a", "ab.tex", "c/d"])),
            v(&["a", "ab.tex", "c/d"])
        );
    }

    #[test]
    fn plans_moves_and_refuses_clashes_and_cycles() {
        let d = tempfile::tempdir().unwrap();
        touch(d.path(), "a.tex", "");
        touch(d.path(), "chapters/a.tex", "");
        touch(d.path(), "chapters/b.tex", "");
        touch(d.path(), "figs/x.png", "");
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let plan = plan_moves(d.path(), &v(&["chapters/b.tex", "figs"]), "").unwrap();
        assert_eq!(
            plan,
            [
                Moved {
                    from: "chapters/b.tex".into(),
                    to: "b.tex".into()
                },
                // figs already sits in the root: nothing to do for it.
            ]
        );
        assert!(
            plan_moves(d.path(), &v(&["a.tex"]), "chapters").is_err(),
            "name taken"
        );
        assert!(
            plan_moves(d.path(), &v(&["chapters"]), "chapters/sub").is_err(),
            "into itself"
        );
        assert!(
            plan_moves(d.path(), &v(&["../x"]), "").is_err(),
            "escapes the project"
        );
    }

    #[test]
    fn trash_and_restore_round_trip() {
        let d = tempfile::tempdir().unwrap();
        touch(d.path(), "chapters/one.tex", "one");
        touch(d.path(), "chapters/two.tex", "two");
        touch(d.path(), "refs.bib", "bib");
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let batch =
            trash_into(d.path(), &v(&["chapters", "chapters/one.tex", "refs.bib"])).unwrap();
        assert_eq!(batch.paths, v(&["chapters", "refs.bib"]));
        assert!(!d.path().join("chapters").exists());
        assert!(!d.path().join("refs.bib").exists());
        let back = restore_from(d.path(), &batch.id).unwrap();
        assert_eq!(back, v(&["chapters", "refs.bib"]));
        assert_eq!(
            std::fs::read_to_string(d.path().join("chapters/two.tex")).unwrap(),
            "two"
        );
        assert!(!d.path().join(TRASH_DIR).join(&batch.id).exists());
    }

    #[test]
    fn restore_refuses_when_the_path_is_taken_again() {
        let d = tempfile::tempdir().unwrap();
        touch(d.path(), "a.tex", "old");
        let batch = trash_into(d.path(), &["a.tex".to_string()]).unwrap();
        touch(d.path(), "a.tex", "new");
        assert!(restore_from(d.path(), &batch.id).is_err());
        assert_eq!(
            std::fs::read_to_string(d.path().join("a.tex")).unwrap(),
            "new"
        );
        assert!(restore_from(d.path(), "../etc").is_err());
    }

    #[test]
    fn purge_removes_only_old_batches() {
        let d = tempfile::tempdir().unwrap();
        let day = 24 * 60 * 60 * 1000u128;
        let now = 100 * day;
        for id in [now - 8 * day, now - day] {
            touch(d.path(), &format!("{TRASH_DIR}/{id}/x.tex"), "");
        }
        assert_eq!(purge_in(d.path(), 7, now), 1);
        assert!(
            d.path()
                .join(TRASH_DIR)
                .join((now - day).to_string())
                .exists()
        );
    }

    #[test]
    fn unused_files_ignores_referenced_ones() {
        let files: Vec<String> = [
            "main.tex",
            "chapters/a.tex",
            "chapters/b.tex",
            "figs/x.png",
            "figs/old.png",
            "refs.bib",
            "notes.txt",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let sources =
            vec!["\\input{chapters/a}\\includegraphics{x}\\addbibresource{refs.bib}".to_string()];
        assert_eq!(
            unused_in(&files, &sources, "main.tex"),
            ["chapters/b.tex", "figs/old.png"]
        );
    }

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

    fn written() -> Written {
        Written::default()
    }

    #[test]
    fn ignores_our_own_writes_but_not_other_edits() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        let f = root.join("main.tex");
        std::fs::write(&f, "ours").unwrap();
        let w = written();
        w.lock_safe().insert(f.clone(), hash_bytes(b"ours"));
        assert_eq!(
            classify(&root, ".lemmata/build", &w, std::slice::from_ref(&f)),
            Changes::default()
        );
        // Someone else edits it.
        std::fs::write(&f, "theirs").unwrap();
        let c = classify(&root, ".lemmata/build", &w, &[f]);
        assert_eq!(c.changed, ["main.tex"]);
        assert!(c.removed.is_empty());
    }

    #[test]
    fn reports_a_deleted_folder_once() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        let gone = [root.join("chapters"), root.join("chapters/a.tex")];
        let c = classify(&root, ".lemmata/build", &written(), &gone);
        assert_eq!(c.removed, ["chapters"]);
        assert!(c.changed.is_empty());
    }

    #[test]
    fn skips_hidden_temp_files_and_the_build_folder_by_whole_segment() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        for rel in [
            ".main.tex.lemmata-tmp",
            ".git/HEAD",
            "build/main.pdf",
            "build2/x.tex",
        ] {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, "x").unwrap();
        }
        let all: Vec<PathBuf> = [
            ".main.tex.lemmata-tmp",
            ".git/HEAD",
            "build/main.pdf",
            "build2/x.tex",
        ]
        .iter()
        .map(|r| root.join(r))
        .collect();
        let c = classify(&root, "build", &written(), &all);
        // "build2" is not the build folder, even though its name starts with it.
        assert_eq!(c.changed, ["build2/x.tex"]);
        // An empty output folder name must not hide everything.
        assert_eq!(
            classify(&root, "", &written(), &all[3..]).changed,
            ["build2/x.tex"]
        );
    }
}
