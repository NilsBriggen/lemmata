// Review comments: threads anchored to a quoted piece of text in a file,
// kept in .lemmata/comments.json inside the project.

export interface Reply {
  body: string;
  created: number;
}

export interface Thread {
  id: string;
  file: string;
  /** 1-based line where the quote started when last seen; used to pick the nearest match. */
  line: number;
  /** The commented text. Empty means the whole line. */
  quote: string;
  body: string;
  created: number;
  resolved: boolean;
  replies: Reply[];
}

export const COMMENTS_FILE = ".lemmata/comments.json";

export function parse(json: string): Thread[] {
  try {
    const data = JSON.parse(json);
    const list = Array.isArray(data) ? data : data?.threads;
    if (!Array.isArray(list)) return [];
    return list
      .filter((t) => t && typeof t.id === "string" && typeof t.file === "string" && typeof t.body === "string")
      .map((t) => ({
        id: t.id,
        file: t.file,
        line: Number.isInteger(t.line) && t.line > 0 ? t.line : 1,
        quote: typeof t.quote === "string" ? t.quote : "",
        body: t.body,
        created: typeof t.created === "number" ? t.created : 0,
        resolved: t.resolved === true,
        replies: Array.isArray(t.replies) ? t.replies.filter((r: Reply) => r && typeof r.body === "string") : [],
      }));
  } catch {
    return [];
  }
}

export function serialize(threads: Thread[]): string {
  return JSON.stringify({ version: 1, threads }, null, 2) + "\n";
}

export function newId(now = Date.now()): string {
  return `${now.toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

/** Offsets of each line start in `text`. */
function lineStarts(text: string): number[] {
  const starts = [0];
  for (let i = 0; i < text.length; i++) if (text.charCodeAt(i) === 10) starts.push(i + 1);
  return starts;
}

/**
 * Where a thread's quote is in `text` now: the occurrence nearest to its last
 * known line. A thread whose quote is gone falls back to its line, if the file
 * still has that many lines. Returns character offsets and the current line.
 */
export function locate(text: string, t: Pick<Thread, "line" | "quote">): { from: number; to: number; line: number } | null {
  const starts = lineStarts(text);
  const lineOf = (pos: number) => {
    let lo = 0;
    let hi = starts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (starts[mid] <= pos) lo = mid;
      else hi = mid - 1;
    }
    return lo + 1;
  };
  if (t.quote) {
    let best: number | null = null;
    for (let at = text.indexOf(t.quote); at !== -1; at = text.indexOf(t.quote, at + 1)) {
      if (best === null || Math.abs(lineOf(at) - t.line) < Math.abs(lineOf(best) - t.line)) best = at;
    }
    if (best !== null) return { from: best, to: best + t.quote.length, line: lineOf(best) };
  }
  if (t.line > starts.length) return null;
  const from = starts[t.line - 1];
  const end = t.line < starts.length ? starts[t.line] - 1 : text.length;
  return { from, to: Math.max(from, end), line: t.line };
}
