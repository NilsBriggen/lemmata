import { describe, expect, it } from "vitest";
import { locate, parse, serialize, type Thread } from "./model";

const thread = (over: Partial<Thread>): Thread => ({
  id: "a",
  file: "main.tex",
  line: 1,
  quote: "",
  body: "Check this",
  created: 1,
  resolved: false,
  replies: [],
  ...over,
});

describe("comments", () => {
  it("round-trips and ignores damaged entries", () => {
    const t = thread({ replies: [{ body: "ok", created: 2 }] });
    expect(parse(serialize([t]))).toEqual([t]);
    expect(parse('{"threads":[{"id":1},{"id":"b","file":"x.tex","body":"b","line":-3}]}')).toEqual([
      thread({ id: "b", file: "x.tex", body: "b", line: 1, created: 0 }),
    ]);
    expect(parse("not json")).toEqual([]);
  });

  it("finds the quote nearest to its old line after lines moved", () => {
    const text = "intro\nthe result\nmore\nmore\nthe result\nend";
    // Was on line 4; two lines were added above, so the copy on line 5 is the one.
    expect(locate(text, { line: 4, quote: "the result" })).toEqual({ from: 27, to: 37, line: 5 });
    expect(locate(text, { line: 1, quote: "the result" })?.line).toBe(2);
  });

  it("falls back to the line when the quote is gone, and gives up past the end", () => {
    const text = "one\ntwo\nthree";
    expect(locate(text, { line: 2, quote: "vanished" })).toEqual({ from: 4, to: 7, line: 2 });
    expect(locate(text, { line: 3, quote: "" })).toEqual({ from: 8, to: 13, line: 3 });
    expect(locate(text, { line: 9, quote: "" })).toBeNull();
  });
});
