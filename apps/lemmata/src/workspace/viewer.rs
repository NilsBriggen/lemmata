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
        return decode_svg(path, bytes);
    }
    let img = image::load_from_memory(bytes).map_err(|e| format!("Could not open {path}: {e}"))?;
    let rgba = img.to_rgba8();
    Ok(ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        &rgba,
    ))
}

fn decode_svg(path: &str, bytes: &[u8]) -> Result<ColorImage, String> {
    const MAX_BYTES: usize = 8_000_000;
    const MAX_PIXELS: usize = 4_000_000;
    if bytes.len() > MAX_BYTES {
        return Err(format!("Could not open {path}: SVG is too large"));
    }
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return Err(format!(
            "Could not open {path}: compressed SVG is not supported"
        ));
    }
    let mut options = resvg::usvg::Options {
        font_family: "IBM Plex Sans".into(),
        ..Default::default()
    };
    std::sync::Arc::make_mut(&mut options.fontdb)
        .load_font_data(include_bytes!("../../assets/fonts/ibm-plex-sans-latin-400.ttf").to_vec());
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    let tree = resvg::usvg::Tree::from_data(bytes, &options)
        .map_err(|e| format!("Could not open {path}: {e}"))?;
    let size = tree.size().to_int_size();
    let (w, h) = (size.width() as usize, size.height() as usize);
    if w == 0 || h == 0 || w.checked_mul(h).is_none_or(|n| n > MAX_PIXELS) {
        return Err(format!(
            "Could not open {path}: SVG dimensions are too large"
        ));
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or_else(|| format!("Could not open {path}: SVG dimensions are too large"))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    Ok(ColorImage::from_rgba_premultiplied([w, h], pixmap.data()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn svg_paths_text_and_transparency_render_offline() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><path fill="#ff0000" d="M0 0h20v10H0z"/><text x="2" y="8" fill="#000000">A</text></svg>"##;
        let img = decode("figure.svg", svg).unwrap();
        assert_eq!(img.size, [20, 10]);
        assert_eq!(img.pixels[0], Color32::RED);
        let transparent = br##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><path fill="#ff0000" d="M0 0h2v2H0z"/></svg>"##;
        let img = decode("transparent.svg", transparent).unwrap();
        assert_eq!(img.pixels[0], Color32::RED);
        assert_eq!(img.pixels[15], Color32::TRANSPARENT);
    }

    #[test]
    fn svg_text_uses_bundled_fonts() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><text x="1" y="16" font-size="16" fill="#000000">A</text></svg>"##;
        let img = decode("text.svg", svg).unwrap();
        assert!(img.pixels.iter().any(|p| p.a() > 0), "text did not render");
    }

    #[test]
    fn gzip_svg_is_rejected_before_decompression() {
        let bytes = [
            31, 139, 8, 0, 0, 0, 0, 0, 2, 255, 21, 204, 49, 14, 131, 48, 12, 64, 209, 171, 88, 238,
            142, 173, 148, 9, 37, 153, 187, 244, 16, 72, 144, 56, 82, 104, 81, 99, 97, 196, 233,
            75, 182, 63, 60, 125, 223, 142, 12, 231, 86, 63, 45, 160, 168, 238, 19, 145, 153, 13,
            246, 28, 190, 191, 76, 142, 153, 233, 22, 8, 86, 22, 149, 128, 35, 130, 172, 37, 139,
            246, 140, 126, 159, 85, 32, 149, 90, 3, 62, 82, 186, 49, 35, 44, 1, 223, 12, 44, 238,
            112, 47, 190, 144, 162, 239, 135, 248, 7, 13, 82, 69, 78, 105, 0, 0, 0,
        ];
        assert!(decode("bomb.svg", &bytes).is_err());
    }

    #[test]
    fn svg_embedded_png_is_allowed_but_external_hrefs_are_not_read() {
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let data = base64::engine::general_purpose::STANDARD.encode(png);
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="4" height="4"><image xlink:href="data:image/png;base64,{data}" width="4" height="4"/></svg>"#
        );
        let img = decode("embedded.svg", svg.as_bytes()).unwrap();
        assert_eq!(img.pixels[0], Color32::RED);
        for href in ["file:///etc/passwd", "https://example.com/spy.png"] {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="4" height="4"><image xlink:href="{href}" width="4" height="4"/></svg>"#
            );
            let img = decode("external.svg", svg.as_bytes()).unwrap();
            assert!(img.pixels.iter().all(|p| *p == Color32::TRANSPARENT));
        }
    }

    #[test]
    fn svg_malformed_xml_and_excessive_dimensions_are_rejected() {
        assert!(decode("broken.svg", b"<svg><path></svg>").is_err());
        let huge = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1000000000" height="1000000000"><path d="M0 0h1v1H0z"/></svg>"#;
        assert!(decode("huge.svg", huge).is_err());
    }
}
