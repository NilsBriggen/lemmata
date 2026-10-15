import { describe, it, expect } from "vitest";
import { targetAt, resolveFile, mathSource } from "./hover";

/** The mouse sits at `|`. */
function at(src: string) {
  const i = src.indexOf("|");
  return targetAt(src.replace("|", ""), i);
}

describe("targetAt", () => {
  it("finds the label of a reference, on the command or inside", () => {
    expect(at("see \\eq|ref{eq:a} here")).toMatchObject({ kind: "ref", name: "eq:a" });
    expect(at("see \\ref{eq|:a} here")).toMatchObject({ kind: "ref", name: "eq:a", from: 9, to: 13 });
    expect(at("see \\Cref*{a|}")).toMatchObject({ kind: "ref", name: "a" });
  });

  it("picks the key under the mouse in a list", () => {
    const src = "\\cite{alpha, be|ta,gamma}";
    expect(at(src)).toMatchObject({ kind: "cite", name: "beta" });
    expect(at("\\cite{alpha, beta,ga|mma}")).toMatchObject({ name: "gamma" });
    expect(at("\\cite{|alpha,beta}")).toMatchObject({ name: "alpha" });
    expect(at("\\cref{a,b|}")).toMatchObject({ kind: "ref", name: "b" });
  });

  it("on the command name of a list gives nothing, of a single key gives the key", () => {
    expect(at("\\ci|te{a,b}")).toBeNull();
    expect(at("\\ci|te{a}")).toMatchObject({ name: "a" });
  });

  it("skips optional arguments, even with braces or brackets inside", () => {
    expect(at("\\textcite[see {\\S 3}][p. 4]{k|ey}")).toMatchObject({ kind: "cite", name: "key" });
    expect(at("\\autocite[{[x]}]{k|}")).toMatchObject({ name: "k" });
  });

  it("finds commands nested in other commands", () => {
    expect(at("\\textbf{see \\ref{x|}} and")).toMatchObject({ kind: "ref", name: "x" });
    expect(at("\\emph{a \\cite{one}, \\cite{tw|o}}")).toMatchObject({ name: "two" });
  });

  it("reads images and input files whole", () => {
    expect(at("\\includegraphics[width=.5\\textwidth]{fi|gs/a.png}")).toMatchObject({ kind: "graphic", name: "figs/a.png" });
    expect(at("\\includegraphics{a_{1}|.png}")).toMatchObject({ name: "a_{1}.png" });
    expect(at("\\inp|ut{chapters/intro}")).toMatchObject({ kind: "input", name: "chapters/intro" });
    expect(at("\\include{ch|2}")).toMatchObject({ kind: "input", name: "ch2" });
  });

  it("ignores everything else", () => {
    expect(at("plain| text")).toBeNull();
    expect(at("\\label{a|}")).toBeNull();
    expect(at("\\hyperref[a]{te|xt}")).toBeNull();
    expect(at("\\ref{a} aft|er")).toBeNull();
    expect(at("\\ref{unclosed|")).toBeNull();
  });

  it("ignores comments", () => {
    expect(at("text % \\ref{a|}")).toBeNull();
    expect(at("50\\% \\ref{a|}")).toMatchObject({ name: "a" });
  });
});

describe("resolveFile", () => {
  const files = ["main.tex", "figs/a.png", "figs/b.pdf", "chapters/intro.tex"];
  it("tries extensions and folders", () => {
    expect(resolveFile("figs/a.png", files, [""])).toBe("figs/a.png");
    expect(resolveFile("a", files, ["", ".png"])).toBe("figs/a.png");
    expect(resolveFile("./chapters/intro", files, ["", ".tex"])).toBe("chapters/intro.tex");
    expect(resolveFile("nope", files, ["", ".tex"])).toBeNull();
  });
});

describe("mathSource", () => {
  it("wraps multi-line environments and drops labels", () => {
    expect(mathSource("align*", "a &= b \\label{x} \\\\ c &= d \\nonumber")).toBe("\\begin{aligned}a &= b  \\\\ c &= d\\end{aligned}");
    expect(mathSource("equation", "E=mc^2")).toBe("E=mc^2");
    expect(mathSource("alignat", "{2}a&=b")).toBe("\\begin{aligned}a&=b\\end{aligned}");
  });
});
