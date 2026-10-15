#!/usr/bin/env node
// Developer-only MCP server (stdio) for driving a debug build of Lemmata
// started with LEMMATA_AUTOMATION=1. Never shipped with the app.
// Speaks the small part of MCP that tools need: initialize, tools/list, tools/call.
import readline from "node:readline";
import { connect } from "./client.mjs";

const tools = [
  {
    name: "lemmata_call",
    description:
      "Run one automation method in the running Lemmata app. Methods: state, check, errors, methods, commands, command {id}, openProject {path}, openFile {path}, click {selector}, contextmenu {selector}, clickText {text, within?}, fill {selector, value}, key {combo}, type {text}, cursor {line|at}, editorText, text {selector}, exists {selector}, query {selector}, idle {timeout?}, sleep {ms}, hoverText {text}, invoke {cmd, args?}, setting {key, value}, storage {key, value}, reload, closeWindow.",
    inputSchema: {
      type: "object",
      properties: { method: { type: "string" }, params: { type: "object" } },
      required: ["method"],
    },
  },
  { name: "lemmata_state", description: "The app's state: project, tabs, active and unsaved files, build status, diagnostics, layout, open dialogs.", inputSchema: { type: "object", properties: {} } },
  { name: "lemmata_check", description: "Invariants that must always hold; returns the ones that are broken (empty list = all good).", inputSchema: { type: "object", properties: {} } },
];

let app = null;
async function call(method, params) {
  app ??= await connect();
  return app.call(method, params);
}

async function handle(msg) {
  switch (msg.method) {
    case "initialize":
      return { protocolVersion: msg.params?.protocolVersion ?? "2025-06-18", capabilities: { tools: {} }, serverInfo: { name: "lemmata-dev", version: "0.1.0" } };
    case "tools/list":
      return { tools };
    case "tools/call": {
      const { name, arguments: args = {} } = msg.params;
      try {
        const result =
          name === "lemmata_state" ? await call("state") : name === "lemmata_check" ? await call("check") : await call(args.method, args.params ?? {});
        return { content: [{ type: "text", text: JSON.stringify(result, null, 2) }] };
      } catch (e) {
        app = null;
        return { content: [{ type: "text", text: e.message }], isError: true };
      }
    }
    case "ping":
      return {};
    default:
      throw Object.assign(new Error(`Unknown method ${msg.method}`), { code: -32601 });
  }
}

const send = (obj) => process.stdout.write(JSON.stringify({ jsonrpc: "2.0", ...obj }) + "\n");
readline.createInterface({ input: process.stdin }).on("line", async (line) => {
  if (!line.trim()) return;
  const msg = JSON.parse(line);
  if (msg.id === undefined) return; // a notification, e.g. notifications/initialized
  try {
    send({ id: msg.id, result: await handle(msg) });
  } catch (e) {
    send({ id: msg.id, error: { code: e.code ?? -32603, message: e.message } });
  }
});
