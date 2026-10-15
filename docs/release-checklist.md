# Release candidate checklist

Run this on real machines for every `-rc.N` before the final release. CI covers the app's behaviour (unit tests, every template, the end-to-end scenarios on Linux and Windows); this list covers what only a real computer shows: installers, the system's file and link handling, other people's TeX setups, real accounts and real documents. Tick each item on each system you test, and note the rc and OS next to anything that fails.

Systems: Linux (the AppImage and the .deb or .rpm), Windows 10 or 11, macOS (Apple silicon; Intel if one is at hand).

## Install and update

- [ ] A fresh install starts and shows the welcome screen.
- [ ] Updating from the previous release through the in-app updater works, keeps settings and recent projects, and shows the What's new window once (not again after a restart).
- [ ] The installer replaces an older version cleanly (Windows: Apps & features shows one Lemmata).
- [ ] macOS: the app opens from the DMG without a Gatekeeper dead end (right-click, Open, if it is ad hoc signed).

## Without TeX

On a machine (or user account) without any TeX distribution:

- [ ] The welcome screen offers Tectonic; **Use Tectonic (no install)** downloads it and the size shown matches.
- [ ] A new project from the Article template compiles; the PDF pane counts the packages while the first build fetches them.
- [ ] The new project's settings show Tectonic as the compiler.
- [ ] A project with biblatex and biber explains that biber doesn't work with Tectonic and suggests BibTeX.

## With TeX

- [ ] TeX Live (Linux, macOS) and MiKTeX (Windows) are found without help.
- [ ] Windows, MiKTeX with Perl missing or blocked: documents still build (Lemmata runs pdflatex directly) and the note explains why.
- [ ] A large thesis (100+ pages, many chapters and figures, biber): first build, an edit and rebuild, SyncTeX both ways, no lag while typing.
- [ ] A project in a cloud-synced folder (Dropbox, OneDrive or iCloud): builds, saves and outside edits behave; the sync client doesn't fight with the build folder.

## Opening things from the system

- [ ] Double-clicking a `.tex` file opens its project, with Lemmata closed and with it running (the running window comes forward).
- [ ] **Open with Lemmata** on a `.tex` file works.
- [ ] A file whose project root is found from `% !TeX root` opens in the right project.
- [ ] `lemmata://new?template=ieee` in a browser opens the New project gallery with IEEE chosen.
- [ ] `lemmata://clone?url=https://github.com/…` opens Clone a repository filled in, and clones only after you confirm.
- [ ] `lemmata://open?path=…` opens that project.

## Online features

- [ ] New project lists the template collection's templates with previews; one of them creates a project that builds. With the network off afterwards, the same templates are still offered.
- [ ] Preferences: choosing a spell-check language that isn't installed downloads it and spell check works in it.
- [ ] A project with `% !TeX spellcheck = es` (not installed) offers the download in a notice.
- [ ] References: online search by title finds a paper and adds it.

## Git and history

- [ ] Clone a real GitHub repository (https), edit, commit, push; pull a change made elsewhere.
- [ ] Clone an Overleaf project by its git URL with an Overleaf token; push a change and see it on Overleaf.
- [ ] Without a git name and email configured, the panel explains the two commands instead of committing.
- [ ] History: restore a snapshot, review changes since a snapshot (accept and reject), and **PDF of the changes** (needs latexdiff; with ulem missing, the message says how to install it).

## Presenting and screens

- [ ] Present full screen, then Esc at once: the window doesn't stay full screen.
- [ ] Two screens: the presenter view shows notes and the timer on one, slides on the other; ending the talk closes both.

## Export

- [ ] Export for arXiv, and as one .tex file: the zip compiles on its own (unzip it elsewhere and build it).
- [ ] Export the project as a zip and the PDF.

## macOS only

macOS has no end-to-end tests in CI, so these get extra care:

- [ ] Opening a `.tex` file from Finder while Lemmata is running (an Apple event, not a new launch).
- [ ] `lemmata://` links from Safari reach the running app.
- [ ] Tectonic downloads and compiles on Apple silicon (and Intel, if available).
- [ ] Shortcuts use ⌘ where the guide says so; the menu bar works.
- [ ] Homebrew: `brew install --cask nilsbriggen/tap/lemmata` installs this version.

## Sign-off

Ship the final release when an rc has gone through this list on all three systems with nothing open, and no new bug has been reported against it for a week.
