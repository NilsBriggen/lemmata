# Lemmata

**A LaTeX editor for your own computer.** Write on the left, and the PDF updates on the right as you type. Double-click the PDF to jump to the source. When a build fails, the error is explained in plain words, often with a one-click fix.

Your projects are ordinary folders on your disk, and they build with the TeX you already have. If you have none, Lemmata can download Tectonic, a 10–22 MB engine, so you don't need to install TeX at all.

[**Download**](https://github.com/NilsBriggen/lemmata/releases/latest) · [Website](https://lemmata.briggen.dev) · [User guide](docs/guide.md) · [Changelog](CHANGELOG.md)

## What it does

**Writing**
- An Overleaf-style layout: file tree and outline, the editor, and the PDF side by side.
- A formatting toolbar, and dialogs for figures, tables, citations and symbols.
- Completion for commands, environments, labels, citations, files and packages, including the commands of the packages your document loads.
- `\begin` and `\end` stay in step when you rename one.
- Indentation follows your environments as you type. Long lines wrap under their own indentation, and faint guides show how environments nest.
- Folding, snippets, a maths preview as you type, and hover cards for `\ref` and `\cite`.
- Renaming a label or citation key renames it everywhere.

**Building**
- Builds start as you stop typing, with latexmk and pdfLaTeX, XeLaTeX, LuaLaTeX or Tectonic.
- Errors and warnings are explained in plain words and point at the line.
- One-click fixes, such as adding a missing `\usepackage`, or the exact command to install a missing package.
- A fast draft mode, and several build targets per project (a paper and its slides).

**PDF**
- Text selection, search, working links, the outline and page thumbnails.
- A full-screen presentation mode for Beamer slides.
- Zoom with Ctrl and the mouse wheel, or by pinching on a touchpad on Linux.

**References and language**
- Add references by DOI, arXiv ID or an online search, or from Zotero.
- A bibliography manager that finds duplicates and unused entries.
- Spell checking:
  - English, German, Swiss German and French come with Lemmata.
  - 14 more languages download when you need them.
- Optional grammar checking with a LanguageTool server you run or trust.

**History, Git and review**
- Snapshots of the whole project. Review the changes since one and undo them one at a time, or restore everything.
- A PDF with the changes marked, made with latexdiff.
- Git built in: commit, pull, push and switch branches, or clone a project from GitHub, GitLab or Overleaf.
- Review comments on any part of the text.

**Projects**
- Start from templates: six built in, and about thirty in the [template collection](https://github.com/NilsBriggen/lemmata-templates). Save any project as a template of your own.
- A start screen with folders: drag projects into them.
- Import a project from Overleaf (its zip download), or export a zip or a ready-to-upload arXiv bundle.

**Private by default.** Lemmata goes online only when you ask it to. That covers:
- looking up references;
- the template collection;
- dictionaries and Tectonic, when you download them;
- a grammar server you choose;
- your Git remotes;
- the update check.

## Install

Get the file for your system from the [latest release](https://github.com/NilsBriggen/lemmata/releases/latest):

| System | File |
|---|---|
| Linux (x86_64 and ARM64) | `.AppImage`, `.deb` or `.rpm` |
| macOS (Apple silicon and Intel) | `.dmg` |
| Windows | `-setup.exe` or `.msi` |

Lemmata checks for updates and installs them when you agree. Release candidates (`-rc`) are on the releases page only.

**TeX.**
- **Linux and macOS:** Lemmata uses TeX Live or MacTeX if you have one. If you don't, it shows the install command for your system, or offers Tectonic instead.
- **Windows:** use MiKTeX. It needs Perl for latexmk; the [user guide](docs/guide.md#first-launch) explains this.

**Optional:**
- `git`, for Git and History.
- Zotero with Better BibTeX, for citations from Zotero.
- A LanguageTool server, for grammar checking.

## What it doesn't do

Lemmata is a local editor. There's no real-time collaboration and no visual (rich-text) editor. To work with others, use review comments and Git.

## Command line

The release also has `lemmata`, a command-line tool for scripts and CI:

| Command | What it does |
|---|---|
| `lemmata build`, `watch` | Compiles a project once, or on every change, and reports its problems. |
| `lemmata lint` | Reports errors, missing citations and undefined labels. |
| `lemmata doctor` | Checks your TeX installation. |
| `lemmata wordcount`, `bib`, `outline` | Word counts, bibliography checks and the section tree. |
| `lemmata export` | Writes a zip or an arXiv bundle. |

Each command can print JSON. See [the guide](docs/guide.md#the-command-line).

## Building from source

You need Rust (stable) and a GPU driver with Vulkan, Metal or DirectX 12. Completion and hover use texlab: the one on your `PATH`, or a pinned build Lemmata downloads on first use.

```bash
cargo run -p lemmata-app
```

| Command | What it does |
|---|---|
| `cargo test --workspace` | All tests, the app's UI scenarios included (they draw headless with wgpu; without TeX they skip the builds) |
| `UPDATE_SNAPSHOTS=1 cargo test -p lemmata-app` | Accepts changed screenshots in `apps/lemmata/tests/snapshots` |
| `cargo run -p lemmata-cli -- build tests/fixtures/article` | Builds a sample project from the command line |
| `scripts/package-app.sh <target> <formats>` | The installers, as a release makes them (needs cargo-packager) |

### Repository layout

| Path | What |
|---|---|
| `apps/lemmata` | The app: egui on wgpu. `src/scenarios.rs` holds its end-to-end scenarios |
| `crates/lemmata-shell` | What the app does without its interface: projects, files, builds, texlab, updates |
| `crates/lemmata-tex` | Finds TeX and runs builds (latexmk, Tectonic) |
| `crates/lemmata-log` | Turns TeX logs into plain-language diagnostics |
| `crates/lemmata-synctex` | Jumps between the PDF and the source |
| `crates/lemmata-project` | `lemmata.toml`, main-file detection, the outline, labels and word counts |
| `crates/lemmata-history` | Snapshots on a private Git reference, compare and restore |
| `crates/lemmata-git` | Git: status, commit, pull, push, branches and clone |
| `crates/lemmata-spell` | Hunspell spell checking |
| `crates/lemmata-grammar` | The LanguageTool client |
| `crates/lemmata-bib` | `.bib` parsing, DOI and arXiv lookup, Zotero |
| `crates/lemmata-pkgmap` | Which TeX package provides a missing file, and how to install it |
| `crates/lemmata-env` | Gives programs started from the AppImage the system's libraries |
| `crates/lemmata-cli` | The `lemmata` command-line tool |
| `data/install.toml` | TeX install commands per system |
| `packaging/` | AUR and Homebrew templates, the Linux desktop entry |
| `tests/fixtures` | Sample projects and real TeX logs |

### Releasing

```bash
scripts/release.sh 2.1.0
git push --follow-tags
```

The script adds the new version to the changelog and tags it. The tag starts `.github/workflows/release.yml`, which builds every system and publishes the release. Plain versions also go to the update feed. Version numbers with `-rc` become pre-releases. The checklist is in [docs/release-checklist.md](docs/release-checklist.md).

## Licence

GPL-3.0-only. See [LICENSE](LICENSE).
