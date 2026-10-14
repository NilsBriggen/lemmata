# Changelog

All notable changes to Lemmata.
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

### Fixed
- Autosave no longer looks like an outside change; deleted folders close quietly
## 1.0.0-rc.1 (2026-10-13)

### Added
- Environment pairing, project-aware completion, folding, shortcuts
- Formatting toolbar, insert figure/table/symbol dialogs, word count, shortcut sheet
- Lazy rendering, text selection, search, links, outline, presentation mode
- Draft mode, stop on first error, timeout, clear cache; package fixes and .blg diagnostics
- File tree drag-to-move, viewer tabs, tab strip, whole-project restore, snapshot compare
- Notification history, completion status, lemmata watch
- Draft-mode indicator, focusable PDF pane; handoff update
- Homework template; fix the PDF-click e2e point

### Fixed
- Wait for the completion list; click the PDF by event
- Say so when a PDF click can't be resolved to source; e2e reports why

### Documentation
- 1.0 guide and readme; new fixtures and e2e cases
## 0.1.0-beta.2 (2026-10-13)

### Added
- Overleaf-style redesign and first-run welcome

### Fixed
- Import Overleaf bulk downloads
## 0.1.0-beta.1 (2026-10-13)

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

### Fixed
- Builds with \include'd chapters in subfolders
- Fetch-texlab.sh works with macOS's bash 3.2
- Provide per-architecture texlab for universal macOS builds
- Sign macOS builds ad hoc until a Developer ID exists

### Documentation
- Plan, website handoff and pitch
