# Lemmata user guide

Lemmata is a LaTeX editor that builds your document as you write and shows the PDF next to the source. Projects are ordinary folders. Lemmata goes online only when you ask it to: for references you look up or search for, the template collection, spell-check dictionaries and Tectonic that you download, a grammar server you choose, git remotes, and the update check.

## First launch

Lemmata needs a TeX distribution (TeX Live, or MiKTeX on Windows). If it can't find one, it shows the install command for your system. **Open terminal** shows that command in a terminal and waits for you to press Enter; Lemmata never installs anything by itself. Afterwards click **Check again**. If TeX is installed somewhere unusual, choose its `bin` folder. **Continue without compiling** opens Lemmata without TeX: nothing compiles until TeX is found, and Lemmata checks again when you come back to its window. A notice offers **Set up TeX** to return to these steps.

**No TeX and no wish to install it?** On the same screen, **Use Tectonic (no install)** downloads Tectonic, a small LaTeX engine (the button shows the size: about 10 MB on Linux, 22 MB on macOS and Windows). You can also pick **Tectonic** as the *Compiler* in project settings, or put `% !TeX program = tectonic` in the first lines of the main file; the comment counts only when `lemmata.toml` doesn't set a compiler. The first build downloads the packages your document needs, so it needs an internet connection and can take a minute or two (the PDF pane counts the files as they arrive); later builds are fast. On a computer with Tectonic and no TeX distribution, new projects start with Tectonic as their compiler. Tectonic runs BibTeX itself. biblatex with biber often fails because Tectonic bundles an older biblatex, so use BibTeX. Fast (draft) mode has no effect with Tectonic. The PDF of the changes in History needs `latexdiff` and `latexmk`, so it needs TeX Live or MiKTeX.

**Windows with MiKTeX:** MiKTeX does not include Perl, and `latexmk` is a Perl script. Also run `winget install StrawberryPerl.StrawberryPerl` and restart Lemmata. The first build downloads the packages your document needs, which can take a minute. If MiKTeX refuses to run because it hasn't checked for updates (in any language, "major issue"), open MiKTeX Console, choose Updates and check for updates, or run `miktex packages update-package-database`; Lemmata shows this command when it happens. If Windows blocks Perl itself (an application control policy such as Smart App Control, error 4551), latexmk can't start; Lemmata then runs `pdflatex` and `biber` directly, with the same extra passes, and says so in the Problems panel. If a build fails before TeX starts, the Problems panel shows what `latexmk` printed.

## Projects

**Folders on the start screen.** The **Folders** section on the left of the start screen lets you group projects: *New folder* creates one, and a project's menu (its ⋯ button, or right-click it) puts it in a folder, takes it out, or makes a new folder. Click a folder to see only its projects. New and imported projects land in the folder you are looking at. Deleting a folder doesn't delete anything; its projects go back to *No folder* and stay in *All projects*. **Undo** brings a deleted folder back for about eight seconds. Folders are only a way to arrange the list: the project folders on disk don't move. The list remembers up to 200 projects.

**Project menu.** Right-click a project on the start screen, or use its ⋯ button, to open it, show it in your file manager, put it in a folder, remove it from the list (the files stay on disk), or *Delete project…*, which moves the whole project folder to your system's trash after asking. Lemmata only deletes folders that hold a `.tex` file or `lemmata.toml` directly, never your home folder or a folder above it.

- **New project** shows a gallery of templates with a preview of each. Pick a category on the left or type in the search box (it also matches descriptions and tags such as `beamer` or `physics`). Six come with Lemmata: Article, Thesis, Slides (Beamer with the metropolis theme), CV, Letter and Homework (exercise sheets with a name-and-course header). The rest come from the Lemmata template collection, a small set of carefully made templates for journals and conferences (IEEE, ACM, Springer, Elsevier, physics), theses and books, posters, slides, exams, lab reports and cover letters, and a blank document. Opening the gallery fetches them; this needs an internet connection the first time, after which they also work offline. If they can't be loaded, the gallery says so and shows the ones it has. Every file is checked before it is used.
- **Clone a repository…** copies a git repository from GitHub, GitLab, Overleaf or any other git URL to your computer and opens it. See [Git](#git).
- **Open folder…** works with any folder containing a `.tex` file with `\documentclass`. If there are several, Lemmata asks which one is the main document. If there are none, it offers **Create main.tex and open**.
- **Import from Overleaf…** takes the zip from Overleaf's *Menu → Download → Source*. If you select several projects in Overleaf and download them together, each one gets its own folder.
- **Example document** opens a short tour of what Lemmata can do.
- **Sort** the list by name or by last opened, with the sort menu next to the search box.

Per-project settings (main file, compiler, bibliography tool, spell-check language, shell escape, build timeout, build folder and build targets) are stored in `lemmata.toml` in the project folder, so they travel with it.

**Several documents in one project.** Under *Project settings → Build targets* you can name more than one main file, for example a paper and its slides, each with its own compiler if you like. With two or more targets, a picker next to the project name in the toolbar switches what Recompile builds.

**Your own templates.** *Save this project as a template…* in the command palette copies the project (without hidden files or build output) so that *New project* offers it next to the built-in ones. Your templates are listed under *My templates* in the New project gallery; the trash button on one deletes it.

**Opening files from your system.** Lemmata is registered for `.tex` files, so you can double-click one or choose **Open with Lemmata**. You can also pass a file or a folder on the command line, as in `lemmata-app notes.tex` or `lemmata-app ~/thesis`. A folder opens as a project. For a file, Lemmata uses the nearest folder above it with a `lemmata.toml` as the project; if there is none, the folder of the main file named by a `% !TeX root = ../main.tex` comment in the file's first lines; and otherwise the file's own folder. The file opens in that project. If Lemmata is already running, the project opens in the running window. `.bib`, `.sty` and `.cls` files work the same way.

**Links that open Lemmata.** Web pages and other programs can open Lemmata with a `lemmata://` link: `lemmata://new?template=ieee` opens the New project gallery with that template chosen, `lemmata://clone?url=https://github.com/you/paper.git` opens Clone a repository with the address filled in, and `lemmata://open?path=/home/you/thesis` opens a project. Clone links must use `https`. These links go to the start screen and close the project you have open. For `new` and `clone`, nothing is created or cloned until you confirm in the dialog.

**Git marks.** If the project folder is in a git repository, changed files in the file tree get a letter (M modified, A added, D deleted, R renamed, U conflicted, ? not tracked) and folders with changes get a dot. The marks only read the repository's status. To commit, pull and push, use the Git panel (see [Git](#git)). History snapshots live on a private reference of their own and don't touch your branches.

## Writing and building

Lemmata saves shortly after you stop typing and builds after each save. In **Preferences** you can choose *With Ctrl+S* instead of saving as you type, and turn off *Compile after every save* (the **Recompile** menu has the same choice, *Only when I recompile*), so the document only builds when you press Recompile. The Recompile menu also switches the compiler. The build output goes to `.lemmata/build`. History snapshots leave it out; to keep it out of your own git commits, list `.lemmata/` in your `.gitignore`. Under *Project settings → Build folder* you can choose another folder inside the project.

The Recompile menu also has:

- **Fast (draft)**: figures are drawn as empty boxes, which makes big documents build quicker. Switch back to Normal for the final PDF.
- **Stop at the first error**: instead of pushing on to a complete PDF.
- **Clear cached files**: deletes auxiliary files, for the odd case where a stale `.aux` confuses LaTeX. It only empties Lemmata's own build folder; a build folder that Lemmata didn't create is left alone.

With pdflatex, Lemmata precompiles the preamble (everything before `\begin{document}`) in the background, so rebuilds after an edit start faster. A precompiled preamble is used only after one build with it gave exactly the same PDF as one without it, and it is made again whenever the preamble or a file it reads changes. Recompile always builds without it. To turn it off, set `fast_preamble = false` in `lemmata.toml`.

A build that runs longer than five minutes is stopped and explained. Change the limit under *Project settings → Stop a build after N seconds* (10 to 3600), or set `timeout_secs` in `lemmata.toml`; 0 means no limit.

If a build fails but an older PDF is still there, the PDF pane keeps that PDF and shows a banner with **Show logs**. Clicking the red build result in the status bar also opens the logs. While a build runs, the Recompile button reads **Stop**.

Errors and warnings appear in the **Problems** panel in plain language, with a few lines of source and a hint where one is known. Click one to jump to its line (**F8** and **Shift+F8** step through problems in the editor). Some come with a one-click fix: **Add `\usepackage{amssymb}`** when a command like `\mathbb` is undefined, or the exact install command when a package is missing from your TeX installation. Problems from the bibliography tool (a missing `.bib` file, a broken entry) are listed too. Many common TeX errors also come with a one-sentence explanation of what went wrong and, where there is one, a fix.

**Show TODOs** in the command palette lists every `% TODO`, `% FIXME` and `% XXX` comment and every `\todo{…}` note in the project, grouped by file. Click one to go to its line.

If a save fails, the tab stays open with your text, the status bar says **Not saved** in red, and the error stays on screen until you dismiss it. With autosave on, closing the window saves unsaved files first. If a file can't be saved, or autosave is off, Lemmata asks before it closes. Switching projects saves first too, and stays where it is if a save fails.

## The editor

- **Environments from `\beg`.** Start typing `\beg` and the list offers every environment as a whole block, the common ones first (`\begin{itemize}`, `\begin{align*}`, `\begin{table}`, …). Choosing `table` or `tabular` (also after typing `\begin{`) inserts a small sample table, header and cells included, and Tab steps through the placeholders. Typing `\en` offers `\end{…}` for the innermost environment that is still open.
- **Environments stay paired.** Type `\begin{itemize` and accept the suggestion: Lemmata adds the body and the matching `\end{itemize}`. Rename either half and the other follows. After a `\begin{name}` with no `\end`, **Enter** adds it.
- **Indentation** is 4 spaces per level by default. Choose 2, 4 or 8 spaces, or a Tab character, under *Preferences → Indentation*; Tab, Enter after `\begin`, and the figures and tables Lemmata inserts all follow it. Existing files are not reformatted.
- **Completion** suggests environments, commands, `\ref` labels from every file, `\cite` keys (with author and title), file names for `\input` and `\includegraphics`, and package names for `\usepackage`. texlab, which ships with Lemmata, adds more.
- **Maths**: inside `$…$`, `\[…\]` or an equation environment, typing `\` suggests symbols and structures first (`\alpha`, `\frac`, `\sum`), each with a rendered preview and a larger one for the highlighted entry; Tab moves between the fields of a template like `\frac{}{}`. In prose, text commands come first. The rendered formula above the cursor works for `$…$`, `$$…$$`, `\[…\]` and the maths environments, also while you're still typing, and keeps the last good version when an expression is incomplete.
- **Folding**: click the arrow in the gutter to fold a section, environment or block of comments.
- **Formatting**: Ctrl+B and Ctrl+I toggle bold and italic, Ctrl+/ comments lines, Ctrl+] and Ctrl+[ change a heading's level, Ctrl+Shift+E wraps the selection in an environment. The toolbar above the editor has buttons for these, headings, links, figures, tables, lists and citations.
- **Insert dialogs**: **Figure** (pick an image from the project or your computer, caption, label, width), **Table** (choose the size on a grid) and **Symbols** (about 300 maths symbols, Ctrl+Shift+M).
- Completion, paired environments, automatic closing of brackets and dollar signs, and folding can be switched off in **Preferences → Editor**, together with the font, line height and line wrapping. *Keyboard* in **Preferences → Editing** offers Vim keys.
- **Format document** in the command palette re-indents the open file with `tex-fmt` or `latexindent` (part of TeX Live), whichever is installed, using your indentation setting. The change is a single edit, so Ctrl+Z brings the old layout back.
- **Hover cards.** Hover over `\ref`, `\eqref` or `\cref` to see what it points to: the rendered equation, the figure with its caption and image, the table caption or the section heading. Over `\cite`, you see the entry. Over `\includegraphics` or `\input`, you see the file.
- **Review comments.** Select the text, then click **Comment** in the editor toolbar or press Ctrl+Alt+M (**Add a comment** in the palette). The text is highlighted and the Comments panel opens for you to write. The **Comments** button in the top toolbar opens the panel at any time; its badge shows how many comments are open. In the panel you can reply, **Resolve** a thread and **Reopen** it later, or **Delete** it (a second click confirms). **Show resolved** lists the resolved threads too. Comments follow the text as you edit it. They are saved in `.lemmata/comments.json` in the project. Lemmata doesn't change your `.gitignore`, so `.lemmata/` is ignored only if yours lists it. To share the comments through git, commit just that file with `git add -f .lemmata/comments.json`; the force flag adds it even when the folder is ignored.

## The PDF

- **Double-click** (or Ctrl+click) in the PDF to jump to the source line.
- **Ctrl+J** in the editor, or the **Go to PDF** button in the editor toolbar, shows the current line in the PDF.
- Text in the PDF can be **selected and copied**. **Ctrl+F** with the PDF focused searches it and highlights every match.
- Links in the PDF work: `\ref`, the table of contents and URLs.
- The **outline** button lists the PDF's bookmarks (load `hyperref` to get them).
- The **page thumbnails** button shows a strip of every page beside the document; click one to go to it.
- The zoom menu (the percentage button) has **Two pages**, which lays pages out side by side with page 1 on its own, like a book, and **Start with two pages**, which pairs pages 1 and 2, 3 and 4, and so on.
- **Open in external viewer** (in the PDF toolbar) opens the PDF in your system's PDF viewer.
- The moon button (or *Dark PDF pages when the app is dark* in Preferences) shows the pages dark when the app is dark.
- **Present full screen** shows one page at a time for Beamer slides: arrow keys, Space or a click move on, Esc leaves.
- **Presenter view** (press **P** while presenting) opens a second window for you: the current slide, the next slide, your speaker notes, a timer (T pauses it, R resets it) and the clock, while the audience window shows only the slide. Either window can change slide and the other follows. With two screens it opens by itself: the slides go to the other screen and the presenter view stays on this one.
- **Speaker notes** come from Beamer. Add `\setbeameroption{show notes on second screen=right}` to the preamble and `\note{…}` after a frame. The PDF then has the slide on the left half of each page and the notes on the right; the audience window shows only the slide and the presenter view shows the notes. Without that option the presenter view says how to add them.

## Shortcuts

Press **Ctrl+Shift+/** or **F1** for the full list inside Lemmata. It has a search box and also lists mouse actions, such as double-clicking in the PDF. The most used:

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
| Ctrl+Alt+M | Add a comment on the selection |
| F8 / Shift+F8 | Next / previous problem |
| F12 / Shift+F12 | Go to definition / find references (labels, citations, files) |
| F2 | Rename a label everywhere |
| F11 | Focus mode |

On macOS, use Cmd instead of Ctrl.

## Citations and references

**Ctrl+Shift+C** searches your `.bib` files by author, title, year or key. Inside an open `\cite{` it adds the key; elsewhere it inserts a new `\cite{…}`.

To add a reference, paste a DOI (`10.1038/nature14539`) or an arXiv ID (`arXiv:1707.06347`) into the picker. Lemmata looks it up on doi.org, gives it a readable key such as `schulman2017proximal` and appends it to the `.bib` file you choose.

Don't have the DOI? Type a title or author in the picker and click **Search online**. That sends your search to Crossref, and only when you click it; nothing is sent while you type. Pick a result to add it to your `.bib` file and cite it.

Those two are the only times Lemmata goes online for references. Zotero, below, talks only to Zotero on your computer.

**Bibliography manager…** in the command palette lists every entry in your `.bib` files, with the file each one is in. It marks **Duplicate key**, **Same DOI** and **Same title** entries and **Not cited** ones, and lists keys that are cited but in no `.bib` file at the top. The menu next to the search box shows all entries, only the duplicates or only the uncited ones, and another menu sorts by key, author or year. On each row, **Open** goes to the entry, **Fields** shows its fields and **Delete…** removes it after you confirm. Entries with a duplicate key also have **Rename key…**, which changes the key in the `.bib` file only; to change it in your `.tex` files too, use *Rename label or citation key…*.

### Zotero

With Zotero and the [Better BibTeX](https://retorque.re/zotero-better-bibtex/) plugin running, **Cite from Zotero** opens Zotero's own citation picker. Lemmata then writes `zotero.bib` with every reference you cite that isn't in your other `.bib` files, and offers to add it to the document. Run **Sync zotero.bib from Zotero** from the command palette after changing references in Zotero.

## Figures, tables and maths

- **Drag image files** onto the editor, or **paste a screenshot**. Lemmata copies them into `figures/` and inserts a figure with a caption and label.
- **Paste cells** copied from LibreOffice, Excel or Google Sheets to get a formatted table. Changed your mind? Click **Paste as text instead**.
- Put the cursor inside **maths** to see it rendered above the line. Your own `\newcommand` and `\DeclareMathOperator` definitions work there too.

If one of these needs a package your document doesn't load yet (`graphicx`, `booktabs`), Lemmata offers to add it.

**Going places and renaming.** `Ctrl+Shift+O` jumps to a section by name. *Go to a label or citation…* in the command palette lists every `\label` and every entry in your `.bib` files. *Rename label or citation key…* changes a label or key everywhere at once: the definition, every `\ref`, `\cref` or `\cite` that uses it, and the `.bib` entry. It shows each place first and saves a snapshot, so History can undo it. Put the cursor on the name before opening it and the dialog is filled in for you.

**Snippets.** *Preferences → Snippets* lets you define short triggers that expand to longer text. You can also open it with *Edit snippets…* in the command palette. Type the trigger (two or more letters) and pick it from the completion list; `${}` in the text marks places Tab jumps to.

## Spelling and word count

A file can choose its own language with a comment in its first lines, such as `% !TeX spellcheck = de_DE` (or `% !TeX lang = fr`), which beats the project setting for that file.

Misspelled words get a wavy underline. Commands, labels, citation keys, file names, maths and comments are skipped. Right-click a word for suggestions or **Add to dictionary**; added words are saved in `lemmata.toml`.

Dictionaries: English (US and UK), German, Swiss German (with ss instead of ß) and French come with Lemmata. By default (*Match the document's language* in Preferences) Lemmata guesses the language from `babel` or `polyglossia`. You can choose one in Preferences for every project, set it in project settings for one project, or turn spell check off.

**More languages.** Spanish, Italian, Portuguese (Portugal and Brazil), Dutch, Polish, Swedish, Danish, Norwegian Bokmål, Czech, Russian, Ukrainian, Turkish and Greek are downloaded when you need them. In **Preferences → Spell check**, the languages that still have to be fetched show their download size, for example *(downloads 5 MB)*. Choose one and Lemmata downloads it once; after that it works offline. The language codes for `% !TeX spellcheck` are `es`, `it`, `pt-PT`, `pt-BR`, `nl`, `pl`, `sv`, `da`, `nb`, `cs`, `ru`, `uk`, `tr` and `el`. If a project or a `% !TeX spellcheck` comment asks for a language that isn't downloaded yet, a notice offers **Download** (once per session); until then that language isn't checked.

**Grammar checking** is off by default. Choose a mode under **Preferences → Grammar check**: **LanguageTool on this computer** (a server at `http://localhost:8081`), or **LanguageTool at an address…** for a server you run or trust. Lemmata sends the prose of the document to that server, a paragraph at a time, and nowhere else. Preferences shows where the text goes, and **Test the connection** checks that a server answers. Lemmata doesn't start one for you; to run one for a session, start it in Docker with `docker run -p 8081:8010 erikvl87/languagetool`, or with the standalone jar with `java -cp languagetool-server.jar org.languagetool.server.HTTPServer --port 8081`, and stop it with Ctrl+C when you're done. Findings get a dotted underline. Hover over one to see its suggestions as buttons, or **Ignore this rule**. Grammar checking covers English (US and UK), German (Germany and Switzerland) and French, following the spell-check language; other languages are not grammar-checked.

The status bar shows the word count of the document body, and the number of words in your selection. **Word count** in the menu breaks it down by file and counts headings, maths, figures, tables and citations. The outline shows words per section and folds.

## History

**History** keeps snapshots of the project in git: one when you start working, then every 10 minutes while you edit, and named ones with **Ctrl+Shift+S**. Select a snapshot to see which files changed, and a file to compare it with the current version, or **compare it with another snapshot**. You can **rename** a snapshot, restore one file with **Restore this version**, or **Restore project** to put every file back. Restoring first snapshots the current state, so it can always be undone.

**Review changes.** The bar at the top of History turns it into a way to review edits, for example after a co-author changed your text. Choose a starting point in *Review changes since* (by default your last review) and click **Review changes**. Every changed text file is listed as separate changes with the old lines in red and the new lines in green. **Accept** keeps the current text for that change; **Reject** puts the old text back for that change only, and Ctrl+Z in the file undoes it. Each file and the whole list also have **Accept all** and **Reject all**. Accepted changes are hidden. **Mark as reviewed** takes a snapshot called *Reviewed* where your next review starts.

**PDF of the changes.** **PDF of the changes** compiles the old and the current version with `latexdiff` into one PDF with insertions and deletions marked, then offers **Open PDF of the changes**. It opens in your PDF viewer. It needs `latexdiff` (part of TeX Live; with MiKTeX it also needs Perl), `latexmk` and the LaTeX package `ulem`. If one is missing, Lemmata says how to install it.

History is on for every project; it only needs git to be installed. A project that is a git repository keeps its snapshots on a private ref of its own, so your branches, commits and staged changes are never touched. Any other project gets a small snapshot store in its hidden `.lemmata` folder; the project folder doesn't become a git repository and nothing else in it changes. Without git, History is off, and replacing or renaming across the project says that it can't be undone.

## Git

Lemmata works with the git on your computer; install git first if you don't have it. Open the panel with **Git: commit, pull, push…** in the command palette or **Git…** in the app menu.

- **Turn this project into a git repository** appears when the project isn't in one. It runs `git init`, adds a `.gitignore` containing `.lemmata/` if there is none, and makes a first commit of everything.
- **Changes** are listed as *Staged*, *Changes* and *Untracked*. Tick a file to stage it and untick it to unstage it. **Stage all** stages every change in one go. Files with merge conflicts are listed under *Conflicts*; resolve them in a terminal.
- **Commit** needs a message and at least one staged file. **Ctrl+Enter** in the message box commits too. Lemmata saves your open files first.
- **Fetch**, **Pull** and **Push** talk to the remote. The numbers next to Pull and Push are the commits you are behind and ahead. Pull only fast-forwards: if both sides changed, Lemmata tells you to merge or rebase in a terminal. The first push of a branch sets up its link to the remote. A repository without a remote needs one added in a terminal first (`git remote add origin <url>`).
- **Branch** menu at the top switches branches; the **+** button next to it creates a branch from where you are. Commit your changes before switching.
- **History** at the bottom lists the last 50 commits.

Lemmata uses your normal git settings, SSH keys and credential helper. If a push or pull asks for a password it can't give, run `git push` or `git pull` once in a terminal in the project folder to sign in.

If git doesn't know your name and email yet, the panel shows the two commands to run once in a terminal:

```
git config --global user.name "Your Name"
git config --global user.email "you@example.com"
```

**Clone a repository.** **Clone a repository…** on the start screen asks for the repository URL (https or ssh, from GitHub, GitLab, Overleaf or your own server), where to put it and the folder name, which follows the URL until you type one. Lemmata then opens the new folder. For Overleaf, create a token under *Account settings → Git integration* and use `https://git:TOKEN@git.overleaf.com/ID` as the URL, or sign in once with `git clone` in a terminal. A token in the URL stays in the project's git settings.

History snapshots and the Git panel don't interfere: snapshots live on a private reference and never touch your branches, commits or staged files.

## Files

Right-click in the file tree to create, rename, duplicate or delete files and folders, copy a path, set the main file, or show a file in your file manager. Drag files from your computer onto a folder to add them.

**Selecting.** Click a row, Ctrl-click (⌘-click on a Mac) to add or remove rows, Shift-click for a range. **Drag** the selection onto a folder to move it. Renaming or moving a chapter or figure offers to update the `\input`, `\include` and `\includegraphics` lines that refer to it. Renaming or moving a file or folder also updates the main document and build targets that point to it, and changing only the capitalisation of a name works on Windows and macOS.

**Keyboard.** Click the tree, then use the arrow keys to move, Left and Right to fold and unfold folders, Enter to open, `F2` to rename, `Delete` to delete, `Ctrl+X`, `Ctrl+C` and `Ctrl+V` to cut, copy and paste, `Ctrl+D` to duplicate, `Ctrl+A` to select all, `Ctrl+N` and `Ctrl+Shift+N` for a new file or folder, and `Ctrl+Z` to undo. Typing a few letters jumps to a matching name. **Shift+F10** or the **Menu** key opens the menu for the selected file or the focused tab.

**Undo.** Moves, renames, deletes, duplicates, pastes and new files can be undone with the *Undo* button in the notice, `Ctrl+Z` in the tree, or *Undo the last file operation* in the command palette. Deleted files wait in `.lemmata/trash` inside the project for a week, then disappear. If you prefer your computer's trash, choose *System trash* under *Deleted files* in Preferences; those deletions can't be undone from Lemmata.

**Sorting and cleaning up.** The sort button above the tree orders files by name, type, last modified or size, with folders first or mixed in, and collapses or expands every folder. Hover a file to see its size and modification time. *Find unused files* lists images, `.tex` parts and bibliographies that no command in the project refers to, so you can delete them in one go. *Show the open file in the file tree* in the command palette scrolls to the file you are editing. Which folders are collapsed is remembered for each project.

**Tabs.** Right-click a tab to pin it: pinned tabs stay at the left, show a pin instead of a close button, and survive *Close others* and *Close all*. `Ctrl+Shift+T` reopens the last closed tab. Folded sections are remembered per file.

Images and PDFs in the project open in a viewer tab. Text formats such as `csv`, `json`, `yaml`, `py`, `Makefile` and `.latexmkrc` open in the editor, and project search covers them too. Tabs can be dragged to reorder; middle-click or Ctrl+W closes one.

Files changed by other programs (another editor, `git pull`) reload automatically, unless you have unsaved changes to the same file. Then autosave pauses for that file and Lemmata asks: **Keep my version** or **Load the version on disk**. Nothing is saved until you choose. Lemmata never mistakes its own autosaves for outside changes. If a file or folder is deleted outside Lemmata, its tabs close and a short notice tells you.

## Exporting

- **Export PDF** saves a copy of the PDF.
- **Export the whole project (.zip)** packs every file of the project (not hidden files or the build folder) plus the latest PDF, for a backup or to send to someone.
- **Export for arXiv** builds the project, then zips exactly the files the build used plus the compiled bibliography (`.bbl`), with comments removed (but not a `%` inside verbatim environments, `\verb` or URLs). If the document doesn't build without errors, Lemmata warns first and offers **Export anyway**. That's ready to upload to arXiv.
- **Export for arXiv as one .tex file** (in the command palette and the app menu) does the same, but puts the whole document into the main file: `\input`, `\include`, `\subfile` and `\import` are replaced by the text of the file they load, and with BibTeX the `.bbl` text replaces `\bibliography`. The files that were merged are left out of the zip. With biblatex, `\printbibliography` stays and the `.bbl` is in the zip. An input that can't be found, or that lies outside the project, is left as it is.

## Shell escape

Some packages (`minted`, `svg`) need to run programs during the build. Because a document could run any command, this is off by default. Turn it on in *Project settings* only for projects you trust; doing so counts as allowing it.

A project's `lemmata.toml` can't turn it on by itself. If it does, Lemmata asks when you open the project: **Allow for this project** lets the document run programs, and **Build without it** builds without shell escape. Until you allow it, builds run without it. If you choose **Build without it**, Lemmata asks again the next time you open the project.

## Updates

Lemmata checks for updates on start and every six hours, and asks before installing. You can turn the check off with *Check for updates automatically* in Preferences, or run **Check now** there. After an update, a short **What's new** window lists what the new version lets you do, once; it doesn't appear on a fresh install. Releases with a suffix such as `-rc.1` are published as pre-releases and are never offered by the updater.

## Reporting a problem

Run **Report a problem** from the command palette. It shows the details it will include (version, system, TeX distribution, the last errors, with file paths removed), then opens a GitHub issue for you to review and submit.

## The command line

`lemmata build [dir]` compiles a project once and prints its problems, `lemmata watch [dir]` rebuilds whenever a file changes, and `lemmata doctor` checks your TeX installation. The other commands check and export a project without the editor, for scripts and CI. The commands that compile (`build`, `watch`, `lint` and `export --arxiv`) use `latexmk` from TeX Live or MiKTeX, or Tectonic when the project's compiler is Tectonic or no TeX is installed (the one Lemmata downloaded, or one on your PATH). The `lemmata` command is not part of the app's installer: each release also has it on its own, as `lemmata-cli_<version>_<platform>` (Linux, macOS, Windows), for scripts and CI. Unpack it and put `lemmata` on your PATH. From the source repository, `cargo install --path crates/lemmata-cli` builds it too.

| Command | What it does |
|---|---|
| `lemmata lint [dir]` | Compiles, then reports errors, citations missing from every `.bib` file and undefined labels. Exits 1 on errors; `--strict` makes warnings fail too. |
| `lemmata wordcount [dir]` | Counts words in the document body, in total and per file. |
| `lemmata bib [dir]` | Lists the bibliography entries. `--unused` lists entries no `.tex` file cites. `--missing` lists keys that are cited but in no `.bib` file, and exits 1 if there are any. |
| `lemmata outline [dir]` | Shows the section tree with line numbers and word counts. |
| `lemmata export --zip out.zip [dir]` | Zips the project's source files, without hidden files, build output or the PDF. |
| `lemmata export --arxiv out.zip [dir]` | Builds, then zips the files the build used plus the compiled bibliography, with comments removed, like Export for arXiv in the app. `--flatten` merges `\input` and `\include` files into the main file, `--keep-comments` keeps the comments, `--anyway` exports even if the build has errors. |

`dir` is the project folder and defaults to the current folder. `build`, `watch`, `lint`, `wordcount`, `bib`, `outline` and `export` take `--json` for machine-readable output. The exit status is 0 on success, 1 when a check finds problems or a build fails (`doctor` exits 1 when TeX is not ready), and 2 for a usage error such as an unknown option. For example, `lemmata lint --strict && lemmata bib --missing` fails a CI job on any warning or missing citation.

## Not in Lemmata

Lemmata is a local editor, so there is no real-time collaboration and no rich-text (visual) editor. To work with others, use review comments, Git and the History review. Open an issue if one of the missing features matters to you.
