// Tauri commands for online reference search and the bibliography manager
// (src-tauri/src/bib.rs).
import { invoke } from "@tauri-apps/api/core";

export interface OnlineHit { title: string; authors: string; year: string; venue: string; doi: string }

export interface ManagedEntry {
  key: string; kind: string; title: string; authors: string; year: string;
  /** The .bib file, relative to the project root. */
  file: string;
  /** 1-based line of the entry in `file`. */
  line: number;
  doi: string;
  fields: Record<string, string>;
  /** Some .tex file of the project cites this key. */
  cited: boolean;
}

export interface Duplicate {
  kind: "key" | "doi" | "title";
  value: string;
  /** Indices into `Library.entries`. */
  entries: number[];
}

export interface Library {
  files: string[];
  entries: ManagedEntry[];
  duplicates: Duplicate[];
  /** Cited in the .tex files but defined in no .bib file. */
  missing: string[];
}

export const bibApi = {
  searchOnline: (query: string) => invoke<OnlineHit[]>("bib_search_online", { query }),
  library: () => invoke<Library>("bib_library"),
  /** Each of these resolves to the paths that changed on disk. */
  removeEntry: (file: string, key: string, line: number) => invoke<string[]>("bib_remove_entry", { file, key, line }),
  renameKey: (file: string, key: string, line: number, newKey: string) => invoke<string[]>("bib_rename_key", { file, key, line, newKey }),
};
