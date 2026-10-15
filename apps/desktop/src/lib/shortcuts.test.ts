import { describe, expect, it } from "vitest";
import { SHORTCUTS, keyLabel, matches, matchesQuery } from "./shortcuts";

const key = (k: string, mods: Partial<KeyboardEvent> = {}) =>
  ({ key: k, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...mods }) as KeyboardEvent;

describe("shortcut matching", () => {
  it("opens the shortcut sheet on either Mod-Shift-/ or F1", () => {
    expect(matches(key("?", { ctrlKey: true, metaKey: true, shiftKey: true }), "shortcuts")).toBe(true);
    expect(matches(key("F1"), "shortcuts")).toBe(true);
    expect(matches(key("F1", { shiftKey: true }), "shortcuts")).toBe(false);
  });

  it("never matches the pointer entries", () => {
    const mouse = SHORTCUTS.filter((s) => s.group === "Mouse");
    expect(mouse.length).toBeGreaterThan(0);
    for (const s of mouse) {
      expect(s.keys).toBe("");
      expect(matches(key("F1"), s.id)).toBe(false);
      expect(matches(key("Enter", { ctrlKey: true, metaKey: true }), s.id)).toBe(false);
    }
  });
});

describe("shortcut search", () => {
  const find = (q: string) => SHORTCUTS.filter((s) => matchesQuery(s, q)).map((s) => s.id);

  it("matches labels and key text, ignoring case", () => {
    expect(find("")).toHaveLength(SHORTCUTS.length);
    expect(find("KEYBOARD")).toContain("shortcuts");
    expect(find("f1")).toContain("shortcuts");
    expect(find("shift-f8")).toContain("prev-problem");
    expect(find("no such thing at all")).toEqual([]);
  });

  it("shows both keys of an entry", () => {
    const s = SHORTCUTS.find((x) => x.id === "shortcuts")!;
    expect(keyLabel(s).endsWith(" / F1")).toBe(true);
  });
});
