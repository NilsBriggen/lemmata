//! Shared test setup: the app's fonts and theme in a headless wgpu harness at the window's size.

use crate::theme::{self, Palette};
use eframe::egui;
use egui_kittest::Harness;

/// The default window size, the size the Svelte references were captured at.
pub const WINDOW: [f32; 2] = [1400.0, 900.0];

/// The closure gets the whole window, as eframe's root `Ui` does (kittest's own has a margin).
pub fn harness<'a>(p: Palette, mut ui: impl FnMut(&mut egui::Ui) + 'a) -> Harness<'a> {
    Harness::builder()
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
