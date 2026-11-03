# Compile mode design

## Intent

Let users keep autosave without getting a build after every autosave, especially while a document or table is temporarily incomplete.

## Behavior

Add a **Compile** selector alongside the existing **Save** selector:

- **As I type**: preserve the current behavior; every save can schedule a build.
- **With Ctrl+S**: autosave still writes changes but does not compile. Ctrl+S saves all files first, then schedules one build when all saves succeed.

The Recompile button and its existing shortcut remain independent build actions. No new keybinding is introduced.

## Approach

Reuse the existing `auto_compile` setting: `true` means **As I type**, `false` means **With Ctrl+S**. Gate save-driven builds on this flag and make the explicit Ctrl+S path save first and build afterward. Do not gate explicit actions such as Recompile, project changes, Git/history operations, or exports.

Add the selector wherever the Save selector appears, reusing its segmented-control presentation. Preserve the setting across restarts and update affected UI snapshots.

## Verification

Add regressions proving that autosave in manual-compile mode does not build, Ctrl+S builds only after successful saves, automatic mode retains current behavior, and the selector persists. Run the app and workspace suites, Clippy, formatting, and visual snapshots.
