// Test automation, for development and test builds only (never in a release
// build: App.svelte imports this behind a compile-time flag). Tests, the
// e2e runner and the developer MCP server drive the app through
// `window.__lemmata`, using the same paths a person would: DOM clicks and
// keys, typing into CodeMirror, and the command palette's own commands.
import { EditorView } from "@codemirror/view";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api";
import { invoke } from "@tauri-apps/api/core";
import { updateSettings, type Settings } from "./settings.svelte";

/** What App.svelte hands over: read-only views of its state and a few entry points. */
export interface Host {
  state: () => AppState;
  /** Text of every open file that differs from what was loaded or saved. */
  buffers: () => Record<string, string>;
  commands: () => { id: string; label: string; run: () => unknown }[];
  openProject: (path: string) => Promise<void>;
  openFile: (path: string) => Promise<void>;
}

export interface AppState {
  project: string | null;
  main: string | null;
  tabs: string[];
  active: string | null;
  dirty: string[];
  building: boolean;
  status: string | null;
  diagnostics: { severity: string; message: string; file: string | null; line: number | null }[];
  layout: string;
  showPdf: boolean;
  hasPdf: boolean;
  logsOpen: boolean;
  /** An automatic build is waiting for the typing pause before it starts. */
  buildQueued: boolean;
  notice: string | null;
  /** The latest notices, newest first (the list under the bell). */
  notices: string[];
  openError: string | null;
  /** Files changed outside with unsaved edits here, waiting for the user's choice. */
  conflicts: string[];
  dialogs: string[];
}

const errors: string[] = [];

function element(selector: string): HTMLElement {
  const el = document.querySelector<HTMLElement>(selector);
  if (!el) throw new Error(`No element matches ${selector}`);
  return el;
}

function view(): EditorView {
  const dom = document.querySelector<HTMLElement>(".cm-editor");
  const v = dom && EditorView.findFromDOM(dom);
  if (!v) throw new Error("No editor is open.");
  return v;
}

/** "Control+Shift+f" → a keydown on the focused element (or the window). */
function key(combo: string) {
  const parts = combo.split("+");
  const k = parts.pop()!;
  const mods = new Set(parts.map((p) => p.toLowerCase()));
  const init: KeyboardEventInit = {
    key: k,
    bubbles: true,
    cancelable: true,
    ctrlKey: mods.has("control") || mods.has("ctrl"),
    metaKey: mods.has("meta") || mods.has("cmd"),
    shiftKey: mods.has("shift"),
    altKey: mods.has("alt"),
  };
  const target = document.activeElement ?? document.body;
  target.dispatchEvent(new KeyboardEvent("keydown", init));
  target.dispatchEvent(new KeyboardEvent("keyup", init));
}

const settle = (ms = 50) => new Promise((r) => setTimeout(r, ms));

/** Things that must always hold. Returns the ones that don't. */
async function check(host: Host): Promise<string[]> {
  const s = host.state();
  // Each error is reported once.
  const problems = errors.splice(0).map((e) => `uncaught error: ${e}`);
  if (s.showPdf && s.hasPdf && !document.querySelector(".pages .page")) problems.push("a PDF exists but the PDF pane shows no page");
  if (s.active && !s.tabs.includes(s.active)) problems.push(`the active file ${s.active} has no tab`);
  const editable = document.querySelector<HTMLElement>('.cm-content[contenteditable="true"]');
  if (s.project && !s.active && editable && editable.offsetParent !== null) problems.push("no file is open but an editable editor is still shown");
  // CodeMirror's own styles are applied: the line numbers sit beside the text, not above it.
  const gutters = document.querySelector<HTMLElement>(".cm-editor .cm-gutters");
  const content = gutters?.parentElement?.querySelector<HTMLElement>(".cm-content");
  if (gutters && content && gutters.offsetParent !== null && gutters.getBoundingClientRect().right > content.getBoundingClientRect().left + 1)
    problems.push("the editor lost its styles: line numbers are not beside the text");
  // Saved files match the disk.
  const buffers = host.buffers();
  for (const path of s.tabs) {
    if (s.dirty.includes(path) || !(path in buffers)) continue;
    const disk = await api.readText(path).catch(() => null);
    if (disk !== null && disk !== buffers[path]) problems.push(`${path} is marked saved but differs from the disk`);
  }
  return problems;
}

export function install(host: Host) {
  // The first stack lines say where it came from.
  const describe = (err: any, fallback: string) => [err?.message ?? fallback, ...String(err?.stack ?? "").split("\n").slice(1, 4)].join(" | ");
  window.addEventListener("error", (e) => errors.push(describe(e.error, e.message)));
  window.addEventListener("unhandledrejection", (e) => errors.push(describe(e.reason, String(e.reason))));

  const methods: Record<string, (p: any) => unknown> = {
    state: () => host.state(),
    check: () => check(host),
    errors: () => errors.splice(0),
    commands: () => host.commands().map(({ id, label }) => ({ id, label })),
    command: async ({ id }: { id: string }) => {
      const c = host.commands().find((c) => c.id === id);
      if (!c) throw new Error(`No command ${id}`);
      await c.run();
    },
    openProject: ({ path }: { path: string }) => host.openProject(path),
    openFile: ({ path }: { path: string }) => host.openFile(path),
    click: ({ selector }: { selector: string }) => element(selector).click(),
    contextmenu: ({ selector }: { selector: string }) => {
      const r = element(selector).getBoundingClientRect();
      element(selector).dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: r.left + 5, clientY: r.top + 5 }));
    },
    /** Drags the first `from` element (containing `fromText`) onto the first `to` (containing `toText`) with pointer events. */
    drag: async ({ from, to, fromText, toText }: { from: string; to: string; fromText?: string; toText?: string }) => {
      const pick = (selector: string, text?: string) => {
        const el = [...document.querySelectorAll<HTMLElement>(selector)].find((e) => !text || e.textContent?.includes(text));
        if (!el) throw new Error(`No element matches ${selector}${text ? ` with "${text}"` : ""}`);
        return el;
      };
      const centre = (el: HTMLElement) => {
        const r = el.getBoundingClientRect();
        return { clientX: r.left + r.width / 2, clientY: r.top + r.height / 2 };
      };
      const send = (type: string, at: { clientX: number; clientY: number }, target: EventTarget) =>
        target.dispatchEvent(new PointerEvent(type, { bubbles: true, button: 0, pointerId: 1, isPrimary: true, ...at }));
      const source = pick(from, fromText);
      const start = centre(source);
      const end = centre(pick(to, toText));
      send("pointerdown", start, source);
      send("pointermove", { clientX: start.clientX + 10, clientY: start.clientY + 10 }, window);
      send("pointermove", end, window);
      await new Promise((r) => setTimeout(r, 50));
      send("pointerup", end, window);
    },
    /** Clicks the first button (or menu item) whose text contains `text`, optionally inside `within`. */
    clickText: ({ text, within }: { text: string; within?: string }) => {
      const root = within ? element(within) : document;
      const el = [...root.querySelectorAll<HTMLElement>("button, [role=menuitem], [role=tab], a")].find((b) => b.textContent?.includes(text));
      if (!el) throw new Error(`No button with text "${text}"`);
      el.click();
    },
    fill: ({ selector, value }: { selector: string; value: string }) => {
      const el = element(selector) as HTMLInputElement;
      el.value = value;
      el.dispatchEvent(new Event("input", { bubbles: true }));
      // A <select> bound with bind:value listens for change.
      el.dispatchEvent(new Event("change", { bubbles: true }));
    },
    key: ({ combo }: { combo: string }) => key(combo),
    /** Types at the cursor (or replaces the selection) as a user edit. */
    type: ({ text }: { text: string }) => {
      const v = view();
      v.focus();
      v.dispatch(v.state.replaceSelection(text), { userEvent: "input.type", scrollIntoView: true });
    },
    /** Moves the cursor: `line` (1-based) or "start" / "end". */
    cursor: ({ line, at }: { line?: number; at?: "start" | "end" }) => {
      const v = view();
      const pos = at === "end" ? v.state.doc.length : at === "start" ? 0 : v.state.doc.line(Math.min(line ?? 1, v.state.doc.lines)).from;
      v.dispatch({ selection: { anchor: pos } });
    },
    editorText: () => view().state.doc.toString(),
    text: ({ selector }: { selector: string }) => element(selector).textContent ?? "",
    exists: ({ selector }: { selector: string }) => !!document.querySelector(selector),
    /** Up to 50 matching elements, described briefly, for finding selectors. */
    query: ({ selector }: { selector: string }) =>
      [...document.querySelectorAll<HTMLElement>(selector)].slice(0, 50).map((el) => ({
        tag: el.tagName.toLowerCase(),
        class: el.className && typeof el.className === "string" ? el.className : undefined,
        label: el.getAttribute("aria-label") ?? el.getAttribute("title") ?? undefined,
        text: el.textContent?.trim().replace(/\s+/g, " ").slice(0, 80) || undefined,
        value:
          el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement || el instanceof HTMLOptionElement
            ? el.value
            : undefined,
        display: getComputedStyle(el).display,
        rect: (({ x, y, width, height }) => [x, y, width, height].map(Math.round))(el.getBoundingClientRect()),
      })),
    /** Share of non-white pixels in a canvas, in `area` (fractions x0, y0, x1, y1) or all of it: is something drawn there? */
    ink: ({ selector, area = [0, 0, 1, 1] }: { selector: string; area?: number[] }) => {
      const c = element(selector) as HTMLCanvasElement;
      const [x0, y0, x1, y1] = area;
      const w = Math.max(1, Math.round((x1 - x0) * c.width));
      const h = Math.max(1, Math.round((y1 - y0) * c.height));
      const d = c.getContext("2d")!.getImageData(Math.round(x0 * c.width), Math.round(y0 * c.height), w, h).data;
      let n = 0;
      for (let i = 0; i < d.length; i += 4) if (d[i] + d[i + 1] + d[i + 2] < 720) n++;
      return n / (w * h);
    },
    /** Computed CSS properties of every element matching `selector` (what this engine made of the style sheets). */
    style: ({ selector, props }: { selector: string; props: string[] }) =>
      [...document.querySelectorAll(selector)].map((e) => {
        const c = getComputedStyle(e);
        return Object.fromEntries(props.map((p) => [p, c.getPropertyValue(p)]));
      }),
    /** A canvas as a PNG data URL, to look at what was drawn. */
    canvasImage: ({ selector }: { selector: string }) => (element(selector) as HTMLCanvasElement).toDataURL("image/png"),
    /** The page's style sheets: where each comes from, whether it applies, and how many rules the browser kept. */
    sheets: () =>
      [...document.styleSheets].map((sh) => {
        let rules: number | string;
        try {
          rules = sh.cssRules.length;
        } catch (e) {
          rules = String(e);
        }
        return { owner: sh.ownerNode?.nodeName ?? "adopted", href: sh.href, disabled: sh.disabled, media: sh.media.mediaText, rules };
      }),
    /** Waits until no build runs and pending saves have settled. */
    idle: async ({ timeout = 60_000 }: { timeout?: number } = {}) => {
      const end = Date.now() + timeout;
      let s = host.state();
      while (Date.now() < end) {
        s = host.state();
        if (!s.building && !s.buildQueued && s.dirty.filter((d) => !s.conflicts.includes(d)).length === 0) return s;
        await settle(100);
      }
      throw new Error(`The app did not become idle: ${JSON.stringify({ building: s.building, buildQueued: s.buildQueued, dirty: s.dirty, openError: s.openError, notices: s.notices })}`);
    },
    sleep: ({ ms }: { ms: number }) => settle(ms),
    /** Double-clicks inside an element at a point given as fractions of its size (0..1). */
    dblclickAt: ({ selector, x, y }: { selector: string; x: number; y: number }) => {
      const el = element(selector);
      el.scrollIntoView({ block: "start" });
      const r = el.getBoundingClientRect();
      el.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: r.left + r.width * x, clientY: r.top + r.height * y }));
    },
    /** Moves the mouse over the first occurrence of `text` in the editor (for hover tooltips). */
    hoverText: async ({ text }: { text: string }) => {
      const v = view();
      const at = v.state.doc.toString().indexOf(text);
      if (at < 0) throw new Error(`"${text}" is not in the editor`);
      v.dispatch({ effects: EditorView.scrollIntoView(at, { y: "center" }) });
      await settle(150); // coordinates are only right once the scroll is drawn
      const r = v.coordsAtPos(at + Math.floor(text.length / 2));
      if (!r) throw new Error("the text is not on screen");
      const init = { bubbles: true, clientX: (r.left + r.right) / 2, clientY: (r.top + r.bottom) / 2 };
      const target = document.elementFromPoint(init.clientX, init.clientY) ?? v.contentDOM;
      target.dispatchEvent(new MouseEvent("mouseover", init));
      // A resting mouse still sends moves; the hover waits for it to settle.
      for (let i = 0; i < 3; i++) {
        target.dispatchEvent(new MouseEvent("mousemove", init));
        await settle(80);
      }
      return { x: Math.round(init.clientX), y: Math.round(init.clientY), target: target.tagName.toLowerCase(), text: target.textContent?.slice(0, 40) };
    },
    /** Calls a backend command directly, for checking what the app gets. */
    invoke: ({ cmd, args }: { cmd: string; args?: Record<string, unknown> }) => invoke(cmd, args ?? {}),
    /** Changes an app setting, as Preferences would. */
    setting: ({ key, value }: { key: keyof Settings; value: unknown }) => updateSettings({ [key]: value } as Partial<Settings>),
    /** Sets a localStorage value (as tests that reload the page do). */
    storage: ({ key, value }: { key: string; value: string }) => localStorage.setItem(key, value),
    /** Reloads the page; the call returns before the reload. */
    reload: () => void setTimeout(() => location.reload(), 50),
    /** Asks the window to close, as the title bar's close button does. */
    closeWindow: () => void getCurrentWindow().close(),
    /** Whether the main window is full screen (a presentation leaves it so only while it runs). */
    fullscreen: () => getCurrentWindow().isFullscreen(),
  };

  methods.methods = () => Object.keys(methods);

  const call = async (method: string, params: unknown = {}) => {
    const fn = methods[method];
    if (!fn) throw new Error(`Unknown method ${method}. Known: ${Object.keys(methods).join(", ")}`);
    const result = await fn(params ?? {});
    await settle();
    return result ?? null;
  };

  (window as any).__lemmata = { call, methods: Object.keys(methods) };
  return call;
}
