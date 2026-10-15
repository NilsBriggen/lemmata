import { describe, expect, it, vi } from "vitest";
import { EditorState, Text } from "@codemirror/state";
import type { DecorationSet } from "@codemirror/view";
import { Decoration } from "@codemirror/view";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { SpellScan, replaceMarks } from "./extension";

const bad = (w: string) => w.includes("q");
const ranges = (set: DecorationSet) => {
  const out: [number, number][] = [];
  set.between(0, 1e9, (f, t) => void out.push([f, t]));
  return out;
};

/** Scans the lines `visible` asks for, as the plugin does, and applies the marks. */
function run(scan: SpellScan, marks: DecorationSet, doc: Text, visible: [number, number][]) {
  const plan = scan.plan(doc, visible);
  if (!plan) return marks;
  scan.commit(plan);
  return replaceMarks(marks, doc, plan, bad);
}

function full(doc: Text) {
  const scan = new SpellScan();
  scan.reset(doc.lines);
  return ranges(run(scan, Decoration.none, doc, [[1, doc.lines]]));
}

const PIECES = ["qux ", "fine ", "\n", "\\begin{equation}", "\\end{equation}", "$$", "\\[", "\\]", "\\begin{verbatim}", "\\end{verbatim}", "word qq ", "% qc\n", "a"];

describe("incremental spell scan", () => {
  it("matches a full rescan after random edits", () => {
    let seed = 7;
    const rnd = (n: number) => (seed = (seed * 1103515245 + 12345) & 0x7fffffff) % n;
    let state = EditorState.create({ doc: Array.from({ length: 60 }, (_, i) => `line ${i} qux fine ${i % 9 === 0 ? "\\begin{equation} q" : i % 9 === 3 ? "\\end{equation} qz" : "qy"}`).join("\n") });
    const scan = new SpellScan();
    scan.reset(state.doc.lines);
    let marks = run(scan, Decoration.none, state.doc, [[1, state.doc.lines]]);
    for (let i = 0; i < 300; i++) {
      const len = state.doc.length;
      const from = rnd(len + 1);
      const to = Math.min(len, from + (rnd(3) === 0 ? rnd(40) : 0));
      const tr = state.update({ changes: { from, to, insert: PIECES[rnd(PIECES.length)] } });
      scan.edit(tr.changes, state.doc, tr.state.doc);
      marks = marks.map(tr.changes);
      state = tr.state;
      // Sometimes several edits pile up before the scan runs.
      if (rnd(3) === 0) continue;
      marks = run(scan, marks, state.doc, [[1, state.doc.lines]]);
      expect(ranges(marks)).toEqual(full(state.doc));
    }
  });

  it("scans only the viewport first, and the rest once it is visible", () => {
    const doc = Text.of(Array.from({ length: 500 }, (_, i) => (i === 3 || i === 400 ? "qux" : "ok")));
    const scan = new SpellScan();
    scan.reset(doc.lines);
    const plan = scan.plan(doc, [[1, 20]])!;
    expect(plan.replace).toEqual(Array.from({ length: 20 }, (_, i) => i + 1));
    let marks = run(scan, Decoration.none, doc, [[1, 20]]);
    expect(ranges(marks)).toEqual([[doc.line(4).from, doc.line(4).from + 3]]);
    expect(scan.plan(doc, [[1, 20]])).toBeNull();
    marks = run(scan, marks, doc, [[380, 420]]);
    expect(ranges(marks)).toHaveLength(2);
    marks = run(scan, marks, doc, [[1, 500]]);
    expect(ranges(marks)).toEqual(full(doc));
  });

  it("rescans only edited lines, then following ones only while the prose state differs", () => {
    const lines = ["a", "b", "\\begin{equation}", "x qux", "\\end{equation}", "c qux", "d", "e"];
    let state = EditorState.create({ doc: lines.join("\n") });
    const scan = new SpellScan();
    scan.reset(state.doc.lines);
    let marks = run(scan, Decoration.none, state.doc, [[1, 8]]);
    expect(ranges(marks)).toHaveLength(1); // the equation body is skipped
    // Editing line 1 leaves the state unchanged: only that line is rescanned.
    let tr = state.update({ changes: { from: 0, to: 1, insert: "aa" } });
    scan.edit(tr.changes, state.doc, tr.state.doc);
    state = tr.state;
    expect(scan.plan(state.doc, [[1, 8]])!.replace).toEqual([1]);
    marks = run(scan, marks.map(tr.changes), state.doc, [[1, 8]]);
    // Breaking \begin{equation} opens up lines 3..5: all are rescanned, 6 is not.
    const at = state.doc.line(3).from;
    tr = state.update({ changes: { from: at, to: at + 1, insert: "%" } });
    scan.edit(tr.changes, state.doc, tr.state.doc);
    state = tr.state;
    expect(scan.plan(state.doc, [[1, 8]])!.replace).toEqual([3, 4, 5]);
    marks = run(scan, marks.map(tr.changes), state.doc, [[1, 8]]);
    expect(ranges(marks)).toEqual(full(state.doc));
  });
});
