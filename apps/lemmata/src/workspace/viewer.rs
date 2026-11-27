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

/// Longest side a texture may have (wgpu's default limit): egui refuses bigger ones.
const MAX_SIDE: u32 = 8192;

/// Decodes an image file, off the UI thread.
pub fn decode(path: &str, bytes: &[u8]) -> Result<ColorImage, String> {
    decode_sized(path, bytes).map(|(image, _)| image)
}

/// `decode`, and the picture's own size in pixels: the texture may be a scaled-down copy.
pub fn decode_sized(path: &str, bytes: &[u8]) -> Result<(ColorImage, [usize; 2]), String> {
    if path.to_lowercase().ends_with(".svg") {
        return decode_svg(path, bytes).map(|i| {
            let size = i.size;
            (i, size)
        });
    }
    let bad = |e: image::ImageError| format!("Could not open {path}: {e}");
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("Could not open {path}: {e}"))?;
    // A file can declare any size; past this it isn't a figure but a way to fill memory.
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(40_000);
    limits.max_image_height = Some(40_000);
    limits.max_alloc = Some(512 << 20);
    reader.limits(limits);
    use image::ImageDecoder;
    let mut decoder = reader.into_decoder().map_err(bad)?;
    // A phone's portrait photo is stored sideways, with a tag that says how to turn it.
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder).map_err(bad)?;
    img.apply_orientation(orientation);
    let size = [img.width() as usize, img.height() as usize];
    // Textures are at most MAX_SIDE a side: a bigger image is shown scaled down.
    let img = if img.width() > MAX_SIDE || img.height() > MAX_SIDE {
        img.thumbnail(MAX_SIDE, MAX_SIDE)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    Ok((
        ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], &rgba),
        size,
    ))
}

fn svg_fontdb() -> std::sync::Arc<resvg::usvg::fontdb::Database> {
    static DB: std::sync::OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> =
        std::sync::OnceLock::new();
    DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_font_data(crate::theme::IBM_PLEX_SANS_400.to_vec());
        std::sync::Arc::new(db)
    })
    .clone()
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
        fontdb: svg_fontdb(),
        ..Default::default()
    };
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    let tree = resvg::usvg::Tree::from_data(bytes, &options)
        .map_err(|e| format!("Could not open {path}: {e}"))?;
    let size = tree.size().to_int_size();
    let (w, h) = (size.width() as usize, size.height() as usize);
    if w == 0
        || h == 0
        || w.max(h) > MAX_SIDE as usize
        || w.checked_mul(h).is_none_or(|n| n > MAX_PIXELS)
    {
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

/// Shows the file; a link clicked in a PDF comes back as the address to open.
pub fn show(ui: &mut Ui, p: &Palette, path: &str, v: &mut Viewer) -> Option<String> {
    let mut url = None;
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
            if let Some(super::super::pdf::view::Event::OpenUrl(u)) = view.show(ui, p) {
                url = Some(u);
            }
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
            // Compact 4.0 image viewer toolbar (replaces the hidden LaTeX formatting bar).
            let bar = Rect::from_min_size(area.min, vec2(area.width(), 40.0));
            ui.painter().rect_filled(bar, 0, p.surface);
            ui.painter().hline(
                bar.x_range(),
                bar.bottom() - 0.5,
                Stroke::new(1.0, p.border),
            );
            let name = path.rsplit('/').next().unwrap_or(path);
            {
                let mut tb = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(bar.shrink2(vec2(12.0, 0.0)))
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                tb.spacing_mut().item_spacing.x = 8.0;
                crate::icons::show(&mut tb, crate::icons::IMAGE, 14.5, p.muted);
                tb.label(
                    egui::RichText::new(name)
                        .font(FontId::new(13.0, crate::theme::font::sans_500()))
                        .color(p.ink),
                );
                tb.label(
                    egui::RichText::new(format!("{} × {} px", size[0], size[1]))
                        .font(FontId::monospace(11.5))
                        .color(p.muted),
                );
                tb.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if crate::widgets::Button::new("100%")
                        .size(crate::widgets::Size::Sm)
                        .kind(if *actual {
                            crate::widgets::Kind::Primary
                        } else {
                            crate::widgets::Kind::Ghost
                        })
                        .show(ui, p)
                        .clicked()
                    {
                        *actual = true;
                    }
                    if crate::widgets::Button::new("Fit")
                        .size(crate::widgets::Size::Sm)
                        .kind(if !*actual {
                            crate::widgets::Kind::Primary
                        } else {
                            crate::widgets::Kind::Ghost
                        })
                        .show(ui, p)
                        .clicked()
                    {
                        *actual = false;
                    }
                });
            }

            let canvas = Rect::from_min_max(pos2(area.left(), bar.bottom()), area.max);
            let natural = vec2(size[0] as f32, size[1] as f32) / ui.ctx().pixels_per_point();
            let stage = canvas.shrink(24.0);
            let fitted = {
                let k = (stage.width() / natural.x)
                    .min(stage.height() / natural.y)
                    .min(1.0);
                natural * k
            };
            let dark_mode = p.dark;
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
                checkerboard(painter, rect, dark_mode);
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
                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(canvas));
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
            // Quiet status pill at bottom-left.
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
                pos2(
                    canvas.left() + 12.0,
                    canvas.bottom() - 10.0 - g.size().y - 6.0,
                ),
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
    url
}

/// The 16 px checkerboard behind transparent images, adapted for light and dark themes.
fn checkerboard(painter: &egui::Painter, rect: Rect, dark: bool) {
    let (base, alt) = if dark {
        (
            Color32::from_rgb(0x20, 0x26, 0x2e),
            Color32::from_rgb(0x28, 0x30, 0x3a),
        )
    } else {
        (Color32::WHITE, Color32::from_rgb(0xec, 0xec, 0xec))
    };
    painter.rect_filled(rect, 0, base);
    // Only the cells on screen: an 8192 px image at 100 % has half a million of them.
    let visible = rect.intersect(painter.clip_rect());
    let c = painter.with_clip_rect(visible);
    let mut row = ((visible.top() - rect.top()) / 8.0).floor().max(0.0) as u32;
    while rect.top() + row as f32 * 8.0 < visible.bottom() {
        let y = rect.top() + row as f32 * 8.0;
        let off = if row.is_multiple_of(2) { 8.0 } else { 0.0 };
        let first = ((visible.left() - rect.left() - off) / 16.0)
            .floor()
            .max(0.0);
        let mut x = rect.left() + off + first * 16.0;
        while x < visible.right() {
            c.rect_filled(Rect::from_min_size(pos2(x, y), vec2(8.0, 8.0)), 0, alt);
            x += 16.0;
        }
        row += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn the_checkerboard_paints_only_what_is_on_screen() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let clip = Rect::from_min_size(pos2(4000.0, 4000.0), vec2(100.0, 100.0));
            let painter = ui.painter().with_clip_rect(clip);
            checkerboard(
                &painter,
                Rect::from_min_size(pos2(0.0, 0.0), vec2(8192.0, 8192.0)),
                false,
            );
        });
        out.textures_delta.clear();
        assert!(out.shapes.len() < 500, "{} shapes", out.shapes.len());
        assert!(out.shapes.len() > 50);
    }

    #[test]
    fn tiff_and_bmp_open_and_a_huge_picture_reports_its_own_size() {
        for format in [image::ImageFormat::Tiff, image::ImageFormat::Bmp] {
            let mut bytes = Vec::new();
            image::RgbImage::from_pixel(3, 2, image::Rgb([9, 9, 9]))
                .write_to(&mut std::io::Cursor::new(&mut bytes), format)
                .unwrap();
            let (img, size) = decode_sized("f", &bytes).unwrap();
            assert_eq!((img.size, size), ([3, 2], [3, 2]));
        }
        let mut wide = Vec::new();
        image::RgbImage::new(9000, 1)
            .write_to(
                &mut std::io::Cursor::new(&mut wide),
                image::ImageFormat::Png,
            )
            .unwrap();
        let (img, size) = decode_sized("wide.png", &wide).unwrap();
        assert_eq!(size, [9000, 1]);
        assert_eq!(img.size[0], 8192, "the texture is the scaled copy");
    }

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
