//! The panels that take the PDF's place: review comments, TODOs, history and Git.

pub mod hunks;
pub mod review;

use crate::icons;
use crate::theme::{self, Palette};
use crate::widgets::{Button, Size};
use eframe::egui::{self, FontId, Rect, Stroke, Ui, pos2, vec2};

pub struct Bar {
    /// The close button was clicked.
    pub closed: bool,
    /// The panel's area under the bar.
    pub rest: Rect,
}

/// `.bar`: 40 px on --surface with a bottom border; the title in 600, then `extra`, and a close
/// button at the right.
pub fn bar(
    ui: &mut Ui,
    p: &Palette,
    title: &str,
    close_label: &str,
    extra: &mut dyn FnMut(&mut Ui),
) -> Bar {
    let rect = ui.max_rect();
    let r = Rect::from_min_size(rect.min, vec2(rect.width(), 40.0));
    ui.painter().rect_filled(r, 0, p.surface);
    ui.painter()
        .hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.border));
    let mut closed = false;
    let mut c = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                pos2(r.left() + 14.0, r.top()),
                pos2(r.right() - 8.0, r.bottom()),
            ))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    if Button::icon(icons::X, close_label)
        .size(Size::Sm)
        .icon_size(15.0)
        .show_icon(&mut c, p)
        .clicked()
    {
        closed = true;
    }
    c.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(
            egui::RichText::new(title)
                .font(FontId::new(13.0, theme::font::sans_600()))
                .color(p.ink),
        );
        ui.add_space(6.0);
        extra(ui);
    });
    Bar {
        closed,
        rest: Rect::from_min_max(pos2(rect.left(), r.bottom()), rect.max),
    }
}
