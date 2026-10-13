# Handoff: Lemmata homepage

You are building the homepage for **Lemmata** at `https://lemmata.briggen.dev`. This file has everything you need. The full software plan is in [PLAN.md](PLAN.md); the visual identity and UI mockups are in [pitch.html](pitch.html).

## The product

Lemmata is a free, open-source LaTeX editor for Linux, macOS and Windows. It gives you the Overleaf workflow (editor, live PDF preview, click the PDF to jump to the source) on your own machine, with plain project folders, git history and no account.

- Licence: GPL-3.0
- Source: `https://github.com/NilsBriggen/lemmata`
- Audience: students and researchers who write in LaTeX and want to work offline or keep their files local.
- Requirement: a TeX distribution (TeX Live, or MiKTeX on Windows). The app detects it and walks users through installing it, so the site should present this as a step, not a barrier.

Only describe features listed in §1 of PLAN.md. Don't invent features, numbers, testimonials or user counts.

## Pages

| Page | Content |
|---|---|
| `/` | One-line pitch, three or four feature highlights, screenshot placeholders (real screenshots come later), a download button that detects the visitor's OS and offers the matching file, a link to all downloads |
| `/download` | Every platform and format, version number, release date, file sizes, checksums, install instructions (below) |
| `/tex` | How to install TeX Live per OS, using the commands in §3 of PLAN.md. Explain that the app shows the same instructions on first launch |
| `/changelog` | Release notes from GitHub Releases, newest first |

Footer: GitHub link, licence, "Made by Nils Briggen".

## Release data

Fetch release information at build time from:

```
https://api.github.com/repos/NilsBriggen/lemmata/releases/latest
https://api.github.com/repos/NilsBriggen/lemmata/releases        (for the changelog)
```

Every release has these assets (version `0.3.0` as an example):

| File | Platform |
|---|---|
| `Lemmata_0.3.0_linux_x86_64.AppImage` | Linux x86_64, any distro |
| `Lemmata_0.3.0_linux_x86_64.deb` | Debian, Ubuntu, Mint |
| `Lemmata_0.3.0_linux_x86_64.rpm` | Fedora, openSUSE |
| `Lemmata_0.3.0_linux_aarch64.AppImage` / `.deb` / `.rpm` | Linux ARM64 |
| `Lemmata_0.3.0_macos_universal.dmg` | macOS, Apple Silicon and Intel |
| `Lemmata_0.3.0_windows_x86_64-setup.exe` | Windows installer (recommended) |
| `Lemmata_0.3.0_windows_x86_64.msi` | Windows MSI, for managed installs |
| `SHA256SUMS.txt` | Checksums for all of the above |

Ignore `.sig`, `.app.tar.gz`, `latest.json` and `beta.json`; they're for the auto-updater. Releases marked `prerelease` are betas: show them only on the changelog, labelled "Beta".

**Rebuilds:** the release workflow sends a GitHub `repository_dispatch` event of type `release-published` to the website repo. Trigger a rebuild and deploy on that event. Also rebuild daily as a fallback.

## Hard requirement: updater redirects

Installed copies of Lemmata check these URLs for updates. They must work from the first release on:

| Path | Response |
|---|---|
| `/update/latest.json` | `302` to `https://github.com/NilsBriggen/lemmata/releases/latest/download/latest.json` |
| `/update/beta.json` | `302` to the `beta.json` asset of the newest pre-release, resolved at build time. If there is none, use the same target as `latest.json` |

Don't cache these redirects for longer than 5 minutes.

## Install instructions to show

| Platform | Instructions |
|---|---|
| Arch / CachyOS / Manjaro | `yay -S lemmata-bin` (or any AUR helper) |
| Debian / Ubuntu | Download the `.deb`, then `sudo apt install ./Lemmata_<version>_linux_x86_64.deb` |
| Fedora / openSUSE | Download the `.rpm`, then `sudo dnf install ./Lemmata_<version>_linux_x86_64.rpm` (or `sudo zypper install …`) |
| Any Linux | Download the AppImage, `chmod +x Lemmata_*.AppImage`, run it |
| macOS | `brew install --cask NilsBriggen/tap/lemmata`, or open the `.dmg` and drag Lemmata to Applications |
| Windows | `winget install NilsBriggen.Lemmata`, or run the setup `.exe` |

To verify a download: `sha256sum -c SHA256SUMS.txt --ignore-missing` (Linux), `shasum -a 256 <file>` (macOS), `Get-FileHash <file>` (PowerShell).

**While builds are unsigned**, show these notes next to the macOS and Windows downloads. They become unnecessary later; keep each in one place so it's easy to remove.

- macOS: "macOS may say Lemmata can't be checked for malware. Open System Settings → Privacy & Security and click Open Anyway."
- Windows: "If SmartScreen warns you, click More info → Run anyway."

## Brand

Take the design tokens from `pitch.html`:

- Accent: non-photo blue (`#2f7fb8` light, `#6fb3e3` dark), the proofreader's pencil colour
- Neutrals: cool paper grey `#f3f4f2`, ink `#18212b`
- Fonts: Young Serif for display, IBM Plex Sans for body, IBM Plex Mono for code and commands
- Light and dark themes, following the system setting
- The name means "small proven steps toward a theorem". Tone: plain, precise, a little academic. No hype words.

Commands on the site should have a copy button.

## Out of scope

- No analytics or tracking scripts without a consent prompt; ideally none.
- No accounts, newsletter or comment system.
- Don't host the binaries yourself; link to GitHub release assets.
