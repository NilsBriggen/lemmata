//! Presenting a Beamer PDF: full-screen audience stage (`show`) and themed presenter console
//! (`presenter_ui`) in a second egui viewport, sharing live slide state, laser/spotlight pointer,
//! freehand ink annotations, customizable panels, editable speaker notes, and annotated PDF export.

pub mod data;
pub mod export;
mod stage;

pub use data::{
    DeckItem, PenColor, PresenterStore, Region, ScreenMode, StageTool, StrokePath, Timer,
    WhiteboardSlide, clamp_page, extract_tex_notes, fit_scale, from_norm, has_notes_page,
    region_size, regions_swapped, to_norm,
};
pub use export::export_annotated_pdf;

use super::engine::{Done, Engine, Job, OutlineNode, Target};
use crate::theme::{self, Palette};
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

/// Maximum rendered texture sizes kept per page: covers audience stage, presenter stage,
/// next-slide card, Beamer second-screen notes card, and filmstrip/overview thumbnails at once.
pub(crate) const MAX_RENDERED_SIZES_PER_PAGE: usize = 5;

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
    /// View layout customization popover open (Speaker notes, Next slide, Filmstrip).
    pub(crate) view_menu_open: bool,
    /// Section / appendix quick-jump popover open.
    pub(crate) sections_menu_open: bool,
    /// Inline slide-number jump editor open in presenter header.
    pub(crate) editing_slide_jump: bool,
    pub(crate) slide_jump_text: String,
    /// Inline speaker-notes editor open in presenter Notes card.
    pub(crate) editing_notes: bool,
    /// Active stage tool (`L` laser, `S` spotlight, `Z` zoom, `D` pen, `U` highlighter).
    pub tool: StageTool,
    pub pen_color: PenColor,
    /// Active stroke currently being drawn (not yet committed to `store.annotations`).
    pub(crate) active_stroke: Option<StrokePath>,
    /// Starting normalized UV of the active stroke (for `Shift`-straight line snapping).
    pub(crate) stroke_anchor_uv: Option<[f32; 2]>,
    /// Live laser / spotlight / zoom position in normalized slide coordinates + recent trail.
    pub(crate) pointer_uv: Option<[f32; 2]>,
    pub(crate) pointer_at: Option<Instant>,
    pub(crate) pointer_source: Option<&'static str>,
    pub(crate) laser_trail: VecDeque<([f32; 2], Instant)>,
    /// Buffered digits for jump-to-slide (`1`-`9`, `Enter`).
    pub(crate) jump_digits: String,
    pub(crate) jump_at: Option<Instant>,
    pub(crate) timer: Timer,
    /// When the presenter entered the current slide (for per-slide duration readout).
    pub(crate) slide_entered_at: Instant,
    /// Cumulative rehearsal seconds spent on each 1-based slide while the timer was running.
    pub(crate) slide_timings: BTreeMap<usize, f64>,
    /// Set when `go()` navigates to a new slide so the horizontal filmstrip centers it once.
    pub(crate) filmstrip_scroll_pending: bool,
    /// Persistent presenter settings, per-slide notes, and per-slide annotations.
    pub store: PresenterStore,
    /// Read-only `.tex` `\note{...}` / `% NOTE:` fallback notes (never persisted unless edited).
    pub(crate) tex_notes: BTreeMap<usize, String>,
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
        let store = project_root
            .as_deref()
            .map(PresenterStore::load)
            .unwrap_or_default();
        let tex_notes = tex_source_notes.unwrap_or_default();
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
            view_menu_open: false,
            sections_menu_open: false,
            editing_slide_jump: false,
            slide_jump_text: String::new(),
            editing_notes: false,
            tool: StageTool::None,
            pen_color: PenColor::Crimson,
            active_stroke: None,
            stroke_anchor_uv: None,
            pointer_uv: None,
            pointer_at: None,
            pointer_source: None,
            laser_trail: VecDeque::new(),
            jump_digits: String::new(),
            jump_at: None,
            timer: Timer::default(),
            slide_entered_at: Instant::now(),
            slide_timings: BTreeMap::new(),
            filmstrip_scroll_pending: false,
            store,
            tex_notes,
            store_dirty: false,
            dirty_for_export: false,
            status_toast: None,
            pending_actions: Vec::new(),
            entered: false,
        }
    }

    /// Returns the active speaker note for 1-based deck `slide`: user-edited note in `store.notes`
    /// takes precedence over read-only `.tex` `\note{}` fallback. Whiteboard slides carry no notes.
    pub(crate) fn effective_note(&self, slide: usize) -> Option<&str> {
        let pdf_num = match self.deck_item(slide)? {
            DeckItem::Pdf(idx) => idx + 1,
            DeckItem::Whiteboard(_) => return None,
        };
        if let Some(custom) = self.store.notes.get(&pdf_num) {
            let trimmed = custom.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        } else {
            self.tex_notes
                .get(&pdf_num)
                .map(String::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
        }
    }

    /// Clones `store` merged with `.tex` fallback notes for annotated PDF export.
    pub fn export_store(&self) -> PresenterStore {
        let mut out = self.store.clone();
        for (&slide, text) in &self.tex_notes {
            out.notes.entry(slide).or_insert_with(|| text.clone());
        }
        out
    }

    pub(crate) fn current_slide_has_beamer_notes(&self) -> bool {
        matches!(self.deck_item(self.page), Some(DeckItem::Pdf(idx)) if self.page_regions(idx).1.is_some())
    }

    /// Returns `"Edited"` when the slide has a custom saved note, `"From .tex"` when showing an
    /// extracted `.tex` `\note{}` / `% NOTE:` fallback, or `None` when empty.
    pub(crate) fn note_origin(&self, slide: usize) -> Option<&'static str> {
        let DeckItem::Pdf(idx) = self.deck_item(slide)? else {
            return None;
        };
        let pdf_num = idx + 1;
        if self
            .store
            .notes
            .get(&pdf_num)
            .is_some_and(|s| !s.trim().is_empty())
        {
            Some("Edited")
        } else if self
            .tex_notes
            .get(&pdf_num)
            .is_some_and(|s| !s.trim().is_empty())
        {
            Some("From .tex")
        } else {
            None
        }
    }

    /// True if `slide` has both a user override in `store.notes` and a `.tex` fallback note to
    /// revert back to.
    pub(crate) fn can_reset_note_to_tex(&self, slide: usize) -> bool {
        let Some(DeckItem::Pdf(idx)) = self.deck_item(slide) else {
            return false;
        };
        let pdf_num = idx + 1;
        self.store.notes.contains_key(&pdf_num)
            && self
                .tex_notes
                .get(&pdf_num)
                .is_some_and(|s| !s.trim().is_empty())
    }

    /// Ordered logical deck interleaving PDF pages and inserted blank whiteboard slides.
    pub(crate) fn deck_items(&self) -> Vec<DeckItem> {
        self.store.build_deck(self.sizes.len())
    }

    /// 1-based lookup into the logical deck.
    pub(crate) fn deck_item(&self, slide_num: usize) -> Option<DeckItem> {
        slide_num
            .checked_sub(1)
            .and_then(|idx| self.deck_items().into_iter().nth(idx))
    }

    pub(crate) fn current_pdf_page(&self) -> Option<usize> {
        match self.deck_item(self.page)? {
            DeckItem::Pdf(idx) => Some(idx + 1),
            DeckItem::Whiteboard(_) => None,
        }
    }

    pub(crate) fn current_whiteboard_id(&self) -> Option<usize> {
        match self.deck_item(self.page)? {
            DeckItem::Whiteboard(id) => Some(id),
            DeckItem::Pdf(_) => None,
        }
    }

    /// Maps a 1-based PDF page number to its 1-based position in the logical deck.
    pub(crate) fn deck_index_for_pdf_page(&self, pdf_page_1based: usize) -> Option<usize> {
        let target = pdf_page_1based.checked_sub(1)?;
        self.deck_items()
            .into_iter()
            .position(|it| it == DeckItem::Pdf(target))
            .map(|i| i + 1)
    }

    /// Inserts a fresh blank whiteboard slide immediately after the current slide, switches to it,
    /// and arms the Pen tool so the presenter can start writing at once.
    pub(crate) fn insert_whiteboard_after_current(&mut self) {
        self.finish_active_stroke();
        let after_pdf_page = match self.deck_item(self.page) {
            Some(DeckItem::Pdf(idx)) => idx + 1,
            Some(DeckItem::Whiteboard(id)) => self
                .store
                .whiteboard(id)
                .map_or(self.sizes.len(), |w| w.after_pdf_page),
            None => 0,
        };
        let id = self.store.next_whiteboard_id();
        self.store.whiteboards.push(WhiteboardSlide {
            id,
            after_pdf_page,
            grid: false,
            strokes: Vec::new(),
        });
        let new_slide = self
            .deck_items()
            .into_iter()
            .position(|it| it == DeckItem::Whiteboard(id))
            .map_or(self.page, |i| i + 1);
        self.store_dirty = true;
        self.dirty_for_export = true;
        self.overview = false;
        self.view_menu_open = false;
        self.go(new_slide as f64);
        if !matches!(self.tool, StageTool::Pen | StageTool::Highlighter) {
            self.tool = StageTool::Pen;
        }
        self.set_status("Added blank whiteboard slide");
    }

    /// Removes an inserted whiteboard slide cleanly and keeps the active page clamped.
    pub(crate) fn remove_whiteboard(&mut self, id: usize) {
        self.finish_active_stroke();
        let before_len = self.store.whiteboards.len();
        self.store.whiteboards.retain(|w| w.id != id);
        if self.store.whiteboards.len() != before_len {
            let new_total = self.total();
            self.page = self.page.min(new_total.max(1));
            if let Some(fp) = self.frozen_page {
                self.frozen_page = Some(fp.min(new_total.max(1)));
            }
            self.store_dirty = true;
            self.dirty_for_export = true;
            self.set_status("Removed whiteboard slide");
        }
    }

    pub(crate) fn toggle_whiteboard_grid(&mut self, id: usize) {
        if let Some(wb) = self.store.whiteboard_mut(id) {
            wb.grid = !wb.grid;
            self.store_dirty = true;
            self.dirty_for_export = true;
        }
    }

    pub(crate) fn has_strokes_on_slide(&self, slide_num: usize) -> bool {
        match self.deck_item(slide_num) {
            Some(DeckItem::Pdf(idx)) => self
                .store
                .annotations
                .get(&(idx + 1))
                .is_some_and(|v| !v.is_empty()),
            Some(DeckItem::Whiteboard(id)) => self
                .store
                .whiteboard(id)
                .is_some_and(|w| !w.strokes.is_empty()),
            None => false,
        }
    }

    fn commit_stroke_on_slide(&mut self, slide_num: usize, stroke: StrokePath) {
        if stroke.points.is_empty() {
            return;
        }
        match self.deck_item(slide_num) {
            Some(DeckItem::Pdf(idx)) => {
                self.store
                    .annotations
                    .entry(idx + 1)
                    .or_default()
                    .push(stroke);
            }
            Some(DeckItem::Whiteboard(id)) => {
                if let Some(wb) = self.store.whiteboard_mut(id) {
                    wb.strokes.push(stroke);
                }
            }
            None => return,
        }
        self.store_dirty = true;
        self.dirty_for_export = true;
    }

    /// Returns `(1-based deck slide, section title)` for every PDF outline bookmark.
    pub(crate) fn deck_sections(&self) -> Vec<(usize, &str)> {
        self.sections
            .iter()
            .filter_map(|(&pdf_page, title)| {
                self.deck_index_for_pdf_page(pdf_page)
                    .map(|deck_slide| (deck_slide, title.as_str()))
            })
            .collect()
    }

    /// Active section title for any 1-based deck slide (including whiteboards inside a section).
    pub(crate) fn section_for_deck(&self, slide_num: usize) -> Option<&str> {
        let pdf_num = match self.deck_item(slide_num)? {
            DeckItem::Pdf(idx) => idx + 1,
            DeckItem::Whiteboard(id) => self.store.whiteboard(id)?.after_pdf_page,
        };
        if pdf_num == 0 {
            return None;
        }
        self.section_for(pdf_num)
    }

    /// Jumps to the next (`dir > 0`) or previous (`dir < 0`) PDF outline section.
    pub(crate) fn jump_section(&mut self, dir: isize) {
        let secs = self.deck_sections();
        if secs.is_empty() {
            return;
        }
        let target = if dir > 0 {
            secs.iter().find(|&&(s, _)| s > self.page).map(|&(s, _)| s)
        } else {
            secs.iter().rfind(|&&(s, _)| s < self.page).map(|&(s, _)| s)
        };
        if let Some(slide) = target {
            self.go(slide as f64);
        }
    }

    /// Flushes elapsed seconds on `self.page` into `slide_timings` while the timer is running.
    pub(crate) fn record_current_slide_time(&mut self) {
        if self.timer.running {
            let dt = self.slide_entered_at.elapsed().as_secs_f64();
            if dt > 0.0 {
                *self.slide_timings.entry(self.page).or_insert(0.0) += dt;
            }
        }
        self.slide_entered_at = Instant::now();
    }

    pub(crate) fn toggle_timer(&mut self) {
        self.record_current_slide_time();
        self.timer.toggle();
        self.slide_entered_at = Instant::now();
    }

    pub(crate) fn reset_timer(&mut self) {
        self.timer.reset();
        self.slide_timings.clear();
        self.slide_entered_at = Instant::now();
    }

    /// Total seconds spent on 1-based deck `slide_num` during this session.
    pub(crate) fn slide_elapsed(&self, slide_num: usize) -> f64 {
        let saved = self.slide_timings.get(&slide_num).copied().unwrap_or(0.0);
        if slide_num == self.page && self.timer.running {
            saved + self.slide_entered_at.elapsed().as_secs_f64()
        } else {
            saved
        }
    }

    /// Top `limit` slides by cumulative rehearsal time (only includes slides with >= 1s).
    pub(crate) fn slowest_slides(&self, limit: usize) -> Vec<(usize, f64)> {
        let total = self.total();
        let mut list: Vec<(usize, f64)> = (1..=total)
            .filter_map(|s| {
                let t = self.slide_elapsed(s);
                (t >= 1.0).then_some((s, t))
            })
            .collect();
        list.sort_by(|a, b| b.1.total_cmp(&a.1));
        list.truncate(limit);
        list
    }

    /// Resolves the presenter console palette: respects `store.force_dark` auditorium override,
    /// otherwise follows Lemmata's active app theme.
    pub(crate) fn effective_palette<'a>(&self, app_pal: &'a Palette) -> &'a Palette {
        if self.store.force_dark {
            &theme::DARK
        } else {
            app_pal
        }
    }

    /// True if the Speaker notes panel should be shown right now: respects explicit user toggle
    /// in `store.show_notes`, otherwise auto-shows only when the deck has notes or Beamer notes.
    pub(crate) fn notes_visible(&self) -> bool {
        self.store.show_notes.unwrap_or_else(|| {
            self.has_any_notes
                || self.store.has_any_custom_notes()
                || self.tex_notes.values().any(|s| !s.trim().is_empty())
        })
    }

    pub(crate) fn toggle_notes_panel(&mut self) {
        let next = !self.notes_visible();
        self.store.show_notes = Some(next);
        if !next {
            self.editing_notes = false;
        }
        self.store_dirty = true;
    }

    /// Closes the presenter console viewport and resets all presenter-only transient states.
    pub(crate) fn close_presenter_console(&mut self) {
        self.presenter = false;
        self.shortcuts_open = false;
        self.editing_notes = false;
        self.editing_slide_jump = false;
        self.target_menu_open = false;
        self.export_menu_open = false;
        self.view_menu_open = false;
        self.sections_menu_open = false;
        if self.pointer_source == Some("presenter-stage") {
            self.pointer_uv = None;
            self.pointer_at = None;
            self.pointer_source = None;
        }
        self.finish_active_stroke();
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
        self.sizes.len() + self.store.whiteboards.len()
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
        if !matches!(
            self.tool,
            StageTool::Laser | StageTool::Spotlight | StageTool::Zoom
        ) {
            self.pointer_uv = None;
            self.pointer_at = None;
            self.pointer_source = None;
            self.laser_trail.clear();
        }
    }

    pub(crate) fn undo_stroke(&mut self) {
        match self.deck_item(self.page) {
            Some(DeckItem::Pdf(idx)) => {
                let slide = idx + 1;
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
            Some(DeckItem::Whiteboard(id)) => {
                if let Some(wb) = self.store.whiteboard_mut(id)
                    && wb.strokes.pop().is_some()
                {
                    self.store_dirty = true;
                    self.dirty_for_export = true;
                }
            }
            None => {}
        }
    }

    pub(crate) fn clear_slide_strokes(&mut self) {
        match self.deck_item(self.page) {
            Some(DeckItem::Pdf(idx)) => {
                if self.store.annotations.remove(&(idx + 1)).is_some() {
                    self.store_dirty = true;
                    self.dirty_for_export = true;
                }
            }
            Some(DeckItem::Whiteboard(id)) => {
                if let Some(wb) = self.store.whiteboard_mut(id)
                    && !wb.strokes.is_empty()
                {
                    wb.strokes.clear();
                    self.store_dirty = true;
                    self.dirty_for_export = true;
                }
            }
            None => {}
        }
    }

    pub(crate) fn finish_active_stroke(&mut self) {
        self.stroke_anchor_uv = None;
        if let Some(stroke) = self.active_stroke.take()
            && !stroke.points.is_empty()
        {
            self.commit_stroke_on_slide(self.page, stroke);
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

    /// Navigates to 1-based deck slide `to`, starting the timer on the first slide change.
    pub(crate) fn go(&mut self, to: f64) {
        self.finish_active_stroke();
        let target = clamp_page(to, self.total());
        if target != self.page {
            self.record_current_slide_time();
            self.page = target;
            self.slide_entered_at = Instant::now();
            self.filmstrip_scroll_pending = true;
            self.editing_notes = false;
            self.editing_slide_jump = false;
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
                    // Keep up to MAX_RENDERED_SIZES_PER_PAGE sizes per page (audience, presenter,
                    // next card, Beamer notes card, filmstrip/overview) to avoid eviction thrash.
                    let mut sizes: Vec<_> = self
                        .tex
                        .iter()
                        .filter(|(k, _)| k.0 == page)
                        .map(|(k, (_, used))| (*used, *k))
                        .collect();
                    sizes.sort_unstable_by_key(|s| std::cmp::Reverse(*s));
                    for (_, k) in sizes.into_iter().skip(MAX_RENDERED_SIZES_PER_PAGE) {
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
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return None;
        }
        let size = *self.sizes.get(page)?;
        if !size.x.is_finite() || !size.y.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
            return None;
        }
        let (w, h, x) = region_size(size.x, size.y, region);
        let k = fit_scale(w, h, rect.width(), rect.height());
        if k <= 0.0 || !k.is_finite() {
            return None;
        }
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
                offset: [0, 3],
                blur: 10,
                spread: 0,
                color: Color32::from_black_alpha(44),
            }
            .as_shape(at, 2),
        );
        ui.painter().rect_filled(at, 2, Color32::WHITE);
        ui.painter().rect_stroke(
            at,
            2,
            egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(16, 24, 32, 22)),
            egui::StrokeKind::Inside,
        );
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

    /// Draws either a PDF slide or an inserted blank whiteboard slide for 1-based deck `slide_num`.
    /// Returns `(slide_rect, slide_region)` on screen.
    pub(crate) fn draw_deck_item(
        &mut self,
        ui: &Ui,
        rect: Rect,
        slide_num: usize,
    ) -> Option<(Rect, Region)> {
        match self.deck_item(slide_num)? {
            DeckItem::Pdf(page_idx) => {
                let (slide, _) = self.page_regions(page_idx);
                self.draw(ui, rect, page_idx, slide).map(|sr| (sr, slide))
            }
            DeckItem::Whiteboard(id) => {
                let grid = self.store.whiteboard(id).is_some_and(|w| w.grid);
                self.draw_whiteboard(ui, rect, grid)
                    .map(|sr| (sr, Region::Full))
            }
        }
    }

    /// Paints a blank drawable whiteboard slide matching the deck's aspect ratio, with optional
    /// subtle coordinate grid lines for neat derivations and diagrams.
    fn draw_whiteboard(&self, ui: &Ui, rect: Rect, grid: bool) -> Option<Rect> {
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return None;
        }
        let (w, h) = self
            .sizes
            .first()
            .map(|s| {
                let (r, _) = regions_swapped(s.x, s.y, self.store.swap_notes_half);
                let (rw, rh, _) = region_size(s.x, s.y, r);
                (rw, rh)
            })
            .filter(|&(w, h)| w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0)
            .unwrap_or((455.0, 256.0));
        let k = fit_scale(w, h, rect.width(), rect.height());
        if k <= 0.0 || !k.is_finite() {
            return None;
        }
        let at = Rect::from_center_size(rect.center(), vec2(w * k, h * k));
        ui.painter().add(
            egui::Shadow {
                offset: [0, 3],
                blur: 10,
                spread: 0,
                color: Color32::from_black_alpha(44),
            }
            .as_shape(at, 2),
        );
        ui.painter().rect_filled(at, 2, Color32::WHITE);
        if grid && at.width() >= 48.0 && at.height() >= 36.0 {
            let p = ui.painter().with_clip_rect(at);
            let step = (at.height() / 14.0).clamp(12.0, 34.0);
            let stroke = egui::Stroke::new(0.8, Color32::from_rgba_unmultiplied(42, 113, 164, 34));
            let mut x = at.left() + step;
            while x < at.right() - 1.0 {
                p.vline(x, at.y_range(), stroke);
                x += step;
            }
            let mut y = at.top() + step;
            while y < at.bottom() - 1.0 {
                p.hline(at.x_range(), y, stroke);
                y += step;
            }
        }
        ui.painter().rect_stroke(
            at,
            2,
            egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(16, 24, 32, 22)),
            egui::StrokeKind::Inside,
        );
        Some(at)
    }

    /// Updates laser/spotlight/zoom/pen/highlighter interaction on a displayed slide rectangle.
    /// Returns `true` if the pointer was consumed by a drawing/stage tool.
    pub(crate) fn interact_stage(
        &mut self,
        ui: &mut Ui,
        slide_rect: Rect,
        slide_num: usize,
        id_salt: &'static str,
        allow_draw: bool,
    ) -> bool {
        if self.tool == StageTool::None {
            return false;
        }
        let sense = match self.tool {
            StageTool::Laser | StageTool::Spotlight | StageTool::Zoom => {
                egui::Sense::click_and_drag()
            }
            StageTool::Pen | StageTool::Highlighter if allow_draw => egui::Sense::click_and_drag(),
            _ => egui::Sense::hover(),
        };
        let resp = ui.interact(slide_rect, ui.scope_id().with(id_salt), sense);
        match self.tool {
            StageTool::Laser | StageTool::Spotlight | StageTool::Zoom => {
                let active_pos = if resp.is_pointer_button_down_on() || resp.dragged() {
                    resp.interact_pointer_pos()
                        .or_else(|| ui.input(|i| i.pointer.latest_pos()))
                        .and_then(|p| to_norm(p, slide_rect))
                } else {
                    None
                };
                if let Some(pos) = active_pos {
                    let now = Instant::now();
                    self.pointer_uv = Some(pos);
                    self.pointer_at = Some(now);
                    self.pointer_source = Some(id_salt);
                    if self.tool == StageTool::Laser {
                        self.laser_trail.push_back((pos, now));
                        while self.laser_trail.len() > 14 {
                            self.laser_trail.pop_front();
                        }
                    }
                    ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(16));
                } else {
                    if self.pointer_source == Some(id_salt) {
                        self.pointer_uv = None;
                        self.pointer_at = None;
                        self.pointer_source = None;
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(if self.tool == StageTool::Zoom {
                            egui::CursorIcon::ZoomIn
                        } else {
                            egui::CursorIcon::Crosshair
                        });
                    }
                }
                true
            }
            StageTool::Pen | StageTool::Highlighter if allow_draw => {
                let is_hl = self.tool == StageTool::Highlighter;
                let width = if is_hl { 0.022 } else { 0.0055 };
                let shift_held = ui.input(|i| i.modifiers.shift);
                if resp.hovered() || resp.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                }
                if resp.drag_started() {
                    let origin = ui
                        .input(|i| i.pointer.press_origin())
                        .and_then(|p| to_norm(p, slide_rect));
                    let cur = resp
                        .interact_pointer_pos()
                        .and_then(|p| to_norm(p, slide_rect));
                    if let Some(anchor) = origin.or(cur) {
                        self.stroke_anchor_uv = Some(anchor);
                        let mut pts = vec![anchor];
                        if let Some(uv) = cur
                            && uv != anchor
                        {
                            pts.push(if shift_held {
                                snap_straight_line(anchor, uv, slide_rect)
                            } else {
                                uv
                            });
                        }
                        self.active_stroke = Some(StrokePath {
                            color: self.pen_color,
                            width,
                            highlighter: is_hl,
                            points: pts,
                        });
                    }
                } else if resp.dragged() {
                    if let Some(uv) = resp
                        .interact_pointer_pos()
                        .and_then(|p| to_norm(p, slide_rect))
                    {
                        let anchor = *self.stroke_anchor_uv.get_or_insert(uv);
                        let stroke = self.active_stroke.get_or_insert_with(|| StrokePath {
                            color: self.pen_color,
                            width,
                            highlighter: is_hl,
                            points: Vec::new(),
                        });
                        if shift_held {
                            let snapped = snap_straight_line(anchor, uv, slide_rect);
                            stroke.points.clear();
                            stroke.points.push(anchor);
                            stroke.points.push(snapped);
                        } else {
                            let push = stroke.points.last().is_none_or(|last| {
                                let dx = last[0] - uv[0];
                                let dy = last[1] - uv[1];
                                dx * dx + dy * dy > 0.000012
                            });
                            if push {
                                stroke.points.push(uv);
                            }
                        }
                    }
                } else if resp.drag_stopped() {
                    self.stroke_anchor_uv = None;
                    if let Some(stroke) = self.active_stroke.take()
                        && !stroke.points.is_empty()
                    {
                        self.commit_stroke_on_slide(slide_num, stroke);
                    }
                } else if resp.clicked()
                    && let Some(uv) = resp
                        .interact_pointer_pos()
                        .and_then(|p| to_norm(p, slide_rect))
                {
                    self.commit_stroke_on_slide(
                        slide_num,
                        StrokePath {
                            color: self.pen_color,
                            width,
                            highlighter: is_hl,
                            points: vec![uv],
                        },
                    );
                }
                true
            }
            _ => false,
        }
    }

    /// Paints persisted strokes, active drawing stroke, and live laser/spotlight/zoom overlay onto
    /// `slide_rect` for 1-based deck `slide_num`.
    pub(crate) fn paint_overlays(
        &mut self,
        ui: &Ui,
        slide_rect: Rect,
        slide_num: usize,
        show_live_pointer: bool,
    ) {
        let painter = ui.painter().with_clip_rect(slide_rect);
        let item = self.deck_item(slide_num);
        // 1. Committed strokes for this slide (PDF page or Whiteboard).
        match item {
            Some(DeckItem::Pdf(idx)) => {
                if let Some(strokes) = self.store.annotations.get(&(idx + 1)) {
                    for s in strokes {
                        paint_stroke(&painter, slide_rect, s);
                    }
                }
            }
            Some(DeckItem::Whiteboard(id)) => {
                if let Some(wb) = self.store.whiteboard(id) {
                    for s in &wb.strokes {
                        paint_stroke(&painter, slide_rect, s);
                    }
                }
            }
            None => {}
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
            .is_some_and(|(_, t)| now.duration_since(*t).as_secs_f32() > 0.20)
        {
            self.laser_trail.pop_front();
        }
        let fresh_pointer = self
            .pointer_at
            .is_some_and(|t| now.duration_since(t).as_secs_f32() < 0.5);
        if !fresh_pointer {
            self.pointer_uv = None;
            self.pointer_source = None;
        }
        if !self.laser_trail.is_empty() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        } else if self.pointer_uv.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        // 3. Laser trail (fades smoothly right after button release) & active Laser / Spotlight / Zoom.
        if self.tool == StageTool::Laser && self.laser_trail.len() >= 2 {
            let pts: Vec<(Pos2, f32)> = self
                .laser_trail
                .iter()
                .map(|(p, t)| {
                    let age = now.duration_since(*t).as_secs_f32();
                    let a = (1.0 - age / 0.20).clamp(0.0, 1.0);
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
        }
        if let Some(uv) = self.pointer_uv {
            let center = from_norm(uv, slide_rect);
            match self.tool {
                StageTool::Laser => {
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
                    let radius = (slide_rect.height() * 0.085).clamp(24.0, 76.0);
                    let outer_r = slide_rect.width().hypot(slide_rect.height()) + 4.0;
                    let scrim = Color32::from_rgba_unmultiplied(5, 8, 12, 165);
                    let segs: u32 = 48;
                    let mut mesh = egui::Mesh::default();
                    mesh.vertices.reserve((segs * 2) as usize);
                    mesh.indices.reserve((segs * 6) as usize);
                    for i in 0..segs {
                        let ang = (i as f32 / segs as f32) * std::f32::consts::TAU;
                        let (sin, cos) = ang.sin_cos();
                        mesh.vertices.push(egui::epaint::Vertex {
                            pos: pos2(center.x + radius * cos, center.y + radius * sin),
                            uv: egui::epaint::WHITE_UV,
                            color: scrim,
                        });
                    }
                    for i in 0..segs {
                        let ang = (i as f32 / segs as f32) * std::f32::consts::TAU;
                        let (sin, cos) = ang.sin_cos();
                        mesh.vertices.push(egui::epaint::Vertex {
                            pos: pos2(center.x + outer_r * cos, center.y + outer_r * sin),
                            uv: egui::epaint::WHITE_UV,
                            color: scrim,
                        });
                    }
                    for i in 0..segs {
                        let j = (i + 1) % segs;
                        mesh.indices
                            .extend_from_slice(&[i, j, segs + j, i, segs + j, segs + i]);
                    }
                    painter.add(egui::Shape::mesh(mesh));
                    painter.circle_stroke(
                        center,
                        radius,
                        egui::Stroke::new(
                            1.75,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 195),
                        ),
                    );
                }
                StageTool::Zoom => {
                    self.paint_zoom_lens(ui, slide_rect, item, uv);
                }
                _ => {}
            }
        }
    }

    /// Paints a synced 2.4x magnifier lens centered on `uv` (clamped neatly inside `slide_rect`),
    /// requesting a high-resolution PDF texture so magnified plots and equations stay sharp.
    fn paint_zoom_lens(&mut self, ui: &Ui, slide_rect: Rect, item: Option<DeckItem>, uv: [f32; 2]) {
        let zoom = 2.4_f32;
        let lens_w = (slide_rect.width() * 0.36).clamp(110.0, 340.0);
        let lens_h = (slide_rect.height() * 0.36).clamp(84.0, 240.0);
        if slide_rect.width() < lens_w * 0.8 || slide_rect.height() < lens_h * 0.8 {
            return;
        }
        let target = from_norm(uv, slide_rect);
        let half = vec2(lens_w * 0.5, lens_h * 0.5);
        let cx = target.x.clamp(
            slide_rect.left() + half.x + 4.0,
            (slide_rect.right() - half.x - 4.0).max(slide_rect.left() + half.x + 4.0),
        );
        let cy = target.y.clamp(
            slide_rect.top() + half.y + 4.0,
            (slide_rect.bottom() - half.y - 4.0).max(slide_rect.top() + half.y + 4.0),
        );
        let lens = Rect::from_center_size(pos2(cx, cy), vec2(lens_w, lens_h));
        let rad = egui::CornerRadius::same(12);

        // Request a sharper 2.4x render of the PDF page so fine print stays razor sharp.
        let mut tex_and_region: Option<(egui::TextureId, f32, f32)> = None;
        if let Some(DeckItem::Pdf(page_idx)) = item
            && let Some(&size) = self.sizes.get(page_idx)
            && size.x > 0.0
            && size.y > 0.0
        {
            let (region, _) = self.page_regions(page_idx);
            let (rw, rh, rx) = region_size(size.x, size.y, region);
            let k = fit_scale(rw, rh, slide_rect.width(), slide_rect.height());
            let hi_scale = (k * zoom * ui.ctx().pixels_per_point()).clamp(0.5, 6.0);
            let key = (page_idx, scale_key(hi_scale));
            if !self.tex.contains_key(&key) && self.asked.insert(key) {
                self.engine.send(Job::Render {
                    doc: self.doc,
                    page: page_idx,
                    scale: key.1 as f32 / 16.0,
                    dark: false,
                    epoch: None,
                    thumb: false,
                });
            }
            if let Some(t) = self.tex.get(&key).map(|(t, _)| t).or_else(|| {
                self.tex
                    .iter()
                    .filter(|(k, _)| k.0 == page_idx)
                    .max_by_key(|(k, _)| k.1)
                    .map(|(_, (t, _))| t)
            }) {
                tex_and_region = Some((t.id(), rx / size.x, rw / size.x));
            }
        }

        let painter = ui.painter().with_clip_rect(slide_rect);
        painter.add(
            egui::Shadow {
                offset: [0, 8],
                blur: 24,
                spread: 0,
                color: Color32::from_black_alpha(120),
            }
            .as_shape(lens, rad),
        );
        painter.rect_filled(lens, rad, Color32::WHITE);

        // Compute virtual zoomed slide rectangle aligned so `uv` maps to `lens.center()`.
        let z_size = slide_rect.size() * zoom;
        let z_min = lens.center() - vec2(uv[0] * z_size.x, uv[1] * z_size.y);
        let z_slide = Rect::from_min_size(z_min, z_size);
        let lens_clip = ui.painter().with_clip_rect(lens.shrink(1.5));

        if let Some((tex_id, u_start, u_span)) = tex_and_region {
            lens_clip.image(
                tex_id,
                z_slide,
                Rect::from_min_max(pos2(u_start, 0.0), pos2(u_start + u_span, 1.0)),
                Color32::WHITE,
            );
        }
        // Magnify committed annotations inside the lens too.
        match item {
            Some(DeckItem::Pdf(idx)) => {
                if let Some(strokes) = self.store.annotations.get(&(idx + 1)) {
                    for s in strokes {
                        paint_stroke(&lens_clip, z_slide, s);
                    }
                }
            }
            Some(DeckItem::Whiteboard(id)) => {
                if let Some(wb) = self.store.whiteboard(id) {
                    for s in &wb.strokes {
                        paint_stroke(&lens_clip, z_slide, s);
                    }
                }
            }
            None => {}
        }

        painter.rect_stroke(
            lens,
            rad,
            egui::Stroke::new(2.0, Color32::from_rgb(0x2a, 0x71, 0xa4)),
            egui::StrokeKind::Inside,
        );
    }

    /// One frame of the presentation (audience window + optional presenter viewport).
    /// Returns `false` when the presentation has ended.
    pub fn show(&mut self, ctx: &egui::Context, pal: &Palette) -> bool {
        self.receive(ctx);
        if !self.entered {
            self.entered = true;
            ctx.send_viewport_cmd(ViewportCommand::Fullscreen(true));
        }
        let mut keep = self.keys(ctx, false);
        stage::audience_ui(self, ctx, pal, &mut keep);

        if self.presenter && keep {
            keep = self.presenter_window(ctx, pal);
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
    fn presenter_window(&mut self, ctx: &egui::Context, pal: &Palette) -> bool {
        let mut keep = true;
        ctx.show_viewport_immediate(
            ViewportId::from_hash_of("presenter"),
            ViewportBuilder::default()
                .with_title("Lemmata presenter view")
                .with_inner_size([1240.0, 780.0]),
            |ctx, _| {
                if ctx.input(|i| i.viewport().close_requested()) {
                    self.close_presenter_console();
                    return;
                }
                if self.presenter_fullscreen_dirty {
                    self.presenter_fullscreen_dirty = false;
                    ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.presenter_fullscreen));
                }
                let eff_pal = self.effective_palette(pal);
                theme::apply(ctx, eff_pal);
                if !self.keys(ctx, true) {
                    keep = false;
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(eff_pal.app))
                    .show(ctx, |ui| {
                        if !self.presenter_ui(ui, pal) {
                            keep = false;
                        }
                    });
            },
        );
        keep
    }

    /// Renders the presenter console inside `ui`. Returns `false` when the user ends the show.
    pub(crate) fn presenter_ui(&mut self, ui: &mut Ui, pal: &Palette) -> bool {
        stage::presenter_ui(self, ui, pal)
    }

    /// Shared keyboard handler for both audience and presenter windows.
    /// Returns `false` on `Esc` when no overlay or blanking state is active.
    pub(crate) fn keys(&mut self, ctx: &egui::Context, presenter: bool) -> bool {
        // Clear hidden notes-editing mode if overview, hidden notes panel, or Beamer second-screen
        // notes replaced it.
        if self.editing_notes
            && (self.overview || !self.notes_visible() || self.current_slide_has_beamer_notes())
        {
            self.editing_notes = false;
        }
        // When typing inside inline speaker notes or slide-jump text box, Escape or Cmd/Ctrl+Enter
        // exits editing cleanly.
        if presenter && (self.editing_notes || self.editing_slide_jump) {
            let exit = ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    || (self.editing_notes
                        && i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter))
            });
            if exit {
                self.editing_notes = false;
                self.editing_slide_jump = false;
            }
            return true;
        }
        // While shortcuts overlay or a popover menu is open, block all underlying stage shortcuts
        // so keys cannot mutate slides/tools/blanking behind the overlay.
        if self.shortcuts_open
            || self.target_menu_open
            || self.export_menu_open
            || self.view_menu_open
            || self.sections_menu_open
        {
            let (esc, toggle_help) = ctx.input_mut(|i| {
                use egui::{Key, Modifiers};
                let esc = i.consume_key(Modifiers::NONE, Key::Escape);
                let help = self.shortcuts_open
                    && (i.consume_key(Modifiers::NONE, Key::Questionmark)
                        || i.consume_key(Modifiers::SHIFT, Key::Slash)
                        || i.consume_key(Modifiers::NONE, Key::H));
                (esc, help)
            });
            if esc || toggle_help {
                if self.shortcuts_open {
                    self.shortcuts_open = false;
                } else {
                    self.target_menu_open = false;
                    self.export_menu_open = false;
                    self.view_menu_open = false;
                    self.sections_menu_open = false;
                }
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
        let mut next_section = false;
        let mut prev_section = false;
        let mut keep = true;
        let mut enter = false;
        let mut digit: Option<char> = None;
        let mut backspace = false;
        let mut toggle_blackout = false;
        let mut toggle_whiteout = false;
        let mut toggle_freeze = false;
        let mut toggle_overview = false;
        let mut toggle_filmstrip = false;
        let mut toggle_notes = false;
        let mut toggle_shortcuts = false;
        let mut toggle_laser = false;
        let mut toggle_spotlight = false;
        let mut toggle_zoom = false;
        let mut toggle_pen = false;
        let mut toggle_highlighter = false;
        let mut insert_whiteboard = false;
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
            if i.consume_key(Modifiers::SHIFT, Key::ArrowRight) {
                next_section = true;
            }
            if i.consume_key(Modifiers::SHIFT, Key::ArrowLeft) {
                prev_section = true;
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
            if presenter && i.consume_key(Modifiers::NONE, Key::N) {
                toggle_notes = true;
            }
            if i.consume_key(Modifiers::NONE, Key::K) {
                insert_whiteboard = true;
            }
            if i.consume_key(Modifiers::NONE, Key::L) {
                toggle_laser = true;
            }
            if i.consume_key(Modifiers::NONE, Key::S) {
                toggle_spotlight = true;
            }
            if i.consume_key(Modifiers::NONE, Key::Z) {
                toggle_zoom = true;
            }
            if i.consume_key(Modifiers::NONE, Key::D) {
                toggle_pen = true;
            }
            if i.consume_key(Modifiers::NONE, Key::U) {
                toggle_highlighter = true;
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
                } else if self.target_menu_open
                    || self.export_menu_open
                    || self.view_menu_open
                    || self.sections_menu_open
                {
                    self.target_menu_open = false;
                    self.export_menu_open = false;
                    self.view_menu_open = false;
                    self.sections_menu_open = false;
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
            } else if self.overview {
                self.overview = false;
            } else if !self.shortcuts_open {
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
            if self.overview {
                self.editing_notes = false;
                self.editing_slide_jump = false;
            }
        }
        if toggle_filmstrip {
            self.store.filmstrip = !self.store.filmstrip;
            self.store_dirty = true;
        }
        if toggle_notes {
            self.toggle_notes_panel();
        }
        if insert_whiteboard {
            self.insert_whiteboard_after_current();
        }
        if toggle_shortcuts {
            self.shortcuts_open = !self.shortcuts_open;
            if self.shortcuts_open {
                self.target_menu_open = false;
                self.export_menu_open = false;
                self.view_menu_open = false;
                self.sections_menu_open = false;
            }
        }
        if toggle_laser {
            self.toggle_tool(StageTool::Laser);
        }
        if toggle_spotlight {
            self.toggle_tool(StageTool::Spotlight);
        }
        if toggle_zoom {
            self.toggle_tool(StageTool::Zoom);
        }
        if toggle_pen {
            self.toggle_tool(StageTool::Pen);
        }
        if toggle_highlighter {
            self.toggle_tool(StageTool::Highlighter);
        }
        if clear_strokes {
            self.clear_slide_strokes();
        }
        if next_section {
            self.jump_section(1);
        } else if prev_section {
            self.jump_section(-1);
        } else if next {
            self.go(self.page as f64 + 1.0);
        } else if back {
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
            self.toggle_timer();
        }
        if r {
            self.reset_timer();
        }
        keep
    }
}

/// Snaps `end` cleanly to an exact horizontal or vertical line from `anchor` when within ~4 deg.
fn snap_straight_line(anchor: [f32; 2], end: [f32; 2], slide_rect: Rect) -> [f32; 2] {
    let dx = (end[0] - anchor[0]) * slide_rect.width();
    let dy = (end[1] - anchor[1]) * slide_rect.height();
    if dy.abs() < dx.abs() * 0.07 {
        [end[0], anchor[1]]
    } else if dx.abs() < dy.abs() * 0.07 {
        [anchor[0], end[1]]
    } else {
        end
    }
}

fn paint_stroke(painter: &egui::Painter, slide_rect: Rect, s: &StrokePath) {
    if s.points.is_empty() {
        return;
    }
    let width = if s.highlighter {
        (s.width * slide_rect.height()).clamp(6.0, 28.0)
    } else {
        (s.width * slide_rect.height()).clamp(1.8, 12.0)
    };
    let base = s.color.color32();
    let color = if s.highlighter {
        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 92)
    } else {
        base
    };
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

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    fn test_presentation(ctx: &egui::Context) -> Presentation {
        let mut p = Presentation::new(ctx, Arc::new(Vec::new()), 1);
        p.sizes = vec![
            vec2(362.83, 272.13),
            vec2(362.83, 272.13),
            vec2(362.83, 272.13),
        ];
        p
    }

    fn setup_fonts(ctx: &egui::Context) {
        crate::theme::install_fonts(ctx);
        crate::theme::apply(ctx, &crate::theme::DARK);
    }

    #[test]
    fn audience_hud_stays_stable_and_clickable_on_hover() {
        let mut h = egui_kittest::Harness::builder()
            .with_size([1200.0, 740.0])
            .with_context_setup(setup_fonts)
            .build_ui_state(
                |ui, state: &mut (Option<Presentation>, bool)| {
                    let p = state.0.get_or_insert_with(|| test_presentation(ui.ctx()));
                    stage::audience_ui(p, ui.ctx(), &crate::theme::DARK, &mut state.1);
                },
                (None, true),
            );
        h.step();

        // Move pointer into the bottom HUD bar region.
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(pos2(600.0, 740.0 - 32.0)));
        h.step();

        let presenter_rect = h
            .query_by_label("Presenter")
            .expect("HUD should appear when pointer enters bottom bar")
            .rect();
        // Move pointer directly onto the Presenter button and hold steady PAST the 2.2s idle
        // auto-hide window (via simulated time), verifying zero oscillation and no premature hide.
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(presenter_rect.center()));
        for frame in 0..12 {
            crate::testing::advance_time_and_step(&mut h, 0.3);
            assert!(
                h.query_by_label("Presenter").is_some(),
                "audience bottom HUD flickered or hid while hovered on frame {frame}"
            );
        }
        crate::testing::assert_settles(&mut h, "audience HUD hovered");
        crate::testing::assert_ui_geometry(&h, "audience HUD hovered");

        // Clicking a non-button gap inside the HUD pill must be blocked and NOT advance the slide.
        let hud_gap = pos2(presenter_rect.left() - 4.0, presenter_rect.center().y);
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(hud_gap));
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: hud_gap,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: hud_gap,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        assert_eq!(
            h.state().0.as_ref().unwrap().page,
            1,
            "clicking HUD pill padding should not advance slide"
        );

        // Clicking Presenter activates presenter mode without advancing the slide.
        h.get_by_label("Presenter").click();
        h.step();
        {
            let p = h.state().0.as_ref().unwrap();
            assert!(
                p.presenter,
                "clicking Presenter should enable presenter mode"
            );
            assert_eq!(
                p.page, 1,
                "clicking Presenter should not fall through to advance slide"
            );
        }

        // Clicking Next slide advances from slide 1 to slide 2.
        h.get_by_label("Next slide").click();
        h.step();
        assert_eq!(h.state().0.as_ref().unwrap().page, 2);

        // Clicking Annotate toggles Pen on AND keeps the HUD usable while hovered so Pen can be
        // toggled back off cleanly.
        h.get_by_label("Annotate slide (D)").click();
        h.step();
        assert_eq!(h.state().0.as_ref().unwrap().tool, StageTool::Pen);
        assert!(
            h.query_by_label("Annotate slide (D)").is_some(),
            "HUD should remain interactive while hovered even with Pen active"
        );
        // While Pen is active + slide frozen, clicking the stage must NOT advance slides behind
        // the frozen audience screen (PRES-14).
        h.state_mut().0.as_mut().unwrap().frozen_page = Some(2);
        let stage_pt = pos2(900.0, 300.0);
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(stage_pt));
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: stage_pt,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: stage_pt,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        assert_eq!(
            h.state().0.as_ref().unwrap().page,
            2,
            "stage click with frozen+Pen should not advance slide"
        );

        // Move back to HUD and toggle Pen off.
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(presenter_rect.center()));
        h.step();
        h.get_by_label("Annotate slide (D)").click();
        h.step();
        assert_eq!(h.state().0.as_ref().unwrap().tool, StageTool::None);
    }

    #[test]
    fn presenter_footer_popovers_isolation_and_viewport_matrix() {
        // Contract: per-page texture cache holds at least 4 sizes (audience + presenter + Beamer
        // notes + filmstrip) so Presenter View never enters an eviction/re-render loop (PRES-02).
        const { assert!(MAX_RENDERED_SIZES_PER_PAGE >= 4) };

        let mut h = crate::testing::cpu_harness_state(
            [1180.0, 740.0],
            crate::theme::LIGHT,
            |ui, state: &mut (Option<Presentation>, bool)| {
                let p = state.0.get_or_insert_with(|| test_presentation(ui.ctx()));
                state.1 = p.presenter_ui(ui, &crate::theme::LIGHT);
            },
            (None, true),
        );
        h.step();

        // Speaker notes auto-hide when the deck has no notes; View popover toggles them on/off
        // and updates panel visibility cleanly.
        assert!(
            !h.state().0.as_ref().unwrap().notes_visible(),
            "notes panel should auto-hide on decks with no speaker notes"
        );
        h.get_by_label("View").click();
        h.run_steps(2);
        assert!(h.state().0.as_ref().unwrap().view_menu_open);
        h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Speaker notes")
            .click();
        h.run_steps(2);
        assert!(
            h.state().0.as_ref().unwrap().notes_visible(),
            "View switch should show Speaker notes panel"
        );
        h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Next slide")
            .click();
        h.run_steps(2);
        assert!(
            !h.state().0.as_ref().unwrap().store.show_next,
            "View switch should hide Next slide panel"
        );
        // Dismiss View popover by clicking outside.
        h.state_mut().0.as_mut().unwrap().view_menu_open = false;
        h.state_mut().0.as_mut().unwrap().store.show_next = true;
        h.run_steps(2);

        // Open Save PDF popover, verify clicking directly over the underlying Overview button
        // dismisses the popover via the foreground scrim WITHOUT toggling Overview (PRES-08).
        let overview_pos = h.get_by_label("Overview").rect().center();
        h.get_by_label("Save PDF").click();
        h.run_steps(2);
        assert!(h.state().0.as_ref().unwrap().export_menu_open);
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(overview_pos));
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: overview_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: overview_pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(2);
        {
            let p = h.state().0.as_ref().unwrap();
            assert!(!p.export_menu_open, "outside click should close popover");
            assert!(
                !p.overview,
                "outside click on popover scrim must not leak release to Overview"
            );
        }

        // Open Save PDF popover again and trigger annotated handout export.
        h.get_by_label("Save PDF").click();
        h.run_steps(2);
        h.get_by_label("Slides + speaker notes (handout)…").click();
        h.run_steps(2);
        let p = h.state_mut().0.as_mut().unwrap();
        assert!(!p.export_menu_open);
        let actions = p.take_actions();
        assert!(matches!(
            actions.as_slice(),
            [PresentAction::ExportPdf {
                include_notes: true
            }]
        ));

        // Keys behind overlays/popovers must not mutate slides/blanking (PRES-05),
        // overview/go clears editing_notes (PRES-07), and close_presenter_console resets flags (PRES-06).
        p.editing_notes = true;
        p.go(2.0);
        assert!(!p.editing_notes, "go() should clear editing_notes");
        p.shortcuts_open = true;
        h.key_press(egui::Key::ArrowRight);
        let ctx = h.ctx.clone();
        let keep = h.state_mut().0.as_mut().unwrap().keys(&ctx, true);
        assert!(keep);
        assert_eq!(
            h.state().0.as_ref().unwrap().page,
            2,
            "ArrowRight behind shortcuts_open must not advance slide"
        );
        h.state_mut().0.as_mut().unwrap().close_presenter_console();
        {
            let p = h.state().0.as_ref().unwrap();
            assert!(
                !p.presenter
                    && !p.shortcuts_open
                    && !p.editing_notes
                    && !p.target_menu_open
                    && !p.view_menu_open
            );
        }

        // Sweep every viewport size from 1400x900 down to 35x35 across overlays, modes, and BOTH
        // light and dark themes (catches PRES-03, PRES-04, PRES-09, PRES-10, PRES-17, PRES-18).
        for (theme_name, pal) in [("light", crate::theme::LIGHT), ("dark", crate::theme::DARK)] {
            for &size in crate::testing::VIEWPORT_MATRIX {
                for mode in 0..8u8 {
                    let mut mh = crate::testing::cpu_harness_state(
                        size,
                        pal,
                        move |ui, state: &mut Option<Presentation>| {
                            let p = state.get_or_insert_with(|| {
                                let mut pr = test_presentation(ui.ctx());
                                pr.store.target_minutes = Some(15);
                                pr.store.show_notes = Some(mode != 5);
                                pr.store.notes_on_right = mode == 6;
                                pr.store.force_dark = mode == 6;
                                pr.store.filmstrip = mode == 1;
                                pr.target_menu_open = mode == 2;
                                pr.export_menu_open = mode == 3;
                                pr.shortcuts_open = mode == 4;
                                pr.view_menu_open = mode == 5;
                                pr.overview = mode == 7;
                                if mode == 6 {
                                    pr.frozen_page = Some(2);
                                    pr.tool = StageTool::Laser;
                                    pr.tex_notes.insert(1, "# Intro\n- Key point".into());
                                    pr.tex_notes.insert(2, "Transition to results".into());
                                }
                                pr
                            });
                            p.presenter_ui(ui, &pal);
                        },
                        None,
                    );
                    mh.run_steps(2);
                    crate::testing::assert_ui_geometry(
                        &mh,
                        &format!("presenter_ui theme={theme_name} size={size:?} mode={mode}"),
                    );
                }
            }
        }
    }

    #[test]
    fn presenter_customization_notes_swap_dark_stage_and_frozen_push() {
        let mut h = crate::testing::cpu_harness_state(
            [1180.0, 740.0],
            crate::theme::LIGHT,
            |ui, state: &mut Option<Presentation>| {
                let p = state.get_or_insert_with(|| {
                    let mut pr = test_presentation(ui.ctx());
                    pr.tex_notes.insert(1, "# Opening\n- Welcome".into());
                    pr.tex_notes
                        .insert(2, "Highlight the convergence proof".into());
                    pr
                });
                p.presenter_ui(ui, &crate::theme::LIGHT);
            },
            None,
        );
        h.run_steps(2);

        // Deck with .tex notes auto-shows Speaker notes and reports "From .tex" origin.
        {
            let p = h.state().as_ref().unwrap();
            assert!(p.notes_visible());
            assert_eq!(p.note_origin(1), Some("From .tex"));
            assert!(!p.can_reset_note_to_tex(1));
        }

        // Open View popover and toggle Notes on right + Dark stage.
        h.get_by_label("View").click();
        h.run_steps(2);
        h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Notes on right")
            .click();
        h.run_steps(2);
        h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Dark stage")
            .click();
        h.run_steps(2);
        {
            let p = h.state().as_ref().unwrap();
            assert!(p.store.notes_on_right);
            assert!(p.store.force_dark);
            assert!(p.effective_palette(&crate::theme::LIGHT).dark);
            assert_eq!(PenColor::ALL.len(), 4);
        }
        // Close View popover, freeze audience on slide 1, browse to slide 2, and push slide 2.
        h.state_mut().as_mut().unwrap().view_menu_open = false;
        h.state_mut().as_mut().unwrap().toggle_freeze();
        h.state_mut().as_mut().unwrap().go(2.0);
        h.run_steps(2);
        assert_eq!(h.state().as_ref().unwrap().frozen_page, Some(1));
        assert!(h.state().as_ref().unwrap().filmstrip_scroll_pending);
        h.get_by_label("Show this slide to the audience while staying frozen")
            .click();
        h.run_steps(2);
        assert_eq!(h.state().as_ref().unwrap().frozen_page, Some(2));

        // Overview + Enter closes overview cleanly onto the selected slide.
        let mut h_ov = crate::testing::cpu_harness_state(
            [1180.0, 740.0],
            crate::theme::LIGHT,
            |ui, state: &mut (Option<Presentation>, bool)| {
                let p = state.0.get_or_insert_with(|| {
                    let mut pr = test_presentation(ui.ctx());
                    pr.overview = true;
                    pr
                });
                state.1 = p.keys(ui.ctx(), true);
                p.presenter_ui(ui, &crate::theme::LIGHT);
            },
            (None, true),
        );
        h_ov.step();
        h_ov.key_press(egui::Key::ArrowRight);
        h_ov.step();
        assert_eq!(h_ov.state().0.as_ref().unwrap().page, 2);
        assert!(h_ov.state().0.as_ref().unwrap().overview);
        h_ov.key_press(egui::Key::Enter);
        h_ov.step();
        assert!(h_ov.state().1);
        assert!(!h_ov.state().0.as_ref().unwrap().overview);
        assert_eq!(h_ov.state().0.as_ref().unwrap().page, 2);
    }

    #[test]
    fn whiteboards_sections_highlighter_shift_lines_and_rehearsal_timings() {
        let mut h = crate::testing::cpu_harness_state(
            [1200.0, 740.0],
            crate::theme::LIGHT,
            |ui, state: &mut Option<Presentation>| {
                let p = state.get_or_insert_with(|| {
                    let mut pr = test_presentation(ui.ctx());
                    pr.sections.insert(1, "Introduction".into());
                    pr.sections.insert(3, "Appendix".into());
                    pr
                });
                p.keys(ui.ctx(), true);
                p.presenter_ui(ui, &crate::theme::LIGHT);
            },
            None,
        );
        h.run_steps(2);

        // Section quick-jump via Shift+ArrowRight / Shift+ArrowLeft and header popover.
        assert_eq!(h.state().as_ref().unwrap().page, 1);
        h.state_mut().as_mut().unwrap().jump_section(1);
        assert_eq!(h.state().as_ref().unwrap().page, 3);
        h.state_mut().as_mut().unwrap().jump_section(-1);
        assert_eq!(h.state().as_ref().unwrap().page, 1);
        h.run_steps(2);
        h.get_by_label("Introduction").click();
        h.run_steps(2);
        assert!(h.state().as_ref().unwrap().sections_menu_open);
        h.get_by_label("Appendix").click();
        h.run_steps(2);
        assert_eq!(h.state().as_ref().unwrap().page, 3);
        assert!(!h.state().as_ref().unwrap().sections_menu_open);

        // Insert a blank whiteboard slide after slide 3: switches to it and arms Pen automatically.
        h.get_by_label("Add blank whiteboard slide (K)").click();
        h.run_steps(2);
        let wb_id = {
            let p = h.state().as_ref().unwrap();
            assert_eq!(p.total(), 4);
            assert_eq!(p.page, 4);
            assert_eq!(p.tool, StageTool::Pen);
            p.current_whiteboard_id().expect("should be on whiteboard")
        };

        // Toggle Grid on whiteboard and switch to translucent Highlighter, drawing a Shift-snapped line.
        h.get_by_label("Grid").click();
        h.run_steps(2);
        assert!(
            h.state()
                .as_ref()
                .unwrap()
                .store
                .whiteboard(wb_id)
                .unwrap()
                .grid
        );

        h.state_mut()
            .as_mut()
            .unwrap()
            .toggle_tool(StageTool::Highlighter);
        h.run_steps(2);
        let p0 = pos2(420.0, 320.0);
        let p1 = pos2(660.0, 324.0);
        h.input_mut()
            .events
            .push(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
        h.input_mut().events.push(egui::Event::PointerMoved(p0));
        h.step();
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: p0,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::SHIFT,
        });
        h.step();
        h.input_mut().events.push(egui::Event::PointerMoved(p1));
        h.step();
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: p1,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::SHIFT,
        });
        h.input_mut()
            .events
            .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        h.run_steps(2);
        {
            let p = h.state().as_ref().unwrap();
            let wb = p.store.whiteboard(wb_id).unwrap();
            assert_eq!(wb.strokes.len(), 1);
            assert!(wb.strokes[0].highlighter);
            assert_eq!(
                wb.strokes[0].points.len(),
                2,
                "Shift-drag should produce a crisp 2-point straight line"
            );
            assert!(
                (wb.strokes[0].points[0][1] - wb.strokes[0].points[1][1]).abs() < 1e-5,
                "nearly horizontal Shift-line should snap cleanly to exact horizontal: {:?}",
                wb.strokes[0].points
            );
            assert_eq!(p.store.annotated_slide_count(), 1);

            // Direct geometry check for H/V snap and unconstrained diagonals.
            let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 450.0));
            let h_snap = snap_straight_line([0.2, 0.5], [0.7, 0.51], r);
            assert!((h_snap[1] - 0.5).abs() < 1e-6);
            let v_snap = snap_straight_line([0.5, 0.2], [0.505, 0.8], r);
            assert!((v_snap[0] - 0.5).abs() < 1e-6);
            let diag = snap_straight_line([0.2, 0.2], [0.6, 0.6], r);
            assert_eq!(diag, [0.6, 0.6]);
        }

        // Rehearsal per-slide timings track cumulative time and slowest_slides ranks them.
        {
            let p = h.state_mut().as_mut().unwrap();
            p.slide_timings.insert(1, 14.0);
            p.slide_timings.insert(3, 42.0);
            let top = p.slowest_slides(2);
            assert_eq!(top[0].0, 3);
            assert_eq!(top[1].0, 1);
            p.reset_timer();
            assert!(p.slide_timings.is_empty());
        }

        // Remove whiteboard cleanly restores deck size and clamps page.
        h.get_by_label("Remove whiteboard slide").click();
        h.run_steps(2);
        {
            let p = h.state().as_ref().unwrap();
            assert_eq!(p.total(), 3);
            assert_eq!(p.page, 3);
            assert!(p.current_whiteboard_id().is_none());
        }
        crate::testing::assert_ui_geometry(&h, "presenter post-whiteboard removal");
    }

    #[test]
    fn laser_and_spotlight_only_light_while_pressed() {
        for tool in [StageTool::Laser, StageTool::Spotlight, StageTool::Zoom] {
            let mut h = crate::testing::cpu_harness_state(
                [1200.0, 740.0],
                crate::theme::DARK,
                move |ui, state: &mut (Option<Presentation>, bool)| {
                    let p = state.0.get_or_insert_with(|| {
                        let mut pr = test_presentation(ui.ctx());
                        pr.tool = tool;
                        pr
                    });
                    stage::audience_ui(p, ui.ctx(), &crate::theme::DARK, &mut state.1);
                },
                (None, true),
            );
            h.run_steps(2);

            // Hovering the slide with Laser/Spotlight enabled must NOT light the pointer.
            let center = pos2(600.0, 370.0);
            h.input_mut().events.push(egui::Event::PointerMoved(center));
            h.run_steps(2);
            assert!(
                h.state().0.as_ref().unwrap().pointer_uv.is_none(),
                "{tool:?} should stay off on hover alone"
            );

            // Pressing & holding on the slide lights the pointer.
            h.input_mut().events.push(egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            });
            h.step();
            assert!(
                h.state().0.as_ref().unwrap().pointer_uv.is_some(),
                "{tool:?} should light while primary button is held"
            );
            crate::testing::assert_ui_geometry(&h, &format!("{tool:?} active"));

            // Releasing immediately turns the pointer back off.
            h.input_mut().events.push(egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(2);
            assert!(
                h.state().0.as_ref().unwrap().pointer_uv.is_none(),
                "{tool:?} should turn off on release"
            );
        }
    }
}
