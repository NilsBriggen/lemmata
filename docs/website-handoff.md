# Website contract (`lemmata.briggen.dev`)

This document defines the contract between Lemmata releases and the static site at `https://lemmata.briggen.dev`.

## Product summary

Lemmata is a free, open-source (GPL-3.0) native LaTeX editor for Linux, macOS, and Windows. It brings the Overleaf workflow (editor, live PDF preview, SyncTeX click-to-jump) to local folders with Git history, no account, and no upload.

- **Source:** `https://github.com/NilsBriggen/lemmata`
- **User guide (source of truth for features):** `https://github.com/NilsBriggen/lemmata/blob/main/docs/guide.md`
- **TeX install commands:** `https://raw.githubusercontent.com/NilsBriggen/lemmata/main/data/install.toml`

Only describe features documented in [guide.md](guide.md). Do not invent features, numbers, testimonials, or user counts. State plainly that Lemmata is a local editor with no real-time collaboration and no rich-text (visual) editor.

## Pages

| Path | Content |
|---|---|
| `/` | Editorial hero with OS-detected download CTA, framed 4.0 light/dark workspace screenshot, feature highlights, visual showcases (citations, guided setup, templates), 3-step getting started, and bottom download CTA |
| `/download` | Every platform and format, version, release date, file sizes, SHA-256 checksums, CLI downloads, and install/verification commands |
| `/tex` | Per-OS TeX Live / MiKTeX install commands rendered from `data/install.toml`, plus Tectonic no-install notes |
| `/changelog` | Release notes from GitHub Releases, newest first |

## Release data and asset naming

The site builder fetches release metadata from:

```
https://api.github.com/repos/NilsBriggen/lemmata/releases?per_page=100
```

Every stable `vX.Y.Z` release publishes these assets (produced by `.github/workflows/release.yml` and renamed by `scripts/finalize-release.py`; suffixed tags like `vX.Y.Z-rc.N` are GitHub pre-releases without `.msi`, `latest.json`, or package-manager updates):

| File | Platform |
|---|---|
| `Lemmata_<ver>_linux_x86_64.AppImage` | Linux x86-64, any distro |
| `Lemmata_<ver>_linux_x86_64.deb` | Debian, Ubuntu, Mint |
| `Lemmata_<ver>_linux_x86_64.rpm` | Fedora, openSUSE |
| `Lemmata_<ver>_linux_aarch64.AppImage` / `.deb` / `.rpm` | Linux ARM64 |
| `Lemmata_<ver>_macos_universal.dmg` | macOS (Apple Silicon and Intel universal) |
| `Lemmata_<ver>_macos_universal_aarch64.dmg` | macOS, Apple Silicon |
| `Lemmata_<ver>_macos_universal_x86_64.dmg` | macOS, Intel |
| `Lemmata_<ver>_windows_x86_64-setup.exe` | Windows installer (recommended) |
| `Lemmata_<ver>_windows_x86_64.msi` | Windows MSI, for managed installs |
| `lemmata-cli_<ver>_<platform>.tar.gz` (`.zip` for `windows_x86_64`) | Standalone `lemmata` CLI for `linux_x86_64`, `linux_aarch64`, `macos_universal`, `windows_x86_64` |
| `SHA256SUMS.txt` | SHA-256 checksums for all release files |

Ignore `.sig`, `.app.tar.gz`, and `latest.json` on `/download`; they are consumed by the in-app updater.

## Hard requirement: updater redirects

Installed copies of Lemmata check these endpoints for updates (`crates/lemmata-shell/src/update.rs`). They must always return `302` with a cache lifetime of at most 5 minutes:

| Path | Response |
|---|---|
| `/update/latest.json` | `302` → `https://github.com/NilsBriggen/lemmata/releases/latest/download/latest.json` |
| `/update/beta.json` | Same `302` as `/update/latest.json` (kept for compatibility with older clients) |

## Unsigned-build notices

While macOS and Windows builds are unsigned, show these notes next to their downloads (kept in one place in the builder so they are easy to remove once signing is enabled):

- **macOS:** "macOS may say Lemmata can't be checked for malware. Open System Settings → Privacy & Security and click Open Anyway."
- **Windows:** "If SmartScreen warns you, click More info → Run anyway."

## Brand & constraints

- **Icon:** Lemmata 4.0 Typeset Split mark (`apps/lemmata/icons/icon.svg`)
- **Accent:** non-photo blue (`#2f7fb8` light, `#6fb3e3` dark)
- **Neutrals:** cool paper grey `#f3f4f2` / deep slate `#10151a`, ink `#18212b` / `#e6eaee`
- **Fonts:** Young Serif (display), IBM Plex Sans (body), IBM Plex Mono (code/commands), STIX Two Text (definition)
- **Themes:** Light and dark following `prefers-color-scheme`
- **Tone:** plain, precise, academic; no hype words
- **Privacy & hosting:** zero analytics/tracking scripts, no accounts/comments, strict CSP, binaries linked directly from GitHub Releases
