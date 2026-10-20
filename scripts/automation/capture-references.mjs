// Screenshots of the Svelte UI (`pnpm dev:mock` on :1420) as the visual spec for the egui app.
// Writes apps/lemmata/tests/references/<shot>-<light|dark>.png at the default window size, 1400×900.
//
//   CHROME=/path/to/chrome-headless-shell node scripts/automation/capture-references.mjs [shot-name-filter]
//
// Talks the DevTools protocol over Node's built-in WebSocket, so it needs no npm packages.
import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const BASE = "http://localhost:1420/";
const OUT = "apps/lemmata/tests/references";

const LIBRARY = { title: "Back to all projects" };

/** Each shot starts from a fresh page with cleared storage; steps click buttons by text, aria-label or title. */
const SHOTS = [
  { name: "welcome-1", query: "?mock=first-run" },
  { name: "welcome-2", query: "?mock=first-run", steps: [{ text: "Continue" }] },
  { name: "welcome-3", query: "?mock=first-run", steps: [{ text: "Continue" }, { text: "Continue" }] },
  { name: "welcome-new-project", query: "?mock=first-run", steps: [{ text: "Continue" }, { text: "Continue" }, { text: "New project" }] },
  { name: "onboarding", query: "?mock=no-tex" },
  // Startup reopens the latest project, so this is the editor; the brand goes back to the library.
  { name: "editor-split", query: "" },
  { name: "library-list", query: "", steps: [LIBRARY] },
  { name: "library-grid", query: "", steps: [LIBRARY, { label: "Grid" }] },
  { name: "library-new-folder", query: "", steps: [LIBRARY, { text: "New folder" }] },
  { name: "library-project-menu", query: "", steps: [LIBRARY, { label: "Actions for rl-thesis" }] },
  { name: "library-new-project", query: "", steps: [LIBRARY, { text: "New project" }] },
  { name: "library-clone", query: "", steps: [LIBRARY, { text: "Clone a repository" }] },
  { name: "library-new-project-papers", query: "", steps: [LIBRARY, { text: "New project" }, { text: "Papers" }] },
  { name: "library-import", query: "", steps: [LIBRARY, { text: "Import from Overleaf" }] },
  { name: "library-save-template", query: "", steps: [LIBRARY, { label: "Actions for rl-thesis" }, { text: "Save as template" }] },
];

const chrome = process.env.CHROME;
if (!chrome) throw new Error("set CHROME to a Chromium or chrome-headless-shell binary");
const filter = process.argv[2] ?? "";
mkdirSync(OUT, { recursive: true });

const profile = mkdtempSync(join(tmpdir(), "lemmata-refs-"));
const proc = spawn(chrome, ["--headless", "--hide-scrollbars", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "--window-size=1400,900", "about:blank"], { stdio: "ignore" });
const portFile = join(profile, "DevToolsActivePort");
while (!existsSync(portFile) || !readFileSync(portFile, "utf8").includes("\n")) await sleep(50);
const port = readFileSync(portFile, "utf8").split("\n")[0];
const target = await (await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, { method: "PUT" })).json();

const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((ok) => ws.addEventListener("open", ok, { once: true }));
let seq = 0;
const pending = new Map();
const waiters = [];
ws.addEventListener("message", (e) => {
  const msg = JSON.parse(e.data);
  if (msg.id && pending.has(msg.id)) {
    const { ok, fail } = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error ? fail(new Error(msg.error.message)) : ok(msg.result);
  } else if (msg.method) {
    for (const w of waiters.splice(0)) if (w.method === msg.method) w.ok(); else waiters.push(w);
  }
});
const send = (method, params = {}) => new Promise((ok, fail) => { const id = ++seq; pending.set(id, { ok, fail }); ws.send(JSON.stringify({ id, method, params })); });
const once = (method) => new Promise((ok) => waiters.push({ method, ok }));
const evaluate = async (expression) => (await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true })).result.value;

await send("Page.enable");
await send("Emulation.setDeviceMetricsOverride", { width: 1400, height: 900, deviceScaleFactor: 1, mobile: false });

for (const shot of SHOTS.filter((s) => s.name.includes(filter))) {
  for (const scheme of ["light", "dark"]) {
    await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }, { name: "prefers-reduced-motion", value: "reduce" }] });
    await navigate(BASE);
    await evaluate("localStorage.clear(), true");
    await navigate(BASE + shot.query);
    for (const step of shot.steps ?? []) {
      const found = await evaluate(`(() => {
        const want = ${JSON.stringify(step)};
        const el = [...document.querySelectorAll("button, [role=menuitem]")].find((b) =>
          want.label ? b.getAttribute("aria-label") === want.label
          : want.title ? b.title === want.title
          : b.textContent.trim().startsWith(want.text));
        el?.click();
        return !!el;
      })()`);
      if (!found) throw new Error(`${shot.name}: no button ${JSON.stringify(step)}`);
      await settle();
    }
    const { data } = await send("Page.captureScreenshot", { format: "png" });
    writeFileSync(join(OUT, `${shot.name}-${scheme}.png`), Buffer.from(data, "base64"));
    console.log(`${shot.name}-${scheme}`);
  }
}
ws.close();
proc.kill();

async function navigate(url) {
  const loaded = once("Page.loadEventFired");
  await send("Page.navigate", { url });
  await loaded;
  await settle();
}

async function settle() {
  await evaluate("document.fonts.ready.then(() => new Promise((r) => requestAnimationFrame(() => setTimeout(r, 400)))).then(() => true)");
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}
