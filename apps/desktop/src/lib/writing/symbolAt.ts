// Which label or citation key is under the cursor? Used to prefill "Rename
// label or citation key". The project-wide scan lives in Rust
// (lemmata-project/symbols.rs); this only looks at one line.
export type SymbolKind = "label" | "cite";

export interface SymbolRef {
  kind: SymbolKind;
  name: string;
}

const LABEL = /\\label\s*\{([^{}]*)\}/g;
const REF = /\\([a-zA-Z]*ref)\*?(?:\[[^\]]*\])?\s*\{([^{}]*)\}/g;
const CITE = /\\[a-zA-Z]*cite[a-zA-Z]*\*?(?:\[[^\]]*\]){0,2}\s*\{([^{}]*)\}/g;
const BIB = /^\s*@[A-Za-z]+\s*[{(]\s*([^,\s{}()]+)\s*,/;

function inComment(line: string, col: number) {
  for (let i = 0; i < Math.min(col, line.length); i++) if (line[i] === "%" && line[i - 1] !== "\\") return true;
  return false;
}

/** The name of the argument group that contains `col`, split at commas. */
function pick(group: string, groupStart: number, col: number): string | null {
  let at = groupStart;
  for (const part of group.split(",")) {
    const lead = part.length - part.trimStart().length;
    const name = part.trim();
    const start = at + lead;
    if (name && col >= start && col <= start + name.length) return name;
    at += part.length + 1;
  }
  return null;
}

export function symbolAt(line: string, col: number, isBib = false): SymbolRef | null {
  if (isBib) {
    const m = BIB.exec(line);
    if (m) {
      const start = line.indexOf(m[1], m[0].length - m[1].length - 1);
      if (col >= start && col <= start + m[1].length) return { kind: "cite", name: m[1] };
    }
    return null;
  }
  if (inComment(line, col)) return null;
  const scan = (re: RegExp, kind: SymbolKind, group: number, skip?: (m: RegExpExecArray) => boolean): SymbolRef | null => {
    re.lastIndex = 0;
    for (let m = re.exec(line); m; m = re.exec(line)) {
      if (skip?.(m)) continue;
      const text = m[group];
      const start = m.index + m[0].lastIndexOf("{") + 1;
      if (col >= start && col <= start + text.length) {
        const name = pick(text, start, col);
        if (name) return { kind, name };
      }
    }
    return null;
  };
  return scan(LABEL, "label", 1) ?? scan(REF, "label", 2, (m) => m[1] === "href" || m[1] === "hyperref") ?? scan(CITE, "cite", 1);
}
