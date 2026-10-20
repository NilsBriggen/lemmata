use crate::theme::{self, Palette};
use crate::widgets;
use eframe::egui::{self, FontId};

pub struct App {
    palette: Palette,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let dark = cc.egui_ctx.system_theme() == Some(egui::Theme::Dark);
        let palette = if dark { theme::DARK } else { theme::LIGHT };
        theme::apply(&cc.egui_ctx, &palette);
        Self { palette }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let p = self.palette;
        // The boot splash of index.html: the wordmark in Young Serif 40 px, accent on --app.
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, p.app);
        widgets::centered_text(
            ui,
            rect,
            "Lemmata",
            FontId::new(40.0, theme::font::display()),
            p.accent,
        );
    }
}
