//! hayro on a worker thread: opens a PDF, renders pages to images, and reads what pdf.js gave the
//! web app besides pixels: page sizes, the outline, links and the text under each glyph.
//!
//! hayro's caches are `Rc`, so one thread owns the document and everything parsed from it; the UI
//! sends jobs and polls for results. Coordinates are PDF points from the page's top-left corner,
//! the same space SyncTeX uses.

use eframe::egui::{self, ColorImage, Rect, Vec2, pos2, vec2};
use hayro::hayro_interpret::font::Glyph as HGlyph;
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, SoftMask, font::GlyphRun, interpret_page,
};
use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::object::{
    Array, Dict, Name, Object, Rect as PdfRect, String as PdfString,
};
use hayro::hayro_syntax::page::Page;
use hayro::kurbo::{Affine, BezPath, Point, Shape};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

/// Where a link or bookmark goes.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A page, and how far down it, in points from its top.
    Page {
        index: usize,
        top: Option<f32>,
    },
    Url(String),
}

#[derive(Clone, Debug)]
pub struct Link {
    pub rect: Rect,
    pub target: Target,
}

#[derive(Clone, Debug)]
pub struct OutlineNode {
    pub title: String,
    pub target: Option<Target>,
    pub children: Vec<OutlineNode>,
}

/// One glyph of the text layer: its text and its em box.
#[derive(Clone, Debug)]
pub struct Glyph {
    pub text: String,
    pub rect: Rect,
}

pub enum Job {
    Load {
        doc: u64,
        bytes: Arc<Vec<u8>>,
    },
    /// `scale` is pixels per point; `epoch` (when set) drops the job once the view has moved on.
    /// `thumb` marks a page thumbnail.
    Render {
        doc: u64,
        page: usize,
        scale: f32,
        dark: bool,
        epoch: Option<u64>,
        thumb: bool,
    },
    /// The text layer and links of a page.
    Decorate {
        doc: u64,
        page: usize,
    },
    /// Page 1 as a PNG this wide, for the project library.
    Png {
        doc: u64,
        width: f32,
    },
}

pub enum Done {
    Loaded {
        doc: u64,
        result: Result<(Vec<Vec2>, Vec<OutlineNode>), String>,
    },
    Rendered {
        doc: u64,
        page: usize,
        scale: f32,
        dark: bool,
        thumb: bool,
        image: ColorImage,
    },
    Decorated {
        doc: u64,
        page: usize,
        glyphs: Vec<Glyph>,
        links: Vec<Link>,
    },
    Png {
        doc: u64,
        png: Vec<u8>,
    },
}

pub struct Engine {
    tx: Sender<Job>,
    pub rx: Receiver<Done>,
    /// Render jobs with an older epoch are skipped.
    pub epoch: Arc<AtomicU64>,
}

impl Engine {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, jobs) = channel();
        let (done, rx) = channel();
        let epoch = Arc::new(AtomicU64::new(0));
        let e = epoch.clone();
        std::thread::Builder::new()
            .name("pdf".into())
            .spawn(move || run(jobs, done, e, ctx))
            .expect("the PDF thread starts");
        Self { tx, rx, epoch }
    }

    pub fn send(&self, job: Job) {
        let _ = self.tx.send(job);
    }
}

fn run(jobs: Receiver<Job>, done: Sender<Done>, epoch: Arc<AtomicU64>, ctx: egui::Context) {
    let send = |d: Done| {
        let _ = done.send(d);
        ctx.request_repaint();
    };
    let mut queue: VecDeque<Job> = VecDeque::new();
    loop {
        // Wait for a document.
        let (doc, bytes) = loop {
            let job = match queue.pop_front() {
                Some(j) => j,
                None => match jobs.recv() {
                    Ok(j) => j,
                    Err(_) => return,
                },
            };
            if let Job::Load { doc, bytes } = job {
                break (doc, bytes);
            }
        };
        let pdf = match guard(|| Pdf::new((*bytes).clone())) {
            Some(Ok(p)) => p,
            Some(Err(e)) => {
                send(Done::Loaded {
                    doc,
                    result: Err(format!("This PDF could not be read ({e:?}).")),
                });
                continue;
            }
            None => {
                send(Done::Loaded {
                    doc,
                    result: Err("This PDF could not be read.".into()),
                });
                continue;
            }
        };
        let pages = pdf.pages();
        let index: HashMap<(i32, i32), usize> = pages
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                p.raw()
                    .obj_id()
                    .map(|id| ((id.obj_number, id.gen_number), i))
            })
            .collect();
        let sizes: Vec<Vec2> = pages
            .iter()
            .map(|p| {
                let (w, h) = p.render_dimensions();
                vec2(w, h)
            })
            .collect();
        let dests = Dests {
            pdf: &pdf,
            index: &index,
        };
        let outline = guard(|| dests.outline()).unwrap_or_default();
        send(Done::Loaded {
            doc,
            result: Ok((sizes, outline)),
        });

        let cache = RenderCache::new();
        let settings = InterpreterSettings::default();
        // Serve jobs for this document until another one loads.
        loop {
            if queue.is_empty() {
                match jobs.recv() {
                    Ok(j) => queue.push_back(j),
                    Err(_) => return,
                }
            }
            while let Ok(j) = jobs.try_recv() {
                queue.push_back(j);
            }
            // A newer document makes everything queued before it moot.
            if let Some(at) = queue.iter().rposition(|j| matches!(j, Job::Load { .. })) {
                queue.drain(..at);
                break;
            }
            let Some(job) = queue.pop_front() else {
                continue;
            };
            match job {
                Job::Load { .. } => unreachable!(),
                Job::Render {
                    doc: d,
                    page,
                    scale,
                    dark,
                    epoch: e,
                    thumb,
                } => {
                    if d != doc || e.is_some_and(|e| e != epoch.load(Ordering::Relaxed)) {
                        continue;
                    }
                    let Some(pg) = pages.get(page) else { continue };
                    // A page hayro can't draw stays blank rather than taking the app down.
                    let image = guard(|| draw(pg, &cache, &settings, scale, dark))
                        .unwrap_or_else(|| blank(pg, scale));
                    send(Done::Rendered {
                        doc,
                        page,
                        scale,
                        dark,
                        thumb,
                        image,
                    });
                }
                Job::Decorate { doc: d, page } => {
                    if d != doc {
                        continue;
                    }
                    let Some(pg) = pages.get(page) else { continue };
                    let (g, l) = guard(|| (glyphs(pg), dests.links(pg))).unwrap_or_default();
                    send(Done::Decorated {
                        doc,
                        page,
                        glyphs: g,
                        links: l,
                    });
                }
                Job::Png { doc: d, width } => {
                    if d != doc {
                        continue;
                    }
                    let Some(pg) = pages.first() else { continue };
                    let (w, _) = pg.render_dimensions();
                    let Some(img) =
                        guard(|| draw(pg, &cache, &settings, width / w.max(1.0), false))
                    else {
                        continue;
                    };
                    let mut png = Vec::new();
                    let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                    if image::write_buffer_with_format(
                        &mut std::io::Cursor::new(&mut png),
                        &rgba,
                        img.size[0] as u32,
                        img.size[1] as u32,
                        image::ExtendedColorType::Rgba8,
                        image::ImageFormat::Png,
                    )
                    .is_ok()
                    {
                        send(Done::Png { doc, png });
                    }
                }
            }
        }
    }
}

/// Runs `f`, turning a panic inside hayro into None: a broken PDF must not end the app (and
/// lose unsaved edits).
fn guard<T>(f: impl FnOnce() -> T) -> Option<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).ok()
}

fn blank(page: &Page<'_>, scale: f32) -> ColorImage {
    let (w, h) = page.render_dimensions();
    let s = scale * (MAX_PIXELS / (w * h * scale * scale)).sqrt().min(1.0);
    ColorImage::filled(
        [((w * s) as usize).max(1), ((h * s) as usize).max(1)],
        egui::Color32::WHITE,
    )
}

/// Most pixels a page image may have; bigger zooms are drawn smaller and stretched, like the
/// canvas cap in PdfView.svelte.
const MAX_PIXELS: f32 = 16e6;

fn draw<'a>(
    page: &'a Page<'a>,
    cache: &RenderCache<'a>,
    settings: &InterpreterSettings,
    scale: f32,
    dark: bool,
) -> ColorImage {
    let (w, h) = page.render_dimensions();
    let k = (MAX_PIXELS / (w * h * scale * scale)).sqrt().min(1.0);
    // No floor on the scale: a page declared absurdly large is simply drawn tiny, never
    // allocated at full size.
    let s = scale * k;
    if !(s.is_finite() && w * s >= 1.0 && h * s >= 1.0) {
        return blank(page, scale);
    }
    let pix = PixmapSettings {
        x_scale: s,
        y_scale: s,
        bg_color: WHITE,
    };
    let pixmap = render(page, cache, settings, &RenderSettings::default(), &pix);
    let size = [pixmap.width() as usize, pixmap.height() as usize];
    let mut data = pixmap.data_as_u8_slice().to_vec();
    if dark {
        darken(&mut data);
    }
    ColorImage::from_rgba_premultiplied(size, &data)
}

/// `filter: invert(0.92) hue-rotate(180deg)`, the dark-pages look of the web app, per pixel.
fn darken(rgba: &mut [u8]) {
    // A page is long runs of one colour (paper, ink): each run is worked out once.
    let mut last: Option<([u8; 4], [u8; 4])> = None;
    for px in rgba.as_chunks_mut::<4>().0 {
        let out = match last {
            Some((from, to)) if from == *px => to,
            _ => {
                let to = dark_pixel(*px);
                last = Some((*px, to));
                to
            }
        };
        *px = out;
    }
}

fn dark_pixel(px: [u8; 4]) -> [u8; 4] {
    // hue-rotate(180deg) as the CSS filter spec defines it.
    const M: [[f32; 3]; 3] = [
        [-0.574, 1.43, 0.144],
        [0.426, 0.43, 0.144],
        [0.426, 1.43, -0.856],
    ];
    let a = px[3] as f32 / 255.0;
    if a == 0.0 {
        return px;
    }
    // Un-premultiply, invert(0.92), rotate, premultiply again.
    let c: [f32; 3] = std::array::from_fn(|i| {
        let v = px[i] as f32 / 255.0 / a;
        0.92 - 0.84 * v
    });
    let mut out = px;
    for (i, row) in M.iter().enumerate() {
        let v = (row[0] * c[0] + row[1] * c[1] + row[2] * c[2]).clamp(0.0, 1.0);
        out[i] = (v * a * 255.0).round() as u8;
    }
    out
}

/// Maps PDF user space of a page to points from its top-left corner.
fn to_view(page: &Page<'_>) -> Affine {
    page.initial_transform(true).to_kurbo()
}

fn rect_of(a: Affine, r: hayro::kurbo::Rect) -> Rect {
    let b = a.transform_rect_bbox(r);
    Rect::from_min_max(
        pos2(b.x0 as f32, b.y0 as f32),
        pos2(b.x1 as f32, b.y1 as f32),
    )
}

/// Text strings: UTF-16BE with a BOM, else PDFDocEncoding (Latin-1 is close enough here).
fn decode_text(b: &[u8]) -> String {
    if b.starts_with(&[0xfe, 0xff]) {
        let units: Vec<u16> = b[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        b.iter().map(|&c| c as char).collect()
    }
}

/// Resolves destinations, links and bookmarks against the page list.
struct Dests<'a> {
    pdf: &'a Pdf,
    index: &'a HashMap<(i32, i32), usize>,
}

impl Dests<'_> {
    fn catalog(&self) -> Option<Dict<'_>> {
        self.pdf.xref().get(self.pdf.xref().root_id())
    }

    /// An explicit destination `[page /XYZ left top zoom]` (or `/FitH top`, …), a named one, or a
    /// dictionary holding one under `/D`.
    fn target(&self, obj: Object<'_>, depth: usize) -> Option<Target> {
        if depth > 8 {
            return None;
        }
        match obj {
            Object::Array(a) => self.explicit(&a),
            Object::Name(n) => self.named(n.as_str().as_bytes(), depth),
            Object::String(s) => self.named(s.as_bytes(), depth),
            Object::Dict(d) => self.target(d.get::<Object>(b"D")?, depth + 1),
            _ => None,
        }
    }

    fn explicit(&self, a: &Array<'_>) -> Option<Target> {
        let items: Vec<Object> = a.iter::<Object>().collect();
        let index = match items.first()? {
            Object::Dict(d) => {
                let id = d.obj_id()?;
                *self.index.get(&(id.obj_number, id.gen_number))?
            }
            Object::Number(n) => n.as_i64().max(0) as usize,
            _ => return None,
        };
        let num = |i: usize| match items.get(i) {
            Some(Object::Number(n)) => Some(n.as_f64()),
            _ => None,
        };
        let kind = match items.get(1) {
            Some(Object::Name(n)) => n.as_str().to_owned(),
            _ => String::new(),
        };
        let y = match kind.as_str() {
            "XYZ" => num(3),
            "FitH" | "FitBH" => num(2),
            _ => None,
        };
        let pages = self.pdf.pages();
        let top = y.and_then(|y| {
            let p = to_view(pages.get(index)?) * Point::new(0.0, y);
            Some(p.y as f32)
        });
        Some(Target::Page { index, top })
    }

    /// Named destinations: the catalog's `/Dests` dictionary, or the `/Names /Dests` name tree.
    fn named(&self, name: &[u8], depth: usize) -> Option<Target> {
        let cat = self.catalog()?;
        if let Some(d) = cat.get::<Dict>(b"Dests")
            && let Some(o) = d.get::<Object>(name)
        {
            return self.target(o, depth + 1);
        }
        let tree = cat.get::<Dict>(b"Names")?.get::<Dict>(b"Dests")?;
        self.target(lookup(tree, name, 0)?, depth + 1)
    }

    fn outline(&self) -> Vec<OutlineNode> {
        let Some(first) = self
            .catalog()
            .and_then(|c| c.get::<Dict>(b"Outlines"))
            .and_then(|o| o.get::<Dict>(b"First"))
        else {
            return Vec::new();
        };
        let mut budget = 10_000;
        self.siblings(first, 0, &mut budget)
    }

    fn siblings(&self, first: Dict<'_>, depth: usize, budget: &mut usize) -> Vec<OutlineNode> {
        let mut out = Vec::new();
        let mut cur = Some(first);
        while let Some(it) = cur {
            if *budget == 0 || depth > 32 {
                break;
            }
            *budget -= 1;
            let title = it
                .get::<PdfString>(b"Title")
                .map(|s| decode_text(s.as_bytes()))
                .unwrap_or_default();
            let target = self.action_target(&it);
            let children = it
                .get::<Dict>(b"First")
                .map(|c| self.siblings(c, depth + 1, budget))
                .unwrap_or_default();
            out.push(OutlineNode {
                title,
                target,
                children,
            });
            cur = it.get::<Dict>(b"Next");
        }
        out
    }

    /// `/Dest`, or an action `/A` that goes to a destination or opens a URL.
    fn action_target(&self, d: &Dict<'_>) -> Option<Target> {
        if let Some(o) = d.get::<Object>(b"Dest") {
            return self.target(o, 0);
        }
        let a = d.get::<Dict>(b"A")?;
        match a.get::<Name>(b"S")?.as_str() {
            "GoTo" => self.target(a.get::<Object>(b"D")?, 0),
            "URI" => Some(Target::Url(decode_text(
                a.get::<PdfString>(b"URI")?.as_bytes(),
            ))),
            _ => None,
        }
    }

    fn links(&self, page: &Page<'_>) -> Vec<Link> {
        let Some(annots) = page.raw().get::<Array>(b"Annots") else {
            return Vec::new();
        };
        let t = to_view(page);
        annots
            .iter::<Dict>()
            .filter(|a| {
                a.get::<Name>(b"Subtype")
                    .is_some_and(|s| s.as_str() == "Link")
            })
            .filter_map(|a| {
                let r = a.get::<PdfRect>(b"Rect")?;
                let target = self.action_target(&a)?;
                Some(Link {
                    rect: rect_of(t, hayro::kurbo::Rect::new(r.x0, r.y0, r.x1, r.y1)),
                    target,
                })
            })
            .collect()
    }
}

/// Finds `key` in a name tree (`/Names [key value …]` leaves under `/Kids`).
fn lookup<'a>(node: Dict<'a>, key: &[u8], depth: usize) -> Option<Object<'a>> {
    if depth > 32 {
        return None;
    }
    if let Some(names) = node.get::<Array>(b"Names") {
        let items: Vec<Object> = names.iter::<Object>().collect();
        for pair in items.chunks(2) {
            if let [Object::String(k), v] = pair
                && k.as_bytes() == key
            {
                return Some(v.clone());
            }
        }
    }
    let kids = node.get::<Array>(b"Kids")?;
    for kid in kids.iter::<Dict>() {
        // Each kid says which keys it covers; skip the others.
        if let Some(limits) = kid.get::<Array>(b"Limits") {
            let l: Vec<PdfString> = limits.iter::<PdfString>().collect();
            if let [lo, hi] = l.as_slice()
                && (key < lo.as_bytes() || key > hi.as_bytes())
            {
                continue;
            }
        }
        if let Some(v) = lookup(kid, key, depth + 1) {
            return Some(v);
        }
    }
    None
}

/// Collects each glyph's text and em box; draws nothing.
struct TextDevice {
    glyphs: Vec<Glyph>,
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
            // The em box from the advance, as a text layer selects it; ink boxes leave gaps.
            let bbox = match &**g {
                HGlyph::Outline(o) => {
                    let ink = o.outline().bounding_box();
                    let adv = o.advance_width().map_or(ink.x1, f64::from);
                    hayro::kurbo::Rect::new(0.0, -200.0, adv, 800.0)
                }
                HGlyph::Type3(_) => hayro::kurbo::Rect::new(0.0, 0.0, 1.0, 1.0),
            };
            self.glyphs.push(Glyph {
                text,
                rect: rect_of(ts, bbox),
            });
        }
    }
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

fn glyphs(page: &Page<'_>) -> Vec<Glyph> {
    let cache = InterpreterCache::new();
    let (w, h) = page.render_dimensions();
    let mut ctx = Context::new(
        to_view(page),
        hayro::kurbo::Rect::new(0.0, 0.0, w as f64, h as f64),
        &cache,
        page.xref(),
        InterpreterSettings::default(),
    );
    let mut dev = TextDevice { glyphs: Vec::new() };
    interpret_page(page, &mut ctx, &mut dev);
    dev.glyphs
}

/// Page text with spaces and line breaks inferred from the gaps between glyphs, and for every
/// char of it the glyph it came from (None for an inferred space).
pub fn page_text(glyphs: &[Glyph]) -> (String, Vec<Option<usize>>) {
    let mut s = String::new();
    let mut map = Vec::new();
    let mut prev: Option<Rect> = None;
    for (i, g) in glyphs.iter().enumerate() {
        let r = g.rect;
        if let Some(p) = prev {
            let line_h = p.height().max(r.height()).max(1.0);
            if (r.center().y - p.center().y).abs() > line_h * 0.6
                || r.left() - p.right() > line_h * 0.12
            {
                // pdf.js joins items on a line break with a space too (hasEOL).
                s.push(' ');
                map.push(None);
            }
        }
        for c in g.text.chars() {
            s.push(c);
            map.push(Some(i));
        }
        prev = Some(r);
    }
    (s, map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(t: &str, x: f32, y: f32) -> Glyph {
        Glyph {
            text: t.into(),
            rect: Rect::from_min_size(pos2(x, y), vec2(5.0, 10.0)),
        }
    }

    #[test]
    fn infers_spaces_and_line_breaks() {
        let glyphs = [
            g("a", 0.0, 0.0),
            g("b", 5.0, 0.0),
            g("c", 20.0, 0.0),
            g("d", 0.0, 30.0),
        ];
        let (s, map) = page_text(&glyphs);
        assert_eq!(s, "ab c d");
        assert_eq!(map, [Some(0), Some(1), None, Some(2), None, Some(3)]);
    }

    #[test]
    fn dark_pages_are_the_same_pixel_by_pixel_and_quick() {
        // A page: paper, runs of ink, antialiased edges, transparent bits.
        let mut page = Vec::new();
        for i in 0..1700 * 2200u32 {
            let v = match i % 997 {
                0..900 => [255, 255, 255, 255],
                900..990 => [20, 20, 20, 255],
                n => [n as u8, (n * 7) as u8, 255 - n as u8, (n % 3 * 120) as u8],
            };
            page.extend(v);
        }
        let want: Vec<u8> = page
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&p| dark_pixel(p))
            .collect();
        let start = std::time::Instant::now();
        darken(&mut page);
        let took = start.elapsed();
        assert_eq!(page, want, "identical to working out every pixel");
        eprintln!("darkened a 1700×2200 page in {took:?}");
    }

    #[test]
    fn dark_pages_turn_white_to_near_black() {
        let mut px = [255, 255, 255, 255, 0, 0, 0, 255];
        darken(&mut px);
        assert!(px[0] < 30 && px[1] < 30 && px[2] < 30, "{px:?}");
        assert!(px[4] > 220, "{px:?}");
    }
}
