# Lemmata user guide

Lemmata is a LaTeX editor that builds your document as you write and shows the PDF next to the source. Projects are ordinary folders; nothing is uploaded anywhere.

## First launch

Lemmata needs a TeX distribution (TeX Live, or MiKTeX on Windows). If it can't find one, it shows the install command for your system. **Open terminal** shows that command in a terminal and waits for you to press Enter; Lemmata never installs anything by itself. Afterwards click **Check again**. If TeX is installed somewhere unusual, choose its `bin` folder.

**Windows with MiKTeX:** MiKTeX does not include Perl, and `latexmk` is a Perl script. Also run `winget install StrawberryPerl.StrawberryPerl` and restart Lemmata. The first build downloads the packages your document needs, which can take a minute. If MiKTeX refuses to run because it hasn't checked for updates (in any language, "major issue"), open MiKTeX Console, choose Updates and check for updates, or run `miktex packages update-package-database`; Lemmata shows this command when it happens. If Windows blocks Perl itself (an application control policy such as Smart App Control, error 4551), latexmk can't start; Lemmata then runs `pdflatex` and `biber` directly, with the same extra passes, and says so in the Problems panel. If a build fails before TeX starts, the Problems panel shows what `latexmk` printed.

## Projects

**Folders on the start screen.** The **Folders** section on the left of the start screen lets you group projects: *New folder* creates one, and a project's menu (its ⋯ button, or right-click it) puts it in a folder, takes it out, or makes a new folder. Click a folder to see only its projects. New and imported projects land in the folder you are looking at. Deleting a folder doesn't delete anything; its projects go back to *No folder* and stay in *All projects*. Folders are only a way to arrange the list: the project folders on disk don't move. The list now remembers up to 200 projects.

**Project menu.** Right-click a project on the start screen, or use its ⋯ button, to open it, show it in your file manager, put it in a folder, remove it from the list (the files stay on disk), or *Delete project…*, which moves the whole project folder to your system's trash after asking. Lemmata only deletes folders that hold a `.tex` file or `lemmata.toml` directly, never your home folder or a folder above it.

- **New project** starts from a template: article, thesis, slides, CV, letter or homework (exercise sheets with problem boxes and a name-and-course header).
- **Open folder** works with any folder containing a `.tex` file with `\documentclass`. If there are several, Lemmata asks which one is the main document.
- **Import Overleaf .zip** takes the zip from Overleaf's *Menu → Download → Source*.

Per-project settings (main file, engine, bibliography tool, spell-check language, shell escape) are stored in `lemmata.toml` in the project folder, so they travel with it.

**Several documents in one project.** Under *Project settings → Build targets* you can name more than one main file, for example a paper and its slides, each with its own compiler if you like. With two or more targets, a picker next to the project name in the toolbar switches what Recompile builds.

**Your own templates.** *Save this project as a template…* in the command palette copies the project (without hidden files or build output) so that *New project* offers it next to the built-in ones. Delete your own templates from the same dialog.

**Git.** If the project folder is in a git repository, changed files in the file tree get a letter (M modified, A added, D deleted, ? not tracked) and folders with changes get a dot. Lemmata only reads the repository's status to draw these marks; it never commits or stages anything in your repository. (History snapshots live on a private reference of their own and don't touch your branches either.)

## Writing and building

Lemmata saves shortly after you stop typing and builds after each save. In **Preferences** you can change the save delay or save only with Ctrl+S, and in the **Recompile** menu you can turn auto-compile off so the document only builds when you press Recompile. The build output goes to `.lemmata/build`, which is kept out of git.

The Recompile menu also has:

- **Fast (draft)**: figures are drawn as empty boxes, which makes big documents build quicker. Switch back to Normal for the final PDF.
- **Stop at the first error**: instead of pushing on to a complete PDF.
- **Clear cached files**: deletes auxiliary files, for the odd case where a stale `.aux` confuses LaTeX.

A build that runs longer than five minutes is stopped and explained (change `timeout_secs` in `lemmata.toml`, or set it to 0 for no limit).

Errors and warnings appear in the **Problems** panel in plain language, with a few lines of source and a hint where one is known. Click one to jump to its line (**F8** and **Shift+F8** step through problems in the editor). Some come with a one-click fix: **Add `\usepackage{amssymb}`** when a command like `\mathbb` is undefined, or the exact install command when a package is missing from your TeX installation. Problems from the bibliography tool (a missing `.bib` file, a broken entry) are listed too.

## The editor

- **Environments from `\beg`.** Start typing `\beg` and the list offers every environment as a whole block, the common ones first (`\begin{itemize}`, `\begin{align*}`, `\begin{table}`, …). Choosing `table` or `tabular` (also after typing `\begin{`) inserts a small sample table, header and cells included, and Tab steps through the placeholders. Typing `\en` offers `\end{…}` for the innermost environment that is still open.
- **Environments stay paired.** Type `\begin{itemize` and accept the suggestion: Lemmata adds the body and the matching `\end{itemize}`. Rename either half and the other follows. After a `\begin{name}` with no `\end`, **Enter** adds it.
- **Indentation** is 4 spaces per level by default. Choose 2, 4 or 8 spaces, or a Tab character, under *Preferences → Indentation*; Tab, Enter after `\begin`, and the figures and tables Lemmata inserts all follow it. Existing files are not reformatted.
- **Completion** suggests environments, commands, `\ref` labels from every file, `\cite` keys (with author and title), file names for `\input` and `\includegraphics`, and package names for `\usepackage`. texlab adds more when it's installed.
- **Maths**: inside `$…$`, `\[…\]` or an equation environment, typing `\` suggests symbols and structures first (`\alpha`, `\frac`, `\sum`), each with a rendered preview and a larger one for the highlighted entry; Tab moves between the fields of a template like `\frac{}{}`. In prose, text commands come first. The rendered formula above the cursor works for `$…$`, `$$…$$`, `\[…\]` and the maths environments, also while you're still typing, and keeps the last good version when an expression is incomplete.
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
- **Presenter view** (press **P** while presenting) opens a second window for you: the current slide, the next slide, your speaker notes, a timer (T pauses it, R resets it) and the clock, while the audience window shows only the slide. Either window can change slide and the other follows. With two screens it opens by itself: the slides go to the other screen and the presenter view stays on this one.
- **Speaker notes** come from Beamer. Add `\setbeameroption{show notes on second screen=right}` to the preamble and `\note{…}` after a frame. The PDF then has the slide on the left half of each page and the notes on the right; the audience window shows only the slide and the presenter view shows the notes. Without that option the presenter view says how to add them.

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

**Going places and renaming.** `Ctrl+Shift+O` jumps to a section by name. *Go to a label or citation…* in the command palette lists every `\label` and every entry in your `.bib` files. *Rename label or citation key…* changes a label or key everywhere at once: the definition, every `\ref`, `\cref` or `\cite` that uses it, and the `.bib` entry. It shows each place first and saves a snapshot, so History can undo it. Put the cursor on the name before opening it and the dialog is filled in for you.

**Snippets.** *Preferences → Snippets* lets you define short triggers that expand to longer text. Type the trigger (two or more letters) and pick it from the completion list; `${}` in the text marks places Tab jumps to.

## Spelling and word count

A file can choose its own language with a comment in its first lines, such as `% !TeX spellcheck = de_DE`, which beats the project setting for that file. English (US and UK), German (Germany and Switzerland) and French are available.

Misspelled words get a wavy underline. Commands, labels, citation keys, file names, maths and comments are skipped. Right-click a word for suggestions or **Add to dictionary**; added words are saved in `lemmata.toml`.

Dictionaries: English (US and UK), German, Swiss German (with ss instead of ß) and French. Lemmata guesses the language from `babel` or `polyglossia`; you can set it in project settings.

The status bar shows the word count of the document body, and the number of words in your selection. **Word count** in the menu breaks it down by file and counts headings, maths, figures, tables and citations. The outline shows words per section and folds.

## History

**History** keeps snapshots of the project in git: one when you start working, then every 10 minutes while you edit, and named ones with **Ctrl+Shift+S**. Select a snapshot to see which files changed, and a file to compare it with the current version, or **compare it with another snapshot**. You can **rename** a snapshot, restore one file with **Restore this version**, or **Restore project** to put every file back. Restoring first snapshots the current state, so it can always be undone.

History uses its own git ref. If the project already is a git repository, your branches, commits and staged changes are never touched.

## Files

Right-click in the file tree to create, rename, duplicate or delete files and folders, copy a path, set the main file, or show a file in your file manager. Drag files from your computer onto a folder to add them.

**Selecting.** Click a row, Ctrl-click (⌘-click on a Mac) to add or remove rows, Shift-click for a range. **Drag** the selection onto a folder to move it. Renaming or moving a chapter or figure offers to update the `\input`, `\include` and `\includegraphics` lines that refer to it.

**Keyboard.** Click the tree, then use the arrow keys to move, Left and Right to fold and unfold folders, Enter to open, `F2` to rename, `Delete` to delete, `Ctrl+X`, `Ctrl+C` and `Ctrl+V` to cut, copy and paste, `Ctrl+D` to duplicate, `Ctrl+A` to select all, `Ctrl+N` and `Ctrl+Shift+N` for a new file or folder, and `Ctrl+Z` to undo. Typing a few letters jumps to a matching name.

**Undo.** Moves, renames, deletes, duplicates, pastes and new files can be undone with the *Undo* button in the notice, `Ctrl+Z` in the tree, or *Undo the last file operation* in the command palette. Deleted files wait in `.lemmata/trash` inside the project for a week, then disappear. If you prefer your computer's trash, choose *System trash* under *Deleted files* in Preferences; those deletions can't be undone from Lemmata.

**Sorting and cleaning up.** The sort button above the tree orders files by name, type, last modified or size, with folders first or mixed in, and collapses or expands every folder. Hover a file to see its size and modification time. *Find unused files* lists images, `.tex` parts and bibliographies that no command in the project refers to, so you can delete them in one go. *Show the open file in the file tree* in the command palette scrolls to the file you are editing. Which folders are collapsed is remembered for each project.

**Tabs.** Right-click a tab to pin it: pinned tabs stay at the left, show a pin instead of a close button, and survive *Close others* and *Close all*. `Ctrl+Shift+T` reopens the last closed tab. Folded sections are remembered per file.

Images and PDFs in the project open in a viewer tab. Tabs can be dragged to reorder; middle-click or Ctrl+W closes one.

Files changed by other programs (another editor, `git pull`) reload automatically. If you have unsaved changes to the same file, Lemmata asks first; it never mistakes its own autosaves for outside changes. If a file or folder is deleted outside Lemmata, its tabs close and a short notice tells you.

## Exporting

- **Export PDF** saves a copy of the PDF.
- **Export the whole project (.zip)** packs every file of the project (not hidden files or the build folder) plus the latest PDF, for a backup or to send to someone.
- **Export for arXiv** builds the project, then zips exactly the files the build used plus the compiled bibliography (`.bbl`), with comments removed. That's ready to upload to arXiv.

## Shell escape

Some packages (`minted`, `svg`) need to run programs during the build. Because a document could run any command, this is off by default. Turn it on in project settings only for projects you trust.

## Updates

Lemmata checks for updates on start and every six hours, and asks before installing. Releases with a suffix such as `-rc.1` are published as pre-releases and are never offered by the updater; the **Betas** setting in Preferences only exists for older versions and currently follows the same releases as the stable channel.

## Reporting a problem

Run **Report a problem** from the command palette. It shows the details it will include (version, system, TeX distribution, the last error), then opens a GitHub issue for you to review and submit.

## The command line

`lemmata build [dir]` compiles a project once and prints its problems, `lemmata watch [dir]` rebuilds whenever a file changes, and `lemmata doctor` checks your TeX installation.

## Not in 1.x

Lemmata is a local editor, so there is no real-time collaboration or track changes, no rich-text (visual) editor, and no grammar checker. Open an issue if one of them matters to you.
