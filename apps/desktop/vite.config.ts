/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri expects a fixed port and doesn't want the screen cleared.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  // Desktop app loaded from disk, so one big chunk is fine.
  build: { target: "es2022", chunkSizeWarningLimit: 2000 },
  test: { environment: "node" },
});
