<script lang="ts">
  // CodeMirror 6 with one EditorState per open file, so each tab keeps its
  // own undo history and selection.
  import { onMount, onDestroy } from "svelte";
  import { EditorState, EditorSelection, Prec, type Extension } from "@codemirror/state";
  import {
    EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter,
    drawSelection, highlightSpecialChars, rectangularSelection, crosshairCursor,
  } from "@codemirror/view";
  import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
  import { StreamLanguage, HighlightStyle, syntaxHighlighting, bracketMatching, indentOnInput, indentUnit } from "@codemirror/language";
  import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
  import { closeBrackets, closeBracketsKeymap, autocompletion, completionKeymap, snippet } from "@codemirror/autocomplete";
  import { setDiagnostics, lintGutter, lintKeymap, nextDiagnostic, previousDiagnostic, type Diagnostic as CmDiagnostic } from "@codemirror/lint";
  import { serverCompletionSource } from "@codemirror/lsp-client";
  import { environmentSync, closeEnvironment, unmatchedEnvironments } from "./editor/environments";
  import { latexCompletions, type Providers } from "./editor/completions";
  import { previews } from "./editor/mathCommands";
  import { renderTex } from "./writing/katexRender";
  import { folding, foldKeymap, foldRange } from "./editor/folding";
  import { snippetCompletions } from "./editor/snippets";
  import { reindent, tabSizeOf, unitOf } from "./editor/indent";
  import { symbolAt } from "./writing/symbolAt";
  import { foldEffect, foldedRanges, unfoldEffect } from "@codemirror/language";
  import { formattingKeymap, toggleWrap } from "./editor/shortcuts";
  import { stex } from "@codemirror/legacy-modes/mode/stex";
  import { tags as t } from "@lezer/highlight";
  import type { Diagnostic } from "./api";
  import { spellcheck } from "./spell/extension";
  import { vim } from "@replit/codemirror-vim";
  import { settings } from "./settings.svelte";
  import { mathPreview } from "./writing/math";
  import { figureSnippet, tsvToTabular } from "./writing/table";

  interface Props {
    onChange: (path: string, text: string) => void;
    onSyncRequest: (path: string, line: number, column: number) => void;
    onCursor?: (path: string, line: number) => void;
    onAddWord?: (word: string) => void;
    /** Saves a pasted image into the project; returns its path. */
    onPasteImage?: (png: Uint8Array) => Promise<string | null>;
    /** A snippet just inserted needs this package. */
    onNeedsPackage?: (pkg: string) => void;
    /** Shown after a paste was turned into a table, with a way back. */
    onPastedTable?: (undo: () => void) => void;
    /** Ctrl+Shift+E: ask which environment to wrap the selection in. */
    onWrapEnvironment?: () => void;
    /** Selection changed; the text is empty when nothing is selected. */
    onSelection?: (text: string) => void;
    /** A section or environment was folded or unfolded. */
    onFoldsChange?: () => void;
  }
  let { onChange, onSyncRequest, onCursor, onAddWord, onPasteImage, onNeedsPackage, onPastedTable, onWrapEnvironment, onSelection, onFoldsChange }: Props = $props();

  /** What completions can look up about the project; set by the app. */
  let providers: Providers | null = null;
  export function setProviders(p: Providers | null) {
    providers = p;
  }

  let host: HTMLDivElement;
  let view: EditorView | undefined;
  let currentPath: string | null = null;
  const states = new Map<string, EditorState>();
  /** Language-server extensions per file, set once texlab is running. */
  let lspFor: ((path: string) => Extension) | null = null;

  const texHighlight = HighlightStyle.define([
    { tag: [t.tagName, t.keyword], color: "var(--accent)" },
    { tag: t.comment, color: "var(--muted)", fontStyle: "italic" },
    { tag: [t.atom, t.string, t.special(t.variableName)], color: "var(--atom)" },
    { tag: t.bracket, color: "var(--muted)" },
    { tag: t.number, color: "var(--ok)" },
    { tag: t.invalid, color: "var(--error)" },
  ]);

  const theme = EditorView.theme({
    "&": { height: "100%", backgroundColor: "var(--code-bg)", color: "var(--ink)", fontSize: "var(--editor-font-size)" },
    ".cm-scroller": { fontFamily: "var(--editor-font, var(--f-mono))", lineHeight: "var(--editor-line-height, 1.65)" },
    ".cm-foldGutter .cm-gutterElement": { cursor: "pointer", padding: "0 3px", color: "var(--faint)", fontSize: "12px" },
    ".cm-foldGutter .cm-gutterElement:hover": { color: "var(--ink)" },
    ".cm-foldPlaceholder": { backgroundColor: "var(--accent-soft)", color: "var(--accent)", border: "none", borderRadius: "4px", padding: "0 6px" },
    ".cm-content": { caretColor: "var(--accent)", padding: "12px 0 40vh" },
    ".cm-line": { padding: "0 16px 0 10px" },
    ".cm-gutters": { backgroundColor: "var(--gutter)", color: "var(--faint)", border: "none", borderRight: "1px solid var(--border)" },
    ".cm-lineNumbers .cm-gutterElement": { padding: "0 10px 0 14px", minWidth: "40px" },
    ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--accent) 6%, transparent)" },
    ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--ink-2)" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
      backgroundColor: "color-mix(in srgb, var(--accent) 24%, transparent) !important",
    },
    ".cm-cursor": { borderLeftColor: "var(--accent)", borderLeftWidth: "2px" },
    ".cm-matchingBracket": { backgroundColor: "color-mix(in srgb, var(--accent) 18%, transparent)", outline: "none" },
    ".cm-tooltip": { backgroundColor: "var(--surface)", border: "1px solid var(--border)", color: "var(--ink)", borderRadius: "8px", boxShadow: "var(--shadow)" },
    ".cm-tooltip-autocomplete > ul > li": { padding: "3px 10px !important", fontFamily: "var(--f-mono)" },
    ".cm-math-glyph": { marginLeft: "12px", color: "var(--ink)", fontSize: "0.95em", pointerEvents: "none" },
    ".cm-math-glyph .katex": { fontSize: "1em" },
    ".cm-completionInfo": { padding: "8px 12px", borderRadius: "var(--r)" },
    ".cm-math-info": { display: "flex", flexDirection: "column", gap: "6px", alignItems: "flex-start", minWidth: "90px" },
    ".cm-math-info .katex-display": { margin: "0" },
    ".cm-math-info code": { fontSize: "11px", color: "var(--muted)" },
    ".cm-tooltip-autocomplete > ul > li[aria-selected]": { backgroundColor: "var(--accent-soft)", color: "var(--ink)" },
    ".cm-panels": { backgroundColor: "var(--surface-2)", color: "var(--ink)", borderColor: "var(--border)" },
    ".cm-panel.cm-search input, .cm-panel.cm-search button": { fontFamily: "var(--f-body)" },
    ".cm-lintRange-error": { backgroundImage: "none", textDecoration: "underline wavy var(--error)", textUnderlineOffset: "3px" },
    ".cm-lintRange-warning": { backgroundImage: "none", textDecoration: "underline wavy var(--warn)", textUnderlineOffset: "3px" },
    ".cm-lint-marker": { width: "0.8em", height: "0.8em" },
  });


  const pasteHandler = EditorView.domEventHandlers({
    paste(e, v) {
      const image = Array.from(e.clipboardData?.files ?? []).find((f) => f.type === "image/png");
      if (image && onPasteImage) {
        e.preventDefault();
        image.arrayBuffer().then(async (buf) => {
          const path = await onPasteImage(new Uint8Array(buf));
          if (path) insertSnippet(figureSnippet(path), "graphicx");
        });
        return true;
      }
      const text = e.clipboardData?.getData("text/plain") ?? "";
      const generated = tsvToTabular(text);
      if (!generated) return false;
      const table = reindent(generated, unitOf(settings.indent));
      e.preventDefault();
      const from = v.state.selection.main.from;
      v.dispatch(v.state.replaceSelection(table));
      onNeedsPackage?.("booktabs");
      onPastedTable?.(() => {
        if (v.state.sliceDoc(from, from + table.length) === table) {
          v.dispatch({ changes: { from, to: from + table.length, insert: text } });
        }
      });
      return true;
    },
  });

  /** Inserts a snippet at the cursor (or `at`), placing the cursor inside it. */
  export function insertSnippet(snippet: { text: string; cursor: number }, needs?: string, at?: number) {
    if (!view) return;
    const pos = at ?? view.state.selection.main.head;
    const line = view.state.doc.lineAt(pos);
    // Figures go on their own lines.
    const prefix = line.text.slice(0, pos - line.from).trim() ? "\n" : "";
    // Blocks are written with two spaces per level; use the indentation the user chose.
    const unit = unitOf(settings.indent);
    const text = reindent(snippet.text, unit);
    const cursor = reindent(snippet.text.slice(0, snippet.cursor), unit).length;
    const insert = prefix + text + "\n";
    view.dispatch({ changes: { from: pos, insert }, selection: { anchor: pos + prefix.length + cursor } });
    view.focus();
    if (needs) onNeedsPackage?.(needs);
  }

  /** Inserts a snippet template (`${}` tab stops) on its own lines, like a figure or table. */
  export function insertBlock(template: string, needs?: string) {
    if (!view) return;
    const head = view.state.selection.main.head;
    const line = view.state.doc.lineAt(head);
    if (view.state.sliceDoc(line.from, head).trim()) {
      view.dispatch({ changes: { from: head, insert: "\n" }, selection: { anchor: head + 1 } });
    }
    const at = view.state.selection.main.head;
    snippet(reindent(template, unitOf(settings.indent)) + "\n")(view, null, at, at);
    view.focus();
    if (needs) onNeedsPackage?.(needs);
  }

  /** Document position under a window point, for drops. */
  export function posAtPoint(x: number, y: number): number | null {
    return view?.posAtCoords({ x, y }) ?? null;
  }

  const isTex = (path: string) => /\.(tex|sty|cls)$/.test(path);

  function extensions(path: string): Extension[] {
    const tex = isTex(path);
    return [
      indentUnit.of(unitOf(settings.indent)),
      EditorState.tabSize.of(tabSizeOf(settings.indent)),
      // Vim must come first so its keys take precedence.
      settings.keymap === "vim" ? vim() : [],
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      history(),
      drawSelection(),
      EditorState.allowMultipleSelections.of(true),
      rectangularSelection(),
      crosshairCursor(),
      indentOnInput(),
      bracketMatching(),
      settings.autoClose ? closeBrackets() : [],
      highlightActiveLine(),
      highlightSelectionMatches(),
      lintGutter(),
      settings.wordWrap ? EditorView.lineWrapping : [],
      tex ? StreamLanguage.define(stex) : [],
      tex
        ? EditorState.languageData.of(() => [{ commentTokens: { line: "%" }, closeBrackets: { brackets: ["(", "[", "{", "$"] } }])
        : [],
      syntaxHighlighting(texHighlight),
      lspFor && /\.(tex|bib|sty|cls)$/.test(path) ? lspFor(path) : [],
      tex && settings.autocomplete
        ? autocompletion({
            // A small rendered glyph next to every maths item.
            addToOptions: [
              {
                position: 55,
                render(c) {
                  const tex = previews.get(c);
                  const html = tex && renderTex(tex);
                  if (!html) return null;
                  const span = document.createElement("span");
                  span.className = "cm-math-glyph";
                  span.innerHTML = html;
                  return span;
                },
              },
            ],
            override: [
              latexCompletions(
                () => providers,
                async (ctx) => (lspFor ? serverCompletionSource(ctx) : null),
              ),
              snippetCompletions(() => settings.snippets),
            ],
          })
        : [],
      tex && settings.envSync ? [environmentSync, unmatchedEnvironments] : [],
      tex && settings.folding ? folding : [],
      path.endsWith(".tex") ? [spellcheck((w) => onAddWord?.(w)), mathPreview, pasteHandler] : [],
      theme,
      Prec.high(
        keymap.of([
          {
            key: "Mod-j",
            run: (v) => {
              const pos = v.state.selection.main.head;
              const line = v.state.doc.lineAt(pos);
              onSyncRequest(path, line.number, pos - line.from);
              return true;
            },
          },
          ...(tex && settings.envSync ? [{ key: "Enter", run: closeEnvironment }] : []),
          // CodeMirror binds Mod-Enter to "insert blank line"; the app uses it to recompile.
          { key: "Mod-Enter", run: () => true },
        ]),
      ),
      tex
        ? formattingKeymap({
            nextProblem: (dir) => (view ? (dir === 1 ? nextDiagnostic(view) : previousDiagnostic(view)) : false),
            wrapInEnvironment: () => (onWrapEnvironment?.(), true),
          })
        : [],
      keymap.of([
        ...closeBracketsKeymap,
        ...completionKeymap,
        ...foldKeymap,
        ...lintKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...historyKeymap,
        indentWithTab,
      ]),
      EditorView.updateListener.of((u) => {
        if (u.docChanged) onChange(path, u.state.doc.toString());
        if (u.transactions.some((tr) => tr.effects.some((e) => e.is(foldEffect) || e.is(unfoldEffect)))) onFoldsChange?.();
        if (u.selectionSet || u.docChanged) {
          onCursor?.(path, u.state.doc.lineAt(u.state.selection.main.head).number);
          const r = u.state.selection.main;
          onSelection?.(r.empty ? "" : u.state.sliceDoc(r.from, r.to));
        }
      }),
    ];
  }

  /** Inserts a CodeMirror snippet (`${}` marks tab stops) at the selection. */
  export function insertTemplate(template: string) {
    if (!view) return;
    const { from, to } = view.state.selection.main;
    snippet(reindent(template, unitOf(settings.indent)))(view, null, from, to);
    view.focus();
  }

  /** Plain text at the cursor, cursor after it. */
  export function insertText(text: string) {
    if (!view) return;
    view.dispatch(view.state.replaceSelection(text));
    view.focus();
  }

  /** Bold, italic and similar: wraps or unwraps the selection in \\cmd{…}. */
  export function toggleCommand(cmd: string) {
    if (!view) return;
    toggleWrap(cmd)({ state: view.state, dispatch: view.dispatch });
    view.focus();
  }

  /** Surrounds the selection with `open` and `close`, or removes them if already there. */
  export function wrapWith(open: string, close: string) {
    if (!view) return;
    const st = view.state;
    const tr = st.changeByRange((r) => {
      const before = st.sliceDoc(Math.max(0, r.from - open.length), r.from);
      const after = st.sliceDoc(r.to, r.to + close.length);
      if (before === open && after === close) {
        return {
          changes: [{ from: r.from - open.length, to: r.from }, { from: r.to, to: r.to + close.length }],
          range: EditorSelection.range(r.from - open.length, r.to - open.length),
        };
      }
      return {
        changes: [{ from: r.from, insert: open }, { from: r.to, insert: close }],
        range: EditorSelection.range(r.from + open.length, r.to + open.length),
      };
    });
    view.dispatch(st.update(tr, { userEvent: "input", scrollIntoView: true }));
    view.focus();
  }

  /** A heading command on the current line, or a new one if the line is empty. */
  export function heading(cmd: string) {
    if (!view) return;
    const line = view.state.doc.lineAt(view.state.selection.main.head);
    const m = /^(\s*\\)(part|chapter|section|subsection|subsubsection|paragraph)(\*?[\[{])/.exec(line.text);
    if (m) {
      const from = line.from + m[1].length;
      view.dispatch({ changes: { from, to: from + m[2].length, insert: cmd }, userEvent: "input" });
      view.focus();
    } else if (view.state.selection.main.empty) {
      insertTemplate("\\" + cmd + "{${}}");
    } else {
      toggleCommand(cmd);
    }
  }

  /** The selected text, for the figure/link dialogs. */
  export function selectedText(): string {
    return view ? view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to) : "";
  }

  /** Wraps the selection (or an empty line) in \begin{name} … \end{name}. */
  export function wrapInEnvironment(name: string) {
    if (!view) return;
    const { from, to } = view.state.selection.main;
    const first = view.state.doc.lineAt(from);
    const last = view.state.doc.lineAt(to);
    const indent = /^\s*/.exec(first.text)![0];
    const body = view.state.sliceDoc(first.from, last.to);
    const insert = `${indent}\\begin{${name}}\n${body.split("\n").map((l) => (l ? "  " + l : l)).join("\n")}\n${indent}\\end{${name}}`;
    view.dispatch({
      changes: { from: first.from, to: last.to, insert },
      selection: { anchor: first.from + indent.length + `\\begin{${name}}`.length },
      userEvent: "input",
    });
    view.focus();
  }

  export function open(path: string, text: string) {
    if (!view) return;
    if (currentPath) states.set(currentPath, view.state);
    let state = states.get(path);
    if (!state || state.doc.toString() !== text) {
      state = EditorState.create({ doc: text, extensions: extensions(path) });
    }
    currentPath = path;
    view.setState(state);
    const folds = pendingFolds.get(path);
    if (folds) {
      pendingFolds.delete(path);
      applyFolds(view, folds);
    }
    view.focus();
  }

  /** Folds waiting for their file to be opened (restored from the last session). */
  const pendingFolds = new Map<string, number[]>();

  function applyFolds(v: EditorView, lines: number[]) {
    const effects = lines
      .filter((n) => n >= 1 && n <= v.state.doc.lines)
      .flatMap((n) => {
        const line = v.state.doc.line(n);
        const r = foldRange(v.state, line.from, line.to);
        return r ? [foldEffect.of(r)] : [];
      });
    if (effects.length) v.dispatch({ effects });
  }

  /** Line numbers of the folded sections in the open file. */
  export function foldedLines(): number[] {
    if (!view) return [];
    const v = view;
    const out: number[] = [];
    foldedRanges(v.state).between(0, v.state.doc.length, (from) => void out.push(v.state.doc.lineAt(from).number));
    return out;
  }

  /** Folds these lines in `path`, now if it is open, otherwise when it opens. */
  export function setFolds(path: string, lines: number[]) {
    if (view && path === currentPath) applyFolds(view, lines);
    else pendingFolds.set(path, lines);
  }

  /** Use a language server for files opened from now on; drops cached
   *  states so every file picks it up. */
  export function setLanguageServer(fn: ((path: string) => Extension) | null) {
    lspFor = fn;
    states.clear();
    if (view && currentPath) {
      const doc = view.state.doc.toString();
      const sel = view.state.selection;
      view.setState(EditorState.create({ doc, selection: sel, extensions: extensions(currentPath) }));
    }
  }

  /** Rebuilds every open file's editor setup, e.g. after the keymap changes. */
  export function refreshExtensions() {
    setLanguageServer(lspFor);
  }

  /** Inserts citation keys: into the braces when the cursor is inside a
   *  \cite-like command, otherwise as a new \cite{...}. */
  export function insertCitation(keys: string[]) {
    if (!view || !keys.length) return;
    const pos = view.state.selection.main.head;
    const line = view.state.doc.lineAt(pos);
    const before = line.text.slice(0, pos - line.from);
    const open = /\\\w*cite\w*\*?(?:\[[^\]]*\])*\{([^}]*)$/.exec(before);
    let insert: string;
    if (open) {
      const inside = open[1];
      insert = (inside === "" || inside.trimEnd().endsWith(",") ? "" : ",") + keys.join(",");
    } else {
      insert = `\\cite{${keys.join(",")}}`;
    }
    view.dispatch({ changes: { from: pos, insert }, selection: { anchor: pos + insert.length } });
    view.focus();
  }

  export function getView(): EditorView | undefined {
    return view;
  }

  /** The label or citation key under the cursor, if any. */
  export function symbolAtCursor() {
    if (!view) return null;
    const head = view.state.selection.main.head;
    const line = view.state.doc.lineAt(head);
    return symbolAt(line.text, head - line.from, currentPath?.endsWith(".bib") ?? false);
  }

  export function currentLine(): number {
    return view ? view.state.doc.lineAt(view.state.selection.main.head).number : 1;
  }

  export function forget(path: string) {
    states.delete(path);
  }

  export function gotoLine(line: number) {
    if (!view) return;
    const l = view.state.doc.line(Math.max(1, Math.min(line, view.state.doc.lines)));
    view.dispatch({ selection: { anchor: l.from }, effects: EditorView.scrollIntoView(l.from, { y: "center" }) });
    view.focus();
  }

  export function showDiagnostics(diags: Diagnostic[]) {
    if (!view || !currentPath) return;
    const doc = view.state.doc;
    const mine: CmDiagnostic[] = diags
      .filter((d) => d.line != null && d.file != null && sameFile(d.file, currentPath!))
      .map((d) => {
        const l = doc.line(Math.max(1, Math.min(d.line!, doc.lines)));
        return {
          from: l.from,
          to: l.to,
          severity: d.severity === "info" ? "info" : d.severity,
          message: d.hint ? `${d.message}\n${d.hint}` : d.message,
        };
      });
    view.dispatch(setDiagnostics(view.state, mine));
  }

  function sameFile(logPath: string, rel: string) {
    return logPath.replace(/^\.\//, "") === rel;
  }

  onMount(() => {
    view = new EditorView({ parent: host, state: EditorState.create({ doc: "", extensions: [theme] }) });
  });
  onDestroy(() => view?.destroy());
</script>

<div class="editor" bind:this={host}></div>

<style>
  .editor { height: 100%; overflow: hidden; }
  .editor :global(.cm-editor) { height: 100%; }
  .editor :global(.cm-editor.cm-focused) { outline: none; }
</style>
