import { describe, expect, it } from "vitest";
import type { ManagedEntry } from "./api";
import { filterEntries, freeKey, sortEntries, validKey, warningsByEntry } from "./manager";

const e = (key: string, authors: string, year: string, title = ""): ManagedEntry =>
  ({ key, kind: "misc", title, authors, year, file: "refs.bib", line: 1, doi: "", fields: {}, cited: false });

const list = [e("b", "Zed", "2001", "Graphs"), e("a", "", "", "Nothing"), e("c", "Abe", "2020", "Proofs")];

describe("bibliography manager helpers", () => {
  it("filters by every word", () => {
    expect(filterEntries(list, "abe 2020").map((x) => x.key)).toEqual(["c"]);
    expect(filterEntries(list, "")).toHaveLength(3);
  });

  it("sorts, with missing values last and newest year first", () => {
    expect(sortEntries(list, "key").map((x) => x.key)).toEqual(["a", "b", "c"]);
    expect(sortEntries(list, "author").map((x) => x.key)).toEqual(["c", "b", "a"]);
    expect(sortEntries(list, "year").map((x) => x.key)).toEqual(["c", "b", "a"]);
  });

  it("maps duplicates to each member", () => {
    const w = warningsByEntry([{ kind: "doi", value: "x", entries: [0, 2] }]);
    expect(w.get(0)).toEqual([{ kind: "doi", others: [2] }]);
    expect(w.has(1)).toBe(false);
  });

  it("finds a free key and validates keys", () => {
    expect(freeKey("k", ["K", "ka"])).toBe("kb");
    expect(validKey("smith2020a")).toBe(true);
    expect(validKey("bad key")).toBe(false);
    expect(validKey("")).toBe(false);
  });
});
