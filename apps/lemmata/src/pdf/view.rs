//! PdfView.svelte: the pages of a PDF in a scrolling column or in spreads, fitted to the pane or
//! at a fixed zoom. Pages render on the engine's thread at the screen's pixel density; a new PDF
//! replaces the old one page by page in place, so the reader keeps their position across builds.
//! Also the text layer (selection, search), links, and the SyncTeX highlight.

use super::engine::{Done, Engine, Glyph, Job, Link, OutlineNode, Target, page_text};
use crate::theme::Palette;
use eframe::egui::{
    self, Color32, CornerRadius, Rect, Sense, Shadow, Stroke, StrokeKind, TextureHandle, Ui, Vec2,
    pos2, vec2,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// "width" and "page" fit the pane; a number is a fixed zoom (1 = 100 %).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fit {
    Width,
    Page,
    Zoom(f32),
}

impl Fit {
    pub fn to_json(self) -> serde_json::Value {
        match self {
            Fit::Width => "width".into(),
            Fit::Page => "page".into(),
            Fit::Zoom(z) => serde_json::json!(z),
        }
    }
    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        match v {
            serde_json::Value::String(s) if s == "width" => Some(Fit::Width),
            serde_json::Value::String(s) if s == "page" => Some(Fit::Page),
            v => v.as_f64().map(|z| Fit::Zoom(z as f32)),
        }
    }
}

/// One page per row, or spreads: "spread" leaves page 1 alone (2–3, 4–5 …, like a book),
/// "spread-first" pairs 1–2, 3–4 …
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    Single,
    Spread,
    SpreadFirst,
}

impl Layout {
    pub fn from_setting(s: &str) -> Self {
        match s {
            "spread" => Layout::Spread,
            "spread-first" => Layout::SpreadFirst,
            _ => Layout::Single,
        }
    }
}

/// PDF points are 1/72 in; CSS pixels (egui points) 1/96 in.
const PT: f32 = 96.0 / 72.0;
const GAP: f32 = 14.0;
/// `.pages` padding: 18 top, 24 sides, 40 bottom.
const PAD_TOP: f32 = 18.0;
const PAD_X: f32 = 24.0;
const PAD_BOTTOM: f32 = 40.0;
/// Thumbnail width (PdfThumbnails.svelte).
pub const THUMB_W: f32 = 168.0;

struct Tex {
    handle: TextureHandle,
    scale: f32,
    dark: bool,
    doc: u64,
}

struct Hit {
    page: usize,
    rects: Vec<Rect>,
}

/// What happened in the view that the pane acts on.
pub enum Event {
    /// A double-click or Ctrl/Cmd-click: page (1-based) and point in PDF points from the top-left.
    SourceClick(usize, f32, f32),
}

pub struct PdfView {
    engine: Engine,
    /// The newest document sent to the engine, and the one whose pages are laid out.
    sent: u64,
    shown: u64,
    /// The newest document's bytes.
    bytes: Option<Arc<Vec<u8>>>,
    pub pages: Vec<Vec2>,
    pub outline: Vec<OutlineNode>,
    pub error: Option<String>,
    pub fit: Fit,
    pub layout: Layout,
    pub dark: bool,
    tex: HashMap<usize, Tex>,
    thumbs: HashMap<usize, Tex>,
    /// Render jobs sent and not answered yet: page → (doc, scale, dark, thumb).
    asked: HashSet<(usize, bool)>,
    epoch_scale: f32,
    deco: HashMap<usize, (Vec<Glyph>, Vec<Link>)>,
    decorating: HashSet<usize>,
    /// 1-based, for the page box.
    pub current_page: usize,
    pub percent: u32,
    /// The page boxes in content coordinates and their scales (points → egui points).
    rects: Vec<Rect>,
    scales: Vec<f32>,
    content: Vec2,
    client: Vec2,
    scroll_y: f32,
    /// Applied on the next frame: an exact offset, or a smooth scroll to a content rect.
    jump: Option<Jump>,
    /// A scroll ratio to restore once a PDF is laid out.
    pending_ratio: Option<f32>,
    /// A page to keep in view after a layout change, and a SyncTeX box to scroll to once laid out.
    pending_page: Option<usize>,
    pending_box: bool,
    highlight: Option<(usize, Rect, f64)>,
    query: String,
    hits: Vec<Hit>,
    /// 1-based, 0 when there is none.
    pub match_index: usize,
    searching: bool,
    selection: Option<(usize, usize, usize)>,
    png_wanted: bool,
    /// The pages have keyboard focus (Ctrl+F searches the PDF, Ctrl+C copies the selection).
    pub focused: bool,
    pub png: Option<Vec<u8>>,
    id: egui::Id,
}

enum Jump {
    Offset(Vec2),
    /// Scroll so this content y sits this far from the top.
    Smooth(f32),
}

impl PdfView {
    pub fn new(ctx: &egui::Context, id: egui::Id) -> Self {
        Self {
            engine: Engine::new(ctx.clone()),
            sent: 0,
            bytes: None,
            shown: 0,
            pages: Vec::new(),
            outline: Vec::new(),
            error: None,
            fit: Fit::Width,
            layout: Layout::Single,
            dark: false,
            tex: HashMap::new(),
            thumbs: HashMap::new(),
            asked: HashSet::new(),
            epoch_scale: 0.0,
            deco: HashMap::new(),
            decorating: HashSet::new(),
            current_page: 1,
            percent: 100,
            rects: Vec::new(),
            scales: Vec::new(),
            content: Vec2::ZERO,
            client: Vec2::ZERO,
            scroll_y: 0.0,
            jump: None,
            pending_ratio: None,
            pending_page: None,
            pending_box: false,
            highlight: None,
            query: String::new(),
            hits: Vec::new(),
            match_index: 0,
            searching: false,
            selection: None,
            png_wanted: false,
            focused: false,
            png: None,
            id,
        }
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Shows a new PDF; the old one stays up until the new pages are drawn.
    pub fn load(&mut self, bytes: Vec<u8>) {
        self.sent += 1;
        let bytes = Arc::new(bytes);
        self.bytes = Some(bytes.clone());
        self.engine.send(Job::Load {
            doc: self.sent,
            bytes,
        });
    }

    /// The PDF on show, for presenting: None until it has loaded.
    pub fn shown_bytes(&self) -> Option<Arc<Vec<u8>>> {
        (self.shown == self.sent && !self.pages.is_empty())
            .then(|| self.bytes.clone())
            .flatten()
    }

    /// Takes what the engine finished.
    fn receive(&mut self, ctx: &egui::Context) {
        while let Ok(d) = self.engine.rx.try_recv() {
            match d {
                Done::Loaded { doc, result } => {
                    if doc != self.sent {
                        continue;
                    }
                    match result {
                        Ok((pages, outline)) => {
                            self.shown = doc;
                            self.error = None;
                            self.pages = pages;
                            self.outline = outline;
                            self.tex.retain(|&n, _| n < self.pages.len());
                            self.thumbs.retain(|&n, _| n < self.pages.len());
                            self.asked.clear();
                            self.deco.clear();
                            self.decorating.clear();
                            self.selection = None;
                            if self.png_wanted {
                                self.engine.send(Job::Png { doc, width: 360.0 });
                            }
                            if !self.query.is_empty() {
                                self.search(&self.query.clone(), false);
                            }
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                Done::Rendered {
                    doc,
                    page,
                    scale,
                    dark,
                    thumb,
                    image,
                } => {
                    self.asked.remove(&(page, thumb));
                    if doc != self.shown {
                        continue;
                    }
                    let handle = ctx.load_texture(
                        format!("pdf-{page}-{thumb}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    let t = Tex {
                        handle,
                        scale,
                        dark,
                        doc,
                    };
                    if thumb {
                        self.thumbs.insert(page, t);
                    } else {
                        self.tex.insert(page, t);
                    }
                }
                Done::Decorated {
                    doc,
                    page,
                    glyphs,
                    links,
                } => {
                    if doc != self.shown {
                        continue;
                    }
                    self.decorating.remove(&page);
                    self.deco.insert(page, (glyphs, links));
                    if self.searching && self.decorating.is_empty() {
                        self.searching = false;
                        let q = self.query.clone();
                        self.find(&q, self.hits.is_empty());
                    }
                }
                Done::Png { doc, png } => {
                    if doc == self.shown {
                        self.png_wanted = false;
                        self.png = Some(png);
                    }
                }
            }
        }
    }

    /// Page 1 as a PNG, for the library; arrives in `png`.
    pub fn request_png(&mut self) {
        if self.shown > 0 {
            self.engine.send(Job::Png {
                doc: self.shown,
                width: 360.0,
            });
        } else {
            self.png_wanted = true;
        }
    }

    fn two_up(&self) -> bool {
        self.layout != Layout::Single && self.pages.len() > 1
    }

    fn scale_for(&self, size: Vec2) -> f32 {
        let cols = if self.two_up() { 2.0 } else { 1.0 };
        let avail_w = ((self.client.x - 48.0 - GAP * (cols - 1.0)) / cols).max(160.0);
        let avail_h = (self.client.y - 40.0).max(160.0);
        match self.fit {
            Fit::Width => avail_w / size.x,
            Fit::Page => (avail_w / size.x).min(avail_h / size.y),
            Fit::Zoom(z) => z * PT,
        }
    }

    /// Where every page goes for the pane's size.
    fn relayout(&mut self) {
        let n = self.pages.len();
        self.scales = self.pages.iter().map(|&s| self.scale_for(s)).collect();
        let sizes: Vec<Vec2> = self
            .pages
            .iter()
            .zip(&self.scales)
            .map(|(s, k)| *s * *k)
            .collect();
        self.rects = vec![Rect::NOTHING; n];
        if n == 0 {
            self.content = Vec2::ZERO;
            return;
        }
        self.percent = (self.scales[0] / PT * 100.0).round() as u32;
        if !self.two_up() {
            let w = sizes.iter().map(|s| s.x).fold(0.0, f32::max);
            let width = (w + 2.0 * PAD_X).max(self.client.x);
            let mut y = PAD_TOP;
            for (i, s) in sizes.iter().enumerate() {
                self.rects[i] = Rect::from_min_size(pos2((width - s.x) / 2.0, y), *s);
                y += s.y + GAP;
            }
            self.content = vec2(width, y - GAP + PAD_BOTTOM);
            return;
        }
        // Two columns: the left one's pages sit at its right edge, the right one's at its left.
        let cell = |i: usize| match self.layout {
            Layout::SpreadFirst => (i / 2, i % 2),
            _ => (i.div_ceil(2), (i + 1) % 2),
        };
        let rows = cell(n - 1).0 + 1;
        let mut cols = [0.0f32; 2];
        let mut heights = vec![0.0f32; rows];
        for (i, s) in sizes.iter().enumerate() {
            let (r, c) = cell(i);
            cols[c] = cols[c].max(s.x);
            heights[r] = heights[r].max(s.y);
        }
        let grid = cols[0] + GAP + cols[1];
        let width = (grid + 2.0 * PAD_X).max(self.client.x);
        let left = (width - grid) / 2.0;
        let mut tops = Vec::with_capacity(rows);
        let mut y = PAD_TOP;
        for h in &heights {
            tops.push(y);
            y += h + GAP;
        }
        for (i, s) in sizes.iter().enumerate() {
            let (r, c) = cell(i);
            let x = if c == 0 {
                left + cols[0] - s.x
            } else {
                left + cols[0] + GAP
            };
            self.rects[i] = Rect::from_min_size(pos2(x, tops[r]), *s);
        }
        self.content = vec2(width, y - GAP + PAD_BOTTOM);
    }

    /// Changes the zoom, keeping the same part of the document in view.
    pub fn set_fit(&mut self, fit: Fit) {
        let ratio = self.scroll_y / self.content.y.max(1.0);
        self.fit = match fit {
            Fit::Zoom(z) => Fit::Zoom(z.clamp(0.25, 4.0)),
            f => f,
        };
        self.pending_ratio = Some(ratio);
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.set_fit(Fit::Zoom(
            ((self.percent as f32 / 100.0) * factor * 20.0).round() / 20.0,
        ));
    }

    /// Changes single page / spreads, keeping the current page in view.
    pub fn set_layout(&mut self, layout: Layout) {
        if layout != self.layout {
            self.layout = layout;
            let n = self.current_page;
            self.pending_page = Some(n);
        }
    }

    pub fn goto_page(&mut self, n: usize) {
        if let Some(r) = self.rects.get(n.clamp(1, self.pages.len().max(1)) - 1) {
            self.jump = Some(Jump::Smooth(r.top() - 12.0));
        }
    }

    pub fn goto_target(&mut self, t: &Target) -> Option<String> {
        match t {
            Target::Page { index, top } => {
                let (Some(r), Some(s)) = (self.rects.get(*index), self.scales.get(*index)) else {
                    return None;
                };
                self.jump = Some(Jump::Smooth(r.top() + top.unwrap_or(0.0) * s - 16.0));
                None
            }
            Target::Url(u) => Some(u.clone()),
        }
    }

    /// `{ fit, scroll }` for the session.
    pub fn position(&self) -> serde_json::Value {
        serde_json::json!({ "fit": self.fit.to_json(), "scroll": self.scroll_y / self.content.y.max(1.0) })
    }

    pub fn set_position(&mut self, v: &serde_json::Value) {
        if let Some(f) = v.get("fit").and_then(Fit::from_json) {
            self.fit = f;
        }
        if let Some(s) = v.get("scroll").and_then(|s| s.as_f64()) {
            self.pending_ratio = Some(s as f32);
        }
    }

    /// The SyncTeX boxes for a line: shows the first, scrolled a third down, fading out.
    pub fn show_box(
        &mut self,
        ctx: &egui::Context,
        page: usize,
        h: f32,
        v: f32,
        width: f32,
        height: f32,
    ) {
        let height = height.max(10.0);
        let r = Rect::from_min_size(pos2(h, v - height), vec2(width.max(40.0), height));
        self.highlight = Some((page.saturating_sub(1), r, ctx.input(|i| i.time)));
        if let (Some(pr), Some(s)) = (
            self.rects.get(page.saturating_sub(1)),
            self.scales.get(page.saturating_sub(1)),
        ) {
            self.jump = Some(Jump::Smooth(
                pr.top() + r.top() * s - 3.0 - self.client.y / 3.0,
            ));
        } else {
            self.pending_box = true;
        }
    }

    // ---- Search ---------------------------------------------------------

    /// Finds `query` on every page; the text layers of pages not seen yet are read first.
    pub fn search(&mut self, query: &str, jump: bool) {
        self.query = query.to_owned();
        self.hits.clear();
        self.match_index = 0;
        if query.trim().is_empty() || self.shown == 0 {
            self.searching = false;
            return;
        }
        for n in 0..self.pages.len() {
            if !self.deco.contains_key(&n) && self.decorating.insert(n) {
                self.engine.send(Job::Decorate {
                    doc: self.shown,
                    page: n,
                });
            }
        }
        if self.decorating.is_empty() {
            self.find(query, jump);
        } else {
            self.searching = true;
        }
    }

    fn find(&mut self, query: &str, jump: bool) {
        let q = query.trim().to_lowercase();
        self.hits.clear();
        self.match_index = 0;
        if q.is_empty() {
            return;
        }
        for n in 0..self.pages.len() {
            let Some((glyphs, _)) = self.deco.get(&n) else {
                continue;
            };
            let (text, map) = page_text(glyphs);
            let hay: Vec<char> = text.to_lowercase().chars().collect();
            let needle: Vec<char> = q.chars().collect();
            // Lowercasing can change lengths (rare); then the map is off and the page is skipped.
            if hay.len() != map.len() {
                continue;
            }
            let mut at = 0;
            while at + needle.len() <= hay.len() {
                if hay[at..at + needle.len()] == needle[..] {
                    let mut rects: Vec<Rect> = Vec::new();
                    for gi in map[at..at + needle.len()].iter().flatten() {
                        let r = glyphs[*gi].rect;
                        match rects.last_mut() {
                            Some(l) if (l.center().y - r.center().y).abs() < r.height() * 0.5 => {
                                *l = l.union(r)
                            }
                            _ => rects.push(r),
                        }
                    }
                    if !rects.is_empty() {
                        self.hits.push(Hit { page: n, rects });
                    }
                    at += needle.len();
                } else {
                    at += 1;
                }
            }
        }
        if jump && !self.hits.is_empty() {
            let first = self
                .hits
                .iter()
                .position(|h| h.page + 1 >= self.current_page)
                .unwrap_or(0);
            self.step_to(first);
        }
    }

    /// Every page in view is drawn from the current document.
    #[cfg(test)]
    pub fn rendered(&self) -> bool {
        let view = Rect::from_min_size(pos2(0.0, self.scroll_y), self.client);
        self.shown > 0
            && self
                .rects
                .iter()
                .enumerate()
                .filter(|(_, r)| r.intersects(view))
                .all(|(i, _)| self.tex.get(&i).is_some_and(|t| t.doc == self.shown))
    }

    /// The SyncTeX box on screen: page index and rect in points.
    #[cfg(test)]
    pub fn highlighted(&self) -> Option<(usize, Rect)> {
        self.highlight.map(|(p, r, _)| (p, r))
    }

    pub fn match_count(&self) -> usize {
        self.hits.len()
    }

    fn step_to(&mut self, index: usize) {
        if self.hits.is_empty() {
            return;
        }
        let i = index % self.hits.len();
        self.match_index = i + 1;
        let h = &self.hits[i];
        if let (Some(r), Some(s)) = (self.rects.get(h.page), self.scales.get(h.page)) {
            self.jump = Some(Jump::Smooth(
                r.top() + h.rects[0].top() * s - self.client.y / 3.0,
            ));
        }
    }

    pub fn search_step(&mut self, dir: isize) {
        let n = self.hits.len() as isize;
        if n > 0 {
            self.step_to((self.match_index as isize - 1 + dir).rem_euclid(n) as usize);
        }
    }

    pub fn clear_search(&mut self) {
        self.query.clear();
        self.hits.clear();
        self.match_index = 0;
        self.searching = false;
    }

    // ---- Drawing --------------------------------------------------------

    /// Asks for the renders a page needs at this scale; thumbnails are never dropped.
    fn want(&mut self, page: usize, scale: f32, thumb: bool, epoch: u64) {
        let have = if thumb {
            self.thumbs.get(&page)
        } else {
            self.tex.get(&page)
        };
        let fresh = have.is_some_and(|t| {
            t.doc == self.shown && t.dark == self.dark && (t.scale - scale).abs() <= scale * 0.005
        });
        if fresh || !self.asked.insert((page, thumb)) {
            return;
        }
        self.engine.send(Job::Render {
            doc: self.shown,
            page,
            scale,
            dark: self.dark,
            epoch: (!thumb).then_some(epoch),
            thumb,
        });
    }

    /// A thumbnail of a page, drawing it if needed; None until the first one arrives.
    pub fn thumbnail(
        &mut self,
        ctx: &egui::Context,
        page: usize,
    ) -> Option<(egui::TextureId, Vec2)> {
        let size = *self.pages.get(page)?;
        let scale = THUMB_W * ctx.pixels_per_point() / size.x;
        self.want(page, scale, true, 0);
        self.thumbs.get(&page).map(|t| (t.handle.id(), size))
    }

    pub fn show(&mut self, ui: &mut Ui, p: &Palette) -> Option<Event> {
        let ctx = ui.ctx().clone();
        self.receive(&ctx);
        let area = ui.max_rect();
        ui.painter().rect_filled(area, 0, p.pdf_ground);
        if self.pages.is_empty() {
            return None;
        }
        let client = area.size();
        if client != self.client {
            if matches!(self.fit, Fit::Width | Fit::Page)
                && (client.x - self.client.x).abs() >= 4.0
                && self.client.x > 0.0
            {
                self.pending_ratio
                    .get_or_insert(self.scroll_y / self.content.y.max(1.0));
            }
            self.client = client;
        }
        self.relayout();
        if let Some(n) = self.pending_page.take()
            && let Some(r) = self.rects.get(n.saturating_sub(1))
        {
            self.jump = Some(Jump::Offset(vec2(0.0, r.top() - 12.0)));
        }
        if let Some(ratio) = self.pending_ratio.take() {
            self.jump = Some(Jump::Offset(vec2(0.0, ratio * self.content.y)));
        }
        if std::mem::take(&mut self.pending_box)
            && let Some((page, r, _)) = self.highlight
            && let (Some(pr), Some(s)) = (self.rects.get(page), self.scales.get(page))
        {
            self.jump = Some(Jump::Smooth(
                pr.top() + r.top() * s - 3.0 - self.client.y / 3.0,
            ));
        }

        // A new zoom or pane size makes earlier render jobs moot.
        let ppp = ctx.pixels_per_point();
        let first = self.scales[0] * ppp;
        if (first - self.epoch_scale).abs() > 1e-4 {
            self.epoch_scale = first;
            self.engine.epoch.fetch_add(1, Ordering::Relaxed);
            self.asked.retain(|(_, thumb)| *thumb);
        }
        let epoch = self.engine.epoch.load(Ordering::Relaxed);

        let mut scroll = egui::ScrollArea::both()
            .id_salt(self.id)
            .auto_shrink([false, false]);
        let mut smooth = None;
        match self.jump.take() {
            Some(Jump::Offset(o)) => scroll = scroll.scroll_offset(o.max(Vec2::ZERO)),
            Some(Jump::Smooth(y)) => smooth = Some(y),
            None => {}
        }
        let mut event = None;
        let out = scroll.show_viewport(ui, |ui, viewport| {
            let origin = ui.max_rect().min;
            let (resp_rect, resp) = ui.allocate_exact_size(self.content, Sense::click_and_drag());
            let _ = resp_rect;
            if let Some(y) = smooth {
                let r = Rect::from_min_size(origin + vec2(viewport.left(), y), viewport.size());
                ui.scroll_to_rect_animation(
                    r,
                    Some(egui::Align::TOP),
                    egui::style::ScrollAnimation::duration(if crate::theme::reduced_motion() {
                        0.0
                    } else {
                        0.3
                    }),
                );
            }
            let near = viewport.expand2(vec2(0.0, viewport.height() * 0.5));
            let keep = viewport.expand2(vec2(0.0, viewport.height() * 2.0));
            // The pages on screen are asked for first, then the ones half a screen away, and
            // their text layers last: the engine works through its queue in order.
            let (mut on_screen, mut around) = (Vec::new(), Vec::new());
            let selected = self.selection.map(|(page, ..)| page);
            for i in 0..self.pages.len() {
                let r = self.rects[i];
                if !keep.intersects(r) {
                    self.tex.remove(&i);
                    // A search needs every page's text, a selection its own page's.
                    if self.query.is_empty() && selected != Some(i) {
                        self.deco.remove(&i);
                    }
                } else if viewport.intersects(r) {
                    on_screen.push(i);
                } else if near.intersects(r) {
                    around.push(i);
                }
            }
            for &i in on_screen.iter().chain(&around) {
                self.want(i, self.scales[i] * ppp, false, epoch);
            }
            for &i in on_screen.iter().chain(&around) {
                if !self.deco.contains_key(&i) && self.decorating.insert(i) {
                    self.engine.send(Job::Decorate {
                        doc: self.shown,
                        page: i,
                    });
                }
            }
            let painter = ui.painter();
            for i in 0..self.pages.len() {
                let r = self.rects[i];
                if !near.intersects(r) {
                    continue;
                }
                let sr = r.translate(origin.to_vec2());
                if !ui.is_rect_visible(sr) {
                    continue;
                }
                // box-shadow: 0 1px 3px rgba(0,0,0,.18), 0 6px 18px -6px rgba(0,0,0,.25)
                painter.add(
                    Shadow {
                        offset: [0, 6],
                        blur: 18,
                        spread: 0,
                        color: Color32::from_black_alpha(40),
                    }
                    .as_shape(sr, 0),
                );
                painter.add(
                    Shadow {
                        offset: [0, 1],
                        blur: 3,
                        spread: 0,
                        color: Color32::from_black_alpha(46),
                    }
                    .as_shape(sr, 0),
                );
                painter.rect_filled(sr, 0, Color32::WHITE);
                if let Some(t) = self.tex.get(&i) {
                    painter.image(
                        t.handle.id(),
                        sr,
                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
            }
            self.overlays(ui, p, origin, &resp, &mut event);
            resp
        });
        self.scroll_y = out.state.clamped_offset().y;
        // The page in the middle of the pane.
        let mid = self.scroll_y + self.client.y / 2.0;
        if let Some(i) = self.rects.iter().position(|r| r.bottom() > mid) {
            self.current_page = i + 1;
        }
        let resp = out.inner;
        self.focused = resp.has_focus();
        // Ctrl+wheel and pinch zoom from where they are.
        if resp.hovered() || resp.contains_pointer() {
            let z = ui.input(|i| i.zoom_delta());
            if z != 1.0 {
                // Unrounded, so the small steps of a slow pinch add up.
                let now = match self.fit {
                    Fit::Zoom(f) => f,
                    _ => self.percent as f32 / 100.0,
                };
                self.set_fit(Fit::Zoom(now * z));
            }
        }
        if resp.has_focus()
            && self.selection.is_some()
            && ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)))
        {
            ctx.copy_text(self.selected_text());
        }
        if self.highlight.is_some() || !self.asked.is_empty() || !self.decorating.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
        event
    }

    /// Links, the selection, search hits and the SyncTeX box over the pages, and the clicks on them.
    fn overlays(
        &mut self,
        ui: &Ui,
        p: &Palette,
        origin: egui::Pos2,
        resp: &egui::Response,
        event: &mut Option<Event>,
    ) {
        let painter = ui.painter();
        let accent = p.accent;
        let screen = |page: usize, r: Rect, rects: &[Rect], scales: &[f32]| {
            let s = scales[page];
            Rect::from_min_size(
                rects[page].min + origin.to_vec2() + r.min.to_vec2() * s,
                r.size() * s,
            )
        };
        // Which page and point the pointer is over.
        let at = |pos: egui::Pos2| -> Option<(usize, egui::Pos2)> {
            let c = pos - origin.to_vec2();
            let i = self.rects.iter().position(|r| r.contains(c))?;
            let s = self.scales[i];
            Some((i, ((c - self.rects[i].min) / s).to_pos2()))
        };
        let hover = resp.hover_pos().and_then(at);
        let mut over_link = None;
        if let Some((i, pt)) = hover
            && let Some((glyphs, links)) = self.deco.get(&i)
        {
            if let Some(l) = links.iter().find(|l| l.rect.contains(pt)) {
                over_link = Some(l.target.clone());
                let r = screen(i, l.rect, &self.rects, &self.scales);
                painter.rect(
                    r,
                    2,
                    accent.gamma_multiply(0.14),
                    Stroke::new(1.0, accent.gamma_multiply(0.5)),
                    StrokeKind::Inside,
                );
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                if let Target::Url(u) = &l.target {
                    resp.clone().on_hover_text(u);
                }
            } else if glyphs.iter().any(|g| g.rect.contains(pt)) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            }
        }

        // Selecting text: within the page the drag started on.
        if resp.drag_started()
            && let Some((i, pt)) = resp.interact_pointer_pos().and_then(at)
            && let Some(g) = self.glyph_near(i, pt)
        {
            self.selection = Some((i, g, g));
            resp.request_focus();
        }
        if resp.dragged()
            && let Some((page, start, _)) = self.selection
            && let Some(pos) = resp.interact_pointer_pos()
        {
            let c = pos - origin.to_vec2();
            let r = self.rects[page];
            let pt = ((c - r.min) / self.scales[page]).to_pos2();
            if let Some(g) = self.glyph_near(page, pt) {
                self.selection = Some((page, start, g));
            }
        }
        if let Some((page, a, b)) = self.selection
            && let Some((glyphs, _)) = self.deco.get(&page)
        {
            for g in &glyphs[a.min(b)..=a.max(b).min(glyphs.len() - 1)] {
                painter.rect_filled(
                    screen(page, g.rect, &self.rects, &self.scales),
                    0,
                    accent.gamma_multiply(0.35),
                );
            }
        }

        // Search hits: yellow, the current one orange.
        for (n, h) in self.hits.iter().enumerate() {
            if h.page >= self.rects.len() {
                continue;
            }
            let current = n + 1 == self.match_index;
            for r in &h.rects {
                // The em box is taller than the letters; .hit draws a little over the line.
                let sr = screen(h.page, *r, &self.rects, &self.scales);
                if !ui.is_rect_visible(sr) {
                    continue;
                }
                if current {
                    painter.rect(
                        sr,
                        2,
                        Color32::from_rgba_unmultiplied(0xf0, 0x8a, 0x24, 140),
                        Stroke::new(1.5, Color32::from_rgb(0xf0, 0x8a, 0x24)),
                        StrokeKind::Outside,
                    );
                } else {
                    painter.rect_filled(
                        sr,
                        2,
                        Color32::from_rgba_unmultiplied(0xf2, 0xc2, 0x30, 107),
                    );
                }
            }
        }

        // The SyncTeX box: 1.8 s, fading over the last 40 %.
        if let Some((page, r, t0)) = self.highlight
            && page < self.rects.len()
        {
            let t = (ui.input(|i| i.time) - t0) as f32;
            if t > 1.8 {
                self.highlight = None;
            } else {
                let o = if t < 1.08 {
                    1.0
                } else {
                    1.0 - (t - 1.08) / 0.72
                };
                let sr = screen(page, r, &self.rects, &self.scales).expand2(vec2(4.0, 3.0));
                painter.rect(
                    sr,
                    CornerRadius::same(3),
                    accent.gamma_multiply(0.22 * o),
                    Stroke::new(1.5, accent.gamma_multiply(o)),
                    StrokeKind::Outside,
                );
            }
        }

        // Clicks: a link goes there; a double-click or Ctrl/Cmd-click asks for the source.
        let sync = resp.double_clicked() || (resp.clicked() && ui.input(|i| i.modifiers.command));
        if sync {
            self.selection = None;
            if let Some((i, pt)) = resp.interact_pointer_pos().and_then(at) {
                *event = Some(Event::SourceClick(i + 1, pt.x, pt.y));
            }
        } else if resp.clicked() {
            resp.request_focus();
            if self.selection.is_some_and(|(_, a, b)| a == b) || self.selection.is_none() {
                self.selection = None;
            }
            if let Some(t) = over_link
                && let Some(url) = self.goto_target(&t)
                && (url.starts_with("http://") || url.starts_with("https://"))
            {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
        }
    }

    /// The glyph under a point, or the nearest one on the page.
    fn glyph_near(&self, page: usize, pt: egui::Pos2) -> Option<usize> {
        let (glyphs, _) = self.deco.get(&page)?;
        glyphs
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.rect
                    .distance_sq_to_pos(pt)
                    .total_cmp(&b.rect.distance_sq_to_pos(pt))
            })
            .map(|(i, _)| i)
    }

    pub fn selected_text(&self) -> String {
        let Some((page, a, b)) = self.selection else {
            return String::new();
        };
        let Some((glyphs, _)) = self.deco.get(&page) else {
            return String::new();
        };
        page_text(&glyphs[a.min(b)..=a.max(b).min(glyphs.len() - 1)]).0
    }
}
