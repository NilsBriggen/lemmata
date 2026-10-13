import { describe, expect, it } from "vitest";
import { recentProjects, rememberProject } from "./recent";

function memoryStorage() {
  const data = new Map<string, string>();
  return {
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
  };
}

describe("recent projects", () => {
  it("puts the newest first without duplicates", () => {
    const s = memoryStorage();
    rememberProject("/a", s);
    rememberProject("/b", s);
    expect(rememberProject("/a", s)).toEqual(["/a", "/b"]);
    expect(recentProjects(s)).toEqual(["/a", "/b"]);
  });

  it("survives corrupt storage", () => {
    const s = memoryStorage();
    s.setItem("lemmata.recent", "{not json");
    expect(recentProjects(s)).toEqual([]);
  });

  it("keeps at most ten", () => {
    const s = memoryStorage();
    for (let i = 0; i < 15; i++) rememberProject(`/p${i}`, s);
    expect(recentProjects(s)).toHaveLength(10);
    expect(recentProjects(s)[0]).toBe("/p14");
  });
});
