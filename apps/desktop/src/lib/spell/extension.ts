// CodeMirror extension: underlines misspelled prose words and offers
// suggestions and "Add to dictionary" on right-click.
import { ViewPlugin, Decoration, EditorView, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { StateEffect, type ChangeSet, type Text } from "@codemirror/state";
import { lineWords, type ProseState, type Word } from "./prose";
import { spell } from "./service";

const misspelled = Decoration.mark({ class: "cm-misspelled" });
const refresh = StateEffect.define<null>();
/** Lines scanned beyond the visible ones, so short scrolls don't wait for a check. */
const MARGIN = 50;

/** Acronyms, mixed-case names (LaTeX, iPhone) and single letters aren't checked. */
function checkable(w: string) {
  return w.length > 1 && w !== w.toUpperCase() && !/\p{Lu}/u.test(w.slice(1));
}

/** What the scanner knows about a line: the prose state after it, and whether its marks are current. */
interface LineInfo {
  end: ProseState;
  done: boolean;
  /** Edited and awaiting a rescan; `end` is then what the old last line of the edit ended in. */
  dirty?: true;
}

export interface Plan {
  /** Per-line results to store once the words are checked. */
  lines: Map<number, LineInfo>;
  /** Checkable words in the rescanned lines. */
  words: Word[];
  /** Lines whose marks are replaced, ascending. */
  replace: number[];
}

const START: ProseState = { env: null, display: null };
const same = (a: ProseState, b: ProseState | undefined) => !!b && a.env === b.env && a.display === b.display;

/**
 * Remembers per line (index = line number - 1) the prose state at its end:
 * undefined = never scanned.
 * lineWords carries state across lines, so a rescan continues past the wanted
 * lines until the state matches what the following lines were scanned with.
 */
export class SpellScan {
  info: (LineInfo | undefined)[] = [];

  reset(lines: number) {
    this.info = new Array(lines).fill(undefined);
  }

  /** Keeps line numbers in step with an edit; edited lines need a rescan. */
  edit(changes: ChangeSet, oldDoc: Text, newDoc: Text) {
    const spans: [number, number, number][] = [];
    changes.iterChangedRanges((fa, ta, fb, tb) => {
      const first = oldDoc.lineAt(fa).number;
      spans.push([first, oldDoc.lineAt(ta).number - first + 1, newDoc.lineAt(tb).number - newDoc.lineAt(fb).number + 1]);
    });
    for (let i = spans.length - 1; i >= 0; i--) {
      const [first, oldCount, newCount] = spans[i];
      // Remember the old end state so a rescan can tell whether the lines after the edit are affected.
      const last = this.info[first + oldCount - 2];
      const dirty = last && { end: last.end, done: false, dirty: true as const };
      this.info.splice(first - 1, oldCount, ...new Array(newCount).fill(dirty));
    }
  }

  /** Which lines to scan: edited ones, and unscanned ones inside `visible` (line ranges). Null when none. */
  plan(doc: Text, visible: [number, number][]): Plan | null {
    const info = this.info;
    const n = doc.lines;
    const want = new Uint8Array(n + 2);
    let first = 0;
    let last = 0;
    const mark = (m: number) => {
      want[m] = 1;
      if (!first || m < first) first = m;
      if (m > last) last = m;
    };
    for (let m = 1; m <= n; m++) if (info[m - 1]?.dirty) mark(m);
    for (const [a, b] of visible) for (let m = a; m <= b; m++) if (!info[m - 1]?.done) mark(m);
    if (!first) return null;

    const lines = new Map<number, LineInfo>();
    const words: Word[] = [];
    const replace: number[] = [];
    // Start at the nearest known line before the first wanted one (no edited line precedes it).
    let k = first - 1;
    while (k >= 1 && (!info[k - 1] || info[k - 1]!.dirty)) k--;
    const cur: ProseState = k >= 1 ? { ...info[k - 1]!.end } : { ...START };
    for (let m = k + 1; m <= n; m++) {
      const old = info[m - 1];
      // A line we already know, entered in the same state as before, is unchanged.
      if (!want[m] && old && same(cur, m === 1 ? START : info[m - 2]?.end)) {
        if (m > last) break;
        Object.assign(cur, old.end);
        continue;
      }
      if (m > last && !old) break;
      const wantWords = !!want[m] || !!old?.done;
      const line = doc.line(m);
      const ws = lineWords(line.text, cur, line.from);
      if (wantWords) {
        replace.push(m);
        for (const w of ws) if (checkable(w.text)) words.push(w);
      }
      lines.set(m, { end: { ...cur }, done: wantWords });
    }
    return { lines, words, replace };
  }

  commit(plan: Plan) {
    for (const [m, i] of plan.lines) this.info[m - 1] = i;
  }
}

/** Swaps the marks in the plan's lines for those of its misspelled words. */
export function replaceMarks(set: DecorationSet, doc: Text, plan: Plan, bad: (word: string) => boolean): DecorationSet {
  const spans: { from: number; to: number }[] = [];
  for (const m of plan.replace) {
    const { from, to } = doc.line(m);
    const prev = spans[spans.length - 1];
    if (prev && prev.to + 1 >= from) prev.to = to;
    else spans.push({ from, to });
  }
  const add = plan.words.filter((w) => bad(w.text)).map((w) => misspelled.range(w.from, w.to));
  return set.update({ add, sort: true, filter: (from) => !spans.some((r) => from >= r.from && from <= r.to) });
}

function visibleLines(view: EditorView): [number, number][] {
  const doc = view.state.doc;
  return view.visibleRanges.map((r) => [
    Math.max(1, doc.lineAt(r.from).number - MARGIN),
    Math.min(doc.lines, doc.lineAt(r.to).number + MARGIN),
  ]);
}

function closeMenus() {
  document.querySelectorAll(".lm-spell-menu").forEach((m) => m.remove());
}

export function spellcheck(onAddWord: (word: string) => void) {
  const plugin = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet = Decoration.none;
      scan = new SpellScan();
      epoch = 0;
      dead = false;
      timer: ReturnType<typeof setTimeout> | undefined;
      unsubscribe: () => void;

      constructor(readonly view: EditorView) {
        this.scan.reset(view.state.doc.lines);
        this.unsubscribe = spell.onChange(() => {
          // Language or dictionary changed: every verdict may differ.
          this.scan.reset(this.view.state.doc.lines);
          this.decorations = Decoration.none;
          this.epoch++;
          this.schedule(0);
        });
        this.schedule(0);
      }

      update(u: ViewUpdate) {
        if (u.docChanged) {
          this.decorations = this.decorations.map(u.changes);
          this.scan.edit(u.changes, u.startState.doc, u.state.doc);
          this.schedule(400);
        } else if (u.viewportChanged && this.timer === undefined) this.schedule(50);
      }

      schedule(delay: number) {
        clearTimeout(this.timer);
        this.timer = setTimeout(() => this.run(), delay);
      }

      async run() {
        this.timer = undefined;
        const view = this.view;
        const doc = view.state.doc;
        if (spell.language === "off") {
          this.scan.reset(doc.lines);
          this.decorations = Decoration.none;
          view.dispatch({ effects: refresh.of(null) });
          return;
        }
        const plan = this.scan.plan(doc, visibleLines(view));
        if (!plan) return;
        const epoch = this.epoch;
        await spell.check(plan.words.map((w) => w.text));
        // Edited or reset meanwhile; a new run is scheduled.
        if (this.dead || view.state.doc !== doc || epoch !== this.epoch) return;
        this.scan.commit(plan);
        this.decorations = replaceMarks(this.decorations, doc, plan, (w) => spell.status(w) === false);
        view.dispatch({ effects: refresh.of(null) });
      }

      wordAt(pos: number): Word | undefined {
        let found: Word | undefined;
        this.decorations.between(pos, pos, (from, to) => {
          found = { from, to, text: this.view.state.sliceDoc(from, to) };
          return false;
        });
        return found;
      }

      destroy() {
        this.dead = true;
        clearTimeout(this.timer);
        this.unsubscribe();
        closeMenus();
      }
    },
    {
      decorations: (v) => v.decorations,
      eventHandlers: {
        contextmenu(e, view) {
          const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
          const word = pos == null ? undefined : this.wordAt(pos);
          if (!word) return false;
          e.preventDefault();
          showMenu(view, word, e.clientX, e.clientY, onAddWord);
          return true;
        },
      },
    },
  );

  const theme = EditorView.baseTheme({
    // Longhands: the WebKitGTK in the AppImage (Ubuntu 22.04) drops the shorthand with a style and colour.
    ".cm-misspelled": { textDecorationLine: "underline", textDecorationStyle: "wavy", textDecorationColor: "var(--error)", textDecorationSkipInk: "none", textUnderlineOffset: "3px" },
  });

  return [plugin, theme];
}

function showMenu(view: EditorView, word: Word, x: number, y: number, onAddWord: (w: string) => void) {
  closeMenus();
  const menu = document.createElement("div");
  menu.className = "lm-spell-menu";
  menu.setAttribute("role", "menu");
  Object.assign(menu.style, { left: `${x}px`, top: `${y}px` });
  document.body.appendChild(menu);

  const item = (label: string, action: () => void, cls = "") => {
    const b = document.createElement("button");
    b.textContent = label;
    b.className = cls;
    b.setAttribute("role", "menuitem");
    b.onclick = () => {
      closeMenus();
      action();
      view.focus();
    };
    menu.appendChild(b);
    return b;
  };

  const loading = document.createElement("div");
  loading.className = "muted";
  loading.textContent = "Finding suggestions…";
  menu.appendChild(loading);
  const add = item(`Add “${word.text}” to dictionary`, () => {
    spell.add(word.text);
    onAddWord(word.text);
  }, "add");

  spell.suggest(word.text).then((list) => {
    loading.remove();
    if (!list.length) {
      const none = document.createElement("div");
      none.className = "muted";
      none.textContent = "No suggestions";
      menu.insertBefore(none, add);
    }
    for (const s of list) {
      const b = item(s, () => {
        // The word may have moved if the user typed meanwhile.
        if (view.state.sliceDoc(word.from, word.to) === word.text) {
          view.dispatch({ changes: { from: word.from, to: word.to, insert: s } });
        }
      });
      menu.insertBefore(b, add);
    }
    (menu.querySelector("button") as HTMLButtonElement | null)?.focus();
  });

  const dismiss = (e: Event) => {
    if (e instanceof KeyboardEvent && e.key !== "Escape") return;
    if (e instanceof MouseEvent && menu.contains(e.target as Node)) return;
    closeMenus();
    window.removeEventListener("mousedown", dismiss, true);
    window.removeEventListener("keydown", dismiss, true);
  };
  window.addEventListener("mousedown", dismiss, true);
  window.addEventListener("keydown", dismiss, true);
}
