// A seeded random walk through the app that checks the invariants after
// every step. Shared by the e2e spec (WebDriver, apps/desktop/e2e/specs/
// invariants.ts) and the local runner (`node scripts/automation/walk.mjs`).
// `lem(method, params)` calls window.__lemmata in the app.

import fs from "node:fs";
import path from "node:path";

/** mulberry32: small, seeded, good enough to pick actions. */
export function random(seed) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Runs `steps` random actions on the article fixture; throws with the seed and the steps taken on the first broken invariant. */
export async function walk(lem, { seed, steps = 40, log = () => {} }) {
  const rnd = random(seed);
  const pick = (xs) => xs[Math.floor(rnd() * xs.length)];
  const files = ["main.tex", "chapters/method.tex", "refs.bib"];
  const words = ["alpha", "beta", "gamma", "delta", "epsilon"];
  const actions = {
    type: async () => {
      await lem("cursor", { line: 12 + Math.floor(rnd() * 3) });
      await lem("type", { text: ` ${pick(words)}` });
    },
    save: () => lem("key", { combo: "Control+s" }),
    open: () => lem("openFile", { path: pick(files) }),
    close: async () => {
      const { active } = await lem("state");
      const selector = `[aria-label="Close ${active}"]`;
      // The tab strip is hidden in PDF-only layout.
      if (active && (await lem("exists", { selector }))) await lem("click", { selector });
    },
    layout: () => lem("click", { selector: `[title="${pick(["Editor only", "Editor and PDF", "PDF only"])}"]` }),
    recompile: () => lem("command", { id: "recompile" }),
    undo: () => lem("key", { combo: "Control+z" }),
    // Another program edits a file: the watcher reloads it (or, with unsaved edits, says so).
    external: async () => {
      const { project } = await lem("state");
      fs.appendFileSync(path.join(project, pick(files)), `% edited outside ${Math.floor(rnd() * 1000)}\n`);
      await lem("sleep", { ms: 1500 });
    },
  };
  const names = Object.keys(actions);
  const taken = [];
  const check = async (step) => {
    const problems = await lem("check");
    if (problems.length) throw new Error(`${step}: ${problems.join("; ")}`);
  };
  try {
    for (let i = 0; i < steps; i++) {
      const name = pick(names);
      taken.push(name);
      log(`${i} ${name}`);
      await actions[name]();
      await lem("sleep", { ms: 150 });
      await check(`step ${i} (${name})`);
    }
    await lem("click", { selector: '[title="Editor and PDF"]' });
    await lem("idle", { timeout: 90_000 });
    await check("after the walk settled");
  } catch (e) {
    throw new Error(`seed ${seed}, steps ${taken.join(" → ")}: ${e.message}`);
  }
}

// Local runner: drives a debug build started with LEMMATA_AUTOMATION=1 and the article fixture open.
//   node scripts/automation/walk.mjs [seed] [steps]
if (import.meta.url === `file://${process.argv[1]}`) {
  const { connect } = await import("./client.mjs");
  const app = await connect();
  const seed = Number(process.argv[2]) || Date.now() % 1_000_000;
  try {
    await walk(app.call, { seed, steps: Number(process.argv[3]) || 40, log: (s) => process.stdout.write(`${s}  `) });
    console.log(`\nseed ${seed}: all invariants held`);
  } catch (e) {
    console.error(`\n${e.message}`);
    process.exitCode = 1;
  } finally {
    app.close();
  }
}
