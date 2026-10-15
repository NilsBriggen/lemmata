// Invoke wrappers for the TODO list and the document formatter (src-tauri/src/tools.rs), plus the grouping helper.
import { invoke } from "@tauri-apps/api/core";

export interface Todo {
  /** Project-relative path, forward slashes. */
  file: string;
  line: number;
  /** "TODO", "FIXME" or "XXX" for comments; "TODO" for \todo notes. */
  kind: string;
  text: string;
}

export interface TodoGroup {
  file: string;
  todos: Todo[];
}

/** Groups todos by file, keeping the order they arrive in (the Rust side sorts by file, then line). */
export function groupTodos(todos: Todo[]): TodoGroup[] {
  const groups = new Map<string, Todo[]>();
  for (const t of todos) {
    const list = groups.get(t.file);
    if (list) list.push(t);
    else groups.set(t.file, [t]);
  }
  return [...groups].map(([file, list]) => ({ file, todos: list }));
}

export const todoApi = {
  list: () => invoke<Todo[]>("todos"),
  /** `indent` is a tab ("\t") or 1 to 8 spaces. Rejects with a message when no formatter is installed. */
  formatTex: (text: string, indent: string) => invoke<string>("format_tex", { text, indent }),
};
