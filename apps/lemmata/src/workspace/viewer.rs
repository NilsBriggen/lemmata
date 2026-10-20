//! FileViewer.svelte: images and PDFs of the project open read-only in a tab.

use crate::pdf::view::PdfView;
use crate::theme::Palette;
use eframe::egui::{
    self, Color32, ColorImage, FontId, Rect, Sense, Shadow, Stroke, TextureHandle, Ui, pos2, vec2,
};

pub enum Viewer {
    Loading,
    Error(String),
    Pdf(Box<PdfView>),
    Image {
        image: Option<ColorImage>,
        tex: Option<TextureHandle>,
        size: [usize; 2],
        /// Shown at 100 % instead of fitted; a click switches.
        actual: bool,
    },
}

/// Decodes an image file, off the UI thread.
pub fn decode(path: &str, bytes: &[u8]) -> Result<ColorImage, String> {
    if path.to_lowercase().ends_with(".svg") {
        // shortcut: SVG needs a vector renderer (resvg, about 1 MB); not decided yet, so it isn't previewed.
        return Err("SVG files can't be previewed here yet. Open them in another program.".into());
    }
    let img = image::load_from_memory(bytes).map_err(|e| format!("Could not open {path}: {e}"))?;
    let rgba = img.to_rgba8();
    Ok(ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        &rgba,
    ))
}

pub fn show(ui: &mut Ui, p: &Palette, path: &str, v: &mut Viewer) {
    let area = ui.max_rect();
    ui.painter().rect_filled(area, 0, p.pdf_ground);
    let msg = |ui: &mut Ui, text: &str| {
        ui.painter().text(
            area.min + vec2(24.0, 24.0),
            egui::Align2::LEFT_TOP,
            text,
            FontId::proportional(13.0),
            p.muted,
        );
    };
    match v {
        Viewer::Loading => msg(ui, "Loading…"),
        Viewer::Error(e) => msg(ui, e),
        Viewer::Pdf(view) => {
            // A PDF of the project: no SyncTeX.
            let _ = view.show(ui, p);
        }
        Viewer::Image {
            image,
            tex,
            size,
            actual,
        } => {
            let tex = tex.get_or_insert_with(|| {
                ui.ctx().load_texture(
                    format!("viewer-{path}"),
                    image.take().unwrap_or_default(),
                    egui::TextureOptions::LINEAR,
                )
            });
            let natural = vec2(size[0] as f32, size[1] as f32) / ui.ctx().pixels_per_point();
            let stage = area.shrink(24.0);
            let fitted = {
                let k = (stage.width() / natural.x)
                    .min(stage.height() / natural.y)
                    .min(1.0);
                natural * k
            };
            let mut clicked = false;
            let mut draw = |ui: &mut Ui, rect: Rect| {
                let painter = ui.painter();
                painter.add(
                    Shadow {
                        offset: [0, 6],
                        blur: 18,
                        spread: 0,
                        color: Color32::from_black_alpha(40),
                    }
                    .as_shape(rect, 0),
                );
                painter.add(
                    Shadow {
                        offset: [0, 1],
                        blur: 3,
                        spread: 0,
                        color: Color32::from_black_alpha(46),
                    }
                    .as_shape(rect, 0),
                );
                checkerboard(painter, rect);
                painter.image(
                    tex.id(),
                    rect,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                let r = ui.interact(rect, ui.scope_id().with("image"), Sense::click());
                r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, path));
                if r.on_hover_cursor(if *actual {
                    egui::CursorIcon::ZoomOut
                } else {
                    egui::CursorIcon::ZoomIn
                })
                .clicked()
                {
                    clicked = true;
                }
            };
            if *actual {
                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(area));
                egui::ScrollArea::both()
                    .id_salt(("viewer", path))
                    .auto_shrink([false, false])
                    .show(&mut c, |ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(natural + vec2(48.0, 48.0), Sense::hover());
                        draw(
                            ui,
                            Rect::from_min_size(rect.min + vec2(24.0, 24.0), natural),
                        );
                    });
            } else {
                draw(ui, Rect::from_center_size(stage.center(), fitted));
            }
            if clicked {
                *actual = !*actual;
            }
            // .info
            let name = path.rsplit('/').next().unwrap_or(path);
            let text = format!(
                "{name} · {} × {} px · click to {}",
                size[0],
                size[1],
                if *actual { "fit" } else { "view at 100%" }
            );
            let g = ui
                .painter()
                .layout_no_wrap(text, FontId::proportional(12.0), p.muted);
            let r = Rect::from_min_size(
                pos2(area.left() + 12.0, area.bottom() - 10.0 - g.size().y - 6.0),
                g.size() + vec2(18.0, 6.0),
            );
            ui.painter().rect(
                r,
                crate::theme::R_SM,
                p.surface,
                Stroke::new(1.0, p.border),
                egui::StrokeKind::Inside,
            );
            ui.painter().galley(r.min + vec2(9.0, 3.0), g, p.muted);
        }
    }
}

/// The 16 px grey-and-white checkerboard behind transparent images.
fn checkerboard(painter: &egui::Painter, rect: Rect) {
    painter.rect_filled(rect, 0, Color32::WHITE);
    let c = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let grey = Color32::from_rgb(0xec, 0xec, 0xec);
    let mut y = rect.top();
    let mut row = 0;
    while y < rect.bottom() {
        let mut x = rect.left() + if row % 2 == 0 { 8.0 } else { 0.0 };
        while x < rect.right() {
            c.rect_filled(Rect::from_min_size(pos2(x, y), vec2(8.0, 8.0)), 0, grey);
            x += 16.0;
        }
        y += 8.0;
        row += 1;
    }
}
