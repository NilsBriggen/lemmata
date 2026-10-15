import { describe, expect, it } from "vitest";
import { DEFAULTS, validSettings } from "./settings.svelte";

describe("validSettings", () => {
  it("keeps values of the right type and drops the rest", () => {
    const got = validSettings({ fontSize: 15, theme: 3, snippets: "x", texDir: null, layout: "pdf", unknown: 1, shellEscapeProjects: ["/p"] });
    expect(got).toEqual({ fontSize: 15, texDir: null, layout: "pdf", shellEscapeProjects: ["/p"] });
    expect(validSettings({ texDir: "/opt/tex" })).toEqual({ texDir: "/opt/tex" });
    expect(DEFAULTS.shellEscapeProjects).toEqual([]);
  });
});
