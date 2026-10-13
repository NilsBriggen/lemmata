import { describe, expect, it } from "vitest";
import { symbolAt } from "./symbolAt";

describe("symbolAt", () => {
  const at = (line: string, needle: string, isBib = false) => symbolAt(line, line.indexOf(needle) + 1, isBib);

  it("finds labels, references and citations under the cursor", () => {
    expect(at("\\section{A}\\label{sec:a}", "sec:a")).toEqual({ kind: "label", name: "sec:a" });
    expect(at("see \\cref{sec:a, eq:b}", "eq:b")).toEqual({ kind: "label", name: "eq:b" });
    expect(at("by \\citep[p.~2]{knuth84,lamport94}", "lamport")).toEqual({ kind: "cite", name: "lamport94" });
  });
  it("ignores links, comments and the command name itself", () => {
    expect(at("\\href{http://x.org}{text}", "http")).toBeNull();
    expect(at("% \\ref{sec:a}", "sec:a")).toBeNull();
    expect(at("\\ref{sec:a}", "ref")).toBeNull();
  });
  it("reads keys in .bib files", () => {
    expect(at("@article{knuth84,", "knuth", true)).toEqual({ kind: "cite", name: "knuth84" });
    expect(at("@article{knuth84,", "article", true)).toBeNull();
  });
});
