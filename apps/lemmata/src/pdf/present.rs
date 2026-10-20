//! Presenting a Beamer PDF (Presentation.svelte, PresenterView.svelte, presenter/layout.ts): the
//! slides full screen in the main window, and with P a presenter window with the current and next
//! slide, the speaker notes, a timer and the clock. Both are drawn from the same state each frame,
//! so they always show the same slide; the presenter window is a second egui viewport.

use super::engine::{Done, Engine, Job};
use crate::theme;
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Key, Rect, RichText, Sense, Stroke, TextureHandle, Ui,
    Vec2, ViewportBuilder, ViewportCommand, ViewportId, pos2, vec2,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

/// Which part of a page to show: Beamer's "show notes on second screen=right" puts the slide on
/// the left half of each page and its notes on the right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Region {
    Full,
    Left,
    Right,
}

/// Slides are 4:3 (1.33) or 16:9 (1.78); with notes beside them the page is about twice that.
pub fn has_notes_page(width: f32, height: f32) -> bool {
    height > 0.0 && width / height > 2.2
}

/// Where the slide and the notes are on a page.
pub fn regions(width: f32, height: f32) -> (Region, Option<Region>) {
    if has_notes_page(width, height) {
        (Region::Left, Some(Region::Right))
    } else {
        (Region::Full, None)
    }
}

/// The size of a region of a page and where it starts, in the page's own units.
pub fn region_size(width: f32, height: f32, region: Region) -> (f32, f32, f32) {
    match region {
        Region::Full => (width, height, 0.0),
        Region::Left => (width / 2.0, height, 0.0),
        Region::Right => (width / 2.0, height, width / 2.0),
    }
}

/// The scale that fits a region into a box without cutting it off.
pub fn fit_scale(w: f32, h: f32, box_w: f32, box_h: f32) -> f32 {
    if w <= 0.0 || h <= 0.0 {
        1.0
    } else {
        (box_w / w).min(box_h / h)
    }
}

/// 83 → "1:23", 3725 → "1:02:05".
pub fn format_elapsed(seconds: f64) -> String {
    let s = seconds.max(0.0).floor() as u64;
    let (h, m, ss) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{ss:02}")
    } else {
        format!("{m}:{ss:02}")
    }
}

/// Keeps a slide number (1-based) inside the document.
pub fn clamp_page(page: f64, total: usize) -> usize {
    let p = if page.is_nan() { 1.0 } else { page.round() };
    (p.max(1.0) as usize).min(total.max(1))
}

#[derive(Default)]
struct Timer {
    started: Option<Instant>,
    running: bool,
    carried: f64,
}

impl Timer {
    fn elapsed(&self) -> f64 {
        self.carried
            + if self.running {
                self.started.map_or(0.0, |s| s.elapsed().as_secs_f64())
            } else {
                0.0
            }
    }
    /// Start, pause or resume.
    fn toggle(&mut self) {
        if self.running {
            self.carried = self.elapsed();
            self.running = false;
        } else {
            self.started = Some(Instant::now());
            self.running = true;
        }
    }
    fn reset(&mut self) {
        self.carried = 0.0;
        if self.running {
            self.started = Some(Instant::now());
        }
    }
}

pub struct Presentation {
    engine: Engine,
    sizes: Vec<Vec2>,
    error: Option<String>,
    /// 1-based, as shown.
    pub page: usize,
    start: usize,
    slide: Region,
    notes: Option<Region>,
    /// Rendered pages by (page, scale in 1/16 px per point).
    tex: HashMap<(usize, u32), TextureHandle>,
    asked: HashSet<(usize, u32)>,
    pub presenter: bool,
    timer: Timer,
    entered: bool,
}

/// One render step: scale is quantised so a resize doesn't render every frame.
fn scale_key(scale: f32) -> u32 {
    (scale * 16.0).round().max(1.0) as u32
}

const DOC: u64 = 1;

impl Presentation {
    pub fn new(ctx: &egui::Context, bytes: Arc<Vec<u8>>, start: usize) -> Self {
        let engine = Engine::new(ctx.clone());
        engine.send(Job::Load { doc: DOC, bytes });
        Presentation {
            engine,
            sizes: Vec::new(),
            error: None,
            page: start.max(1),
            start: start.max(1),
            slide: Region::Full,
            notes: None,
            tex: HashMap::new(),
            asked: HashSet::new(),
            presenter: false,
            timer: Timer::default(),
            entered: false,
        }
    }

    /// The current slide has an image (for tests).
    #[cfg(test)]
    pub fn rendered(&self) -> bool {
        self.tex.keys().any(|k| k.0 + 1 == self.page)
    }

    pub fn total(&self) -> usize {
        self.sizes.len()
    }

    /// `true` when the presenter's own key started it, so the timer starts with the first change.
    fn go(&mut self, to: f64) {
        let target = clamp_page(to, self.total());
        if target != self.page {
            self.page = target;
            if self.timer.started.is_none() {
                self.timer.toggle();
            }
        }
    }

    pub(crate) fn receive(&mut self, ctx: &egui::Context) {
        while let Ok(d) = self.engine.rx.try_recv() {
            match d {
                Done::Loaded {
                    result: Ok((sizes, _)),
                    ..
                } => {
                    if let Some(first) = sizes.first() {
                        (self.slide, self.notes) = regions(first.x, first.y);
                    }
                    self.sizes = sizes;
                    self.page = clamp_page(self.start as f64, self.sizes.len());
                }
                Done::Loaded { result: Err(e), .. } => self.error = Some(e),
                Done::Rendered {
                    page, scale, image, ..
                } => {
                    let key = (page, scale_key(scale));
                    let t = ctx.load_texture(
                        format!("present-{page}-{}", key.1),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.tex.insert(key, t);
                    // Keep the pages around the current one; drop the rest.
                    let keep = |p: usize, cur: usize| p + 2 >= cur && p <= cur + 2;
                    let cur = self.page - 1;
                    self.tex.retain(|k, _| keep(k.0, cur));
                    self.asked.retain(|k| keep(k.0, cur));
                }
                _ => {}
            }
        }
    }

    /// Draws page `page` (0-based), only `region` of it, fitted and centred in `rect`.
    fn draw(&mut self, ui: &Ui, rect: Rect, page: usize, region: Region) {
        let Some(size) = self.sizes.get(page) else {
            return;
        };
        let (w, h, x) = region_size(size.x, size.y, region);
        let k = fit_scale(w, h, rect.width(), rect.height());
        let shown = vec2(w * k, h * k);
        let at = Rect::from_center_size(rect.center(), shown);
        let scale = k * ui.ctx().pixels_per_point();
        let key = (page, scale_key(scale));
        if !self.tex.contains_key(&key) && self.asked.insert(key) {
            self.engine.send(Job::Render {
                doc: DOC,
                page,
                scale: key.1 as f32 / 16.0,
                dark: false,
                epoch: None,
                thumb: false,
            });
        }
        // The exact size, else any size of this page while it renders.
        let tex = self
            .tex
            .get(&key)
            .or_else(|| self.tex.iter().find(|(k, _)| k.0 == page).map(|(_, t)| t));
        ui.painter().rect_filled(at, 0, Color32::WHITE);
        if let Some(t) = tex {
            let u0 = x / size.x;
            let u1 = (x + w) / size.x;
            ui.painter().image(
                t.id(),
                at,
                Rect::from_min_max(pos2(u0, 0.0), pos2(u1, 1.0)),
                Color32::WHITE,
            );
        }
    }

    /// The keys both windows share. Returns false on Esc.
    fn keys(&mut self, ctx: &egui::Context, presenter: bool) -> bool {
        let mut next = false;
        let mut back = false;
        let mut keep = true;
        let total = self.total();
        ctx.input_mut(|i| {
            for k in [
                Key::ArrowRight,
                Key::ArrowDown,
                Key::PageDown,
                Key::Space,
                Key::Enter,
            ] {
                next |= i.consume_key(egui::Modifiers::NONE, k);
            }
            for k in [Key::ArrowLeft, Key::ArrowUp, Key::PageUp, Key::Backspace] {
                back |= i.consume_key(egui::Modifiers::NONE, k);
            }
            if i.consume_key(egui::Modifiers::NONE, Key::Escape) {
                keep = false;
            }
        });
        if next {
            self.go(self.page as f64 + 1.0);
        }
        if back {
            self.go(self.page as f64 - 1.0);
        }
        let (home, end, p, t, r) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::Home),
                i.consume_key(egui::Modifiers::NONE, Key::End),
                !presenter && i.consume_key(egui::Modifiers::NONE, Key::P),
                presenter && i.consume_key(egui::Modifiers::NONE, Key::T),
                presenter && i.consume_key(egui::Modifiers::NONE, Key::R),
            )
        });
        if home {
            self.go(1.0);
        }
        if end {
            self.go(total as f64);
        }
        if p {
            self.presenter = true;
        }
        if t {
            self.timer.toggle();
        }
        if r {
            self.timer.reset();
        }
        keep
    }

    /// One frame of the show. Returns false when it has ended.
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        self.receive(ctx);
        if !self.entered {
            self.entered = true;
            // Full screen is the window's, as the Svelte app did through Tauri.
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
        }
        let mut keep = self.keys(ctx, false);
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::unique("presentation"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, r) = ui.allocate_exact_size(screen.size(), Sense::click());
                r.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Image,
                        true,
                        format!("Slide {} of {}", self.page, self.total()),
                    )
                });
                ui.painter().rect_filled(rect, 0, Color32::BLACK);
                if let Some(e) = &self.error {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        e,
                        FontId::proportional(14.0),
                        Color32::from_gray(200),
                    );
                } else if self.total() > 0 {
                    let (page, slide) = (self.page - 1, self.slide);
                    self.draw(ui, rect, page, slide);
                }
                // .count, bottom right.
                ui.painter().text(
                    rect.right_bottom() - vec2(16.0, 12.0),
                    egui::Align2::RIGHT_BOTTOM,
                    format!("{} / {} · P: presenter view", self.page, self.total()),
                    FontId::proportional(12.0),
                    Color32::from_rgba_unmultiplied(255, 255, 255, 128),
                );
                // A click on the left third goes back, anywhere else forward.
                if r.clicked()
                    && let Some(pos) = r.interact_pointer_pos()
                {
                    let step = if pos.x > rect.width() / 3.0 {
                        1.0
                    } else {
                        -1.0
                    };
                    self.go(self.page as f64 + step);
                }
                if !r.hovered() || ui.input(|i| i.pointer.time_since_last_movement() > 2.0) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                }
            });
        if self.presenter && keep {
            keep = self.presenter_window(ctx);
        }
        // The timer and the clock move.
        if self.presenter {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        if !keep {
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
        }
        keep
    }

    /// The presenter's window. Returns false when the presentation should end.
    fn presenter_window(&mut self, ctx: &egui::Context) -> bool {
        let mut keep = true;
        ctx.show_viewport_immediate(
            ViewportId::from_hash_of("presenter"),
            ViewportBuilder::default()
                .with_title("Lemmata presenter view")
                .with_inner_size([1180.0, 740.0]),
            |ctx, _| {
                if ctx.input(|i| i.viewport().close_requested()) {
                    self.presenter = false;
                    return;
                }
                if !self.keys(ctx, true) {
                    keep = false;
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(BG))
                    .show(ctx, |ui| {
                        if !self.presenter_ui(ui) {
                            keep = false;
                        }
                    });
            },
        );
        keep
    }

    pub(crate) fn presenter_ui(&mut self, ui: &mut Ui) -> bool {
        let mut keep = true;
        let full = ui.max_rect();
        let bar = |ui: &mut Ui, rect: Rect, top: bool| {
            ui.painter().rect_filled(rect, 0, BAR);
            let y = if top {
                rect.bottom() - 0.5
            } else {
                rect.top() + 0.5
            };
            ui.painter()
                .hline(rect.x_range(), y, Stroke::new(1.0, LINE));
        };
        let head = Rect::from_min_size(full.min, vec2(full.width(), 56.0));
        let foot = Rect::from_min_max(pos2(full.left(), full.bottom() - 50.0), full.max);
        bar(ui, head, true);
        bar(ui, foot, false);
        let total = self.total();
        // header: the slide number, the timer, the clock.
        let mut h = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(head.shrink2(vec2(18.0, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        h.spacing_mut().item_spacing.x = 0.0;
        h.label(RichText::new("Slide ").size(16.0).color(INK));
        h.label(
            RichText::new(self.page.to_string())
                .size(16.0)
                .strong()
                .color(INK),
        );
        h.label(
            RichText::new(format!(
                " / {}",
                if total > 0 {
                    total.to_string()
                } else {
                    "…".into()
                }
            ))
            .size(16.0)
            .color(INK),
        );
        let mid = Rect::from_center_size(head.center(), vec2(320.0, head.height()));
        let mut m = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(mid)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        m.spacing_mut().item_spacing.x = 10.0;
        let idle = !self.timer.running;
        m.label(
            RichText::new(format_elapsed(self.timer.elapsed()))
                .font(FontId::new(30.0, theme::font::mono_500()))
                .color(if idle { MUTED } else { INK }),
        );
        let label = if self.timer.running {
            "Pause"
        } else if self.timer.started.is_some() {
            "Resume"
        } else {
            "Start"
        };
        if button(&mut m, label, false)
            .on_hover_text("Start or pause the timer (T)")
            .clicked()
        {
            self.timer.toggle();
        }
        if button(&mut m, "Reset", false)
            .on_hover_text("Reset the timer (R)")
            .clicked()
        {
            self.timer.reset();
        }
        let clock = chrono::Local::now().format("%H:%M").to_string();
        ui.painter().text(
            pos2(head.right() - 18.0, head.center().y),
            egui::Align2::RIGHT_CENTER,
            clock,
            FontId::new(18.0, theme::font::mono_500()),
            MUTED,
        );

        // main: the current slide, then next and notes.
        let main = Rect::from_min_max(
            pos2(full.left() + 14.0, head.bottom() + 14.0),
            pos2(full.right() - 14.0, foot.top() - 14.0),
        );
        if let Some(e) = &self.error {
            ui.painter().text(
                main.left_top() + vec2(26.0, 26.0),
                egui::Align2::LEFT_TOP,
                e,
                FontId::proportional(14.0),
                Color32::from_rgb(0xe5, 0x8c, 0x8c),
            );
        } else {
            let cur_w = (main.width() - 14.0) * 1.7 / 2.7;
            let cur = Rect::from_min_size(main.min, vec2(cur_w, main.height()));
            let side = Rect::from_min_max(pos2(cur.right() + 14.0, main.top()), main.max);
            let boxed = |ui: &Ui, r: Rect| ui.painter().rect_filled(r, CornerRadius::same(6), PIT);
            boxed(ui, cur);
            if total > 0 {
                self.draw(ui, cur.shrink(1.0), self.page - 1, self.slide);
            }
            let label_h = 22.0;
            let rest = side.height() - 2.0 * label_h - 3.0 * 6.0;
            let next_h = rest * 0.8 / 1.8;
            let label = |ui: &Ui, y: f32, t: &str| {
                let mut job = egui::text::LayoutJob::default();
                job.append(
                    t,
                    0.0,
                    egui::TextFormat {
                        font_id: FontId::new(11.0, theme::font::sans_600()),
                        color: MUTED,
                        extra_letter_spacing: 1.1,
                        ..Default::default()
                    },
                );
                let g = ui.fonts_mut(|f| f.layout_job(job));
                ui.painter().galley(pos2(side.left(), y + 4.0), g, MUTED);
            };
            label(ui, side.top(), "NEXT");
            let next = Rect::from_min_size(
                pos2(side.left(), side.top() + label_h + 6.0),
                vec2(side.width(), next_h),
            );
            boxed(ui, next);
            if self.page < total {
                let slide = self.slide;
                self.draw(ui, next.shrink(1.0), self.page, slide);
            } else {
                ui.painter().text(
                    next.center(),
                    egui::Align2::CENTER_CENTER,
                    "End of slides",
                    FontId::proportional(14.0),
                    MUTED,
                );
            }
            label(ui, next.bottom() + 6.0, "NOTES");
            let notes = Rect::from_min_max(
                pos2(side.left(), next.bottom() + 6.0 + label_h + 6.0),
                side.max,
            );
            boxed(ui, notes);
            match self.notes {
                Some(region) if total > 0 => {
                    self.draw(ui, notes.shrink(1.0), self.page - 1, region)
                }
                _ => {
                    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(notes.shrink(14.0)));
                    c.add(
                        egui::Label::new(
                            RichText::new(
                                "No speaker notes in this PDF. Add \\setbeameroption{show notes on second screen=right} to the preamble and \\note{…} after a slide, then recompile.",
                            )
                            .size(13.0)
                            .color(MUTED),
                        )
                        .wrap(),
                    );
                }
            }
        }

        // footer: back, next, the keys, end.
        let mut f = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(foot.shrink2(vec2(18.0, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        f.spacing_mut().item_spacing.x = 18.0;
        if button(&mut f, "◀ Back", self.page <= 1).clicked() {
            self.go(self.page as f64 - 1.0);
        }
        if button(&mut f, "Next ▶", self.page >= total).clicked() {
            self.go(self.page as f64 + 1.0);
        }
        f.with_layout(egui::Layout::right_to_left(egui::Align::Center), |f| {
            if end_button(f).clicked() {
                keep = false;
            }
            f.label(
                RichText::new("← → space: change slide · T: timer · R: reset · Esc: end")
                    .size(12.5)
                    .color(MUTED),
            );
        });
        keep
    }
}

// PresenterView.svelte's colours: its own dark theme in both app themes.
const BG: Color32 = Color32::from_rgb(0x14, 0x19, 0x1f);
const BAR: Color32 = Color32::from_rgb(0x1d, 0x26, 0x30);
const LINE: Color32 = Color32::from_rgb(0x33, 0x40, 0x4d);
const PIT: Color32 = Color32::from_rgb(0x0d, 0x11, 0x16);
const INK: Color32 = Color32::from_rgb(0xdb, 0xe3, 0xea);
const MUTED: Color32 = Color32::from_rgb(0x8d, 0x9c, 0xab);

/// The presenter's 30 px buttons on #2b3743.
fn button(ui: &mut Ui, text: &str, disabled: bool) -> egui::Response {
    let g = ui
        .painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(14.0), INK);
    let (rect, r) = ui.allocate_exact_size(
        vec2(g.size().x + 24.0, 30.0),
        if disabled {
            Sense::hover()
        } else {
            Sense::click()
        },
    );
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, !disabled, text));
    let fill = if r.hovered() && !disabled {
        Color32::from_rgb(0x36, 0x45, 0x55)
    } else {
        Color32::from_rgb(0x2b, 0x37, 0x43)
    };
    let alpha = if disabled { 0.4 } else { 1.0 };
    ui.painter().rect(
        rect,
        CornerRadius::same(6),
        fill.gamma_multiply(alpha),
        Stroke::new(
            1.0,
            Color32::from_rgb(0x3a, 0x48, 0x56).gamma_multiply(alpha),
        ),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - g.size() / 2.0, g, INK.gamma_multiply(alpha));
    if disabled {
        r
    } else {
        r.on_hover_cursor(egui::CursorIcon::PointingHand)
    }
}

fn end_button(ui: &mut Ui) -> egui::Response {
    let text = "End presentation";
    let g = ui
        .painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(14.0), INK);
    let (rect, r) = ui.allocate_exact_size(vec2(g.size().x + 24.0, 30.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    ui.painter().rect(
        rect,
        CornerRadius::same(6),
        Color32::from_rgb(0x6d, 0x2f, 0x2f),
        Stroke::new(1.0, Color32::from_rgb(0x8a, 0x3d, 0x3d)),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(rect.center() - g.size() / 2.0, g, INK);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    // presenter/layout.test.ts
    #[test]
    fn notes_pages_are_told_apart() {
        assert!(!has_notes_page(455.0, 256.0));
        assert!(!has_notes_page(362.0, 272.0));
        assert!(has_notes_page(910.0, 256.0));
        assert!(has_notes_page(724.0, 272.0));
        assert!(!has_notes_page(595.0, 842.0));
        assert!(!has_notes_page(100.0, 0.0));
        assert_eq!(regions(910.0, 256.0), (Region::Left, Some(Region::Right)));
        assert_eq!(regions(455.0, 256.0), (Region::Full, None));
        assert_eq!(region_size(910.0, 256.0, Region::Left), (455.0, 256.0, 0.0));
        assert_eq!(
            region_size(910.0, 256.0, Region::Right),
            (455.0, 256.0, 455.0)
        );
        assert_eq!(region_size(455.0, 256.0, Region::Full), (455.0, 256.0, 0.0));
    }

    #[test]
    fn helpers() {
        assert_eq!(fit_scale(400.0, 200.0, 800.0, 800.0), 2.0);
        assert_eq!(fit_scale(400.0, 200.0, 800.0, 200.0), 1.0);
        assert_eq!(fit_scale(0.0, 0.0, 800.0, 600.0), 1.0);
        assert_eq!(format_elapsed(0.0), "0:00");
        assert_eq!(format_elapsed(83.0), "1:23");
        assert_eq!(format_elapsed(3725.0), "1:02:05");
        assert_eq!(format_elapsed(-4.0), "0:00");
        assert_eq!(clamp_page(0.0, 10), 1);
        assert_eq!(clamp_page(11.0, 10), 10);
        assert_eq!(clamp_page(4.6, 10), 5);
        assert_eq!(clamp_page(f64::NAN, 10), 1);
        assert_eq!(clamp_page(3.0, 0), 1);
    }
}
