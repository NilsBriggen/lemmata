import { describe, expect, it } from "vitest";
import { forgetProject, recentEntries, recentProjects, rememberProject, rememberProjects } from "./recent";

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
    rememberProject("/a", s, 1);
    rememberProject("/b", s, 2);
    rememberProject("/a", s, 3);
    expect(recentProjects(s)).toEqual(["/a", "/b"]);
    expect(recentEntries(s)[0]).toEqual({ path: "/a", lastOpened: 3 });
  });

  it("reads the old format of plain paths", () => {
    const s = memoryStorage();
    s.setItem("lemmata.recent", JSON.stringify(["/old"]));
    expect(recentEntries(s)).toEqual([{ path: "/old", lastOpened: 0 }]);
  });

  it("survives corrupt storage", () => {
    const s = memoryStorage();
    s.setItem("lemmata.recent", "{not json");
    expect(recentProjects(s)).toEqual([]);
  });

  it("adds several projects at once, keeping their order", () => {
    const s = memoryStorage();
    rememberProject("/x", s, 1);
    rememberProjects(["/p1", "/p2"], s, 2);
    expect(recentProjects(s)).toEqual(["/p1", "/p2", "/x"]);
    expect(recentProjects(s)).not.toContain(undefined);
    forgetProject("/p1", s);
    expect(recentProjects(s)).toEqual(["/p2", "/x"]);
  });
});
