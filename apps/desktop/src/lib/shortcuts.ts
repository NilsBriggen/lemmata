// Every keyboard shortcut in one table: the app registers its global ones
// from here, the editor takes its keys from here, and the shortcut sheet
// lists them, so the three can't disagree. Keys use CodeMirror's notation.

export interface Shortcut {
  id: string;
  keys: string;
  label: string;
  group: "Project" | "Editing" | "Writing" | "PDF" | "Navigation" | "Files";
}

export const SHORTCUTS: Shortcut[] = [
  { id: "recompile", keys: "Mod-Enter", label: "Recompile", group: "Project" },
  { id: "save", keys: "Mod-s", label: "Save all files", group: "Project" },
  { id: "snapshot", keys: "Mod-Shift-s", label: "Save a snapshot", group: "Project" },
  { id: "palette", keys: "Mod-Shift-p", label: "Command palette", group: "Navigation" },
  { id: "quick-open", keys: "Mod-p", label: "Open a file by name", group: "Navigation" },
  { id: "search", keys: "Mod-Shift-f", label: "Search in project", group: "Navigation" },
  { id: "focus", keys: "F11", label: "Focus mode", group: "Navigation" },
  { id: "close-tab", keys: "Mod-w", label: "Close tab", group: "Navigation" },
  { id: "next-tab", keys: "Ctrl-Tab", label: "Next tab", group: "Navigation" },
  { id: "prev-tab", keys: "Ctrl-Shift-Tab", label: "Previous tab", group: "Navigation" },
  { id: "goto-heading", keys: "Mod-Shift-o", label: "Go to a section", group: "Navigation" },
  { id: "reopen-tab", keys: "Mod-Shift-t", label: "Reopen the last closed tab", group: "Navigation" },
  { id: "shortcuts", keys: "Mod-Shift-/", label: "Keyboard shortcuts", group: "Navigation" },
  { id: "cite", keys: "Mod-Shift-c", label: "Insert citation", group: "Writing" },
  { id: "symbols", keys: "Mod-Shift-m", label: "Insert symbol", group: "Writing" },
  { id: "bold", keys: "Mod-b", label: "Bold", group: "Writing" },
  { id: "italic", keys: "Mod-i", label: "Italic", group: "Writing" },
  { id: "comment", keys: "Mod-/", label: "Comment or uncomment lines", group: "Writing" },
  { id: "wrap-env", keys: "Mod-Shift-e", label: "Wrap selection in an environment", group: "Writing" },
  { id: "heading-down", keys: "Mod-]", label: "Make heading one level deeper", group: "Writing" },
  { id: "heading-up", keys: "Mod-[", label: "Make heading one level higher", group: "Writing" },
  { id: "next-problem", keys: "F8", label: "Next problem", group: "Editing" },
  { id: "prev-problem", keys: "Shift-F8", label: "Previous problem", group: "Editing" },
  { id: "find", keys: "Mod-f", label: "Find in file", group: "Editing" },
  { id: "fold", keys: "Ctrl-Shift-[", label: "Fold", group: "Editing" },
  { id: "unfold", keys: "Ctrl-Shift-]", label: "Unfold", group: "Editing" },
  { id: "sync", keys: "Mod-j", label: "Show this line in the PDF", group: "PDF" },
  { id: "pdf-search", keys: "Mod-f", label: "Search the PDF (when it has focus)", group: "PDF" },
  { id: "definition", keys: "F12", label: "Go to definition", group: "Editing" },
  { id: "references", keys: "Shift-F12", label: "Find references", group: "Editing" },
  { id: "rename", keys: "F2", label: "Rename symbol", group: "Editing" },
  // These work while the file tree has focus.
  { id: "tree-rename", keys: "F2", label: "Rename the selected file", group: "Files" },
  { id: "tree-delete", keys: "Delete", label: "Delete the selected files", group: "Files" },
  { id: "tree-cut", keys: "Mod-x", label: "Cut files", group: "Files" },
  { id: "tree-copy", keys: "Mod-c", label: "Copy files", group: "Files" },
  { id: "tree-paste", keys: "Mod-v", label: "Paste files", group: "Files" },
  { id: "tree-duplicate", keys: "Mod-d", label: "Duplicate files", group: "Files" },
  { id: "tree-undo", keys: "Mod-z", label: "Undo the last file operation", group: "Files" },
  { id: "tree-select-all", keys: "Mod-a", label: "Select all files", group: "Files" },
  { id: "tree-new-file", keys: "Mod-n", label: "New file in the selected folder", group: "Files" },
  { id: "tree-new-folder", keys: "Mod-Shift-n", label: "New folder", group: "Files" },
];

export const keysOf = (id: string): string => SHORTCUTS.find((s) => s.id === id)?.keys ?? "";

const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

/** "Mod-Shift-p" → "Ctrl+Shift+P" (or ⌘⇧P on a Mac). */
export function display(keys: string): string {
  const parts = keys.split(/-(?=.)/);
  const key = parts.pop()!;
  const mods = parts.map((m) => (m === "Mod" ? (isMac ? "⌘" : "Ctrl") : m === "Shift" ? (isMac ? "⇧" : "Shift") : m === "Alt" ? (isMac ? "⌥" : "Alt") : m === "Ctrl" ? (isMac ? "⌃" : "Ctrl") : m));
  const shown = key.length === 1 ? key.toUpperCase() : key;
  return isMac ? [...mods, shown].join("") : [...mods, shown].join("+");
}

/** Does a keyboard event match a shortcut in the table? */
export function matches(e: KeyboardEvent, id: string): boolean {
  const spec = keysOf(id);
  if (!spec) return false;
  const parts = spec.split(/-(?=.)/);
  const key = parts.pop()!;
  const want = new Set(parts);
  const mod = want.has("Mod") ? (isMac ? e.metaKey : e.ctrlKey) : false;
  if (want.has("Mod") && !(mod || e.metaKey || e.ctrlKey)) return false;
  if (!want.has("Mod") && !want.has("Ctrl") && (e.ctrlKey || e.metaKey)) return false;
  if (want.has("Ctrl") && !e.ctrlKey) return false;
  if (want.has("Shift") !== e.shiftKey) {
    // "?" and "/" differ by Shift; accept either form of Mod-Shift-/.
    if (!(key === "/" && e.key === "?")) return false;
  }
  if (want.has("Alt") !== e.altKey) return false;
  return e.key.toLowerCase() === key.toLowerCase() || (key === "/" && e.key === "?");
}
