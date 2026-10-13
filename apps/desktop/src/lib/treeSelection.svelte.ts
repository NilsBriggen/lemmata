// Selection, keyboard cursor and collapsed folders of the file tree. Shared
// by the nested Tree components, so it lives at module level.
import type { Entry } from "./api";
import { flatten, range, topLevel } from "./treeModel";

export const tree = $state({
  selected: [] as string[],
  /** Where Shift extends a range from. */
  anchor: null as string | null,
  /** The row the keyboard acts on. */
  cursor: null as string | null,
  collapsed: {} as Record<string, boolean>,
  /** Copied or cut entries waiting for Paste. */
  clipboard: null as { mode: "copy" | "cut"; paths: string[] } | null,
  /** `git status` letters per file (M, A, D, R, ?, U); empty outside a repository. */
  git: {} as Record<string, string>,
  /** Folders with at least one changed file inside, for the dot on their rows. */
  gitFolders: {} as Record<string, boolean>,
});

export function setGitStatus(list: { path: string; status: string }[]) {
  tree.git = Object.fromEntries(list.map((f) => [f.path, f.status]));
  const folders: Record<string, boolean> = {};
  for (const f of list) {
    for (let i = f.path.indexOf("/"); i >= 0; i = f.path.indexOf("/", i + 1)) folders[f.path.slice(0, i)] = true;
  }
  tree.gitFolders = folders;
}

export const isSelected = (path: string) => tree.selected.includes(path);

export function clearSelection() {
  tree.selected = [];
  tree.anchor = null;
}

export function select(path: string) {
  tree.selected = [path];
  tree.anchor = path;
  tree.cursor = path;
}

/** Click semantics: plain selects one, Ctrl/Cmd toggles, Shift extends from the anchor. */
export function click(path: string, rows: Entry[], mods: { ctrl: boolean; shift: boolean }) {
  tree.cursor = path;
  if (mods.shift && tree.anchor) {
    tree.selected = range(rows, tree.anchor, path);
  } else if (mods.ctrl) {
    tree.selected = isSelected(path) ? tree.selected.filter((p) => p !== path) : [...tree.selected, path];
    tree.anchor = path;
  } else {
    select(path);
  }
}

/** Moves the cursor by `delta` rows (Shift keeps extending the selection). */
export function moveCursor(rows: Entry[], delta: number, extend: boolean) {
  if (!rows.length) return;
  const i = rows.findIndex((r) => r.path === tree.cursor);
  const next = rows[Math.max(0, Math.min(rows.length - 1, (i < 0 ? (delta > 0 ? -1 : rows.length) : i) + delta))];
  if (extend && tree.anchor) {
    tree.cursor = next.path;
    tree.selected = range(rows, tree.anchor, next.path);
  } else {
    select(next.path);
  }
}

export function cursorTo(rows: Entry[], index: number, extend: boolean) {
  const row = rows.at(index);
  if (!row) return;
  if (extend && tree.anchor) {
    tree.cursor = row.path;
    tree.selected = range(rows, tree.anchor, row.path);
  } else select(row.path);
}

export function selectAll(entries: Entry[]) {
  const rows = flatten(entries, tree.collapsed);
  tree.selected = rows.map((r) => r.path);
  tree.anchor = rows[0]?.path ?? null;
}

/** What an action applies to: the selection, or just the cursor row, or the given row. */
export function targets(fallback?: string): string[] {
  if (fallback && !isSelected(fallback)) return [fallback];
  const picked = tree.selected.length ? tree.selected : tree.cursor ? [tree.cursor] : fallback ? [fallback] : [];
  return topLevel(picked);
}

export function collapseAll(entries: Entry[]) {
  const next: Record<string, boolean> = {};
  const walk = (list: Entry[]) => list.forEach((e) => e.is_dir && ((next[e.path] = true), walk(e.children)));
  walk(entries);
  tree.collapsed = next;
}

export const expandAll = () => (tree.collapsed = {});

/** Opens every folder above `path` so its row is visible. */
export function reveal(path: string) {
  const parts = path.split("/").slice(0, -1);
  for (let i = 1; i <= parts.length; i++) delete tree.collapsed[parts.slice(0, i).join("/")];
}

/** Drops selection and clipboard entries for paths that no longer exist. */
export function prune(exists: (path: string) => boolean) {
  tree.selected = tree.selected.filter(exists);
  if (tree.anchor && !exists(tree.anchor)) tree.anchor = null;
  if (tree.cursor && !exists(tree.cursor)) tree.cursor = null;
  if (tree.clipboard) {
    const paths = tree.clipboard.paths.filter(exists);
    tree.clipboard = paths.length ? { ...tree.clipboard, paths } : null;
  }
}
