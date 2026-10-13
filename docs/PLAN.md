# Lemmata: software plan

Lemmata is a GPL-3.0 desktop LaTeX editor that gives the Overleaf workflow on your own machine: an editor, a PDF that updates as you type, and click-to-jump between PDF and source. Projects are plain folders, history is git, and documents compile with the TeX Live already on the system.

- Repository: `github.com/NilsBriggen/lemmata`
- Homepage: `https://lemmata.briggen.dev`
- Visual identity and UI mockups: [pitch.html](pitch.html)

## Contents

1. [Product scope](#1-product-scope)
2. [Architecture](#2-architecture)
3. [TeX detection and onboarding](#3-tex-detection-and-onboarding)
4. [Repository layout](#4-repository-layout)
5. [Milestones](#5-milestones)
6. [Automatic release builds](#6-automatic-release-builds)
7. [Testing](#7-testing)
8. [Risks](#8-risks)
9. [Changes from the first plan](#9-changes-from-the-first-plan)

---

## 1. Product scope

### In v1.0

| Area | Feature |
|---|---|
| Projects | Library with PDF thumbnails, templates (article, thesis, slides, CV, letter), Overleaf `.zip` import, optional `lemmata.toml`, session restore |
| Editing | File tree with create/rename/trash, tabs, outline with word counts, LaTeX highlighting, texlab completion/hover/definitions/rename, spell check (en, en-GB, de, de-CH, fr), Vim keys, focus mode |
| Navigation | Command palette, quick open, project search and replace |
| Building | Compile on save through latexmk, pdfLaTeX / XeLaTeX / LuaLaTeX per project, BibTeX and biber, `\include` in subfolders |
| Preview | PDF.js viewer that keeps its position across rebuilds, SyncTeX in both directions, dark pages, math preview in the editor (KaTeX) |
| Errors | Log parsed into plain-language messages with file, line and a suggested fix; missing-package prompts with the exact install command and an Open terminal button |
| References | Citation picker, add by DOI or arXiv ID, Zotero via Better BibTeX |
| Figures and tables | Drag-and-drop and pasted images into `figures/`, spreadsheet paste as booktabs tables, offers to add missing packages |
| Setup | First-launch TeX Live / MiKTeX detection with per-OS install instructions |
| History | Snapshots on a private git ref, history view, diff, restore |
| Export | PDF, arXiv-ready zip from the files the build used |
| Updates | Signed auto-update, stable and beta channels |
| Support | "Report a problem" opens a prefilled GitHub issue the user reviews first |

### Not in v1

Real-time collaboration, cloud sync, Flathub, a bundled TeX engine, WYSIWYG editing, AI features.

---

## 2. Architecture

```
┌──────────────────────── webview: Svelte 5 + TypeScript ────────────────────────┐
│ CodeMirror 6 editor ── LSP messages over Tauri IPC ──▶ texlab (sidecar binary) │
│ PDF.js viewer                                                                  │
│ Panels: file tree, outline, errors, history, onboarding, settings              │
└──────────────────────────────── Tauri commands + events ───────────────────────┘
                                         │
┌──────────────────────────── Rust core (apps/desktop/src-tauri) ────────────────┐
│ lemmata-tex       detect TeX Live / MiKTeX, run latexmk, queue + cancel builds  │
│ lemmata-log       parse .log / .blg into structured diagnostics                 │
│ lemmata-synctex   wrap the `synctex view` / `synctex edit` CLI                  │
│ lemmata-pkgmap    missing file → TeX Live package → distro package / tlmgr      │
│ lemmata-project   lemmata.toml, main-file detection, file watcher, zip import   │
│ lemmata-history   snapshots via gitoxide                                        │
└────────────────────────────────────────────────────────────────────────────────┘
lemmata-cli  `lemmata build | watch | doctor`, built on the same crates, no GUI
```

### Key decisions

- **Frontend: Svelte 5 + Vite + TypeScript.** Small runtime, simple state handling for the panels. CodeMirror 6 and PDF.js are framework-agnostic, so this choice is cheap to revisit.
- **Compile command:**
  `latexmk -pdf|-xelatex|-lualatex -synctex=1 -interaction=nonstopmode -file-line-error -outdir=.lemmata/build <main>.tex`
  - Builds start 300 ms after the last save. A new save cancels the build that's running.
  - The finished PDF replaces the old one atomically and the viewer restores page and scroll offset.
  - `.lemmata/` is added to the project's `.gitignore` on first build.
- **`--shell-escape` is off by default.** Turning it on is a per-project setting with a clear warning, because opening someone else's project must never run their code.
- **SyncTeX** uses the `synctex` CLI that ships with TeX Live and MiKTeX instead of a home-made parser.
- **texlab** is bundled as a Tauri sidecar (`bundle.externalBin`). CI downloads the matching binary for each target.
- **History:**
  - In a git repo, snapshots are commits on a private ref `refs/lemmata/snapshots`, so they never touch the user's branches.
  - Outside a repo, Lemmata offers to run `git init`.
  - Snapshots are taken every 10 minutes while the project has changes, and whenever the user clicks "Save snapshot".
- **`lemmata.toml`** is optional:

  ```toml
  main = "main.tex"
  engine = "pdflatex"     # pdflatex | xelatex | lualatex
  bib = "biber"           # biber | bibtex
  shell_escape = false
  outdir = ".lemmata/build"
  ```

  Without the file, the main file is the `.tex` file that contains `\documentclass`. If several do, the user picks one and the choice is saved to `lemmata.toml`.
- **Settings and recent projects** are stored in the OS config directory via `tauri-plugin-store`.

---

## 3. TeX detection and onboarding

### Detection

The probe order, stopping at the first location that has the required tools:

1. `PATH` as seen by the app
2. `/Library/TeX/texbin` (MacTeX; apps launched from the Dock don't inherit the shell's PATH)
3. `/usr/local/texlive/*/bin/*` (vanilla TeX Live on Linux and macOS)
4. `%LOCALAPPDATA%\Programs\MiKTeX\miktex\bin\x64`, `C:\Program Files\MiKTeX\miktex\bin\x64`, `C:\texlive\*\bin\windows`
5. A folder the user picks

| Tools | Status |
|---|---|
| `latexmk`, `pdflatex` | Required |
| `synctex`, `biber`, `bibtex`, `xelatex`, `lualatex` | Recommended; missing ones are listed, not blocking. Without `synctex`, PDF ↔ source jumps are off |

- The distribution and year come from `tex --version` (TeX Live) or `miktex --version`.
- TeX Live older than 2022 gets a warning but still works.

### Onboarding screen

- It runs on first launch and again whenever detection fails.
- It shows the checks, the detected OS / distro and the install command.
- Lemmata **never runs `sudo` itself.** It offers Copy, Open terminal, Check again and Choose folder.
- Install commands live in `data/install.toml`, so contributors can fix them without touching code. Each entry has a `recommended` and a `full` variant:

| Platform | Recommended | Full |
|---|---|---|
| Arch / CachyOS / Manjaro | `texlive-basic texlive-latex texlive-latexrecommended texlive-latexextra texlive-fontsrecommended texlive-bibtexextra texlive-binextra texlive-mathscience biber` | `texlive texlive-lang biber` |
| Debian / Ubuntu | `texlive-latex-extra texlive-fonts-recommended texlive-bibtex-extra texlive-science latexmk biber` | `texlive-full` |
| Fedora | `texlive-scheme-medium latexmk biber` | `texlive-scheme-full` |
| openSUSE | `texlive-scheme-medium texlive-latexmk biber` | `texlive-scheme-full` |
| macOS | `brew install --cask mactex-no-gui` or the MacTeX installer | same |
| Windows | TeX Live `install-tl-windows.exe` or the MiKTeX installer | same |

Check these package names against each distro before M1. They change between releases.

### Missing packages

- `lemmata-log` recognises `! LaTeX Error: File 'x.sty' not found.` and the equivalents for `.cls`, `.bst` and fonts.
- `lemmata-pkgmap` turns the missing file into a fix:

| Installation | Suggested fix |
|---|---|
| Distro TeX Live | The distro package that contains the file, e.g. `sudo pacman -S texlive-pictures` |
| Vanilla TeX Live | `tlmgr install <package>` |
| MiKTeX | Nothing to do if auto-install is on (the default). Otherwise `miktex packages install <package>` |

- The mapping data is generated by `scripts/gen-pkgmap` from TeX Live's `texlive.tlpdb` (file → package → collection). It's then joined with a per-distro table from collection to distro package, and regenerated each TeX Live release.

---

## 4. Repository layout

```
lemmata/
├─ apps/desktop/              Svelte frontend
│  └─ src-tauri/              Tauri shell, commands, updater, sidecar config
├─ crates/
│  ├─ lemmata-tex/
│  ├─ lemmata-log/
│  ├─ lemmata-synctex/
│  ├─ lemmata-pkgmap/
│  ├─ lemmata-project/
│  ├─ lemmata-history/
│  └─ lemmata-cli/
├─ data/install.toml          install commands per OS / distro
├─ templates/                 article, thesis, beamer, cv, letter
├─ packaging/
│  ├─ aur/PKGBUILD.in         lemmata-bin
│  ├─ homebrew/lemmata.rb.in  cask
│  └─ winget/                 manifest templates
├─ scripts/                   release.sh, gen-pkgmap, fetch-texlab.sh
├─ tests/fixtures/            sample projects + corpus of real .log files
├─ docs/                      PLAN.md, website-handoff.md, pitch.html
├─ .github/workflows/         ci.yml, release.yml
├─ Cargo.toml                 workspace
├─ package.json / pnpm-workspace.yaml
├─ cliff.toml                 git-cliff changelog config
├─ CHANGELOG.md
└─ LICENSE                    GPL-3.0
```

---

## 5. Milestones

The weeks assume one developer working full time. The release pipeline skeleton is set up during M0, so every milestone ships as a pre-release.

| # | Milestone | Weeks | Done when |
|---|---|---|---|
| M0 | Spike | 1–2 | Tauri window, CodeMirror, latexmk on save, PDF.js preview. Edit → updated PDF in under 1.5 s on a 60-page thesis on Linux, macOS and Windows, with scroll position kept. Release workflow builds unsigned artifacts from a tag. |
| M1 | Onboarding | 3–4 | Fresh VMs (Arch, Ubuntu, Fedora, macOS, Windows with TeX Live, Windows with MiKTeX) reach a compiled template using only the instructions on screen. Missing-package prompts work. |
| M2 | Editor core | 5–8 | Tree, tabs, outline, texlab completion for commands, labels and citations, SyncTeX both ways, error panel with plain-language messages. |
| M3 | Projects | 9–10 | Library, templates, Overleaf zip import, `lemmata.toml`, snapshots and history view with diff. |
| M4 | Release pipeline | 11–12 | One tag produces signed builds, a published GitHub Release, AUR / Homebrew / winget updates, and a working auto-update from the previous version (see §6). |
| M5 | Public 0.1 | 13 | Website live, user docs, CONTRIBUTING, issue templates, opt-in crash reporting. |
| Later | After 0.1 | – | Bibliography manager and Zotero (Better BibTeX) sync, LanguageTool grammar check, visual PDF diff, standalone CLI release. |

---

## 6. Automatic release builds

### Versioning

- Semantic versioning, one version for the whole repo.
- `scripts/release.sh 0.3.0`:
  1. Bumps the version in the workspace `Cargo.toml`, `apps/desktop/package.json` and `tauri.conf.json`.
  2. Regenerates `CHANGELOG.md` with git-cliff from Conventional Commits.
  3. Commits `chore(release): v0.3.0`.
  4. Creates the tag `v0.3.0`.
- `git push --follow-tags` starts the release.
- A tag with a suffix (`v0.3.0-beta.1`) becomes a GitHub pre-release, publishes to the beta update channel, and skips the package-manager jobs.

### `ci.yml`: every pull request and push to `main`

| Job | Runs on | Does |
|---|---|---|
| lint | ubuntu | `cargo fmt --check`, `cargo clippy -D warnings`, `pnpm lint`, `svelte-check` |
| test | ubuntu, macos, windows | `cargo test --workspace`, `pnpm vitest run` |
| build-check | ubuntu-22.04, macos-14, windows-latest | `tauri build --no-bundle` |
| tex-integration | ubuntu | installs TeX Live with `TeX-Live/setup-texlive-action`, compiles `tests/fixtures/*` with `lemmata-cli build`, compares diagnostics with the expected JSON |

### `release.yml`: on `push: tags: ['v*']`

**Job 1: build** (matrix, `tauri-apps/tauri-action`, `releaseDraft: true`, `includeUpdaterJson: true`)

| Runner | Rust target | Produces |
|---|---|---|
| `ubuntu-22.04` | `x86_64-unknown-linux-gnu` | `.AppImage`, `.deb`, `.rpm` |
| `ubuntu-22.04-arm` | `aarch64-unknown-linux-gnu` | `.AppImage`, `.deb`, `.rpm` |
| `macos-14` | `universal-apple-darwin` | `.dmg`, `.app.tar.gz` + `.sig` (updater) |
| `windows-latest` | `x86_64-pc-windows-msvc` | NSIS `-setup.exe` + `.sig`, `.msi` |

- Ubuntu 22.04 links against an older glibc, so the AppImage runs on more distros.
- `scripts/fetch-texlab.sh <target>` downloads the matching texlab release before the build.
- Updater artifacts are signed with `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- macOS signing and notarisation use `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID`. The step is skipped when the secrets are absent, so the pipeline works before there is an Apple Developer account.
- Windows builds are unsigned at first. Apply to SignPath's free open-source program and add its signing action once accepted.

**Job 2: finalize** (needs build)

1. Rename every asset to `Lemmata_<version>_<os>_<arch>.<ext>`:

   ```
   Lemmata_0.3.0_linux_x86_64.AppImage
   Lemmata_0.3.0_linux_x86_64.deb
   Lemmata_0.3.0_linux_x86_64.rpm
   Lemmata_0.3.0_linux_aarch64.AppImage
   Lemmata_0.3.0_linux_aarch64.deb
   Lemmata_0.3.0_linux_aarch64.rpm
   Lemmata_0.3.0_macos_universal.dmg
   Lemmata_0.3.0_macos_universal.app.tar.gz      (+ .sig)
   Lemmata_0.3.0_windows_x86_64-setup.exe        (+ .sig)
   Lemmata_0.3.0_windows_x86_64.msi
   ```

2. Rewrite `latest.json` so its URLs point at the renamed files. For pre-releases, upload it as `beta.json` instead.
3. Write `SHA256SUMS.txt` covering every asset.
4. Set the release notes to the matching `CHANGELOG.md` section.
5. Publish the draft.

**Job 3: distribute** (needs finalize; stable tags only; jobs are independent so one failure doesn't block the others)

| Target | How | Secret |
|---|---|---|
| AUR `lemmata-bin` | Render `packaging/aur/PKGBUILD.in` with version + sha256 of the x86_64 and aarch64 `.deb`, push with `KSXGitHub/github-actions-deploy-aur` | `AUR_SSH_KEY` |
| Homebrew | Render `packaging/homebrew/lemmata.rb.in`, commit to `NilsBriggen/homebrew-tap` as `Casks/lemmata.rb` | `TAP_TOKEN` (fine-grained PAT, contents write on the tap repo) |
| winget | `vedantmgoyal9/winget-releaser` opens a PR to `microsoft/winget-pkgs` for `NilsBriggen.Lemmata` | `WINGET_TOKEN` (classic PAT, `public_repo`) |
| Website | `repository_dispatch` with event type `release-published` to the website repo | `WEBSITE_DISPATCH_TOKEN` |

### Auto-update

- `tauri-plugin-updater` checks `https://lemmata.briggen.dev/update/latest.json` (stable) or `.../update/beta.json` (beta, chosen in settings).
- The website answers both with a 302 redirect to the GitHub asset:
  - `latest.json` → `https://github.com/NilsBriggen/lemmata/releases/latest/download/latest.json`
  - `beta.json` → the newest pre-release's `beta.json`, which the website resolves at build time
- Installed copies only know the briggen.dev URL, so the repo can move without breaking updates.
- The app checks on start and every 6 hours. It downloads in the background and asks to restart; it never restarts on its own.

### One-time setup

- [ ] `pnpm tauri signer generate`. Put the public key in `tauri.conf.json`, the private key and password in GitHub secrets, and keep an offline backup. **Losing this key means existing installs can never update again.**
- [ ] Add the GitHub secrets listed above.
- [ ] Create the repo `NilsBriggen/homebrew-tap`.
- [ ] Create an AUR account, add the SSH key, and register `lemmata-bin` with the first manual push.
- [ ] Submit the first winget manifest by hand; later versions are automated.
- [ ] Optional: Apple Developer account (99 USD/year) for signing and notarisation.
- [ ] Optional: SignPath open-source application for Windows signing.

---

## 7. Testing

| Level | What | Where |
|---|---|---|
| Unit | Log parser against a corpus of real `.log` / `.blg` files (undefined refs, missing files, overfull boxes, biber failures, runaway arguments); pkgmap lookups; TeX detection against a fake filesystem; `lemmata.toml` parsing | `cargo test` |
| Frontend | Stores, LSP bridge, error-panel formatting | vitest |
| Integration | Compile every project in `tests/fixtures/` with real TeX Live; compare diagnostics to expected JSON | CI `tex-integration` |
| End-to-end | WebdriverIO + `tauri-driver` on Linux and Windows (macOS isn't supported by tauri-driver): open project, edit, PDF updates, click PDF jumps to source, missing-package prompt appears | CI, nightly |
| Release | Manual checklist per OS: fresh install, onboarding, auto-update from the previous version, uninstall | before each stable tag |

---

## 8. Risks

| Risk | Mitigation |
|---|---|
| New users give up during a multi-GB TeX Live install | Lead with the smaller recommended set, let users browse templates while it installs, explain why it's needed in one sentence |
| Distro package names change | Keep them in `data/install.toml` and the pkgmap tables; check them in the M1 VM tests |
| WebKitGTK renders PDF.js slowly on Linux | Benchmark in M0; fall back to rendering only visible pages at lower resolution while scrolling |
| Unsigned builds trigger Gatekeeper and SmartScreen warnings | Website explains the workaround; sign as soon as accounts exist |
| Old TeX Live on Ubuntu LTS | Warn below 2022, link to the vanilla TeX Live installer |
| Updater signing key lost or leaked | Offline backup; document key rotation, which needs a manual reinstall |
| Malicious projects | `--shell-escape` off by default, never execute project-provided scripts |

---

## 9. Changes from the first plan

Decisions that changed during implementation, and why.

| Planned | Built | Why |
|---|---|---|
| History via gitoxide | The `git` CLI with a private index file (`crates/lemmata-history`) | Far less code; a separate `GIT_INDEX_FILE` and the private ref `refs/lemmata/snapshots` keep the user's HEAD, branches and staging area untouched. If git is missing, history explains how to get it. |
| Spell check with nspell in a Web Worker | spellbook (Rust Hunspell) in `crates/lemmata-spell` | nspell rejected correct German compounds ("Übersetzung") and took 1.8 s to load French. spellbook handles compounding and loads every dictionary in under 10 ms. |
| Swiss German dictionary package | German dictionary + ß→ss rule | The upstream de-CH package still contains ß forms. |
| Word count via texcount | Own counter sharing rules with spell check (`crates/lemmata-project/src/prose.rs`) | Per-section counts line up with the outline; no dependency on texcount's output format. |
| Outline from texlab document symbols | Own walk following `\input`/`\include` (`lemmata_project::outline`) | texlab returns symbols per file, so a main file made of `\input`s showed an empty outline. |
| `synctex` required | Recommended | Minimal TeX Live installs (and GitHub's runners) lack it; building works without it. |
| MathJax for math preview | KaTeX 0.18 | Smaller and faster; supports the preamble's macros. |
| Opt-in crash reporting | "Report a problem" with a prefilled GitHub issue | No server needed; the user sees and edits everything before it's sent. |
| ISBN lookup for references | DOI and arXiv only | arXiv papers have DataCite DOIs, so one doi.org lookup covers both; ISBN services need API keys. |

