import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";
import { CompletionContext } from "@codemirror/autocomplete";
import { matchSnippets, snippetCompletions, validTrigger, type Snippet } from "./snippets";

const S: Snippet[] = [
  { trigger: "fig", body: "\\begin{figure}\n${}\n\\end{figure}", description: "A figure" },
  { trigger: "figs", body: "x", description: "" },
  { trigger: "thm", body: "\\begin{theorem}${}\\end{theorem}", description: "Theorem" },
  { trigger: "bad trigger", body: "x", description: "" },
];

const run = (doc: string) => {
  const state = EditorState.create({ doc });
  return snippetCompletions(() => S)(new CompletionContext(state, doc.length, false)) as ReturnType<ReturnType<typeof snippetCompletions>>;
};

describe("matchSnippets", () => {
  it("needs two letters and matches by prefix, shortest first", () => {
    expect(matchSnippets(S, "f")).toEqual([]);
    expect(matchSnippets(S, "fi").map((s) => s.trigger)).toEqual(["fig", "figs"]);
    expect(matchSnippets(S, "TH").map((s) => s.trigger)).toEqual(["thm"]);
  });
  it("skips invalid triggers", () => {
    expect(matchSnippets(S, "bad")).toEqual([]);
    expect(validTrigger("a")).toBe(false);
    expect(validTrigger("eq-1")).toBe(true);
  });
});

describe("snippetCompletions", () => {
  it("offers snippets for a typed trigger", async () => {
    const r = (await run("Some text fig")) as { from: number; options: { label: string }[] };
    expect(r.from).toBe(10);
    expect(r.options.map((o) => o.label)).toEqual(["fig", "figs"]);
  });
  it("stays out of commands and braced arguments", async () => {
    expect(await run("\\fig")).toBeNull();
    expect(await run("\\ref{fig")).toBeNull();
    expect(await run("nothing matches zz")).toBeNull();
  });
});
