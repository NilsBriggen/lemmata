# Changelog

All notable changes to Lemmata.

## 4.0.0-rc.5 (2026-11-25)

A major visual refresh paired with a comprehensive performance, security, and install-size overhaul—with zero features removed.

### Fixed (since rc.4)
- Awaited async `save_project_config` completion via `wait_for` in `build_targets_toolbar_picker_switches_main_and_engine` instead of a fixed 3-frame step count.

## 4.0.0-rc.4 (2026-11-25)

A major visual refresh paired with a comprehensive performance, security, and install-size overhaul—with zero features removed.

### Fixed (since rc.3)
- Normalized CRLF line endings across all assertions in the Windows figure/table insertion test and added retries for transient macOS `hdiutil: Resource busy` DMG packaging errors.

## 4.0.0-rc.3 (2026-11-25)

A major visual refresh paired with a comprehensive performance, security, and install-size overhaul—with zero features removed.

### Fixed (since rc.2)
- Fixed macOS Find-bar test flake caused by an async startup toast overlapping the bottom Find bar and normalized CRLF line endings in the Windows table-insertion test.

## 4.0.0-rc.2 (2026-11-25)

A major visual refresh paired with a comprehensive performance, security, and install-size overhaul—with zero features removed.

### Fixed (since rc.1)
- Canonicalized project trust lookups in `Settings::trust_of` / `set_trust` so Windows path normalization matches migrated pre-3.2 entries consistently.
- Fixed macOS & Linux CI test portability (synthetic momentum test gating on macOS, deterministic Welcome shortcut labels under tests, software-renderer pixel thresholds, and Find/notification test stability).

## 4.0.0-rc.1 (2026-11-25)

A major visual refresh paired with a comprehensive performance, security, and install-size overhaul—with zero features removed.

### Added
- **Refreshed UI across all screens:** Redesigned Library (grid and list views, sidebar navigation, search, folder organization), Workspace chrome, Preferences, Project settings, Bibliography manager, Symbols, and modal dialogs, paired with the new **Typeset Split** app icon set.
- **Guided 4-step Welcome & TeX setup:** New two-column setup shell with an animated miniature split-workspace preview, visual System/Light/Dark theme cards with live typography preview, grouped workflow preferences, starter action cards, and a unified TeX Live / Tectonic onboarding flow.

### Changed
- **Smaller install footprint:** Bundled Hunspell dictionaries (`en`, `en-GB`, `de`, `fr`) are now shipped gzip-compressed (cutting dictionary disk usage by ~70%), `wgpu` GPU backends are gated per target OS, release profiles are tuned for size on cold crates (plus a dedicated `panic = "abort"` profile for `lemmata-cli`), and `.rpm`/tarball/NSIS packaging uses stronger compression.
- **Tighter trust and path boundaries:** Hardened project path validation, shell/latexdiff command safety, SVG decompression guards, and settings persistence.

### Faster
- **Editor & UI responsiveness:** Cross-frame `LineLayout` (`Arc<Galley>`) caching, zero-copy `LayoutJob` slicing, in-place height updates on same-line edits, memoized bracket matching and math-under-cursor lookup, and a fast-path LaTeX syntax highlighter.
- **Zero idle CPU when unfocused:** Caret blink stops waking the app when the window is unfocused or minimized, redundant 30 ms PDF polling and per-frame `tectonic` disk `stat()` syscalls are removed, and toast timers wake once at exact expiry.
- **PDF, Math & SVG rendering:** Eliminated full-page pixel copies in light-mode PDF rendering, throttled multi-page GPU texture uploads, switched math preview cache to O(1) LRU ticks, and cached the SVG viewer's font database.
- **LaTeX builds, SyncTeX, BibTeX & Git:** Eliminated up to 9 redundant directory walks and double `.log` reads per build, offloaded build file hashing to blocking threads with `(len, mtime)` for binary assets, cached parsed `.bib` files across `.tex` autosaves, streamed `.synctex.gz` line-by-line to cut peak RAM, cached Git repo metadata with adaptive command backoff, sped up History full-project restore with `diff-tree`, and moved spellchecking to a persistent worker thread with a 2-entry dictionary LRU.

### Fixed
- Resolved dozens of UI state, accessibility, PDF/editor interaction, and edge-case bugs identified during a full codebase audit.

## 3.3.0 (2026-11-24)

A release of fixes and smaller features from a review of the whole app: more than 200 problems, from wrong clicks in the Compile menu to text that could be lost in rare cases.

### Added
- A Code block button in the edit bar. It lists common languages and inserts a `lstlisting` block, which needs no shell escape; when your document already loads minted, it inserts a minted block. The language names also complete after `\begin{minted}{`, `\mintinline{`, `\inputminted{` and `language=`.
- A warning before building when your document loads minted but shell escape can't run (it is off, the project isn't trusted, or the engine is Tectonic), with a button to Project settings.
- `\begin{` offers the environments of packages such as minted, algorithm, tcolorbox and mdframed, and offers to load the package.
- Build only some chapters: Project settings lists the chapters your main file `\include`s, and ticking some builds only those (`\includeonly`), trusted or not.
- Paste a BibTeX entry into the citation picker or the bibliography manager to add it. A key already taken gets a letter added; a DOI already in your library isn't added twice, also not from an online search.
- Right-click in the editor keeps the selection and offers Cut, Copy and Paste, plus the suggestions on a misspelled word.
- With word wrap off, long lines scroll sideways and the view follows the cursor.
- PDF text selection runs over page breaks, Ctrl+A selects the whole PDF, and copying waits for the text of pages that weren't loaded yet. Email links in a PDF open, and links work in PDF tabs too.
- TIFF and BMP figures open, pictures are turned as their camera recorded, and the real size is shown for pictures over 8192 pixels.
- History pages back past the newest 200 snapshots, takes a snapshot when you open a project that changed outside Lemmata, and says when a manual snapshot had nothing new.
- Citations are found by any co-author or title in the picker and in `\cite` completion.
- The command palette lists Undo and Redo and hides editor commands when no file is open.
- With many tabs open they narrow and scroll to the active one, show their folder when two have the same name, and show their path on hover. Closing a tab shows its neighbour.

### Changed
- Releases stop before publishing anything if a publishing secret or the release notes are missing, instead of publishing only in part.
- Saving a latin1 (Windows-1252) file keeps it latin1, also after all its special characters were deleted.
- Replace all saves unsaved edits and searches again first, so it changes exactly the lines it lists. When results stop at 2000, it says so and replaces only the listed ones.
- Git commands get ten minutes before they're stopped, a stopped command gets to remove its lock files, and the Git panel lists and stages only the project's folder when the repository is bigger.
- The raw log view shows the start and the end of logs over 1 MB.
- Dialogs with unsaved changes no longer close on a click beside them, and their × button no longer gets focus first.

### Faster
- PDF: pages land on screen first after a far jump or while searching, a PDF that fails to reload keeps showing the old pages, and the presentation keeps fewer textures.
- The editor no longer copies the whole text for screen readers when none is running, fold arrows don't scan whole sections on each key, and search no longer builds a table the size of the file.
- Comparing two very different versions of a file in History no longer takes up to a gigabyte of memory.
- Analysing the project after a save waits two seconds after the last one.

### Fixed
- The Compile menu did the wrong thing from "Normal" on: Fast (draft) turned draft off, Clear cached files turned on Stop at the first error, and Stop compiling did nothing.
- Projects that use BibTeX and aren't trusted got no bibliography on TeX Live; citations in `\include`d chapters are now found too.
- Enter after `\begin{X}` adds `\end{X}` also when arguments follow (`\begin{minted}{python}`, `\begin{figure}[h]`) and when the cursor is still before the closing brace.
- Typing in a long wrapped paragraph no longer makes the view jump, and Down walks through its rows.
- Code in verbatim, lstlisting and minted blocks and in `\verb` is no longer coloured or paired as LaTeX, and `\begin`/`\end` pairs with different names are underlined.
- One duplicate key, undefined `@string` or broken entry no longer hides a whole `.bib` file, and entries using `@string` macros show their fields.
- Arguments such as `[htbp]` and `{lcr}` after `\begin` are no longer spell checked, the spell-check language is read from `\documentclass` options and `main=`, and text sent for grammar keeps its numbers.
- Latin1 files are found by project search, rename, Find unused files and the bibliography manager.
- Restoring a snapshot never deletes a file just because Git couldn't read it.
- Builds without latexmk rerun when the contents or the lists of figures change and keep going past errors when you asked them to; `.bib` errors from biber or BibTeX are shown; a fatal error with no other message is explained.
- `!pdfTeX error`, font warnings and missing characters are reported, and errors after an overfull box point at the right file.
- Switching projects during a build no longer waits for that build to finish.
- Project replace with a pattern that can match nothing changes only what it found; editing the replacement keeps the lines you unticked.
- Quick changes in the Compile menu land in `lemmata.toml` in order and stay on screen.
- A PDF no longer loses its selection and reloads its pages when another PDF is open at the same time (a PDF tab or a presentation).
- Reloading a file changed outside keeps your cursors and folds; CRLF files keep CRLF line breaks; "Load the version on disk" that can't be read keeps your edits.
- Image and PDF tabs reload when their file changes and close when it's deleted.
- Deleting a file whose save failed says its edits are gone; a partial Undo of a move keeps the rest for later.
- Review comments pulled from Git are no longer overwritten by the next save.
- Ctrl+Shift+7 comments lines on German and French keyboards.
- On Windows, Lemmata's icon shows in Explorer, the Start menu, pinned taskbar entries and the window, instead of the generic one.
- Format document says which Perl module latexindent is missing (on Arch, for example) and how to install it, instead of printing Perl's search path.
- Completion offers the open environment first after `\end{`, leaves `\href` alone and keeps snippets out of paths and key lists.
- A clone can be left with Cancel again and is added to the library if it finishes later.
- Settings files that can't be read are kept as `.bak` before being replaced, and one badly shaped setting no longer resets all of them.
- Many smaller fixes in the editor, search, the file tree, the library, the PDF view, presentations, exports and the command-line tool.

## 3.3.0-rc.1 (2026-11-24)

A release of fixes and smaller features from a review of the whole app: more than 200 problems, from wrong clicks in the Compile menu to text that could be lost in rare cases.

### Added
- A Code block button in the edit bar. It lists common languages and inserts a `lstlisting` block, which needs no shell escape; when your document already loads minted, it inserts a minted block. The language names also complete after `\begin{minted}{`, `\mintinline{`, `\inputminted{` and `language=`.
- A warning before building when your document loads minted but shell escape can't run (it is off, the project isn't trusted, or the engine is Tectonic), with a button to Project settings.
- `\begin{` offers the environments of packages such as minted, algorithm, tcolorbox and mdframed, and offers to load the package.
- Build only some chapters: Project settings lists the chapters your main file `\include`s, and ticking some builds only those (`\includeonly`), trusted or not.
- Paste a BibTeX entry into the citation picker or the bibliography manager to add it. A key already taken gets a letter added; a DOI already in your library isn't added twice, also not from an online search.
- Right-click in the editor keeps the selection and offers Cut, Copy and Paste, plus the suggestions on a misspelled word.
- With word wrap off, long lines scroll sideways and the view follows the cursor.
- PDF text selection runs over page breaks, Ctrl+A selects the whole PDF, and copying waits for the text of pages that weren't loaded yet. Email links in a PDF open, and links work in PDF tabs too.
- TIFF and BMP figures open, pictures are turned as their camera recorded, and the real size is shown for pictures over 8192 pixels.
- History pages back past the newest 200 snapshots, takes a snapshot when you open a project that changed outside Lemmata, and says when a manual snapshot had nothing new.
- Citations are found by any co-author or title in the picker and in `\cite` completion.
- The command palette lists Undo and Redo and hides editor commands when no file is open.
- With many tabs open they narrow and scroll to the active one, show their folder when two have the same name, and show their path on hover. Closing a tab shows its neighbour.

### Changed
- Releases stop before publishing anything if a publishing secret or the release notes are missing, instead of publishing only in part.
- Saving a latin1 (Windows-1252) file keeps it latin1, also after all its special characters were deleted.
- Replace all saves unsaved edits and searches again first, so it changes exactly the lines it lists. When results stop at 2000, it says so and replaces only the listed ones.
- Git commands get ten minutes before they're stopped, a stopped command gets to remove its lock files, and the Git panel lists and stages only the project's folder when the repository is bigger.
- The raw log view shows the start and the end of logs over 1 MB.
- Dialogs with unsaved changes no longer close on a click beside them, and their × button no longer gets focus first.

### Faster
- PDF: pages land on screen first after a far jump or while searching, a PDF that fails to reload keeps showing the old pages, and the presentation keeps fewer textures.
- The editor no longer copies the whole text for screen readers when none is running, fold arrows don't scan whole sections on each key, and search no longer builds a table the size of the file.
- Comparing two very different versions of a file in History no longer takes up to a gigabyte of memory.
- Analysing the project after a save waits two seconds after the last one.

### Fixed
- The Compile menu did the wrong thing from "Normal" on: Fast (draft) turned draft off, Clear cached files turned on Stop at the first error, and Stop compiling did nothing.
- Projects that use BibTeX and aren't trusted got no bibliography on TeX Live; citations in `\include`d chapters are now found too.
- Enter after `\begin{X}` adds `\end{X}` also when arguments follow (`\begin{minted}{python}`, `\begin{figure}[h]`) and when the cursor is still before the closing brace.
- Typing in a long wrapped paragraph no longer makes the view jump, and Down walks through its rows.
- Code in verbatim, lstlisting and minted blocks and in `\verb` is no longer coloured or paired as LaTeX, and `\begin`/`\end` pairs with different names are underlined.
- One duplicate key, undefined `@string` or broken entry no longer hides a whole `.bib` file, and entries using `@string` macros show their fields.
- Arguments such as `[htbp]` and `{lcr}` after `\begin` are no longer spell checked, the spell-check language is read from `\documentclass` options and `main=`, and text sent for grammar keeps its numbers.
- Latin1 files are found by project search, rename, Find unused files and the bibliography manager.
- Restoring a snapshot never deletes a file just because Git couldn't read it.
- Builds without latexmk rerun when the contents or the lists of figures change and keep going past errors when you asked them to; `.bib` errors from biber or BibTeX are shown; a fatal error with no other message is explained.
- `!pdfTeX error`, font warnings and missing characters are reported, and errors after an overfull box point at the right file.
- Switching projects during a build no longer waits for that build to finish.
- Project replace with a pattern that can match nothing changes only what it found; editing the replacement keeps the lines you unticked.
- Quick changes in the Compile menu land in `lemmata.toml` in order and stay on screen.
- A PDF no longer loses its selection and reloads its pages when another PDF is open at the same time (a PDF tab or a presentation).
- Reloading a file changed outside keeps your cursors and folds; CRLF files keep CRLF line breaks; "Load the version on disk" that can't be read keeps your edits.
- Image and PDF tabs reload when their file changes and close when it's deleted.
- Deleting a file whose save failed says its edits are gone; a partial Undo of a move keeps the rest for later.
- Review comments pulled from Git are no longer overwritten by the next save.
- Ctrl+Shift+7 comments lines on German and French keyboards.
- Completion offers the open environment first after `\end{`, leaves `\href` alone and keeps snippets out of paths and key lists.
- A clone can be left with Cancel again and is added to the library if it finishes later.
- Settings files that can't be read are kept as `.bak` before being replaced, and one badly shaped setting no longer resets all of them.
- Many smaller fixes in the editor, search, the file tree, the library, the PDF view, presentations, exports and the command-line tool.

## 3.2.0 (2026-11-19)

A security and reliability release. Opening a project you got from someone else can no longer run code on your computer or write outside the project's folder.

### Changed
- Opening a project for the first time asks whether you trust its authors, before anything in it is read or run. Your answer is kept per folder and can be changed in Project settings. Projects you create from a bundled template are trusted without asking, unless the template brings a `latexmkrc` or turns on shell escape. Projects you already use ask once after the update, except those you had allowed shell escape for, which stay trusted.
- A restricted (untrusted) project still builds, but runs nothing of its own: the TeX engine is run directly instead of through latexmk, shell escape is off whatever `lemmata.toml` says, texlab can't run the project's `latexmkrc`, and the Git panel and "PDF of the changes" are off until you trust it.
- The command-line tool treats projects as restricted too: `lemmata build`, `watch`, `lint` and `export` honour a project's `latexmkrc` and shell escape only with `--trust`.
- History only uses a snapshot store Lemmata created on this computer. A store a project brings along is used once you trust the project.
- Git commands Lemmata runs on its own (snapshots, status, the Git panel's view) never run a repository's hooks or helpers and never reach the network.

### Added
- The command palette has the actions that only had a shortcut: Save, Close tab, Find, Go to line, Select all matches, Next and Previous problem, Next TODO, Next comment, Go to definition, Find references, Bold, Italic, heading level up and down, Fold all and Unfold all.
- `.bib` files are highlighted: entry types, keys, field names, braces and comments.
- Words in `\foreignlanguage` and `otherlanguage` are counted but not spell checked.
- The window's zoom is kept for the next start.

### Faster
- Saving no longer reads and hashes every project file up to three times: a build hashes once, and files over 1 MB (figures, data) are compared by size and modification time.
- Long lists draw only the rows in view: project search, the outline and the bibliography manager. The citation picker matches once per search instead of four times a frame.
- Typing in very long files: renaming an environment in a 20,000-line file is about five times faster per key, and review comments no longer slow every key down.

### Fixed
- Two cursors whose deletions overlap (for example Ctrl+Backspace with two cursors in one word) no longer crash the app.
- "Sync from Zotero" with Zotero closed no longer empties `zotero.bib`.
- A file another program changed is reloaded without being counted as an unsaved edit, so it is no longer written back, and after a Git branch switch the old branch's text can't overwrite the new one.
- Deleting a file saves what you typed since the last save first, so Undo brings back the latest version.
- A main file with a dot in its name (`paper.v2.tex`) shows its PDF.
- Deleted files leave the project's trash after a week, as the delete dialog says.
- Replace all no longer replaces at stale positions after the text changed since the search.
- Undo in the file tree waits for saves, so undoing a new file or a rename leaves no copy behind.
- Cut with a selection and an empty cursor no longer deletes a character the clipboard never got.
- Review comments move with a renamed or moved file.
- Files that aren't UTF-8 (Latin-1 or Windows-1252) open, are saved back in Windows-1252, and are exported to arXiv unchanged.
- An edit to a file outside the project that the document reads (`\input{../common}`, a `.sty` in TEXMFHOME, `~/library.bib`) counts as a change, and the fast rebuild reruns biber or BibTeX for a changed outside `.bib`.
- Warnings point at the right file when its name contains spaces.
- Session restore finishes when a tab's file can't be read or all tabs are images and PDFs.
- F12 says why when texlab can't answer yet; renaming a label says when History can't undo it.
- A cursor between the two characters of a Windows line break no longer crashes word moves or Enter, and a completion from before an edit is no longer applied at old positions.
- A grammar check that answers after the text got shorter no longer crashes the app.
- Saving a project as a template copies the files of linked folders.
- `lemmata watch` keeps watching after a failed build.
- `lemmata build` with nothing changed says the PDF is up to date instead of "no PDF written".
- Opening the last project at start no longer says "TeX isn't installed yet" while TeX is still being found.
- PDF pages sometimes stayed blank after switching documents quickly.
- On Windows, moving a project to the trash right after closing it no longer fails because its language server or a background preamble build was still running in the folder.
- On Windows, the outline of a restricted project follows `\input` files in subfolders.
- Security: project files that are crafted to harm, such as deeply nested or looping bibliographies, absurdly sized PDF pages and images, oversized SyncTeX files, links to endless files or named pipes, and unusual file names, can no longer crash the app, fill memory, hang it or reach outside the project; file saves can't be redirected by links planted in a project, and in a restricted project no write follows a link out of it; `lemmata://open` links refuse network paths; and the updater only installs a build signed after the running version.

## 3.2.0-rc.1 (2026-11-19)

The first release candidate for 3.2.0. A security and reliability release. Opening a project you got from someone else can no longer run code on your computer or write outside the project's folder.

### Changed
- Opening a project for the first time asks whether you trust its authors, before anything in it is read or run. Your answer is kept per folder and can be changed in Project settings. Projects you create from a bundled template are trusted without asking, unless the template brings a `latexmkrc` or turns on shell escape. Projects you already use ask once after the update, except those you had allowed shell escape for, which stay trusted.
- A restricted (untrusted) project still builds, but runs nothing of its own: the TeX engine is run directly instead of through latexmk, shell escape is off whatever `lemmata.toml` says, texlab can't run the project's `latexmkrc`, and the Git panel and "PDF of the changes" are off until you trust it.
- The command-line tool treats projects as restricted too: `lemmata build`, `watch`, `lint` and `export` honour a project's `latexmkrc` and shell escape only with `--trust`.
- History only uses a snapshot store Lemmata created on this computer. A store a project brings along is used once you trust the project.
- Git commands Lemmata runs on its own (snapshots, status, the Git panel's view) never run a repository's hooks or helpers and never reach the network.

### Added
- The command palette has the actions that only had a shortcut: Save, Close tab, Find, Go to line, Select all matches, Next and Previous problem, Next TODO, Next comment, Go to definition, Find references, Bold, Italic, heading level up and down, Fold all and Unfold all.
- `.bib` files are highlighted: entry types, keys, field names, braces and comments.
- Words in `\foreignlanguage` and `otherlanguage` are counted but not spell checked.
- The window's zoom is kept for the next start.

### Faster
- Saving no longer reads and hashes every project file up to three times: a build hashes once, and files over 1 MB (figures, data) are compared by size and modification time.
- Long lists draw only the rows in view: project search, the outline and the bibliography manager. The citation picker matches once per search instead of four times a frame.
- Typing in very long files: renaming an environment in a 20,000-line file is about five times faster per key, and review comments no longer slow every key down.

### Fixed
- Two cursors whose deletions overlap (for example Ctrl+Backspace with two cursors in one word) no longer crash the app.
- "Sync from Zotero" with Zotero closed no longer empties `zotero.bib`.
- A file another program changed is reloaded without being counted as an unsaved edit, so it is no longer written back, and after a Git branch switch the old branch's text can't overwrite the new one.
- Deleting a file saves what you typed since the last save first, so Undo brings back the latest version.
- A main file with a dot in its name (`paper.v2.tex`) shows its PDF.
- Deleted files leave the project's trash after a week, as the delete dialog says.
- Replace all no longer replaces at stale positions after the text changed since the search.
- Undo in the file tree waits for saves, so undoing a new file or a rename leaves no copy behind.
- Cut with a selection and an empty cursor no longer deletes a character the clipboard never got.
- Review comments move with a renamed or moved file.
- Files that aren't UTF-8 (Latin-1 or Windows-1252) open, are saved back in Windows-1252, and are exported to arXiv unchanged.
- An edit to a file outside the project that the document reads (`\input{../common}`, a `.sty` in TEXMFHOME, `~/library.bib`) counts as a change, and the fast rebuild reruns biber or BibTeX for a changed outside `.bib`.
- Warnings point at the right file when its name contains spaces.
- Session restore finishes when a tab's file can't be read or all tabs are images and PDFs.
- F12 says why when texlab can't answer yet; renaming a label says when History can't undo it.
- A cursor between the two characters of a Windows line break no longer crashes word moves or Enter, and a completion from before an edit is no longer applied at old positions.
- A grammar check that answers after the text got shorter no longer crashes the app.
- Saving a project as a template copies the files of linked folders.
- `lemmata watch` keeps watching after a failed build.
- `lemmata build` with nothing changed says the PDF is up to date instead of "no PDF written".
- Opening the last project at start no longer says "TeX isn't installed yet" while TeX is still being found.
- PDF pages sometimes stayed blank after switching documents quickly.
- On Windows, moving a project to the trash right after closing it no longer fails because its language server or a background preamble build was still running in the folder.
- On Windows, the outline of a restricted project follows `\input` files in subfolders.
- Security: project files that are crafted to harm, such as deeply nested or looping bibliographies, absurdly sized PDF pages and images, oversized SyncTeX files, links to endless files or named pipes, and unusual file names, can no longer crash the app, fill memory, hang it or reach outside the project; file saves can't be redirected by links planted in a project, and in a restricted project no write follows a link out of it; `lemmata://open` links refuse network paths; and the updater only installs a build signed after the running version.

## 3.1.1 (2026-11-08)

A bugfix release.

### Fixed
- Accept a completion by clicking it: the list no longer closes before the click lands.
- Export a just-built PDF without being told it is out of date: an autosave just before Ctrl+S builds no longer counts as an unbuilt edit. Freshness now compares file contents with what the last good build read, so a Ctrl+S with nothing changed also reuses that PDF.
- Choose "Build and export" or "Export older PDF" in one notice when the PDF really is out of date.

## 3.1.0 (2026-11-04)

A reliability, performance, and preview release.

### Added
- Navigate build problems across files with F8 and Shift+F8: stable file/line ordering, wraparound, destination focus, and an accessible “Problem N of M” status.
- Preview SVG figures in viewer tabs and hover cards with bundled fonts, transparency, embedded PNG support, offline rendering, and hard input/pixel limits.
- Download smaller ARM64 or Intel macOS DMGs alongside the existing universal alternative; Homebrew selects the correct architecture automatically.

### Faster
- Virtualize PDF page and thumbnail traversal, with unchanged-frame medians falling from 0.384 ms to 0.103 ms for pages and 4.779 ms to 0.130 ms for thumbnails.
- Cache accessibility snapshots, PDF search text, thumbnail geometry, and texture budgets to reduce repeated work and memory use.
- Reuse one source snapshot for outline, labels, and bibliography discovery.
- Align direct skrifa with hayro at 0.49, removing the duplicate 0.46 stack and shrinking the stripped binary and package.

### Fixed
- Preserve edits across project opens, fresh exports, stale responses, save retries, external conflicts, and failed writes.
- Make Add package and template operations transactional; preserve backups after install-and-restore double failures.
- Keep PDF/SyncTeX errors visible, repair toolbar and status controls, stop CLI watch output loops, and complete migration only after a successful scan.
- Harden project and preview lifecycle: stale preflights/export dialogs cannot affect newer projects, rollback preserves external destinations, and legacy read failures do not write empty defaults.
- Block external SVG resources and reject compressed SVG before unbounded decompression.

### Notes
- AppImage xz, editor offset caching, and latest-only automatic build queueing were evaluated and rejected because they did not meet their performance gates.
- MSI compression and Apple/Windows signing require upstream support and organizational credentials; this release retains existing updater minisign behavior.

### Verification
- 646 tests pass; clippy passes with warnings denied. Native macOS/Windows installation and hardware-specific validation remain to be checked separately.

## 3.0.1-rc.6 (2026-11-03)

## 3.0.1-rc.5 (2026-11-03)

### Added
- Improve PDF and compile workflows

## 3.0.1-rc.4 (2026-11-03)

## 3.0.1-rc.3 (2026-11-03)

### Fixed
- Render every active PDF document

## 3.0.1-rc.2 (2026-11-03)

## 3.0.1-rc.1 (2026-11-03)

### Fixed
- Harden project work and desktop performance

## 3.0.0 (2026-10-23)

Lemmata 3 is the same editor rebuilt as one native program, without a web view. Your settings, recent projects, library folders and where you were in each project carry over from 2.x.

### Changed
- Lemmata is a native app (egui on wgpu): the download is about a third smaller, and texlab downloads on first use instead of shipping with every copy.
- Lemmata checks for and installs updates itself, from the same signed manifest. On Linux the AppImage updates itself; .deb and .rpm installs update through the package manager. On a Mac an update downloads only that Mac's architecture.
- It runs on computers without Vulkan or DirectX 12 (virtual machines, older graphics) through OpenGL.

### Added
- Problems show as you type: an unclosed brace, a reference to a missing label or a citation key that isn't in your bibliography is marked before you build.
- Your own keyboard shortcuts: in Keyboard shortcuts (F1), click a shortcut's keys and press new ones.
- F7 and F9 step through TODOs and review comments; Ctrl+Shift+I reindents.
- Preferences for line numbers, Reduce motion, and how soon after typing a save happens.
- Touchpad scrolling coasts after you lift your fingers, and a pinch over the PDF zooms the pages (on Linux under X11, Ctrl and scroll does).

### Fixed
- A downloaded project can't run programs or delete files outside itself: not through its git settings, its main file's name, its build folder or a zip, nor through its latexmkrc until you trust it.
- Unsaved edits survive a file deleted by another program, switching projects and a crash during a save.
- A PDF is rebuilt when a file was deleted or synced in with an old date, and biber runs again after a .bib edit made during a build.
- PDFs made with MiKTeX's bitmap fonts can be searched, and their text selected.
- On Windows, reviewing changes since a snapshot no longer marks every line of a file with Windows line endings, and the library shows every project's thumbnail.

### Faster
- Typing in a 20,000-line file takes milliseconds, and resizing the window stays quick.
- Pages after a rebuild draw side by side, and dark pages draw faster.
- Lemmata starts sooner on Windows, and dictionaries and templates download compressed.

### Removed
- The Vim keymap. A saved Vim setting is ignored.

## 3.0.0-rc.4 (2026-10-21)

### Faster
- A running build takes less CPU to show its progress.
- Undo, typing and resizing the window stay quick in very long files.
- The file tree and the bibliography stay quick in projects with thousands of files or entries.

### Smaller
- Lemmata holds less memory over a long session: maths previews, images in hover cards and the PDF's SyncTeX index are kept within bounds.
- The program is about 350 KB smaller.

## 3.0.0-rc.3 (2026-10-21)

### Fixed
- An open project no longer wakes the app every second on Linux.

### Faster
- Lemmata starts sooner on Windows, and the start screen no longer waits for project thumbnails.
- Dark PDF pages draw faster, and the page in view is drawn before the ones around it.
- Dictionaries and templates download compressed: 36 MB becomes 14 MB for Turkish.

### Smaller
- The .deb is 28% smaller.
- The macOS disk image is smaller, and an update on a Mac downloads only that Mac's architecture, about half the size.

## 3.0.0-rc.2 (2026-10-21)

### Fixed
- Touchpad scrolling coasts and pinch zooms the PDF on Wayland, the editor toolbar stays clear of the PDF, labels and the folder header behave

## 3.0.0-rc.1 (2026-10-21)

Lemmata 3 is the same editor rebuilt as one native program, without a web view. This release candidate is for trying it before 3.0.0; the updater offers it to nobody.

### Changed
- Lemmata is a native app (egui on wgpu): the download is about a third smaller, and texlab downloads on first use instead of shipping with every copy.
- Settings, recent projects, library folders and where you were in each project carry over from 2.x.
- A touchpad pinch over the PDF zooms the pages on every system, and no longer the whole window on Linux.
- Lemmata checks for and installs updates itself, from the same signed manifest. On Linux only the AppImage updates itself; .deb and .rpm installs update through the package manager.

### Removed
- The Vim keymap. A saved Vim setting is ignored.

### Fixed
- A downloaded project can't run programs through its latexmkrc until you trust it, and damaged files can't crash the app.
- Edits survive project switches, reloads and failed saves, and restoring, deleting and moving files behave under real use.

## 2.1.3 (2026-10-20)

### Fixed
- On Windows, saving while a build reads the file no longer fails with "Access is denied"

## 2.1.2 (2026-10-19)

### Fixed
- The window no longer slides up out of view; the editor's tooltip layer made the page twice as tall

## 2.1.1 (2026-10-19)

### Fixed
- Project settings save again when the build timeout is the default
- Spelling, error and grammar underlines show again in the Linux AppImage

### Documentation
- A README that describes Lemmata 2.1

## 2.1.0 (2026-10-16)

### Added
- Long lines wrap under their own indentation, and faint guides show how environments nest
- Indentation follows environments as you type: Enter inside one indents a level, and a typed `\end{…}` lines up with its `\begin`
- Reindent lines (main menu, command palette or Ctrl+Alt+\) cleans up the indentation of the selection, or of the whole file
- The start screen shows your folders first, then the projects in none: drag a project onto a folder, or use Move to folder… in its menu

## 2.0.3 (2026-10-16)

### Fixed
- On Linux, a touchpad pinch zooms the PDF instead of magnifying the whole window

## 2.0.2 (2026-10-16)

### Fixed
- Linux AppImage: git push and pull also work after updating from inside the app (2.0.1 only fixed a fresh start)
- The maths preview is no longer cut off by the PDF next to the editor
- A zoomed-in PDF scrolls to its left edge too
- Autocomplete offers the commands of loaded packages, like \Needspace (none of the language server's commands were shown before)
- Completion and go-to-definition work when texlab takes more than 5 seconds to start (they were off for the session, with errors)
- Windows: a closed project can be deleted or saved as a template right away; texlab kept running in its folder until the next project opened
- No internal error when switching to a file without autocomplete (like a .bib file) right after typing

## 2.0.1 (2026-10-15)

### Fixed
- Linux AppImage: git push, pull and clone over HTTPS work again. Programs Lemmata starts (git, latexmk, texlab, the terminal, the PDF viewer, the browser) now use the system's libraries instead of the AppImage's

## 2.0.0 (2026-10-15)

### Added
- Git, built in: commit, pull, push and switch branches from the Git panel, and clone a project from GitHub, GitLab or Overleaf
- Review changes against a snapshot and keep or undo each one, or get a PDF with the changes marked (latexdiff)
- Tectonic as an optional engine, downloaded on demand: compile without installing TeX, on Linux, macOS and Windows
- 30 templates from the Lemmata template collection (papers, theses, posters, slides, exams, letters, and journal and conference classes), with the six basic ones built in; New project starts with six kinds of document
- Save any project as a template from its menu, and optionally move the project to the trash
- 14 more spell-check languages, downloaded on demand, and an offer to download one when a file asks for it
- Open .tex files and project folders from the system, and lemmata:// links that open, clone or start a project
- Export for arXiv as one .tex file
- The command-line tool on every release on its own, with lint, wordcount, bib, outline and export (--zip, --arxiv)
- A short What's new window after an update
- \be offers \begin on its own, then the list of environments
- Git and Cite buttons, a grouped main menu, side panels that always show, a ⋯ menu in a narrow PDF toolbar, and a clickable word count, compiler and language in the status bar
- Recently deleted restores deleted files, also after a restart; Project settings lists the dictionary's words and ignored grammar rules so they can be removed
- Hover cards for an unknown citation or label offer a search

### Fixed
- Only one cursor after a snippet that typed in two places, such as \begin{…}…\end{…}
- A build after a stopped or failed one always runs, so the PDF and the jump to the source stay current
- Opening a project trusts its PDF only if nothing changed after its build began
- A very large image after another one is no longer blank in the PDF
- Images in the project open instead of loading forever
- Inserted figures, tables and lists keep the cursor's indentation
- Git asks for a name and email instead of guessing them
- Ctrl+Alt+M adds a review comment, and Ctrl+/ only comments lines
- A missing biber is explained on Windows too, and the jump between PDF and source explains a missing synctex file
- Offers to act stay on screen until answered; deleting the open project works on Windows

## 1.5.2 (2026-10-15)

### Fixed
- An edit saved during a build was left out of the next one
## 1.5.1 (2026-10-15)

### Fixed
- Editor styles were blocked; PDFs failed on older WebView2
- Reading a file no longer counts as a change
- Jump from the PDF to a file in a subfolder
## 1.5.0 (2026-10-15)

### Added
- Search for references online and manage the bibliography
- Grammar checking with LanguageTool (opt-in)
- Hover cards for references, citations and figures
- TODO list and Format document
- Page thumbnails, two-page view and open in another PDF viewer
- Review comments anchored in the text
- Wire up the 1.5 writing features
- Plain-language explanations for 31 more TeX errors
- % !TeX root and % !TeX program magic comments

### Fixed
- Grammar settings apply at once; scenarios for hover cards and grammar

### Faster
- Precompile the preamble for warm rebuilds (40–60% faster)

### Documentation
- Guide for 1.5
## 1.4.0 (2026-10-15)

### Faster
- Read SyncTeX data in process
- Faster start and rebuilds
- Lighter PDF viewer, loaded when needed
- Incremental spell checking
- No git outside repositories; thumbnails as they come into view
- Open projects and complete without waiting
## 1.3.1 (2026-10-15)

### Fixed
- Stopping a build only signals the build's own processes
## 1.3.0 (2026-10-15)

### Added
- History is on for every project; restores keep unsaved edits
- Open file and tab menus from the keyboard
- Undo folder delete, sort by name, visible controls on the start screen
- Searchable shortcut sheet, F1, mouse actions and a Go to PDF button
- Work without TeX, clearer build state, start a folder with no main file

### Fixed
- Save before closing, run commands off the UI thread, stricter paths
- Never overwrite a change another program made to an open file
- No edits lost when tabs close, and failures say so
- Builds with the main file in a subfolder, stuck builds stop, safer clean
- ArXiv export keeps % in verbatim, \verb and URLs
- Renames follow the config, multi-file edits are all or nothing
- Lemmata.toml keeps its comments; clear errors for a missing main file
- Find TinyTeX and MacPorts; no Xcode tools prompt on macOS
- Presenting survives a rebuild and a quick Esc
- Readable contrast and proper roles for menus and the palette
- Stale builds don't run, shell escape needs approval, settings are checked
- No console window for SyncTeX on Windows; \verb isn't prose

### Faster
- Load dialogs, KaTeX and Vim when first needed

### Documentation
- Guide for 1.3
## 1.2.3 (2026-10-14)

### Added
- One menu per project on the start screen, with Delete project

### Fixed
- File tree menu actions did nothing
- Table templates put the caption below the table
## 1.2.2 (2026-10-14)

### Fixed
- The build without latexmk skips what has not changed
## 1.2.1 (2026-10-14)

### Fixed
- Build without latexmk when it cannot start; start without waiting for TeX detection
- Identify TeX from its install path instead of launching it
## 1.2.0 (2026-10-14)

### Added
- Presenter view with speaker notes

### Fixed
- Presenting no longer uses the browser's Fullscreen API
## 1.1.4 (2026-10-14)

### Fixed
- Recognise MiKTeX's update prompt in any language
## 1.1.3 (2026-10-14)

### Added
- Environment completion fills in tables and offers every \begin block
- Folders on the start screen

### Fixed
- Tables ask for booktabs, and the label goes below the table
## 1.1.2 (2026-10-14)

### Added
- New tables come prefilled, with Tab stepping through the cells

### Fixed
- A build folder can't point outside the project; one build at a time
- Editor, notices and docs from the review
## 1.1.1 (2026-10-14)

### Added
- Choose the indentation width, 4 spaces by default

### Fixed
- The PDF comes back after switching to Editor only and back
## 1.1.0 (2026-10-14)

### Added
- File manager, project navigation and build targets

### Fixed
- Explain builds that stop before TeX starts, and MiKTeX without Perl
## 1.0.0 (2026-10-13)

### Added
- Editor, live PDF preview, SyncTeX and TeX setup (M0)
- Install commands for missing packages
- Completion, hover and go to definition via texlab, plus an outline
- Project library, templates, Overleaf import and project settings
- Project history with snapshots, diffs and restore
- Spell check and word count
- Preferences, auto-update, Vim keys and dark PDF pages
- Writing aids: citation picker, references by DOI, figures, tables, math preview
- Project tools and comfort features
- Zotero citations through Better BibTeX
- Report a problem, user guide and end-to-end tests
- Overleaf-style redesign and first-run welcome
- Environment pairing, project-aware completion, folding, shortcuts
- Formatting toolbar, insert figure/table/symbol dialogs, word count, shortcut sheet
- Lazy rendering, text selection, search, links, outline, presentation mode
- Draft mode, stop on first error, timeout, clear cache; package fixes and .blg diagnostics
- File tree drag-to-move, viewer tabs, tab strip, whole-project restore, snapshot compare
- Notification history, completion status, lemmata watch
- Draft-mode indicator, focusable PDF pane; handoff update
- Homework template; fix the PDF-click e2e point

### Fixed
- Builds with \include'd chapters in subfolders
- Fetch-texlab.sh works with macOS's bash 3.2
- Provide per-architecture texlab for universal macOS builds
- Sign macOS builds ad hoc until a Developer ID exists
- Import Overleaf bulk downloads
- Wait for the completion list; click the PDF by event
- Say so when a PDF click can't be resolved to source; e2e reports why
- Autosave no longer looks like an outside change; deleted folders close quietly

### Documentation
- Plan, website handoff and pitch
- 1.0 guide and readme; new fixtures and e2e cases
