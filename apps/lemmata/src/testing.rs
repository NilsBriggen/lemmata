//! Shared test setup: the app's fonts and theme in a headless wgpu harness at the window's size.

use crate::theme::{self, Palette};
use eframe::egui;
use egui_kittest::Harness;

/// The default window size, the size the Svelte references were captured at.
pub const WINDOW: [f32; 2] = [1400.0, 900.0];

/// The closure gets the whole window, as eframe's root `Ui` does (kittest's own has a margin).
pub fn harness<'a>(p: Palette, mut ui: impl FnMut(&mut egui::Ui) + 'a) -> Harness<'a> {
    Harness::builder()
        .with_max_steps(256)
        .with_size(WINDOW)
        .with_context_setup(move |ctx| {
            ctx.add_font_provider(std::sync::Arc::new(
                egui_system_font_provider::SystemFontProvider::new(),
            ));
            theme::install_fonts(ctx);
            theme::apply(ctx, &p);
        })
        .wgpu()
        .build_ui(move |outer| {
            let rect = outer.ctx().content_rect();
            let mut child = outer.new_child(egui::UiBuilder::new().max_rect(rect));
            ui(&mut child);
        })
}

/// TeX Live 2026 in /usr/bin with every tool, as the mock backend reports it.
pub fn detection() -> lemmata_tex::detect::Detection {
    use lemmata_tex::detect::{Detection, Distribution, Tool};
    let tools = [
        "latexmk", "pdflatex", "synctex", "biber", "bibtex", "xelatex", "lualatex",
    ]
    .iter()
    .enumerate()
    .map(|(i, n)| Tool {
        name: (*n).into(),
        path: Some(format!("/usr/bin/{n}").into()),
        required: i < 2,
    })
    .collect();
    Detection {
        bin_dir: Some("/usr/bin".into()),
        distribution: Distribution::TexLive { year: Some(2026) },
        tools,
        ready: true,
        tex_ready: true,
        tectonic: None,
        outdated: false,
    }
}

/// The mock's recent projects, with rl-thesis just opened.
pub fn recents(now: f64) -> crate::store::Recents {
    crate::store::Recents(
        [
            ("/home/you/Documents/papers/rl-thesis", 0.0),
            ("/home/you/Documents/uni/CA_Assignement_4", 86400e3),
            ("/home/you/Documents/uni/NET_assignment_4", 3.0 * 86400e3),
            ("/home/you/Documents/cv", 20.0 * 86400e3),
        ]
        .into_iter()
        .map(|(path, back)| crate::store::Recent {
            path: path.into(),
            last_opened: now - back,
        })
        .collect(),
    )
}

/// Draws the library as the screen behind a dialog.
pub fn library_behind(ui: &mut egui::Ui, p: &Palette) {
    use crate::screens::library;
    let now = 1_800_000_000_000.0;
    let recents = recents(now);
    let mut groups = crate::store::Groups::default();
    let mut settings = crate::store::Settings::default();
    static STATE: std::sync::OnceLock<std::sync::Mutex<library::State>> =
        std::sync::OnceLock::new();
    let mut st = STATE
        .get_or_init(|| std::sync::Mutex::new(library::State::default()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut c = library::Ctx {
        recents: &recents,
        groups: &mut groups,
        settings: &mut settings,
        error: None,
        no_main_in: None,
        thumbnail: std::sync::Arc::new(|_| None),
        now_ms: now,
    };
    library::show(ui, p, &mut st, &mut c);
}

// ── P0 Proactive UI Oracles (CPU-only egui_kittest, zero wgpu overhead) ───────

/// Breakpoint & degenerate viewport matrix for proactive geometry sweeps.
pub const VIEWPORT_MATRIX: &[[f32; 2]] = &[
    [1400.0, 900.0], // desktop reference
    [1180.0, 740.0], // presenter default
    [1059.0, 700.0], // just below presenter_footer compact (1060)
    [900.0, 560.0],  // compact workspace
    [766.0, 600.0],  // presenter_header ETA/center boundary
    [760.0, 600.0],  // exact presenter_header compact threshold
    [759.0, 600.0],  // just below presenter_header compact
    [600.0, 500.0],  // narrow compact overlap probe
    [350.0, 350.0],  // short-height notes_card probe
    [260.0, 260.0],  // below popover widths (272 / 294) -> clamp panic probe
    [35.0, 35.0],    // degenerate tiny pane (< 40px HUD, < 48px viewer)
];

/// CPU-only harness (never initializes wgpu) for fast proactive layout & interaction sweeps.
pub fn cpu_harness_state<'a, S>(
    size: [f32; 2],
    p: Palette,
    mut ui: impl FnMut(&mut egui::Ui, &mut S) + 'a,
    state: S,
) -> Harness<'a, S> {
    Harness::builder()
        .with_max_steps(128)
        .with_size(size)
        .with_context_setup(move |ctx| {
            ctx.add_font_provider(std::sync::Arc::new(
                egui_system_font_provider::SystemFontProvider::new(),
            ));
            theme::install_fonts(ctx);
            theme::apply(ctx, &p);
        })
        .build_ui_state(
            move |outer, st| {
                let rect = outer.ctx().content_rect();
                let mut child = outer.new_child(egui::UiBuilder::new().max_rect(rect));
                ui(&mut child, st);
            },
            state,
        )
}

/// Advances egui's simulated clock deterministically without sleeping, then steps one frame.
pub fn advance_time_and_step<S>(h: &mut Harness<'_, S>, dt_secs: f64) {
    let now = h.ctx.input(|i| i.time);
    h.input_mut().time = Some(now + dt_secs);
    h.step();
}

/// Runs the harness until egui settles (`repaint_delay != 0`), failing immediately on infinite
/// repaint / cache-eviction / hover-oscillation loops.
pub fn assert_settles<S>(h: &mut Harness<'_, S>, context: &str) {
    if let Err(err) = h.try_run() {
        panic!("UI failed to settle ({context}): {err}");
    }
}

/// Checks universal geometry invariants over the rendered `FullOutput` shapes and AccessKit tree:
/// - Every clip rect and painted shape bounding box has finite, non-negative dimensions.
/// - Every visible AccessKit node has finite, non-negative bounds.
pub fn assert_ui_geometry<S>(h: &Harness<'_, S>, context: &str) {
    use egui_kittest::kittest::NodeT;

    for cs in &h.output().shapes {
        let cr = cs.clip_rect;
        assert!(
            cr.min.x.is_finite()
                && cr.min.y.is_finite()
                && cr.max.x.is_finite()
                && cr.max.y.is_finite(),
            "non-finite clip_rect {cr:?} ({context})"
        );
        let bb = cs.shape.visual_bounding_rect();
        if bb.is_finite() {
            assert!(
                bb.width() >= -0.01 && bb.height() >= -0.01,
                "negative visual bounding rect {bb:?} ({context})"
            );
        }
    }

    fn walk<'a>(node: &egui_kittest::Node<'a>, context: &str) {
        if let Some(b) = node.accesskit_node().raw_bounds() {
            assert!(
                b.x0.is_finite()
                    && b.y0.is_finite()
                    && b.x1.is_finite()
                    && b.y1.is_finite()
                    && b.x1 >= b.x0 - 0.5
                    && b.y1 >= b.y0 - 0.5,
                "invalid AccessKit bounds {b:?} on {:?} ({context})",
                node.accesskit_node().label()
            );
        }
        for child in node.children() {
            walk(&child, context);
        }
    }
    walk(&h.root(), context);
}

/// Asserts that time-dependent UI scheduled a future repaint (`repaint_delay < Duration::MAX`).
pub fn assert_timer_schedules_repaint<S>(h: &Harness<'_, S>, context: &str) {
    let min_delay = h
        .output()
        .viewport_output
        .values()
        .map(|v| v.repaint_delay)
        .min()
        .unwrap_or(std::time::Duration::MAX);
    assert!(
        min_delay < std::time::Duration::MAX,
        "expected time-dependent UI to schedule a future repaint, got Duration::MAX ({context})"
    );
}
