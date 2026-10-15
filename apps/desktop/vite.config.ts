/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { fileURLToPath } from "node:url";

// `vite --mode mock` swaps the Tauri modules for fakes in src/mock, so the
// interface can be previewed in a normal browser. Release builds never use it.
const mocks = (mode: string) =>
  mode === "mock"
    ? Object.fromEntries(
        Object.entries({
          "@tauri-apps/api/core": "tauri-core",
          "@tauri-apps/api/event": "tauri-event",
          "@tauri-apps/api/app": "tauri-app",
          "@tauri-apps/api/path": "tauri-path",
          "@tauri-apps/api/webview": "tauri-webview",
          "@tauri-apps/api/window": "tauri-window",
          "@tauri-apps/api/webviewWindow": "tauri-webviewWindow",
          "@tauri-apps/plugin-dialog": "plugin-dialog",
          "@tauri-apps/plugin-store": "plugin-store",
          "@tauri-apps/plugin-opener": "plugin-opener",
          "@tauri-apps/plugin-process": "plugin-process",
        }).map(([from, to]) => [from, fileURLToPath(new URL(`./src/mock/${to}.ts`, import.meta.url))]),
      )
    : {};

// Tauri's webviews all read WOFF2, so drop the WOFF and TTF fallbacks that
// KaTeX and @fontsource declare. A regex miss leaves the CSS untouched.
const woff2Only = {
  name: "woff2-only",
  enforce: "pre" as const,
  transform(code: string, id: string) {
    if (!/node_modules\/(katex|@fontsource\/[^/]+)\/.*\.css$/.test(id)) return null;
    return code.replace(/,\s*url\([^)]*\.(?:woff|ttf)\)\s*format\(["']?(?:woff|truetype)["']?\)/g, "");
  },
};

export default defineConfig(({ mode }) => ({
  plugins: [woff2Only, svelte()],
  resolve: { alias: mocks(mode) },
  // Tauri expects a fixed port and doesn't want the screen cleared.
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  // Desktop app loaded from disk, so one big chunk is fine.
  build: { target: "es2022", chunkSizeWarningLimit: 2000 },
  test: { environment: "node" },
}));
