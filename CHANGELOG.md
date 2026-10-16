# Changelog

All notable changes to Lemmata.

## 2.0.2 (2026-10-16)

### Fixed
- Linux AppImage: git push and pull also work after updating from inside the app (2.0.1 only fixed a fresh start)
- The maths preview is no longer cut off by the PDF next to the editor
- A zoomed-in PDF scrolls to its left edge too
- Autocomplete offers the commands of loaded packages, like \Needspace (none of the language server's commands were shown before)
- Completion and go-to-definition work when texlab takes more than 5 seconds to start (they were off for the session, with errors)

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
