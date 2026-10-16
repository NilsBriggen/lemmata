// Fixed scenarios for bugs that were found and fixed, each checking the
// invariants. Shared by the e2e spec (apps/desktop/e2e/specs/invariants.ts)
// and the local runner (`node scripts/automation/scenarios.mjs`).
// Each takes `lem(method, params)` and the open project's folder.
import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import { fileURLToPath } from "node:url";

const fixtures = fileURLToPath(new URL("../../tests/fixtures/", import.meta.url));

/** A fresh copy of a fixture next to the project folder (no earlier build output). */
function copyFixture(dir, name) {
  const to = path.join(path.dirname(dir), `${path.basename(dir)}-${name}`);
  fs.rmSync(to, { recursive: true, force: true });
  fs.cpSync(path.join(fixtures, name), to, { recursive: true });
  return to;
}

/** A solid gray RGB PNG of any size (compresses to a few hundred KB even at 25 megapixels). */
function grayPng(w, h) {
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    for (let k = 0; k < 8; k++) n = n & 1 ? 0xedb88320 ^ (n >>> 1) : n >>> 1;
    return n >>> 0;
  });
  const crc = (buf) => {
    let c = 0xffffffff;
    for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type), data]);
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const sum = Buffer.alloc(4);
    sum.writeUInt32BE(crc(body));
    return Buffer.concat([len, body, sum]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr.set([8, 2, 0, 0, 0], 8); // 8-bit RGB
  const row = Buffer.alloc(1 + 3 * w, 0x80);
  row[0] = 0; // no filter
  const raw = Buffer.concat(Array.from({ length: h }, () => row));
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", zlib.deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

/** On screen: present, displayed and with a size. */
async function visible(lem, selector) {
  const found = await lem("query", { selector });
  return found.some((e) => e.display !== "none" && e.rect[2] > 0 && e.rect[3] > 0);
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
    if (!s.diagnostics.some((d) => d.severity === "error" && d.message.includes("undefinedcommandxyz")))
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

  async "offers commands that only the loaded packages define"(lem) {
    // Up to 2.0.1 none of texlab's commands showed: its labels have no backslash, and \\autoc never matched them.
    await lem("openFile", { path: "main.tex" });
    await lem("cursor", { line: 11 }); // inside the document: texlab offers nothing after \\end{document}
    const before = await lem("editorText");
    await lem("type", { text: "\\autoc" });
    // texlab may still be starting: ask again (Ctrl+Space) every few seconds, which cancels a pending answer.
    let polls = 0;
    await until(lem, "\\autocite from biblatex in the list", async () => {
      if ((await lem("query", { selector: ".cm-tooltip-autocomplete li" })).some((o) => (o.text ?? "").startsWith("\\autocite"))) return true;
      if (++polls % 15 === 0) await lem("key", { combo: "Control+Space" });
      return false;
    }, 60_000);
    await sleep(lem, 400); // CodeMirror ignores Enter for a moment after the list opens
    await lem("key", { combo: "Enter" });
    if (!/\n\\auto[a-z]+\\input/i.test(await lem("editorText"))) throw new Error("the command was not inserted with one backslash");
    for (let i = 0; i < 6 && (await lem("editorText")) !== before; i++) await lem("key", { combo: "Control+z" });
    await lem("key", { combo: "Control+s" });
  },

  async "switches to a file without completion while one is coming"(lem) {
    // Autocomplete's timers outlived the switch and threw "Field is not present in this state".
    for (let i = 0; i < 10; i++) {
      await lem("openFile", { path: "main.tex" });
      await lem("cursor", { line: 11 });
      const before = await lem("editorText");
      await lem("type", { text: "\\a" });
      await lem("openFile", { path: "refs.bib" });
      await sleep(lem, 300);
      const errors = await lem("errors");
      if (errors.length) throw new Error(errors[0]);
      await lem("openFile", { path: "main.tex" });
      for (let j = 0; j < 4 && (await lem("editorText")) !== before; j++) await lem("key", { combo: "Control+z" });
    }
    await lem("key", { combo: "Control+s" });
  },

  async "indents inside environments as you type, and lines \\end up with its \\begin"(lem) {
    await lem("openFile", { path: "main.tex" });
    await lem("cursor", { line: 11 });
    const before = await lem("editorText");
    await lem("type", { text: "\\begin{itemize}\n\\end{itemize}\n" });
    await lem("cursor", { line: 11 });
    await lem("key", { combo: "End" });
    await lem("key", { combo: "Enter" });
    const line = async (n) => (await lem("editorText")).split("\n")[n - 1];
    if ((await line(12)) !== "    ") throw new Error(`Enter inside itemize gave ${JSON.stringify(await line(12))}`);
    await lem("type", { text: "\\end{itemize}" });
    if ((await line(12)) !== "\\end{itemize}") throw new Error(`a typed \\end stayed at ${JSON.stringify(await line(12))}`);
    for (let i = 0; i < 8 && (await lem("editorText")) !== before; i++) await lem("key", { combo: "Control+z" });
    if ((await lem("editorText")) !== before) throw new Error("undo did not restore the file");
    await lem("key", { combo: "Control+s" });
  },

  async "\\begin opens the environment list, and the chosen one is typed once"(lem) {
    // 1.5.2 inserted \\begin{name}…\\end{name} with a cursor in each, showed no names, and kept both
    // cursors after leaving the braces, so typing went to two places.
    await lem("openFile", { path: "main.tex" });
    await lem("cursor", { at: "start" });
    const before = await lem("editorText");
    await lem("type", { text: "\\begin" });
    await until(lem, "the completion list", () => lem("exists", { selector: ".cm-tooltip-autocomplete" }));
    await sleep(lem, 400); // CodeMirror ignores Enter for a moment after the list opens
    const pick = async (test) => {
      const items = (await lem("query", { selector: ".cm-tooltip-autocomplete li" })).map((o) => o.text ?? "");
      const at = items.findIndex(test);
      if (at < 0) throw new Error(`not in the list: ${JSON.stringify(items.slice(0, 8))}`);
      for (let i = 0; i < at; i++) await lem("key", { combo: "ArrowDown" });
      await lem("key", { combo: "Enter" });
    };
    await pick((t) => t.startsWith("\\begin") && !t.startsWith("\\begin{"));
    // Only "\\begin{" so far, and the names are offered at once.
    if ((await lem("editorText")) !== "\\begin{" + before) throw new Error(`expected just \\begin{: ${(await lem("editorText")).slice(0, 40)}`);
    await until(lem, "the environment names", async () => (await lem("query", { selector: ".cm-tooltip-autocomplete li" })).some((o) => (o.text ?? "").startsWith("itemize")));
    await lem("type", { text: "itemiz" });
    await sleep(lem, 400);
    await pick((t) => t.startsWith("itemize"));
    const text = await lem("editorText");
    if (!text.includes("\\begin{itemize}") || !text.includes("\\end{itemize}")) throw new Error(`no itemize environment: ${text.slice(0, 80)}`);
    await lem("type", { text: "%x" });
    const after = await lem("editorText");
    if (after.split("%x").length !== 2) throw new Error(`typing went to more than one place: ${after.slice(0, 80)}`);
    for (let i = 0; i < 8 && (await lem("editorText")) !== before; i++) await lem("key", { combo: "Control+z" });
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
    await lem("click", { selector: '[role=dialog] [aria-label="Close"]' });
    // A dialog may take a moment to go; one still open after that names itself in the error.
    await until(lem, "the bibliography manager to close", async () => !(await lem("exists", { selector: "[role=dialog]" })), 5_000).catch(async () => {
      throw new Error(`a dialog is still open: ${JSON.stringify(await lem("query", { selector: "[role=dialog]" }))}`);
    });
    await lem("idle", { timeout: 120_000 });
    await check(lem, "after TODOs and the manager");
  },

  async "shows hover cards for citations and equations"(lem) {
    await lem("openFile", { path: "chapters/method.tex" });
    // Hover again on each try: a tooltip that was dismissed (or a panel still closing) needs a new mouse move.
    await until(lem, "the citation card", async () => {
      await lem("hoverText", { text: "schulman2017ppo" });
      return (await lem("text", { selector: ".cm-hover-card" }).catch(() => "")).includes("Proximal Policy");
    });
    // texlab answers the same hover; its note must not show next to the card.
    await sleep(lem, 800);
    const lsp = await lem("query", { selector: ".cm-lsp-hover-tooltip" });
    if (lsp.some((t) => t.rect[2] > 0)) throw new Error(`texlab's tooltip shows next to the card: ${JSON.stringify(lsp)}`);
    await lem("key", { combo: "Escape" });
    // The first "eq:objective" is the \\label itself; hover the \\ref.
    await until(lem, "the rendered equation", async () => {
      await lem("hoverText", { text: "ref{eq:objective}" });
      return lem("exists", { selector: ".cm-hover-card .katex" });
    });
    await lem("key", { combo: "Escape" });
    await lem("openFile", { path: "main.tex" });
  },

  async "underlines grammar mistakes (when a LanguageTool server runs)"(lem) {
    const url = "http://localhost:8081";
    if (await lem("invoke", { cmd: "grammar_status", args: { url } }).then(() => false, () => true)) return; // no server here
    await lem("setting", { key: "grammar", value: "local" });
    await lem("openFile", { path: "main.tex" });
    await lem("cursor", { line: 12 });
    await lem("type", { text: "This are a sentence with a mistake.\n\n" });
    await until(lem, "a grammar underline", () => lem("exists", { selector: ".cm-grammar" }), 20_000);
    for (let i = 0; i < 4; i++) await lem("key", { combo: "Control+z" });
    await lem("setting", { key: "grammar", value: "off" });
    await lem("key", { combo: "Control+s" });
    await lem("idle", { timeout: 120_000 });
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
    // Markers unique to this run: an earlier run's are still in the file.
    const [mine, theirs] = [`% mine ${Date.now()}`, `% theirs ${Date.now()}`];
    await lem("openFile", { path: "main.tex" });
    await lem("idle", { timeout: 90_000 });
    // Unsaved edits here, and another program writes the file before autosave runs.
    await lem("cursor", { at: "end" });
    await lem("type", { text: `\n${mine}` });
    fs.appendFileSync(file, `${theirs}\n`);
    await until(lem, "the conflict dialog", async () => (await lem("state")).conflicts.includes("main.tex"));
    await sleep(lem, 1500); // longer than the autosave delay
    if (!fs.readFileSync(file, "utf8").includes(theirs)) throw new Error("autosave overwrote the other program's change");
    await lem("clickText", { text: "Keep my version" });
    await lem("idle", { timeout: 90_000 });
    const after = fs.readFileSync(file, "utf8");
    if (!after.includes(mine) || after.includes(theirs)) throw new Error("Keep my version did not save the editor's text");
    await check(lem, "after keeping mine");
    // The other way round: the version on disk wins.
    await lem("type", { text: `\n${mine} again` });
    fs.appendFileSync(file, `${theirs} again\n`);
    await until(lem, "the conflict dialog", async () => (await lem("state")).conflicts.includes("main.tex"));
    await lem("clickText", { text: "Load the version on disk" });
    await lem("idle", { timeout: 90_000 });
    if ((await lem("editorText")) !== fs.readFileSync(file, "utf8")) throw new Error("Load the version on disk did not load it");
    await check(lem, "after loading theirs");
  },

  async "shows an image from the project"(lem, dir) {
    // A 4 × 3 red PNG. It used to stay at "Loading…" for good.
    const png = Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAAQAAAADCAIAAAA7ljmRAAAAEElEQVR4nGO4o6EBRww4OQAa3g4RLYR9sQAAAABJRU5ErkJggg==",
      "base64",
    );
    fs.writeFileSync(path.join(dir, "red.png"), png);
    // Retried: the file appears in the project once the watcher has seen it.
    await until(lem, "the image to show", async () => {
      await lem("openFile", { path: "red.png" }).catch(() => {});
      return (await lem("text", { selector: ".viewer" }).catch(() => "")).includes("4 × 3 px");
    });
    await check(lem, "after opening an image");
    fs.rmSync(path.join(dir, "red.png"));
    await lem("openFile", { path: "main.tex" });
  },

  async "offers the dictionary a file asks for, and turns spell check on"(lem) {
    // Needs the internet: Spanish (about 1 MB) comes from the LibreOffice dictionaries.
    await lem("openFile", { path: "main.tex" });
    await lem("cursor", { at: "start" });
    await lem("type", { text: "% !TeX spellcheck = es\n" });
    await until(lem, "the offer to download", async () => ((await lem("state")).notice ?? "").includes("needs its dictionary")).catch(async () => {
      throw new Error(`no offer to download; latest notices: ${JSON.stringify((await lem("state")).notices)}`);
    });
    await lem("clickText", { text: "Download", within: ".toasts" });
    // The dictionary itself, not a notice (a later one can replace it); a failure says why.
    const installed = async () => (await lem("invoke", { cmd: "spell_available" })).find((d) => d.code === "es")?.installed;
    await until(lem, "the Spanish dictionary", installed, 90_000).catch(async () => {
      throw new Error(`the Spanish dictionary wasn't installed; last notice: ${(await lem("state")).notice}`);
    });
    await lem("key", { combo: "Control+z" });
    await lem("key", { combo: "Control+s" });
    await lem("idle", { timeout: 120_000 });
    await check(lem, "after downloading a dictionary");
  },

  async "renames and deletes the open file from the file menu"(lem, dir) {
    fs.writeFileSync(path.join(dir, "notes.tex"), "Notes.\n");
    await until(lem, "notes.tex in the tree", () => lem("exists", { selector: '.tree-scroll [data-path="notes.tex"]' }));
    await lem("openFile", { path: "notes.tex" });
    await lem("cursor", { at: "end" });
    await lem("type", { text: "Not saved yet.\n" });
    // Rename while the edit is unsaved: it must survive, saved under the new name.
    await lem("contextmenu", { selector: '.tree-scroll [data-path="notes.tex"]' });
    await lem("clickText", { text: "Rename", within: ".tree-menu" });
    // The dialog opens once the references to the file are found.
    await until(lem, "the rename dialog", () => lem("exists", { selector: "#fa-name" }));
    await lem("fill", { selector: "#fa-name", value: "renamed.tex" });
    await lem("clickText", { text: "Rename", within: "[role=dialog]" });
    await until(lem, "the renamed tab", async () => (await lem("state")).tabs.includes("renamed.tex"));
    await lem("idle", { timeout: 60_000 });
    if (fs.existsSync(path.join(dir, "notes.tex"))) throw new Error("the old name is still on disk");
    if (!fs.readFileSync(path.join(dir, "renamed.tex"), "utf8").includes("Not saved yet.")) throw new Error("the unsaved edit was lost in the rename");
    await check(lem, "after renaming the open file");
    await lem("contextmenu", { selector: '.tree-scroll [data-path="renamed.tex"]' });
    await lem("clickText", { text: "Delete", within: ".tree-menu" });
    // Deleting into the project's trash can be undone, so it doesn't ask; the system trash asks first.
    const asked = () => lem("exists", { selector: "[role=dialog]" });
    await until(lem, "the delete to happen or ask", async () => (await asked()) || !(await lem("state")).tabs.includes("renamed.tex"));
    if (await asked()) await lem("clickText", { text: "Delete", within: "[role=dialog]" });
    await until(lem, "the tab to close", async () => !(await lem("state")).tabs.includes("renamed.tex"));
    if (fs.existsSync(path.join(dir, "renamed.tex"))) throw new Error("the deleted file is still there");
    await check(lem, "after deleting the open file");
    await lem("openFile", { path: "main.tex" });
  },

  async "stops a build and saves twice in a row"(lem, dir) {
    await lem("openFile", { path: "main.tex" });
    await lem("cursor", { at: "end" });
    await lem("type", { text: "\n% stop and save" });
    await lem("command", { id: "recompile" });
    await until(lem, "the build to start", async () => (await lem("state")).building, 10_000).catch(() => {});
    if ((await lem("state")).building) await lem("clickText", { text: "Stop" });
    await lem("key", { combo: "Control+s" });
    await lem("key", { combo: "Control+s" });
    await lem("idle", { timeout: 120_000 });
    if (!fs.readFileSync(path.join(dir, "main.tex"), "utf8").includes("% stop and save")) throw new Error("the edit wasn't saved");
    await check(lem, "after stopping and saving twice");
    await lem("key", { combo: "Control+z" });
    await lem("key", { combo: "Control+s" });
    await lem("idle", { timeout: 120_000 });
  },

  async "restores a snapshot without unsaved edits coming back"(lem, dir) {
    const file = path.join(dir, "main.tex");
    const original = fs.readFileSync(file, "utf8");
    await lem("openFile", { path: "main.tex" });
    await lem("cursor", { at: "start" });
    await lem("type", { text: "% version one\n" });
    await lem("key", { combo: "Control+s" });
    await lem("idle", { timeout: 120_000 });
    await lem("command", { id: "snapshot" });
    await lem("fill", { selector: "#snapshot-label", value: "Version one" });
    await lem("clickText", { text: "Save snapshot" });
    await lem("cursor", { at: "start" });
    await lem("type", { text: "% version two\n" });
    await lem("key", { combo: "Control+s" });
    await lem("idle", { timeout: 120_000 });
    // Typed but not saved when the restore starts.
    await lem("type", { text: "% not saved\n" });
    await lem("command", { id: "history" });
    await until(lem, "the snapshot", async () => (await lem("text", { selector: "aside.history" }).catch(() => "")).includes("Version one"));
    await lem("clickText", { text: "Version one", within: "aside.history" });
    await lem("clickText", { text: "Restore project", within: "aside.history" });
    await lem("clickText", { text: "Restore", within: "aside.history .tools" });
    await until(lem, "the restored text", async () => !(await lem("editorText")).includes("% version two"));
    await sleep(lem, 1500); // past the autosave delay: nothing may write the old edits back
    const disk = fs.readFileSync(file, "utf8");
    if (!disk.includes("% version one") || disk.includes("% version two") || disk.includes("% not saved")) throw new Error(`the file isn't the snapshot's: ${disk.slice(0, 120)}`);
    if ((await lem("editorText")) !== disk) throw new Error("the editor doesn't show the restored file");
    await check(lem, "after restoring a snapshot");
    await lem("click", { selector: 'aside.history [aria-label="Close history"]' });
    fs.writeFileSync(file, original);
    await until(lem, "the original text back", async () => (await lem("editorText")) === original);
    await lem("idle", { timeout: 120_000 });
  },

  async "reviews the changes since a snapshot and undoes one"(lem, dir) {
    const file = path.join(dir, "main.tex");
    const original = fs.readFileSync(file, "utf8");
    await lem("openFile", { path: "main.tex" });
    await lem("command", { id: "snapshot" });
    await lem("fill", { selector: "#snapshot-label", value: "Before review" });
    await lem("clickText", { text: "Save snapshot" });
    await lem("cursor", { at: "end" });
    await lem("type", { text: "\n% a change to undo" });
    await lem("key", { combo: "Control+s" });
    await lem("idle", { timeout: 120_000 });
    await lem("command", { id: "history" });
    await until(lem, "the snapshot", async () => (await lem("text", { selector: "aside.history" }).catch(() => "")).includes("Before review"));
    const option = (await lem("query", { selector: "aside.history .review option" })).find((o) => (o.text ?? "").startsWith("Before review"));
    if (!option) throw new Error("the snapshot isn't offered for review");
    await lem("fill", { selector: "aside.history .review select", value: option.value });
    await lem("clickText", { text: "Review changes", within: "aside.history .review" });
    await until(lem, "the change to review", async () => (await lem("exists", { selector: "aside.history .hunk" })));
    await lem("clickText", { text: "Reject", within: "aside.history .hunk" });
    await until(lem, "the change undone", async () => !fs.readFileSync(file, "utf8").includes("% a change to undo"));
    if ((await lem("editorText")).includes("% a change to undo")) throw new Error("the editor still shows the rejected change");
    await check(lem, "after rejecting a change");
    await lem("click", { selector: 'aside.history [aria-label="Close history"]' });
    if (fs.readFileSync(file, "utf8") !== original) {
      fs.writeFileSync(file, original);
      await until(lem, "the original text back", async () => (await lem("editorText")) === original);
    }
    await lem("idle", { timeout: 120_000 });
  },

  async "zooms the PDF, not the whole window, on a touchpad pinch"(lem, dir) {
    // Linux only: WebKitGTK magnified the whole window. Other systems have no made-up pinch to send.
    const pinch = async ([x, y, w, h], scale) => {
      for (const [phase, s] of [[0, 1], [1, scale], [2, scale]])
        await lem("invoke", { cmd: "automation_pinch", args: { phase, scale: s, x: x + w / 2, y: y + h / 2 } });
    };
    const rect = async (selector) => (await lem("query", { selector }))[0].rect;
    const zoom = async () => parseInt((await lem("query", { selector: ".zoom" }))[0].text);
    await lem("openProject", { path: copyFixture(dir, "article") }); // no side panel over the PDF
    await lem("idle", { timeout: 120_000 });
    if ((await lem("state")).logsOpen) await lem("click", { selector: ".bar .logs" });
    await until(lem, "the PDF", () => lem("exists", { selector: ".pages .page canvas" }), 120_000);
    const body = JSON.stringify(await rect("body"));
    const before = await zoom();
    try {
      await pinch(await rect(".cm-content"), 2);
    } catch {
      return lem("openProject", { path: dir }); // not Linux
    }
    await sleep(lem, 500);
    if (JSON.stringify(await rect("body")) !== body) throw new Error("a pinch on the editor magnified the window");
    if ((await zoom()) !== before) throw new Error("a pinch on the editor zoomed the PDF");
    await pinch(await rect(".scroller"), 1.5);
    await until(lem, "the PDF zoomed in", async () => Math.abs((await zoom()) - before * 1.5) <= 2);
    if (JSON.stringify(await rect("body")) !== body) throw new Error("a pinch on the PDF magnified the window");
    await lem("openProject", { path: dir });
  },

  async "draws a very large image after another large one"(lem, dir) {
    // The WebKitGTK of Ubuntu 22.04 (and so the AppImage) drew the second one blank:
    // 6 then 25 megapixels, like a circuit diagram after a wide one.
    const big = path.join(path.dirname(dir), `${path.basename(dir)}-big-images`);
    fs.rmSync(big, { recursive: true, force: true });
    fs.mkdirSync(big);
    fs.writeFileSync(path.join(big, "wide.png"), grayPng(4488, 1400));
    fs.writeFileSync(path.join(big, "tall.png"), grayPng(4912, 5208));
    fs.writeFileSync(
      path.join(big, "main.tex"),
      "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\n\\noindent\\includegraphics[width=\\linewidth]{wide.png}\n" +
        "\\newpage\n\\noindent\\includegraphics[width=\\linewidth]{tall.png}\n\\end{document}\n",
    );
    await lem("openProject", { path: big });
    const s = await lem("idle", { timeout: 120_000 });
    if (s.status !== "success") throw new Error(`the build failed: ${await describe(lem)}`);
    const canvas = '.page[data-page="2"] canvas';
    await until(lem, "page 2 drawn", async () => {
      await lem("click", { selector: '[aria-label="Next page"]' }).catch(() => {});
      return lem("exists", { selector: canvas });
    }, 30_000);
    // The image fills the text width from the top margin: its middle is gray, not white.
    await until(lem, "the image on page 2", async () => (await lem("ink", { selector: canvas, area: [0.3, 0.2, 0.7, 0.4] })) > 0.9, 15_000);
    await lem("openProject", { path: dir });
    await lem("idle", { timeout: 120_000 });
    fs.rmSync(big, { recursive: true, force: true });
  },

  async "opens every side panel where it can be seen, in every layout"(lem) {
    await lem("openFile", { path: "main.tex" });
    const panels = { git: "aside.git", todos: ".todos", comments: "aside.review", history: "aside.history" };
    for (const layout of ["editor", "split", "pdf", "focus"]) {
      for (const [id, selector] of Object.entries(panels)) {
        await lem("setting", { key: "layout", value: layout === "focus" ? "split" : layout });
        // Opening a panel leaves focus mode, so this turns it on again each time.
        if (layout === "focus") await lem("command", { id: "focus" });
        await lem("command", { id });
        await until(lem, `${id} to show in ${layout}`, () => visible(lem, selector), 5_000);
      }
    }
    await lem("click", { selector: '[aria-label="Close history"]' });
    // The file tree from "PDF only": revealing the open file brings the sidebar back.
    await lem("setting", { key: "layout", value: "pdf" });
    await lem("command", { id: "reveal" });
    await until(lem, "the file tree", () => visible(lem, "aside.sidebar"), 5_000);
    await lem("setting", { key: "layout", value: "split" });
    await check(lem, "after opening the panels");
  },

  async "puts controls where people look"(lem) {
    await lem("openFile", { path: "main.tex" });
    await lem("idle", { timeout: 120_000 });
    await until(lem, "the PDF", async () => (await lem("state")).hasPdf, 30_000);
    if (await lem("exists", { selector: '[aria-label="Dark pages"]' })) throw new Error("the dark-pages button is back in the PDF toolbar");
    const toolbar = await lem("text", { selector: "header.toolbar" });
    if (!toolbar.includes("Git")) throw new Error(`no Git button in the toolbar: ${toolbar}`);
    // The word count in the status bar opens the breakdown.
    await lem("clickText", { text: " words", within: "footer.status" });
    await until(lem, "the word count", async () => (await lem("text", { selector: "[role=dialog]" }).catch(() => "")).includes("Word"));
    await lem("key", { combo: "Escape" });
    // At the default width the PDF toolbar's extra buttons are in its ⋯ menu, Present among them.
    // (The toolbar shows only Logs while the logs are open.)
    if ((await lem("state")).logsOpen) await lem("clickText", { text: "Logs" });
    await lem("setting", { key: "pdfWidth", value: 560 });
    await until(lem, "the ⋯ menu", () => visible(lem, '[aria-label="More"]'), 5_000);
    await lem("click", { selector: '[aria-label="More"]' });
    await until(lem, "Present in the ⋯ menu", async () => (await lem("text", { selector: ".menu" }).catch(() => "")).includes("Present full screen"));
    await lem("key", { combo: "Escape" });
    // Dark PDF pages can't be switched on where they would do nothing.
    await lem("setting", { key: "theme", value: "light" });
    await lem("command", { id: "preferences" });
    await until(lem, "the preference greyed out", () => lem("exists", { selector: "#pref-pdfdark:disabled" }));
    await lem("key", { combo: "Escape" });
    await lem("setting", { key: "theme", value: "system" });
  },

  async "keeps the cursor's indentation when inserting a list or a figure"(lem, dir) {
    fs.writeFileSync(path.join(dir, "figure.png"), grayPng(8, 8));
    fs.writeFileSync(path.join(dir, "indent.tex"), "\\begin{itemize}\n    \\item first\n\\end{itemize}\n");
    await until(lem, "indent.tex in the tree", () => lem("exists", { selector: '.tree-scroll [data-path="indent.tex"]' }));
    await lem("openFile", { path: "indent.tex" });
    const inserted = (before, after) => after.split("\n").slice(2, 2 + after.split("\n").length - before.split("\n").length);
    // After text on an indented line: a new line, and every line of the list indented like it.
    let before = await lem("editorText");
    await lem("cursor", { line: 2 });
    await lem("key", { combo: "End" });
    await lem("click", { selector: '[aria-label="Bulleted list"]' });
    let lines = inserted(before, await lem("editorText"));
    if (!lines.length || lines.some((l) => !l.startsWith("    "))) throw new Error(`the list lost the indentation: ${JSON.stringify(lines)}`);
    // The figure dialog writes plain text: same rule.
    before = await lem("editorText");
    await lem("cursor", { line: 2 });
    await lem("key", { combo: "End" });
    await lem("command", { id: "figure" });
    await until(lem, "the figure dialog", () => lem("exists", { selector: "#figure-form" }));
    await lem("clickText", { text: "Insert", within: "[role=dialog]" });
    await until(lem, "the figure", async () => (await lem("editorText")).includes("\\begin{figure}"));
    lines = inserted(before, await lem("editorText"));
    if (lines.some((l) => l.trim() && !l.startsWith("    "))) throw new Error(`the figure lost the indentation: ${JSON.stringify(lines)}`);
    await lem("key", { combo: "Control+s" });
    await lem("idle", { timeout: 120_000 });
    await lem("openFile", { path: "main.tex" });
    fs.rmSync(path.join(dir, "indent.tex"));
    fs.rmSync(path.join(dir, "figure.png"));
  },

  async "restores a deleted file from Recently deleted after reopening"(lem, dir) {
    await lem("setting", { key: "trashMode", value: "project" });
    fs.writeFileSync(path.join(dir, "gone.tex"), "Keep me.\n");
    await until(lem, "gone.tex in the tree", () => lem("exists", { selector: '.tree-scroll [data-path="gone.tex"]' }));
    await lem("contextmenu", { selector: '.tree-scroll [data-path="gone.tex"]' });
    await lem("clickText", { text: "Delete", within: ".tree-menu" });
    await until(lem, "the file deleted", async () => !fs.existsSync(path.join(dir, "gone.tex")));
    // Reopening forgets the undo log, like a restart.
    await lem("openProject", { path: dir });
    await lem("idle", { timeout: 120_000 });
    await lem("command", { id: "deleted" });
    await until(lem, "the list", async () => (await lem("text", { selector: "[role=dialog]" }).catch(() => "")).includes("gone.tex"));
    await lem("clickText", { text: "Restore", within: "[role=dialog]" });
    await until(lem, "the file back", async () => fs.existsSync(path.join(dir, "gone.tex")));
    if (fs.readFileSync(path.join(dir, "gone.tex"), "utf8") !== "Keep me.\n") throw new Error("the restored file differs");
    await lem("clickText", { text: "Close", within: "[role=dialog]" });
    fs.rmSync(path.join(dir, "gone.tex"));
  },

  async "offers a way forward on an unknown citation or label"(lem) {
    await lem("openFile", { path: "chapters/method.tex" });
    await until(lem, "Search online on the unknown key", async () => {
      await lem("hoverText", { text: "nobody2020" });
      return (await lem("text", { selector: ".cm-hover-card" }).catch(() => "")).includes("Search online");
    });
    await lem("clickText", { text: "Search online", within: ".cm-hover-card" });
    await until(lem, "the citation picker with the key", () => lem("exists", { selector: "[role=dialog]" }));
    await lem("key", { combo: "Escape" });
    await until(lem, "Search the project on the unknown label", async () => {
      await lem("hoverText", { text: "sec:missing" });
      return (await lem("text", { selector: ".cm-hover-card" }).catch(() => "")).includes("Search the project");
    });
    await lem("clickText", { text: "Search the project", within: ".cm-hover-card" });
    await until(lem, "the search for it", async () => (await lem("query", { selector: ".search-wrap input" })).some((i) => i.value === "sec:missing"));
    await lem("openFile", { path: "main.tex" });
  },

  async "puts projects into folders by dragging them, or from their menu"(lem, dir) {
    const copy = copyFixture(dir, "article");
    const name = path.basename(copy);
    await lem("openProject", { path: copy });
    await lem("idle", { timeout: 120_000 });
    await lem("command", { id: "close" });
    await until(lem, "the library", () => lem("exists", { selector: ".library" }));
    await lem("setting", { key: "libraryView", value: "list" });
    const rows = async () => (await lem("query", { selector: ".library .tr:not(.th)" })).map((r) => r.text ?? "");
    const folderRow = async () => (await rows()).find((t) => t.includes("Course")) ?? "";
    await lem("clickText", { text: "New folder" });
    await lem("fill", { selector: "[aria-label='Folder name']", value: "Course" });
    await lem("clickText", { text: "Create", within: "[role=dialog]" });
    await until(lem, "the folder", async () => (await folderRow()).includes("0 projects"));
    // The ⋯ button sits centred in its row (a stray .ghost style once pinned it lower, unclickable).
    const [rowRect] = (await lem("query", { selector: ".library .tr.folder" })).map((r) => r.rect);
    const [moreRect] = (await lem("query", { selector: ".library .tr.folder .more" })).map((r) => r.rect);
    if (Math.abs(moreRect[1] + moreRect[3] / 2 - (rowRect[1] + rowRect[3] / 2)) > 4) throw new Error(`the folder's ⋯ button isn't centred in its row: ${moreRect} vs ${rowRect}`);

    await lem("drag", { from: ".library .tr:not(.th):not(.folder)", fromText: name, to: ".library .tr.folder", toText: "Course" });
    await until(lem, "the dragged project in the folder", async () => (await folderRow()).includes("1 project"));
    if ((await rows()).some((t) => t.includes(name))) throw new Error("the project is still listed outside its folder");
    await lem("clickText", { text: "Course", within: ".library .table" });
    await until(lem, "the folder's projects", async () => (await rows()).some((t) => t.includes(name)));
    await lem("drag", { from: ".library .tr:not(.th)", fromText: name, to: ".crumb" });
    await until(lem, "an empty folder", async () => (await rows()).length === 0);
    await lem("click", { selector: ".crumb" });
    await until(lem, "the project back at the top", async () => (await rows()).some((t) => t.includes(name)));

    // The same without dragging: the project's menu.
    const row = `.library .tr:not(.th):not(.folder)`;
    await lem("fill", { selector: "#library-search", value: name });
    await until(lem, "only that project listed", async () => (await rows()).length === 1);
    await lem("contextmenu", { selector: row });
    await lem("clickText", { text: "Move to folder" });
    await lem("clickText", { text: "Course", within: "[role=dialog]" });
    await lem("fill", { selector: "#library-search", value: "" });
    await until(lem, "the moved project in the folder", async () => (await folderRow()).includes("1 project"));
    await lem("contextmenu", { selector: ".library .tr.folder" });
    await lem("clickText", { text: "Delete folder" });
    await until(lem, "the folder gone, its project back", async () => !(await folderRow()) && (await rows()).some((t) => t.includes(name)));
    await lem("openProject", { path: dir });
    await lem("idle", { timeout: 120_000 });
  },

  async "deletes a project from the start screen"(lem, dir) {
    const copy = copyFixture(dir, "article");
    await lem("openProject", { path: copy });
    await lem("idle", { timeout: 120_000 });
    await lem("command", { id: "close" });
    await until(lem, "the library", () => lem("exists", { selector: ".library" }));
    await lem("fill", { selector: "#library-search", value: path.basename(copy) });
    await until(lem, "only that project listed", async () => (await lem("query", { selector: ".library .table .tr:not(.th), .library .grid .card" })).length === 1);
    await lem("contextmenu", { selector: ".library .table .tr:not(.th), .library .grid .card" });
    await lem("clickText", { text: "Delete project" });
    await lem("clickText", { text: "Move to trash" });
    await until(lem, "the project gone", async () => !fs.existsSync(copy), 15_000).catch(async () => {
      throw new Error(`the project folder is still there; the dialog says: ${await lem("text", { selector: "[role=dialog]" }).catch(() => "(no dialog)")}`);
    });
    await until(lem, "the list without it", async () => (await lem("query", { selector: ".library .table .tr:not(.th), .library .grid .card" })).length === 0);
    await lem("fill", { selector: "#library-search", value: "" });
    await lem("openProject", { path: dir });
    await lem("idle", { timeout: 120_000 });
  },

  async "turns a project into a template from the start screen"(lem, dir) {
    const copy = copyFixture(dir, "article");
    const name = `Template ${path.basename(copy)}`;
    await lem("openProject", { path: copy });
    await lem("idle", { timeout: 120_000 });
    await lem("command", { id: "close" });
    await until(lem, "the library", () => lem("exists", { selector: ".library" }));
    await lem("fill", { selector: "#library-search", value: path.basename(copy) });
    await until(lem, "only that project listed", async () => (await lem("query", { selector: ".library .table .tr:not(.th), .library .grid .card" })).length === 1);
    await lem("contextmenu", { selector: ".library .table .tr:not(.th), .library .grid .card" });
    await lem("clickText", { text: "Save as template" });
    await until(lem, "the dialog", () => lem("exists", { selector: "#st-name" }));
    await lem("fill", { selector: "#st-name", value: name });
    await lem("click", { selector: ".check input" });
    await lem("clickText", { text: "Save template" });
    await until(lem, "the project in the trash", async () => !fs.existsSync(copy), 15_000);
    await lem("fill", { selector: "#library-search", value: "" });
    await lem("clickText", { text: "New project" });
    await until(lem, "My templates", () => lem("exists", { selector: ".gallery .folder" }));
    await lem("clickText", { text: "My templates", within: ".gallery" });
    const del = `[aria-label="Delete template ${name}"]`;
    await until(lem, "the new template", () => lem("exists", { selector: del }));
    await lem("click", { selector: del });
    await until(lem, "the template deleted", async () => !(await lem("exists", { selector: del })));
    await lem("clickText", { text: "Cancel" });
    await lem("openProject", { path: dir });
    await lem("idle", { timeout: 120_000 });
  },

  async "leaves a presentation cleanly when Esc comes at once"(lem) {
    await lem("openFile", { path: "main.tex" });
    await lem("idle", { timeout: 120_000 });
    await until(lem, "the PDF", async () => (await lem("state")).hasPdf, 30_000);
    // The PDF's toolbar is hidden while the logs are shown.
    if ((await lem("state")).logsOpen) await lem("click", { selector: ".bar .logs" });
    // Esc before going full screen has finished must not leave the window full screen.
    await lem("click", { selector: '[aria-label="Present full screen"]' });
    await lem("key", { combo: "Escape" });
    await sleep(lem, 1500);
    if (await lem("exists", { selector: ".stage .count" })) throw new Error("the presentation is still showing");
    if (await lem("fullscreen")) throw new Error("the window stayed full screen");
    await check(lem, "after leaving a presentation");
  },

  async "shows What's new once after an update"(lem, dir) {
    // During a reload the app can't answer, so wait here, not through the app.
    const reload = async () => {
      await lem("reload");
      await new Promise((r) => setTimeout(r, 5000));
    };
    await lem("setting", { key: "lastSeenVersion", value: "1.4.0" });
    await sleep(lem, 600); // settings are written after a short delay
    await reload();
    await until(lem, "the What's new window", async () => (await lem("text", { selector: "[role=dialog]" }).catch(() => "")).includes("What's new"));
    await lem("clickText", { text: "Got it", within: "[role=dialog]" });
    await until(lem, "the window closed", async () => !(await lem("exists", { selector: "[role=dialog]" })));
    await sleep(lem, 600);
    await reload();
    if (await lem("exists", { selector: "[role=dialog]" })) throw new Error("What's new showed a second time");
    await lem("openProject", { path: dir });
    await lem("idle", { timeout: 120_000 });
  },

  async "creates a project from the template gallery and builds it"(lem, dir) {
    const name = `${path.basename(dir)}-gallery`;
    fs.rmSync(path.join(path.dirname(dir), name), { recursive: true, force: true });
    await lem("command", { id: "close" });
    await until(lem, "the library", () => lem("exists", { selector: ".library" }));
    await lem("clickText", { text: "New project" });
    await until(lem, "the categories", () => lem("exists", { selector: ".cats .cat" }));
    await lem("clickText", { text: "School", within: ".cats" });
    await until(lem, "the School templates", () => lem("exists", { selector: ".templates [role=radio]" }));
    await lem("clickText", { text: "Homework", within: ".templates" });
    await lem("fill", { selector: "#np-name", value: name });
    await lem("fill", { selector: "#np-parent", value: path.dirname(dir) });
    await lem("clickText", { text: "Create project" });
    await until(lem, "the new project", async () => (await lem("state")).project?.endsWith(name), 30_000);
    const project = path.join(path.dirname(dir), name);
    await lem("idle", { timeout: 120_000 });
    await until(lem, "the new project's PDF", async () => (await lem("state")).hasPdf, 30_000).catch(async () => {
      // What the build left behind says more than the app's state.
      const out = path.join(project, ".lemmata/build");
      const log = fs.existsSync(path.join(out, "main.log")) ? fs.readFileSync(path.join(out, "main.log"), "utf8").slice(-1500) : "(no log)";
      const files = fs.existsSync(out) ? fs.readdirSync(out).join(", ") : "(no build folder)";
      throw new Error(`the new project did not build: ${await describe(lem)}\nbuild folder: ${files}\nlog tail: ${log}`);
    });
    if ((await lem("state")).status !== "success") throw new Error(`the new project's build failed: ${await describe(lem)}`);
    await check(lem, "after creating from a template");
    await lem("openProject", { path: dir });
    await lem("idle", { timeout: 120_000 });
  },

  async "commits a saved edit from the Git panel"(lem, dir) {
    const copy = copyFixture(dir, "article");
    await lem("openProject", { path: copy });
    await lem("idle", { timeout: 120_000 });
    await lem("command", { id: "git" });
    const panel = () => lem("text", { selector: "aside.git" });
    await until(lem, "the Git panel", async () => (await lem("exists", { selector: "aside.git" })) && /git repository|Changes|No changes/.test(await panel()));
    // Never commit into a repository the temp folder happens to sit in.
    if (!(await panel()).includes("isn't in a git repository")) throw new Error(`the copy is already in a repository: ${await panel()}`);
    await lem("clickText", { text: "Turn this project into a git repository", within: "aside.git" });
    await until(lem, "the repository", async () => /now a git repository|doesn't know who you are/.test(await panel()));
    if ((await panel()).includes("doesn't know who you are")) throw new Error("git has no user name and email here; set them (git config --global) to run this scenario");
    await lem("openFile", { path: "main.tex" });
    await lem("cursor", { at: "start" });
    await lem("type", { text: "% committed from the panel\n" });
    await lem("key", { combo: "Control+s" });
    await until(lem, "the saved edit in the panel", async () => (await panel()).includes("main.tex"));
    await lem("clickText", { text: "Stage all", within: "aside.git" });
    await until(lem, "the staged file", async () => (await panel()).includes("Commit 1 file"));
    await lem("fill", { selector: 'aside.git textarea[aria-label="Commit message"]', value: "Edit from the panel" });
    await lem("click", { selector: "aside.git .commitbox button" });
    await until(lem, "the commit", async () => {
      const t = await panel();
      return t.includes("Edit from the panel") && t.includes("No changes");
    });
    await check(lem, "after committing");
    await lem("click", { selector: 'aside.git [aria-label="Close Git"]' });
    await lem("openProject", { path: dir });
    await lem("idle", { timeout: 120_000 });
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
