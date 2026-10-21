//! The Svelte app's automation, in process: the invariants its `check` enforced and its seeded
//! random walk (scripts/automation/walk.mjs). A failing walk names its seed and the steps taken.

use super::tests::{in_article_with, wait_for};
use super::*;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

/// What must hold after every step, or why not.
pub(crate) fn check(h: &Harness<App>) -> Vec<String> {
    let mut out = Vec::new();
    let Some(ws) = &h.state().project else {
        return out;
    };
    if let Some(a) = &ws.active
        && !ws.tabs.contains(a)
    {
        out.push(format!("the active file {a} has no tab"));
    }
    // A file the app believes saved is what is on disk.
    if !ws.writing() {
        for p in &ws.tabs {
            let (Some(ed), Some(saved)) = (ws.editors.get(p), ws.buffers.last_saved.get(p)) else {
                continue;
            };
            let text = ed.doc.text();
            if ws.buffers.dirty.contains(p) || ws.buffers.conflicts.contains(p) || text != *saved {
                continue;
            }
            match std::fs::read_to_string(ws.info.root.join(p)) {
                Ok(disk) if disk != text => {
                    out.push(format!("{p} is marked saved but differs from the disk"))
                }
                _ => {}
            }
        }
    }
    out
}

/// mulberry32, as walk.mjs: small, seeded, good enough to pick actions.
fn random(mut seed: u32) -> impl FnMut() -> f64 {
    move || {
        seed = seed.wrapping_add(0x6d2b_79f5);
        let mut t = (seed ^ (seed >> 15)).wrapping_mul(1 | seed);
        t = (t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t))) ^ t;
        ((t ^ (t >> 14)) as f64) / 4_294_967_296.0
    }
}

fn walk(seed: u32, steps: usize) {
    let tmp = tempfile::tempdir().unwrap();
    let settings = Settings {
        autosave_ms: 400.0,
        ..Settings::default()
    };
    let Some((mut h, project)) = in_article_with(tmp.path(), settings) else {
        return;
    };
    let mut rnd = random(seed);
    let mut pick = |n: usize| (rnd() * n as f64) as usize;
    const FILES: [&str; 3] = ["main.tex", "chapters/method.tex", "refs.bib"];
    const WORDS: [&str; 5] = ["alpha", "beta", "gamma", "delta", "epsilon"];
    const ACTIONS: [&str; 9] = [
        "type",
        "save",
        "open",
        "close",
        "layout",
        "recompile",
        "undo",
        "external",
        "type",
    ];
    let mut taken: Vec<&str> = Vec::new();
    let fail =
        |taken: &[&str], why: String| panic!("seed {seed}, steps {}: {why}", taken.join(" → "));
    for i in 0..steps {
        let conflicted = h
            .state()
            .project
            .as_ref()
            .is_some_and(|w| !w.buffers.conflicts.is_empty());
        let name = if conflicted {
            "resolve"
        } else {
            ACTIONS[pick(ACTIONS.len())]
        };
        taken.push(name);
        let active = h.state().project.as_ref().and_then(|w| w.active.clone());
        match name {
            "resolve" => {
                let label = ["Keep my version", "Load the version on disk"][pick(2)];
                h.run_steps(3);
                if let Some(b) = h.query_by_label(label) {
                    b.click();
                }
            }
            "type" => {
                if let Some(a) = active {
                    let word = WORDS[pick(WORDS.len())];
                    let line = 11 + pick(3);
                    let app = h.state_mut();
                    if let Some(ed) = app.project.as_mut().and_then(|w| w.editors.get_mut(&a)) {
                        let n = line.min(ed.doc.line_count() - 1);
                        ed.doc.selection =
                            crate::editor::buffer::Selection::cursor(ed.doc.line_end(n));
                        crate::editor::commands::insert_text(&mut ed.doc, &format!(" {word}"));
                    }
                }
            }
            "save" => h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::S),
            "open" => {
                let f = FILES[pick(FILES.len())];
                let app = h.state_mut();
                let backend = &app.backend;
                if let Some(ws) = app.project.as_mut() {
                    ws.open_file(backend, f);
                }
            }
            "close" => {
                if let Some(a) = active {
                    let app = h.state_mut();
                    let (backend, settings) = (&app.backend, &app.settings);
                    if let Some(ws) = app.project.as_mut() {
                        ws.close_tab(backend, &a, settings);
                    }
                }
            }
            "layout" => h.state_mut().settings.layout = ["editor", "split", "pdf"][pick(3)].into(),
            "recompile" => h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter),
            "undo" => {
                if let Some(a) = active {
                    let app = h.state_mut();
                    if let Some(ed) = app.project.as_mut().and_then(|w| w.editors.get_mut(&a)) {
                        ed.doc.undo();
                    }
                }
            }
            "external" => {
                let f = FILES[pick(FILES.len())];
                let path = project.join(f);
                let mut text = std::fs::read_to_string(&path).unwrap();
                text.push_str(&format!("% edited outside {}\n", pick(1000)));
                std::fs::write(&path, &text).unwrap();
                // The watcher reloads it, or with unsaved edits says so.
                let settled = |h: &Harness<App>| {
                    let ws = h.state().project.as_ref().unwrap();
                    ws.editors.get(f).is_none_or(|e| e.doc.text() == text)
                        || ws.buffers.conflicts.iter().any(|c| c == f)
                };
                let start = std::time::Instant::now();
                while !settled(&h) && start.elapsed() < std::time::Duration::from_secs(10) {
                    h.run_steps(1);
                    std::thread::sleep(std::time::Duration::from_millis(30));
                }
            }
            _ => unreachable!(),
        }
        h.run_steps(4);
        std::thread::sleep(std::time::Duration::from_millis(60));
        h.run_steps(2);
        let problems = check(&h);
        if !problems.is_empty() {
            fail(
                &taken,
                format!("step {i} ({name}): {}", problems.join("; ")),
            );
        }
    }
    // Settle: answer conflicts, save, and nothing may be left that differs from the disk.
    for _ in 0..5 {
        if let Some(b) = h.query_by_label("Keep my version") {
            b.click();
        }
        h.run_steps(3);
    }
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::S);
    wait_for(&mut h, "the saves", |h| {
        h.state().project.as_ref().is_some_and(|w| !w.writing())
    });
    h.run_steps(3);
    let problems = check(&h);
    if !problems.is_empty() {
        fail(
            &taken,
            format!("after the walk settled: {}", problems.join("; ")),
        );
    }
}

#[test]
fn a_random_walk_keeps_the_invariants() {
    // Fixed seeds, so a failure replays: walk(<seed>, <steps>).
    for seed in [1, 42, 2026] {
        walk(seed, 40);
    }
}

// ── Scenarios (scripts/automation/scenarios.mjs) ──────────────────────────────────

use super::tests::{copy_dir, in_article};
use crate::editor::buffer::Selection;

fn ws<'a>(h: &'a Harness<'_, App>) -> &'a Workspace {
    h.state().project.as_ref().expect("a project is open")
}

fn ws_mut<'a>(h: &'a mut Harness<'_, App>) -> (&'a mut Workspace, &'a Backend, &'a mut Settings) {
    let app = h.state_mut();
    (
        app.project.as_mut().expect("a project is open"),
        &app.backend,
        &mut app.settings,
    )
}

/// Waits for the build that is running or about to start, and returns whether it succeeded.
fn built(h: &mut Harness<App>) -> bool {
    wait_for(h, "a build", |h| {
        ws(h).build.outcome.is_some() && !ws(h).build.building && ws(h).build.deadline.is_none()
    });
    ws(h)
        .build
        .outcome
        .as_ref()
        .is_some_and(|o| o.status == lemmata_tex::Status::Success)
}

/// Types `text` into the open file at `at` (characters), as keystrokes would.
fn type_at(h: &mut Harness<App>, at: usize, text: &str) {
    let (ws, _, _) = ws_mut(h);
    let a = ws.active.clone().unwrap();
    let ed = ws.editors.get_mut(&a).unwrap();
    ed.doc.selection = Selection::cursor(at);
    crate::editor::commands::insert_text(&mut ed.doc, text);
    h.run_steps(2);
}

fn assert_ok(h: &Harness<App>, step: &str) {
    let p = check(h);
    assert!(p.is_empty(), "{step}: {}", p.join("; "));
}

#[test]
fn the_check_catches_a_saved_file_that_differs() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((h, project)) = in_article(tmp.path()) else {
        return;
    };
    assert_ok(&h, "at the start");
    std::fs::write(project.join("main.tex"), "changed behind the app's back").unwrap();
    assert!(
        !check(&h).is_empty(),
        "the invariant sees it before the watcher reloads"
    );
}

#[test]
fn a_new_error_is_listed_and_undo_clears_it() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, _)) = in_article(tmp.path()) else {
        return;
    };
    assert!(built(&mut h));
    type_at(&mut h, 0, "\\undefinedcommandxyz\n");
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::S);
    wait_for(&mut h, "the error", |h| {
        ws(h)
            .build
            .diagnostics
            .iter()
            .any(|d| d.message.contains("undefinedcommandxyz"))
            && !ws(h).build.building
    });
    {
        let (ws, _, _) = ws_mut(&mut h);
        ws.editors.get_mut("main.tex").unwrap().doc.undo();
    }
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::S);
    wait_for(&mut h, "the fixed build", |h| {
        !ws(h).build.building
            && ws(h)
                .build
                .outcome
                .as_ref()
                .is_some_and(|o| o.status == lemmata_tex::Status::Success)
    });
    assert_ok(&h, "after undo");
}

#[test]
fn a_missing_package_is_named_and_the_logs_open() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, _)) = in_article(tmp.path()) else {
        return;
    };
    let other = tmp.path().join("missing-package");
    copy_dir(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/missing-package"),
        &other,
    );
    h.state_mut()
        .open_project(other.to_string_lossy().into_owned());
    wait_for(&mut h, "the other project", |h| {
        h.state()
            .project
            .as_ref()
            .is_some_and(|w| w.root.ends_with("missing-package"))
    });
    assert!(!built(&mut h));
    assert!(
        ws(&h)
            .build
            .diagnostics
            .iter()
            .any(|d| d.message.contains("lemmata-does-not-exist.sty"))
    );
    // No PDF, so the logs open by themselves, like Overleaf.
    h.run_steps(3);
    assert!(ws(&h).build.logs_open);
}

#[test]
fn switching_files_while_completion_is_coming_is_safe() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, _)) = in_article(tmp.path()) else {
        return;
    };
    for _ in 0..5 {
        let at = ws(&h).editors["main.tex"].doc.line_start(11);
        type_at(&mut h, at, "\\a");
        let (ws, backend, _) = ws_mut(&mut h);
        ws.open_file(backend, "refs.bib");
        h.run_steps(5);
        let (ws, backend, _) = ws_mut(&mut h);
        ws.open_file(backend, "main.tex");
        h.run_steps(2);
        let (ws, _, _) = ws_mut(&mut h);
        let ed = ws.editors.get_mut("main.tex").unwrap();
        while ed.doc.text().contains("\\a\n") || ed.doc.line(11).starts_with("\\a") {
            if !ed.doc.undo() {
                break;
            }
        }
    }
    assert_ok(&h, "after switching");
}

#[test]
fn a_resolved_comment_is_no_longer_marked() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, project)) = in_article(tmp.path()) else {
        return;
    };
    {
        let (ws, _, _) = ws_mut(&mut h);
        let ed = ws.editors.get_mut("main.tex").unwrap();
        let s = ed.doc.line_start(5);
        ed.doc.selection = Selection::single(crate::editor::buffer::Range::new(s, s + 6));
    }
    h.get_by_label("Comment").click();
    h.run_steps(3);
    h.get_by_label("Write a comment")
        .type_text("Is this the right title?");
    h.run_steps(2);
    h.query_all_by_label("Comment").last().unwrap().click();
    wait_for(&mut h, "the mark", |h| {
        !ws(h).editors["main.tex"].comments.is_empty()
    });
    let saved = std::fs::read_to_string(project.join(".lemmata/comments.json")).unwrap();
    assert!(saved.contains("Is this the right title?"));
    h.get_by_label("Resolve").click();
    wait_for(&mut h, "the mark to go", |h| {
        ws(h).editors["main.tex"].comments.is_empty()
    });
    assert_ok(&h, "after resolving");
}

#[test]
fn the_pdf_stays_after_editor_only_and_back() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, _)) = in_article(tmp.path()) else {
        return;
    };
    assert!(built(&mut h));
    wait_for(&mut h, "the PDF", |h| ws(h).pane.view.rendered());
    h.state_mut().settings.layout = "editor".into();
    h.run_steps(3);
    h.state_mut().settings.layout = "split".into();
    wait_for(&mut h, "the PDF again", |h| ws(h).pane.view.rendered());
    assert_ok(&h, "layout round trip");
}

#[test]
fn closing_the_last_tab_leaves_nothing_to_type_into() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, project)) = in_article(tmp.path()) else {
        return;
    };
    let before = std::fs::read_to_string(project.join("main.tex")).unwrap();
    while let Some(a) = ws(&h).active.clone() {
        let (ws, backend, settings) = ws_mut(&mut h);
        ws.close_tab(backend, &a, settings);
        h.run_steps(2);
    }
    assert_ok(&h, "every tab closed");
    h.event(egui::Event::Text("GHOST ".into()));
    h.run_steps(3);
    std::thread::sleep(std::time::Duration::from_millis(600));
    h.run_steps(3);
    assert_eq!(
        std::fs::read_to_string(project.join("main.tex")).unwrap(),
        before
    );
}

#[test]
fn stopping_a_build_and_saving_twice_keeps_the_edit() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, project)) = in_article(tmp.path()) else {
        return;
    };
    let end = ws(&h).editors["main.tex"].doc.len();
    type_at(&mut h, end, "\n% stop and save");
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Enter);
    h.run_steps(3);
    if ws(&h).build.building {
        let (ws, backend, _) = ws_mut(&mut h);
        ws.stop_build(backend);
    }
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::S);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::S);
    wait_for(&mut h, "the saves", |h| {
        !ws(h).writing() && ws(h).buffers.unsaved().is_empty()
    });
    assert!(
        std::fs::read_to_string(project.join("main.tex"))
            .unwrap()
            .contains("% stop and save")
    );
    assert_ok(&h, "after stopping and saving twice");
}

#[test]
fn a_pinch_zooms_the_pdf_not_the_editor() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, _)) = in_article(tmp.path()) else {
        return;
    };
    assert!(built(&mut h));
    wait_for(&mut h, "the PDF", |h| ws(h).pane.view.rendered());
    let before = ws(&h).pane.view.percent;
    let pinch = |h: &mut Harness<App>, at: egui::Pos2, k: f32| {
        h.event(egui::Event::PointerMoved(at));
        h.event(egui::Event::Zoom(k));
        h.run_steps(3);
    };
    let editor = h.get_by_label("Text of main.tex").rect().center();
    pinch(&mut h, editor, 2.0);
    assert_eq!(
        ws(&h).pane.view.percent,
        before,
        "a pinch on the editor leaves the PDF"
    );
    assert_eq!(h.ctx.zoom_factor(), 1.0, "and the window");
    // The middle of the PDF pane, at the window's right.
    let pdf = egui::pos2(
        crate::testing::WINDOW[0] - 280.0,
        crate::testing::WINDOW[1] / 2.0,
    );
    pinch(&mut h, pdf, 1.5);
    assert!(
        ws(&h).pane.view.percent > before,
        "{} → {}",
        before,
        ws(&h).pane.view.percent
    );
    // A slow pinch is many tiny steps; they add up instead of rounding away.
    let start = ws(&h).pane.view.percent;
    for _ in 0..40 {
        pinch(&mut h, pdf, 1.003);
    }
    assert!(
        ws(&h).pane.view.percent >= start + 8,
        "{start} → {}",
        ws(&h).pane.view.percent
    );
}

#[test]
fn esc_at_once_leaves_a_presentation_cleanly() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, _)) = in_article(tmp.path()) else {
        return;
    };
    assert!(built(&mut h));
    wait_for(&mut h, "the PDF", |h| ws(h).pane.view.rendered());
    let ctx = h.ctx.clone();
    ws_mut(&mut h).0.present(&ctx);
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(ws(&h).presentation.is_none(), "the presentation ended");
    assert_ok(&h, "after leaving a presentation");
}

/// Runs a palette command, as the palette or a shortcut would.
fn cmd(h: &mut Harness<App>, id: &str) {
    let ctx = h.ctx.clone();
    let (ws, backend, settings) = ws_mut(h);
    ws.run_command(&ctx, id, backend, settings);
    h.run_steps(3);
}

/// Ctrl+S, and waits until everything is written.
fn save(h: &mut Harness<App>) {
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::S);
    wait_for(h, "the saves", |h| {
        !ws(h).writing() && ws(h).buffers.unsaved().is_empty()
    });
}

fn snapshot(h: &mut Harness<App>, label: &str) {
    let (w, backend, _) = ws_mut(h);
    w.take_snapshot(backend, label.into());
    wait_for(h, "the snapshot", |h| {
        ws(h).history.snapshots.iter().any(|s| s.label == label)
    });
}

/// The session's first save takes no snapshot, so a named one right after has something to keep.
/// Waits until history is on (opening a project sets it up in the background) first.
fn no_automatic_snapshot(h: &mut Harness<App>) {
    wait_for(h, "history", |h| {
        matches!(
            ws(h).history.status,
            Some(lemmata_shell::history::HistoryStatus::On)
        )
    });
    ws_mut(h).0.history.last = Some(std::time::Instant::now());
}

/// Lets time pass with the app running (autosave, the watcher).
fn idle(h: &mut Harness<App>, ms: u64) {
    let until = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    while std::time::Instant::now() < until {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}

#[test]
fn restoring_a_snapshot_brings_no_unsaved_edits_back() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, project)) = in_article(tmp.path()) else {
        return;
    };
    let file = project.join("main.tex");
    no_automatic_snapshot(&mut h);
    type_at(&mut h, 0, "% version one\n");
    save(&mut h);
    snapshot(&mut h, "Version one");
    type_at(&mut h, 0, "% version two\n");
    save(&mut h);
    // Typed but not saved when the restore starts.
    type_at(&mut h, 0, "% not saved\n");
    cmd(&mut h, "history");
    wait_for(&mut h, "the snapshot row", |h| {
        h.query_by_label("Version one").is_some()
    });
    h.run_steps(2);
    h.get_by_label("Version one").click();
    h.run_steps(3);
    h.get_by_label("Restore project").click();
    h.run_steps(3);
    h.get_by_label("Restore").click();
    wait_for(&mut h, "the restored text", |h| {
        !ws(h).editors["main.tex"]
            .doc
            .text()
            .contains("% version two")
    });
    // Past the autosave delay: nothing may write the old edits back.
    idle(&mut h, 1500);
    let disk = std::fs::read_to_string(&file).unwrap();
    assert!(
        disk.contains("% version one")
            && !disk.contains("% version two")
            && !disk.contains("% not saved"),
        "{}",
        &disk[..120.min(disk.len())]
    );
    assert_eq!(
        ws(&h).editors["main.tex"].doc.text(),
        disk,
        "the editor shows the restored file"
    );
    assert_ok(&h, "after restoring a snapshot");
}

#[test]
fn rejecting_a_reviewed_change_undoes_it() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, project)) = in_article(tmp.path()) else {
        return;
    };
    let file = project.join("main.tex");
    no_automatic_snapshot(&mut h);
    type_at(&mut h, 0, "% draft\n");
    save(&mut h);
    snapshot(&mut h, "Draft one");
    let end = ws(&h).editors["main.tex"].doc.len();
    type_at(&mut h, end, "\n% a change to undo");
    save(&mut h);
    cmd(&mut h, "history");
    let id = ws(&h)
        .history
        .snapshots
        .iter()
        .find(|s| s.label == "Draft one")
        .unwrap()
        .id
        .clone();
    // What choosing it in "Review changes since" sets.
    ws_mut(&mut h).0.history.since = id;
    wait_for(&mut h, "Review changes", |h| {
        h.query_by_label("Review changes").is_some()
    });
    h.run_steps(2);
    h.get_by_label("Review changes").click();
    wait_for(&mut h, "the change to review", |h| {
        h.query_by_label("Reject").is_some()
    });
    h.run_steps(2);
    h.get_by_label("Reject").click();
    wait_for(&mut h, "the change undone", |_| {
        !std::fs::read_to_string(&file)
            .unwrap()
            .contains("% a change to undo")
    });
    assert!(
        !ws(&h).editors["main.tex"]
            .doc
            .text()
            .contains("% a change to undo"),
        "the editor still shows it"
    );
    assert_ok(&h, "after rejecting a change");
}

/// A gray PNG of this size.
fn gray_png(path: &std::path::Path, w: u32, h: u32) {
    image::GrayImage::from_pixel(w, h, image::Luma([128]))
        .save(path)
        .unwrap();
}

#[test]
fn a_very_large_image_is_drawn_after_another_large_one() {
    // The WebKitGTK of Ubuntu 22.04 drew the second blank: 6 then 25 megapixels.
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, _)) = in_article(tmp.path()) else {
        return;
    };
    let big = tmp.path().join("big-images");
    std::fs::create_dir_all(&big).unwrap();
    gray_png(&big.join("wide.png"), 4488, 1400);
    gray_png(&big.join("tall.png"), 4912, 5208);
    std::fs::write(
        big.join("main.tex"),
        "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\n\\noindent\\includegraphics[width=\\linewidth]{wide.png}\n\\newpage\n\\noindent\\includegraphics[width=\\linewidth]{tall.png}\n\\end{document}\n",
    )
    .unwrap();
    h.state_mut()
        .open_project(big.to_string_lossy().into_owned());
    wait_for(&mut h, "the other project", |h| {
        h.state()
            .project
            .as_ref()
            .is_some_and(|w| w.root.ends_with("big-images"))
    });
    assert!(built(&mut h));
    wait_for(&mut h, "two pages", |h| ws(h).pane.view.page_count() == 2);
    ws_mut(&mut h).0.pane.view.goto_page(2);
    idle(&mut h, 600);
    wait_for(&mut h, "page 2 drawn", |h| ws(h).pane.view.rendered());
    // Page 2's top is at the pane's top: below its margin the image is gray, not white.
    let img = h.render().unwrap();
    let (x, y0) = (
        (crate::testing::WINDOW[0] - 280.0) as u32,
        (crate::testing::WINDOW[1] * 0.3) as u32,
    );
    let gray = (0..40)
        .filter(|k| (100..160).contains(&img.get_pixel(x, y0 + k * 5).0[0]))
        .count();
    assert!(gray > 30, "page 2 shows {gray}/40 gray pixels");
}

#[test]
fn every_side_panel_shows_in_every_layout() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, _)) = in_article(tmp.path()) else {
        return;
    };
    let shown = |h: &Harness<App>, close: &str| {
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, crate::testing::WINDOW.into());
        h.query_by_label(close)
            .is_some_and(|n| window.contains_rect(n.rect()) && n.rect().width() > 0.0)
    };
    for layout in ["editor", "split", "pdf", "focus"] {
        for (id, close) in [
            ("git", "Close Git"),
            ("todos", "Close TODOs"),
            ("comments", "Close comments"),
            ("history", "Close history"),
        ] {
            h.state_mut().settings.layout = if layout == "focus" { "split" } else { layout }.into();
            h.run_steps(2);
            // Opening a panel leaves focus mode, so this turns it on again each time.
            if layout == "focus" && !ws(&h).focus_mode {
                cmd(&mut h, "focus");
            }
            cmd(&mut h, id);
            wait_for(&mut h, &format!("{id} in {layout}"), |h| shown(h, close));
        }
    }
    h.get_by_label("Close history").click();
    // From "PDF only", revealing the open file brings the tree back.
    h.state_mut().settings.layout = "pdf".into();
    h.run_steps(2);
    cmd(&mut h, "reveal");
    wait_for(&mut h, "the file tree", |h| {
        h.query_by_role_and_label(egui::accesskit::Role::TreeItem, "main.tex")
            .is_some()
    });
    assert_ok(&h, "after opening the panels");
}

#[test]
fn controls_are_where_people_look() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, _)) = in_article(tmp.path()) else {
        return;
    };
    assert!(built(&mut h));
    assert!(
        h.query_by_label("Dark pages").is_none(),
        "no dark-pages button in the PDF toolbar"
    );
    assert!(
        h.query_by_label("Git").is_some(),
        "a Git button in the toolbar"
    );
    // The status bar's word count opens the breakdown.
    wait_for(&mut h, "the word count", |h| {
        h.query_by_label_contains(" words").is_some()
    });
    h.get_by_label_contains(" words").click();
    wait_for(&mut h, "the breakdown", |h| ws(h).words.is_some());
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    // At 560 px the PDF toolbar's extras are in ⋯, Present among them.
    if ws(&h).build.logs_open {
        ws_mut(&mut h).0.build.logs_open = false;
    }
    h.state_mut().settings.pdf_width = 560.0;
    wait_for(&mut h, "the ⋯ menu", |h| {
        h.query_by_label("More").is_some()
    });
    h.get_by_label("More").click();
    wait_for(&mut h, "Present in ⋯", |h| {
        h.query_by_label("Present full screen").is_some()
    });
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
}

#[test]
fn a_deleted_file_comes_back_from_recently_deleted_after_reopening() {
    let tmp = tempfile::tempdir().unwrap();
    let settings = Settings {
        trash_mode: "project".into(),
        ..Settings::default()
    };
    let Some((mut h, project)) = in_article_with(tmp.path(), settings) else {
        return;
    };
    let gone = project.join("gone.tex");
    std::fs::write(&gone, "Keep me.\n").unwrap();
    wait_for(&mut h, "gone.tex in the tree", |h| {
        h.query_by_role_and_label(egui::accesskit::Role::TreeItem, "gone.tex")
            .is_some()
    });
    {
        let (ws, backend, settings) = ws_mut(&mut h);
        ws.trash(backend, settings, vec!["gone.tex".into()]);
    }
    wait_for(&mut h, "the file deleted", |_| !gone.exists());
    // Reopening forgets the undo log, like a restart.
    let before = ws(&h).project;
    h.state_mut()
        .open_project(project.to_string_lossy().into_owned());
    wait_for(&mut h, "the project again", |h| {
        h.state()
            .project
            .as_ref()
            .is_some_and(|w| w.project != before && !w.tabs.is_empty())
    });
    cmd(&mut h, "deleted");
    wait_for(&mut h, "the list", |h| {
        h.query_by_label_contains("gone.tex").is_some()
    });
    h.run_steps(3);
    h.get_by_label("Restore").click();
    wait_for(&mut h, "the file back", |_| gone.exists());
    assert_eq!(std::fs::read_to_string(&gone).unwrap(), "Keep me.\n");
}

#[test]
fn an_unknown_citation_or_label_offers_a_search() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, _)) = in_article(tmp.path()) else {
        return;
    };
    {
        let (ws, backend, _) = ws_mut(&mut h);
        ws.open_file(backend, "chapters/method.tex");
    }
    wait_for(&mut h, "method.tex", |h| {
        ws(h)
            .editors
            .get("chapters/method.tex")
            .is_some_and(|e| e.doc.text().contains("nobody2020"))
    });
    h.run_steps(3);
    // The card as resting the pointer on the name shows it.
    let hover = |h: &mut Harness<App>, name: &str| {
        let ed = ws_mut(h).0.editors.get_mut("chapters/method.tex").unwrap();
        let n = (0..ed.doc.line_count())
            .find(|&n| ed.doc.line(n).contains(name))
            .unwrap();
        let line = ed.doc.line(n);
        let at = line[..line.find(name).unwrap()].chars().count() + 1;
        let t = crate::editor::hover::target_at(&line, at).unwrap();
        // Anywhere counts as on the card until it is drawn; then the pointer rests on it.
        ed.hover = Some((n, t, egui::Rect::EVERYTHING));
        h.event(egui::Event::PointerMoved(egui::pos2(5.0, 5.0)));
        h.run_steps(1);
        let card = ws(h).editors["chapters/method.tex"]
            .hover
            .as_ref()
            .unwrap()
            .2;
        h.event(egui::Event::PointerMoved(card.center()));
        h.run_steps(2);
    };
    hover(&mut h, "nobody2020");
    h.get_by_label("Search online").click();
    wait_for(&mut h, "the citation picker", |h| ws(h).cite.is_some());
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    hover(&mut h, "sec:missing");
    h.get_by_label("Search the project").click();
    wait_for(&mut h, "the search for it", |h| {
        ws(h).search.is_some()
            && h.query_all_by_label("Search the project")
                .any(|n| n.value().as_deref() == Some("sec:missing"))
    });
}

#[test]
fn projects_go_into_folders_by_dragging_or_from_their_menu() {
    let tmp = tempfile::tempdir().unwrap();
    let Some(mut h) = super::tests::in_library(tmp.path()) else {
        return;
    };
    let paper = tmp.path().join("paper").to_string_lossy().into_owned();
    h.state_mut().settings.library_view = "list".into();
    h.get_by_label("New folder").click();
    wait_for(&mut h, "the dialog", |h| {
        h.query_by_label("Folder name").is_some()
    });
    h.run_steps(3);
    h.get_by_label("Folder name").type_text("Course");
    h.run_steps(1);
    h.get_by_label("Create").click();
    wait_for(&mut h, "the folder", |h| {
        h.query_by_label("Course").is_some()
    });
    h.run_steps(2);
    let folder = |h: &Harness<App>| h.state().groups.members.get(&paper).cloned();
    // Drag the project's row onto the folder's.
    let from = h
        .get_by_role_and_label(egui::accesskit::Role::Row, "paper")
        .rect()
        .center();
    let to = h
        .get_by_role_and_label(egui::accesskit::Role::Row, "Course")
        .rect()
        .center();
    let press = |h: &mut Harness<App>, pos: egui::Pos2, pressed: bool| {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    };
    h.event(egui::Event::PointerMoved(from));
    press(&mut h, from, true);
    h.run_steps(1);
    for k in 1..=8 {
        h.event(egui::Event::PointerMoved(
            from + (to - from) * (k as f32 / 8.0),
        ));
        h.run_steps(1);
    }
    press(&mut h, to, false);
    wait_for(&mut h, "the dragged project in the folder", |h| {
        folder(h).is_some()
    });
    wait_for(&mut h, "the project only in its folder", |h| {
        h.query_by_role_and_label(egui::accesskit::Role::Row, "paper")
            .is_none()
    });
    // Out again from its menu, then back in the same way.
    h.state_mut().groups.move_project(&paper, None);
    h.run_steps(3);
    h.get_by_label("Actions for paper").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Move to folder…")
        .click();
    wait_for(&mut h, "the move dialog", |h| {
        h.query_by_role_and_label(egui::accesskit::Role::RadioButton, "Course")
            .is_some()
    });
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::RadioButton, "Course")
        .click();
    wait_for(&mut h, "the moved project in the folder", |h| {
        folder(h).is_some()
    });
    // Deleting the folder keeps its project.
    h.get_by_label("Actions for folder Course").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Delete folder")
        .click();
    wait_for(&mut h, "the folder gone, its project back", |h| {
        h.state().groups.groups.is_empty()
            && h.query_by_role_and_label(egui::accesskit::Role::Row, "paper")
                .is_some()
    });
}

#[test]
fn a_project_is_deleted_from_the_start_screen() {
    let tmp = tempfile::tempdir().unwrap();
    let Some(mut h) = super::tests::in_library(tmp.path()) else {
        return;
    };
    let paper = tmp.path().join("paper");
    h.get_by_label("Actions for paper").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Delete project…")
        .click();
    wait_for(&mut h, "the confirmation", |h| {
        h.query_by_label("Move to trash").is_some()
    });
    h.run_steps(3);
    h.get_by_label("Move to trash").click();
    wait_for(&mut h, "the project gone", |_| !paper.exists());
    wait_for(&mut h, "the list without it", |h| {
        h.query_by_role_and_label(egui::accesskit::Role::Button, "paper")
            .is_none()
    });
}

/// "Commit 1 file", "Commit 3 files": there is something staged.
fn commit_button<'a>(h: &'a Harness<'_, App>) -> Option<egui_kittest::Node<'a>> {
    h.query_all_by_label_contains("Commit ").find(|n| {
        egui_kittest::kittest::NodeT::accesskit_node(n)
            .label()
            .is_some_and(|l| l.ends_with(" file") || l.ends_with(" files"))
    })
}

#[test]
fn a_saved_edit_is_committed_from_the_git_panel() {
    let tmp = tempfile::tempdir().unwrap();
    let Some((mut h, project)) = in_article(tmp.path()) else {
        return;
    };
    // Commits need a name and an email; without them git refuses, and so does this test.
    let who = std::process::Command::new("git")
        .args(["config", "user.email"])
        .current_dir(&project)
        .output();
    if !who.is_ok_and(|o| o.status.success()) {
        eprintln!("skipped: git has no user.email here");
        return;
    }
    cmd(&mut h, "git");
    wait_for(&mut h, "the git overview", |h| {
        h.query_by_label("Turn this project into a git repository")
            .is_some()
    });
    h.get_by_label("Turn this project into a git repository")
        .click();
    wait_for(&mut h, "the repository", |h| {
        h.query_by_label("Commit message").is_some()
    });
    type_at(&mut h, 0, "% committed from the panel\n");
    save(&mut h);
    wait_for(&mut h, "the saved edit in the panel", |h| {
        h.query_by_label("Stage all").is_some()
    });
    h.get_by_label("Stage all").click();
    wait_for(&mut h, "the staged files", |h| commit_button(h).is_some());
    h.get_by_label("Commit message").click();
    h.get_by_label("Commit message")
        .type_text("Edit from the panel");
    h.run_steps(2);
    commit_button(&h).unwrap().click();
    wait_for(&mut h, "the commit", |h| {
        h.query_by_label("No changes. Everything is committed.")
            .is_some()
    });
    let log = std::process::Command::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(&project)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&log.stdout).trim(),
        "Edit from the panel"
    );
    assert_ok(&h, "after committing");
}
