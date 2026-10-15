import { describe, expect, it } from "vitest";
import { aheadBehind, commitBlocker, isOverleaf, repoName } from "./api";

describe("git helpers", () => {
  it("names the folder after the repository", () => {
    expect(repoName("https://git.overleaf.com/64f0c1a2/")).toBe("64f0c1a2");
    expect(repoName("git@github.com:you/thesis.git")).toBe("thesis");
    expect(repoName("https://github.com/you/thesis.git")).toBe("thesis");
    expect(repoName("  ")).toBe("");
  });

  it("summarises ahead and behind", () => {
    expect(aheadBehind({ ahead: 0, behind: 0 })).toBe("");
    expect(aheadBehind({ ahead: 2, behind: 0 })).toBe("↑2");
    expect(aheadBehind({ ahead: 2, behind: 1 })).toBe("↑2 ↓1");
  });

  it("says what blocks a commit", () => {
    const staged = [{ path: "a.tex", status: "M", from: null }];
    expect(commitBlocker({ staged: [] }, "msg")).toMatch(/Tick/);
    expect(commitBlocker({ staged }, "  ")).toMatch(/message/);
    expect(commitBlocker({ staged }, "msg")).toBeNull();
  });

  it("recognises Overleaf URLs", () => {
    expect(isOverleaf("https://git.overleaf.com/abc")).toBe(true);
    expect(isOverleaf("https://git:tok@git.overleaf.com/abc")).toBe(true);
    expect(isOverleaf("https://github.com/you/x.git")).toBe(false);
  });
});
