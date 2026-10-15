// Hover cards for \ref, \cite, \includegraphics and \input: what the label,
// key or file points at, without leaving the line.
import { hoverTooltip, EditorView, type Tooltip } from "@codemirror/view";
import type { Extension } from "@codemirror/state";
import type { Providers } from "./completions";
import type { BibEntry, LabelInfo } from "../api";
import { loadKatex, renderTexResult } from "../writing/katexRender";

/** What the hover cards need on top of the completion providers. */
export interface HoverProviders extends Providers {
  /** Bytes of a project file (images). */
  readBinary?(path: string): Promise<Uint8Array>;
  /** Text of a project file (\input'd .tex). */
  readText?(path: string): Promise<string>;
}

/** `extras::labels` fields beyond `LabelInfo`; optional so an older backend still works. */
type HoverLabel = LabelInfo & { env?: string; snippet?: string; graphic?: string };
/** `doi` and `venue` are shown when the bib parser supplies them. */
type HoverEntry = BibEntry & { doi?: string; venue?: string };

export type Target =
  | { kind: "ref" | "cite"; name: string; from: number; to: number }
  | { kind: "graphic" | "input"; name: string; from: number; to: number };

const REF = /^(?:[A-Za-z]*ref|eqref|cref|Cref|autoref|pageref|vref|Vref|nameref|Nameref)$/;
const CITE = /^[A-Za-z]*cite[A-Za-z]*$/;
const INPUT = /^(?:input|include|subfile)$/;

/** Index just after the group opened at `i` (`open` … `close`, braces nested), or -1. */
function skipGroup(s: string, i: number, open: string, close: string): number {
  let depth = 0;
  for (let p = i; p < s.length; p++) {
    const c = s[p];
    if (c === "\\") p++;
    else if (c === "{" && open === "{") depth++;
    else if (c === "}" && open === "{") {
      if (--depth === 0) return p + 1;
    } else if (open === "[") {
      if (c === "{") depth++;
      else if (c === "}") depth--;
      else if (c === close && depth <= 0) return p + 1;
    }
  }
  return -1;
}

/** Where an unescaped `%` starts a comment, or the line length. */
function commentStart(line: string): number {
  for (let i = 0; i < line.length; i++) {
    if (line[i] === "\\") i++;
    else if (line[i] === "%") return i;
  }
  return line.length;
}

/**
 * The reference, citation key, image or input file at `offset` in one line of
 * LaTeX. In a list (`\cite{a,b}`) it is the key under `offset`; on the command
 * itself, the only key.
 */
export function targetAt(line: string, offset: number): Target | null {
  const end = commentStart(line);
  if (offset > end) return null;
  const text = line.slice(0, end);
  for (const m of text.matchAll(/\\([A-Za-z]+)\*?/g)) {
    const cmd = m[1];
    const kind = REF.test(cmd) && cmd !== "hyperref" ? "ref" : CITE.test(cmd) ? "cite" : cmd === "includegraphics" ? "graphic" : INPUT.test(cmd) ? "input" : null;
    if (!kind) continue;
    const start = m.index;
    if (offset < start) break;
    let i = start + m[0].length;
    for (;;) {
      const ws = /^\s*/.exec(text.slice(i))![0].length;
      if (text[i + ws] !== "[") { i += ws; break; }
      const next = skipGroup(text, i + ws, "[", "]");
      if (next < 0) return null;
      i = next;
    }
    if (text[i] !== "{") continue;
    const close = skipGroup(text, i, "{", "}");
    if (close < 0) continue;
    if (offset > close) continue;
    const inner = text.slice(i + 1, close - 1);
    if (kind === "graphic" || kind === "input") {
      const name = inner.trim();
      return name ? { kind, name, from: start, to: close } : null;
    }
    // Split at top-level commas, remembering where each item sits.
    const items: { name: string; from: number; to: number }[] = [];
    let depth = 0, from = 0;
    for (let p = 0; p <= inner.length; p++) {
      const c = inner[p];
      if (c === "{") depth++;
      else if (c === "}") depth--;
      if (p === inner.length || (c === "," && depth === 0)) {
        const raw = inner.slice(from, p);
        const name = raw.trim();
        if (name) {
          const lead = raw.length - raw.trimStart().length;
          items.push({ name, from: i + 1 + from + lead, to: i + 1 + from + lead + name.length });
        }
        from = p + 1;
      }
    }
    const hit = items.find((it) => offset >= it.from && offset <= it.to) ?? (offset < i + 1 && items.length === 1 ? items[0] : null);
    return hit ? { kind, ...hit } : null;
  }
  return null;
}

const IMAGE_EXT = /\.(png|jpe?g|gif|webp|svg)$/i;
const MIME: Record<string, string> = { png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", webp: "image/webp", svg: "image/svg+xml" };

/** The project file a `\includegraphics`/`\input` path names: as written, with a default extension, or as a suffix (relative to another folder). */
export function resolveFile(path: string, files: string[], exts: string[]): string | null {
  const p = path.replace(/^\.\//, "");
  for (const e of exts) {
    const want = p + e;
    const hit = files.find((f) => f === want) ?? files.find((f) => f.endsWith("/" + want));
    if (hit) return hit;
  }
  return null;
}

const GRAPHIC_EXTS = ["", ".png", ".jpg", ".jpeg", ".svg", ".pdf", ".gif", ".webp"];

const MATH_ENV = /^(equation|displaymath|math|align|alignat|flalign|gather|multline|eqnarray)\*?$/;
const FIGURE_ENV = /^(figure|subfigure|wrapfigure|sidewaysfigure)\*?$/;
const TABLE_ENV = /^(table|subtable|sidewaystable)\*?$/;
const HEADING = /^((sub)*section|chapter|part|paragraph)$/;

/** What KaTeX should render for an equation environment's body. */
export function mathSource(env: string, body: string): string {
  const base = env.replace(/\*$/, "");
  let tex = body.replace(/\\(?:label\{[^}]*\}|nonumber|notag)/g, "").trim();
  if (base === "alignat") tex = tex.replace(/^\{\d+\}/, "");
  if (/^(align|alignat|flalign|eqnarray)$/.test(base)) return `\\begin{aligned}${tex}\\end{aligned}`;
  if (/^(gather|multline)$/.test(base)) return `\\begin{gathered}${tex}\\end{gathered}`;
  return tex;
}

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

interface Card {
  dom: HTMLElement;
  /** Starts the slow part (reading an image) once the tooltip is actually shown. */
  start?: (alive: () => boolean, own: (url: string) => void) => void;
}

function note(text: string): Card {
  return { dom: el("div", "cm-hover-card cm-hover-note", text) };
}

function image(card: HTMLElement, path: string, p: HoverProviders): Card["start"] {
  const read = p.readBinary;
  const ext = path.split(".").pop()!.toLowerCase();
  if (!read || !IMAGE_EXT.test(path)) return undefined;
  return (alive, own) => {
    read(path).then(
      (bytes) => {
        if (!alive()) return;
        const url = URL.createObjectURL(new Blob([bytes.buffer as ArrayBuffer], { type: MIME[ext] }));
        own(url);
        const img = el("img", "cm-hover-img");
        img.src = url;
        img.alt = path;
        card.append(img);
      },
      () => {},
    );
  };
}

function fileLine(path: string) {
  return el("div", "cm-hover-file", path);
}

async function refCard(name: string, p: HoverProviders): Promise<Card> {
  const labels = (await p.labels()) as HoverLabel[];
  const l = labels.find((x) => x.label === name);
  if (!l) return note(`No \\label{${name}} in the project.`);
  const env = l.env ?? "";
  const base = env.replace(/\*$/, "");
  const dom = el("div", "cm-hover-card");
  const kind = MATH_ENV.test(env) ? "Equation" : FIGURE_ENV.test(env) ? "Figure" : TABLE_ENV.test(env) ? "Table" : HEADING.test(env) ? cap(env) : env ? cap(base) : "Label";
  dom.append(el("div", "cm-hover-head", `${kind} · ${l.file}:${l.line}`));
  const snippet = l.snippet ?? "";
  let start: Card["start"];
  if (MATH_ENV.test(env) && snippet) {
    await loadKatex().catch(() => {});
    const { html } = renderTexResult(mathSource(env, snippet), true);
    if (html) {
      const m = el("div", "cm-hover-math");
      m.innerHTML = html;
      dom.append(m);
    } else dom.append(el("pre", "cm-hover-code", snippet));
  } else if (HEADING.test(env)) {
    dom.append(el("div", "cm-hover-title", snippet || l.context));
  } else {
    if (FIGURE_ENV.test(env) && l.graphic) {
      const file = resolveFile(l.graphic, p.files(), GRAPHIC_EXTS);
      if (file && IMAGE_EXT.test(file)) start = image(dom, file, p);
      else dom.append(fileLine(file ?? l.graphic));
    }
    const text = snippet || l.context;
    if (text) dom.append(el("div", "cm-hover-text", text));
  }
  return { dom, start };
}

async function citeCard(key: string, p: HoverProviders): Promise<Card> {
  const e = ((await p.cites()) as HoverEntry[]).find((x) => x.key === key);
  if (!e) return note("Not in any .bib file.");
  const dom = el("div", "cm-hover-card");
  dom.append(el("div", "cm-hover-title", e.title || e.key));
  const who = [e.authors, e.year].filter(Boolean).join(", ");
  if (who) dom.append(el("div", "cm-hover-text", who));
  const where = [e.venue, e.kind].filter(Boolean).join(" · ");
  dom.append(el("div", "cm-hover-head", `${where ? where + " · " : ""}${e.file}`));
  if (e.doi) dom.append(el("div", "cm-hover-file", `doi.org/${e.doi.replace(/^https?:\/\/(dx\.)?doi\.org\//, "")}`));
  return { dom };
}

async function fileCard(t: Target, p: HoverProviders): Promise<Card> {
  const graphic = t.kind === "graphic";
  const file = resolveFile(t.name, p.files(), graphic ? GRAPHIC_EXTS : ["", ".tex"]);
  if (!file) return note(`No file ${t.name} in the project.`);
  const dom = el("div", "cm-hover-card");
  if (graphic) {
    dom.append(fileLine(file));
    return { dom, start: image(dom, file, p) };
  }
  dom.append(fileLine(file));
  const src = await p.readText?.(file).catch(() => null);
  if (src) dom.append(el("pre", "cm-hover-code", src.split("\n").slice(0, 8).join("\n")));
  return { dom };
}

/** Hover cards for references, citations, images and input files. */
export function referenceHovers(getProviders: () => HoverProviders | null): Extension {
  return [
    hoverTooltip(
      async (view, pos): Promise<Tooltip | null> => {
        const line = view.state.doc.lineAt(pos);
        const t = targetAt(line.text, pos - line.from);
        const p = getProviders();
        if (!t || !p) return null;
        const card = await (t.kind === "ref" ? refCard(t.name, p) : t.kind === "cite" ? citeCard(t.name, p) : fileCard(t, p)).catch(() => null);
        if (!card) return null;
        return {
          pos: line.from + t.from,
          end: line.from + t.to,
          create() {
            let dead = false;
            const urls: string[] = [];
            card.start?.(() => !dead, (u) => urls.push(u));
            return {
              dom: card.dom,
              destroy() {
                dead = true;
                urls.forEach((u) => URL.revokeObjectURL(u));
              },
            };
          },
        };
      },
      { hoverTime: 300 },
    ),
    EditorView.baseTheme({
      ".cm-hover-card": { padding: "8px 12px", maxWidth: "min(460px, 70vw)", maxHeight: "50vh", overflow: "auto", fontSize: "13px", lineHeight: "1.45", backgroundColor: "var(--surface)", color: "var(--ink)" },
      ".cm-hover-note": { color: "var(--muted)" },
      ".cm-hover-head": { color: "var(--muted)", fontSize: "11px", marginBottom: "4px" },
      ".cm-hover-card > .cm-hover-title + .cm-hover-text + .cm-hover-head": { marginTop: "4px", marginBottom: "0" },
      ".cm-hover-title": { fontWeight: "600" },
      ".cm-hover-text": { color: "var(--ink)" },
      ".cm-hover-file": { fontFamily: "var(--f-mono)", fontSize: "11.5px", color: "var(--muted)" },
      ".cm-hover-code": { margin: "4px 0 0", fontFamily: "var(--f-mono)", fontSize: "11.5px", color: "var(--ink)", whiteSpace: "pre-wrap" },
      ".cm-hover-math": { overflowX: "auto", fontSize: "1.1em" },
      ".cm-hover-math .katex-display": { margin: "0" },
      ".cm-hover-img": { display: "block", maxWidth: "240px", maxHeight: "240px", marginTop: "6px", borderRadius: "4px", border: "1px solid var(--border)", backgroundColor: "#fff" },
    }),
  ];
}
