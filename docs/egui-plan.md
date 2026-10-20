# Lemmata: pure egui + wgpu UI (branch `egui`)

## Context
Lemmata v2.1.2 is a Tauri 2 shell around a Svelte 5 / CodeMirror 6 / pdf.js / KaTeX frontend (19k lines of TS/Svelte, 3.8 MB `dist`, 33.5 MB stripped binary, plus a 14.6 MB texlab sidecar). The goal is the smallest and fastest possible app with the **same design and the same feature set**: no webview, no JS, one Rust binary using egui on wgpu.

The 12 crates under `crates/` have no UI or Tauri dependencies and stay untouched. Only the `apps/desktop` frontend and the Tauri glue are replaced. Nothing is released until parity is complete (project rule: no rc or release with known gaps).

**Decisions already made:** PDF engine is `hayro` (pure Rust), gated by a spike with `mupdf` as the fallback. texlab is downloaded on first use (pinned and checksummed), not bundled. Renderer is wgpu, as requested.

## Branch and layout
- `git switch -c egui` from `main` (matches the `2.0` precedent). Conventional commits, no Claude attribution (per user CLAUDE.md).
- `apps/desktop` (Tauri) stays in place until parity, then is deleted in one commit. Not refactored to share code with the new app, because that churn is wasted when it gets deleted.
- New workspace members:
  - `crates/lemmata-shell`: the UI-independent logic that today lives only in the Tauri shell, with the `tauri::State/AppHandle/emit` types removed. Sources: `src-tauri/src/{files,projects,tools,open_with,online_templates,lsp,terminal,figures,export}.rs` (about 4k lines: project search/replace, rename/move with reference updates, the in-project trash and undo, notify watcher with own-write suppression, template listing, argv and `lemmata://` parsing, texlab JSON-RPC framing).
  - `apps/lemmata` (binary `lemmata-app`): the egui app.
- `apps/lemmata/src/` modules:
  - `app.rs`: root state and layout. This is what `App.svelte` (2336 lines) did. It is split into one file per panel from the start.
  - `theme.rs`, `icons.rs`, `widgets/` (Button, IconButton, Segmented, Modal, DropMenu, Splitter, Toast, Kbd, Badge)
  - `editor/`: buffer, view, highlight, fold, complete, hover, spell, grammar, comments, vim, math
  - `pdf/`: render, textlayer, view, thumbnails, outline, presenter
  - `panels/`: toolbar, sidebar, tree, outline, tabs, status, problems, git, todos, review, history, search, library, welcome, onboarding, prefs, project_settings, cite, bib, figure, table, symbols, snippets, palette, shortcuts, whatsnew, wordcount
  - `backend.rs`: the replacement for the ~128 Tauri commands. It is direct calls into the crates plus `lemmata-shell`. A tokio runtime runs on a background thread. Results come back over an mpsc channel that calls `ctx.request_repaint()`. The `Emitter` events (`build-progress`, `files-changed`, `lsp-message`, `update-progress`, `open-request`) become channel messages.
  - `settings.rs`, `session.rs`: a serde/toml file in the config dir replaces plugin-store (`settings.json`, per-project sessions, recents, groups).

## Size and speed levers (decided up front, measured at every milestone)
- eframe with `default-features = false`: features `wgpu`, `winit` platform bits and `accesskit` only. Turn off `default_fonts` (saves the bundled Ubuntu/Noto/emoji fonts). `wgpu_no_default_features` with only vulkan, metal and dx12 (no GLES, no WebGPU).
- Release profile: `opt-level = 3` (speed matters as much as size), `lto = "fat"`, `codegen-units = 1`, `panic = "abort"` (check nothing relies on `catch_unwind`), `strip = true`. Thin LTO is currently set only in `release.yml`; move all of it into `Cargo.toml`.
- Render only on demand (egui's default reactive mode). Virtualize the editor: lay out only the visible lines.
- Fonts: IBM Plex Sans 400/500/600, Plex Mono 400/500 and Young Serif, latin and latin-ext subsets only (the same range the web bundle ships), converted from woff2 to ttf with a one-time `pyftsubset` script. Lazy system-font fallback through `fontdb` for non-Latin text, because the webview did this implicitly.
- Icons: build the 88 Lucide icons used today into **one icon font** (about 20 KB) with a codepoint table in `icons.rs`. No SVG rasterizer, no `resvg` (about 1 MB saved).
- Dependencies: audit `cargo tree -d` and `cargo bloat` after the app compiles. Candidates to trim later, measured and not guessed: `reqwest` to `ureq`, tokio feature set, `regex` features.
- Dictionaries (3.6 MB bundled; 14 more download) and the 6 bundled templates stay as they are. texlab downloads on first use, reusing the dictionary download and checksum code path. The checksums are already pinned in `scripts/fetch-texlab.sh`.
- Expectation to report honestly at M0: wgpu is heavier than glow. The size win comes mostly from dropping the 3.8 MB bundle, texlab and the Tauri/webview runtime, not from egui being tiny.

## Design 1:1
`theme.rs` holds every token from `app.css` as `Color32` constants for light and dark (surfaces, borders, ink, chrome, accent, status, editor, PDF ground), radii 4/6/10, shadows (`epaint::Shadow`), the focus ring, control metrics (buttons 30/26/38, inputs 32, status bar 26, sidebar 240, PDF pane 560, editor minimum 320), font sizes (base 13/1.45, labels 11px 600 uppercase +0.06em, editor 13/1.65) and `color-mix` results precomputed. A dark toolbar and sidebar ("chrome") apply in both themes. Hover transition is 0.12 s through `animate_bool`, and animations are disabled for reduced motion. `docs/pitch.html` is the design reference.

**How parity is checked, not eyeballed:** the existing `pnpm dev:mock` build (vite `--mode mock`, scenarios `?mock=first-run|no-tex|errors|empty`) is an executable visual spec. Capture reference screenshots of each screen at fixed viewport sizes in light and dark with the built-in browser. Then render the same screens in the egui app with `egui_kittest` snapshots (wgpu feature) and compare side by side. A screen is only done when the two match.

## The three hard parts

### 1. The code editor (largest piece; egui `TextEdit` has none of this)
Custom widget on `ropey`, painting one `LayoutJob` per visible line.
- Per-file state (buffer, undo history, selections, folds), because each tab keeps its own.
- Multi-cursor and rectangular (Alt-drag) selection, select-next (`Mod-d`), bracket matching, auto-close for `( [ { $`.
- Own soft-wrap with **hanging indent** (epaint only indents the first row), indent guides, indent unit from settings, environment-aware indent and the `\begin`/`\end` sync, auto-close on Enter and the unmatched-environment lint.
- Line numbers, active-line highlight, fold gutter, lint gutter.
- In-file search panel (`Mod-f`/`Mod-g`) and goto line.
- Highlighting: port the CodeMirror `stex` legacy tokenizer (about 200 lines) with the same six tag styles. No tree-sitter, to save size.
- Folding of sections, environments and `%` blocks (per-file state remembered).
- Completion popup with sources for environments, commands (about 95), labels, citation keys, file paths, packages, math symbols (with glyph) and user snippets, plus texlab results over LSP. Snippets with tab stops and `endSnippetCursors`.
- Hover cards: `\ref` (label context, equation, figure), `\cite` (entry and online lookup), `\includegraphics` and `\input` (file preview).
- Spell underlines (wavy `--error`), incremental per line over the visible range plus 50 lines, with a suggestion menu and Add to dictionary, driven by `lemmata-spell`. Grammar underlines (dotted `--accent`) with the LanguageTool tooltip and Ignore rule. Diagnostics (F8/Shift+F8), comment ranges mapped through edits.
- Paste handlers (PNG paste becomes a figure, TSV paste becomes a tabular with an undo-to-plain notice), the formatting shortcuts (wrap, comment toggle, heading level, wrap in environment), move/duplicate/delete line, reindent, `format_tex`.
- Vim keymap: an in-house modal state machine (normal, insert, visual, operators, motions, registers, `.`, basic ex commands) behind the `keymap: "vim"` setting. Scheduled last; it is its own mini-project and part of parity.
- Reuse and port the logic tests: the ~30 vitest files in `src/lib` (completions, environments, folding, indent, snippets, mathContext, hover, shortcuts, spell/prose, symbols, table, session, settings, groups, recent, todos, treeModel, fuzzy, hunks) become Rust unit tests. They are the behaviour spec.
- Move the TS mirrors `spell/prose.ts` and `grammar/text.ts` onto the Rust originals (`lemmata-project` `prose.rs`) so there is one implementation.
- History diff/hunks: `similar` or `imara-diff` replaces the `@codemirror/merge` `Chunk.build` usage.

### 2. PDF (hayro)
- Rasterize pages on a worker thread, upload as wgpu textures at the current zoom and DPI, with a cache and cancel on zoom change. Lazy thumbnails.
- Single page, spread and spread-first layouts, fit and zoom, trackpad pinch through `zoom_delta()` (drops the GTK-only `pinch.rs`), dark-page mode (`invert(0.92) hue-rotate(180deg)`) as a small shader or a tint in the texture upload.
- Text layer for selection and search, link annotations, outline, via the `hayro-syntax` and `hayro-interpret` APIs. This is the least certain part and the reason for the spike.
- SyncTeX: `lemmata-synctex` already works in PDF points. The new work is the pixel-to-point mapping, the click-to-source handler and the highlight box.
- Page-1 thumbnails for the library through the same renderer (replaces `save_thumbnail`/`load_thumbnail` and the pdf.js path).
- Presenter: a second egui viewport with notes, next slide, timer and clock, synced by channel. Monitor placement and fullscreen through winit.
- **Spike (Milestone 1, go/no-go):** render `tests/fixtures` PDFs and a Beamer deck with hayro and compare against pdf.js (fidelity, speed for a 60-page thesis, text extraction coordinates). If it fails, switch to `mupdf` and say so before continuing.

### 3. Math (KaTeX replacement)
Used by the hover preview, the symbol palette and the completion glyphs. Spike RaTeX (KaTeX-compatible, pure Rust) against `latex-rust` (TeX-faithful box layout, draws into egui) on the symbol list in `writing/symbols.ts` and the existing math tests. Choose the one that covers the symbol palette and is smaller. Both need a math font subset (about 100 KB). Preamble `\newcommand` macros (`collectMacros`) are expanded before layout, and errors show the message and stale state as today. Pre-render to cached textures keyed by source and size.

## Other replacements for Tauri features
| Tauri | Replacement |
|---|---|
| Window, min size 900x560, 1400x900 default | eframe/winit; boot splash becomes the first frame |
| File dialogs | `rfd` |
| Clipboard and image paste | egui clipboard, `arboard` for images |
| OS file drops | `raw.dropped_files` plus hover state (library and tree drag are in-app) |
| Opener / relaunch | `open` crate (keep the `lemmata-env::host` AppImage trick), `current_exe` spawn |
| Single instance, `lemmata://`, `.tex` association | `single-instance`/`interprocess` forwarding argv; reuse the `open_with.rs` parser with the `url` crate; own `.desktop`/MIME, Windows registry and macOS Info.plist entries from the packager. macOS Apple Events (`RunEvent::Opened`) need a winit workaround |
| Updater (minisign, `latest.json`) | Small self-updater that verifies the same minisign signatures against the same manifest URL, so existing CI signing and `finalize-release.py` keep working |
| Bundler (AppImage, deb, rpm, dmg, nsis, msi) | `cargo-packager` for the same formats and the same asset names (the website, AUR, Homebrew and winget jobs depend on them) |
| Deep-link/store plugins | Config file in `directories` paths |
| Presenter window | second viewport |

## Tests and automation
- `cargo test --workspace` unchanged. Ported vitest cases become Rust tests next to the new modules.
- Replace WebdriverIO/tauri-driver with **`egui_kittest`** (AccessKit queries, in-process, headless, and it covers macOS, which tauri-driver never did). Port the 40 scenarios in `scripts/automation/scenarios.mjs` and the `check` invariants one by one; the intents are the spec. Keep the seeded random walk (`walk.mjs`).
- Keep the `lemmata-dev` MCP stack (`mcp.mjs`, `client.mjs`, the Unix socket or named pipe). Replace `automation.rs` + `window.eval` + `automation.ts` with an in-process server that answers `state`, `check`, `click`, `fill`, `key`, `type`, `query`, `text`, `invoke`, `setting` and so on by AccessKit label or role. It only exists in debug builds with `LEMMATA_AUTOMATION=1`.
- Drop: `dev:mock`, `.claude/launch.json` `ui-preview` (after the visual-reference captures are done), `wdio.conf.ts`, `invariants.ts`, `tauri-driver`, `webkit2gtk-driver`, `xvfb` and msedgedriver CI steps, the `__lemmata` bundle grep, Node and pnpm CI steps.
- CI: keep fmt, clippy `-D warnings`, `tex-integration`, `package-cli.sh`, audit; swap the Tauri build for the egui build and a kittest job on the three OSes.

## Milestones (each ends compiling with tests green and a size and startup measurement)
| # | Deliverable |
|---|---|
| M0 | Branch, workspace members, empty eframe+wgpu window, release profile, font and icon pipeline, `theme.rs`, widgets. **Measure** binary size, cold start and idle CPU against v2.1.2. |
| M1 | Spikes: hayro vs pdf.js (go/no-go), RaTeX vs latex-rust. Visual reference screenshots captured from `dev:mock`. |
| M2 | `lemmata-shell` extracted; `backend.rs`; settings, session and recents; Welcome, Onboarding, Library. |
| M3 | Editor core: buffer, undo, selections, highlighting, wrap, gutter, search, tabs, tree, outline, toolbar, status bar. Save with the content-hash conflict check, autosave, file watcher, conflict dialog. |
| M4 | Build and PDF: build, stop, clean, problems, log, PDF view, SyncTeX both ways, thumbnails, PDF outline, search, export. |
| M5 | Editor intelligence: completion, snippets, hover, folds, environments, diagnostics, texlab on-demand, math preview, symbol palette. |
| M6 | Language and review: spell, grammar, comments/review, TODOs, history and hunks, latexdiff, Git, bibliography (cite picker, manager, Zotero, online), figure/table dialogs, rename symbol, project search/replace, Preferences, Project Settings. |
| M7 | Command palette, quick open, shortcuts sheet, What's new, word count, focus mode, Presentation and presenter, drag and drop everywhere, deep links, single instance. |
| M8 | Vim keymap. |
| M9 | Port all 40 scenarios and invariants; automation socket; visual snapshot pass in both themes; packaging, updater and release workflow changes; size and speed trim pass; delete `apps/desktop`. |

Parity gate before any tag: every item in `docs/guide.md`, every command in the `lib/shortcuts.ts` table (about 60) and every scenario passes. Anything left over is listed and explicitly moved out of scope with the user first. Not `-rc`, not "beta parity".

## Risks
- Editor and Vim are large; the hanging-indent wrap and multi-cursor must be built, not borrowed.
- hayro is young and CPU-only; text-layer coordinates and speed on big documents are unproven (hence the spike).
- No CSS: shadows, blurs and `color-mix` are manual, so exact pixel match is not achievable, and "match" means visually indistinguishable at normal size.
- IME, RTL and complex scripts in a custom editor are weaker than a browser's. Check CJK input early in M3.
- Accessibility depends on AccessKit coverage of custom widgets; the editor needs its own AccessKit text nodes.
- macOS Apple Events, tray of window state, monitor enumeration for the presenter, and Windows/macOS packaging cannot be verified on this Linux machine.

## Verification
Per milestone: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, `cargo build --release -p lemmata-app`, `ls -l` and `cargo bloat --release --crates` for size, a cold-start timing and idle CPU check, kittest snapshot comparison against the captured references, and the ported scenarios through `lemmata_check`. Final: a manual pass through `docs/guide.md` on Linux, then CI on the three OSes before any release discussion.

## Measurements

| Build | Size | Notes |
|---|---|---|
| v2.1.2 Tauri `lemmata-app` (stripped, no LTO) | 33.5 MB | plus the 14.6 MB texlab sidecar |
| M0 egui app: splash only (stripped, fat LTO, `panic = "abort"`) | 13.4 MB | linux x86_64, 2026-10-20 |

M0's largest crates (`cargo bloat --crates`, unstripped `.text` 10.0 MiB): std 1.5 MiB, naga 1.1, wgpu-core 0.6, zbus 0.6 + accesskit_unix 0.5 (Linux accessibility over D-Bus), winit 0.5, skrifa + harfrust + read-fonts 1.0 (text), vello_cpu 0.3, wayland-client 0.4. The UI fonts add 404 KB.

M0 at runtime: about 200 MB RSS shortly after start, most of it the Vulkan driver (NVIDIA). Startup timing is not measured yet; it needs a frame-time probe, planned for M2.
