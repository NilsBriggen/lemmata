// End-to-end tests: drive the real app through tauri-driver (WebKitWebDriver
// on Linux, Edge WebDriver on Windows). See .github/workflows/ci.yml (e2e).
import { spawn, type ChildProcess } from "node:child_process";
import path from "node:path";
import os from "node:os";

const root = path.resolve(import.meta.dirname, "../../..");
const exe = path.join(root, "target", "debug", process.platform === "win32" ? "lemmata-app.exe" : "lemmata-app");
let driver: ChildProcess | undefined;

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: ["./specs/**/*.ts"],
  maxInstances: 1,
  hostname: "127.0.0.1",
  port: 4444,
  capabilities: [{ "tauri:options": { application: exe } } as WebdriverIO.Capabilities],
  logLevel: "warn",
  framework: "mocha",
  reporters: ["spec"],
  // Each scenario bounds its own waits; this only catches a hung one. A dictionary download plus a MiKTeX rebuild on
  // Windows can take longer than 2 minutes.
  mochaOpts: { ui: "bdd", timeout: 300_000 },
  waitforTimeout: 60_000,
  // Longer than any wait inside a call (idle takes up to 2 minutes), so a stuck call reports the app's own error.
  connectionRetryTimeout: 360_000,
  beforeSession() {
    const bin = path.join(os.homedir(), ".cargo", "bin", process.platform === "win32" ? "tauri-driver.exe" : "tauri-driver");
    // On Windows, tauri-driver needs an Edge WebDriver matching WebView2 (CI downloads one into MSEDGEDRIVER).
    const native = process.env.MSEDGEDRIVER ?? (process.env.EDGEWEBDRIVER && path.join(process.env.EDGEWEBDRIVER, "msedgedriver.exe"));
    const edge = process.platform === "win32" && native ? ["--native-driver", native] : [];
    driver = spawn(bin, edge, { stdio: [null, process.stdout, process.stderr] });
  },
  afterSession() {
    driver?.kill();
  },
};
