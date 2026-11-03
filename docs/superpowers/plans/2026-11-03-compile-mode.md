# Compile Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users keep autosave while compiling only with Ctrl+S or an explicit Recompile.

**Architecture:** Reuse `Settings::auto_compile` and the existing `Workspace::recompile` save barrier. Gate save-driven builds on the current flag; in manual mode, make Ctrl+S invoke the existing save-then-build path rather than adding a shortcut or settings field.

**Tech Stack:** Rust, egui, existing app integration tests and snapshots.

**Spec:** `docs/superpowers/specs/2026-11-03-compile-mode-design.md`

## Global Constraints

- **As I type** preserves current save-triggered builds.
- **With Ctrl+S** never compiles from autosave; Ctrl+S builds only after all saves succeed.
- Explicit Recompile, exports, project changes, Git, and history operations keep their existing behavior.
- Preserve settings across restarts without schema changes or new keybindings.

## Review Focus

- Autosave in manual mode writes text but leaves the build sequence and deadline unchanged.
- Ctrl+S in manual mode builds once after successful saves, not after failed/conflicting saves.
- Automatic mode retains current behavior.
- Preferences and the compile menu expose the same two labels and update `auto_compile`.
- Existing saved `autoCompile` values continue to load correctly.

---

### Task 1: Save and build behavior

**Files:**
- Modify: `apps/lemmata/src/workspace/mod.rs:1052-1054`
- Modify: `apps/lemmata/src/app.rs` integration tests

**Interfaces:**
- Consumes: `Settings::auto_compile`, `Workspace::save_all`, `Workspace::recompile`.
- Produces: manual Ctrl+S behavior through the existing shortcut, with no new public API.

- [ ] **Step 1: Write the failing integration test**

Add `manual_compile_mode_builds_only_with_ctrl_s` using `in_article_with` with `auto_compile = false`. Wait for the initial build, type text, wait for autosave, and assert the build sequence and deadline are unchanged. Send Ctrl+S and assert the build sequence increases once.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p lemmata-app manual_compile_mode_builds_only_with_ctrl_s -- --nocapture`
Expected: FAIL because autosave schedules a build and Ctrl+S does not trigger a manual build.

- [ ] **Step 3: Route Ctrl+S by compile mode**

In `Workspace::shortcuts`, call `save_all` when `settings.auto_compile` is true and `recompile` when false. `recompile` already saves everything and schedules the build only after successful writes.

- [ ] **Step 4: Run behavior regressions**

Run: `cargo test -p lemmata-app manual_compile_mode_builds_only_with_ctrl_s -- --nocapture`
Expected: PASS. Also run `cargo test -p lemmata-app stopping_a_build_and_saving_twice_keeps_the_edit -- --nocapture` to cover the existing explicit-save path.

### Task 2: Compile mode controls

**Files:**
- Modify: `apps/lemmata/src/screens/prefs.rs:254-292`
- Modify: `apps/lemmata/src/workspace/chrome.rs:1571-1604`
- Modify: `apps/lemmata/src/store.rs:49`
- Update affected snapshots in `apps/lemmata/tests/snapshots`.

**Interfaces:**
- Consumes: the same `Settings::auto_compile` used by Task 1.
- Produces: **As I type** (`true`) and **With Ctrl+S** (`false`) in Preferences and the compile menu.

- [ ] **Step 1: Replace the Preferences checkbox with a segmented selector**

Add a **Compile** row using the Save row's segmented control and labels **As I type** and **With Ctrl+S**. Selecting index 0 sets `auto_compile = true`; index 1 sets it false and emits the existing settings-change action.

- [ ] **Step 2: Rename the compile-menu choices**

Use the same two labels in `Menu::Compile`, retaining the existing `settings.auto_compile = i == 1` behavior. Update the `auto_compile` field comment to describe both modes.

- [ ] **Step 3: Verify controls and snapshots**

Run the existing Preferences integration test and update only snapshots that intentionally changed from the new selector:
`cargo test -p lemmata-app preferences_snippets_and_project_settings_save -- --nocapture`

### Task 3: Full verification

**Files:**
- Modify only files needed by regressions found in Tasks 1–2.

**Interfaces:**
- Consumes: completed behavior and controls from Tasks 1–2.

- [ ] **Step 1: Run all standard checks**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git diff --check
```

Expected: all pass, including the new manual-mode regression and existing automatic-mode scenarios.

- [ ] **Step 2: Review the diff**

Confirm autosave behavior is unchanged outside compilation, explicit Recompile is untouched, settings round-trips require no migration, and no unrelated snapshot or dependency changes are included.
