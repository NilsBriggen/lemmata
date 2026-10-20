//! Lucide icons drawn as strokes, so they stay sharp at any scale and take the text colour like the
//! Svelte UI's `currentColor` SVGs. The path data is generated into `icons_data.rs`.

use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2};
use eframe::epaint::PathShape;

#[allow(dead_code)]
mod data {
    include!("icons_data.rs");
}
#[allow(unused_imports)] // shortcut: used by the panels from M2 on.
pub use data::*;

pub type Icon = &'static [&'static str];

/// One subpath in the icon's 24×24 box.
#[derive(Debug, PartialEq)]
pub struct Line {
    pub points: Vec<Pos2>,
    pub closed: bool,
}

/// Lucide draws with `stroke-width: 2` on a 24 box, round caps and joins.
const STROKE: f32 = 2.0;

/// Allocates a `size`×`size` icon and paints it.
pub fn show(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint(ui.painter(), rect, icon, color);
    response
}

pub fn paint(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let scale = rect.width() / 24.0;
    let width = STROKE * scale;
    let stroke = Stroke::new(width, color);
    for d in icon {
        for line in parse(d) {
            let points: Vec<Pos2> = line
                .points
                .iter()
                .map(|p| rect.min + p.to_vec2() * scale)
                .collect();
            if !line.closed {
                // Round caps.
                for end in [points.first(), points.last()].into_iter().flatten() {
                    painter.circle_filled(*end, width / 2.0, color);
                }
            }
            let shape = if line.closed {
                PathShape::closed_line(points, stroke)
            } else {
                PathShape::line(points, stroke)
            };
            painter.add(Shape::Path(shape));
        }
    }
}

/// Parses SVG path data into polylines, flattening curves and arcs.
pub fn parse(d: &str) -> Vec<Line> {
    let mut p = Parser {
        s: d.as_bytes(),
        i: 0,
    };
    let mut lines: Vec<Line> = Vec::new();
    let mut cur = pos2(0.0, 0.0);
    let mut start = cur;
    let mut last_ctrl: Option<Pos2> = None;
    let mut cmd = b'M';
    let mut pts: Vec<Pos2> = Vec::new();

    let flush = |pts: &mut Vec<Pos2>, lines: &mut Vec<Line>, closed: bool| {
        if pts.len() > 1 {
            lines.push(Line {
                points: std::mem::take(pts),
                closed,
            });
        }
        pts.clear();
    };

    while p.skip_ws() {
        if p.peek().is_ascii_alphabetic() {
            cmd = p.next();
        } else if cmd == b'M' {
            cmd = b'L';
        } else if cmd == b'm' {
            cmd = b'l';
        }
        let rel = cmd.is_ascii_lowercase();
        let base = if rel { cur.to_vec2() } else { Vec2::ZERO };
        let pt = |p: &mut Parser| -> Pos2 {
            let x = p.num();
            let y = p.num();
            pos2(x, y) + base
        };
        let mut ctrl = None;
        // After a Z the next segment starts from the subpath's start.
        if pts.is_empty() && !matches!(cmd, b'M' | b'm' | b'Z' | b'z') {
            pts.push(cur);
        }
        match cmd.to_ascii_uppercase() {
            b'M' => {
                flush(&mut pts, &mut lines, false);
                cur = pt(&mut p);
                start = cur;
                pts.push(cur);
            }
            b'L' => {
                cur = pt(&mut p);
                pts.push(cur);
            }
            b'H' => {
                let x = p.num();
                cur.x = if rel { cur.x + x } else { x };
                pts.push(cur);
            }
            b'V' => {
                let y = p.num();
                cur.y = if rel { cur.y + y } else { y };
                pts.push(cur);
            }
            b'C' | b'S' => {
                let c1 = if cmd.eq_ignore_ascii_case(&b'C') {
                    pt(&mut p)
                } else {
                    last_ctrl.map_or(cur, |c| cur + (cur - c))
                };
                let c2 = pt(&mut p);
                let end = pt(&mut p);
                cubic(&mut pts, cur, c1, c2, end);
                ctrl = Some(c2);
                cur = end;
            }
            b'Q' | b'T' => {
                let c = if cmd.eq_ignore_ascii_case(&b'Q') {
                    pt(&mut p)
                } else {
                    last_ctrl.map_or(cur, |c| cur + (cur - c))
                };
                let end = pt(&mut p);
                let c1 = cur + (c - cur) * (2.0 / 3.0);
                let c2 = end + (c - end) * (2.0 / 3.0);
                cubic(&mut pts, cur, c1, c2, end);
                ctrl = Some(c);
                cur = end;
            }
            b'A' => {
                let rx = p.num();
                let ry = p.num();
                let rot = p.num();
                let large = p.flag();
                let sweep = p.flag();
                let end = pt(&mut p);
                arc(&mut pts, cur, rx, ry, rot, large, sweep, end);
                cur = end;
            }
            b'Z' => {
                cur = start;
                flush(&mut pts, &mut lines, true);
            }
            _ => break,
        }
        last_ctrl = ctrl;
    }
    flush(&mut pts, &mut lines, false);
    lines
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> u8 {
        self.s.get(self.i).copied().unwrap_or(0)
    }
    fn next(&mut self) -> u8 {
        let c = self.peek();
        self.i += 1;
        c
    }
    /// Skips separators; false at the end.
    fn skip_ws(&mut self) -> bool {
        while matches!(self.peek(), b' ' | b',' | b'\n' | b'\t') {
            self.i += 1;
        }
        self.i < self.s.len()
    }
    fn num(&mut self) -> f32 {
        self.skip_ws();
        let start = self.i;
        if matches!(self.peek(), b'-' | b'+') {
            self.i += 1;
        }
        let mut dot = false;
        while let c @ (b'0'..=b'9' | b'.') = self.peek() {
            if c == b'.' {
                if dot {
                    break; // ".5.5" is two numbers
                }
                dot = true;
            }
            self.i += 1;
        }
        if matches!(self.peek(), b'e' | b'E') {
            self.i += 1;
            if matches!(self.peek(), b'-' | b'+') {
                self.i += 1;
            }
            while self.peek().is_ascii_digit() {
                self.i += 1;
            }
        }
        std::str::from_utf8(&self.s[start..self.i])
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0)
    }
    /// Arc flags are one character and may be written without separators ("a1 1 0 011 1").
    fn flag(&mut self) -> bool {
        self.skip_ws();
        self.next() == b'1'
    }
}

fn cubic(pts: &mut Vec<Pos2>, a: Pos2, b: Pos2, c: Pos2, d: Pos2) {
    const N: usize = 12;
    for i in 1..=N {
        let t = i as f32 / N as f32;
        let u = 1.0 - t;
        let p = a.to_vec2() * (u * u * u)
            + b.to_vec2() * (3.0 * u * u * t)
            + c.to_vec2() * (3.0 * u * t * t)
            + d.to_vec2() * (t * t * t);
        pts.push(p.to_pos2());
    }
}

/// SVG endpoint arc to points (SVG 1.1 implementation notes, F.6.5).
#[allow(clippy::too_many_arguments)]
fn arc(
    pts: &mut Vec<Pos2>,
    from: Pos2,
    rx: f32,
    ry: f32,
    rot: f32,
    large: bool,
    sweep: bool,
    to: Pos2,
) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 || from == to {
        pts.push(to);
        return;
    }
    let (sin, cos) = rot.to_radians().sin_cos();
    let h = (from - to) / 2.0;
    let x1 = cos * h.x + sin * h.y;
    let y1 = -sin * h.x + cos * h.y;
    let l = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if l > 1.0 {
        rx *= l.sqrt();
        ry *= l.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut k = (num / den).max(0.0).sqrt();
    if large == sweep {
        k = -k;
    }
    let cx1 = k * rx * y1 / ry;
    let cy1 = -k * ry * x1 / rx;
    let mid = (from.to_vec2() + to.to_vec2()) / 2.0;
    let cx = cos * cx1 - sin * cy1 + mid.x;
    let cy = sin * cx1 + cos * cy1 + mid.y;
    let angle = |ux: f32, uy: f32| uy.atan2(ux);
    let t1 = angle((x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut dt = angle((-x1 - cx1) / rx, (-y1 - cy1) / ry) - t1;
    let tau = std::f32::consts::TAU;
    if sweep && dt < 0.0 {
        dt += tau;
    } else if !sweep && dt > 0.0 {
        dt -= tau;
    }
    let n = ((dt.abs() / tau) * 32.0).ceil().max(2.0) as usize;
    for i in 1..=n {
        let t = t1 + dt * i as f32 / n as f32;
        let (s, c) = t.sin_cos();
        pts.push(pos2(
            cx + cos * rx * c - sin * ry * s,
            cy + sin * rx * c + cos * ry * s,
        ));
    }
    *pts.last_mut().unwrap() = to;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Pos2, b: Pos2) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn relative_lines_and_compact_numbers() {
        // From a-arrow-down: "m2 16 4.039-9.69a.5.5 0 0 1 .923 0L11 16".
        let lines = parse("m2 16 4.039-9.69a.5.5 0 0 1 .923 0L11 16");
        assert_eq!(lines.len(), 1);
        let pts = &lines[0].points;
        assert!(near(pts[0], pos2(2.0, 16.0)));
        assert!(near(pts[1], pos2(6.039, 6.31)));
        assert!(near(*pts.last().unwrap(), pos2(11.0, 16.0)));
    }

    #[test]
    fn circle_as_two_arcs_stays_on_the_circle() {
        let lines = parse("M2 12A10 10 0 1 0 22 12A10 10 0 1 0 2 12Z");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].closed);
        for p in &lines[0].points {
            assert!(
                ((*p - pos2(12.0, 12.0)).length() - 10.0).abs() < 1e-2,
                "{p:?}"
            );
        }
        // Both halves: points on either side of the centre.
        assert!(lines[0].points.iter().any(|p| p.y > 20.0));
        assert!(lines[0].points.iter().any(|p| p.y < 4.0));
    }

    #[test]
    fn flags_without_separators() {
        let a = parse("M0 0a1 1 0 011 1");
        let b = parse("M0 0a1 1 0 0 1 1 1");
        assert_eq!(a, b);
    }

    #[test]
    fn every_icon_parses_inside_its_box() {
        for icon in [
            ARROW_DOWN,
            BELL,
            BOLD,
            CIRCLE_CHECK,
            GIT_BRANCH,
            HISTORY,
            SETTINGS,
            SIGMA,
            TABLE,
            X,
        ] {
            for d in icon {
                let lines = parse(d);
                assert!(!lines.is_empty(), "{d}");
                for p in lines.iter().flat_map(|l| &l.points) {
                    assert!(
                        (-0.5..=24.5).contains(&p.x) && (-0.5..=24.5).contains(&p.y),
                        "{d}: {p:?}"
                    );
                }
            }
        }
    }
}
