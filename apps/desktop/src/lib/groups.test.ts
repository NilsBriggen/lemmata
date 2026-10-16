import { describe, expect, it } from "vitest";
import { countIn, createGroup, deleteGroup, emptyGroups, forgetPath, inView, moveProject, nameProblem, readGroups, renameGroup, restoreGroup, writeGroups } from "./groups";

const memory = () => {
  const data = new Map<string, string>();
  return { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
};

describe("project folders", () => {
  it("creates folders with unique, trimmed names", () => {
    const a = createGroup(emptyGroups(), "  Thesis ", "t")!;
    expect(a.state.groups).toEqual([{ id: "t", name: "Thesis" }]);
    expect(createGroup(a.state, "thesis", "x")).toBeNull();
    expect(nameProblem(a.state, "")).toMatch(/name/);
    expect(nameProblem(a.state, "x".repeat(41))).toMatch(/at most/);
    expect(nameProblem(a.state, "Thesis", "t")).toBeNull();
  });

  it("renames, moves projects and counts them", () => {
    let s = createGroup(createGroup(emptyGroups(), "A", "a")!.state, "B", "b")!.state;
    s = moveProject(moveProject(s, "/p/one", "a"), "/p/two", "a");
    expect(countIn(s, "a")).toBe(2);
    s = moveProject(s, "/p/one", "b");
    expect(countIn(s, "a")).toBe(1);
    expect(renameGroup(s, "a", "B")).toBeNull();
    expect(renameGroup(s, "a", "Courses")!.groups[0].name).toBe("Courses");
    expect(moveProject(s, "/p/two", null).members).toEqual({ "/p/one": "b" });
    // A folder that does not exist leaves the project unfiled.
    expect(moveProject(s, "/p/two", "zzz").members["/p/two"]).toBeUndefined();
  });

  it("deleting a folder unfiles its projects and forgetting drops a project", () => {
    let s = createGroup(emptyGroups(), "A", "a")!.state;
    s = moveProject(moveProject(s, "/p/one", "a"), "/p/two", "a");
    s = deleteGroup(s, "a");
    expect(s).toEqual({ groups: [], members: {} });
    s = createGroup(s, "B", "b")!.state;
    s = moveProject(s, "/p/one", "b");
    expect(forgetPath(s, "/p/one").members).toEqual({});
    expect(forgetPath(s, "/nope")).toBe(s);
  });

  it("restores a deleted folder in its place with its projects", () => {
    let s = createGroup(createGroup(emptyGroups(), "A", "a")!.state, "B", "b")!.state;
    s = moveProject(moveProject(s, "/p/one", "a"), "/p/two", "b");
    const gone = deleteGroup(s, "a");
    expect(restoreGroup(gone, s, "a")).toEqual(s);
    // A folder already back, or one that never existed, changes nothing.
    expect(restoreGroup(s, s, "a")).toBe(s);
    expect(restoreGroup(gone, s, "zzz")).toBe(gone);
    // Changes made since the delete win over the old membership.
    const later = moveProject(gone, "/p/one", "b");
    expect(restoreGroup(later, s, "a").members).toEqual({ "/p/one": "b", "/p/two": "b" });
  });

  it("filters by view", () => {
    let s = createGroup(emptyGroups(), "A", "a")!.state;
    s = moveProject(s, "/p/one", "a");
    expect(["/p/one", "/p/two"].filter((p) => inView(s, p, "a"))).toEqual(["/p/one"]);
    // The top level holds only the projects in no folder; the folder itself is shown there instead.
    expect(["/p/one", "/p/two"].filter((p) => inView(s, p, "root"))).toEqual(["/p/two"]);
  });

  it("survives a round trip and ignores damaged storage", () => {
    const store = memory();
    let s = createGroup(emptyGroups(), "A", "a")!.state;
    s = moveProject(s, "/p/one", "a");
    writeGroups(s, store);
    expect(readGroups(store)).toEqual(s);
    store.setItem("lemmata.groups", "{not json");
    expect(readGroups(store)).toEqual(emptyGroups());
    store.setItem("lemmata.groups", JSON.stringify({ groups: [{ id: "a", name: "A" }], members: { "/p": "gone", "/q": "a" } }));
    expect(readGroups(store).members).toEqual({ "/q": "a" });
  });
});
