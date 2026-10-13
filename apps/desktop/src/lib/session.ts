// Where you left off, per project: open tabs, the active file, its cursor
// line and the PDF position. Stored in the app's config folder.
import { load, type Store } from "@tauri-apps/plugin-store";

export interface Session {
  tabs: string[];
  active: string | null;
  line: number;
  pdf: { zoom: number; scroll: number } | null;
}

let store: Promise<Store> | null = null;
const getStore = () => (store ??= load("sessions.json", { autoSave: 500, defaults: {} }));

export async function loadSession(root: string): Promise<Session | null> {
  try {
    return (await (await getStore()).get<Session>(root)) ?? null;
  } catch {
    return null;
  }
}

export async function saveSession(root: string, session: Session) {
  try {
    await (await getStore()).set(root, session);
  } catch {
    // Not remembering the session is harmless.
  }
}
