// One completion source for LaTeX that knows the project: environments,
// commands, labels, citation keys, file paths and packages. Anything it
// doesn't handle goes to texlab.
import { snippet, type Completion, type CompletionContext, type CompletionResult, type CompletionSource } from "@codemirror/autocomplete";
import { ENVIRONMENTS, COMMANDS, type Env } from "./data";
import { tokens } from "./environments";
import { inMath } from "./mathContext";
import { mathCompletions, mathLabels } from "./mathCommands";
import type { BibEntry, LabelInfo } from "../api";

/** What the editor can look up about the open project. */
export interface Providers {
  labels(): Promise<LabelInfo[]>;
  cites(): Promise<BibEntry[]>;
  /** Project-relative file paths. */
  files(): string[];
  packages(): Promise<string[]>;
  /** An environment that needs this package was inserted: offer to load it if the document doesn't. */
  needsPackage?(pkg: string): void;
}

const REF = /\\[a-zA-Z]*ref[a-zA-Z]*\*?\{([^{}\n]*)$/;
const CITE = /\\[a-zA-Z]*cite[a-zA-Z]*\*?(?:\[[^\]\n]*\]){0,2}\{([^{}\n]*)$/;
const FILE = /\\(input|include|subfile|includegraphics|bibliography|addbibresource|includepdf)\*?(?:\[[^\]\n]*\])?\{([^{}\n]*)$/;
const PKG = /\\(?:usepackage|RequirePackage)(?:\[[^\]\n]*\])?\{([^{}\n]*)$/;
const ENV = /\\(begin|end)\{([A-Za-z*]*)$/;
const CMD = /\\[A-Za-z]*$/;

const IMAGES = /\.(png|jpe?g|pdf|svg|eps|gif|webp)$/i;

/** Text after the last "," or "{" in a braced list, and where it starts. */
function listItem(before: string, inner: string, pos: number) {
  const word = inner.slice(inner.lastIndexOf(",") + 1);
  const lead = word.length - word.trimStart().length;
  return { from: pos - word.length + lead, typed: word.trimStart() };
}

/** The snippet after `\begin{`: the name, any arguments, the body (indented one level) and the \end. */
export function envSnippet(e: Pick<Env, "name" | "body" | "args">): string {
  const body = e.body.split("\n").map((l) => `\t${l}`).join("\n");
  return `${e.name}}${e.args ?? ""}\n${body}\n\\end{${e.name}}`;
}

/** The environments most documents use come first in the list. */
const COMMON = new Set(["itemize", "enumerate", "figure", "table", "equation", "align", "align*", "tabular", "theorem", "proof", "abstract"]);

/** Runs `apply`, then asks the app whether the document loads `pkg` (an inserted table needs booktabs). */
export function withPackage(apply: Completion["apply"], pkg: string | undefined, getProviders: () => Providers | null): Completion["apply"] {
  if (!pkg || typeof apply !== "function") return apply;
  return (view, completion, from, to) => {
    apply(view, completion, from, to);
    getProviders()?.needsPackage?.(pkg);
  };
}

/** Overleaf-style: typing \beg offers every environment as a whole \begin{…}…\end{…} block. */
function beginBlocks(getProviders: () => Providers | null): Completion[] {
  return ENVIRONMENTS.map((e) => ({
    label: `\\begin{${e.name}}`,
    detail: e.detail,
    type: "class",
    boost: COMMON.has(e.name) ? 16 : 12,
    apply: withPackage(snippet(`\\begin{${envSnippet(e)}`), e.pkg, getProviders),
  }));
}

/** `\end{name}` for the innermost environment that is still open before `pos`. */
function endForOpenEnvironment(state: CompletionContext["state"], pos: number): Completion[] {
  const open = tokens(state.doc).filter((t) => t.kind === "begin" && t.partner == null && t.to <= pos);
  const last = open.at(-1);
  if (!last) return [];
  const name = state.doc.sliceString(last.from, last.to);
  return [{ label: `\\end{${name}}`, detail: "close the open environment", type: "class", boost: 30, apply: `\\end{${name}}` }];
}

export function latexCompletions(
  getProviders: () => Providers | null,
  fallback: (ctx: CompletionContext) => Promise<CompletionResult | null> | CompletionResult | null,
): CompletionSource {
  return async (ctx) => {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const before = line.text.slice(0, ctx.pos - line.from);
    const after = line.text.slice(ctx.pos - line.from);
    const p = getProviders();

    let m = ENV.exec(before);
    if (m) {
      const [, kind, typed] = m;
      const from = ctx.pos - typed.length;
      const closing = after.startsWith("}");
      // Renaming an environment that already has its \end: only the name changes.
      const paired = kind === "begin" && tokens(ctx.state.doc).some((t) => t.kind === "begin" && t.partner != null && ctx.pos >= t.from && ctx.pos <= t.to);
      const to = closing ? ctx.pos + 1 : ctx.pos;
      const options: Completion[] = ENVIRONMENTS.map((e) => ({
        label: e.name,
        detail: e.detail,
        type: "class",
        boost: 10,
        apply:
          kind === "end" || paired
            ? `${e.name}}`
            : withPackage(snippet(envSnippet(e)), e.pkg, getProviders),
      }));
      // Also offer names already used in this document.
      const used = new Set(ENVIRONMENTS.map((e) => e.name));
      for (const x of ctx.state.doc.toString().matchAll(/\\begin\{([A-Za-z*]+)\}/g)) {
        if (!used.has(x[1])) {
          used.add(x[1]);
          options.push({ label: x[1], type: "class", detail: "used here", apply: kind === "end" || paired ? `${x[1]}}` : snippet(`${x[1]}}\n\t\${}\n\\end{${x[1]}}`) });
        }
      }
      return { from, to, options, filter: true, validFor: /^[A-Za-z*]*$/ };
    }

    m = REF.exec(before);
    if (m && p) {
      const { from, typed } = listItem(before, m[1], ctx.pos);
      const labels = await p.labels();
      return {
        from,
        options: labels.map((l) => ({ label: l.label, detail: `${l.file}:${l.line}`, info: l.context || undefined, type: "constant" })),
        validFor: /^[^,{}\s]*$/,
        filter: typed.length > 0 || ctx.explicit,
      };
    }

    m = CITE.exec(before);
    if (m && p) {
      const { from } = listItem(before, m[1], ctx.pos);
      const entries = await p.cites();
      return {
        from,
        options: entries.map((e) => ({
          label: e.key,
          detail: [e.authors.split(" and ")[0], e.year].filter(Boolean).join(" "),
          info: e.title || undefined,
          type: "text",
        })),
        validFor: /^[^,{}\s]*$/,
      };
    }

    m = FILE.exec(before);
    if (m && p) {
      const cmd = m[1];
      const from = ctx.pos - m[2].length;
      const files = p.files().filter((f) => {
        if (cmd === "includegraphics") return IMAGES.test(f);
        if (cmd === "bibliography" || cmd === "addbibresource") return f.endsWith(".bib");
        if (cmd === "includepdf") return f.endsWith(".pdf");
        return f.endsWith(".tex");
      });
      const strip = cmd === "input" || cmd === "include" || cmd === "subfile" ? /\.tex$/ : cmd === "bibliography" ? /\.bib$/ : null;
      return {
        from,
        options: files.map((f) => ({ label: strip ? f.replace(strip, "") : f, type: "file" })),
        validFor: /^[^{}\s]*$/,
      };
    }

    m = PKG.exec(before);
    if (m && p) {
      const { from } = listItem(before, m[1], ctx.pos);
      const names = await p.packages();
      return { from, options: names.map((n) => ({ label: n, type: "namespace" })), validFor: /^[\w-]*$/ };
    }

    const cmd = CMD.exec(before);
    if (cmd && (cmd[0].length > 1 || ctx.explicit)) {
      const from = ctx.pos - cmd[0].length;
      // In maths, symbols and maths structures come first; in prose, text commands do.
      const math = inMath(ctx.state.doc, ctx.pos);
      const mathItems = mathCompletions(math ? 20 : -40);
      const text: Completion[] = COMMANDS.filter((x) => !mathLabels.has(x.label)).map((x) => ({
        label: x.label,
        detail: x.detail,
        type: "function",
        boost: math ? -10 : 5,
        apply: snippet(x.apply),
      }));
      const ours = [...mathItems, ...text, ...beginBlocks(getProviders), ...endForOpenEnvironment(ctx.state, ctx.pos)];
      let merged = ours;
      try {
        const theirs = await fallback(ctx);
        if (theirs) {
          const seen = new Set(ours.map((o) => o.label));
          merged = ours.concat(theirs.options.filter((o) => !seen.has(o.label)));
        }
      } catch {
        /* texlab not running; ours are enough */
      }
      return { from, options: merged, validFor: /^\\[A-Za-z]*$/ };
    }

    try {
      return await fallback(ctx);
    } catch {
      return null;
    }
  };
}
