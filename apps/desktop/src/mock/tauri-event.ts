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
  // Other tabs of the preview (the presenter window) hear it too.
  try {
    new BroadcastChannel("lemmata-mock-events").postMessage({ name, payload });
  } catch {
    /* no BroadcastChannel: single tab only */
  }
}

export const emit = async (name: string, payload?: unknown) => mockEmit(name, payload);

try {
  new BroadcastChannel("lemmata-mock-events").onmessage = (e) => handlers.get(e.data.name)?.forEach((h) => h({ payload: e.data.payload }));
} catch {
  /* single tab only */
}

// Lets a person (or a test) in the browser console fire an event, e.g.
// __mockEmit("files-changed", { changed: ["main.tex"], removed: [] }).
(window as unknown as { __mockEmit: typeof mockEmit }).__mockEmit = mockEmit;
