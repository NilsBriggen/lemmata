//! M1 spike: can hayro do what pdf.js does for Lemmata?
//!
//! `cargo run --release -- OUT_DIR file.pdf...` renders every page at 144 dpi (zoom 1.5 on a 1.33x
//! screen, a typical case), times it, compares page renders with poppler's `pdftoppm` as the
//! reference rasterizer, checks the extracted text against `pdftotext`, and lists links and outline
//! entries.

use hayro::hayro_interpret::font::Glyph;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, SoftMask, font::GlyphRun, interpret_page,
};
use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::object::{Array, Dict, Name, Object, Rect as PdfRect, String as PdfString};
use hayro::hayro_syntax::page::Page;
use hayro::kurbo::{Affine, BezPath, Rect, Shape};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

const SCALE: f32 = 2.0; // 144 dpi

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("OUT_DIR");
    std::fs::create_dir_all(&out).unwrap();
    for file in args {
        report(Path::new(&file), Path::new(&out));
    }
}

fn report(file: &Path, out: &Path) {
    let name = file.parent().unwrap().file_name().unwrap().to_string_lossy().to_string();
    println!("\n== {name} ({} bytes)", std::fs::metadata(file).unwrap().len());

    let t = Instant::now();
    let pdf = Pdf::new(std::fs::read(file).unwrap()).unwrap();
    let pages = pdf.pages();
    let open = t.elapsed();

    let settings = InterpreterSettings::default();
    let cache = RenderCache::new();
    let pix = PixmapSettings { x_scale: SCALE, y_scale: SCALE, bg_color: WHITE };
    let mut times = Vec::new();
    let mut first = None;
    let mut worst_diff = (0.0f64, 0usize);
    for (i, page) in pages.iter().enumerate() {
        let t = Instant::now();
        let pixmap = render(page, &cache, &settings, &RenderSettings::default(), &pix);
        let dt = t.elapsed().as_secs_f64() * 1000.0;
        if i == 0 {
            first = Some(open.as_secs_f64() * 1000.0 + dt);
        }
        times.push(dt);
        // Compare a sample of pages with poppler.
        if i < 4 || i % 10 == 0 {
            let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
            let rgba: Vec<u8> = pixmap.data_as_u8_slice().to_vec();
            if let Some(diff) = compare_with_poppler(file, i, w, h, &rgba) {
                if diff > worst_diff.0 {
                    worst_diff = (diff, i);
                }
            }
            if i < 4 || i == 20 {
                std::fs::write(out.join(format!("{name}-p{}-hayro.png", i + 1)), pixmap.into_png().unwrap()).unwrap();
            }
        }
    }
    times.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!(
        "render: {} pages, open+first page {:.1} ms, mean {:.1} ms, p95 {:.1} ms, max {:.1} ms",
        times.len(),
        first.unwrap(),
        mean,
        times[(times.len() * 95 / 100).min(times.len() - 1)],
        times.last().unwrap()
    );
    println!("fidelity: worst sampled page {} differs from poppler in {:.2}% of pixels", worst_diff.1 + 1, worst_diff.0);

    // Text layer.
    let mut found = 0;
    let mut total = 0;
    let mut sample = String::new();
    for (i, page) in pages.iter().enumerate() {
        let text = extract_text(page);
        if i == 0 {
            sample = text.chars().take(160).collect();
        }
        let reference = Command::new("pdftotext")
            .args(["-f", &(i + 1).to_string(), "-l", &(i + 1).to_string(), "-enc", "UTF-8"])
            .arg(file)
            .arg("-")
            .output()
            .unwrap()
            .stdout;
        let reference = String::from_utf8_lossy(&reference);
        let ours: std::collections::HashSet<&str> = text.split_whitespace().collect();
        for word in reference.split_whitespace() {
            total += 1;
            if ours.contains(word) {
                found += 1;
            }
        }
    }
    println!("text: {found}/{total} pdftotext words found as whole words ({:.1}%)", 100.0 * found as f64 / total.max(1) as f64);
    println!("text sample p1: {sample:?}");

    // Links and outline.
    let mut links = 0;
    let mut link_kinds = std::collections::BTreeMap::<String, usize>::new();
    for page in pages.iter() {
        let Some(annots) = page.raw().get::<Array>(b"Annots") else { continue };
        for annot in annots.iter::<Dict>() {
            if annot.get::<Name>(b"Subtype").is_some_and(|s| s.as_str() == "Link") && annot.get::<PdfRect>(b"Rect").is_some() {
                links += 1;
                let kind = if let Some(a) = annot.get::<Dict>(b"A") {
                    a.get::<Name>(b"S").map(|s| s.as_str().to_owned()).unwrap_or_default()
                } else if annot.get::<Object>(b"Dest").is_some() {
                    "Dest".into()
                } else {
                    "?".into()
                };
                *link_kinds.entry(kind).or_default() += 1;
            }
        }
    }
    println!("links: {links} {link_kinds:?}");
    let catalog: Option<Dict> = pdf.xref().get(pdf.xref().root_id());
    let mut outline = Vec::new();
    if let Some(first) = catalog.and_then(|c| c.get::<Dict>(b"Outlines")).and_then(|o| o.get::<Dict>(b"First")) {
        walk_outline(first, 0, &mut outline);
    }
    println!("outline: {} entries, first: {:?}", outline.len(), outline.iter().take(4).collect::<Vec<_>>());
}

fn walk_outline(item: Dict<'_>, depth: usize, out: &mut Vec<String>) {
    let mut cur = Some(item);
    while let Some(it) = cur {
        let title = it.get::<PdfString>(b"Title").map(|s| decode_text(s.as_bytes())).unwrap_or_default();
        out.push(format!("{}{}", "  ".repeat(depth), title));
        if let Some(child) = it.get::<Dict>(b"First") {
            walk_outline(child, depth + 1, out);
        }
        cur = it.get::<Dict>(b"Next");
        if out.len() > 10_000 {
            return;
        }
    }
}

/// PDF text strings: UTF-16BE with a BOM, else PDFDocEncoding (close enough to Latin-1 here).
fn decode_text(b: &[u8]) -> String {
    if b.starts_with(&[0xfe, 0xff]) {
        let units: Vec<u16> = b[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units)
    } else {
        b.iter().map(|&c| c as char).collect()
    }
}

struct TextDevice {
    /// (char, device-space box) in reading order of the content stream.
    glyphs: Vec<(String, Rect)>,
}

impl<'a> Device<'a> for TextDevice {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        for g in run.glyphs() {
            let Some(text) = g.as_unicode() else { continue };
            let text = match text {
                hayro::hayro_interpret::hayro_cmap::BfString::Char(c) => c.to_string(),
                hayro::hayro_interpret::hayro_cmap::BfString::String(s) => s,
            };
            let ts: Affine = props.transform * g.transform();
            // The em box from the advance, as a text layer selects it; ink boxes leave gaps between letters.
            let bbox = match &**g {
                Glyph::Outline(o) => {
                    let ink = o.outline().bounding_box();
                    let adv = o.advance_width().map_or(ink.x1, f64::from);
                    Rect::new(0.0, -200.0, adv, 800.0)
                }
                Glyph::Type3(_) => Rect::new(0.0, 0.0, 1.0, 1.0),
            };
            self.glyphs.push((text, ts.transform_rect_bbox(bbox)));
        }
    }
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

/// Page text with spaces and newlines inferred from glyph gaps, the way a text layer does.
fn extract_text(page: &Page<'_>) -> String {
    let cache = InterpreterCache::new();
    let (w, h) = page.render_dimensions();
    let mut ctx = Context::new(
        page.initial_transform(true).to_kurbo(),
        Rect::new(0.0, 0.0, w as f64, h as f64),
        &cache,
        page.xref(),
        InterpreterSettings::default(),
    );
    let mut dev = TextDevice { glyphs: Vec::new() };
    interpret_page(page, &mut ctx, &mut dev);
    let mut s = String::new();
    let mut prev: Option<Rect> = None;
    for (t, r) in dev.glyphs {
        if let Some(p) = prev {
            let line_h = p.height().max(r.height()).max(1.0);
            if (r.center().y - p.center().y).abs() > line_h * 0.6 {
                s.push('\n');
            } else if r.x0 - p.x1 > line_h * 0.12 {
                s.push(' ');
            }
        }
        s.push_str(&t);
        prev = Some(r);
    }
    s
}

use hayro::hayro_interpret::util::TransformExt;

/// Percentage of pixels whose channel difference to poppler's render exceeds 64 (out of 255).
fn compare_with_poppler(file: &Path, page: usize, w: usize, h: usize, rgba: &[u8]) -> Option<f64> {
    let n = (page + 1).to_string();
    let out = Command::new("pdftoppm")
        .args(["-r", "144", "-f", &n, "-l", &n, "-scale-to-x", &w.to_string(), "-scale-to-y", &h.to_string()])
        .arg(file)
        .output()
        .ok()?
        .stdout;
    // P6 header: "P6\n<w> <h>\n255\n".
    let mut fields = 0;
    let mut i = 0;
    while fields < 4 {
        while out[i].is_ascii_whitespace() {
            i += 1;
        }
        while !out[i].is_ascii_whitespace() {
            i += 1;
        }
        fields += 1;
    }
    let data = &out[i + 1..];
    if data.len() < w * h * 3 {
        return None;
    }
    let mut bad = 0usize;
    for px in 0..w * h {
        let (a, b) = (&rgba[px * 4..px * 4 + 3], &data[px * 3..px * 3 + 3]);
        if a.iter().zip(b).any(|(x, y)| x.abs_diff(*y) > 64) {
            bad += 1;
        }
    }
    Some(100.0 * bad as f64 / (w * h) as f64)
}
