//! Presenting a Beamer PDF: full-screen audience stage (`show`) and studio-dark presenter
//! console (`presenter_ui`) in a second egui viewport, sharing live slide state, laser/spotlight
//! pointer, freehand ink annotations, editable speaker notes, and annotated PDF export.

pub mod data;
pub mod export;
mod stage;

pub use data::{
    PenColor, PresenterStore, Region, ScreenMode, StageTool, StrokePath, Timer, clamp_page,
    extract_tex_notes, fit_scale, from_norm, has_notes_page, region_size, regions_swapped, to_norm,
};
pub use export::export_annotated_pdf;

use super::engine::{Done, Engine, Job, OutlineNode, Target};
use eframe::egui::{
    self, Color32, Pos2, Rect, TextureHandle, Ui, ViewportBuilder, ViewportCommand, ViewportId,
    pos2, vec2,
};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// Action emitted by `Presentation` for the parent `Workspace` to execute (e.g. exporting an
/// annotated PDF via the native save dialog during or after a presentation).
#[derive(Clone, Debug)]
pub enum PresentAction {
    ExportPdf { include_notes: bool },
}

/// One render step: scale is quantised so a resize doesn't render every frame.
fn scale_key(scale: f32) -> u32 {
    (scale * 16.0).round().max(1.0) as u32
}

pub struct Presentation {
    engine: Engine,
    doc: u64,
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) project_root: Option<PathBuf>,
    pub(crate) sizes: Vec<egui::Vec2>,
    /// 1-based slide number -> active section title from PDF outline bookmarks.
    pub(crate) sections: BTreeMap<usize, String>,
    pub(crate) error: Option<String>,
    /// 1-based current slide shown in presenter (and audience unless frozen).
    pub page: usize,
    start: usize,
    /// When `Some(p)`, the audience screen stays locked on 1-based slide `p` while the presenter
    /// can browse freely.
    pub frozen_page: Option<usize>,
    /// True when at least one page in the deck has Beamer second-screen speaker notes.
    pub(crate) has_any_notes: bool,
    /// Rendered pages by (page, scale in 1/16 px per point), with when each was last drawn.
    tex: HashMap<(usize, u32), (TextureHandle, u64)>,
    frame: u64,
    asked: HashSet<(usize, u32)>,
    pub presenter: bool,
    pub(crate) presenter_fullscreen: bool,
    pub(crate) presenter_fullscreen_dirty: bool,
    /// Audience screen blanking mode (`B` blackout, `W` whiteout).
    pub screen: ScreenMode,
    /// Backwards-compatible mirror of `screen == ScreenMode::Blackout`.
    pub blackout: bool,
    /// Slide overview grid (`G`).
    pub overview: bool,
    /// Presenter shortcuts cheat-sheet overlay (`?` / `H`).
    pub(crate) shortcuts_open: bool,
    /// Target duration picker popover open.
    pub(crate) target_menu_open: bool,
    /// Save-annotated-PDF popover open.
    pub(crate) export_menu_open: bool,
    /// Inline slide-number jump editor open in presenter header.
    pub(crate) editing_slide_jump: bool,
    pub(crate) slide_jump_text: String,
    /// Inline speaker-notes editor open in presenter Notes card.
    pub(crate) editing_notes: bool,
    /// Active stage tool (`L` laser, `S` spotlight, `D` pen).
    pub tool: StageTool,
    pub pen_color: PenColor,
    /// Active stroke currently being drawn (not yet committed to `store.annotations`).
    pub(crate) active_stroke: Option<StrokePath>,
    /// Live laser / spotlight position in normalized slide coordinates + recent trail.
    pub(crate) pointer_uv: Option<[f32; 2]>,
    pub(crate) pointer_at: Option<Instant>,
    pub(crate) laser_trail: VecDeque<([f32; 2], Instant)>,
    /// Buffered digits for jump-to-slide (`1`-`9`, `Enter`).
    pub(crate) jump_digits: String,
    pub(crate) jump_at: Option<Instant>,
    pub(crate) timer: Timer,
    /// Persistent presenter settings, per-slide notes, and per-slide annotations.
    pub store: PresenterStore,
    pub(crate) store_dirty: bool,
    /// True if annotations or notes were modified during this presentation since last PDF export.
    pub dirty_for_export: bool,
    /// Transient in-presenter status banner (e.g. "Saved annotated PDF").
    pub(crate) status_toast: Option<(String, Instant)>,
    /// Pending actions for Workspace to handle on this frame.
    pending_actions: Vec<PresentAction>,
    entered: bool,
}

impl Presentation {
    #[cfg(test)]
    pub fn new(ctx: &egui::Context, bytes: Arc<Vec<u8>>, start: usize) -> Self {
        Self::with_project(ctx, bytes, start, None, None)
    }

    pub fn with_project(
        ctx: &egui::Context,
        bytes: Arc<Vec<u8>>,
        start: usize,
        project_root: Option<PathBuf>,
        tex_source_notes: Option<BTreeMap<usize, String>>,
    ) -> Self {
        let engine = Engine::new(ctx.clone());
        let doc = engine.next_doc();
        engine.send(Job::Load {
            doc,
            bytes: bytes.clone(),
        });
        let mut store = project_root
            .as_deref()
            .map(PresenterStore::load)
            .unwrap_or_default();
        // Seed speaker notes from `.tex` \note{...} / % NOTE: comments for slides without custom notes.
        if let Some(extracted) = tex_source_notes {
            for (slide, text) in extracted {
                store.notes.entry(slide).or_insert(text);
            }
        }
        Presentation {
            engine,
            doc,
            bytes,
            project_root,
            sizes: Vec::new(),
            sections: BTreeMap::new(),
            error: None,
            page: start.max(1),
            start: start.max(1),
            frozen_page: None,
            has_any_notes: false,
            tex: HashMap::new(),
            frame: 0,
            asked: HashSet::new(),
            presenter: false,
            presenter_fullscreen: false,
            presenter_fullscreen_dirty: false,
            screen: ScreenMode::Normal,
            blackout: false,
            overview: false,
            shortcuts_open: false,
            target_menu_open: false,
            export_menu_open: false,
            editing_slide_jump: false,
            slide_jump_text: String::new(),
            editing_notes: false,
            tool: StageTool::None,
            pen_color: PenColor::Crimson,
            active_stroke: None,
            pointer_uv: None,
            pointer_at: None,
            laser_trail: VecDeque::new(),
            jump_digits: String::new(),
            jump_at: None,
            timer: Timer::default(),
            store,
            store_dirty: false,
            dirty_for_export: false,
            status_toast: None,
            pending_actions: Vec::new(),
            entered: false,
        }
    }

    /// Drains any pending workspace actions (such as exporting an annotated PDF).
    pub fn take_actions(&mut self) -> Vec<PresentAction> {
        std::mem::take(&mut self.pending_actions)
    }

    pub fn push_action(&mut self, action: PresentAction) {
        self.pending_actions.push(action);
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_toast = Some((msg.into(), Instant::now()));
    }

    pub fn mark_exported(&mut self) {
        self.dirty_for_export = false;
    }

    pub fn bytes(&self) -> Arc<Vec<u8>> {
        self.bytes.clone()
    }

    /// Slide and optional speaker-notes region for 0-based `page`.
    pub fn page_regions(&self, page: usize) -> (Region, Option<Region>) {
        self.sizes.get(page).map_or((Region::Full, None), |s| {
            regions_swapped(s.x, s.y, self.store.swap_notes_half)
        })
    }

    /// Active section title for a 1-based slide number from the PDF outline.
    pub fn section_for(&self, slide: usize) -> Option<&str> {
        self.sections
            .range(..=slide)
            .next_back()
            .map(|(_, title)| title.as_str())
    }

    /// The slide currently displayed on the audience screen (respects Freeze).
    pub fn audience_page(&self) -> usize {
        self.frozen_page
            .unwrap_or(self.page)
            .clamp(1, self.total().max(1))
    }

    /// The current slide has an image (for tests).
    #[cfg(test)]
    pub fn rendered(&self) -> bool {
        self.tex.keys().any(|k| k.0 + 1 == self.page)
    }

    pub fn total(&self) -> usize {
        self.sizes.len()
    }

    pub(crate) fn set_screen(&mut self, mode: ScreenMode) {
        self.screen = mode;
        self.blackout = mode == ScreenMode::Blackout;
    }

    pub(crate) fn toggle_blackout(&mut self) {
        let next = if self.screen == ScreenMode::Blackout {
            ScreenMode::Normal
        } else {
            ScreenMode::Blackout
        };
        self.set_screen(next);
    }

    pub(crate) fn toggle_whiteout(&mut self) {
        let next = if self.screen == ScreenMode::Whiteout {
            ScreenMode::Normal
        } else {
            ScreenMode::Whiteout
        };
        self.set_screen(next);
    }

    pub(crate) fn toggle_freeze(&mut self) {
        if self.frozen_page.is_some() {
            self.frozen_page = None;
        } else {
            self.frozen_page = Some(self.page);
        }
    }

    pub(crate) fn toggle_tool(&mut self, tool: StageTool) {
        self.finish_active_stroke();
        self.tool = if self.tool == tool {
            StageTool::None
        } else {
            tool
        };
        if !matches!(self.tool, StageTool::Laser | StageTool::Spotlight) {
            self.pointer_uv = None;
            self.laser_trail.clear();
        }
    }

    pub(crate) fn undo_stroke(&mut self) {
        let slide = self.page;
        if let Some(list) = self.store.annotations.get_mut(&slide)
            && list.pop().is_some()
        {
            if list.is_empty() {
                self.store.annotations.remove(&slide);
            }
            self.store_dirty = true;
            self.dirty_for_export = true;
        }
    }

    pub(crate) fn clear_slide_strokes(&mut self) {
        if self.store.annotations.remove(&self.page).is_some() {
            self.store_dirty = true;
            self.dirty_for_export = true;
        }
    }

    pub(crate) fn finish_active_stroke(&mut self) {
        if let Some(stroke) = self.active_stroke.take()
            && !stroke.points.is_empty()
        {
            self.store
                .annotations
                .entry(self.page)
                .or_default()
                .push(stroke);
            self.store_dirty = true;
            self.dirty_for_export = true;
        }
    }

    pub(crate) fn flush_store(&mut self) {
        if self.store_dirty {
            self.store_dirty = false;
            if let Some(root) = &self.project_root {
                let _ = self.store.save(root);
            }
        }
    }

    /// Navigates to 1-based slide `to`, starting the timer on the first slide change.
    pub(crate) fn go(&mut self, to: f64) {
        self.finish_active_stroke();
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
                    result: Ok((sizes, outline)),
                    ..
                } => {
                    self.has_any_notes = sizes.iter().any(|s| has_notes_page(s.x, s.y));
                    self.sizes = sizes;
                    self.sections = index_outline(&outline);
                    self.page = clamp_page(self.start as f64, self.sizes.len());
                }
                Done::Loaded { result: Err(e), .. } => self.error = Some(e),
                Done::Rendered {
                    page, scale, image, ..
                } => {
                    let key = (page, scale_key(scale));
                    self.asked.remove(&key);
                    let t = ctx.load_texture(
                        format!("present-{page}-{}", key.1),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.tex.insert(key, (t, self.frame));
                    if !self.overview && !self.store.filmstrip {
                        // Keep the pages around the current (and frozen) slide; drop the rest.
                        let cur = self.page.saturating_sub(1);
                        let frz = self.frozen_page.map(|p| p.saturating_sub(1));
                        let keep = |p: usize| {
                            (p + 2 >= cur && p <= cur + 2)
                                || frz.is_some_and(|f| p + 1 >= f && p <= f + 1)
                        };
                        self.tex.retain(|k, _| keep(k.0));
                        self.asked.retain(|k| keep(k.0));
                    }
                    // Keep up to 3 rendered sizes per page (audience, presenter, next/notes/thumb).
                    let mut sizes: Vec<_> = self
                        .tex
                        .iter()
                        .filter(|(k, _)| k.0 == page)
                        .map(|(k, (_, used))| (*used, *k))
                        .collect();
                    sizes.sort_unstable_by_key(|s| std::cmp::Reverse(*s));
                    for (_, k) in sizes.into_iter().skip(3) {
                        self.tex.remove(&k);
                        self.asked.remove(&k);
                    }
                }
                _ => {}
            }
        }
    }

    /// Draws page `page` (0-based), only `region` of it, fitted and centred in `rect`.
    /// Returns the actual rendered slide rectangle on screen.
    pub(crate) fn draw(
        &mut self,
        ui: &Ui,
        rect: Rect,
        page: usize,
        region: Region,
    ) -> Option<Rect> {
        let size = *self.sizes.get(page)?;
        let (w, h, x) = region_size(size.x, size.y, region);
        let k = fit_scale(w, h, rect.width(), rect.height());
        let shown = vec2(w * k, h * k);
        let at = Rect::from_center_size(rect.center(), shown);
        let scale = k * ui.ctx().pixels_per_point();
        let key = (page, scale_key(scale));
        self.frame += 1;
        if let Some((_, used)) = self.tex.get_mut(&key) {
            *used = self.frame;
        }
        if !self.tex.contains_key(&key) && self.asked.insert(key) {
            self.engine.send(Job::Render {
                doc: self.doc,
                page,
                scale: key.1 as f32 / 16.0,
                dark: false,
                epoch: None,
                thumb: false,
            });
        }
        let tex = self.tex.get(&key).map(|(t, _)| t).or_else(|| {
            self.tex
                .iter()
                .filter(|(k, _)| k.0 == page)
                .max_by_key(|(k, _)| k.1)
                .map(|(_, (t, _))| t)
        });
        ui.painter().add(
            egui::Shadow {
                offset: [0, 6],
                blur: 22,
                spread: 0,
                color: Color32::from_black_alpha(120),
            }
            .as_shape(at, 2),
        );
        ui.painter().rect_filled(at, 2, Color32::WHITE);
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
        Some(at)
    }

    /// Updates laser/spotlight/pen interaction on a displayed slide rectangle.
    /// Returns `true` if the pointer was consumed by a drawing/stage tool.
    pub(crate) fn interact_stage(
        &mut self,
        ui: &mut Ui,
        slide_rect: Rect,
        slide_num: usize,
        id_salt: &str,
        allow_draw: bool,
    ) -> bool {
        if self.tool == StageTool::None {
            return false;
        }
        let sense = if self.tool == StageTool::Pen && allow_draw {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::hover()
        };
        let resp = ui.interact(slide_rect, ui.scope_id().with(id_salt), sense);
        match self.tool {
            StageTool::Laser | StageTool::Spotlight => {
                if let Some(pos) = resp.hover_pos().and_then(|p| to_norm(p, slide_rect)) {
                    let now = Instant::now();
                    self.pointer_uv = Some(pos);
                    self.pointer_at = Some(now);
                    if self.tool == StageTool::Laser {
                        self.laser_trail.push_back((pos, now));
                        while self.laser_trail.len() > 14 {
                            self.laser_trail.pop_front();
                        }
                    }
                    ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(16));
                }
                true
            }
            StageTool::Pen if allow_draw => {
                if resp.hovered() || resp.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                }
                if resp.drag_started() {
                    if let Some(uv) = resp
                        .interact_pointer_pos()
                        .and_then(|p| to_norm(p, slide_rect))
                    {
                        self.active_stroke = Some(StrokePath {
                            color: self.pen_color,
                            width: 0.0055,
                            points: vec![uv],
                        });
                    }
                } else if resp.dragged() {
                    if let Some(uv) = resp
                        .interact_pointer_pos()
                        .and_then(|p| to_norm(p, slide_rect))
                    {
                        let stroke = self.active_stroke.get_or_insert_with(|| StrokePath {
                            color: self.pen_color,
                            width: 0.0055,
                            points: Vec::new(),
                        });
                        let push = stroke.points.last().is_none_or(|last| {
                            let dx = last[0] - uv[0];
                            let dy = last[1] - uv[1];
                            dx * dx + dy * dy > 0.000012
                        });
                        if push {
                            stroke.points.push(uv);
                        }
                    }
                } else if resp.drag_stopped() {
                    if let Some(stroke) = self.active_stroke.take()
                        && !stroke.points.is_empty()
                    {
                        self.store
                            .annotations
                            .entry(slide_num)
                            .or_default()
                            .push(stroke);
                        self.store_dirty = true;
                        self.dirty_for_export = true;
                    }
                } else if resp.clicked()
                    && let Some(uv) = resp
                        .interact_pointer_pos()
                        .and_then(|p| to_norm(p, slide_rect))
                {
                    self.store
                        .annotations
                        .entry(slide_num)
                        .or_default()
                        .push(StrokePath {
                            color: self.pen_color,
                            width: 0.0055,
                            points: vec![uv],
                        });
                    self.store_dirty = true;
                    self.dirty_for_export = true;
                }
                true
            }
            _ => false,
        }
    }

    /// Paints persisted strokes, active drawing stroke, and live laser/spotlight overlay onto
    /// `slide_rect` for 1-based `slide_num`.
    pub(crate) fn paint_overlays(
        &mut self,
        ui: &Ui,
        slide_rect: Rect,
        slide_num: usize,
        show_live_pointer: bool,
    ) {
        let painter = ui.painter().with_clip_rect(slide_rect);
        // 1. Committed strokes for this slide.
        if let Some(strokes) = self.store.annotations.get(&slide_num) {
            for s in strokes {
                paint_stroke(&painter, slide_rect, s);
            }
        }
        // 2. Active stroke being drawn on this slide.
        if slide_num == self.page
            && let Some(active) = &self.active_stroke
        {
            paint_stroke(&painter, slide_rect, active);
        }
        if !show_live_pointer {
            return;
        }
        // Prune expired laser trail points.
        let now = Instant::now();
        while self
            .laser_trail
            .front()
            .is_some_and(|(_, t)| now.duration_since(*t).as_secs_f32() > 0.22)
        {
            self.laser_trail.pop_front();
        }
        let fresh_pointer = self
            .pointer_at
            .is_some_and(|t| now.duration_since(t).as_secs_f32() < 3.0);
        if !fresh_pointer {
            self.pointer_uv = None;
        }
        // 3. Laser or Spotlight overlay.
        if let Some(uv) = self.pointer_uv {
            let center = from_norm(uv, slide_rect);
            match self.tool {
                StageTool::Laser => {
                    // Soft fading trail.
                    let pts: Vec<(Pos2, f32)> = self
                        .laser_trail
                        .iter()
                        .map(|(p, t)| {
                            let age = now.duration_since(*t).as_secs_f32();
                            let a = (1.0 - age / 0.22).clamp(0.0, 1.0);
                            (from_norm(*p, slide_rect), a)
                        })
                        .collect();
                    for win in pts.windows(2) {
                        let a = (win[1].1 * 180.0) as u8;
                        painter.line_segment(
                            [win[0].0, win[1].0],
                            egui::Stroke::new(
                                3.5 * win[1].1 + 1.5,
                                Color32::from_rgba_unmultiplied(255, 56, 56, a),
                            ),
                        );
                    }
                    // Outer halo + crisp hot core.
                    let r = (slide_rect.height() * 0.014).clamp(6.0, 14.0);
                    painter.circle_filled(
                        center,
                        r * 1.9,
                        Color32::from_rgba_unmultiplied(255, 45, 45, 55),
                    );
                    painter.circle_filled(
                        center,
                        r,
                        Color32::from_rgba_unmultiplied(255, 48, 48, 220),
                    );
                    painter.circle_filled(
                        center,
                        r * 0.42,
                        Color32::from_rgba_unmultiplied(255, 235, 235, 255),
                    );
                }
                StageTool::Spotlight => {
                    // Dim the surrounding slide with 4 rectangles around a circular spotlight feel
                    // using a thick dark ring + outer scrim.
                    let radius = (slide_rect.height() * 0.14).clamp(44.0, 140.0);
                    let scrim = Color32::from_rgba_unmultiplied(5, 8, 12, 165);
                    // Build a clean octagonal/32-gon cutout feel via outer strips + corner arcs:
                    // simpler & crisp in epaint: concentric translucent dark rings + bright rim.
                    let max_r = slide_rect.width().hypot(slide_rect.height());
                    let ring_w = (max_r - radius).max(0.0);
                    painter.circle_stroke(
                        center,
                        radius + ring_w * 0.5,
                        egui::Stroke::new(ring_w, scrim),
                    );
                    painter.circle_stroke(
                        center,
                        radius,
                        egui::Stroke::new(2.0, Color32::from_rgba_unmultiplied(255, 255, 255, 190)),
                    );
                }
                _ => {}
            }
        }
    }

    /// One frame of the presentation (audience window + optional presenter viewport).
    /// Returns `false` when the presentation has ended.
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        self.receive(ctx);
        if !self.entered {
            self.entered = true;
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
        }
        let mut keep = self.keys(ctx, false);
        stage::audience_ui(self, ctx, &mut keep);

        if self.presenter && keep {
            keep = self.presenter_window(ctx);
        }
        self.flush_store();

        if self.presenter {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        if !keep {
            self.finish_active_stroke();
            self.flush_store();
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
        }
        keep
    }

    /// The presenter's independent window. Returns `false` when the presentation should end.
    fn presenter_window(&mut self, ctx: &egui::Context) -> bool {
        let mut keep = true;
        ctx.show_viewport_immediate(
            ViewportId::from_hash_of("presenter"),
            ViewportBuilder::default()
                .with_title("Lemmata presenter view")
                .with_inner_size([1240.0, 780.0]),
            |ctx, _| {
                if ctx.input(|i| i.viewport().close_requested()) {
                    self.presenter = false;
                    return;
                }
                if self.presenter_fullscreen_dirty {
                    self.presenter_fullscreen_dirty = false;
                    ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.presenter_fullscreen));
                }
                if !self.keys(ctx, true) {
                    keep = false;
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(stage::BG))
                    .show(ctx, |ui| {
                        if !self.presenter_ui(ui) {
                            keep = false;
                        }
                    });
            },
        );
        keep
    }

    /// Renders the presenter console inside `ui`. Returns `false` when the user ends the show.
    pub(crate) fn presenter_ui(&mut self, ui: &mut Ui) -> bool {
        stage::presenter_ui(self, ui)
    }

    /// Shared keyboard handler for both audience and presenter windows.
    /// Returns `false` on `Esc` when no overlay or blanking state is active.
    pub(crate) fn keys(&mut self, ctx: &egui::Context, presenter: bool) -> bool {
        // When typing inside inline speaker notes or slide-jump text box, only Escape exits editing.
        if presenter && (self.editing_notes || self.editing_slide_jump) {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.editing_notes = false;
                self.editing_slide_jump = false;
            }
            return true;
        }
        if self
            .jump_at
            .is_some_and(|t| t.elapsed().as_secs_f32() > 2.0)
        {
            self.jump_digits.clear();
            self.jump_at = None;
        }
        let mut next = false;
        let mut back = false;
        let mut keep = true;
        let mut enter = false;
        let mut digit: Option<char> = None;
        let mut backspace = false;
        let mut toggle_blackout = false;
        let mut toggle_whiteout = false;
        let mut toggle_freeze = false;
        let mut toggle_overview = false;
        let mut toggle_filmstrip = false;
        let mut toggle_shortcuts = false;
        let mut toggle_laser = false;
        let mut toggle_spotlight = false;
        let mut toggle_pen = false;
        let mut clear_strokes = false;
        let mut undo_stroke = false;
        let total = self.total();

        ctx.input_mut(|i| {
            use egui::{Key, Modifiers};
            for (k, ch) in [
                (Key::Num0, '0'),
                (Key::Num1, '1'),
                (Key::Num2, '2'),
                (Key::Num3, '3'),
                (Key::Num4, '4'),
                (Key::Num5, '5'),
                (Key::Num6, '6'),
                (Key::Num7, '7'),
                (Key::Num8, '8'),
                (Key::Num9, '9'),
            ] {
                if i.consume_key(Modifiers::NONE, k) {
                    digit = Some(ch);
                }
            }
            if i.consume_key(Modifiers::COMMAND, Key::Z) {
                undo_stroke = true;
            }
            if i.consume_key(Modifiers::NONE, Key::B) || i.consume_key(Modifiers::NONE, Key::Period)
            {
                toggle_blackout = true;
            }
            if i.consume_key(Modifiers::NONE, Key::W) {
                toggle_whiteout = true;
            }
            if i.consume_key(Modifiers::NONE, Key::F) {
                toggle_freeze = true;
            }
            if i.consume_key(Modifiers::NONE, Key::G) {
                toggle_overview = true;
            }
            if presenter && i.consume_key(Modifiers::NONE, Key::M) {
                toggle_filmstrip = true;
            }
            if i.consume_key(Modifiers::NONE, Key::L) {
                toggle_laser = true;
            }
            if i.consume_key(Modifiers::NONE, Key::S) {
                toggle_spotlight = true;
            }
            if i.consume_key(Modifiers::NONE, Key::D) {
                toggle_pen = true;
            }
            if i.consume_key(Modifiers::NONE, Key::C) {
                clear_strokes = true;
            }
            if i.consume_key(Modifiers::NONE, Key::Questionmark)
                || i.consume_key(Modifiers::SHIFT, Key::Slash)
                || i.consume_key(Modifiers::NONE, Key::H)
            {
                toggle_shortcuts = true;
            }
            if i.consume_key(Modifiers::NONE, Key::Enter) {
                enter = true;
            }
            if i.consume_key(Modifiers::NONE, Key::Backspace) {
                backspace = true;
            }
            for k in [Key::ArrowRight, Key::ArrowDown, Key::PageDown, Key::Space] {
                next |= i.consume_key(Modifiers::NONE, k);
            }
            for k in [Key::ArrowLeft, Key::ArrowUp, Key::PageUp] {
                back |= i.consume_key(Modifiers::NONE, k);
            }
            if i.consume_key(Modifiers::NONE, Key::Escape) {
                if !self.jump_digits.is_empty() {
                    self.jump_digits.clear();
                    self.jump_at = None;
                } else if self.shortcuts_open {
                    self.shortcuts_open = false;
                } else if self.target_menu_open || self.export_menu_open {
                    self.target_menu_open = false;
                    self.export_menu_open = false;
                } else if self.overview {
                    self.overview = false;
                } else if self.screen != ScreenMode::Normal {
                    self.set_screen(ScreenMode::Normal);
                } else if self.tool != StageTool::None {
                    self.toggle_tool(self.tool);
                } else {
                    keep = false;
                }
            }
        });

        if undo_stroke {
            self.undo_stroke();
        }
        if let Some(ch) = digit {
            if self.jump_digits.len() < 4 {
                self.jump_digits.push(ch);
            }
            self.jump_at = Some(Instant::now());
        } else if enter {
            if !self.jump_digits.is_empty() {
                if let Ok(n) = self.jump_digits.parse::<usize>() {
                    self.go(n as f64);
                    self.overview = false;
                }
                self.jump_digits.clear();
                self.jump_at = None;
            } else if !self.overview && !self.shortcuts_open {
                next = true;
            }
        } else if backspace {
            if !self.jump_digits.is_empty() {
                self.jump_digits.pop();
                self.jump_at = (!self.jump_digits.is_empty()).then(Instant::now);
            } else {
                back = true;
            }
        }
        if toggle_blackout {
            self.toggle_blackout();
        }
        if toggle_whiteout {
            self.toggle_whiteout();
        }
        if toggle_freeze {
            self.toggle_freeze();
        }
        if toggle_overview {
            self.overview = !self.overview;
        }
        if toggle_filmstrip {
            self.store.filmstrip = !self.store.filmstrip;
            self.store_dirty = true;
        }
        if toggle_shortcuts {
            self.shortcuts_open = !self.shortcuts_open;
        }
        if toggle_laser {
            self.toggle_tool(StageTool::Laser);
        }
        if toggle_spotlight {
            self.toggle_tool(StageTool::Spotlight);
        }
        if toggle_pen {
            self.toggle_tool(StageTool::Pen);
        }
        if clear_strokes {
            self.clear_slide_strokes();
        }
        if next {
            self.go(self.page as f64 + 1.0);
        }
        if back {
            self.go(self.page as f64 - 1.0);
        }
        let (home, end, p, t, r) = ctx.input_mut(|i| {
            use egui::{Key, Modifiers};
            (
                i.consume_key(Modifiers::NONE, Key::Home),
                i.consume_key(Modifiers::NONE, Key::End),
                !presenter && i.consume_key(Modifiers::NONE, Key::P),
                presenter && i.consume_key(Modifiers::NONE, Key::T),
                presenter && i.consume_key(Modifiers::NONE, Key::R),
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
}

fn paint_stroke(painter: &egui::Painter, slide_rect: Rect, s: &StrokePath) {
    if s.points.is_empty() {
        return;
    }
    let width = (s.width * slide_rect.height()).clamp(1.8, 12.0);
    let color = s.color.color32();
    if s.points.len() == 1 {
        painter.circle_filled(from_norm(s.points[0], slide_rect), width * 0.5, color);
        return;
    }
    let pts: Vec<Pos2> = s.points.iter().map(|&p| from_norm(p, slide_rect)).collect();
    painter.add(egui::Shape::line(
        pts.clone(),
        egui::Stroke::new(width, color),
    ));
    // Round caps at endpoints.
    painter.circle_filled(pts[0], width * 0.5, color);
    if let Some(&last) = pts.last() {
        painter.circle_filled(last, width * 0.5, color);
    }
}

fn index_outline(nodes: &[OutlineNode]) -> BTreeMap<usize, String> {
    let mut map = BTreeMap::new();
    fn walk(nodes: &[OutlineNode], map: &mut BTreeMap<usize, String>) {
        for n in nodes {
            if let Some(Target::Page { index, .. }) = n.target {
                let slide = index + 1;
                let title = n.title.trim();
                if !title.is_empty() {
                    map.entry(slide).or_insert_with(|| title.to_owned());
                }
            }
            walk(&n.children, map);
        }
    }
    walk(nodes, &mut map);
    map
}
