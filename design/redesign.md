# Lemmata UI redesign

The plan is one screen at a time. Each screen gets an HTML mockup and an OK before any Rust is written.

Mockups:
- `mockups/workspace.html`: screen 1. Open it in a browser. The switch at the bottom right toggles light/dark and shows today's UI for comparison.

## Direction

Today's look is Overleaf's: a dark navy top bar and file sidebar, a light editor and a grey PDF ground. The new direction keeps Lemmata's identity:
- Young Serif for display text, IBM Plex Sans and IBM Plex Mono;
- the blue accent;
- Lucide icons.

It changes the frame around the work:
- **Quiet chrome.** The rail, the top bar and the sidebar sit on the app ground, with no dark bands. Only the work (editor, PDF) is on white cards.
- **One rail for panels.** Files, Outline, Search, Git, History, Comments and To-dos are icons in a 52 px rail. Clicking one switches the sidebar; clicking the open one hides it. This replaces the Comments/History/Git buttons in the top bar and the stacked Files + Outline panes.
- **A calm top bar:**
  - left: the project switcher and the file path;
  - centre: the Editor / Split / PDF segmented control;
  - right: a command pill (Ctrl K), the build status, and Recompile as the only filled button.
- **Two cards:** editor and PDF, 8 px apart, with a draggable divider. Each card has its own header (tabs, or PDF controls) and toolbar.
- **The toolbar shrinks on its own.** A "Paragraph ▾" style picker replaces "H ▾", and groups are split by thin dividers. When the card is narrow, labels go first, then the less-used tools move into "…".
- **Problems are visible where they matter:**
  - the error and warning counts are in the editor footer;
  - the PDF header has a "4 issues" chip instead of "Logs 4";
  - wavy underlines mark the problems in the text.

Everything in the mockup can be drawn in egui: solid fills, 1 px strokes, rounded corners and epaint shadows. There's no blur and no gradients.

## Screens, in order

1. **Workspace (editor + PDF):** mockup ready (`mockups/workspace.html`).
2. **Library (home):** the light sidebar from screen 1, a "Continue writing" row with real PDF first-page thumbnails (today's cards are empty placeholders), quick actions (New, Open folder, Clone, Import from Overleaf) and the list/grid view.
3. **New project and onboarding (TeX setup):** template cards with a preview, and onboarding in the same card style.
4. **Workspace panels:** Logs/Problems, Git, History, Comments and To-dos as rail panels.
5. **Preferences and project settings:** a settings layout with a left nav.
6. **Polish:** command palette, quick open, completion and hover popups.

## Token changes for `apps/lemmata/src/theme.rs`

The mockup's `:root` variables use the `Palette` field names.

| Token | Today (light) | Mockup (light) | Note |
|---|---|---|---|
| `app` | `#e8ebee` | `#eceef1` | The ground now carries the rail, top bar and sidebar too. |
| `surface_3` | `#eef0f2` | `#e4e7eb` | Hover fill on the ground, and the segmented control's track. |
| `muted` | `#5c6873` | `#5a6671` | Keeps 4.5:1 on the new ground. |
| `ok` | `#2e7d55` | `#2a7550` | 4.8:1 on the new ground (was 4.3). |
| `warn` | `#9a6b0a` | `#8a5f08` | 5.1:1 on `warn_soft` for the issues chip (was 4.2). |
| `pdf_ground` | `#c8cdd2` | `#e1e5e9` | Lighter, because the PDF is now inside a card. |
| `chrome*` (8 fields) | navy | not used in the workspace | The library and dialogs keep them until their own redesign. |
| `rail_active` | (new) | `#ffffff` / dark `#1e262e` | The selected rail and sidebar item, lifted with `shadow_card`. |
| `line_hl` | (new) | `#f4f6f8` / dark `#1c232a` | Caret line. |
| `shadow_card` | (new) | 1 px hairline + 2 px soft shadow | The cards and the selected items. |

The dark theme keeps today's values apart from the new tokens. Small accent text on `accent_soft` (the `MAIN` tag) uses `accent_on_dark` there, because `accent` reaches only 4.4:1.

All text pairs in the mockup reach 4.5:1 in both themes.

## Mobbin references

These are the shared patterns behind the direction. The links open on Mobbin.

- Light sidebar on the ground, white content card, icon rail:
  - [Plane, editor](https://mobbin.com/screens/c93205ba-cc78-4df0-8a31-7b1a90ad2636)
  - [Plane, code block with language picker](https://mobbin.com/screens/5690b3a4-ddd6-446e-9b4f-1448946ed9f6)
  - [Snowflake, notebook](https://mobbin.com/screens/31cbecf2-e0b4-40e1-9fd4-8fa5e93710e1)
- Centred segmented control for the view (like Editor / Split / PDF):
  - [Subframe](https://mobbin.com/screens/fec41599-0c06-4585-91d2-3513d9e0dd66)
- Editor and preview as cards side by side:
  - [Cofounder](https://mobbin.com/screens/725ccb78-8265-4c8f-9b79-b9946c95300e)
  - [WRITER](https://mobbin.com/screens/98d60fb2-886b-4a0d-8eea-87031ed6e15d)
- Quiet document chrome and a compact formatting bar:
  - [Notion](https://mobbin.com/screens/b8778e0e-13d3-49a6-9f01-3680ea66249b)
  - [Fibery](https://mobbin.com/screens/349b2a81-48a8-4955-8118-76062dca5bff)
  - [Grok files](https://mobbin.com/screens/007f9e99-0490-45c4-82a3-b5330d10927d)
- For screen 2 (library):
  - [Canva home](https://mobbin.com/screens/9f542971-3024-4f14-967f-a92c8536dd35)
  - [Snowflake home](https://mobbin.com/screens/eabc3923-dafb-4de7-ad04-44f226c6473b)
  - [Subframe projects](https://mobbin.com/screens/b8d8bc11-c02b-4bf8-bfa7-ed4d3d60a19f)

## Building the mockup

`mockups/workspace.html` is generated: its icons are inlined from `apps/lemmata/src/icons_data.rs`, so it stays one self-contained file. It loads only Google Fonts.
