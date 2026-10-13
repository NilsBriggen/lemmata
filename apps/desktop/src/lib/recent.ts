// Recently opened project folders, newest first. Kept in localStorage for
// now; moves to tauri-plugin-store with the project library in M3.
const KEY = "lemmata.recent";
const MAX = 10;

export function recentProjects(storage: Pick<Storage, "getItem"> = localStorage): string[] {
  try {
    const parsed = JSON.parse(storage.getItem(KEY) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter((p) => typeof p === "string") : [];
  } catch {
    return [];
  }
}

export function rememberProject(path: string, storage: Pick<Storage, "getItem" | "setItem"> = localStorage): string[] {
  const list = [path, ...recentProjects(storage).filter((p) => p !== path)].slice(0, MAX);
  try {
    storage.setItem(KEY, JSON.stringify(list));
  } catch {
    // Storage full or unavailable: the list just isn't remembered.
  }
  return list;
}
