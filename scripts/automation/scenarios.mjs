// Fixed scenarios for bugs that were found and fixed, each checking the
// invariants. Shared by the e2e spec (apps/desktop/e2e/specs/invariants.ts)
// and the local runner (`node scripts/automation/scenarios.mjs`).
// Each takes `lem(method, params)` and the open project's folder.
import fs from "node:fs";
import path from "node:path";

const sleep = (lem, ms) => lem("sleep", { ms });

async function check(lem, step) {
  const problems = await lem("check");
  if (problems.length) throw new Error(`${step}: ${problems.join("; ")}`);
}

async function until(lem, what, test, timeout = 15_000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    if (await test()) return;
    await sleep(lem, 200);
  }
  throw new Error(`Timed out waiting for ${what}.`);
}

export const scenarios = {
  async "keeps the PDF after switching to editor only and back"(lem) {
    await lem("click", { selector: '[title="Editor only"]' });
    await lem("click", { selector: '[title="Editor and PDF"]' });
    await until(lem, "the PDF page", () => lem("exists", { selector: ".pages .page" }));
    await check(lem, "layout round trip");
  },

  async "accepts no edits once the last tab is closed"(lem, dir) {
    const file = path.join(dir, "main.tex");
    const before = fs.readFileSync(file, "utf8");
    for (const t of (await lem("state")).tabs) await lem("click", { selector: `[aria-label="Close ${t}"]` });
    await check(lem, "close every tab");
    await lem("type", { text: "GHOST " }).catch(() => {});
    await sleep(lem, 1500);
    if (fs.readFileSync(file, "utf8") !== before) throw new Error("typing with no file open changed main.tex");
    await lem("openFile", { path: "main.tex" });
  },

  async "never overwrites a change made by another program"(lem, dir) {
    const file = path.join(dir, "main.tex");
    await lem("openFile", { path: "main.tex" });
    await lem("idle", { timeout: 90_000 });
    // Unsaved edits here, and another program writes the file before autosave runs.
    await lem("cursor", { at: "end" });
    await lem("type", { text: "\n% mine" });
    fs.appendFileSync(file, "% theirs\n");
    await until(lem, "the conflict dialog", async () => (await lem("state")).conflicts.includes("main.tex"));
    await sleep(lem, 1500); // longer than the autosave delay
    if (!fs.readFileSync(file, "utf8").includes("% theirs")) throw new Error("autosave overwrote the other program's change");
    await lem("clickText", { text: "Keep my version" });
    await lem("idle", { timeout: 90_000 });
    const after = fs.readFileSync(file, "utf8");
    if (!after.includes("% mine") || after.includes("% theirs")) throw new Error("Keep my version did not save the editor's text");
    await check(lem, "after keeping mine");
    // The other way round: the version on disk wins.
    await lem("type", { text: "\n% mine again" });
    fs.appendFileSync(file, "% theirs again\n");
    await until(lem, "the conflict dialog", async () => (await lem("state")).conflicts.includes("main.tex"));
    await lem("clickText", { text: "Load the version on disk" });
    await lem("idle", { timeout: 90_000 });
    if ((await lem("editorText")) !== fs.readFileSync(file, "utf8")) throw new Error("Load the version on disk did not load it");
    await check(lem, "after loading theirs");
  },
};

// Local runner: a debug build started by scripts/automation/launch.sh.
//   node scripts/automation/scenarios.mjs [name filter]
if (import.meta.url === `file://${process.argv[1]}`) {
  const { connect } = await import("./client.mjs");
  const app = await connect();
  const dir = (await app.call("state")).project;
  let failed = 0;
  for (const [name, run] of Object.entries(scenarios)) {
    if (process.argv[2] && !name.includes(process.argv[2])) continue;
    try {
      await run(app.call, dir);
      console.log(`ok    ${name}`);
    } catch (e) {
      failed++;
      console.log(`FAIL  ${name}: ${e.message}`);
    }
  }
  app.close();
  process.exitCode = failed ? 1 : 0;
}
