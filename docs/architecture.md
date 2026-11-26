# Lemmata architecture

Lemmata is a native Rust desktop LaTeX editor built with **egui** on **wgpu** (`eframe`). There is no embedded browser or JavaScript runtime. All domain logic lives in UI-independent Rust crates under `crates/`, shared between the desktop app (`apps/lemmata`) and the command-line tool (`crates/lemmata-cli`).

## Workspace layout

```
apps/
  lemmata/              Native desktop app (egui + wgpu, binary `lemmata-app`)
    src/
      app.rs            Root application state, layout, window & event wiring
      theme.rs          Light/dark palette tokens, typography & spacing metrics
      editor/           Custom ropey-backed code editor, LaTeX highlighter,
                        line-layout cache, folding, completion, hover, spell,
                        grammar, comments, inline RaTeX math preview
      pdf/              hayro PDF renderer, text layer, links, outline,
                        thumbnails, SyncTeX mapping, full-screen presenter
      workspace/        Activity rail, file tree, outline, search, problems,
                        git, history, comments, TODOs, command palette, modals
      library/          Start screen, folders, grid/list views, templates
      welcome.rs        4-step guided first-run setup (TeX, Appearance,
                        Workflow, Start)
      onboarding.rs     TeX Live / MiKTeX / Tectonic setup & detection view
      store.rs          Settings, recent projects, trust decisions, sessions
    tests/snapshots/    Headless wgpu kittest UI snapshots

crates/
  lemmata-shell/        UI-independent app backend: projects, files, trash/undo,
                        build orchestration, texlab LSP client, online
                        templates, updater, argv & `lemmata://` URL handling
  lemmata-tex/          TeX Live / MiKTeX / Tectonic detection and build runner
  lemmata-log/          TeX / BibTeX / biber log parser and plain-language hints
  lemmata-synctex/      Streaming `.synctex.gz` / `.synctex` forward & inverse search
  lemmata-project/      `lemmata.toml`, main-file detection, outline, labels,
                        prose extraction, word counts, ZIP/arXiv export
  lemmata-history/      Project snapshots on a private Git ref, diff & restore
  lemmata-git/          Git status, staging, commit, pull, push, branches, clone
  lemmata-spell/        Hunspell spell checking (gzip-compressed bundled dicts)
  lemmata-grammar/      Optional LanguageTool HTTP client
  lemmata-bib/          `.bib` parser, duplicate detection, DOI/arXiv/Crossref
                        lookup, Zotero Better BibTeX bridge
  lemmata-pkgmap/       Maps missing LaTeX files to TeX Live & distro packages
  lemmata-env/          Restores host library paths when launching tools from AppImage
  lemmata-cli/          Standalone `lemmata` CLI for scripts and CI

data/
  dictionaries/         Gzip-compressed bundled Hunspell dictionaries (en, en-GB, de, fr)
  install.toml          Per-OS TeX install commands (shared with the website)
  distro-packages.toml  Distro package names for missing TeX packages

templates/              Six built-in starter templates (article, thesis, slides, cv, letter, homework)
scripts/                Release, packaging, and fixture-check scripts
```

## Key subsystems

### Editor (`apps/lemmata/src/editor/`)
- Backed by `ropey` with a per-line `LineLayout` cache (keyed by line text, font, wrap width, theme, and diagnostics) so large theses stay responsive.
- Hand-written LaTeX tokenizer, environment pairing (`\begin`/`\end`), indentation guides, section/environment/comment folding, and live diagnostics.
- **Completion & hover:** built-in LaTeX, label, citation, path, package, and math-symbol completion, augmented by `texlab` (from `PATH` or downloaded on first use with pinned checksums).
- **Inline math preview:** parsed and laid out in pure Rust with `ratex-parser` + `ratex-layout` and painted directly via `epaint` using bundled KaTeX fonts.
- **Spell & grammar checking:** persistent background spellcheck worker using `lemmata-spell` (bundled dictionaries are stored `.gz` and decompressed on load); optional LanguageTool client in `lemmata-grammar`.

### PDF viewer (`apps/lemmata/src/pdf/`)
- Pure-Rust rendering via `hayro` on a worker thread, uploaded as `wgpu` textures at the current zoom and DPI.
- Light-mode pages upload zero-copy directly to `egui::ColorImage`; dark-page mode applies a luminance inversion pass.
- Text selection, search, clickable links, document outline, thumbnails, SyncTeX forward/inverse navigation, and a multi-window Beamer presenter view.

### Rendering & footprint
- Reactive rendering: unfocused windows pause background repaint loops to keep idle CPU at zero.
- `wgpu` backends are OS-gated in `apps/lemmata/Cargo.toml` (Metal on macOS; Vulkan + GLES on Linux; DX12 + Vulkan + GLES on Windows).
- Release builds use LTO, `codegen-units = 1`, `panic = "abort"`, and stripped symbols (`Cargo.toml`).

## Testing and verification

- `cargo test --workspace` runs unit tests across all crates plus headless `wgpu` UI scenarios and snapshot comparisons (`egui_kittest`) in `apps/lemmata`.
- `UPDATE_SNAPSHOTS=1 cargo test -p lemmata-app` regenerates reference PNGs under `apps/lemmata/tests/snapshots/`.
- `scripts/check-fixtures.sh` validates the CLI against `tests/fixtures/`.
- Release packaging is handled by `scripts/package-app.sh` and `.github/workflows/release.yml`, with `scripts/finalize-release.py` renaming artifacts to match the contract in [website-handoff.md](website-handoff.md).
