// Recently opened project folders, newest first, with when they were last
// opened. Older versions stored plain paths; those still load.
const KEY = "lemmata.recent";
const MAX = 30;

export interface Recent {
  path: string;
  lastOpened: number;
}

type Storage_ = Pick<Storage, "getItem" | "setItem">;

export function recentEntries(storage: Pick<Storage, "getItem"> = localStorage): Recent[] {
  try {
    const parsed = JSON.parse(storage.getItem(KEY) ?? "[]");
    if (!Array.isArray(parsed)) return [];
    return parsed
      .map((e) => (typeof e === "string" ? { path: e, lastOpened: 0 } : e))
      .filter((e): e is Recent => typeof e?.path === "string" && typeof e?.lastOpened === "number");
  } catch {
    return [];
  }
}

function write(list: Recent[], storage: Storage_) {
  try {
    storage.setItem(KEY, JSON.stringify(list));
  } catch {
    // Storage full or unavailable: the list just isn't remembered.
  }
}

/** Paths only, newest first. */
export function recentProjects(storage: Pick<Storage, "getItem"> = localStorage): string[] {
  return recentEntries(storage).map((e) => e.path);
}

export function rememberProject(path: string, storage: Storage_ = localStorage, now = Date.now()): Recent[] {
  const list = [{ path, lastOpened: now }, ...recentEntries(storage).filter((e) => e.path !== path)].slice(0, MAX);
  write(list, storage);
  return list;
}

export function rememberProjects(paths: string[], storage: Storage_ = localStorage, now = Date.now()): Recent[] {
  let list = recentEntries(storage);
  for (const p of [...paths].reverse()) list = [{ path: p, lastOpened: now }, ...list.filter((e) => e.path !== p)];
  list = list.slice(0, MAX);
  write(list, storage);
  return list;
}

export function forgetProject(path: string, storage: Storage_ = localStorage): Recent[] {
  const list = recentEntries(storage).filter((e) => e.path !== path);
  write(list, storage);
  return list;
}
