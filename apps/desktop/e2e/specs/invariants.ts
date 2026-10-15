// Drives the app through its test automation API (src/lib/automation.ts,
// present because CI builds with VITE_LEMMATA_TEST=1) and checks the
// invariants after every step: fixed scenarios for known bugs, then a seeded
// random walk. A failing walk prints its seed; rerun it with E2E_SEED=<seed>.
import { browser, expect } from "@wdio/globals";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
// @ts-expect-error: plain JavaScript module shared with the local runner
import { walk } from "../../../../scripts/automation/walk.mjs";

const fixture = path.resolve(import.meta.dirname, "../../../../tests/fixtures/article");

const lem = <T = unknown>(method: string, params: object = {}) =>
  browser.execute(
    async (m: string, p: object) => {
      const api = (window as any).__lemmata;
      if (!api) throw new Error("No automation API: build with VITE_LEMMATA_TEST=1.");
      return api.call(m, p);
    },
    method,
    params,
  ) as Promise<T>;

const check = async (step: string) => {
  const problems = await lem<string[]>("check");
  if (problems.length) throw new Error(`${step}: ${problems.join("; ")}`);
};

describe("invariants", () => {
  let dir: string;

  before(async () => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), "lemmata-e2e-"));
    fs.cpSync(fixture, dir, { recursive: true });
    await browser.waitUntil(() => browser.execute(() => !!(window as any).__lemmata), { timeout: 30_000 });
    await lem("openProject", { path: dir });
    await lem("idle", { timeout: 90_000 });
    await check("open");
  });

  it("keeps the PDF after switching to editor only and back", async () => {
    await lem("click", { selector: '[title="Editor only"]' });
    await lem("click", { selector: '[title="Editor and PDF"]' });
    await browser.waitUntil(() => lem<boolean>("exists", { selector: ".pages .page" }), { timeout: 15_000 });
    await check("layout round trip");
  });

  it("accepts no edits once the last tab is closed", async () => {
    const before = fs.readFileSync(path.join(dir, "main.tex"), "utf8");
    const { tabs } = await lem<{ tabs: string[] }>("state");
    for (const t of tabs) await lem("click", { selector: `[aria-label="Close ${t}"]` });
    await check("close every tab");
    await lem("type", { text: "GHOST " }).catch(() => {});
    await lem("sleep", { ms: 1500 });
    expect(fs.readFileSync(path.join(dir, "main.tex"), "utf8")).toBe(before);
    await lem("openFile", { path: "main.tex" });
  });

  it("holds every invariant on a random walk", async () => {
    const seed = Number(process.env.E2E_SEED) || Date.now() % 1_000_000;
    await walk(lem, { seed, steps: 40 });
  });
});
