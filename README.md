# Lemmata

A local-first LaTeX editor: write on the left, see the PDF update on the right, double-click the PDF to jump to the source. Projects are plain folders on your disk, and documents build with the TeX Live (or MiKTeX) already on your computer.

Status: early development (milestone M0). See [docs/PLAN.md](docs/PLAN.md) for the full plan.

## Requirements

- A TeX distribution with `latexmk`, `pdflatex` and `synctex`. The app explains how to install one on first launch, or run `lemmata doctor`.

## Developing

You need Rust (stable), Node 22+ and pnpm, plus the [Tauri system libraries](https://v2.tauri.app/start/prerequisites/) for your OS. For completion and hover, have `texlab` on your PATH, or fetch the pinned build with `scripts/fetch-texlab.sh <target-triple>`; release builds bundle it.

```bash
pnpm install
pnpm dev                 # run the app with hot reload
cargo test --workspace   # Rust tests (build the frontend first: pnpm --filter lemmata-desktop build)
pnpm test                # frontend tests
pnpm check               # Svelte and TypeScript checks
cargo run -p lemmata-cli -- doctor              # check the TeX installation
cargo run -p lemmata-cli -- build tests/fixtures/article
```

## Layout

| Path | What |
|---|---|
| `apps/desktop` | Svelte 5 frontend; `src-tauri/` is the Tauri shell and its commands |
| `crates/lemmata-tex` | Finds TeX, picks install instructions, runs latexmk |
| `crates/lemmata-log` | Turns TeX logs into diagnostics with plain-language hints |
| `crates/lemmata-synctex` | PDF ↔ source jumps through the `synctex` CLI |
| `crates/lemmata-project` | `lemmata.toml`, main-file detection, file tree |
| `crates/lemmata-cli` | `lemmata build` and `lemmata doctor` |
| `data/install.toml` | TeX install commands per OS and distro |
| `packaging/` | AUR and Homebrew templates rendered on release |
| `tests/fixtures` | Sample projects and real TeX logs |

## Releasing

```bash
scripts/release.sh 0.1.0
git push --follow-tags
```

The tag starts `.github/workflows/release.yml`, which builds every platform, publishes the GitHub release and updates the AUR, Homebrew and winget. The secrets it needs are listed in [docs/PLAN.md §6](docs/PLAN.md#6-automatic-release-builds).

## Licence

GPL-3.0-only. See [LICENSE](LICENSE).
