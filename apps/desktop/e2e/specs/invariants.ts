// Drives the app through its test automation API (src/lib/automation.ts,
// present because CI builds with VITE_LEMMATA_TEST=1) and checks the
// invariants after every step: fixed scenarios for known bugs, then a seeded
// random walk. A failing walk prints its seed; rerun it with E2E_SEED=<seed>.
import { browser } from "@wdio/globals";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
// @ts-expect-error: plain JavaScript modules shared with the local runners
import { walk } from "../../../../scripts/automation/walk.mjs";
// @ts-expect-error: see above
import { scenarios } from "../../../../scripts/automation/scenarios.mjs";

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
    // WebDriver stops a script after 30 s by default, which cut long calls like idle short; they bound their own waits.
    await browser.setTimeout({ script: 300_000 });
    await browser.waitUntil(() => browser.execute(() => !!(window as any).__lemmata), { timeout: 30_000 });
    await lem("openProject", { path: dir });
    await lem("idle", { timeout: 90_000 });
    await check("open");
  });

  for (const [name, run] of Object.entries(scenarios)) {
    it(name, () => run(lem, dir));
  }

  it("holds every invariant on a random walk", async () => {
    const seed = Number(process.env.E2E_SEED) || Date.now() % 1_000_000;
    await walk(lem, { seed, steps: 40 });
  });
});
