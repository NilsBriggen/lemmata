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
  mochaOpts: { ui: "bdd", timeout: 120_000 },
  waitforTimeout: 60_000,
  beforeSession() {
    const bin = path.join(os.homedir(), ".cargo", "bin", process.platform === "win32" ? "tauri-driver.exe" : "tauri-driver");
    driver = spawn(bin, [], { stdio: [null, process.stdout, process.stderr] });
  },
  afterSession() {
    driver?.kill();
  },
};
