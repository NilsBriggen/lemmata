# UI redesign: implementation handoff

This document is for whoever builds the redesign in Rust (Antigravity or a person). It says what to build, in which order, where the code lives today and how to check it. Read it top to bottom once before starting.

- **The design:** the mockups in `design/mockups/`. Open `design/mockups/index.html` in a browser. Every page has a switch at the bottom right for dark mode and for today's UI. The mockups are the spec: when this document and a mockup disagree, ask; don't guess.
- **Why and what changes:** `design/redesign.md` (direction, screen list, token table, Mobbin references).
- **Status:** design approved for mockups only. Nothing is implemented. The user decides after reviewing whether to proceed, screen by screen.

All code paths below are relative to `apps/lemmata/src/` unless they start with `crates/`, `design/` or `apps/`. Line numbers are from the 3.3.0 tree (commit `3e4b8fb`), so they will drift. Search for the function name if a line is off.

---

## 0. Before you start

### 0.1 Branch and merge order

- The design lives on branch **`ui-redesign`**, which branches from `3e4b8fb` (the 3.3.0 release). It contains only `design/`; no Rust changes.
- Your in-progress work on `main` touches many of the same files (`workspace/chrome.rs`, `workspace/mod.rs`, `screens/library.rs`, `editor/*`, `theme.rs` and more). **Finish and commit that work first.** Then rebase `ui-redesign` onto `main`, or merge `main` into it. Only then start the Rust changes, on `ui-redesign` or a branch from it.
- One screen per pull request, in the order of section 2. Each one is reviewed against its mockup before the next starts.
- Don't push, tag or release without the user's OK. No release goes out with a known gap: the redesign ships when its screens are done, not half-way.

### 0.2 How to check your work (every PR)

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --exclude lemmata-app
cargo test -p lemmata-app
```

- **Snapshots:** the app's UI tests are kittest snapshot tests inside the source files (`#[cfg(test)]` modules). There are no `.rs` files in `apps/lemmata/tests/`. That folder holds only the reference PNGs in `apps/lemmata/tests/snapshots/`.
  - Regenerate them with `UPDATE_SNAPSHOTS=1 cargo test -p lemmata-app`.
  - Look at every changed PNG before committing it. A snapshot that changed in a way the mockup doesn't show is a bug.
- **Test harness:**
  - `testing::harness(p, ui_fn)` (`testing.rs:11`) gives a 1400×900 kittest harness with the fonts and theme.
  - `app.rs` has `snap(h, name)` (≈`:1439`).
  - `screens/welcome.rs`, `screens/library.rs` and `screens/dialogs.rs` each have a private `render(...)` helper.
  - Whole-app scenarios are in `scenarios.rs` and in the `app.rs` test module.
- **Label-based tests:** about 370 tests find widgets by their accessibility label: 279 in `app.rs`, 66 in `scenarios.rs`, 30 in `editor/view.rs`, plus a few elsewhere. **Renaming a button breaks them.** When a label must change, change the tests in the same commit. Section 2 lists the labels each screen touches.
- **Run the app and look at it next to the mockup,** in light and dark, at 1400 px and about 1000 px wide. Tests alone don't show whether it looks like the design.

### 0.3 Rules of the design

These hold for every screen.

1. **Quiet chrome.** Navigation (rail, top bar, sidebars) is drawn straight on the app ground, `Palette.app`. Only the work (editor, PDF, the library's main area, dialogs) sits on white cards.
2. **One accent.** `accent` marks the selection and the single main action of a screen (Recompile, Create project, Commit, Save). `error` and `warn` are only for problems.
3. **Type.** Young Serif (`theme::font::display()`) for page titles only. IBM Plex Sans for the interface, at 13 px with 500/600 weights through `font::sans_500()` and `font::sans_600()`. IBM Plex Mono for code, paths, line numbers and keyboard keys.
4. **What egui can draw.** Solid fills, 1 px strokes, rounded corners and `epaint::Shadow`. No blur, no gradients. The mockup uses nothing else; if something looks like it needs more, ask.
5. **Spacing.** The 8 px grid:
   - 8 px between cards and around the main area;
   - 4 px between rail buttons;
   - 6–8 px inside dense rows.
6. **Corner radii:**
   - 10 px for cards (`R_LG`);
   - 14 px for dialogs (new: `R_XL`);
   - 8 px for buttons and inputs;
   - 6–7 px for small controls and rows;
   - 4 px for tags.
7. **Accessibility.** Every icon-only button keeps an accessible label (`Button::icon(icon, "Label")`). Text contrast is at least 4.5:1 in both themes. Keyboard focus keeps the existing `widgets::focus_ring`.

---

## 1. Foundation (do this first, as its own PR)

Every screen depends on this.

### 1.1 Tokens in `theme.rs`

`Palette` (`theme.rs:12-54`), `LIGHT` (≈`:78`) and `DARK` (≈`:122`) are the only place colours live. The mockups' CSS variables (`design/mockups/src/base.css`, top of file) map 1:1 onto `Palette` fields: `--surface-2` is `surface_2`, and so on.

| Field | Light today → new | Dark today → new | Notes |
|---|---|---|---|
| `app` | `#e8ebee` → `#eceef1` | unchanged `#0f1418` | The ground now carries the rail, top bar and sidebar. |
| `surface_3` | `#eef0f2` → `#e4e7eb` | `#222a32` → `#232b33` | Hover fill on the ground, and the track of segmented controls. |
| `muted` | `#5c6873` → `#5a6671` | unchanged | Keeps 4.5:1 on the new ground. |
| `faint` | `#616b76` → `#5c6670` | `#86919b` → `#8a95a0` | Keeps 4.5:1 on `surface_3` (4.65:1 light) and full-opacity gutter numbers. |
| `error` | `#c4472f` → `#b83c24` | unchanged `#ec7f69` | Was 4.26:1 on `error_soft` and 4.21:1 on `app`; now 5.04:1 / 4.98:1. Error badges use `accent_ink` (6.79:1 dark). |
| `ok` | `#2e7d55` → `#2a7550` | unchanged | Was 4.3:1 on the new ground, now 4.8:1. |
| `warn` | `#9a6b0a` → `#8a5f08` | unchanged | Was 4.2:1 on `warn_soft`, now 5.1:1. |
| `code_comment` | `#6b7681` → `#5f6b76` | `#8a95a0` | Was 4.28:1 on `line_hl`; now 4.95:1. |
| `pdf_ground` | `#c8cdd2` → `#e1e5e9` | unchanged `#0b0f13` | The PDF now sits inside a card. |
| `rail_active` **new** | `#ffffff` | `#1e262e` | Fill of the selected rail button, sidebar row and segment. |
| `line_hl` **new** | `#f4f6f8` | `#1c232a` | Caret line in the editor. |
| `accent_text` **new** | = `accent` | `#7cbbe6` | Small accent text on `accent_soft` (tags, links in panels). In dark, `accent` on `accent_soft` is only 4.4:1. |
| `shadow_card` **new** | 1 px hairline `rgba(16,24,32,.06)` + `0 1px 2px rgba(16,24,32,.06)` | hairline `rgba(255,255,255,.04)` + `0 1px 2px rgba(0,0,0,.4)` | Cards, the selected row or segment, inputs. In egui: a 1 px stroke in the hairline colour, plus `Shadow { offset: [0,1], blur: 2, .. }`. |

The `chrome*` fields (eight of them, `theme.rs:24-31`) stop being used by the workspace. Today they are used in:
- `workspace/chrome.rs` (36 lines);
- `workspace/search.rs` (26);
- `widgets.rs` (11, `Kind::Chrome` and `chrome_input`);
- `screens/library.rs` (9).

Move each use to the light tokens as its screen is redone. Delete the `chrome*` fields and `Kind::Chrome` in the last PR, once nothing uses them. Until then, keep them so unfinished screens still look right.

**Tests:**
- Update `tokens_match_app_css` (`theme.rs:507`).
- Replace `muted_chrome_text_is_readable_on_the_chrome_surfaces` with one test that checks every text/background pair the design uses is at least 4.5:1, in both palettes. The pairs:
  - `ink`, `ink_2`, `muted` and `faint` on `app`, `surface`, `surface_2` and `rail_active`;
  - `ok` on `app`;
  - `warn` on `warn_soft`;
  - `error` on `error_soft` and on `surface`;
  - `accent_text` on `accent_soft`;
  - `accent_ink` on `accent`.

  The mockup values pass all of these. The calculation script used to check them is a 10-line WCAG relative-luminance formula.

### 1.2 Widgets in `widgets.rs`

Reuse what's there (`Button` with `Kind`/`Size`/`.on()`/`.pill()`, `segmented`, `select`, `search_input`, `text_input`, `text_area`, `checkbox`, `card`, `modal*`, `drop_menu`, `MenuItem`, `kbd`, `splitter`). Add only these:

| New widget | Look (mockup class) | Notes |
|---|---|---|
| `switch(ui, p, &mut bool, label) -> Response` | `.sw`: 36×20 track with a 16 px knob. The track is `accent` when on and `border_strong` when off. | Settings use it instead of checkboxes. AccessKit role is a switch or checkbox, with the setting's name as the label. |
| `chip(ui, p, text, tone)` | `.chip`, `.pill`, `.badge-st`: an 18–28 px rounded pill in `warn_soft`/`warn`, `error_soft`/`error` or `ok_soft`/`ok`. | `project_settings.rs:524` and `bib.rs:1025` have private versions. Replace them with this one. |
| `rail_button(ui, p, icon, label, on, badge: Option<(u32, Tone)>) -> Response` | `.rail-btn`: 36×36, 8 px radius. Icon 18 px in `muted`. When on, `accent` icon on `rail_active` with `shadow_card`. Badge top right. | The label is the accessible name and the tooltip. |
| `side_row(ui, p, icon, text, on, trailing)` | `.row`: 30 px high, 7 px radius. Hover fill `surface_3`. When on, `rail_active` + `shadow_card`, the icon in `accent`. | Shared by the file tree, outline, library nav and settings nav. |
| `card_frame(p) -> egui::Frame` | `.card`: `surface`, `R_LG`, `shadow_card`. | One place for the card look. |
| `R_XL = 14` constant | Dialog corners. | `modal*` uses it. |

Changes to existing widgets:
- **`Kind::Ghost`:** the outline becomes a 1 px `border` inset, with a transparent fill and a `surface_3` hover fill (`.btn.ghost`).
- **`segmented`** (`widgets.rs:330-353`):
  - the track becomes `surface_3`, with 9 px radius and 3 px padding;
  - the selected thumb becomes `surface` with `shadow_card`;
  - the `chrome: bool` argument goes away once nothing passes `true`.
- **`modal*`** (`widgets.rs:981-1022`): `R_XL` corners, and a 32 px close button at the top right. The footer is drawn on `surface_2` with a 1 px top border (see `.d-foot` and `.s-foot`).
- **Widget gallery** (`widgets.rs:1679`, snapshots `gallery_light`/`gallery_dark`): add the new widgets so they get snapshot coverage.

**Icons:** all the icons the mockups use already exist in `icons_data.rs` (the mockups inline them from that file). Draw them with `icons::show(ui, icon, size, color)` or `icons::paint(...)` (`icons.rs:40-51`). If you need a new one, append its Lucide paths to `icons_data.rs`. The test `every_icon_parses_inside_its_box` checks them.

---

## 2. Screens, in build order

Each screen: what changes, where the code is, the steps, the labels that change, and the snapshots to regenerate.

### Screen 1: Workspace frame

**Mockup:** `design/mockups/workspace.html`, compared with `today/editor-split-light.png`.

**Today:**
- **No egui panels.** `Workspace::show` (`workspace/mod.rs:1312`) cuts the window into `Rect`s by hand. There is no `SidePanel` or `CentralPanel`.
- **Toolbar:** a 46 px strip, `chrome::toolbar` (`workspace/chrome.rs:35-277`), painted in `p.chrome`.
- **Sidebar:** `chrome::sidebar` (`chrome.rs:296`), painted in `chrome_2`. Files take 60% of its height and the outline 40%.
- **Layout:** the string `settings.layout`, one of `"editor"`, `"split"` or `"pdf"`. The segment labels are "Editor", "Both" and "PDF" (`chrome.rs:54-58`, `:92-97`).
- **Panels:** Git, History and Comments buttons in the top bar (`chrome.rs:229-262`) open `Panel` {Review, History, Git, Todos} (`workspace/panels.rs:13`) in **the PDF's rect** (`mod.rs:≈1434`).
- **Edit bar:** `chrome::edit_bar` (`chrome.rs:1226`). It is a horizontal `ScrollArea` with hidden scrollbars when narrow (`:1260`). There is no overflow menu.
- **Status bar:** `chrome::status_bar` (`chrome.rs:1442`).

**Target geometry** (from `base.css`):

| Part | Size |
|---|---|
| Rail | 52 px wide, full height, on `app`. Logo tile 32×32 at the top. Buttons 36×36 with 2 px gaps. "All projects" and "Preferences" at the bottom. |
| Top bar | 48 px high, starting right of the rail, on `app`. |
| Sidebar | 232 px (Files, Outline) or 312 px (all other panels), on `app`. The existing sidebar splitter keeps working, min 180, max 480; remember the width per kind. |
| Main area | Padding 0 on the left, 8 px right and bottom. Editor card, an 8 px gap that is the splitter (a 3×32 px grip shows on hover), then the PDF card. |
| Editor card | Tabs row 40 px. Edit bar 40 px. Code area. Footer 30 px. 1 px `border` lines between them. |
| PDF card | Header 40 px (issues chip, pager, zoom, search, download, "…"). Body on `pdf_ground` with 24 px padding. |

**Steps:**
1. **Rail.** Draw a new `chrome::rail` at the left of `Workspace::show`, before the toolbar.
   - Items in order: Files (`FILES`), Outline (`LIST_TREE`), Search (`SEARCH`), Problems (`CIRCLE_ALERT`, with an error-count badge in `error` / `accent_ink`), Git (`GIT_BRANCH`, with a change-count badge), History (`HISTORY`), Comments (`MESSAGE_SQUARE`, with an open-thread badge), To-dos (`CIRCLE_CHECK`).
   - Then a spacer, then Notifications (`BELL`, with unread badge), All projects (`LAYOUT_GRID`, accessible label `"Back to all projects"`) and Preferences (`SETTINGS`).
   - Clicking the active item hides the sidebar (`settings.sidebar_open = false`). Clicking another opens the sidebar on it (on windows `<= 1000 px`, opens as an overlay drawer or compact sidebar so panels remain reachable).
   - The panel content is screen 4. For this PR, Files and Outline are enough, plus the move of the existing panels (see screen 4, step 1).
2. **Top bar** (`chrome::toolbar`):
   - Paint `app` instead of `chrome`, with no bottom border.
   - **Left:** the project name as a button with `CHEVRON_DOWN`. It opens today's `Menu::Main` items. Then `/`, then the current file's path in `ink_2`. Truncate project name and file path independently so the centred layout control never collides.
   - **Centre:** the layout `segmented` with "Editor" (`SQUARE_PEN`), "Split" (`COLUMNS_2`) and "PDF" (`FILE_TEXT`). Keep the values `"editor"`/`"split"`/`"pdf"`.
   - **Right:**
     - a command pill ~240 px wide: `SEARCH`, "Run a command", `kbd("Ctrl Shift P")` (or `Cmd Shift P` on macOS). Opens Command Palette (`Mod-Shift-P`); Quick Open stays separate on `Mod-P`;
     - the build status: `CHECK` + "Compiled 1.1 s" in `ok`, or the error state in `error`. Today this is in the status bar as "Compiled in…";
     - the Build targets picker (`<target> ▾`) whenever `config.targets.len() > 1`;
     - the Recompile split button: keep the existing button + caret + `Menu::Compile` (with `accent_ink` alpha divider so dark mode looks crisp).
   - **Removed from the top bar:** the menu button, "Lemmata /", the sidebar toggle (the rail replaces it), the project settings gear (moves to the project menu and the settings window, screen 5), and the Git/History/Comments buttons (now in the rail).
   - **Narrow:** below 1180 px the pill collapses to a 36 px search icon. Below 1000 px the sidebar defaults collapsed (clicking any rail icon opens it as a drawer/panel) and the status text hides.
3. **Sidebar:** paint `app` instead of `chrome_2`, with no vline. The header is 32 px: the title in sans 600 13 px, then its action icons (`FILE_PLUS`, `FOLDER_PLUS`, `UPLOAD` for Files).
   - File rows use `side_row`. The active file is `on`. Unsaved files get a 7 px `warn` dot. `main.tex` gets a `MAIN` tag in `accent_text` on `accent_soft`. Folder icons are in `atom`.
   - Footer line: `GIT_BRANCH` + "main · 3 changes" in `faint`.
   - The outline is no longer stacked under the files. It is its own rail panel, using today's `outline_rows` (`chrome.rs:835`).
4. **Cards:** draw the editor column and the PDF pane inside `card_frame` rects, with the 8 px gap. The gap is the existing PDF splitter, made 8 px wide and invisible apart from the hover grip.
5. **Tabs** (`mod.rs:≈1556`): a 40 px row in the card header. The active tab is `surface` with a 1 px `border` on three sides, joined to the line below. Inactive tabs are `muted` text. The close `X` shows on hover and on the active tab. Unsaved shows the `warn` dot before the close button. Keep the "…" at the end of the row.
6. **Edit bar** (`chrome::edit_bar`):
   - A "Paragraph ▾" style picker replaces "H ▾" (same `Menu::Heading`). It is a bordered 28 px button showing the current level's name.
   - Groups, separated by 1×18 px `border` dividers:
     - style;
     - Bold, Italic, Link;
     - Maths, Symbol, Figure, Table, Code block (today's code button and `Menu::Code`);
     - Bulleted, Numbered;
     - Cite (icon + label);
     - then pushed right: Comment, "Show in PDF" (today "Go to"; keep the action) and "…".
   - **Replace the hidden-scrollbar `ScrollArea` with overflow by width:**
     - at 720 px card width or less, Comment and Show in PDF lose their labels;
     - at 600 px or less, Code block and Numbered move into "…";
     - at 480 px or less, Figure and Table move too.

     "…" is a `drop_menu` listing the moved items, with their labels. Measure with the same approach as `segmented_width`.
7. **Code area:** gutter numbers in `faint` at full opacity (keeping >= 4.5:1 contrast), and the current line's number in `ink_2`. The caret line is `line_hl`. The gutter has no separate background (today it uses `gutter`). Wrapped lines hang by 2 columns (the mockup's wrap indent).
8. **Footer** (`chrome::status_bar`), 30 px, `faint` 12 px:
   - left: "Ln 7, Col 78", the save state, then the error and warning counts (`CIRCLE_X` 1 in `error`, `TRIANGLE_ALERT` 3 in `warn`), plus clickable "Grammar error" retry link and Restricted indicator when applicable. Clicking a count opens Problems;
   - right: language, words, engine;
   - "Compiled in…" moves to the top bar.
9. **PDF card header** (`pdf/pane.rs`):
   - The "Logs N" button (`logs_button`, `pane.rs:813`) becomes a chip: `TRIANGLE_ALERT` + "4 issues", in `warn_soft`/`warn`, or `error_soft`/`error` when there are errors. **Clicking it opens the Problems rail panel** (screen 4) instead of toggling `Build.logs_open`.
   - Then a divider, the pager (‹ [7] / 42 ›), a spacer, zoom (−, "Fit width", +), a divider, search, download and "…".

**Labels that change** (update the tests in the same commit):
- **"Both" → "Split".** The layout segment. No test queries it by text today, but the a11y names come from it.
- **"Logs" → "4 issues" chip.** Give the chip the accessible label "Problems", and update the clicks at `app.rs:1948`, `:1955` and `:2076`.
- **"Comments", "History", "Git".** They move from top-bar buttons to rail buttons. Keep exactly these labels on the rail buttons, so `app.rs:2321`, `:2329`, `:2623`, `:2638` and `scenarios.rs:888` keep working.
- **"Menu".** The button is gone; its items move to the project-name button. Update the two tests that use it.
- **"Back to all projects".** Keep it as the accessible label of the rail's All projects button (9 tests use it).
- **"Compiled in…".** 8 tests use it. Either keep it as the accessible label of the top-bar status, or update the tests.

**Snapshots to regenerate:** `editor-split-light`, `editor-split-built-light`, `edit-bar-narrow-light`, `editor-problem-light`, `pdf-narrow-dark`, `tree-drag-light`, `editor-alone`, `editor-folded`, plus every panel snapshot (screen 4).

### Screen 4: Workspace panels (build right after screen 1)

**Mockup:** `workspace.html` with `#search`, `#problems`, `#git`, `#history`, `#comments` or `#todos` in the URL, or click the rail. Compare with `today/logs-light.png` and the `git-light`, `history-light`, `review-light`, `todos-light` and `search-light` snapshots.

**Today:**
- `Panel` {Review, History, Git, Todos} (`workspace/panels.rs:13`) draws in the PDF's rect (`show_panel`, `:101`).
- `open_panel` (`:64`) switches `"editor"` to `"split"` to make room.
- Project search is drawn in the sidebar slot when active (`search_panel`, `workspace/search.rs:320`).
- Problems are drawn inside the PDF pane (`Problems::show`, `pdf/problems.rs:36`, toggled by `Build.logs_open`).

**Target:** one enum for what the sidebar shows:

```rust
pub enum SidePanel { Files, Outline, Search, Problems, Git, History, Comments, Todos }
```

It replaces `Panel`, the "search active" special case and `logs_open` as a place. Persist it in settings like `sidebar_open`. Every panel draws in the sidebar rect, and **the PDF stays visible**. `open_panel` no longer changes the layout; it opens the sidebar instead.

**Steps:**
1. **Move the existing panels.** Point `review_panel` (`panels.rs:281`, which draws through `panels/review.rs:180`), `history_panel` (`history.rs:333`), `git_panel` (`panels.rs:672`) and `todos_panel` (`panels.rs:377`) at the sidebar rect.
   - They were drawn at PDF width (≈560 px) and now get 312 px. Check each one for fixed widths and rows that don't wrap.
   - Drop the per-panel header with its close `X` (`panels::bar`, `panels/mod.rs:20`). The rail closes the panel, so each panel gets the standard 32 px sidebar header: the title plus its own actions.
2. **Search** (mockup `.panel.search`):
   - the query field with the Match case / Whole word / Regex toggles inside it (24×22 mono buttons; on = `accent_soft`/`accent_text`);
   - a "Replace with" field;
   - "9 hits in 3 files" plus a "Replace all" link;
   - results grouped by file (chevron, file icon, path in sans 600 12.5 px, count), with hits as mono 12.5 px rows (line number in `faint`, the match on a `warn_soft` mark). The selected hit is `rail_active` + `shadow_card`.
   - The logic stays in `workspace/search.rs`. Its 26 `chrome_` uses move to the light tokens.
3. **Problems** (mockup `.panel.problems`):
   - The header shows "Problems" plus count pills (`error` and `warn`), and a "Raw log" icon (`SCROLL_TEXT`). Raw log opens today's raw log view in the PDF card's place, with a back button; keep `Problems`'s raw-log tab logic.
   - Each problem is a card: the severity icon, a bold message with inline `code`, the location as a mono link in `accent_text` (`chapters/method.tex:7`) and the help text in `muted`.
   - The first error shows its source excerpt (`pdf/problems.rs:403` `excerpt`), with the line on `error_soft`, and any fix buttons ("Search online", "Open refs.bib"; use the existing `NoticeAction`s where they apply).
   - "Show 2 layout notes" checkbox at the end.
   - Move `Problems::show`'s drawing from the PDF pane to here. Its `card` (`:216`) needs narrow-width wrapping.
4. **Git** (mockup `.panel.git`):
   - the branch `select` with ahead/behind ("↑1 ↓0"), then Fetch / Pull / Push N as small ghost buttons;
   - "CHANGES 3" with a "Stage all" link, and rows of checkbox + path + status letter (`M` in `warn`, `A` in `ok`);
   - the commit message `text_area`, then a full-width primary "Commit 2 files" button with `kbd("Ctrl Enter")`;
   - "RECENT": the last commits as two-line rows.
5. **History** (mockup `.panel.history`):
   - the header has a "+ Snapshot" ghost button (today "Save a snapshot");
   - "Review changes since" `select`, then "Review changes" (primary, small) and "PDF of changes";
   - snapshots grouped by day (TODAY, YESTERDAY) as a timeline: a 7 px dot in `border_strong`, the current one in `accent`, named snapshots in `warn`; a two-line row; the current row lifted.
   - Diff and review views keep opening where they open today.
6. **Comments** (mockup `.panel.comments`):
   - the header has an Open N / Resolved switch (small segmented);
   - each thread is a card: the quoted text (italic, `muted`, a 2 px `warn` left bar), messages with a 24 px initials avatar (others on `warn_soft`, you on `accent_soft`), then a reply field + "Resolve";
   - the thread at the caret gets a 2 px `accent` ring.
7. **To-dos** (mockup `.panel.todos`): grouped by file like Search, with the note "Collected from % TODO and % FIXME comments." at the end.

**Labels:**
- Keep "Comments", "History", "Git" and "Problems".
- "Save a snapshot" → "Snapshot": its accessible label stays "Save a snapshot".
- Check the tests that close panels through the old header's "Close Git" and similar labels; they now click the rail button.

**Snapshots:** `git-light`, `history-light`, `review-light`, `todos-light`, `search-light`, `logs-light`.

### Screen 2: Library

**Mockup:** `design/mockups/library.html`, compared with `today/library-grid-light.png`.

**Today:** `screens/library.rs`:
- `show` (`:319`) draws a fixed 248 px rail (`rail_ui`, `:432`, dark `chrome`) and `main_ui` (`:534`): header, sort `select`, List/Grid `segmented`, "Search projects", "New folder".
- The views are `table` (`:764`) and `grid` (`:1096`), with cards drawn by `card_frame` (`:1284`) and `card_info` (`:1338`).
- **Thumbnails are real.** `thumb` (`:1038`) decodes PNGs on 2 worker threads; `thumb_mini` (`:1058`) is the 30×38 placeholder.
- Per-project actions: `more_button` (`:741`) and `overlays` (`:1396`).

**Steps:**
1. **Sidebar** (`rail_ui`): paint `app` instead of `chrome`. Contents:
   - the "Lemmata" wordmark (Young Serif 24 px);
   - "New project" (primary, full width, 36 px, `kbd("Ctrl N")`);
   - nav rows: Recent (on), All projects (with count), Templates (opens New project on the gallery);
   - "FOLDERS" with a `FOLDER_PLUS` action (today's "New folder" button moves here), then the folder rows with counts;
   - "BRING IN A PROJECT": Open a folder…, Clone a repository…, Import from Overleaf…, Example document (today's four rail entries, `:469-472`);
   - at the bottom: Preferences and "What's new in 3.3".
2. **Main area:** a white card (`card_frame`), with 8 px margin on the top, right and bottom and padding 26/32 px.
   - **Header:** the title in Young Serif 28 px ("Recent", or the folder name), a spacer, the search field (240 px, `kbd("/")` focuses it), the sort `select` (150 px) and the grid/list `segmented` (icon only).
   - **"Continue writing":** up to 3 cards in an auto-fill grid with a minimum width of 220 px. Each has a 150 px thumbnail area on `pdf_ground` with the real first page (the existing `thumb`), the name in sans 600, a status chip (`chip`: ✓ in `ok`, or ⚠ N in `warn`, or N errors in `error`, from the last build if it's known), then "folder · N pages · when". **Only show what's actually known**: leave out pages or status when they aren't recorded.
   - **"All projects":** today's `table`, restyled. Columns: Name (icon + name in sans 600 over the path in mono 11.5 px `faint`), Folder, Compiler, Last opened, then the "…" `more_button`. Rows are 1 px `border` separated, with a `surface_2` hover.
   - The grid view (`grid`) uses the same card as "Continue writing".
3. **Narrow:** below 900 px hide the Compiler column. Below 760 px hide the sidebar and the Folder column, and the search goes full width.

**Labels:**
- "Your projects" (3 tests) becomes the title "Recent" or the folder name. Keep "Your projects" as the main area's accessible name, or update the tests.
- Keep "New project", "Search projects" and "Actions for {name}".

**Snapshots:** `library-{grid,list,clone,import,new-project,new-project-papers,save-template}-{light,dark}` and `library-long-folder-light`.

### Screen 3a: New project

**Mockup:** `design/mockups/new-project.html`, compared with `today/library-new-project-light.png`.

**Today:**
- `dialogs::new_project` (`screens/dialogs.rs:251`) is a 720 px modal. It shows either categories (`gallery_categories`, `:307`) or a category's templates (`gallery_templates`, `:491`), with "Folder name" and "Location" below.
- The categories come from `NewProject::categories` (`:150`) and their icons from `category_icon` (`:85`).

**Steps:**
1. A 980×680 dialog (`R_XL`), in three rows:
   - **Header:** "New project" in Young Serif 22 px, the subtitle "Start from a template. It's a normal folder you can open in any editor." and a close button.
   - **Body:**
     - left, a 210 px category nav on `surface_2`: All templates (with count), each category with its icon and count, then "YOURS": My templates, Journals & conferences;
     - right, "Search templates", then a grid of template cards (auto-fill, minimum 168 px): a 128 px preview on `pdf_ground`, the name, and a short "engine · bibliography" line. The selected card has a 2 px `accent` ring.
   - **Footer** on `surface_2`: Folder name | Location (mono, with "Choose…") | Cancel and "Create project".
2. One step instead of two: the categories filter the grid instead of replacing the view.
   - Previews: the `NewProject.previews` field exists. Use real previews where a template has one; otherwise draw the mini page placeholder (white page, title line, grey bars).
3. Keep the online templates (Journals & conferences: `online_loading`/`online_error`) as a category whose grid loads on demand.

**Labels:** keep "New project", "Folder name" (6 tests), "Location", "Create project" and "Cancel".

**Snapshots:** `library-new-project-{light,dark}` and `library-new-project-papers-{light,dark}`.

### Screen 3b: Set up TeX (onboarding)

**Mockup:** `design/mockups/onboarding.html`, compared with `today/onboarding-light.png`.

**Today:** `screens/onboarding.rs`: `show` (`:61`), `body` (`:118`), `checks` (`:242`, the tool tiles), `tabs` (`:313`, the OS tabs), `sizes` (`:368`), `command_box` (`:426`), `tectonic` (`:491`) and `footer` (`:556`).

**Steps** (mostly restyling, as the content stays):
1. **Top row:** the "Lemmata" wordmark on the left. On the right, the steps "1 Install TeX — 2 Check — 3 Start writing" (20 px dots; the current one is `accent` with `accent_ink`).
2. **Card:** 860 px max, 14 px radius, 32/36 px padding.
   - eyebrow "ONE STEP BEFORE YOU START" in `accent_text`;
   - title in Young Serif 32 px;
   - lead paragraph in `ink_2` 14 px.
3. **Tool chips:** missing required tools on `error_soft` with `CIRCLE_X` in `error`. Optional ones on `surface_2` with an inset `border` and `CIRCLE` in `faint`. Each has the name in mono and "required"/"optional" under it.
4. **"Install it":** OS tabs as a `segmented`, with " · this computer" in `faint`. Then two radio cards (Recommended / Full; the selected one has a 2 px `accent` inset and `accent_soft` fill). Then the command block (fixed dark `#0f1418` in both themes, mono 13 px), then Copy and "Open a terminal" as ghost buttons.
5. **Tectonic:** a `surface_2` box with its text and a "Use Tectonic" button.
6. **Footer:** "Installed somewhere else? Choose the TeX bin folder" and a primary "Check again".

The welcome flow (`screens/welcome.rs`, three steps) should use the same card and step header. It has no separate mockup: follow this page.

**Snapshots:** `onboarding-{light,dark}` and `welcome-{1,2,3}-{light,dark}`.

### Screen 5: Settings

**Mockup:** `design/mockups/settings.html` (`#project` opens the project scope), compared with `today/preferences-light.png` and the `project-settings-light` snapshot.

**Today:** two separate modals.
- **Preferences:** `screens/prefs.rs:93-125`, a 600 px `modal_with` with sections Appearance (`:162`), Editing (`:254`), Language (`:368`), Files (`:481`), TeX (`:508`) and Updates (`:545`), plus the snippets editor modal (`:577`).
- **Project settings:** `workspace/project_settings.rs:81`, a 540 px `modal_editing` with Save/Cancel.
  - Fields: main document, build targets, compiler, bibliography, timeout, build folder, "Build only these chapters", spell check, trust, and shell escape.
  - Turning shell escape on also grants trust (`grants_shell_escape_trust`, `:59`, applied at `:148-150`).

**Target:** one 920×660 settings window.
- **Left nav** (230 px, `surface_2`): the title "Settings" in Young Serif 20 px, a scope `segmented` "Lemmata | <project name>" (the project half only inside a workspace), the section rows, and the version at the bottom.
- **Right:** a header (section title + "These apply to every project." or "Only for <project>."), a scrolling body, and a footer.

**Steps:**
1. A new `screens/settings.rs` (or rename `prefs.rs`) that owns the window and the scope. Keep the existing per-field code: move each section's body into its own function and draw it in the right pane.
2. **Lemmata scope:** sections Appearance, Editing, Language, Files, TeX, Updates (today's headings).
   - The snippets editor opens from a "Manage snippets" row under Editing.
   - Preferences apply at once (as today). The footer says "Changes apply at once." and has "Done".
3. **Project scope:** sections Build (main document, compiler, bibliography, stop after N seconds, build folder, build targets, Build only these chapters, spell check language) and Trust & safety (trust, shell escape, and the warning callout on `warn_soft`).
   - **Save/Cancel semantics stay** (`modal_editing`: a click outside does not close and discard). The footer reads "Saved in lemmata.toml, so they travel with the project." with Cancel and Save.
   - **Keep the trust rule exactly:** turning shell escape on grants trust. It's a security behaviour with tests.
4. **Rows** (`.setting`): label (500) and help text (`muted` 12.5 px) on the left; the control on the right, 200–280 px wide; a 1 px `border` between rows. Booleans use the new `switch`. Choices of 2–3 use `segmented`; longer ones use `select`.
5. **Ways in:**
   - Preferences (library sidebar, rail, palette) opens the Lemmata scope. No `Ctrl+,` binding (per user decision);
   - Project settings (project menu, palette, and the minted notice's `NoticeAction::ProjectSettings`) opens the project scope on the right section; the minted notice opens Trust & safety.
   - Switching from a dirty Project scope to Lemmata scope warns before discarding unsaved project edits.

**Labels:**
- "Preferences" (a MenuItem query at `app.rs:3135`) and "Project settings" stay as the entry points.
- Field labels stay the same, so the field-level tests survive.
- The dialog titles change from "Preferences" / "Project settings" to "Settings". Update the tests that query the dialog by title.

**Snapshots:** `preferences-light`, `project-settings-light` and `snippets-light`.

### Screen 6: Popups and small surfaces

**Mockup:** `design/mockups/popups.html`, compared with `today/palette-light.png` and the `quick-open-light`, `completion`, `completion-maths`, `hover-input` and `math-preview` snapshots.

**Steps:**
1. **Command Palette (`Mod-Shift-P`) and Quick Open (`Mod-P`).**
   - Keep `Palette` (`workspace/palette.rs:42`) with its separate `Kind` modes (`Commands`, `Files`, `Environments`, `Headings`, `Symbols`) and existing shortcuts (`Mod-Shift-P` and `Mod-P`) — do NOT merge them into `Ctrl K` (per user decision).
   - Restyle the popup to the new design tokens:
     - 500 px wide, 46 px input row with `SEARCH` and `kbd("Esc")`;
     - 34 px rows with icon, text (the match in sans 600 `ink`) and a mono detail on the right; the selected row on `accent_soft`;
     - a footer with the key hints (`↑↓ move · Enter open/run`).
   - Draw it with `R_LG` and `shadow_lg` (as `palette.rs:292` does today).
2. **Completion** (`editor/popup.rs:119`, logic in `editor/complete.rs`):
   - a 360 px list of 28 px rows: an 18 px kind badge (`\` for commands on `surface_3`, `{}` for environments on `ok_soft`/`ok`, `#` for labels on `warn_soft`/`warn`), the typed part in bold `ink`, and a short detail on the right in sans `faint`;
   - a one-line help section at the bottom for the selected item, where `complete.rs` has text for it.
3. **Hover cards** (`editor/hover.rs:251`; maths preview `math_preview` at `editor/view.rs:2957`): a 330 px card with a 12 px `faint` title line (icon + "Equation (2.1)", or "key · refs.bib"), the content (using `surface_2`/`ink` in dark mode so maths previews don't flash pure white), then small ghost action buttons ("Open entry", "arXiv") where the actions exist.
4. **Notices** (`Workspace::toasts`, `mod.rs:1973`; `Notice`/`NoticeAction`, `mod.rs:55-132`):
   - a bottom-centre stack of cards up to 420 px wide: the tone icon (`error`, `warn` or `ok`), the text with a bold lead, the action as a small button, and a dismiss `X`;
   - keep the 8 s expiry rule (errors and notices with actions stay);
   - successful builds don't toast, since the status lives in the top bar.

**Labels:** the palette's input label and the completion list's labels are used in `editor/view.rs` tests (30 label queries). Keep them.

**Snapshots:** `palette-light`, `quick-open-light`, `symbols-light`, `completion`, `completion-maths`, `hover-input`, `math-preview`, `update-light`.

### Last PR: clean-up

- Remove the `chrome*` palette fields, `Kind::Chrome`, `widgets::chrome_input` and the `chrome` argument of `segmented`, once nothing uses them.
- Remove `Panel`, `Build.logs_open` as a place, and `Menu::Main`'s old button.
- Update `docs/guide.md` and the README screenshots if they show the old UI.
- Regenerate every snapshot one last time and look through them in light and dark.

---

## 3. Resolved decisions

1. **Panels move from the PDF side to the left sidebar** (screen 4): **Approved.**
2. **Unified Settings window** (screen 5): **Approved** (preserving Save/Cancel for Project scope and instant-apply for Lemmata scope).
3. **Merge Command Palette & Quick Open into `Ctrl K`** (screen 6): **Declined.** Keep separate `Mod-Shift-P` and `Mod-P` boxes, restyled visually.
4. **Bind `Ctrl ,` to Settings**: **Declined.** No `Mod-,` shortcut added.
5. **3.3.0 features preserved:** Build targets picker in top bar, Notification Center bell in rail, Git Stash/Diff/Merge/Rebase/Restricted states, virtualized Raw Log, Confirm-before-Replace-All modal, Comments Reply & Delete-Cancel, Grammar error retry, Outline `Nw` word counts, Onboarding `found` chips & Continue without TeX.

## 4. Working on the mockups

When the design changes, edit `design/mockups/src/` (the pages, `base.css` and `partials/`) and run:

```bash
python3 design/build.py
```

It expands the `<!-- @include … -->` partials and inlines the Lucide icons from `apps/lemmata/src/icons_data.rs`. Commit both `src/` and the generated pages. `design/mockups/today/` holds copies of the current snapshots for the before/after switch. Refresh them if the "today" view should show a newer state.
