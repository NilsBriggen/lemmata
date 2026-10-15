import { describe, expect, it } from "vitest";
import type { Entry } from "./api";
import { UndoLog } from "./fileUndo";
import { canMove, copyName, countFiles, flatten, formatSize, namesIn, range, remap, sortEntries, topLevel } from "./treeModel";

const f = (path: string, size = 0, modified = 0): Entry => ({ name: path.split("/").pop()!, path, is_dir: false, size, modified, children: [] });
const d = (path: string, children: Entry[] = []): Entry => ({ name: path.split("/").pop()!, path, is_dir: true, size: 0, modified: 0, children });

const sample = [d("chapters", [f("chapters/b.tex", 5, 30), f("chapters/a.tex", 50, 10)]), f("main.tex", 10, 20), f("refs.bib", 1, 40), f("Zebra.png", 900, 5)];

describe("sortEntries", () => {
  const names = (e: Entry[]) => e.map((x) => x.name);
  it("sorts by name with folders first, case-insensitively", () => {
    expect(names(sortEntries(sample, "name", true))).toEqual(["chapters", "main.tex", "refs.bib", "Zebra.png"]);
  });
  it("sorts nested children too", () => {
    expect(names(sortEntries(sample, "name", true)[0].children)).toEqual(["a.tex", "b.tex"]);
  });
  it("sorts by size, largest first", () => {
    expect(names(sortEntries(sample, "size", true))).toEqual(["chapters", "Zebra.png", "main.tex", "refs.bib"]);
  });
  it("sorts by modified time, newest first", () => {
    expect(names(sortEntries(sample, "modified", true))).toEqual(["chapters", "refs.bib", "main.tex", "Zebra.png"]);
  });
  it("sorts by type, then name", () => {
    expect(names(sortEntries(sample, "type", true))).toEqual(["chapters", "refs.bib", "Zebra.png", "main.tex"]);
  });
  it("mixes folders and files when folders-first is off", () => {
    expect(names(sortEntries(sample, "size", false))).toEqual(["Zebra.png", "main.tex", "refs.bib", "chapters"]);
  });
  it("numbers sort naturally", () => {
    expect(names(sortEntries([f("ch10.tex"), f("ch2.tex")], "name", true))).toEqual(["ch2.tex", "ch10.tex"]);
  });
  it("does not change its input", () => {
    const copy = JSON.stringify(sample);
    sortEntries(sample, "size", true);
    expect(JSON.stringify(sample)).toBe(copy);
  });
});

describe("flatten and range", () => {
  it("lists visible rows only", () => {
    expect(flatten(sample, {}).map((r) => r.path)).toEqual(["chapters", "chapters/b.tex", "chapters/a.tex", "main.tex", "refs.bib", "Zebra.png"]);
    expect(flatten(sample, { chapters: true }).map((r) => r.path)).toEqual(["chapters", "main.tex", "refs.bib", "Zebra.png"]);
  });
  it("selects a range in either direction", () => {
    const rows = flatten(sample, {});
    expect(range(rows, "chapters/a.tex", "refs.bib")).toEqual(["chapters/a.tex", "main.tex", "refs.bib"]);
    expect(range(rows, "refs.bib", "chapters/a.tex")).toEqual(["chapters/a.tex", "main.tex", "refs.bib"]);
    expect(range(rows, "gone", "main.tex")).toEqual(["main.tex"]);
  });
});

describe("moves and paths", () => {
  it("keeps only top-level paths", () => {
    expect(topLevel(["a", "a/b", "c", "a"])).toEqual(["a", "c"]);
  });
  it("refuses moves into themselves or where they already are", () => {
    expect(canMove(["a"], "a")).toBe(false);
    expect(canMove(["a"], "a/b")).toBe(false);
    expect(canMove(["a/x.tex"], "a")).toBe(false);
    expect(canMove(["a/x.tex"], "")).toBe(true);
    expect(canMove(["a/x.tex", "y.tex"], "")).toBe(true);
  });
  it("remaps paths under a renamed folder", () => {
    expect(remap("old/x.tex", "old", "new")).toBe("new/x.tex");
    expect(remap("older/x.tex", "old", "new")).toBe("older/x.tex");
    expect(remap("old", "old", "new")).toBe("new");
  });
});

describe("naming", () => {
  it("finds a free copy name", () => {
    expect(copyName("main.tex", false, new Set())).toBe("main copy.tex");
    expect(copyName("main.tex", false, new Set(["main copy.tex"]))).toBe("main copy 2.tex");
    expect(copyName("figures", true, new Set(["figures copy"]))).toBe("figures copy 2");
    expect(copyName(".gitignore", false, new Set())).toBe(".gitignore copy");
  });
  it("lists names in a folder", () => {
    expect([...namesIn(sample, "chapters")].sort()).toEqual(["a.tex", "b.tex"]);
    expect(namesIn(sample, "").has("main.tex")).toBe(true);
  });
  it("counts files and formats sizes", () => {
    expect(countFiles(sample[0])).toBe(2);
    expect(formatSize(512)).toBe("512 B");
    expect(formatSize(2048)).toBe("2.0 KB");
    expect(formatSize(3 * 1024 * 1024)).toBe("3.0 MB");
  });
});

describe("UndoLog", () => {
  it("undoes the newest step first and keeps at most 20", async () => {
    const log = new UndoLog();
    const ran: number[] = [];
    for (let i = 0; i < 25; i++) log.push({ label: `s${i}`, undo: async () => void ran.push(i) });
    expect(log.size).toBe(20);
    await log.undo();
    await log.undo();
    expect(ran).toEqual([24, 23]);
  });
  it("drops a failing step", async () => {
    const log = new UndoLog();
    log.push({ label: "bad", undo: async () => Promise.reject(new Error("x")) });
    await expect(log.undo()).rejects.toThrow();
    expect(log.size).toBe(0);
    expect(await log.undo()).toBeNull();
  });
});

describe("isTextFile", () => {
  it("opens text formats and known names, not binaries", async () => {
    const { isTextFile, isViewerFile } = await import("./treeModel");
    for (const p of ["main.tex", "data/results.csv", "notes.MD", "scripts/plot.py", "Makefile", "sub/.latexmkrc", "README"]) expect(isTextFile(p), p).toBe(true);
    for (const p of ["paper.docx", "fig.png", "font.ttf", "archive.zip", "x.pdf"]) expect(isTextFile(p), p).toBe(false);
    expect(isViewerFile("fig.png") && isViewerFile("x.pdf")).toBe(true);
  });
});
