# Lemmata user guide

Lemmata is a LaTeX editor that builds your document as you write and shows the PDF next to the source. Projects are ordinary folders; nothing is uploaded anywhere.

## First launch

Lemmata needs a TeX distribution (TeX Live, or MiKTeX on Windows). If it can't find one, it shows the install command for your system. **Open terminal** shows that command in a terminal and waits for you to press Enter; Lemmata never installs anything by itself. Afterwards click **Check again**. If TeX is installed somewhere unusual, choose its `bin` folder.

## Projects

- **New project** starts from a template: article, thesis, slides, CV, letter or homework (exercise sheets with problem boxes and a name-and-course header).
- **Open folder** works with any folder containing a `.tex` file with `\documentclass`. If there are several, Lemmata asks which one is the main document.
- **Import Overleaf .zip** takes the zip from Overleaf's *Menu → Download → Source*.

Per-project settings (main file, engine, bibliography tool, spell-check language, shell escape) are stored in `lemmata.toml` in the project folder, so they travel with it.

## Writing and building

Lemmata saves shortly after you stop typing and builds after each save. In **Preferences** you can change the save delay or save only with Ctrl+S, and in the **Recompile** menu you can turn auto-compile off so the document only builds when you press Recompile. The build output goes to `.lemmata/build`, which is kept out of git.

The Recompile menu also has:

- **Fast (draft)**: figures are drawn as empty boxes, which makes big documents build quicker. Switch back to Normal for the final PDF.
- **Stop at the first error**: instead of pushing on to a complete PDF.
- **Clear cached files**: deletes auxiliary files, for the odd case where a stale `.aux` confuses LaTeX.

A build that runs longer than five minutes is stopped and explained (change `timeout_secs` in `lemmata.toml`, or set it to 0 for no limit).

Errors and warnings appear in the **Problems** panel in plain language, with a few lines of source and a hint where one is known. Click one to jump to its line (**F8** and **Shift+F8** step through problems in the editor). Some come with a one-click fix: **Add `\usepackage{amssymb}`** when a command like `\mathbb` is undefined, or the exact install command when a package is missing from your TeX installation. Problems from the bibliography tool (a missing `.bib` file, a broken entry) are listed too.

## The editor

- **Environments stay paired.** Type `\begin{itemize` and accept the suggestion: Lemmata adds the body and the matching `\end{itemize}`. Rename either half and the other follows. After a `\begin{name}` with no `\end`, **Enter** adds it.
- **Completion** suggests environments, commands, `\ref` labels from every file, `\cite` keys (with author and title), file names for `\input` and `\includegraphics`, and package names for `\usepackage`. texlab adds more when it's installed.
- **Folding**: click the arrow in the gutter to fold a section, environment or block of comments.
- **Formatting**: Ctrl+B and Ctrl+I toggle bold and italic, Ctrl+/ comments lines, Ctrl+] and Ctrl+[ change a heading's level, Ctrl+Shift+E wraps the selection in an environment. The toolbar above the editor has buttons for these, headings, links, figures, tables, lists and citations.
- **Insert dialogs**: **Figure** (pick an image from the project or your computer, caption, label, width), **Table** (choose the size on a grid) and **Symbols** (about 300 maths symbols, Ctrl+Shift+M).
- Everything above can be switched off in **Preferences → Editor**, together with the font, line height and line wrapping.

## The PDF

- **Double-click** (or Ctrl+click) in the PDF to jump to the source line.
- **Ctrl+J** in the editor shows the current line in the PDF.
- Text in the PDF can be **selected and copied**. **Ctrl+F** with the PDF focused searches it and highlights every match.
- Links in the PDF work: `\ref`, the table of contents and URLs.
- The **outline** button lists the PDF's bookmarks (load `hyperref` to get them).
- **Present full screen** shows one page at a time for Beamer slides: arrow keys, Space or a click move on, Esc leaves.

## Shortcuts

Press **Ctrl+Shift+/** for the full list inside Lemmata. The most used:

| Keys | Action |
|---|---|
| Ctrl+Enter | Recompile |
| Ctrl+S | Save now |
| Ctrl+P | Open a file by name |
| Ctrl+Shift+P | Command palette: every action |
| Ctrl+Shift+F | Search and replace in the project |
| Ctrl+Shift+C | Insert a citation |
| Ctrl+Shift+M | Insert a symbol |
| Ctrl+Shift+S | Save a named snapshot |
| Ctrl+B / Ctrl+I | Bold / italic |
| Ctrl+/ | Comment or uncomment |
| Ctrl+W, Ctrl+Tab | Close tab, next tab |
| Ctrl+J | Show this line in the PDF |
| F8 / Shift+F8 | Next / previous problem |
| F12 / Shift+F12 | Go to definition / find references (labels, citations, files) |
| F2 | Rename a label everywhere |
| F11 | Focus mode |

On macOS, use Cmd instead of Ctrl.

## Citations and references

**Ctrl+Shift+C** searches your `.bib` files by author, title, year or key. Inside an open `\cite{` it adds the key; elsewhere it inserts a new `\cite{…}`.

To add a reference, paste a DOI (`10.1038/nature14539`) or an arXiv ID (`arXiv:1707.06347`) into the picker. Lemmata looks it up on doi.org, gives it a readable key such as `schulman2017proximal` and appends it to the `.bib` file you choose. This lookup is the only time Lemmata goes online for references.

### Zotero

With Zotero and the [Better BibTeX](https://retorque.re/zotero-better-bibtex/) plugin running, **Cite from Zotero** opens Zotero's own citation picker. Lemmata then writes `zotero.bib` with every reference you cite that isn't in your other `.bib` files, and offers to add it to the document. Run **Sync zotero.bib from Zotero** from the command palette after changing references in Zotero.

## Figures, tables and maths

- **Drag image files** onto the editor, or **paste a screenshot**. Lemmata copies them into `figures/` and inserts a figure with a caption and label.
- **Paste cells** copied from LibreOffice, Excel or Google Sheets to get a formatted table. Changed your mind? Click **Paste as text instead**.
- Put the cursor inside **maths** to see it rendered above the line. Your own `\newcommand` and `\DeclareMathOperator` definitions work there too.

If one of these needs a package your document doesn't load yet (`graphicx`, `booktabs`), Lemmata offers to add it.

## Spelling and word count

Misspelled words get a wavy underline. Commands, labels, citation keys, file names, maths and comments are skipped. Right-click a word for suggestions or **Add to dictionary**; added words are saved in `lemmata.toml`.

Dictionaries: English (US and UK), German, Swiss German (with ss instead of ß) and French. Lemmata guesses the language from `babel` or `polyglossia`; you can set it in project settings.

The status bar shows the word count of the document body, and the number of words in your selection. **Word count** in the menu breaks it down by file and counts headings, maths, figures, tables and citations. The outline shows words per section and folds.

## History

**History** keeps snapshots of the project in git: one when you start working, then every 10 minutes while you edit, and named ones with **Ctrl+Shift+S**. Select a snapshot to see which files changed, and a file to compare it with the current version, or **compare it with another snapshot**. You can **rename** a snapshot, restore one file with **Restore this version**, or **Restore project** to put every file back. Restoring first snapshots the current state, so it can always be undone.

History uses its own git ref. If the project already is a git repository, your branches, commits and staged changes are never touched.

## Files

Right-click in the file tree to create, rename, duplicate or delete files and folders, copy a path, or show a file in your file manager. **Drag** a file onto a folder to move it, or drag files from your computer onto a folder to add them. Renaming or moving a chapter or figure offers to update the `\input`, `\include` and `\includegraphics` lines that refer to it. Deleted files go to your system's trash.

Images and PDFs in the project open in a viewer tab. Tabs can be dragged to reorder; middle-click or Ctrl+W closes one, and right-click offers *Close others*, *Close to the right* and *Close all*.

Files changed by other programs (another editor, `git pull`) reload automatically. If you have unsaved changes to the same file, Lemmata asks first.

## Exporting

- **Export PDF** saves a copy of the PDF.
- **Export for arXiv** builds the project, then zips exactly the files the build used plus the compiled bibliography (`.bbl`), with comments removed. That's ready to upload to arXiv.

## Shell escape

Some packages (`minted`, `svg`) need to run programs during the build. Because a document could run any command, this is off by default. Turn it on in project settings only for projects you trust.

## Updates

Lemmata checks for updates on start and every six hours, and asks before installing. Choose **Betas** in Preferences to get new features sooner.

## Reporting a problem

Run **Report a problem** from the command palette. It shows the details it will include (version, system, TeX distribution, the last error), then opens a GitHub issue for you to review and submit.

## The command line

`lemmata build [dir]` compiles a project once and prints its problems, `lemmata watch [dir]` rebuilds whenever a file changes, and `lemmata doctor` checks your TeX installation.

## Not in 1.0

Lemmata is a local editor, so there is no real-time collaboration or track changes, no rich-text (visual) editor, and no grammar checker. Open an issue if one of them matters to you.
