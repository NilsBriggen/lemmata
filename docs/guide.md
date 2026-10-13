# Lemmata user guide

Lemmata is a LaTeX editor that builds your document as you write and shows the PDF next to the source. Projects are ordinary folders; nothing is uploaded anywhere.

## First launch

Lemmata needs a TeX distribution (TeX Live, or MiKTeX on Windows). If it can't find one, it shows the install command for your system. **Open terminal** shows that command in a terminal and waits for you to press Enter; Lemmata never installs anything by itself. Afterwards click **Check again**. If TeX is installed somewhere unusual, choose its `bin` folder.

## Projects

- **New project** starts from a template: article, thesis, slides, CV or letter.
- **Open folder** works with any folder containing a `.tex` file with `\documentclass`. If there are several, Lemmata asks which one is the main document.
- **Import Overleaf .zip** takes the zip from Overleaf's *Menu → Download → Source*.

Per-project settings (main file, engine, bibliography tool, spell-check language, shell escape) are stored in `lemmata.toml` in the project folder, so they travel with it.

## Writing and building

Lemmata saves and builds shortly after you stop typing. You can change the delay, or switch to building only on Ctrl+S, in **Preferences**. The build output goes to `.lemmata/build`, which is kept out of git.

Errors and warnings appear in the **Problems** panel in plain language, with a hint where one is known. Click one to jump to its line. If a package is missing, the panel shows the exact command that installs it on your system.

## Moving between source and PDF

- **Double-click** (or Ctrl+click) in the PDF to jump to the source line.
- **Ctrl+J** in the editor shows the current line in the PDF.

## Shortcuts

| Keys | Action |
|---|---|
| Ctrl+Enter | Recompile |
| Ctrl+S | Save now |
| Ctrl+P | Open a file by name |
| Ctrl+Shift+P | Command palette: every action |
| Ctrl+Shift+F | Search and replace in the project |
| Ctrl+Shift+C | Insert a citation |
| Ctrl+Shift+S | Save a named snapshot |
| Ctrl+J | Show this line in the PDF |
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

The header shows the word count of the document body. The outline shows it per section.

## History

**History** keeps snapshots of the project in git: one when you start working, then every 10 minutes while you edit, and named ones with **Ctrl+Shift+S**. Select a snapshot to see which files changed, and a file to compare it with the current version. **Restore this version** first snapshots the current state, so a restore can always be undone.

History uses its own git ref. If the project already is a git repository, your branches, commits and staged changes are never touched.

## Files

Right-click in the file tree to create, rename or delete files and folders. Renaming a chapter or figure offers to update the `\input`, `\include` and `\includegraphics` lines that refer to it. Deleted files go to your system's trash.

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
