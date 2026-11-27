//! Pure data models, timers, geometry helpers, `.tex` speaker-note extraction, and persistence
//! for Lemmata's presentation & presenter view.

use eframe::egui::{Color32, Pos2, Rect, pos2};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

/// Relative path inside a project where presenter notes, annotations, and layout are saved.
/// Never touches the user's `.tex` files.
pub const PRESENTER_FILE: &str = ".lemmata/presenter.json";

/// Which part of a page to show: Beamer's "show notes on second screen=right" puts the slide on
/// the left half of each page and its notes on the right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Full,
    Left,
    Right,
}

/// Slides are 4:3 (1.33) or 16:9 (1.78); with notes beside them the page is about twice that.
pub fn has_notes_page(width: f32, height: f32) -> bool {
    height > 0.0 && width / height > 2.2
}

/// Where the slide and the notes are on a page (with optional swap for `second screen=left`).
pub fn regions_swapped(width: f32, height: f32, swap: bool) -> (Region, Option<Region>) {
    if has_notes_page(width, height) {
        if swap {
            (Region::Right, Some(Region::Left))
        } else {
            (Region::Left, Some(Region::Right))
        }
    } else {
        (Region::Full, None)
    }
}

/// Where the slide and the notes are on a page (Beamer default: notes on right).
#[cfg(test)]
pub fn regions(width: f32, height: f32) -> (Region, Option<Region>) {
    regions_swapped(width, height, false)
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
    if w <= 0.0 || h <= 0.0 || !w.is_finite() || !h.is_finite() {
        1.0
    } else if !box_w.is_finite() || !box_h.is_finite() {
        0.0
    } else {
        (box_w.max(0.0) / w).min(box_h.max(0.0) / h)
    }
}

/// 83 -> "1:23", 3725 -> "1:02:05".
pub fn format_elapsed(seconds: f64) -> String {
    let s = seconds.max(0.0).floor() as u64;
    let (h, m, ss) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{ss:02}")
    } else {
        format!("{m}:{ss:02}")
    }
}

/// Formats remaining time when a target talk duration is set:
/// positive -> "14:20", zero -> "0:00", overtime (`<= -1.0s`) -> "+0:01", "+1:05".
pub fn format_remaining(remaining_seconds: f64) -> String {
    if remaining_seconds <= -1.0 {
        format!("+{}", format_elapsed(-remaining_seconds))
    } else {
        format_elapsed(remaining_seconds.max(0.0))
    }
}

/// Keeps a slide number (1-based) inside the document.
pub fn clamp_page(page: f64, total: usize) -> usize {
    let p = if page.is_nan() { 1.0 } else { page.round() };
    (p.max(1.0) as usize).min(total.max(1))
}

/// Maps a screen point inside `slide_rect` into normalized slide coordinates `[0, 1] x [0, 1]`.
pub fn to_norm(pos: Pos2, slide_rect: Rect) -> Option<[f32; 2]> {
    if slide_rect.width() <= 0.0 || slide_rect.height() <= 0.0 || !slide_rect.contains(pos) {
        return None;
    }
    let u = ((pos.x - slide_rect.left()) / slide_rect.width()).clamp(0.0, 1.0);
    let v = ((pos.y - slide_rect.top()) / slide_rect.height()).clamp(0.0, 1.0);
    Some([u, v])
}

/// Maps normalized slide coordinates `[0, 1] x [0, 1]` back into a display rect.
pub fn from_norm(uv: [f32; 2], slide_rect: Rect) -> Pos2 {
    pos2(
        slide_rect.left() + uv[0].clamp(0.0, 1.0) * slide_rect.width(),
        slide_rect.top() + uv[1].clamp(0.0, 1.0) * slide_rect.height(),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaceState {
    Idle,
    OnPace,
    Warning,
    Overtime,
}

#[derive(Default)]
pub struct Timer {
    pub started: Option<Instant>,
    pub running: bool,
    carried: f64,
}

impl Timer {
    pub fn elapsed(&self) -> f64 {
        self.carried
            + if self.running {
                self.started.map_or(0.0, |s| s.elapsed().as_secs_f64())
            } else {
                0.0
            }
    }

    /// Start, pause or resume.
    pub fn toggle(&mut self) {
        if self.running {
            self.carried = self.elapsed();
            self.running = false;
        } else {
            self.started = Some(Instant::now());
            self.running = true;
        }
    }

    pub fn reset(&mut self) {
        self.carried = 0.0;
        // Paused: back to "Start", not "Resume" at 0:00.
        self.started = self.running.then(Instant::now);
    }

    /// Pacing status relative to an optional target duration in minutes.
    pub fn pace(&self, target_minutes: Option<u32>) -> PaceState {
        let Some(mins) = target_minutes.filter(|&m| m > 0) else {
            return if self.running {
                PaceState::OnPace
            } else {
                PaceState::Idle
            };
        };
        let target = mins as f64 * 60.0;
        let elapsed = self.elapsed();
        if !self.running && elapsed < 0.5 {
            return PaceState::Idle;
        }
        let rem = target - elapsed;
        if rem <= -1.0 {
            PaceState::Overtime
        } else if rem <= (target * 0.15).max(120.0).min(target * 0.5) {
            PaceState::Warning
        } else {
            PaceState::OnPace
        }
    }
}

/// Audience screen blanking / hold modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScreenMode {
    #[default]
    Normal,
    Blackout,
    Whiteout,
}

/// Interactive stage tool synced between presenter preview and audience screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StageTool {
    #[default]
    None,
    Laser,
    Spotlight,
    Zoom,
    Pen,
    Highlighter,
}

/// One entry in the presentation's logical slide sequence: either a compiled PDF page or an
/// inserted blank whiteboard slide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeckItem {
    /// 0-based page index into the compiled PDF.
    Pdf(usize),
    /// Unique 1-based identifier into `PresenterStore::whiteboards`.
    Whiteboard(usize),
}

/// High-contrast ink colors readable on both light and dark Beamer themes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PenColor {
    #[default]
    Crimson,
    Sky,
    Amber,
    Emerald,
}

impl PenColor {
    pub const ALL: [PenColor; 4] = [
        PenColor::Crimson,
        PenColor::Sky,
        PenColor::Amber,
        PenColor::Emerald,
    ];

    pub fn color32(self) -> Color32 {
        match self {
            PenColor::Crimson => Color32::from_rgb(0xe8, 0x3b, 0x3b),
            PenColor::Sky => Color32::from_rgb(0x1f, 0x8d, 0xd6),
            PenColor::Amber => Color32::from_rgb(0xd9, 0x77, 0x06),
            PenColor::Emerald => Color32::from_rgb(0x1f, 0x8a, 0x5b),
        }
    }

    pub fn rgb_f32(self) -> (f32, f32, f32) {
        let c = self.color32();
        (
            c.r() as f32 / 255.0,
            c.g() as f32 / 255.0,
            c.b() as f32 / 255.0,
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            PenColor::Crimson => "Red ink",
            PenColor::Sky => "Blue ink",
            PenColor::Amber => "Amber ink",
            PenColor::Emerald => "Green ink",
        }
    }
}

/// One freehand or straight-line annotation stroke in slide-normalized `[0, 1] x [0, 1]` coords.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrokePath {
    pub color: PenColor,
    /// Relative stroke width as a fraction of slide height (e.g. `0.0055` pen, `0.022` highlighter).
    pub width: f32,
    /// When true, renders as a translucent highlighter that keeps text beneath legible.
    #[serde(default)]
    pub highlighter: bool,
    pub points: Vec<[f32; 2]>,
}

/// An inserted blank drawable whiteboard slide anchored after a 1-based PDF page (`0` = before 1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WhiteboardSlide {
    pub id: usize,
    pub after_pdf_page: usize,
    #[serde(default)]
    pub grid: bool,
    #[serde(default)]
    pub strokes: Vec<StrokePath>,
}

fn default_true() -> bool {
    true
}

fn default_notes_width() -> f32 {
    0.24
}

fn default_next_width() -> f32 {
    0.20
}

/// Persistent presenter state stored in `<project>/.lemmata/presenter.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresenterStore {
    pub target_minutes: Option<u32>,
    pub countdown: bool,
    pub notes_font_size: f32,
    pub split_x: f32,
    pub split_y: f32,
    /// `None` = automatic (open when the deck has speaker notes or Beamer notes, hidden otherwise).
    pub show_notes: Option<bool>,
    #[serde(default = "default_true")]
    pub show_next: bool,
    #[serde(default = "default_notes_width")]
    pub notes_width: f32,
    #[serde(default = "default_next_width")]
    pub next_width: f32,
    /// When true, puts Speaker notes on the right and Next slide on the left.
    pub notes_on_right: bool,
    /// When true, renders the presenter console in dark mode even while Lemmata is in light mode.
    pub force_dark: bool,
    pub filmstrip: bool,
    pub swap_notes_half: bool,
    /// 1-based PDF slide number -> speaker notes text.
    pub notes: BTreeMap<usize, String>,
    /// 1-based PDF slide number -> drawn ink/highlighter strokes.
    pub annotations: BTreeMap<usize, Vec<StrokePath>>,
    /// Inserted blank whiteboard slides (ordered by insertion, anchored after a PDF page).
    #[serde(default)]
    pub whiteboards: Vec<WhiteboardSlide>,
    /// True if an existing `.lemmata/presenter.json` failed to parse; prevents silent overwrite.
    #[serde(skip)]
    pub load_failed: bool,
}

impl Default for PresenterStore {
    fn default() -> Self {
        Self {
            target_minutes: None,
            countdown: false,
            notes_font_size: 16.0,
            split_x: 0.62,
            split_y: 0.42,
            show_notes: None,
            show_next: true,
            notes_width: default_notes_width(),
            next_width: default_next_width(),
            notes_on_right: false,
            force_dark: false,
            filmstrip: false,
            swap_notes_half: false,
            notes: BTreeMap::new(),
            annotations: BTreeMap::new(),
            whiteboards: Vec::new(),
            load_failed: false,
        }
    }
}

impl PresenterStore {
    pub fn load(project_root: &Path) -> Self {
        let path = project_root.join(PRESENTER_FILE);
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let mut store: Self = match serde_json::from_str(&raw) {
            Ok(s) => s,
            Err(_) => {
                return Self {
                    load_failed: !raw.trim().is_empty(),
                    ..Self::default()
                };
            }
        };
        store.notes_font_size = store.notes_font_size.clamp(13.0, 28.0);
        store.split_x = store.split_x.clamp(0.38, 0.78);
        store.split_y = store.split_y.clamp(0.22, 0.72);
        store.notes_width = store.notes_width.clamp(0.16, 0.40);
        store.next_width = store.next_width.clamp(0.14, 0.36);
        store
    }

    pub fn save(&self, project_root: &Path) -> Result<(), String> {
        if self.load_failed {
            return Err(
                "Skipped overwriting malformed .lemmata/presenter.json to preserve existing data."
                    .into(),
            );
        }
        let path = project_root.join(PRESENTER_FILE);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let body = serde_json::to_string_pretty(self).map_err(|e| e.to_string())? + "\n";
        std::fs::write(path, body).map_err(|e| e.to_string())
    }

    pub fn annotated_slide_count(&self) -> usize {
        self.annotations.values().filter(|v| !v.is_empty()).count()
            + self
                .whiteboards
                .iter()
                .filter(|w| !w.strokes.is_empty())
                .count()
    }

    pub fn has_any_custom_notes(&self) -> bool {
        self.notes.values().any(|s| !s.trim().is_empty())
    }

    /// Builds the ordered logical deck of PDF pages interleaved with inserted whiteboard slides.
    pub fn build_deck(&self, pdf_pages: usize) -> Vec<DeckItem> {
        let mut out = Vec::with_capacity(pdf_pages + self.whiteboards.len());
        for wb in &self.whiteboards {
            if wb.after_pdf_page == 0 {
                out.push(DeckItem::Whiteboard(wb.id));
            }
        }
        for page_idx in 0..pdf_pages {
            out.push(DeckItem::Pdf(page_idx));
            let pdf_num = page_idx + 1;
            for wb in &self.whiteboards {
                if wb.after_pdf_page == pdf_num {
                    out.push(DeckItem::Whiteboard(wb.id));
                }
            }
        }
        // Any whiteboard anchored past the current PDF page count (e.g. after deck shortened)
        // stays reachable at the end rather than disappearing.
        if pdf_pages > 0 {
            for wb in &self.whiteboards {
                if wb.after_pdf_page > pdf_pages {
                    out.push(DeckItem::Whiteboard(wb.id));
                }
            }
        }
        out
    }

    pub fn whiteboard(&self, id: usize) -> Option<&WhiteboardSlide> {
        self.whiteboards.iter().find(|w| w.id == id)
    }

    pub fn whiteboard_mut(&mut self, id: usize) -> Option<&mut WhiteboardSlide> {
        self.whiteboards.iter_mut().find(|w| w.id == id)
    }

    pub fn next_whiteboard_id(&self) -> usize {
        self.whiteboards.iter().map(|w| w.id).max().unwrap_or(0) + 1
    }
}

/// Lightweight parser that extracts Beamer `\note{...}` commands and `% NOTE:` comments
/// grouped by `\begin{frame}...\end{frame}` (1-based frame index). Used to pre-seed
/// speaker notes when a deck does not yet have saved presenter notes.
pub fn extract_tex_notes(source: &str) -> BTreeMap<usize, String> {
    let mut out = BTreeMap::new();
    let mut frame_idx = 0usize;
    let mut in_frame = false;
    let mut current: Vec<String> = Vec::new();
    let bytes = source.as_bytes();
    let mut pos = 0usize;

    let flush = |idx: usize, buf: &mut Vec<String>, map: &mut BTreeMap<usize, String>| {
        if idx > 0 && !buf.is_empty() {
            let text = buf.join("\n").trim().to_owned();
            if !text.is_empty() {
                map.entry(idx)
                    .and_modify(|e| {
                        e.push('\n');
                        e.push_str(&text);
                    })
                    .or_insert(text);
            }
        }
        buf.clear();
    };

    while pos < bytes.len() {
        let rest = &source[pos..];
        if rest.starts_with("\\begin{frame}") {
            flush(frame_idx, &mut current, &mut out);
            frame_idx += 1;
            in_frame = true;
            pos += "\\begin{frame}".len();
            continue;
        }
        if rest.starts_with("\\end{frame}") {
            flush(frame_idx, &mut current, &mut out);
            in_frame = false;
            pos += "\\end{frame}".len();
            continue;
        }
        if rest.starts_with("\\note")
            && !bytes
                .get(pos + "\\note".len())
                .is_some_and(u8::is_ascii_alphabetic)
        {
            let after_cmd = pos + "\\note".len();
            let mut cursor = after_cmd;
            // Skip optional overlay spec `<...>` and/or `[...]` in any order.
            loop {
                while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
                if cursor < bytes.len()
                    && bytes[cursor] == b'<'
                    && let Some(end) = source[cursor..].find('>')
                {
                    cursor += end + 1;
                    continue;
                }
                if cursor < bytes.len()
                    && bytes[cursor] == b'['
                    && let Some(end) = source[cursor..].find(']')
                {
                    cursor += end + 1;
                    continue;
                }
                break;
            }
            if cursor < bytes.len() && bytes[cursor] == b'{' {
                let start = cursor + 1;
                let mut depth = 1i32;
                let mut i = start;
                while i < bytes.len() && depth > 0 {
                    match bytes[i] {
                        b'\\' => {
                            i += 1;
                            if i < bytes.len() {
                                i += source[i..].chars().next().map_or(1, char::len_utf8);
                            }
                        }
                        b'{' => {
                            depth += 1;
                            i += 1;
                        }
                        b'}' => {
                            depth -= 1;
                            i += 1;
                        }
                        _ => i += source[i..].chars().next().map_or(1, char::len_utf8),
                    }
                }
                if depth == 0 {
                    let note = source[start..i - 1].trim();
                    if !note.is_empty() {
                        let target = if frame_idx == 0 { 1 } else { frame_idx };
                        if in_frame {
                            current.push(note.to_owned());
                        } else {
                            // \note{...} placed immediately after \end{frame} belongs to that frame.
                            out.entry(target)
                                .and_modify(|e| {
                                    e.push('\n');
                                    e.push_str(note);
                                })
                                .or_insert_with(|| note.to_owned());
                        }
                    }
                    pos = i;
                    continue;
                }
            }
        }
        // Skip escaped characters (such as `\%`, `\\`) so `\%` is not mistaken for a comment.
        if bytes[pos] == b'\\' {
            pos += 1;
            if pos < bytes.len() {
                pos += source[pos..].chars().next().map_or(1, char::len_utf8);
            }
            continue;
        }
        if bytes[pos] == b'%' {
            let line_end = source[pos..].find('\n').map_or(bytes.len(), |n| pos + n);
            let comment = source[pos + 1..line_end].trim();
            if let Some(note) = comment
                .strip_prefix("NOTE:")
                .or_else(|| comment.strip_prefix("note:"))
            {
                let trimmed = note.trim();
                if !trimmed.is_empty() && frame_idx > 0 {
                    if in_frame {
                        current.push(trimmed.to_owned());
                    } else {
                        out.entry(frame_idx)
                            .and_modify(|e| {
                                e.push('\n');
                                e.push_str(trimmed);
                            })
                            .or_insert_with(|| trimmed.to_owned());
                    }
                }
            }
            pos = line_end;
            continue;
        }
        pos += source[pos..].chars().next().map_or(1, char::len_utf8);
    }
    flush(frame_idx, &mut current, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_pages_are_told_apart() {
        assert!(!has_notes_page(455.0, 256.0));
        assert!(!has_notes_page(362.0, 272.0));
        assert!(has_notes_page(910.0, 256.0));
        assert!(has_notes_page(724.0, 272.0));
        assert!(!has_notes_page(595.0, 842.0));
        assert!(!has_notes_page(100.0, 0.0));
        assert_eq!(regions(910.0, 256.0), (Region::Left, Some(Region::Right)));
        assert_eq!(
            regions_swapped(910.0, 256.0, true),
            (Region::Right, Some(Region::Left))
        );
        assert_eq!(regions(455.0, 256.0), (Region::Full, None));
        assert_eq!(region_size(910.0, 256.0, Region::Left), (455.0, 256.0, 0.0));
        assert_eq!(
            region_size(910.0, 256.0, Region::Right),
            (455.0, 256.0, 455.0)
        );
        assert_eq!(region_size(455.0, 256.0, Region::Full), (455.0, 256.0, 0.0));
    }

    #[test]
    fn a_reset_timer_offers_start_again() {
        let mut t = Timer::default();
        t.toggle();
        t.toggle();
        assert!(t.started.is_some());
        t.reset();
        assert!(t.started.is_none() && t.elapsed() == 0.0);
    }

    #[test]
    fn helpers_and_normalized_coords() {
        assert_eq!(fit_scale(400.0, 200.0, 800.0, 800.0), 2.0);
        assert_eq!(fit_scale(400.0, 200.0, 800.0, 200.0), 1.0);
        assert_eq!(fit_scale(0.0, 0.0, 800.0, 600.0), 1.0);
        assert_eq!(format_elapsed(0.0), "0:00");
        assert_eq!(format_elapsed(83.0), "1:23");
        assert_eq!(format_elapsed(3725.0), "1:02:05");
        assert_eq!(format_elapsed(-4.0), "0:00");
        assert_eq!(format_remaining(90.0), "1:30");
        assert_eq!(format_remaining(-65.0), "+1:05");
        assert_eq!(clamp_page(0.0, 10), 1);
        assert_eq!(clamp_page(11.0, 10), 10);
        assert_eq!(clamp_page(4.6, 10), 5);
        assert_eq!(clamp_page(f64::NAN, 10), 1);
        assert_eq!(clamp_page(3.0, 0), 1);

        let r = Rect::from_min_max(pos2(100.0, 50.0), pos2(300.0, 250.0));
        let uv = to_norm(pos2(200.0, 100.0), r).unwrap();
        assert!((uv[0] - 0.5).abs() < 1e-5);
        assert!((uv[1] - 0.25).abs() < 1e-5);
        assert_eq!(from_norm(uv, r), pos2(200.0, 100.0));
        assert!(to_norm(pos2(50.0, 100.0), r).is_none());
    }

    #[test]
    fn extracts_beamer_notes_and_comments() {
        let tex = r#"
\begin{frame}{Intro}
  Hello world.
  \note{Pause for questions here.}
\end{frame}
\begin{frame}{Method}
  % NOTE: Emphasize the convergence rate.
  Equation goes here.
\end{frame}
\note[item]{Mention appendix proof.}
"#;
        let notes = extract_tex_notes(tex);
        assert_eq!(
            notes.get(&1).map(String::as_str),
            Some("Pause for questions here.")
        );
        assert_eq!(
            notes.get(&2).map(String::as_str),
            Some("Emphasize the convergence rate.\nMention appendix proof.")
        );
    }

    #[test]
    fn presenter_store_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = PresenterStore {
            target_minutes: Some(20),
            countdown: true,
            notes_font_size: 20.0,
            ..PresenterStore::default()
        };
        s.notes.insert(1, "Opening remark".into());
        s.annotations.insert(
            1,
            vec![StrokePath {
                color: PenColor::Crimson,
                width: 0.005,
                highlighter: false,
                points: vec![[0.1, 0.2], [0.4, 0.5]],
            }],
        );
        s.save(tmp.path()).unwrap();
        let loaded = PresenterStore::load(tmp.path());
        assert_eq!(loaded, s);
        assert_eq!(loaded.annotated_slide_count(), 1);
    }

    #[test]
    fn corrupt_presenter_json_is_not_silently_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".lemmata");
        std::fs::create_dir_all(&dir).unwrap();
        let bad = "{ broken json";
        std::fs::write(dir.join("presenter.json"), bad).unwrap();
        let loaded = PresenterStore::load(tmp.path());
        assert!(loaded.load_failed);
        assert!(loaded.save(tmp.path()).is_err());
        assert_eq!(
            std::fs::read_to_string(dir.join("presenter.json")).unwrap(),
            bad
        );
    }

    #[test]
    fn extract_tex_notes_handles_escapes_utf8_and_overlay_orders() {
        let tex = r"\begin{frame}
100\% complete! \note[item]<1-2>{Ümlaut \ö and \{braces\}}
\end{frame}";
        let notes = extract_tex_notes(tex);
        assert_eq!(
            notes.get(&1).map(String::as_str),
            Some(r"Ümlaut \ö and \{braces\}")
        );
    }

    #[test]
    fn remaining_time_never_flashes_plus_zero_and_fit_scale_clamps_negative_boxes() {
        for step in -200..=200 {
            let rem = step as f64 * 0.05;
            let text = format_remaining(rem);
            assert_ne!(text, "+0:00", "rem={rem} produced +0:00");
        }
        assert_eq!(fit_scale(400.0, 200.0, -10.0, 200.0), 0.0);
        assert_eq!(fit_scale(400.0, 200.0, f32::NAN, 200.0), 0.0);
    }
}
