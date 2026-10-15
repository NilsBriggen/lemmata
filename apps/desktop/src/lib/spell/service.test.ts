import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
import { spell } from "./service";

const calls = (cmd: string) => invoke.mock.calls.filter((c) => c[0] === cmd);

describe("spell service cache", () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockImplementation(async (cmd: string, args: { words?: string[] }) => (cmd === "spell_check" ? args.words!.filter((w) => w.startsWith("x")) : undefined));
  });

  it("asks about each word once and keeps the cache for the same language", async () => {
    spell.setLanguage("en", ["a"]);
    await spell.check(["good", "xbad", "good"]);
    expect(calls("spell_check")[0][1].words).toEqual(["good", "xbad"]);
    expect(spell.status("xbad")).toBe(false);
    expect(spell.status("good")).toBe(true);
    spell.setLanguage("en", ["a"]);
    await spell.check(["good", "xbad", "new"]);
    expect(calls("spell_set_language")).toHaveLength(1);
    expect(calls("spell_check")[1][1].words).toEqual(["new"]);
  });

  it("reloads and clears the cache when the language or the word list changes", async () => {
    spell.setLanguage("de", []);
    await spell.check(["xbad"]);
    expect(calls("spell_set_language")).toHaveLength(1);
    expect(spell.status("xbad")).toBe(false);
    spell.setLanguage("de", ["xbad"]);
    expect(calls("spell_set_language")).toHaveLength(2);
    expect(spell.status("xbad")).toBeUndefined();
  });

  it("add marks the word correct, and the reload that follows keeps the cache", async () => {
    spell.setLanguage("de", ["xbad"]);
    await spell.check(["xother"]);
    const before = calls("spell_set_language").length;
    spell.add("xother");
    expect(spell.status("xother")).toBe(true);
    spell.setLanguage("de", ["xbad", "xother"]);
    expect(calls("spell_set_language")).toHaveLength(before);
    expect(spell.status("xother")).toBe(true);
  });
});
