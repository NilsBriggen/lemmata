import { describe, expect, it } from "vitest";
import type { TexStatus } from "./api";
import { loadTexCache, saveTexCache } from "./texCache";

const memory = () => {
  const data = new Map<string, string>();
  return { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
};
const status = (ready: boolean) => ({ detection: { ready, tools: [], bin_dir: null, distribution: { name: "unknown" }, outdated: false }, os: {}, instructions: {} }) as unknown as TexStatus;

describe("TeX detection cache", () => {
  it("remembers a working installation", () => {
    const store = memory();
    saveTexCache(status(true), store);
    expect(loadTexCache(store)?.detection.ready).toBe(true);
  });
  it("does not remember a failed detection, so the next start waits for the real answer", () => {
    const store = memory();
    saveTexCache(status(false), store);
    expect(loadTexCache(store)).toBeNull();
  });
  it("ignores damaged or empty storage", () => {
    const store = memory();
    expect(loadTexCache(store)).toBeNull();
    store.setItem("lemmata.tex", "{nope");
    expect(loadTexCache(store)).toBeNull();
    store.setItem("lemmata.tex", JSON.stringify({ detection: { ready: true } }));
    expect(loadTexCache(store)).toBeNull();
  });
});
