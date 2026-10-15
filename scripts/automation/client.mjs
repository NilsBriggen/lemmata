// Talks to a debug build of Lemmata started with LEMMATA_AUTOMATION=1
// (see apps/desktop/src-tauri/src/automation.rs). Development only.
import net from "node:net";
import os from "node:os";
import path from "node:path";

export function address() {
  const v = process.env.LEMMATA_AUTOMATION;
  if (v && v !== "1") return v;
  if (process.platform === "win32") return "\\\\.\\pipe\\lemmata-automation";
  return path.join(process.env.XDG_RUNTIME_DIR || os.tmpdir(), "lemmata-automation.sock");
}

/** Opens one connection; `call(method, params)` resolves with the result or rejects with the app's error. */
export function connect(addr = address()) {
  return new Promise((resolve, reject) => {
    const socket = net.createConnection(addr);
    const waiting = new Map();
    let next = 1;
    let buffer = "";
    socket.setEncoding("utf8");
    socket.on("data", (chunk) => {
      buffer += chunk;
      let nl;
      while ((nl = buffer.indexOf("\n")) >= 0) {
        const msg = JSON.parse(buffer.slice(0, nl));
        buffer = buffer.slice(nl + 1);
        const w = waiting.get(msg.id);
        waiting.delete(msg.id);
        if (w) msg.error ? w.reject(new Error(msg.error)) : w.resolve(msg.result);
      }
    });
    socket.on("error", (e) => {
      reject(new Error(`Can't reach Lemmata at ${addr} (${e.message}). Start a debug build with LEMMATA_AUTOMATION=1.`));
      for (const w of waiting.values()) w.reject(e);
    });
    socket.on("connect", () =>
      resolve({
        call: (method, params = {}) =>
          new Promise((res, rej) => {
            const id = next++;
            waiting.set(id, { resolve: res, reject: rej });
            socket.write(JSON.stringify({ id, method, params }) + "\n");
          }),
        close: () => socket.end(),
      }),
    );
  });
}
