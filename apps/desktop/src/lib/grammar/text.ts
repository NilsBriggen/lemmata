// Turns LaTeX source into plain prose paragraphs for LanguageTool, keeping a
// map from every character of the text back to its position in the source, so
// a finding can be underlined (and fixed) exactly where it is.
import { lineWords, type ProseState } from "../spell/prose";

export interface Paragraph {
  /** Plain text sent to LanguageTool. */
  text: string;
  /** Source offset of each text character; -1 for characters made up here (spaces, placeholders). */
  map: number[];
  /** Source range the paragraph covers. */
  from: number;
  to: number;
}

/** Lines that end a paragraph: structure, not running text. \item and \caption only start one. */
const BREAK_BEFORE = /^\s*\\(?:begin|end|item|part|chapter|(?:sub)*section|(?:sub)*paragraph|caption|title|author|date|maketitle|\\)(?![A-Za-z])/;
const BREAK_AFTER = /^\s*\\(?:begin|end|part|chapter|(?:sub)*section|(?:sub)*paragraph|title|author|date|maketitle)(?![A-Za-z])/;
const PUNCT = /[.,;:!?)\]”’"]/;
const OPEN = /[(\[“‘"]/;

/** What remains of the source between two words: a space, plus punctuation that followed a command, and X for inline maths. */
function gapText(gap: string): string {
  let math = false;
  const g = gap
    .replace(/\$[^$]*\$|\\\([^)]*\\\)/g, () => ((math = true), " "))
    .replace(/\\[A-Za-z]+\*?(?:\[[^\]]*\])*(?:\{[^{}]*\})*/g, " ")
    .replace(/[{}~]/g, " ");
  const punct = [...g].filter((c) => PUNCT.test(c) || OPEN.test(c)).join("");
  // A placeholder keeps "Let $x$ be" a sentence; punctuation after it ("$x$,") stays attached.
  return `${math ? " X" : ""}${punct}${g.includes(" ") || !punct ? " " : ""}`;
}

export function paragraphs(source: string): Paragraph[] {
  const out: Paragraph[] = [];
  const state: ProseState = { env: null, display: null };
  let cur: Paragraph | null = null;
  let offset = 0;

  const flush = () => {
    if (cur && /\p{L}/u.test(cur.text)) {
      cur.text = cur.text.replace(/ +$/, "");
      cur.map.length = cur.text.length;
      out.push(cur);
    }
    cur = null;
  };
  const push = (p: Paragraph, s: string, src: number | null) => {
    for (let i = 0; i < s.length; i++) {
      if (src == null && s[i] === " " && (!p.text || p.text.endsWith(" "))) continue;
      p.text += s[i];
      p.map.push(src == null ? -1 : src + i);
    }
  };

  for (const line of source.split("\n")) {
    const start = offset;
    offset += line.length + 1;
    const mathAtStart = !!(state.env || state.display);
    const words = lineWords(line, state, start);
    const mathAtEnd = !!(state.env || state.display);
    if (!line.trim()) {
      flush();
      continue;
    }
    const breaks = BREAK_BEFORE.test(line);
    if (breaks) flush();
    if (!words.length) {
      if (BREAK_AFTER.test(line)) flush();
      continue;
    }
    const p: Paragraph = (cur ??= { text: "", map: [], from: words[0].from, to: words[0].to });
    push(p, " ", null);

    // Comments are not text: gaps are taken from the line without one.
    const code = line.replace(/(^|[^\\])%.*$/, "$1");
    let at = 0; // position in `code` consumed so far
    words.forEach((w, k) => {
      const ws = w.from - start;
      const we = w.to - start;
      // Gap before the word: skipped in maths, otherwise reduced to spaces and punctuation.
      const gap = code.slice(at, ws);
      if (k === 0 && mathAtStart) push(p, " ", null);
      else if (gap) push(p, gapText(gap), null);
      push(p, w.text, w.from);
      p.to = w.to;
      at = we;
      // Punctuation right behind the word, even past a closing brace: "\emph{word}."
      let q = we;
      while (q < code.length && (code[q] === "}" || PUNCT.test(code[q]))) {
        if (code[q] !== "}") {
          push(p, code[q], start + q);
          p.to = start + q + 1;
        }
        q++;
      }
      at = q;
    });
    const tail = code.slice(at);
    push(p, mathAtEnd || !tail.trim() ? " " : gapText(tail), null);
    if (BREAK_AFTER.test(line)) flush();
  }
  flush();
  return out;
}

export interface SourceRange {
  from: number;
  to: number;
  /** The source is exactly the flagged text, so a replacement can be applied to it. */
  editable: boolean;
}

/** Maps a finding in `para.text` back to the source; null when it covers nothing real. */
export function toSource(para: Paragraph, offset: number, length: number, source: string): SourceRange | null {
  let a = offset;
  let b = Math.min(offset + length, para.text.length) - 1;
  while (a <= b && para.map[a] < 0) a++;
  while (b >= a && para.map[b] < 0) b--;
  if (a > b) return null;
  const from = para.map[a];
  const to = para.map[b] + 1;
  return { from, to, editable: source.slice(from, to) === para.text.slice(offset, offset + length) };
}
