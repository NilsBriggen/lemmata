// CodeMirror extension: underlines misspelled prose words and offers
// suggestions and "Add to dictionary" on right-click.
import { ViewPlugin, Decoration, EditorView, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { RangeSetBuilder, StateEffect } from "@codemirror/state";
import { lineWords, type Word } from "./prose";
import { spell } from "./service";

const misspelled = Decoration.mark({ class: "cm-misspelled" });
const refresh = StateEffect.define<null>();

/** Acronyms, mixed-case names (LaTeX, iPhone) and single letters aren't checked. */
function checkable(w: string) {
  return w.length > 1 && w !== w.toUpperCase() && !/\p{Lu}/u.test(w.slice(1));
}

function closeMenus() {
  document.querySelectorAll(".lm-spell-menu").forEach((m) => m.remove());
}

export function spellcheck(onAddWord: (word: string) => void) {
  const plugin = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet = Decoration.none;
      words: Word[] = [];
      timer: ReturnType<typeof setTimeout> | undefined;
      unsubscribe: () => void;

      constructor(readonly view: EditorView) {
        this.unsubscribe = spell.onChange(() => this.schedule(0));
        this.schedule(0);
      }

      update(u: ViewUpdate) {
        if (u.docChanged) {
          this.decorations = this.decorations.map(u.changes);
          this.schedule(400);
        }
      }

      schedule(delay: number) {
        clearTimeout(this.timer);
        this.timer = setTimeout(() => this.run(), delay);
      }

      async run() {
        const view = this.view;
        const doc = view.state.doc;
        if (spell.language === "off") {
          this.words = [];
          this.decorations = Decoration.none;
          view.dispatch({ effects: refresh.of(null) });
          return;
        }
        const state = { env: null, display: null };
        const words: Word[] = [];
        for (let n = 1; n <= doc.lines; n++) {
          const line = doc.line(n);
          for (const w of lineWords(line.text, state, line.from)) if (checkable(w.text)) words.push(w);
        }
        await spell.check(words.map((w) => w.text));
        if (view.state.doc !== doc) return; // edited meanwhile; a new run is scheduled
        const b = new RangeSetBuilder<Decoration>();
        for (const w of words) if (spell.status(w.text) === false) b.add(w.from, w.to, misspelled);
        this.words = words;
        this.decorations = b.finish();
        view.dispatch({ effects: refresh.of(null) });
      }

      wordAt(pos: number) {
        return this.words.find((w) => w.from <= pos && pos <= w.to && spell.status(w.text) === false);
      }

      destroy() {
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
    ".cm-misspelled": { textDecoration: "underline wavy var(--mark)", textDecorationSkipInk: "none", textUnderlineOffset: "3px" },
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
