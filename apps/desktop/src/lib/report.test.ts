import { describe, expect, it } from "vitest";
import { errorSummary, issueUrl, scrubPaths } from "./report";

describe("problem reports", () => {
  it("removes paths that could contain names", () => {
    expect(scrubPaths("File /home/nils/thesis/main.tex not found")).toBe("File <path> not found");
    expect(scrubPaths("at C:\\Users\\Nils\\paper\\a.tex")).toBe("at <path>");
    expect(scrubPaths("see ~/texlive/2026 and ./chapters/a.tex")).toBe("see <path> and ./chapters/a.tex");
  });

  it("summarises errors with project-relative locations", () => {
    const s = errorSummary([
      { severity: "error", kind: { type: "other" }, file: "./main.tex", line: 4, message: "Undefined control sequence.", hint: null },
      { severity: "warning", kind: { type: "other" }, file: null, line: null, message: "ignored", hint: null },
    ]);
    expect(s).toBe("main.tex:4: Undefined control sequence.");
  });

  it("builds an issue-form URL", () => {
    const u = new URL(issueUrl("Lemmata 0.1.0", "x"));
    expect(u.searchParams.get("template")).toBe("bug.yml");
    expect(u.searchParams.get("system")).toBe("Lemmata 0.1.0");
  });
});
