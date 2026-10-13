// One completion source for LaTeX that knows the project: environments,
// commands, labels, citation keys, file paths and packages. Anything it
// doesn't handle goes to texlab.
import { snippet, type Completion, type CompletionContext, type CompletionResult, type CompletionSource } from "@codemirror/autocomplete";
import { ENVIRONMENTS, COMMANDS } from "./data";
import type { BibEntry, LabelInfo } from "../api";

/** What the editor can look up about the open project. */
export interface Providers {
  labels(): Promise<LabelInfo[]>;
  cites(): Promise<BibEntry[]>;
  /** Project-relative file paths. */
  files(): string[];
  packages(): Promise<string[]>;
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
      const to = closing ? ctx.pos + 1 : ctx.pos;
      const options: Completion[] = ENVIRONMENTS.map((e) => ({
        label: e.name,
        detail: e.detail,
        type: "class",
        boost: 10,
        apply:
          kind === "end"
            ? `${e.name}}`
            : snippet(`${e.name}}\n\t${e.body}\n\\end{${e.name}}`),
      }));
      // Also offer names already used in this document.
      const used = new Set(ENVIRONMENTS.map((e) => e.name));
      for (const x of ctx.state.doc.toString().matchAll(/\\begin\{([A-Za-z*]+)\}/g)) {
        if (!used.has(x[1])) {
          used.add(x[1]);
          options.push({ label: x[1], type: "class", detail: "used here", apply: kind === "end" ? `${x[1]}}` : snippet(`${x[1]}}\n\t\${}\n\\end{${x[1]}}`) });
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
      const ours: Completion[] = COMMANDS.map((x) => ({
        label: x.label,
        detail: x.detail,
        type: "function",
        boost: 5,
        apply: snippet(x.apply),
      }));
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
