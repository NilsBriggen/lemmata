//! In-process reader for the `.synctex(.gz)` file TeX writes next to the PDF.
//!
//! The file is a flat list of records. Boxes (`[` vbox, `(` hbox) nest until
//! their `]` / `)`, everything else (`h`/`v` void boxes, `x` current, `k`
//! kern, `g` glue, `$` math) is a leaf. All coordinates are converted to PDF
//! points from the top-left corner while parsing, so queries never convert.

use crate::{PdfBox, SourceLocation};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const NONE: u32 = u32::MAX;
/// TeX scaled points per PDF point (65536 * 72.27 / 72).
const SP_PER_BP: f64 = 65781.76;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    VBox,
    HBox,
    Void,
    Kern,
    Glue,
    Math,
    /// `x`: the position TeX was at when it read a source line.
    Current,
}

#[derive(Clone, Debug)]
struct Node {
    kind: Kind,
    tag: u32,
    line: u32,
    page: u32,
    parent: u32,
    child: u32,
    next: u32,
    h: f32,
    v: f32,
    w: f32,
    height: f32,
    depth: f32,
}

impl Node {
    /// Where a hit on this node is shown: a kern is drawn from where it starts.
    fn visible_h(&self) -> f32 {
        if self.kind == Kind::Kern {
            self.h - self.w
        } else {
            self.h
        }
    }

    /// Horizontal extent a click can be measured against.
    fn h_span(&self) -> (f32, f32) {
        match self.kind {
            Kind::Kern | Kind::Glue | Kind::Math | Kind::Current => (self.h, self.h),
            _ => (self.h, self.h + self.w),
        }
    }

    fn v_span(&self) -> (f32, f32) {
        match self.kind {
            Kind::Kern | Kind::Glue | Kind::Math | Kind::Current => (self.v, self.v),
            _ => (self.v - self.height, self.v + self.depth),
        }
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        let (h0, h1) = (self.h, self.h + self.w);
        let (v0, v1) = (self.v - self.height, self.v + self.depth);
        x >= h0.min(h1) && x <= h0.max(h1) && y >= v0 && y <= v1
    }
}

#[derive(Debug)]
struct Sheet {
    page: u32,
    /// First top-level node.
    first: u32,
    /// The nodes of this page are `nodes[start..end]`.
    start: usize,
    end: usize,
}

#[derive(Debug)]
pub struct Index {
    nodes: Vec<Node>,
    sheets: Vec<Sheet>,
    /// For each (input tag, line): the boxes that are on that line or hold a
    /// node of it, in document order.
    by_line: HashMap<(u32, u32), Vec<u32>>,
    /// (tag, path); relative paths are already joined to the synctex directory.
    inputs: Vec<(u32, PathBuf)>,
}

fn dist(span: (f32, f32), p: f32) -> f32 {
    if p < span.0 {
        span.0 - p
    } else if p > span.1 {
        p - span.1
    } else {
        0.0
    }
}

impl Index {
    /// `base` resolves relative `Input:` paths.
    pub fn parse(text: &str, base: &Path) -> Option<Index> {
        let (mut unit, mut mag, mut xoff, mut yoff) = (1.0f64, 1000.0f64, 0.0f64, 0.0f64);
        let mut inputs = Vec::new();
        let mut nodes: Vec<Node> = Vec::new();
        let mut sheets = Vec::new();
        let mut in_content = false;
        let mut page = 0u32;
        // Open boxes, and the last node added at each level (sheet level first).
        let mut open: Vec<u32> = Vec::new();
        let mut last: Vec<u32> = vec![NONE];
        let mut saw_header = false;

        for raw in text.lines() {
            if !saw_header {
                saw_header = true;
                if !raw.starts_with("SyncTeX Version") {
                    return None;
                }
                continue;
            }
            let Some(c) = raw.chars().next() else {
                continue;
            };
            let rest = &raw[c.len_utf8()..];
            // Inputs are listed before the content and again after it (aux files).
            if let Some(r) = raw.strip_prefix("Input:") {
                let (tag, path) = r.split_once(':')?;
                inputs.push((tag.parse().ok()?, base.join(path)));
                continue;
            }
            if !in_content {
                let num = |k: &str| {
                    raw.strip_prefix(k)
                        .and_then(|v| v.trim().parse::<f64>().ok())
                };
                if raw == "Content:" {
                    in_content = true;
                } else if let Some(v) = num("Magnification:") {
                    mag = v;
                } else if let Some(v) = num("Unit:") {
                    unit = v;
                } else if let Some(v) = num("X Offset:") {
                    xoff = v;
                } else if let Some(v) = num("Y Offset:") {
                    yoff = v;
                }
                continue;
            }
            let f = unit * mag / 1000.0 / SP_PER_BP;
            let (ox, oy) = (xoff / SP_PER_BP, yoff / SP_PER_BP);
            match c {
                '{' => {
                    page = rest.trim().parse().ok()?;
                    sheets.push(Sheet {
                        page,
                        first: NONE,
                        start: nodes.len(),
                        end: usize::MAX,
                    });
                    open.clear();
                    last = vec![NONE];
                }
                '}' => {
                    if let Some(s) = sheets.last_mut() {
                        s.end = nodes.len();
                    }
                    open.clear();
                    last = vec![NONE];
                }
                ']' | ')' => {
                    if open.pop().is_some() {
                        last.pop();
                    }
                }
                '[' | '(' | 'h' | 'v' | 'x' | 'k' | 'g' | '$' => {
                    let mut parts = rest.split(':');
                    let mut ids = parts.next()?.split(',');
                    let tag: u32 = ids.next()?.parse().ok()?;
                    let line: u32 = ids.next()?.parse().ok()?;
                    let mut pos = parts.next()?.split(',');
                    let h = pos.next()?.parse::<i64>().ok()? as f64;
                    let v = pos.next()?.parse::<i64>().ok()? as f64;
                    // Width, height and depth, as many as the record has.
                    let mut dims = [0.0f64; 3];
                    for (slot, n) in dims
                        .iter_mut()
                        .zip(parts.next().into_iter().flat_map(|s| s.split(',')))
                    {
                        *slot = n.parse::<i64>().ok()? as f64 * f;
                    }
                    let d = |i: usize| dims[i];
                    let kind = match c {
                        '[' => Kind::VBox,
                        '(' => Kind::HBox,
                        'h' | 'v' => Kind::Void,
                        'k' => Kind::Kern,
                        'g' => Kind::Glue,
                        '$' => Kind::Math,
                        _ => Kind::Current,
                    };
                    let idx = nodes.len() as u32;
                    let parent = open.last().copied().unwrap_or(NONE);
                    nodes.push(Node {
                        kind,
                        tag,
                        line,
                        page,
                        parent,
                        child: NONE,
                        next: NONE,
                        h: (h * f + ox) as f32,
                        v: (v * f + oy) as f32,
                        w: d(0) as f32,
                        height: if kind == Kind::Kern { 0.0 } else { d(1) as f32 },
                        depth: if kind == Kind::Kern { 0.0 } else { d(2) as f32 },
                    });
                    // Link as the next sibling, or as the first child of the open box.
                    let prev = *last.last()?;
                    if prev != NONE {
                        nodes[prev as usize].next = idx;
                    } else if parent != NONE {
                        nodes[parent as usize].child = idx;
                    } else if let Some(s) = sheets.last_mut() {
                        s.first = idx;
                    }
                    *last.last_mut()? = idx;
                    if matches!(c, '[' | '(') {
                        open.push(idx);
                        last.push(NONE);
                    }
                }
                _ => {}
            }
        }
        if !in_content || sheets.is_empty() {
            return None;
        }
        if let Some(s) = sheets.last_mut().filter(|s| s.end == usize::MAX) {
            s.end = nodes.len();
        }
        let mut by_line: HashMap<(u32, u32), Vec<u32>> = HashMap::new();
        for (i, n) in nodes.iter().enumerate() {
            let boxed = if matches!(n.kind, Kind::VBox | Kind::HBox) {
                i as u32
            } else {
                n.parent
            };
            if n.line > 0 && boxed != NONE {
                by_line.entry((n.tag, n.line)).or_default().push(boxed);
            }
        }
        for v in by_line.values_mut() {
            v.sort_unstable();
            v.dedup();
        }
        Some(Index {
            nodes,
            sheets,
            by_line,
            inputs,
        })
    }

    /// Tags of the recorded inputs that are the same file as `path`.
    fn tags_for(&self, path: &Path) -> Vec<u32> {
        let want = crate::normalize(&path.to_string_lossy());
        let want_canon = want.canonicalize().ok();
        let name = want.file_name();
        self.inputs
            .iter()
            .filter(|(_, p)| {
                let p = crate::normalize(&p.to_string_lossy());
                p == want
                    || (p.file_name() == name
                        && want_canon.is_some()
                        && p.canonicalize().ok() == want_canon)
            })
            .map(|(t, _)| *t)
            .collect()
    }

    pub fn forward(&self, tex_file: &Path, line: u32) -> Vec<PdfBox> {
        let tags = self.tags_for(tex_file);
        let exact = self.hits(&tags, line);
        if !exact.is_empty() {
            return exact;
        }
        let mut lines: Vec<u32> = self
            .by_line
            .keys()
            .filter(|(t, _)| tags.contains(t))
            .map(|&(_, l)| l)
            .collect();
        lines.sort_unstable();
        lines.dedup();
        // Nearest line with a hit, a later one if two are equally far (like synctex).
        lines.sort_by_key(|&l| (l.abs_diff(line), l < line));
        for l in lines.into_iter().take(32) {
            let hits = self.hits(&tags, l);
            if !hits.is_empty() {
                return hits;
            }
        }
        Vec::new()
    }

    /// The boxes holding source line `line`, in document order.
    fn hits(&self, tags: &[u32], line: u32) -> Vec<PdfBox> {
        let mine = |n: &Node| n.line == line && tags.contains(&n.tag);
        let mut out = Vec::new();
        let mut boxes: Vec<u32> = tags
            .iter()
            .filter_map(|&t| self.by_line.get(&(t, line)))
            .flatten()
            .copied()
            .collect();
        boxes.sort_unstable();
        boxes.dedup();
        for b in boxes.into_iter().map(|i| &self.nodes[i as usize]) {
            // `best` is the node of the line to point at: a lone "current" mark
            // gives way to what follows it, but only before the first kern, glue
            // or math node; anything of another line in between starts over.
            let (mut best, mut seen_other) = (None::<&Node>, false);
            let (mut first, mut stray) = (None::<&Node>, false);
            let kids = self.children(b.child, |k, g| mine(k) && mine(g));
            for (_, n) in kids {
                first.get_or_insert(n);
                match n.kind {
                    Kind::VBox | Kind::HBox => {}
                    Kind::Void => {
                        seen_other = true;
                        if !mine(n) {
                            if best.is_none_or(|p| p.kind == Kind::Current) {
                                best = None;
                            }
                            stray = true;
                        }
                    }
                    Kind::Current => {
                        if mine(n) && best.is_none_or(|p| p.kind == Kind::Current && !seen_other) {
                            best = Some(n);
                        }
                    }
                    Kind::Kern | Kind::Glue | Kind::Math => {
                        seen_other = true;
                        if !mine(n) {
                            if best.is_none_or(|p| p.kind == Kind::Current) {
                                best = None;
                            }
                            stray = true;
                        } else if best.is_none_or(|p| p.kind == Kind::Current) {
                            best = Some(n);
                        }
                    }
                }
            }
            let whole_box = b.kind == Kind::HBox && mine(b) && first.is_some_and(mine) && !stray;
            let (x, y, mut shown) = if whole_box {
                // A box whose own line this is is framed by the line it sits in.
                (b.h, b.v, self.parent_hbox(b).unwrap_or(b))
            } else if let Some(n) = best {
                (n.visible_h(), n.v, b)
            } else {
                continue;
            };
            // Boxes wrapping nothing but this one are the same line.
            while let Some(p) = self.parent_hbox(shown).filter(|p| self.only_child(p)) {
                shown = p;
            }
            out.push(PdfBox {
                page: b.page,
                x: x as f64,
                y: y as f64,
                h: shown.h as f64,
                v: (shown.v + shown.depth) as f64,
                width: shown.w as f64,
                height: (shown.height + shown.depth) as f64,
            });
        }
        out
    }

    pub fn backward(&self, page: u32, x: f64, y: f64) -> Option<SourceLocation> {
        let (x, y) = (x as f32, y as f32);
        let n = if self.over_text(page, x, y) {
            self.descend(page, x, y)?
        } else {
            self.nearest(self.page_nodes(page).iter(), x, y)?
        };
        let tag = self.inputs.iter().find(|(t, _)| *t == n.tag)?;
        Some(SourceLocation {
            file: crate::normalize(&tag.1.to_string_lossy()),
            line: n.line,
            column: None,
        })
    }

    /// Off the text: the closest kern, glue or math node on the page.
    fn nearest<'a>(
        &self,
        nodes: impl Iterator<Item = &'a Node>,
        x: f32,
        y: f32,
    ) -> Option<&'a Node> {
        let mut best: Option<(f32, &Node)> = None;
        for n in nodes {
            if matches!(n.kind, Kind::VBox | Kind::HBox | Kind::Void)
                || n.line == 0
                || self.is_echo(n)
            {
                continue;
            }
            let d = (n.h - x).powi(2) + (n.v - y).powi(2);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, n));
            }
        }
        best.map(|b| b.1)
    }

    /// A "current" mark with the line of its own box only repeats the box.
    fn is_echo(&self, n: &Node) -> bool {
        n.kind == Kind::Current
            && self
                .nodes
                .get(n.parent as usize)
                .is_some_and(|p| (p.tag, p.line) == (n.tag, n.line))
    }

    /// Walks down the boxes, always into the child nearest the point.
    fn descend(&self, page: u32, x: f32, y: f32) -> Option<&Node> {
        let mut level = self.sheet(page)?.first;
        let mut along_h = false;
        let mut hit = None;
        while level != NONE {
            let kids = self.children(level, |k, g| (k.tag, k.line) == (g.tag, g.line));
            let only_echoes = kids.iter().all(|(_, n)| self.is_echo(n));
            let mut best: Option<(f32, u32)> = None;
            for (i, n) in kids {
                if self.is_echo(n) && !only_echoes {
                    continue;
                }
                let d = if along_h {
                    dist(n.h_span(), x)
                } else {
                    dist(n.v_span(), y)
                };
                if best.is_none_or(|(bd, _)| d < bd - 1e-3) {
                    best = Some((d, i));
                }
            }
            let n = &self.nodes[best?.1 as usize];
            hit = Some(n);
            level = n.child;
            along_h = n.kind != Kind::VBox;
        }
        hit
    }
}

impl Index {
    fn parent_hbox(&self, n: &Node) -> Option<&Node> {
        self.nodes
            .get(n.parent as usize)
            .filter(|p| p.kind == Kind::HBox)
    }

    fn only_child(&self, p: &Node) -> bool {
        p.child != NONE && self.nodes[p.child as usize].next == NONE
    }

    /// The nodes from `first` on, without the kern and glue that close a
    /// line (`\parfillskip`) when `closing` accepts the pair: they are not a target.
    fn children(&self, first: u32, closing: impl Fn(&Node, &Node) -> bool) -> Vec<(u32, &Node)> {
        let mut kids = Vec::new();
        let mut i = first;
        while i != NONE {
            kids.push((i, &self.nodes[i as usize]));
            i = self.nodes[i as usize].next;
        }
        if let [.., (_, k), (_, g)] = kids[..]
            && k.kind == Kind::Kern
            && g.kind == Kind::Glue
            && closing(k, g)
        {
            kids.truncate(kids.len() - 2);
        }
        kids
    }
}

impl Index {
    /// Whether the point is inside a horizontal box, i.e. on a line of text.
    pub fn over_text(&self, page: u32, x: f32, y: f32) -> bool {
        self.page_nodes(page)
            .iter()
            .any(|n| n.kind == Kind::HBox && n.contains(x, y))
    }

    fn sheet(&self, page: u32) -> Option<&Sheet> {
        self.sheets.iter().find(|s| s.page == page)
    }

    fn page_nodes(&self, page: u32) -> &[Node] {
        self.sheet(page)
            .map_or(&[], |s| &self.nodes[s.start..s.end.min(self.nodes.len())])
    }
}
