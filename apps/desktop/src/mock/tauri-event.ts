// In-page event bus standing in for Tauri events.
export type UnlistenFn = () => void;
type Handler = (e: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();

export async function listen<T>(name: string, cb: (e: { payload: T }) => void): Promise<UnlistenFn> {
  const set = handlers.get(name) ?? new Set();
  set.add(cb as Handler);
  handlers.set(name, set);
  return () => set.delete(cb as Handler);
}

export function mockEmit(name: string, payload: unknown) {
  handlers.get(name)?.forEach((h) => h({ payload }));
}

// Lets a person (or a test) in the browser console fire an event, e.g.
// __mockEmit("files-changed", { changed: ["main.tex"], removed: [] }).
(window as unknown as { __mockEmit: typeof mockEmit }).__mockEmit = mockEmit;
