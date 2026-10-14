// Folders on the start screen: named groups you put projects into. They only
// organise the list; the project folders on disk never move. Stored next to
// the recent-projects list, so they behave the same way.
const KEY = "lemmata.groups";

export interface Group {
  id: string;
  name: string;
}

export interface Groups {
  groups: Group[];
  /** Project path → the id of the group it is in. A project in no group is absent. */
  members: Record<string, string>;
}

type StorageRead = Pick<Storage, "getItem">;
type StorageWrite = Pick<Storage, "setItem">;

export const emptyGroups = (): Groups => ({ groups: [], members: {} });

export function readGroups(storage: StorageRead = localStorage): Groups {
  try {
    const raw = JSON.parse(storage.getItem(KEY) ?? "null");
    if (!raw || !Array.isArray(raw.groups)) return emptyGroups();
    const groups: Group[] = raw.groups.filter((g: unknown): g is Group => typeof (g as Group)?.id === "string" && typeof (g as Group)?.name === "string");
    const ids = new Set(groups.map((g) => g.id));
    const members: Record<string, string> = {};
    for (const [path, id] of Object.entries(raw.members ?? {})) if (typeof id === "string" && ids.has(id)) members[path] = id;
    return { groups, members };
  } catch {
    return emptyGroups();
  }
}

export function writeGroups(state: Groups, storage: StorageWrite = localStorage) {
  try {
    storage.setItem(KEY, JSON.stringify(state));
  } catch {
    // Storage full or unavailable: the folders just aren't remembered.
  }
}

export const MAX_NAME = 40;

/** Why `name` can't be used, or null when it can. `except` is the group being renamed. */
export function nameProblem(state: Groups, name: string, except?: string): string | null {
  const n = name.trim();
  if (!n) return "Give the folder a name.";
  if (n.length > MAX_NAME) return `Use at most ${MAX_NAME} characters.`;
  if (state.groups.some((g) => g.id !== except && g.name.toLowerCase() === n.toLowerCase())) return "A folder with that name exists.";
  return null;
}

export function createGroup(state: Groups, name: string, id: string = newId()): { state: Groups; id: string } | null {
  if (nameProblem(state, name)) return null;
  return { state: { ...state, groups: [...state.groups, { id, name: name.trim() }] }, id };
}

export function renameGroup(state: Groups, id: string, name: string): Groups | null {
  if (nameProblem(state, name, id)) return null;
  return { ...state, groups: state.groups.map((g) => (g.id === id ? { ...g, name: name.trim() } : g)) };
}

/** Removes the folder; its projects go back to "no folder". */
export function deleteGroup(state: Groups, id: string): Groups {
  const members = Object.fromEntries(Object.entries(state.members).filter(([, g]) => g !== id));
  return { groups: state.groups.filter((g) => g.id !== id), members };
}

/** Puts a project in a folder, or in none with `null`. */
export function moveProject(state: Groups, path: string, id: string | null): Groups {
  const members = { ...state.members };
  if (id && state.groups.some((g) => g.id === id)) members[path] = id;
  else delete members[path];
  return { ...state, members };
}

export function forgetPath(state: Groups, path: string): Groups {
  if (!(path in state.members)) return state;
  return moveProject(state, path, null);
}

export const countIn = (state: Groups, id: string) => Object.values(state.members).filter((g) => g === id).length;

/** Which projects a folder view shows: everything, one folder, or only projects in none. */
export type View = "all" | "none" | string;
export function inView(state: Groups, path: string, view: View): boolean {
  if (view === "all") return true;
  if (view === "none") return !(path in state.members);
  return state.members[path] === view;
}

function newId() {
  return Math.random().toString(36).slice(2, 10);
}
