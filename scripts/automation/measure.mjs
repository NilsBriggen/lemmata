#!/usr/bin/env node
// Rough timings of the running test app (debug build: compare runs, not absolute numbers).
//   node scripts/automation/measure.mjs <project folder> [runs]
import { connect } from "./client.mjs";

const [dir, runs = "3"] = process.argv.slice(2);
const app = await connect();
const ms = (t) => Math.round(performance.now() - t);
const median = (xs) => xs.sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const open = [];
const build = [];
for (let i = 0; i < Number(runs); i++) {
  let t = performance.now();
  await app.call("openProject", { path: dir });
  await app.call("idle", { timeout: 120000 });
  open.push(ms(t));
  await app.call("openFile", { path: "main.tex" }); // the edit must go into the document, not whatever tab was active
  await app.call("cursor", { at: "end" });
  await app.call("type", { text: "\n%" });
  t = performance.now();
  await app.call("key", { combo: "Control+s" });
  await app.call("sleep", { ms: 50 });
  await app.call("idle", { timeout: 120000 });
  build.push(ms(t));
}
console.log(JSON.stringify({ openToIdleMs: median(open), saveToBuiltMs: median(build), runs: Number(runs) }));
app.close();
