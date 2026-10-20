# Lemmata: pure egui + wgpu UI (branch `egui`)

## Context
Lemmata v2.1.2 is a Tauri 2 shell around a Svelte 5 / CodeMirror 6 / pdf.js / KaTeX frontend (19k lines of TS/Svelte, 3.8 MB `dist`, 33.5 MB stripped binary, plus a 14.6 MB texlab sidecar). The goal is the smallest and fastest possible app with the **same design and the same feature set**: no webview, no JS, one Rust binary using egui on wgpu.

The 12 crates under `crates/` have no UI or Tauri dependencies and stay untouched. Only the `apps/desktop` frontend and the Tauri glue are replaced. Nothing is released until parity is complete (project rule: no rc or release with known gaps).

**Decisions already made:** PDF engine is `hayro` (pure Rust); the M1 spike passed, so no `mupdf` fallback. Math is RaTeX (M1 spike). texlab is downloaded on first use (pinned and checksummed), not bundled. Renderer is wgpu, as requested.

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
Used by the hover preview, the symbol palette and the completion glyphs. **RaTeX** (decided in M1): `ratex-parser` + `ratex-layout` give a `DisplayList` of glyphs, lines, rects and paths, which the app paints with egui itself. `ratex-render` (tiny-skia + PNG) is not used. The KaTeX fonts (580 KB, OFL) are the ones the Svelte app shows today. Preamble macros (`collectMacros`) are passed as `\def`/`\renewcommand` before the formula; `\newcommand` of a name KaTeX predefines, such as `\R`, errors, just as KaTeX's `macros` option avoids. Errors show the message and stale state as today. Cache the painted shapes keyed by source and size.

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
| M2 egui app: welcome, onboarding, library, all of lemmata-shell linked | 17.6 MB | linux x86_64, 2026-10-20 |
| M3 egui app: plus the editor core and the workspace (no PDF yet) | 20.0 MB | linux x86_64, 2026-10-20 |
| M4 egui app: plus building, logs, the hayro PDF view, image viewer, `panic = "unwind"` | 29.1 MB | linux x86_64, 2026-10-20; 26.3 MB with `panic = "abort"` |
| M5 egui app: plus completion, snippets, folds, problems in the editor, RaTeX maths, hover cards, texlab client | 32.2 MB | linux x86_64, 2026-10-20; texlab (14.6 MB) is downloaded on first use, not shipped |
| M6 egui app: plus spell and grammar checks, review comments, TODOs, Git, History with latexdiff, citation picker and bibliography manager, figure/table dialogs, rename, project search, file actions, Preferences, Project settings | 34.9 MB | linux x86_64, 2026-10-20 |
| M7 egui app: plus the command palette and quick open, shortcuts sheet, word count, reports, What's new, presenting with a presenter window, drag and drop in the tree and from the computer, single instance and `lemmata://` registration | 35.2 MB | linux x86_64, 2026-10-20 |

M4: the release profile switched to `panic = "unwind"` (+2.8 MB) so a PDF that makes hayro panic leaves that page blank instead of ending the app with unsaved edits. The alternative is rendering in a child process with `abort`. The hayro stack (hayro-interpret, hayro-syntax, vello_cpu, fearless_simd, JBIG2/JPEG 2000) is about 3.5 MB of the rest; JPEG, GIF and WebP decoding for image tabs adds about 0.4 MB.

M0's largest crates (`cargo bloat --crates`, unstripped `.text` 10.0 MiB): std 1.5 MiB, naga 1.1, wgpu-core 0.6, zbus 0.6 + accesskit_unix 0.5 (Linux accessibility over D-Bus), winit 0.5, skrifa + harfrust + read-fonts 1.0 (text), vello_cpu 0.3, wayland-client 0.4. The UI fonts add 404 KB.

M0 at runtime: about 200 MB RSS shortly after start, most of it the Vulkan driver (NVIDIA). Startup timing is not measured yet; it needs a frame-time probe, planned for M2.

## M1 spike results (2026-10-20)

Code in `spikes/pdf` and `spikes/math` (standalone crates, not workspace members; `cargo run --release -- OUT_DIR …`).

### PDF: hayro 0.8 — **go**
Inputs were the `thesis-60`, `article`, `slides` (Beamer) and `thesis` fixtures built with latexmk, plus `spikes/pdf/stress.tex` (PNG and JPEG figures, TikZ shading, opacity and dashes, hyperref links and outline, T1 accents and ligatures, 43 pages).

| | thesis-60 (76 pp.) | stress (43 pp.) | article | slides |
|---|---|---|---|---|
| Open + first page at 144 dpi | 4.1 ms | 9.2 ms | 3.2 ms | 1.3 ms |
| Mean / p95 / max per page | 5.4 / 6.9 / 7.9 ms | 2.0 / 7.1 / 40.1 ms | 3.1 ms | 0.4 / 1.2 ms |
| Pixels differing from poppler by > 64/255 (worst sampled page) | 5.1 % | 1.8 % | 0.6 % | 0.7 % |
| pdftotext words found in our text layer | 99.6 % | 94.8 % | 85.2 % | — |
| Links / outline entries | 0 / 0 (no hyperref) | 45 (43 GoTo, 2 URI) / 42 | — | 2 GoTo / 1 |

- Every page checked by eye matches poppler; the pixel differences are anti-aliasing on dense text. Math, Beamer themes, shadings, transparency, dashed curves, PNG and JPEG all render.
- Single-threaded CPU rendering is far inside a frame budget, so rendering on a worker thread and uploading textures is enough. No GPU path needed.
- Text layer: a custom `hayro_interpret::Device` gets each glyph's unicode and advance box in page space. That is enough for selection, search and the SyncTeX pixel mapping. The misses are math tokens that pdftotext orders differently.
- Links (`/Annots`, `/A /S /URI|GoTo`, `/Dest`) and the outline (`/Outlines` → `/First` / `/Next`) are read from raw dictionaries in about 40 lines. Named destinations still need resolving through the catalog's `/Names` tree.
- Size: hayro alone is 6.1 MB stripped with the release profile, including std. vello_cpu, skrifa and kurbo are already in the egui build, so the marginal cost is expected to be lower; to be measured in M4.
- Known limits: no encrypted PDFs, no blend modes or knockout groups (hayro's README), and non-embedded base-14 fonts need a font resolver. LaTeX embeds its fonts; included figures may not, so wire the resolver to the system fonts egui already loads.

### Math: RaTeX 0.1.15 — **chosen**; latex-rust 3.0.1 — rejected

| | RaTeX | latex-rust |
|---|---|---|
| Symbol palette (340 entries from `writing/symbols.ts`) | 339 (only `\!` fails, an empty render) | 326 (no `\supsetneq \thickapprox \uplus \hom \lbrace \rbrace \lbrack \rbrack \degree \copyright \pounds`, `\displaystyle`) |
| 22 display formulas (`spikes/math/formulas.txt`) | 22, layout right | 21; `\operatorname*` fails, `array` rules and `\underbrace` placed wrong |
| Preamble macros | `\def`, `\renewcommand` | none |
| Look | KaTeX fonts, the same as the current preview | STIX Two, which is a visible change |
| Speed per formula | 0.33 ms | 0.35 ms |
| egui coupling | none (paint `DisplayList` ourselves) | its egui feature pins egui 0.36, the app is on 0.37 |
| Licence | MIT, fonts OFL | MIT/Apache, fonts OFL |

RaTeX's one visible flaw in the sheet: `\overbrace{a+b}^{n}\underbrace{c+d}_{m}` draws the braces slightly too wide and they touch. Report it upstream or patch it locally in M5. Size: a standalone RaTeX renderer binary is 4.9 MB, including tiny-skia, png and std, which the app won't use; the real cost is measured in M5.

## Known differences from the Svelte UI (to close or accept before any release)

- Modals: the backdrop is dimmed but not blurred (`backdrop-filter: blur(2px)`); egui has no cheap blur.
- Text: egui lays out IBM Plex about 1–2 % narrower than Chrome, so a paragraph can wrap one word later.
- Long shell commands wrap only at spaces; Chrome also breaks after hyphens.
- "Imported N projects." shows in the library's notice line instead of a toast, until toasts land in M3.
- Show in file manager opens the project folder instead of selecting it in its parent.
- The cached TeX status that made the Svelte app's start instant (texCache.ts) is not ported yet; the splash shows while TeX is detected.
- Recent projects and library folders lived in the webview's localStorage and are not migrated from the Tauri app yet (settings are).
- PDF: selecting text works within the page the drag starts on, not across pages. Scrolling to a page, link or match animates for 0.3 s rather than the browser's smooth scroll.
- PDF: search highlights are drawn with alpha, not `mix-blend-mode: multiply`.
- SVG files in the project are not previewed (needs resvg, about 1 MB); other images and PDFs are.
- Maths is drawn by RaTeX with KaTeX's fonts, rasterized once per size; a few constructs lay out slightly differently from KaTeX (see the M1 spike notes).
- Completion icons are drawn shapes (ƒ, ○, C, abc, ▢, a key) rather than CodeMirror's emoji and symbol glyphs.
- texlab's hover notes show as plain text, not rendered Markdown.
- The package offer ("This needs the X package…") is a toast with "Add it" and a dismiss button, not the separate toast with a package icon and "Not now".
- F2 in the editor opens "Rename label or citation key" on the name under the cursor; the Svelte app listed F2 as "Rename symbol" but only the palette opened the dialog.
- Tree Copy and Cut also put the paths on the system clipboard: egui only reports Ctrl+V when the clipboard holds something, so Paste needs it.
- Preferences: "Check now" for updates comes with the self-updater in M9; the automatic-check setting is kept.
- Editor fonts other than Plex Mono are looked up with fc-match on Linux and by file name in the font folders on macOS and Windows; a family the lookup misses falls back to Plex Mono, as the browser did for fonts that aren't installed.
- Modals that scroll (Preferences, Project settings, Bibliography) give their list a fixed maximum height up front: an egui area only offers the height it had on the last frame, so content could never grow it.
- Presenting: the slides go full screen on the window's own screen and P opens the presenter window; eframe doesn't list monitors, so the Svelte app's "slides to the projector, presenter view here" on a second screen isn't automatic. Drag the presenter window to the other screen.
- macOS hands files and links to a running (or starting) app through Apple Events, which eframe doesn't pass on; on macOS, "Open with" and `lemmata://` links reach Lemmata only through the command line until the packaging work in M9 adds the bundle's event handler.
- Single instance: a later launch hands its file or link over a localhost socket guarded by a token in a user-only file in the cache folder, where the Tauri plugin used D-Bus, a named pipe or a Mach port. As before, only release builds do this.
