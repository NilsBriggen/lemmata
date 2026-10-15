# Test automation (development only)

Debug and test builds of Lemmata can be driven by scripts. Release builds contain none of this: the frontend part is behind a build-time flag (CI checks the normal bundle for `__lemmata`), and the Rust bridge is compiled only with `debug_assertions`.

- `apps/desktop/src/lib/automation.ts` adds `window.__lemmata.call(method, params)`. The actions go through the UI: DOM clicks and keys, typing into CodeMirror, the command palette's commands. `state` describes the app, and `check` returns every broken invariant (uncaught errors, saved files that differ from the disk, a missing PDF, an editable editor with no file open). `methods` lists the rest.
- It is present in `pnpm tauri dev`, the browser mock (`pnpm dev:mock`), and any build made with `VITE_LEMMATA_TEST=1`.
- `apps/desktop/src-tauri/src/automation.rs`: with `LEMMATA_AUTOMATION=1`, a debug build listens on `$XDG_RUNTIME_DIR/lemmata-automation.sock` (a named pipe on Windows) for one JSON request per line. It has no network port. `LEMMATA_AUTOMATION=<path>` picks another socket.

## Running it

```bash
VITE_LEMMATA_TEST=1 pnpm --filter lemmata-desktop tauri build --debug --no-bundle
LEMMATA_AUTOMATION=1 target/debug/lemmata-app
```

Point `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `XDG_CACHE_HOME` at a scratch folder to keep your own settings and projects out of it.

```bash
node scripts/automation/drive.mjs state
node scripts/automation/drive.mjs openProject '{"path":"/tmp/article"}'
node scripts/automation/walk.mjs 42 100
```

- `drive.mjs` calls one method.
- `walk.mjs` does a seeded random walk, starting from an open copy of `tests/fixtures/article`, and checks the invariants after every step.
- `mcp.mjs` is the developer MCP server registered in `.mcp.json`, so Claude Code can drive the running app.

In CI the e2e job builds with `VITE_LEMMATA_TEST=1`, and `apps/desktop/e2e/specs/invariants.ts` runs the same walk over WebDriver. A failure prints its seed; replay it with `E2E_SEED=<seed>`, or locally with `walk.mjs <seed>`.
