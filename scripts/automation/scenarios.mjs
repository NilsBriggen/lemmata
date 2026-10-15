// Fixed scenarios for bugs that were found and fixed, each checking the
// invariants. Shared by the e2e spec (apps/desktop/e2e/specs/invariants.ts)
// and the local runner (`node scripts/automation/scenarios.mjs`).
// Each takes `lem(method, params)` and the open project's folder.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const fixtures = fileURLToPath(new URL("../../tests/fixtures/", import.meta.url));

/** A fresh copy of a fixture next to the project folder (no earlier build output). */
function copyFixture(dir, name) {
  const to = path.join(path.dirname(dir), `${path.basename(dir)}-${name}`);
  fs.rmSync(to, { recursive: true, force: true });
  fs.cpSync(path.join(fixtures, name), to, { recursive: true });
  return to;
}

/** The app's state in a failure message, so CI logs say what the app was doing. */
const describe = async (lem) => JSON.stringify(await lem("state"));

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
  async "builds a project from scratch and shows its PDF"(lem, dir) {
    fs.rmSync(path.join(dir, ".lemmata"), { recursive: true, force: true });
    await lem("openProject", { path: dir });
    const s = await lem("idle", { timeout: 120_000 });
    if (s.status !== "success" || !s.hasPdf) throw new Error(`the build did not show a PDF: ${await describe(lem)}`);
    await until(lem, "a drawn PDF page", () => lem("exists", { selector: ".pages .page canvas" }), 30_000);
    if (!(await lem("editorText")).includes("\\documentclass")) throw new Error("the main file isn't in the editor");
    await check(lem, "first build");
  },

  async "lists a new error after an edit, and clears it after undo"(lem) {
    await lem("openFile", { path: "main.tex" });
    await lem("cursor", { at: "start" });
    await lem("type", { text: "\\undefinedcommandxyz\n" });
    await lem("key", { combo: "Control+s" });
    await sleep(lem, 300);
    let s = await lem("idle", { timeout: 120_000 });
    if (!s.diagnostics.some((d) => d.severity === "error" && /Undefined control sequence/.test(d.message)))
      throw new Error(`the error wasn't listed: ${await describe(lem)}`);
    await lem("key", { combo: "Control+z" });
    await lem("key", { combo: "Control+s" });
    await sleep(lem, 300);
    s = await lem("idle", { timeout: 120_000 });
    if (s.status !== "success") throw new Error(`undoing the error didn't fix the build: ${await describe(lem)}`);
    await check(lem, "after undo");
  },

  async "explains a missing package"(lem, dir) {
    const other = copyFixture(dir, "missing-package");
    await lem("openProject", { path: other });
    const s = await lem("idle", { timeout: 120_000 });
    if (!s.diagnostics.some((d) => d.message.includes("lemmata-does-not-exist.sty")))
      throw new Error(`the missing package wasn't named: ${await describe(lem)}`);
    // No PDF, so the logs open by themselves, like Overleaf.
    if (!s.logsOpen) throw new Error("the logs didn't open for a build without a PDF");
    await lem("openProject", { path: dir });
    await lem("idle", { timeout: 120_000 });
  },

  async "completes an environment together with its \\end"(lem) {
    await lem("openFile", { path: "main.tex" });
    await lem("cursor", { at: "start" });
    const before = await lem("editorText");
    await lem("type", { text: "\\begin{itemiz" });
    await until(lem, "the completion list", () => lem("exists", { selector: ".cm-tooltip-autocomplete" }));
    await sleep(lem, 400); // CodeMirror ignores Enter for a moment after the list opens
    await lem("key", { combo: "Enter" });
    if (!(await lem("editorText")).includes("\\end{itemize}")) throw new Error("\\end{itemize} was not added");
    for (let i = 0; i < 6 && (await lem("editorText")) !== before; i++) await lem("key", { combo: "Control+z" });
    await lem("key", { combo: "Control+s" });
    await lem("idle", { timeout: 120_000 });
  },

  async "jumps from the PDF to the source"(lem) {
    await until(lem, "a drawn PDF page", () => lem("exists", { selector: ".pages .page canvas" }), 30_000);
    // On page 1, the text between 31% and 40% of the height comes from chapters/method.tex (checked with `synctex edit`).
    await lem("dblclickAt", { selector: ".pages .page", x: 0.34, y: 0.36 });
    await until(lem, "chapters/method.tex to open", async () => (await lem("state")).active === "chapters/method.tex").catch(async (e) => {
      throw new Error(`${e.message} ${await describe(lem)}`);
    });
  },

  async "adds, shows and resolves a review comment"(lem, dir) {
    await lem("openFile", { path: "main.tex" });
    await lem("cursor", { line: 12 });
    await lem("command", { id: "comment" });
    await until(lem, "the comment form", () => lem("exists", { selector: ".review .draft textarea" }));
    await lem("fill", { selector: ".review .draft textarea", value: "Is this the right citation?" });
    await lem("click", { selector: '.review .draft button[type="submit"]' });
    await until(lem, "the highlight", () => lem("exists", { selector: ".cm-comment" }));
    const saved = JSON.parse(fs.readFileSync(path.join(dir, ".lemmata/comments.json"), "utf8"));
    if (saved.threads?.[0]?.body !== "Is this the right citation?") throw new Error(`comment not saved: ${JSON.stringify(saved)}`);
    await lem("clickText", { text: "Resolve", within: ".review" });
    await until(lem, "the highlight to go", async () => !(await lem("exists", { selector: ".cm-comment" })));
    await lem("click", { selector: '[aria-label="Close comments"]' });
    await check(lem, "after resolving");
  },

  async "lists TODOs and opens the bibliography manager"(lem) {
    await lem("openFile", { path: "main.tex" });
    await lem("cursor", { line: 12 });
    await lem("type", { text: "% TODO tighten this paragraph\n" });
    await lem("key", { combo: "Control+s" });
    await lem("idle", { timeout: 120_000 });
    await lem("command", { id: "todos" });
    await until(lem, "the TODO", async () => (await lem("text", { selector: ".todos" }).catch(() => "")).includes("tighten this paragraph"));
    await lem("click", { selector: '[aria-label="Close TODOs"]' });
    await lem("key", { combo: "Control+z" });
    await lem("key", { combo: "Control+s" });
    await lem("command", { id: "bib-manager" });
    await until(lem, "the bibliography manager", async () => (await lem("text", { selector: "[role=dialog]" }).catch(() => "")).includes("schulman2017ppo"));
    await lem("key", { combo: "Escape" });
    await lem("idle", { timeout: 120_000 });
    await check(lem, "after TODOs and the manager");
  },

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
