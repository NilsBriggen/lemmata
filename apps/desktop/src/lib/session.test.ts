import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Buffers } from "./session.svelte";
import { contentHash } from "./contentHash";

/** A disk in memory that refuses a write whose expected hash doesn't match, like write_text. */
function setup(autosaveMs = 600) {
  const disk = new Map<string, string>();
  const autosaves: string[] = [];
  let failNext: string | null = null;
  const io = {
    write: vi.fn(async (path: string, text: string, expected: number | undefined) => {
      await Promise.resolve();
      if (failNext) {
        const m = failNext;
        failNext = null;
        throw new Error(m);
      }
      if (expected !== undefined && disk.has(path) && contentHash(disk.get(path)!) !== expected) throw new Error("conflict: changed on disk");
      disk.set(path, text);
    }),
    autosaveMs: () => autosaveMs,
    autosave: (path: string) => autosaves.push(path),
  };
  const b = new Buffers(io);
  return { b, disk, io, autosaves, fail: (m: string) => (failNext = m) };
}

describe("Buffers", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("autosaves after the delay, once, and not when autosave is off", () => {
    const { b, autosaves } = setup();
    b.loaded("a.tex", "x");
    b.edit("a.tex", "xy");
    vi.advanceTimersByTime(300);
    b.edit("a.tex", "xyz"); // typing restarts the delay
    vi.advanceTimersByTime(599);
    expect(autosaves).toEqual([]);
    vi.advanceTimersByTime(1);
    expect(autosaves).toEqual(["a.tex"]);
    const off = setup(0);
    off.b.edit("a.tex", "x");
    vi.advanceTimersByTime(10_000);
    expect(off.autosaves).toEqual([]);
  });

  it("saves, and keeps edits typed while the write was in flight unsaved", async () => {
    const { b, disk, autosaves } = setup();
    b.loaded("a.tex", "one");
    b.edit("a.tex", "two");
    const saving = b.write("a.tex");
    b.edit("a.tex", "three"); // typed during the write
    expect(await saving).toEqual({ saved: "two" });
    expect(disk.get("a.tex")).toBe("two");
    expect(b.dirty["a.tex"]).toBe(true);
    vi.advanceTimersByTime(600);
    expect(autosaves).toEqual(["a.tex"]);
    expect(await b.write("a.tex")).toEqual({ saved: "three" });
    expect(b.dirty["a.tex"]).toBe(false);
    expect(await b.write("a.tex")).toBeNull(); // nothing left to save
  });

  it("never writes over another program's change, and pauses autosave until the user decides", async () => {
    const { b, disk, autosaves } = setup();
    disk.set("a.tex", "base");
    b.loaded("a.tex", "base");
    b.edit("a.tex", "mine");
    disk.set("a.tex", "theirs");
    expect(await b.write("a.tex")).toEqual({ conflict: true });
    expect(disk.get("a.tex")).toBe("theirs");
    expect(b.conflicts).toEqual(["a.tex"]);
    b.edit("a.tex", "mine, more");
    vi.advanceTimersByTime(5000);
    expect(autosaves).toEqual([]);
    expect(await b.write("a.tex")).toBeNull();
    // Keep mine: the next write goes over theirs.
    b.resolve("a.tex", "mine");
    expect(await b.write("a.tex")).toEqual({ saved: "mine, more" });
    expect(disk.get("a.tex")).toBe("mine, more");
    expect(b.conflicts).toEqual([]);
  });

  it("drops its copy when the user loads the version on disk", () => {
    const { b } = setup();
    b.loaded("a.tex", "base");
    b.edit("a.tex", "mine");
    b.conflict("a.tex");
    b.resolve("a.tex", "disk");
    expect(b.contents.has("a.tex")).toBe(false);
    expect(b.dirty["a.tex"]).toBe(false);
    expect(b.conflicts).toEqual([]);
  });

  it("tells its own writes and the editor's text from real outside changes", async () => {
    const { b } = setup();
    b.loaded("a.tex", "one");
    expect(b.outsideChange("a.tex", "one")).toBe("ignore"); // our own read or write echoing back
    b.edit("a.tex", "two");
    expect(b.outsideChange("a.tex", "two")).toBe("ignore"); // someone wrote exactly what we have
    expect(b.lastSaved.get("a.tex")).toBe("two");
    b.edit("a.tex", "three");
    expect(b.outsideChange("a.tex", "other")).toBe("conflict");
    expect(b.conflicts).toEqual(["a.tex"]);
    const clean = setup();
    clean.b.loaded("b.tex", "x");
    expect(clean.b.outsideChange("b.tex", "y")).toBe("reload");
  });

  it("keeps a failed save unsaved and listed, until it succeeds", async () => {
    const { b, fail } = setup();
    b.loaded("a.tex", "x");
    b.edit("a.tex", "y");
    fail("disk full");
    expect(await b.write("a.tex")).toEqual({ error: "disk full" });
    expect(b.dirty["a.tex"]).toBe(true);
    expect(b.failedSaves).toEqual(["a.tex"]);
    expect(await b.write("a.tex")).toEqual({ saved: "y" });
    expect(b.failedSaves).toEqual([]);
  });

  it("moves unsaved marks and conflicts with a rename, and forgets closed files", () => {
    const { b, autosaves } = setup();
    b.loaded("ch/a.tex", "a");
    b.edit("ch/a.tex", "a2");
    b.conflict("ch/a.tex");
    b.rename("ch", "chapters");
    expect(b.dirty["chapters/a.tex"]).toBe(true);
    expect(b.conflicts).toEqual(["chapters/a.tex"]);
    expect(b.contents.has("ch/a.tex")).toBe(false);
    b.loaded("main.tex", "m");
    b.edit("main.tex", "m2");
    b.forget("main.tex");
    vi.advanceTimersByTime(5000);
    expect(autosaves).toEqual([]); // a closed file's pending save never runs
    expect(b.unsaved).toEqual(["chapters/a.tex"]);
    b.clear();
    expect(b.unsaved).toEqual([]);
  });
});
