import { describe, expect, it } from "vitest";
import { fuzzyFilter, fuzzyScore } from "./fuzzy";

describe("fuzzy matching", () => {
  it("requires characters in order", () => {
    expect(fuzzyScore("cmt", "chapters/method.tex")).not.toBeNull();
    expect(fuzzyScore("tmc", "chapters/method.tex")).toBeNull();
  });

  it("ranks word starts and runs first", () => {
    const files = ["chapters/introduction.tex", "main.tex", "figures/intro-plot.png"];
    expect(fuzzyFilter(files, "main", (f) => f)[0]).toBe("main.tex");
    expect(fuzzyFilter(files, "intro", (f) => f)).toHaveLength(2);
  });
});
