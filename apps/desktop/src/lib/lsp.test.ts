import { describe, expect, it } from "vitest";
import { fromUri, toUri } from "./lsp";

describe("file URIs", () => {
  it("round-trips Unix paths", () => {
    const uri = toUri("/home/a/My Thesis", "chapters/one.tex");
    expect(uri).toBe("file:///home/a/My%20Thesis/chapters/one.tex");
    expect(fromUri("/home/a/My Thesis", uri)).toBe("chapters/one.tex");
  });

  it("round-trips Windows paths", () => {
    const uri = toUri("C:\\Users\\a\\paper", "main.tex");
    expect(uri).toBe("file:///C:/Users/a/paper/main.tex");
    expect(fromUri("C:\\Users\\a\\paper", uri)).toBe("main.tex");
  });

  it("escapes characters that end a URI path", () => {
    expect(toUri("/p", "a#b?.tex")).toBe("file:///p/a%23b%3F.tex");
    expect(fromUri("/p", "file:///p/a%23b%3F.tex")).toBe("a#b?.tex");
  });

  it("rejects files outside the project", () => {
    expect(fromUri("/home/a/paper", "file:///usr/share/texmf/tex/latex/base/article.cls")).toBeNull();
  });
});
