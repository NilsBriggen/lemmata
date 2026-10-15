// Pure logic behind the file tree: sorting, the flat list of visible rows,
// range selection and file names for copies. No DOM, so it is unit-tested.
import type { Entry } from "./api";

export type SortMode = "name" | "type" | "modified" | "size";

const ext = (name: string) => {
  const i = name.lastIndexOf(".");
  return i > 0 ? name.slice(i + 1).toLowerCase() : "";
};
const byName = (a: Entry, b: Entry) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });

/** A sorted copy of the tree. Folders always sort by name; `foldersFirst` keeps them above files. */
export function sortEntries(entries: Entry[], mode: SortMode, foldersFirst: boolean): Entry[] {
  const cmp = (a: Entry, b: Entry): number => {
    if (foldersFirst && a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
    if (a.is_dir && b.is_dir) return byName(a, b);
    switch (mode) {
      case "type":
        return ext(a.name).localeCompare(ext(b.name)) || byName(a, b);
      case "modified":
        return b.modified - a.modified || byName(a, b);
      case "size":
        return b.size - a.size || byName(a, b);
      default:
        return byName(a, b);
    }
  };
  return [...entries].sort(cmp).map((e) => (e.is_dir ? { ...e, children: sortEntries(e.children, mode, foldersFirst) } : e));
}

/** The rows on screen, top to bottom, skipping the insides of collapsed folders. */
export function flatten(entries: Entry[], collapsed: Record<string, boolean>): Entry[] {
  const out: Entry[] = [];
  const walk = (list: Entry[]) => {
    for (const e of list) {
      out.push(e);
      if (e.is_dir && !collapsed[e.path]) walk(e.children);
    }
  };
  walk(entries);
  return out;
}

/** Paths from `a` to `b` inclusive, in row order (either may come first). */
export function range(rows: Entry[], a: string, b: string): string[] {
  const i = rows.findIndex((r) => r.path === a);
  const j = rows.findIndex((r) => r.path === b);
  if (i < 0 || j < 0) return [b];
  return rows.slice(Math.min(i, j), Math.max(i, j) + 1).map((r) => r.path);
}

const TEXT_EXT = /\.(tex|bib|sty|cls|bst|bbx|cbx|tikz|txt|md|markdown|toml|csv|tsv|json|ya?ml|xml|ini|cfg|conf|py|r|m|jl|sh|lua)$/i;
const TEXT_NAME = /^(\.?latexmkrc|makefile|readme|license|changelog|\.gitignore|\.gitattributes|\.editorconfig)$/i;
/** Files the editor opens as text. Everything that opens anywhere (tree, palette, keyboard) uses this. */
export const isTextFile = (path: string) => TEXT_EXT.test(path) || TEXT_NAME.test(baseName(path));
/** Images and PDFs open in a read-only viewer tab. */
export const isViewerFile = (path: string) => /\.(png|jpe?g|gif|webp|svg|pdf)$/i.test(path);

export const parentOf = (path: string) => path.split("/").slice(0, -1).join("/");
export const baseName = (path: string) => path.split("/").pop() ?? path;
export const joinPath = (folder: string, name: string) => (folder ? `${folder}/${name}` : name);

/** Drops paths that sit inside another selected path: acting on a folder already covers its contents. */
export function topLevel(paths: string[]): string[] {
  return paths.filter((p, i) => paths.indexOf(p) === i && !paths.some((o) => o !== p && p.startsWith(o + "/")));
}

/** Can all of `paths` be moved into `folder`? Not into themselves, their own children, or where they already are. */
export function canMove(paths: string[], folder: string): boolean {
  const movable = topLevel(paths).filter((p) => parentOf(p) !== folder);
  return movable.length > 0 && movable.every((p) => folder !== p && !folder.startsWith(p + "/"));
}

/** "main copy.tex", "main copy 2.tex", … the first name not in `taken`. */
export function copyName(name: string, isDir: boolean, taken: Set<string>): string {
  const dot = name.lastIndexOf(".");
  const [stem, tail] = !isDir && dot > 0 ? [name.slice(0, dot), name.slice(dot)] : [name, ""];
  for (let i = 1; i < 1000; i++) {
    const candidate = `${stem} copy${i > 1 ? ` ${i}` : ""}${tail}`;
    if (!taken.has(candidate)) return candidate;
  }
  return `${stem} copy ${Date.now()}${tail}`;
}

/** Names directly inside `folder` ("" is the project root). */
export function namesIn(entries: Entry[], folder: string): Set<string> {
  if (!folder) return new Set(entries.map((e) => e.name));
  const find = (list: Entry[]): Entry | undefined => {
    for (const e of list) {
      if (e.path === folder) return e;
      if (e.is_dir && folder.startsWith(e.path + "/")) return find(e.children);
    }
  };
  return new Set((find(entries)?.children ?? []).map((e) => e.name));
}

export function findEntry(entries: Entry[], path: string): Entry | undefined {
  for (const e of entries) {
    if (e.path === path) return e;
    if (e.is_dir && path.startsWith(e.path + "/")) return findEntry(e.children, path);
  }
}

/** Number of files (not folders) under an entry, for the folder badge. */
export function countFiles(e: Entry): number {
  return e.is_dir ? e.children.reduce((n, c) => n + countFiles(c), 0) : 1;
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(bytes < 10 * 1024 ? 1 : 0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** Remaps a path after `from` was renamed or moved to `to`. */
export const remap = (p: string, from: string, to: string) => (p === from ? to : p.startsWith(from + "/") ? to + p.slice(from.length) : p);
