//! Maths rendering for the live preview, the completion glyphs, hover cards and the symbol palette
//! (writing/katexRender.ts). RaTeX lays the formula out with KaTeX's fonts and metrics; the result
//! is rasterized with vello_cpu (already in the app for PDFs), glyph outlines read with skrifa, and
//! kept as a texture. Results are cached; the cache is dropped when the preamble's macros change.

use eframe::egui::{self, Color32, ColorImage, TextureHandle, Vec2, vec2};
use hayro::kurbo::{Affine, BezPath, Point, Rect, Stroke};
use hayro::vello_cpu::{self, Pixmap, RenderContext, color::AlphaColor, color::Srgb};
use ratex_types::display_item::{DisplayItem, DisplayList};
use ratex_types::path_command::PathCommand;
use skrifa::{
    MetadataProvider, instance::LocationRef, instance::Size, outline::DrawSettings,
    outline::OutlinePen,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

/// A rendered formula: its texture and size in points.
#[derive(Clone)]
pub struct Math {
    pub texture: TextureHandle,
    pub size: Vec2,
}

type Key = (String, bool, u32, [u8; 4], u32);

#[derive(Default)]
struct Cache {
    macros: BTreeMap<String, String>,
    /// The macros as `\def`s put before every formula.
    prelude: String,
    done: HashMap<Key, Result<Math, String>>,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

/// The preamble's `\newcommand`s and operators (from `math::collect_macros`); drops the cache when
/// they changed.
pub fn set_macros(macros: BTreeMap<String, String>) {
    CACHE.with_borrow_mut(|c| {
        if c.macros == macros {
            return;
        }
        c.prelude = macros
            .iter()
            .map(|(name, body)| {
                // `#3` in the body means three parameters.
                let n = (1..=9)
                    .rev()
                    .find(|k| body.contains(&format!("#{k}")))
                    .unwrap_or(0);
                let params: String = (1..=n).map(|k| format!("#{k}")).collect();
                format!("\\def{name}{params}{{{body}}}")
            })
            .collect();
        c.macros = macros;
        c.done.clear();
    });
}

/// Lays out `tex`; the error message when it can't be parsed.
pub fn layout(tex: &str, display: bool) -> Result<DisplayList, String> {
    use ratex_layout::{LayoutOptions, layout, to_display_list, validate_layout};
    use ratex_types::math_style::MathStyle;
    let prelude = CACHE.with_borrow(|c| c.prelude.clone());
    let src = format!("{prelude}{tex}");
    let ast = ratex_parser::parse(&src).map_err(|e| {
        e.to_string()
            .trim_start_matches("KaTeX parse error: ")
            .to_owned()
    })?;
    let style = if display {
        MathStyle::Display
    } else {
        MathStyle::Text
    };
    let lbox = layout(&ast, &LayoutOptions::default().with_style(style));
    validate_layout(&lbox).map_err(|e| e.to_string())?;
    Ok(to_display_list(&lbox))
}

/// `tex` rendered `size` points per em in `color`, cached.
pub fn render(
    ctx: &egui::Context,
    tex: &str,
    display: bool,
    size: f32,
    color: Color32,
) -> Result<Math, String> {
    let ppp = ctx.pixels_per_point();
    let key: Key = (
        tex.to_owned(),
        display,
        size.to_bits(),
        color.to_array(),
        ppp.to_bits(),
    );
    if let Some(hit) = CACHE.with_borrow(|c| c.done.get(&key).cloned()) {
        return hit;
    }
    let result = layout(tex, display).and_then(|list| {
        let em = size * ppp;
        let image =
            raster(&list, em, color).ok_or_else(|| "This formula can't be drawn.".to_owned())?;
        let texture = ctx.load_texture(
            format!("math-{}", key.0.len()),
            image,
            egui::TextureOptions::LINEAR,
        );
        Ok(Math {
            texture,
            size: vec2(
                (list.width as f32 * em + 2.0 * PAD) / ppp,
                ((list.height + list.depth) as f32 * em + 2.0 * PAD) / ppp,
            ),
        })
    });
    CACHE.with_borrow_mut(|c| {
        if c.done.len() > 800 {
            c.done.clear();
        }
        c.done.insert(key, result.clone());
    });
    result
}

/// Pixels around the formula, so antialiased edges aren't cut.
const PAD: f32 = 1.0;

fn font_bytes(name: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
    ratex_katex_fonts::ttf_bytes(&format!("KaTeX_{name}.ttf"))
}

/// Collects a glyph outline into a path, scaled and flipped into place.
struct Pen<'a> {
    path: &'a mut BezPath,
    at: Affine,
}

impl OutlinePen for Pen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(self.at * Point::new(x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(self.at * Point::new(x as f64, y as f64));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.path.quad_to(
            self.at * Point::new(cx as f64, cy as f64),
            self.at * Point::new(x as f64, y as f64),
        );
    }
    fn curve_to(&mut self, a: f32, b: f32, c: f32, d: f32, x: f32, y: f32) {
        self.path.curve_to(
            self.at * Point::new(a as f64, b as f64),
            self.at * Point::new(c as f64, d as f64),
            self.at * Point::new(x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        self.path.close_path();
    }
}

fn raster(list: &DisplayList, em: f32, color: Color32) -> Option<ColorImage> {
    let w = (list.width as f32 * em + 2.0 * PAD)
        .ceil()
        .clamp(1.0, 4096.0) as u16;
    let h = ((list.height + list.depth) as f32 * em + 2.0 * PAD)
        .ceil()
        .clamp(1.0, 4096.0) as u16;
    let em = em as f64;
    let pad = PAD as f64;
    let mut cx = RenderContext::new(w, h);
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    cx.set_paint(AlphaColor::<Srgb>::from_rgba8(r, g, b, a));
    let mut fonts: HashMap<String, Option<std::borrow::Cow<'static, [u8]>>> = HashMap::new();
    for item in &list.items {
        match item {
            DisplayItem::GlyphPath {
                x,
                y,
                scale,
                font,
                char_code,
                ..
            } => {
                let Some(bytes) = fonts
                    .entry(font.clone())
                    .or_insert_with(|| font_bytes(font))
                else {
                    continue;
                };
                let Ok(f) = skrifa::FontRef::new(bytes) else {
                    continue;
                };
                let Some(ch) = char::from_u32(*char_code) else {
                    continue;
                };
                let Some(gid) = f.charmap().map(ch) else {
                    continue;
                };
                let Some(glyph) = f.outline_glyphs().get(gid) else {
                    continue;
                };
                let upem = f
                    .metrics(Size::unscaled(), LocationRef::default())
                    .units_per_em
                    .max(1) as f64;
                let k = em * scale / upem;
                let at = Affine::new([k, 0.0, 0.0, -k, x * em + pad, y * em + pad]);
                let mut path = BezPath::new();
                if glyph
                    .draw(
                        DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
                        &mut Pen {
                            path: &mut path,
                            at,
                        },
                    )
                    .is_ok()
                {
                    cx.fill_path(&path);
                }
            }
            DisplayItem::Line {
                x,
                y,
                width,
                thickness,
                ..
            } => {
                let t = (thickness * em).max(1.0);
                let (x0, y0) = (x * em + pad, y * em + pad - t / 2.0);
                cx.fill_rect(&Rect::new(x0, y0, x0 + width * em, y0 + t));
            }
            DisplayItem::Rect {
                x,
                y,
                width,
                height,
                ..
            } => {
                let (x0, y0) = (x * em + pad, y * em + pad);
                cx.fill_rect(&Rect::new(x0, y0, x0 + width * em, y0 + height * em));
            }
            DisplayItem::Path {
                x,
                y,
                commands,
                fill,
                ..
            } => {
                let (ox, oy) = (x * em + pad, y * em + pad);
                let p = |px: f64, py: f64| (ox + px * em, oy + py * em);
                // Each subpath on its own, as ratex-render does: stretchy arrows mix windings.
                let mut path = BezPath::new();
                let flush = |path: &mut BezPath, cx: &mut RenderContext| {
                    if path.elements().is_empty() {
                        return;
                    }
                    if *fill {
                        cx.fill_path(path);
                    } else {
                        cx.set_stroke(Stroke::new(1.5));
                        cx.stroke_path(path);
                    }
                    *path = BezPath::new();
                };
                for c in commands {
                    match *c {
                        PathCommand::MoveTo { x, y } => {
                            if *fill {
                                flush(&mut path, &mut cx);
                            }
                            path.move_to(p(x, y));
                        }
                        PathCommand::LineTo { x, y } => path.line_to(p(x, y)),
                        PathCommand::QuadTo { x1, y1, x, y } => path.quad_to(p(x1, y1), p(x, y)),
                        PathCommand::CubicTo {
                            x1,
                            y1,
                            x2,
                            y2,
                            x,
                            y,
                        } => path.curve_to(p(x1, y1), p(x2, y2), p(x, y)),
                        PathCommand::Close => path.close_path(),
                    }
                }
                flush(&mut path, &mut cx);
            }
        }
    }
    cx.flush();
    let mut pixmap = Pixmap::new(w, h);
    cx.render_with(
        &mut pixmap,
        &mut vello_cpu::Resources::default(),
        vello_cpu::RasterizerSettings::default(),
    );
    Some(ColorImage::from_rgba_premultiplied(
        [w as usize, h as usize],
        pixmap.data_as_u8_slice(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lays_out_the_palette_and_macros() {
        let bad: Vec<String> = crate::writing::symbols::symbols()
            .iter()
            .filter(|s| layout(&s.tex, false).is_err())
            .map(|s| s.tex.clone())
            .collect();
        // RaTeX drew all but `\!` in the M1 spike.
        assert!(bad.len() <= 2, "{bad:?}");
        set_macros(BTreeMap::from([
            ("\\R".to_owned(), "\\mathbb{R}".to_owned()),
            (
                "\\norm".to_owned(),
                "\\left\\lVert #1 \\right\\rVert".to_owned(),
            ),
        ]));
        assert!(layout("f: \\R \\to \\R, \\norm{x}", false).is_ok());
        assert!(layout("\\frac{a}{", false).is_err());
        let list = layout("\\sqrt{x^2+1}", true).unwrap();
        let img = raster(&list, 40.0, Color32::BLACK).unwrap();
        assert!(
            img.pixels.iter().any(|p| p.a() > 200),
            "something was drawn"
        );
    }
}
