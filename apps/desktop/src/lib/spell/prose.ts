// Finds prose words in LaTeX source: no commands, technical arguments,
// comments or maths. Mirrors crates/lemmata-project/src/prose.rs, which
// does the same for word counts; keep the two in step.

const MATH_ENVS = new Set([
  "equation", "equation*", "align", "align*", "alignat", "alignat*", "gather", "gather*",
  "multline", "multline*", "flalign", "flalign*", "eqnarray", "eqnarray*", "displaymath", "math",
]);
const VERBATIM_ENVS = new Set(["verbatim", "verbatim*", "lstlisting", "minted", "comment", "tikzpicture"]);
const NON_PROSE_ARGS = new Set([
  "label", "ref", "eqref", "cref", "Cref", "autoref", "pageref", "nameref", "cite", "citep", "citet",
  "parencite", "textcite", "autocite", "footcite", "citeauthor", "citeyear", "nocite", "input", "include",
  "subfile", "usepackage", "RequirePackage", "documentclass", "begin", "end", "includegraphics", "url",
  "href", "bibliography", "addbibresource", "bibliographystyle", "newcommand", "renewcommand",
  "providecommand", "DeclareMathOperator", "newenvironment", "renewenvironment", "setlength",
  "addtolength", "hspace", "vspace", "graphicspath", "color", "textcolor", "definecolor", "colorbox",
  "hypersetup", "pagestyle", "thispagestyle", "setcounter", "addtocounter", "usetheme", "usecolortheme",
  "setmainfont", "setsansfont", "setmonofont", "geometry", "newpage", "clearpage", "printbibliography",
  "bibitem", "lstinline", "verb", "texttt",
]);

export interface ProseState {
  env: string | null;
  display: string | null;
}

export interface Word {
  from: number;
  to: number;
  text: string;
}

const LETTER = /\p{L}/u;

function braced(s: string, i: number): { body: string; len: number } | null {
  if (s[i] !== "{") return null;
  let depth = 1;
  for (let p = i + 1; p < s.length; p++) {
    if (s[p] === "{") depth++;
    else if (s[p] === "}" && --depth === 0) return { body: s.slice(i + 1, p), len: p - i + 1 };
  }
  return null;
}

function skipWs(s: string, i: number) {
  while (i < s.length && (s[i] === " " || s[i] === "\t")) i++;
  return i;
}

function stripComment(line: string) {
  for (let i = 0; i < line.length; i++) {
    if (line[i] === "%" && (i === 0 || line[i - 1] !== "\\")) return line.slice(0, i);
  }
  return line;
}

/** Prose words in one line; `offset` is added to positions. Updates `state`. */
export function lineWords(line: string, state: ProseState, offset = 0): Word[] {
  const code = stripComment(line);
  const out: Word[] = [];
  let i = 0;
  while (i < code.length) {
    if (state.env) {
      const end = `\\end{${state.env}}`;
      const p = code.indexOf(end, i);
      if (p < 0) return out;
      i = p + end.length;
      state.env = null;
      continue;
    }
    if (state.display) {
      const p = code.indexOf(state.display, i);
      if (p < 0) return out;
      i = p + state.display.length;
      state.display = null;
      continue;
    }
    const c = code[i];
    if (c === "$") {
      if (code.startsWith("$$", i)) {
        state.display = "$$";
        i += 2;
      } else {
        const p = code.indexOf("$", i + 1);
        i = p < 0 ? code.length : p + 1;
      }
    } else if (c === "\\") {
      const rest = code.slice(i + 1);
      if (rest.startsWith("[")) {
        state.display = "\\]";
        i += 2;
        continue;
      }
      if (rest.startsWith("(")) {
        const p = code.indexOf("\\)", i + 2);
        i = p < 0 ? code.length : p + 2;
        continue;
      }
      const name = /^[A-Za-z]*/.exec(rest)![0];
      if (!name) {
        i += 2;
        continue;
      }
      i += 1 + name.length;
      if (name === "begin") {
        const b = braced(code, i);
        if (b) {
          i += b.len;
          if (MATH_ENVS.has(b.body) || VERBATIM_ENVS.has(b.body)) state.env = b.body;
        }
        continue;
      }
      if (NON_PROSE_ARGS.has(name)) {
        if (code[i] === "*") i++;
        for (let k = 0; k < 2; k++) {
          let s = skipWs(code, i);
          if (code[s] === "[") {
            const p = code.indexOf("]", s);
            i = p < 0 ? code.length : p + 1;
          }
          s = skipWs(code, i);
          const b = braced(code, s);
          if (!b) break;
          i = s + b.len;
        }
      }
    } else if (LETTER.test(c)) {
      let end = i;
      while (end < code.length && (LETTER.test(code[end]) || ((code[end] === "'" || code[end] === "-") && end > i))) end++;
      let text = code.slice(i, end);
      text = text.replace(/['-]+$/, "");
      if (text) out.push({ from: offset + i, to: offset + i + text.length, text });
      i = end;
    } else {
      i++;
    }
  }
  return out;
}
