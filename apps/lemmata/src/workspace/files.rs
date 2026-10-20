//! Everything the file tree does to files (FileActions.svelte, treeSelection.svelte.ts and the
//! tree keys of Sidebar.svelte): the selection, the right-click menu, the dialogs behind it (new,
//! rename, move, delete, unused files, recently deleted), clipboard operations and Undo.

use super::{Menu, Notice, NoticeAction, Tone, Workspace};
use crate::backend::{Backend, Msg};
use crate::icons;
use crate::store::Settings;
use crate::theme::Palette;
use crate::widgets::{self, Button, Kind, MenuItem, MenuResult, Size};
use eframe::egui::{self, FontId, Key, Margin, Modifiers, Rect, RichText, Stroke, Ui, vec2};
use lemmata_project::Entry;
use lemmata_shell::files::{Reference, TrashBatch};

/// treeModel.ts helpers.
pub fn parent_of(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

pub fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub fn join_path(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}

/// Drops paths inside another listed path (acting on a folder covers its contents) and repeats.
pub fn top_level(paths: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in paths {
        if !out.contains(p)
            && !paths
                .iter()
                .any(|o| o != p && p.starts_with(&format!("{o}/")))
        {
            out.push(p.clone());
        }
    }
    out
}

/// Can all of `paths` move into `folder`? Not into themselves, their own children, or where they
/// already are.
pub fn can_move(paths: &[String], folder: &str) -> bool {
    let movable: Vec<String> = top_level(paths)
        .into_iter()
        .filter(|p| parent_of(p) != folder)
        .collect();
    !movable.is_empty()
        && movable
            .iter()
            .all(|p| folder != p && !folder.starts_with(&format!("{p}/")))
}

/// Files being dragged in the tree: their paths, what the ghost says, and the folder under the
/// pointer that would take them ("" is the project root).
pub struct TreeDrag {
    pub paths: Vec<String>,
    pub name: String,
    pub target: Option<String>,
}

/// "main copy.tex", "main copy 2.tex", … the first name not taken.
pub fn copy_name(name: &str, is_dir: bool, taken: &[String]) -> String {
    let (stem, tail) = match name.rfind('.') {
        Some(i) if !is_dir && i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (1..1000)
        .map(|i| {
            format!(
                "{stem} copy{}{tail}",
                if i > 1 {
                    format!(" {i}")
                } else {
                    String::new()
                }
            )
        })
        .find(|c| !taken.contains(c))
        .unwrap_or_else(|| format!("{stem} copy {}{tail}", crate::when::now_ms()))
}

/// Names directly inside `folder` ("" is the project root).
fn names_in(entries: &[Entry], folder: &str) -> Vec<String> {
    if folder.is_empty() {
        return entries.iter().map(|e| e.name.clone()).collect();
    }
    crate::workspace::tree::find(entries, folder)
        .map(|e| e.children.iter().map(|c| c.name.clone()).collect())
        .unwrap_or_default()
}

/// Paths from `a` to `b` inclusive, in row order.
fn range(rows: &[String], a: &str, b: &str) -> Vec<String> {
    match (
        rows.iter().position(|r| r == a),
        rows.iter().position(|r| r == b),
    ) {
        (Some(i), Some(j)) => rows[i.min(j)..=i.max(j)].to_vec(),
        _ => vec![b.to_owned()],
    }
}

/// Remaps a path after `from` was renamed or moved to `to`.
pub fn remap(p: &str, from: &str, to: &str) -> String {
    if p == from {
        to.to_owned()
    } else if let Some(rest) = p.strip_prefix(&format!("{from}/")) {
        format!("{to}/{rest}")
    } else {
        p.to_owned()
    }
}

#[derive(Default)]
pub struct TreeSel {
    pub selected: Vec<String>,
    /// Where Shift extends a range from.
    pub anchor: Option<String>,
    /// Copied (false) or cut (true) entries waiting for Paste.
    pub clipboard: Option<(bool, Vec<String>)>,
    /// The tree has the keyboard: a click on it, until something else takes the focus.
    pub focused: bool,
    /// Letters typed to jump to a row, and when the last one came.
    typed: String,
    typed_at: Option<std::time::Instant>,
}

impl TreeSel {
    pub fn is_selected(&self, path: &str) -> bool {
        self.selected.iter().any(|s| s == path)
    }
}

/// How to put a file operation back.
enum Undo {
    /// Rename these back (to, from), updating references or not.
    Rename(Vec<(String, String)>, bool),
    /// Move what was made to the trash.
    Trash(Vec<String>),
    /// Restore a trash batch.
    Restore(String),
}

pub enum FileDialog {
    New {
        dir: bool,
        folder: String,
        name: String,
    },
    Rename {
        path: String,
        name: String,
        refs: Vec<Reference>,
        update: bool,
    },
    Trash {
        paths: Vec<String>,
        refs: Vec<Reference>,
    },
    Move {
        paths: Vec<String>,
        folder: String,
        refs: Vec<Reference>,
        update: bool,
    },
    Unused {
        files: Option<Vec<String>>,
        picked: Vec<String>,
    },
    Deleted(Option<Vec<TrashBatch>>),
}

#[derive(Default)]
pub struct FileActions {
    pub dialog: Option<FileDialog>,
    pub error: Option<String>,
    focus: bool,
    log: Vec<(String, Undo)>,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn describe(paths: &[String]) -> String {
    if paths.len() == 1 {
        base_name(&paths[0]).to_owned()
    } else {
        plural(paths.len(), "item", "items")
    }
}

impl Workspace {
    /// The rows of the tree as drawn, top to bottom.
    pub(super) fn tree_paths(&self, settings: &Settings) -> Vec<String> {
        let sorted = crate::workspace::tree::sort_entries(
            &self.info.tree,
            &settings.tree_sort,
            settings.tree_folders_first,
        );
        crate::workspace::tree::flatten(&sorted, &self.collapsed)
            .into_iter()
            .map(|(e, _)| e.path.clone())
            .collect()
    }

    /// Click semantics: plain selects one, Ctrl toggles, Shift extends from the anchor.
    pub(super) fn tree_click(&mut self, path: &str, rows: &[String], mods: Modifiers) {
        self.tree_cursor = Some(path.to_owned());
        self.sel.focused = true;
        let s = &mut self.sel;
        if mods.shift
            && let Some(a) = &s.anchor
        {
            s.selected = range(rows, a, path);
        } else if mods.command {
            if s.is_selected(path) {
                s.selected.retain(|p| p != path);
            } else {
                s.selected.push(path.to_owned());
            }
            s.anchor = Some(path.to_owned());
        } else {
            s.selected = vec![path.to_owned()];
            s.anchor = Some(path.to_owned());
        }
    }

    fn tree_select(&mut self, path: &str) {
        self.sel.selected = vec![path.to_owned()];
        self.sel.anchor = Some(path.to_owned());
        self.tree_cursor = Some(path.to_owned());
    }

    /// What an action applies to: the selection, or the cursor row, or the given row.
    pub(super) fn targets(&self, fallback: Option<&str>) -> Vec<String> {
        if let Some(f) = fallback
            && !self.sel.is_selected(f)
        {
            return vec![f.to_owned()];
        }
        let picked = if !self.sel.selected.is_empty() {
            self.sel.selected.clone()
        } else if let Some(c) = self
            .tree_cursor
            .clone()
            .or_else(|| fallback.map(str::to_owned))
        {
            vec![c]
        } else {
            Vec::new()
        };
        top_level(&picked)
    }

    fn folder_of(&self, path: Option<&str>) -> String {
        match path {
            None => String::new(),
            Some(p)
                if crate::workspace::tree::find(&self.info.tree, p).is_some_and(|e| e.is_dir) =>
            {
                p.to_owned()
            }
            Some(p) => parent_of(p).to_owned(),
        }
    }

    fn refresh_tree(&self, backend: &Backend) {
        backend.spawn(|shell| Msg::Info(shell.project_info_cmd()));
        self.refresh_outline(backend);
    }

    /// A file or folder was renamed or moved: its tabs, editors and marks follow (onRenamed).
    fn renamed(&mut self, from: &str, to: &str) {
        let moved = |p: &str| remap(p, from, to);
        for map_keys in [self.editors.keys().cloned().collect::<Vec<_>>()] {
            for k in map_keys {
                let n = moved(&k);
                if n != k
                    && let Some(mut ed) = self.editors.remove(&k)
                {
                    ed.path = n.clone();
                    self.buffers.rename(&k, &n);
                    self.editors.insert(n, ed);
                }
            }
        }
        for k in self.viewers.keys().cloned().collect::<Vec<_>>() {
            let n = moved(&k);
            if n != k
                && let Some(v) = self.viewers.remove(&k)
            {
                self.viewers.insert(n, v);
            }
        }
        for list in [
            &mut self.tabs,
            &mut self.pinned,
            &mut self.closed,
            &mut self.sel.selected,
        ] {
            for p in list.iter_mut() {
                *p = moved(p);
            }
        }
        for p in [
            &mut self.active,
            &mut self.tree_cursor,
            &mut self.sel.anchor,
        ]
        .into_iter()
        .flatten()
        {
            *p = moved(p);
        }
    }

    /// Gone from disk: tabs close without saving, which would bring the file back.
    fn trashed(&mut self, backend: &Backend, paths: &[String]) {
        self.close_gone(backend, paths);
        self.sel.selected.retain(|p| {
            !paths
                .iter()
                .any(|t| p == t || p.starts_with(&format!("{t}/")))
        });
    }

    fn file_failure(&mut self, e: String) {
        self.notice(e, Tone::Error);
    }

    fn push_undo(&mut self, label: String, undo: Undo) {
        self.files.log.push((label, undo));
        if self.files.log.len() > 20 {
            self.files.log.remove(0);
        }
    }

    fn undo_notice(&mut self, text: String) {
        self.notices
            .push(Notice::new(text, Tone::Ok).action(NoticeAction::UndoFiles));
    }

    pub fn new_item(&mut self, dir: bool, folder: String) {
        self.files.error = None;
        self.files.focus = true;
        self.files.dialog = Some(FileDialog::New {
            dir,
            folder,
            name: String::new(),
        });
    }

    fn start_rename(&mut self, backend: &Backend, path: String) {
        self.files.error = None;
        let p = path.clone();
        Workspace::job(
            backend,
            move |s| s.find_references(p).unwrap_or_default(),
            move |ws, _, refs| {
                let update = !refs.is_empty();
                ws.files.focus = true;
                ws.files.dialog = Some(FileDialog::Rename {
                    name: base_name(&path).to_owned(),
                    path,
                    refs,
                    update,
                });
            },
        );
    }

    /// References to `paths` from files that stay where they are.
    fn refs_of(shell: &lemmata_shell::Shell, paths: &[String]) -> Vec<Reference> {
        paths
            .iter()
            .flat_map(|p| shell.find_references(p.clone()).unwrap_or_default())
            .filter(|r| {
                !paths
                    .iter()
                    .any(|p| r.file == *p || r.file.starts_with(&format!("{p}/")))
            })
            .collect()
    }

    /// Moves entries into a folder; asks first when other files point at them.
    pub fn move_paths(
        &mut self,
        backend: &Backend,
        settings: &Settings,
        paths: Vec<String>,
        folder: String,
    ) {
        self.files.error = None;
        let list: Vec<String> = top_level(&paths)
            .into_iter()
            .filter(|p| parent_of(p) != folder)
            .collect();
        if list.is_empty() {
            return;
        }
        if let Some(bad) = list
            .iter()
            .find(|p| folder == **p || folder.starts_with(&format!("{p}/")))
        {
            self.file_failure(format!("{} can't be moved into itself.", base_name(bad)));
            return;
        }
        self.save_all_quietly(backend, settings);
        Workspace::job(
            backend,
            move |s| {
                let refs = Workspace::refs_of(s, &list);
                (list, refs)
            },
            move |ws, _, (list, refs)| {
                if refs.is_empty() {
                    ws.run_move(list, folder, false);
                } else {
                    ws.files.dialog = Some(FileDialog::Move {
                        paths: list,
                        folder,
                        refs,
                        update: true,
                    });
                }
            },
        );
    }

    fn run_move(&mut self, paths: Vec<String>, folder: String, update: bool) {
        self.after_writes.push(Box::new(move |_, b| {
            let f = folder.clone();
            Workspace::job(
                b,
                move |s| s.move_paths(paths, f, update),
                move |ws, b, r| match r {
                    Ok(moved) if moved.is_empty() => {}
                    Ok(moved) => {
                        for m in &moved {
                            ws.renamed(&m.from, &m.to);
                        }
                        let from: Vec<String> = moved.iter().map(|m| m.from.clone()).collect();
                        let back = moved
                            .iter()
                            .rev()
                            .map(|m| (m.to.clone(), m.from.clone()))
                            .collect();
                        ws.push_undo(
                            format!("Move {}", describe(&from)),
                            Undo::Rename(back, update),
                        );
                        let to = if folder.is_empty() {
                            " to the project folder".to_owned()
                        } else {
                            format!(" into {}", base_name(&folder))
                        };
                        ws.undo_notice(format!("Moved {}{to}.", describe(&from)));
                        ws.refresh_tree(b);
                    }
                    Err(e) => ws.file_failure(e),
                },
            );
        }));
    }

    /// Deletes entries. The project's own trash can be undone; the system trash cannot.
    pub fn trash(&mut self, backend: &Backend, settings: &Settings, paths: Vec<String>) {
        self.files.error = None;
        let list = top_level(&paths);
        if list.is_empty() {
            return;
        }
        let system = settings.trash_mode == "system";
        Workspace::job(
            backend,
            move |s| {
                let refs = Workspace::refs_of(s, &list);
                (list, refs)
            },
            move |ws, b, (list, refs)| {
                if !refs.is_empty() || system {
                    ws.files.dialog = Some(FileDialog::Trash { paths: list, refs });
                } else {
                    ws.run_trash(b, list, false);
                }
            },
        );
    }

    fn run_trash(&mut self, backend: &Backend, paths: Vec<String>, system: bool) {
        // Saving afterwards would bring the files back: drop their edits first.
        self.trashed(backend, &paths);
        Workspace::job(
            backend,
            move |s| {
                if system {
                    paths
                        .iter()
                        .try_for_each(|p| s.trash_path(p.clone()))
                        .map(|_| None)
                        .map_err(|e| (e, paths.clone()))
                } else {
                    s.trash_paths(paths.clone())
                        .map(Some)
                        .map_err(|e| (e, paths.clone()))
                }
                .map(|b| (b, paths))
            },
            move |ws, b, r| {
                match r {
                    Ok((None, paths)) => ws.notice(
                        format!("Moved {} to the system trash.", describe(&paths)),
                        Tone::Ok,
                    ),
                    Ok((Some(batch), paths)) => {
                        ws.push_undo(
                            format!("Delete {}", describe(&paths)),
                            Undo::Restore(batch.id),
                        );
                        ws.undo_notice(format!("Deleted {}.", describe(&paths)));
                    }
                    Err((e, _)) => ws.file_failure(e),
                }
                ws.refresh_tree(b);
            },
        );
    }

    /// Copies next to the originals, named "… copy".
    pub fn duplicate(&mut self, backend: &Backend, settings: &Settings, paths: Vec<String>) {
        let list = top_level(&paths);
        let mut made: Vec<(String, String)> = Vec::new();
        for p in &list {
            let Some(e) = crate::workspace::tree::find(&self.info.tree, p) else {
                continue;
            };
            let dir = parent_of(p);
            let mut taken = names_in(&self.info.tree, dir);
            taken.extend(
                made.iter()
                    .filter(|(_, m)| parent_of(m) == dir)
                    .map(|(_, m)| base_name(m).to_owned()),
            );
            made.push((
                p.clone(),
                join_path(dir, &copy_name(&e.name, e.is_dir, &taken)),
            ));
        }
        self.copy_into(backend, settings, made, None);
    }

    /// Copies `(from, to)` pairs; the notice says `pasted` when it was a paste.
    fn copy_into(
        &mut self,
        backend: &Backend,
        settings: &Settings,
        pairs: Vec<(String, String)>,
        pasted: Option<Vec<String>>,
    ) {
        if pairs.is_empty() {
            return;
        }
        self.save_all_quietly(backend, settings);
        self.after_writes.push(Box::new(move |_, b| {
            Workspace::job(
                b,
                move |s| {
                    let mut made = Vec::new();
                    let mut err = None;
                    for (from, to) in pairs {
                        match s.copy_path(from, to.clone()) {
                            Ok(()) => made.push(to),
                            Err(e) => {
                                err = Some(e);
                                break;
                            }
                        }
                    }
                    (made, err)
                },
                move |ws, b, (made, err)| {
                    if let Some(e) = err {
                        ws.file_failure(e);
                    }
                    if made.is_empty() {
                        return;
                    }
                    ws.refresh_tree(b);
                    match pasted {
                        Some(src) => {
                            ws.push_undo(
                                format!("Paste {}", describe(&src)),
                                Undo::Trash(made.clone()),
                            );
                            ws.undo_notice(format!("Pasted {}.", describe(&made)));
                        }
                        None => {
                            ws.push_undo(
                                format!("Duplicate {}", describe(&made)),
                                Undo::Trash(made.clone()),
                            );
                            ws.tree_select(&made[0]);
                        }
                    }
                },
            );
        }));
    }

    /// Ctrl+C and Ctrl+X in the tree. The paths also go on the system clipboard, so Ctrl+V,
    /// which only arrives with something on it, works.
    fn clip(&mut self, ctx: &egui::Context, cut: bool, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        let list = top_level(&paths);
        ctx.copy_text(list.join("\n"));
        let verb = if cut { "Cut" } else { "Copied" };
        self.notice(
            format!("{verb} {}. Paste it into a folder.", describe(&list)),
            Tone::Info,
        );
        self.sel.clipboard = Some((cut, list));
    }

    fn paste(&mut self, backend: &Backend, settings: &Settings, folder: String) {
        let Some((cut, paths)) = self.sel.clipboard.clone() else {
            return;
        };
        if cut {
            self.sel.clipboard = None;
            self.move_paths(backend, settings, paths, folder);
            return;
        }
        let mut taken = names_in(&self.info.tree, &folder);
        let mut pairs = Vec::new();
        for p in &paths {
            let Some(e) = crate::workspace::tree::find(&self.info.tree, p) else {
                continue;
            };
            if folder == *p || folder.starts_with(&format!("{p}/")) {
                self.file_failure(format!("{} can't be copied into itself.", e.name));
                return;
            }
            let name = if taken.contains(&e.name) {
                copy_name(&e.name, e.is_dir, &taken)
            } else {
                e.name.clone()
            };
            taken.push(name.clone());
            pairs.push((p.clone(), join_path(&folder, &name)));
        }
        self.copy_into(backend, settings, pairs, Some(paths));
    }

    /// Undoes the last file operation made in the tree.
    pub fn undo_files(&mut self, backend: &Backend) {
        let Some((label, step)) = self.files.log.pop() else {
            self.notice("Nothing to undo in the file tree.", Tone::Info);
            return;
        };
        Workspace::job(
            backend,
            move |s| match step {
                Undo::Rename(pairs, update) => {
                    let mut done = Vec::new();
                    for (from, to) in pairs {
                        s.rename_path(from.clone(), to.clone(), update)?;
                        done.push((from, to));
                    }
                    Ok(done)
                }
                Undo::Trash(paths) => s.trash_paths(paths).map(|_| Vec::new()),
                Undo::Restore(id) => s.restore_trash(id).map(|_| Vec::new()),
            },
            move |ws, b, r: Result<Vec<(String, String)>, String>| {
                match r {
                    Ok(done) => {
                        for (from, to) in done {
                            ws.renamed(&from, &to);
                        }
                        ws.notice(format!("Undid: {}.", label.to_lowercase()), Tone::Ok);
                    }
                    Err(e) => ws.file_failure(e),
                }
                ws.refresh_tree(b);
            },
        );
    }

    fn set_main(&mut self, backend: &Backend, settings: &Settings, path: String) {
        let p = path.clone();
        self.set_config(backend, settings, move |c| c.main = Some(p));
        self.open_file(backend, &path);
    }

    /// Files dropped from the computer onto the tree: copied into `folder`.
    pub fn add_dropped(&mut self, backend: &Backend, paths: Vec<String>, folder: String) {
        Workspace::job(
            backend,
            move |s| s.add_files(paths, folder),
            |ws, b, r| match r {
                Ok(added) => {
                    ws.refresh_tree(b);
                    ws.notice(
                        format!("Added {}.", plural(added.len(), "file", "files")),
                        Tone::Ok,
                    );
                }
                Err(e) => ws.file_failure(e),
            },
        );
    }

    /// "Add files…": files from the computer, copied into `folder`.
    pub fn upload(&mut self, backend: &Backend, folder: String) {
        backend.spawn_async(|shell| async move {
            let picked: Vec<String> = rfd::AsyncFileDialog::new()
                .set_title("Add files to the project")
                .pick_files()
                .await
                .unwrap_or_default()
                .iter()
                .map(|h| h.path().to_string_lossy().into_owned())
                .collect();
            let r = (!picked.is_empty()).then(|| shell.add_files(picked, folder));
            Msg::Apply(Box::new(move |ws: &mut Workspace, b: &Backend| match r {
                Some(Ok(added)) => {
                    ws.refresh_tree(b);
                    ws.notice(
                        format!("Added {}.", plural(added.len(), "file", "files")),
                        Tone::Ok,
                    );
                }
                Some(Err(e)) => ws.file_failure(e),
                None => {}
            }) as crate::backend::Job)
        });
    }

    pub(super) fn show_unused(&mut self, backend: &Backend) {
        self.files.dialog = Some(FileDialog::Unused {
            files: None,
            picked: Vec::new(),
        });
        Workspace::job(
            backend,
            |s| s.unused_files(),
            |ws, _, r| match r {
                Ok(files) => {
                    if let Some(FileDialog::Unused { files: f, .. }) = &mut ws.files.dialog {
                        *f = Some(files);
                    }
                }
                Err(e) => {
                    ws.files.dialog = None;
                    ws.file_failure(e);
                }
            },
        );
    }

    pub(super) fn show_deleted(&mut self, backend: &Backend) {
        self.files.dialog = Some(FileDialog::Deleted(None));
        Workspace::job(
            backend,
            |s| s.trash_batches(),
            |ws, _, r| match r {
                Ok(list) => {
                    if let Some(FileDialog::Deleted(l)) = &mut ws.files.dialog {
                        *l = Some(list);
                    }
                }
                Err(e) => {
                    ws.files.dialog = None;
                    ws.file_failure(e);
                }
            },
        );
    }

    fn restore_deleted(&mut self, backend: &Backend, batch: TrashBatch) {
        Workspace::job(
            backend,
            move |s| s.restore_trash(batch.id.clone()).map(|_| batch),
            |ws, b, r| match r {
                Ok(batch) => {
                    if let Some(FileDialog::Deleted(Some(l))) = &mut ws.files.dialog {
                        l.retain(|x| x.id != batch.id);
                    }
                    ws.refresh_tree(b);
                    ws.notice(format!("Restored {}.", describe(&batch.paths)), Tone::Ok);
                }
                Err(e) => ws.file_failure(e),
            },
        );
    }

    /// The tree's keys, while it has the keyboard (Sidebar.svelte onKey).
    pub(super) fn tree_keys(
        &mut self,
        ctx: &egui::Context,
        backend: &Backend,
        settings: &mut Settings,
    ) {
        if ctx.memory(|m| m.focused().is_some()) || self.modal_open() || self.files.dialog.is_some()
        {
            self.sel.focused = false;
        }
        if !self.sel.focused {
            return;
        }
        let rows = self.tree_paths(settings);
        let cur = self.tree_cursor.clone();
        let cur_entry = cur
            .as_deref()
            .and_then(|c| crate::workspace::tree::find(&self.info.tree, c))
            .cloned();
        let folder_of_cursor = self.folder_of(cur.as_deref());
        use crate::shortcuts::consume;
        if consume(ctx, "tree-rename") {
            if let Some(c) = cur.clone() {
                self.start_rename(backend, c);
            }
        } else if consume(ctx, "tree-delete") {
            let t = self.targets(None);
            self.trash(backend, settings, t);
        } else if consume(ctx, "tree-duplicate") {
            let t = self.targets(None);
            self.duplicate(backend, settings, t);
        } else if consume(ctx, "tree-undo") {
            self.undo_files(backend);
        } else if consume(ctx, "tree-select-all") {
            self.sel.selected = rows.clone();
            self.sel.anchor = rows.first().cloned();
        } else if consume(ctx, "tree-new-file") {
            self.new_item(false, folder_of_cursor.clone());
        } else if consume(ctx, "tree-new-folder") {
            self.new_item(true, folder_of_cursor.clone());
        }
        // Copy, cut and paste arrive as their own events.
        let (copy, cut, paste) = ctx.input_mut(|i| {
            let (mut c, mut x, mut v) = (false, false, false);
            i.events.retain(|e| match e {
                egui::Event::Copy => !std::mem::replace(&mut c, true),
                egui::Event::Cut => !std::mem::replace(&mut x, true),
                egui::Event::Paste(_) => !std::mem::replace(&mut v, true),
                _ => true,
            });
            (c, x, v)
        });
        if copy || cut {
            let t = self.targets(None);
            self.clip(ctx, cut, t);
        }
        if paste {
            self.paste(backend, settings, folder_of_cursor.clone());
        }
        let plain = |i: &mut egui::InputState, k: Key| {
            i.consume_key(Modifiers::NONE, k) || i.consume_key(Modifiers::SHIFT, k)
        };
        let extend = ctx.input(|i| i.modifiers.shift);
        let mut go: Option<usize> = None;
        let at = cur.as_ref().and_then(|c| rows.iter().position(|r| r == c));
        let n = rows.len();
        ctx.input_mut(|i| {
            if n == 0 {
                return;
            }
            if plain(i, Key::ArrowDown) {
                go = Some(at.map_or(0, |a| (a + 1).min(n - 1)));
            } else if plain(i, Key::ArrowUp) {
                go = Some(at.map_or(n - 1, |a| a.saturating_sub(1)));
            } else if plain(i, Key::Home) {
                go = Some(0);
            } else if plain(i, Key::End) {
                go = Some(n - 1);
            }
        });
        if let Some(g) = go {
            let path = rows[g].clone();
            if extend && let Some(a) = self.sel.anchor.clone() {
                self.sel.selected = range(&rows, &a, &path);
                self.tree_cursor = Some(path);
            } else {
                self.tree_select(&path);
            }
            return;
        }
        let (right, left, enter, escape, menu) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowRight),
                i.consume_key(Modifiers::NONE, Key::ArrowLeft),
                i.consume_key(Modifiers::NONE, Key::Enter)
                    || i.consume_key(Modifiers::NONE, Key::Space),
                i.consume_key(Modifiers::NONE, Key::Escape),
                i.consume_key(Modifiers::SHIFT, Key::F10),
            )
        });
        if let Some(e) = &cur_entry {
            if right
                && e.is_dir
                && !self.collapsed.remove(&e.path)
                && let Some(a) = at
            {
                let next = rows[(a + 1).min(n - 1)].clone();
                self.tree_select(&next);
            }
            if left {
                if e.is_dir && !self.collapsed.contains(&e.path) {
                    self.collapsed.insert(e.path.clone());
                } else if !parent_of(&e.path).is_empty() {
                    let p = parent_of(&e.path).to_owned();
                    self.tree_select(&p);
                }
            }
            if enter {
                if e.is_dir {
                    if !self.collapsed.remove(&e.path) {
                        self.collapsed.insert(e.path.clone());
                    }
                } else if crate::workspace::tree::is_text_file(&e.path)
                    || crate::workspace::tree::is_viewer_file(&e.path)
                {
                    self.open_file(backend, &e.path);
                }
            }
            if menu {
                let at = ctx.content_rect().center();
                self.menu = Some(Menu::Tree(
                    Some(e.path.clone()),
                    Rect::from_min_size(at, vec2(0.0, 0.0)),
                ));
            }
        }
        if escape {
            self.sel.selected = cur.iter().cloned().collect();
        }
        // Type a few letters to jump to the next row starting with them.
        let typed: String = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(t) if !i.modifiers.command && !i.modifiers.alt => {
                        Some(t.to_lowercase())
                    }
                    _ => None,
                })
                .collect()
        });
        if !typed.is_empty() {
            if self
                .sel
                .typed_at
                .is_none_or(|t| t.elapsed().as_millis() > 700)
            {
                self.sel.typed.clear();
            }
            self.sel.typed_at = Some(std::time::Instant::now());
            self.sel.typed.push_str(&typed);
            let start = at.map_or(0, |a| a + usize::from(self.sel.typed.chars().count() == 1));
            let hit = rows[start.min(n)..]
                .iter()
                .chain(&rows[..start.min(n)])
                .find(|r| base_name(r).to_lowercase().starts_with(&self.sel.typed))
                .cloned();
            if let Some(h) = hit {
                self.tree_select(&h);
            }
        }
    }

    /// The right-click menu of a row (`entry`) or of the tree's empty space.
    pub(super) fn tree_menu(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        backend: &Backend,
        settings: &mut Settings,
        entry: Option<String>,
        at: Rect,
    ) {
        let picked = entry
            .as_deref()
            .map(|e| self.targets(Some(e)))
            .unwrap_or_default();
        let many = picked.len() > 1;
        let target = entry
            .as_deref()
            .and_then(|e| crate::workspace::tree::find(&self.info.tree, e))
            .cloned();
        let system = settings.trash_mode == "system";
        let undo_label = self
            .files
            .log
            .last()
            .map(|(l, _)| format!("Undo {}", l.to_lowercase()));
        let heading = format!("{} items selected", picked.len());
        let paste_label = if target.is_some() {
            "Paste"
        } else {
            "Paste here"
        };
        // (item, what it does)
        let mut items: Vec<(MenuItem, u8)> = Vec::new();
        if many {
            items.push((MenuItem::Heading(&heading), 0));
        } else {
            items.push((MenuItem::new(icons::FILE_PLUS, "New file"), 1));
            items.push((MenuItem::new(icons::FOLDER_PLUS, "New folder"), 2));
            items.push((MenuItem::new(icons::UPLOAD, "Add files…"), 3));
            items.push((MenuItem::Separator, 0));
        }
        if target.is_some() {
            items.push((MenuItem::new(icons::SCISSORS, "Cut"), 4));
            items.push((MenuItem::new(icons::COPY, "Copy"), 5));
        }
        if self.sel.clipboard.is_some() && !many {
            items.push((MenuItem::new(icons::CLIPBOARD_PASTE, paste_label), 6));
        }
        if target.is_some() && !many {
            items.push((MenuItem::new(icons::SQUARE_PEN, "Rename…"), 7));
        }
        if target.is_some() {
            items.push((MenuItem::new(icons::FILES, "Duplicate"), 8));
        }
        if let Some(t) = target.as_ref().filter(|_| !many) {
            if !t.is_dir && t.path != self.info.main && t.name.to_lowercase().ends_with(".tex") {
                items.push((MenuItem::new(icons::FLAG, "Set as main file"), 9));
            }
            items.push((MenuItem::new(icons::COPY, "Copy path"), 10));
            items.push((
                MenuItem::new(icons::FOLDER_SEARCH, "Show in file manager"),
                11,
            ));
        }
        if target.is_some() {
            items.push((
                MenuItem::new(
                    icons::TRASH,
                    if system { "Move to trash…" } else { "Delete" },
                )
                .danger(),
                12,
            ));
        } else {
            items.push((
                MenuItem::new(icons::CHEVRONS_DOWN_UP, "Collapse all folders"),
                13,
            ));
            items.push((
                MenuItem::new(icons::CHEVRONS_UP_DOWN, "Expand all folders"),
                14,
            ));
            items.push((MenuItem::new(icons::SCAN_SEARCH, "Find unused files…"), 15));
            if let Some(l) = &undo_label {
                items.push((MenuItem::new(icons::UNDO_2, l), 16));
            }
            items.push((MenuItem::new(icons::HISTORY, "Recently deleted…"), 17));
        }
        let list: Vec<MenuItem> = items.iter().map(|(m, _)| m.clone()).collect();
        let what = match widgets::drop_menu(ctx, p, egui::Id::unique("tree-menu"), at, false, &list)
        {
            MenuResult::Open => return,
            MenuResult::Closed => {
                self.menu = None;
                return;
            }
            MenuResult::Clicked(i) => items[i].1,
        };
        self.menu = None;
        let folder = self.folder_of(entry.as_deref());
        match what {
            1 | 2 => self.new_item(what == 2, folder),
            3 => self.upload(backend, folder),
            4 | 5 => self.clip(ctx, what == 4, picked),
            6 => self.paste(backend, settings, folder),
            7 => {
                if let Some(t) = target {
                    self.start_rename(backend, t.path);
                }
            }
            8 => self.duplicate(backend, settings, picked),
            9 => {
                if let Some(t) = target {
                    self.set_main(backend, settings, t.path);
                }
            }
            10 => {
                if let Some(t) = target {
                    ctx.copy_text(t.path);
                }
            }
            11 => {
                if let Some(t) = target {
                    let abs = self.info.root.join(&t.path);
                    let dir = if t.is_dir {
                        abs
                    } else {
                        abs.parent().map(|d| d.to_path_buf()).unwrap_or(abs)
                    };
                    let _ = open::that_detached(dir);
                }
            }
            12 => self.trash(backend, settings, picked),
            13 => {
                fn dirs(entries: &[Entry], out: &mut std::collections::HashSet<String>) {
                    for e in entries.iter().filter(|e| e.is_dir) {
                        out.insert(e.path.clone());
                        dirs(&e.children, out);
                    }
                }
                dirs(&self.info.tree, &mut self.collapsed);
            }
            14 => self.collapsed.clear(),
            15 => self.show_unused(backend),
            16 => self.undo_files(backend),
            17 => self.show_deleted(backend),
            _ => {}
        }
    }

    pub(super) fn file_dialogs(
        &mut self,
        ctx: &egui::Context,
        p: &Palette,
        backend: &Backend,
        settings: &Settings,
    ) {
        let Some(mut dialog) = self.files.dialog.take() else {
            return;
        };
        let system = settings.trash_mode == "system";
        let title = match &dialog {
            FileDialog::New { dir: true, .. } => "New folder".to_owned(),
            FileDialog::New { .. } => "New file".to_owned(),
            FileDialog::Move { paths, .. } => format!("Move {}", describe(paths)),
            FileDialog::Rename { path, .. } => format!("Rename {}", base_name(path)),
            FileDialog::Unused { .. } => "Files nothing refers to".to_owned(),
            FileDialog::Trash { paths, .. } if system => {
                format!("Move {} to the trash?", describe(paths))
            }
            FileDialog::Trash { paths, .. } => format!("Delete {}?", describe(paths)),
            FileDialog::Deleted(_) => "Recently deleted".to_owned(),
        };
        let subtitle = matches!(dialog, FileDialog::Deleted(_))
            .then_some("Files deleted in this project stay here for a week.");
        let submit_label = match &dialog {
            FileDialog::New { .. } => "Create",
            FileDialog::Rename { .. } => "Rename",
            FileDialog::Move { .. } => "Move",
            FileDialog::Unused { .. } => "Delete selected",
            _ => "Delete",
        };
        let danger = matches!(dialog, FileDialog::Trash { .. } | FileDialog::Unused { .. });
        let can_submit = match &dialog {
            FileDialog::Unused { files, picked } => {
                files.as_ref().is_some_and(|f| !f.is_empty()) && !picked.is_empty()
            }
            FileDialog::New { name, .. } | FileDialog::Rename { name, .. } => {
                !name.trim().is_empty()
            }
            _ => true,
        };
        let shows_submit = match &dialog {
            FileDialog::Unused { files, .. } => files.as_ref().is_some_and(|f| !f.is_empty()),
            FileDialog::Deleted(_) => false,
            _ => true,
        };
        let close_label = match &dialog {
            FileDialog::Unused { files: Some(f), .. } if f.is_empty() => "Close",
            FileDialog::Deleted(_) => "Close",
            _ => "Cancel",
        };
        let (mut submit, mut enter, mut cancel) = (false, false, false);
        let mut restore = None;
        let error = self.files.error.clone();
        let focus = std::mem::take(&mut self.files.focus);
        let width = if matches!(dialog, FileDialog::Deleted(_)) {
            480.0
        } else {
            460.0
        };
        let r = widgets::modal_with(
            ctx,
            p,
            &title,
            subtitle,
            width,
            |ui| {
                ui.spacing_mut().item_spacing.y = 12.0;
                let muted = |ui: &mut Ui, t: &str| {
                    ui.add(egui::Label::new(RichText::new(t).color(p.muted)).wrap());
                };
                match &mut dialog {
                    FileDialog::New { dir, folder, name } => {
                        let r = widgets::field(ui, p, "Name", |ui| {
                            widgets::text_input(
                                ui,
                                p,
                                name,
                                if *dir { "chapters" } else { "introduction" },
                                true,
                            )
                        });
                        if focus {
                            r.request_focus();
                        }
                        enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        let place = if folder.is_empty() {
                            "the project folder"
                        } else {
                            folder.as_str()
                        };
                        muted(
                            ui,
                            &format!(
                                "In {place}.{}",
                                if *dir {
                                    ""
                                } else {
                                    " Without an extension, .tex is added."
                                }
                            ),
                        );
                    }
                    FileDialog::Rename {
                        name, refs, update, ..
                    } => {
                        let r = widgets::field(ui, p, "New name", |ui| {
                            widgets::text_input(ui, p, name, "", true)
                        });
                        if focus {
                            r.request_focus();
                            // The name without its extension is selected.
                            let end = name.rfind('.').filter(|&i| i > 0).unwrap_or(name.len());
                            let end = name[..end].chars().count();
                            if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), r.id) {
                                st.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(end),
                                )));
                                st.store(ui.ctx(), r.id);
                            }
                        }
                        enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        if !refs.is_empty() {
                            widgets::checkbox(
                                ui,
                                p,
                                update,
                                &format!(
                                    "Also update {} to it",
                                    plural(refs.len(), "reference", "references")
                                ),
                            );
                            ref_list(ui, p, refs);
                        }
                    }
                    FileDialog::Move {
                        paths,
                        folder,
                        refs,
                        update,
                    } => {
                        let place = if folder.is_empty() {
                            "the project folder".to_owned()
                        } else {
                            folder.clone()
                        };
                        widgets::Rich::new()
                            .add("Move ", FontId::proportional(13.0), p.muted)
                            .add(
                                &describe(paths),
                                FontId::new(13.0, crate::theme::font::sans_600()),
                                p.muted,
                            )
                            .add(
                                &format!(" to {place}."),
                                FontId::proportional(13.0),
                                p.muted,
                            )
                            .show(ui);
                        let them = if paths.len() == 1 { "it" } else { "them" };
                        widgets::checkbox(
                            ui,
                            p,
                            update,
                            &format!(
                                "Also update {} to {them}",
                                plural(refs.len(), "reference", "references")
                            ),
                        );
                        ref_list(ui, p, refs);
                    }
                    FileDialog::Unused { files, picked } => match files {
                        None => muted(ui, "Looking…"),
                        Some(f) if f.is_empty() => muted(
                            ui,
                            "Every image, .tex file and bibliography is referenced somewhere.",
                        ),
                        Some(f) => {
                            muted(
                                ui,
                                "No \\input, \\include, \\includegraphics or bibliography command names these. Check the ones to delete; they can be restored with Undo.",
                            );
                            boxed(ui, p, |ui| {
                                for file in f.iter() {
                                    let mut on = picked.contains(file);
                                    if widgets::checkbox(ui, p, &mut on, file).clicked() {
                                        if on {
                                            picked.push(file.clone());
                                        } else {
                                            picked.retain(|x| x != file);
                                        }
                                    }
                                }
                            });
                        }
                    },
                    FileDialog::Trash { paths, refs } => {
                        let it = if paths.len() == 1 { "it" } else { "them" };
                        if system {
                            muted(
                                ui,
                                &format!("You can restore {it} from your system's trash."),
                            );
                        } else {
                            muted(ui, &format!("You can bring {it} back with Undo."));
                        }
                        if !refs.is_empty() {
                            let these = if paths.len() == 1 { "it" } else { "these" };
                            ui.label(
                                RichText::new(format!(
                                    "{} to {these}:",
                                    plural(refs.len(), "line still refers", "lines still refer")
                                ))
                                .color(p.warn),
                            );
                            ref_list(ui, p, refs);
                        }
                    }
                    FileDialog::Deleted(list) => match list {
                        None => muted(ui, "Looking…"),
                        Some(l) if l.is_empty() => muted(
                            ui,
                            &format!(
                                "Nothing was deleted in the last week{}.",
                                if system {
                                    " (deleted files go to the system trash)"
                                } else {
                                    ""
                                }
                            ),
                        ),
                        Some(l) => {
                            egui::ScrollArea::vertical()
                                .max_height(320.0)
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 6.0;
                                    for b in l.iter() {
                                        egui::Frame::new()
                                            .stroke(Stroke::new(1.0, p.border))
                                            .corner_radius(crate::theme::R)
                                            .inner_margin(Margin::symmetric(8, 6))
                                            .show(ui, |ui| {
                                                ui.set_width(ui.available_width());
                                                ui.horizontal(|ui| {
                                                    ui.allocate_ui_with_layout(
                                                        vec2(ui.available_width() - 80.0, 0.0),
                                                        egui::Layout::top_down(egui::Align::Min),
                                                        |ui| {
                                                            ui.spacing_mut().item_spacing.y = 2.0;
                                                            ui.add(
                                                                egui::Label::new(
                                                                    RichText::new(
                                                                        b.paths.join(", "),
                                                                    )
                                                                    .font(FontId::monospace(12.0))
                                                                    .color(p.ink),
                                                                )
                                                                .wrap(),
                                                            );
                                                            let when =
                                                                b.id.parse::<f64>()
                                                                    .map(crate::when::medium_ms)
                                                                    .unwrap_or_default();
                                                            ui.label(
                                                                RichText::new(when)
                                                                    .size(11.0)
                                                                    .color(p.muted),
                                                            );
                                                        },
                                                    );
                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(
                                                            egui::Align::Center,
                                                        ),
                                                        |ui| {
                                                            if Button::new("Restore")
                                                                .size(Size::Sm)
                                                                .show(ui, p)
                                                                .clicked()
                                                            {
                                                                restore = Some(b.clone());
                                                            }
                                                        },
                                                    );
                                                });
                                            });
                                    }
                                });
                        }
                    },
                }
                if let Some(e) = &error {
                    ui.label(RichText::new(e).color(p.error));
                }
            },
            Some(&mut |ui: &mut Ui| {
                if shows_submit
                    && Button::new(submit_label)
                        .kind(if danger { Kind::Danger } else { Kind::Primary })
                        .enabled(can_submit)
                        .show(ui, p)
                        .clicked()
                {
                    submit = true;
                }
                if Button::new(close_label).show(ui, p).clicked() {
                    cancel = true;
                }
            }),
        );
        if let Some(b) = restore {
            self.restore_deleted(backend, b);
        }
        if r.closed || cancel {
            self.files.error = None;
            return;
        }
        if !((submit || enter) && can_submit) {
            self.files.dialog = Some(dialog);
            return;
        }
        self.files.error = None;
        match dialog {
            FileDialog::New { dir, folder, name } => {
                let name = name.trim();
                let path = join_path(
                    &folder,
                    &if dir || name.contains('.') {
                        name.to_owned()
                    } else {
                        format!("{name}.tex")
                    },
                );
                let keep = FileDialog::New {
                    dir,
                    folder,
                    name: name.to_owned(),
                };
                self.files.dialog = Some(keep);
                Workspace::job(
                    backend,
                    move |s| s.create_path(path.clone(), dir).map(|_| path),
                    move |ws, b, r| match r {
                        Ok(path) => {
                            ws.files.dialog = None;
                            ws.push_undo(
                                format!("Create {}", base_name(&path)),
                                Undo::Trash(vec![path.clone()]),
                            );
                            ws.refresh_tree(b);
                            if !dir && crate::workspace::tree::is_text_file(&path) {
                                ws.open_file(b, &path);
                            }
                        }
                        Err(e) => ws.files.error = Some(e),
                    },
                );
            }
            FileDialog::Rename {
                path,
                name,
                refs,
                update,
            } => {
                let to = join_path(parent_of(&path), name.trim());
                let reload: Vec<String> = refs.iter().map(|r| r.file.clone()).collect();
                self.files.dialog = Some(FileDialog::Rename {
                    path: path.clone(),
                    name,
                    refs,
                    update,
                });
                self.save_all_quietly(backend, settings);
                self.after_writes.push(Box::new(move |_, b| {
                    Workspace::job(
                        b,
                        move |s| {
                            s.rename_path(path.clone(), to.clone(), update)
                                .map(|_| (path, to))
                        },
                        move |ws, b, r| match r {
                            Ok((from, to)) => {
                                ws.files.dialog = None;
                                ws.renamed(&from, &to);
                                if update {
                                    for f in &reload {
                                        ws.reload(b, f);
                                    }
                                }
                                ws.push_undo(
                                    format!("Rename {}", base_name(&from)),
                                    Undo::Rename(vec![(to, from)], update),
                                );
                                ws.refresh_tree(b);
                            }
                            Err(e) => ws.files.error = Some(e),
                        },
                    );
                }));
            }
            FileDialog::Move {
                paths,
                folder,
                update,
                ..
            } => {
                self.save_all_quietly(backend, settings);
                self.run_move(paths, folder, update);
            }
            FileDialog::Trash { paths, .. } => self.run_trash(backend, paths, system),
            FileDialog::Unused { picked, .. } => self.run_trash(backend, picked, system),
            FileDialog::Deleted(_) => {}
        }
    }
}

/// `.refs`: the lines that point at a file.
fn ref_list(ui: &mut Ui, p: &Palette, refs: &[Reference]) {
    boxed(ui, p, |ui| {
        let font = FontId::monospace(11.5);
        for r in refs {
            let job = widgets::Rich::new()
                .add(&format!("{}:{}", r.file, r.line), font.clone(), p.muted)
                .add(&format!(" {}", r.text), font.clone(), p.ink);
            ui.add(egui::Label::new(job.0).truncate());
        }
    });
}

/// A list box on --surface-2, at most 190 px tall.
fn boxed(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(p.surface_2)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(crate::theme::R)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let w = ui.available_width();
            ui.allocate_ui(vec2(w, 174.0), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(174.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        add(ui);
                    });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    // treeModel.test.ts
    #[test]
    fn names_paths_and_ranges() {
        let taken = vec!["main copy.tex".to_owned()];
        assert_eq!(copy_name("main.tex", false, &[]), "main copy.tex");
        assert_eq!(copy_name("main.tex", false, &taken), "main copy 2.tex");
        assert_eq!(copy_name("figs.v2", true, &[]), "figs.v2 copy");
        assert_eq!(copy_name(".latexmkrc", false, &[]), ".latexmkrc copy");
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(top_level(&s(&["a", "a/b", "c", "a"])), s(&["a", "c"]));
        let rows = s(&["a", "b", "c", "d"]);
        assert_eq!(range(&rows, "c", "a"), s(&["a", "b", "c"]));
        assert_eq!(range(&rows, "x", "b"), s(&["b"]));
        assert_eq!(remap("ch/a.tex", "ch", "chapters"), "chapters/a.tex");
        assert_eq!(remap("chx/a.tex", "ch", "chapters"), "chx/a.tex");
        assert_eq!(parent_of("a/b/c.tex"), "a/b");
        assert_eq!(join_path("", "x"), "x");
        // treeModel.test.ts canMove
        assert!(can_move(&s(&["a/x.tex"]), ""));
        assert!(!can_move(&s(&["a/x.tex"]), "a"), "already there");
        assert!(!can_move(&s(&["a"]), "a/b"), "into itself");
        assert!(can_move(&s(&["a", "b/c.tex"]), "b/d"));
    }
}
