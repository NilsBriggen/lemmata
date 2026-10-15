import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
import { grammar, ltLanguage } from "./service";

const m = (ruleId: string, category = "GRAMMAR") => ({ offset: 0, length: 1, message: "", shortMessage: "", replacements: [], ruleId, category });
const calls = () => invoke.mock.calls.filter((c) => c[0] === "grammar_check");

describe("grammar service", () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockImplementation(async (_c: string, a: { text: string }) => (a.text.startsWith("bad") ? [m("A"), m("WHITESPACE_RULE")] : []));
    grammar.configure("off", "", "en");
    grammar.resetIgnored();
    grammar.configure("local", "", "en");
  });

  it("maps spell languages", () => {
    expect(ltLanguage("en")).toBe("en-US");
    expect(ltLanguage("de")).toBe("de-DE");
    expect(ltLanguage("off")).toBeNull();
  });

  it("asks about a paragraph text once; edited text is asked again", async () => {
    await grammar.check(["bad one", "fine", "bad one"]);
    expect(calls().map((c) => c[1].text).sort()).toEqual(["bad one", "fine"]);
    expect(calls()[0][1]).toMatchObject({ url: "http://localhost:8081", language: "en-US" });
    await grammar.check(["bad one", "fine", "bad one edited"]);
    expect(calls()).toHaveLength(3);
  });

  it("drops noisy rules and ignored rules", async () => {
    await grammar.check(["bad one"]);
    expect(grammar.found("bad one")!.map((x) => x.ruleId)).toEqual(["A"]);
    grammar.ignoreRule("A");
    expect(grammar.found("bad one")).toEqual([]);
    expect(grammar.found("unchecked")).toBeUndefined();
  });

  it("keeps at most two requests in flight and forgets everything on a new server", async () => {
    let running = 0;
    let peak = 0;
    invoke.mockImplementation(async () => {
      peak = Math.max(peak, ++running);
      await new Promise((r) => setTimeout(r, 5));
      running--;
      return [];
    });
    await grammar.check(["a1", "a2", "a3", "a4", "a5"]);
    expect(peak).toBe(2);
    grammar.configure("custom", "https://lt.example.org", "en");
    expect(grammar.has("a1")).toBe(false);
    await grammar.check(["a1"]);
    expect(invoke).toHaveBeenLastCalledWith("grammar_check", { url: "https://lt.example.org", language: "en-US", text: "a1" });
  });

  it("stops on an error and reports it", async () => {
    invoke.mockReset();
    invoke.mockRejectedValue("No LanguageTool server at http://localhost:8081. Start one, or change the address in Preferences.");
    await grammar.check(["x1", "x2", "x3", "x4"]);
    expect(grammar.error).toMatch(/No LanguageTool server/);
    expect(calls().length).toBeLessThanOrEqual(2);
  });

  it("does nothing when off or for a language without a rule set", async () => {
    grammar.configure("off", "", "en");
    await grammar.check(["x"]);
    grammar.configure("local", "", "off");
    await grammar.check(["x"]);
    expect(invoke).not.toHaveBeenCalled();
  });
});
