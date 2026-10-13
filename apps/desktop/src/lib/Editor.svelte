<script lang="ts">
  // CodeMirror 6 with one EditorState per open file, so each tab keeps its
  // own undo history and selection.
  import { onMount, onDestroy } from "svelte";
  import { EditorState, type Extension } from "@codemirror/state";
  import {
    EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter,
    drawSelection, highlightSpecialChars,
  } from "@codemirror/view";
  import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
  import { StreamLanguage, HighlightStyle, syntaxHighlighting, bracketMatching, indentOnInput } from "@codemirror/language";
  import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
  import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
  import { setDiagnostics, lintGutter, type Diagnostic as CmDiagnostic } from "@codemirror/lint";
  import { stex } from "@codemirror/legacy-modes/mode/stex";
  import { tags as t } from "@lezer/highlight";
  import type { Diagnostic } from "./api";
  import { spellcheck } from "./spell/extension";
  import { vim } from "@replit/codemirror-vim";
  import { settings } from "./settings.svelte";

  interface Props {
    onChange: (path: string, text: string) => void;
    onSyncRequest: (path: string, line: number, column: number) => void;
    onCursor?: (path: string, line: number) => void;
    onAddWord?: (word: string) => void;
  }
  let { onChange, onSyncRequest, onCursor, onAddWord }: Props = $props();

  let host: HTMLDivElement;
  let view: EditorView | undefined;
  let currentPath: string | null = null;
  const states = new Map<string, EditorState>();
  /** Language-server extensions per file, set once texlab is running. */
  let lspFor: ((path: string) => Extension) | null = null;

  const texHighlight = HighlightStyle.define([
    { tag: [t.tagName, t.keyword], color: "var(--blue)" },
    { tag: t.comment, color: "var(--muted)", fontStyle: "italic" },
    { tag: [t.atom, t.string, t.special(t.variableName)], color: "var(--atom)" },
    { tag: t.bracket, color: "var(--muted)" },
    { tag: t.number, color: "var(--ok)" },
    { tag: t.invalid, color: "var(--mark)" },
  ]);

  const theme = EditorView.theme({
    "&": { height: "100%", backgroundColor: "var(--code-bg)", color: "var(--ink)", fontSize: "var(--editor-font-size)" },
    ".cm-scroller": { fontFamily: "var(--f-mono)", lineHeight: "1.6" },
    ".cm-content": { caretColor: "var(--blue)", padding: "10px 0" },
    ".cm-gutters": { backgroundColor: "var(--code-bg)", color: "var(--muted)", border: "none" },
    ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "var(--blue-soft)" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
      backgroundColor: "color-mix(in srgb, var(--blue) 25%, transparent) !important",
    },
    ".cm-cursor": { borderLeftColor: "var(--blue)", borderLeftWidth: "2px" },
    ".cm-matchingBracket": { backgroundColor: "color-mix(in srgb, var(--blue) 18%, transparent)", outline: "none" },
    ".cm-tooltip": { backgroundColor: "var(--sheet)", border: "1px solid var(--rule)", color: "var(--ink)" },
    ".cm-panels": { backgroundColor: "var(--chrome)", color: "var(--ink)" },
  });

  function extensions(path: string): Extension[] {
    return [
      // Vim must come first so its keys take precedence.
      settings.keymap === "vim" ? vim() : [],
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      history(),
      drawSelection(),
      indentOnInput(),
      bracketMatching(),
      closeBrackets(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      lintGutter(),
      EditorView.lineWrapping,
      path.endsWith(".tex") || path.endsWith(".sty") || path.endsWith(".cls") ? StreamLanguage.define(stex) : [],
      syntaxHighlighting(texHighlight),
      lspFor && /\.(tex|bib|sty|cls)$/.test(path) ? lspFor(path) : [],
      path.endsWith(".tex") ? spellcheck((w) => onAddWord?.(w)) : [],
      theme,
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
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...historyKeymap,
        indentWithTab,
      ]),
      EditorView.updateListener.of((u) => {
        if (u.docChanged) onChange(path, u.state.doc.toString());
        if (u.selectionSet || u.docChanged) {
          onCursor?.(path, u.state.doc.lineAt(u.state.selection.main.head).number);
        }
      }),
    ];
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
    view.focus();
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

  export function getView(): EditorView | undefined {
    return view;
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
