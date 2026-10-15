#!/usr/bin/env node
// Drives a running debug build of Lemmata (started with LEMMATA_AUTOMATION=1).
//
//   node scripts/automation/drive.mjs state
//   node scripts/automation/drive.mjs click '{"selector":".tab.on .close"}'
//   node scripts/automation/drive.mjs check
//
// `methods` lists what the app understands (apps/desktop/src/lib/automation.ts).
import { connect } from "./client.mjs";

const [method = "methods", params = "{}"] = process.argv.slice(2);
const app = await connect();
try {
  const result = await app.call(method, JSON.parse(params));
  console.log(JSON.stringify(result, null, 2));
} catch (e) {
  console.error(e.message);
  process.exitCode = 1;
} finally {
  app.close();
}
