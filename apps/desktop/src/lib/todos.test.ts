import { describe, expect, it } from "vitest";
import { groupTodos, type Todo } from "./todos";

const t = (file: string, line: number, text = "x"): Todo => ({ file, line, kind: "TODO", text });

describe("groupTodos", () => {
  it("returns no groups for no todos", () => {
    expect(groupTodos([])).toEqual([]);
  });

  it("groups by file in arrival order and keeps each file's lines in order", () => {
    const groups = groupTodos([t("main.tex", 2), t("a/b.tex", 1), t("main.tex", 9)]);
    expect(groups.map((g) => g.file)).toEqual(["main.tex", "a/b.tex"]);
    expect(groups[0].todos.map((x) => x.line)).toEqual([2, 9]);
    expect(groups[1].todos.map((x) => x.line)).toEqual([1]);
  });
});
