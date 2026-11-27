//! PdfPane.svelte: the right pane, after Overleaf's. A bar (logs, pages, zoom, search, outline,
//! thumbnails, export), the PDF itself, or the build's logs in its place.

use super::engine::OutlineNode;
use super::problems::Problems;
use super::view::{Fit, Layout, PdfView};
use crate::icons::{self, Icon};
use crate::shortcuts::{display, keys_of};
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind, MenuItem, MenuResult, Size};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use lemmata_log::{Diagnostic, Severity};
use lemmata_tex::Status;
use std::collections::HashSet;

#[cfg(test)]
static PDF_THUMB_WIDGETS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// What the pane needs to know about the build.
pub struct Build<'a> {
    pub diagnostics: &'a [Diagnostic],
    pub building: bool,
    /// A plain-language note on what a long build is doing.
    pub note: Option<&'a str>,
    /// The last build ended without writing a PDF.
    pub failed: bool,
    pub status: Option<Status>,
    /// Changes after every build, so the raw log reloads.
    pub id: u64,
    /// True when the Problems panel is currently open in the left sidebar.
    pub problems_open: bool,
    /// True when the Raw TeX log is replacing the PDF canvas.
    pub raw_log_open: bool,
    /// Lines `from..=to` (1-based) of a project file, for the excerpts under errors; fewer
    /// at its end.
    pub source: &'a dyn Fn(&str, usize, usize) -> Vec<String>,
}

/// What the pane asks of the workspace.
pub enum Action {
    /// Toggle the Problems sidebar panel (PDF bar issues chip).
    Problems,
    /// Unconditionally open the Problems sidebar panel (error banner / empty state).
    OpenProblems,
    /// Open or close the raw TeX log view in the PDF card.
    RawLog(bool),
    SourceClick(usize, f32, f32),
    OpenUrl(String),
    Goto(String, usize),
    AddPackage(String),
    Export,
    OpenExternally,
    Present,
    Layout(&'static str),
    /// Opens the log through the backend; answered with `set_log`.
    ReadLog,
    OpenTerminal(String),
    /// Runs the install command a problem shows, in a terminal.
    OpenCommand(String),
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Outline,
    Thumbs,
}

#[derive(Clone, PartialEq)]
enum Menu {
    Zoom(Rect),
    More(Rect),
}

pub struct Pane {
    pub view: PdfView,
    pub problems: Problems,
    page_input: String,
    search_open: bool,
    focus_search: bool,
    search_focused: bool,
    pages_in_bar: bool,
    zoom_btns_in_bar: bool,
    side_btns_in_bar: bool,
    present_in_bar: bool,
    query: String,
    /// When the query last changed; it is searched 180 ms later.
    query_at: Option<f64>,
    side: Option<Side>,
    outline_closed: HashSet<Vec<usize>>,
    menu: Option<Menu>,
    /// The page whose thumbnail was last scrolled into view.
    thumb_page: usize,
    /// Set by `Workspace` to `false` when a modal, sheet, or menu is open above the pane.
    pub allow_shortcuts: bool,
}

impl Pane {
    pub fn new(ctx: &egui::Context) -> Self {
        Self {
            view: PdfView::new(ctx, egui::Id::unique("pdf-view")),
            problems: Problems::default(),
            page_input: String::new(),
            search_open: false,
            focus_search: false,
            search_focused: false,
            pages_in_bar: true,
            zoom_btns_in_bar: true,
            side_btns_in_bar: true,
            present_in_bar: true,
            query: String::new(),
            query_at: None,
            side: None,
            outline_closed: HashSet::new(),
            menu: None,
            thumb_page: 0,
            allow_shortcuts: true,
        }
    }

    pub fn has_pdf(&self) -> bool {
        self.view.page_count() > 0
    }

    pub fn open_search(&mut self) {
        self.search_open = true;
        self.focus_search = true;
    }

    fn close_search(&mut self) {
        self.search_open = false;
        self.query.clear();
        self.query_at = None;
        self.view.clear_search();
    }

    pub fn show(&mut self, ui: &mut Ui, p: &Palette, b: &Build) -> Vec<Action> {
        let mut out = Vec::new();
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, p.pdf_ground);
        let ctx = ui.ctx().clone();
        let has_pdf = self.has_pdf();

        // Ctrl+F while the PDF (or its search bar) has focus searches the PDF, not the editor.
        if self.allow_shortcuts
            && (self.view_focused(&ctx) || self.search_focused)
            && crate::shortcuts::consume(&ctx, "pdf-search")
        {
            out.push(Action::RawLog(false));
            self.open_search();
        }
        if self.allow_shortcuts
            && self.search_open
            && !b.raw_log_open
            && (self.view_focused(&ctx) || self.search_focused)
        {
            let f3 = ctx.input_mut(|i| {
                if i.consume_key(egui::Modifiers::SHIFT, egui::Key::F3) {
                    Some(-1)
                } else if i.consume_key(egui::Modifiers::NONE, egui::Key::F3) {
                    Some(1)
                } else {
                    None
                }
            });
            if let Some(dir) = f3 {
                if self.query_at.take().is_some() {
                    let q = self.query.clone();
                    self.view.search(&q, true);
                } else {
                    self.view.search_step(dir);
                }
            }
        }

        // .bar
        let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 40.0));
        ui.painter().rect_filled(bar, 0, p.surface);
        ui.painter().hline(
            bar.x_range(),
            bar.bottom() - 0.5,
            Stroke::new(1.0, p.border),
        );
        self.bar(
            &mut ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink2(vec2(8.0, 0.0)))),
            p,
            b,
            &mut out,
        );

        // .progress: a third of the width sliding across while building.
        let progress =
            Rect::from_min_size(pos2(rect.left(), bar.bottom()), vec2(rect.width(), 2.0));
        if b.building {
            let t = (ui.input(|i| i.time) / 1.1).fract() as f32;
            let w = rect.width() * 0.35;
            let x = progress.left() - w + t * 4.0 * w;
            let r = Rect::from_min_size(pos2(x, progress.top()), vec2(w, 2.0)).intersect(progress);
            ui.painter().rect_filled(r, 0, p.accent);
            ctx.request_repaint_after(crate::theme::ANIMATION_FRAME);
        }
        let mut y = progress.bottom();

        if self.search_open && !b.raw_log_open {
            let r = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), 35.0));
            ui.painter().rect_filled(r, 0, p.surface);
            ui.painter()
                .hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.border));
            self.search_row(
                &mut ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(8.0, 4.0)))),
                p,
            );
            y = r.bottom();
        } else {
            self.search_focused = false;
        }
        if let Some(at) = self.query_at
            && ui.input(|i| i.time) - at >= 0.18
        {
            self.query_at = None;
            let q = self.query.clone();
            self.view.search(&q, true);
        } else if self.query_at.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }

        let mut body = Rect::from_min_max(pos2(rect.left(), y), rect.max);
        if let Some(side) = self.side.filter(|_| !b.raw_log_open) {
            let side_w = (body.width() * 0.30).clamp(120.0, 200.0);
            let r = Rect::from_min_size(body.min, vec2(side_w, body.height()));
            ui.painter().rect_filled(r, 0, p.surface);
            ui.painter()
                .vline(r.right() - 0.5, r.y_range(), Stroke::new(1.0, p.border));
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(6.0, 8.0))));
            match side {
                Side::Thumbs => self.thumbnails(&mut c, p),
                Side::Outline => {
                    if let Some(action) = self.outline(&mut c, p) {
                        out.push(action);
                    }
                }
            }
            body.min.x = r.right();
        }
        if b.building
            && let Some(note) = b.note
        {
            let h = 30.0;
            let r = Rect::from_min_size(body.min, vec2(body.width(), h));
            ui.painter().rect_filled(r, 0, p.accent_soft);
            ui.painter()
                .hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.border));
            ui.painter().text(
                pos2(r.left() + 10.0, r.center().y),
                egui::Align2::LEFT_CENTER,
                note,
                FontId::proportional(12.5),
                p.ink,
            );
            body.min.y = r.bottom();
        }
        if b.failed && has_pdf && !b.raw_log_open && !b.building {
            let errors = b
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .count();
            let r = Rect::from_min_size(body.min, vec2(body.width(), 38.0));
            ui.painter().rect_filled(r, 0, p.error_soft);
            ui.painter()
                .hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.border));
            let mut c = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(r.shrink2(vec2(10.0, 0.0)))
                    .layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            if Button::new("View problems")
                .size(Size::Sm)
                .show(&mut c, p)
                .clicked()
            {
                out.push(Action::OpenProblems);
            }
            let with = if errors > 0 {
                format!(" with {errors} error{}", if errors == 1 { "" } else { "s" })
            } else {
                String::new()
            };
            c.with_layout(egui::Layout::left_to_right(egui::Align::Center), |c| {
                c.add(
                    egui::Label::new(
                        egui::RichText::new(format!(
                            "The latest build failed{with}. This is the last PDF that built."
                        ))
                        .size(12.5)
                        .color(p.ink),
                    )
                    .truncate(),
                );
            });
            body.min.y = r.bottom();
        }

        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(body));
        if b.raw_log_open {
            // Not drawn, so it can't tell that it lost focus: Ctrl+F is the raw log's own meanwhile.
            self.view.focused = false;
            out.extend(
                self.problems
                    .show_raw_log(&mut c, p, b, self.allow_shortcuts),
            );
        } else if has_pdf {
            match self.view.show(&mut c, p) {
                Some(super::view::Event::SourceClick(n, x, y)) => {
                    out.push(Action::SourceClick(n, x, y));
                }
                Some(super::view::Event::OpenUrl(url)) => out.push(Action::OpenUrl(url)),
                None => {}
            }
        } else {
            self.view.show(&mut c, p);
            if self.view.error.is_none() && empty_state(&mut c, p, b) {
                out.push(Action::OpenProblems);
            }
        }
        self.menus(&ctx, p, &mut out);
        out
    }

    fn view_focused(&self, _ctx: &egui::Context) -> bool {
        self.view.focused
    }

    fn bar(&mut self, ui: &mut Ui, p: &Palette, b: &Build, out: &mut Vec<Action>) {
        let width = ui.max_rect().width() + 16.0;
        // Progressive collapse based on actual control widths (instead of hiding everything at 660 px):
        // - > 580 px: full bar (-/+ zoom, Outline, Thumbnails, Present, External, Export, Search)
        // - 521..=580 px: drop -/+ zoom buttons (zoom dropdown stays); keep all 6 right-hand icons
        // - 451..=520 px: move Outline, Thumbnails, External into `More`, keep Present on the bar
        // - 406..=450 px: move Present into `More` as well
        // - <= 405 px: also move page arrows into `More`
        self.zoom_btns_in_bar = width > 580.0;
        self.side_btns_in_bar = width > 520.0;
        self.present_in_bar = width > 450.0;
        self.pages_in_bar = width > 405.0;
        let show_more = !self.side_btns_in_bar || !self.present_in_bar || !self.pages_in_bar;
        ui.spacing_mut().item_spacing.x = if width <= 620.0 { 6.0 } else { 10.0 };
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let errors = b
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .count();
            let warnings = b
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Warning)
                .count();
            if b.raw_log_open {
                if Button::new("Back to PDF")
                    .with_icon(icons::CHEVRON_LEFT)
                    .kind(Kind::Ghost)
                    .size(Size::Sm)
                    .icon_size(14.0)
                    .show(ui, p)
                    .clicked()
                {
                    out.push(Action::RawLog(false));
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Raw TeX log")
                        .font(FontId::new(13.0, theme::font::sans_600()))
                        .color(p.ink),
                );
                return;
            }
            let total_issues = errors + warnings;
            let (chip_icon, chip_text, chip_tone) = if errors > 0 {
                (
                    Some(icons::CIRCLE_X),
                    format!(
                        "{total_issues} issue{}",
                        if total_issues == 1 { "" } else { "s" }
                    ),
                    widgets::ChipTone::Error,
                )
            } else if warnings > 0 {
                (
                    Some(icons::TRIANGLE_ALERT),
                    format!(
                        "{total_issues} issue{}",
                        if total_issues == 1 { "" } else { "s" }
                    ),
                    widgets::ChipTone::Warn,
                )
            } else {
                (
                    Some(icons::CHECK),
                    "No issues".to_owned(),
                    widgets::ChipTone::Ok,
                )
            };
            let chip =
                widgets::chip_button(ui, p, chip_icon, &chip_text, "Build issues", chip_tone)
                    .on_hover_text("Open Problems panel");
            if b.problems_open {
                ui.painter().rect_stroke(
                    chip.rect,
                    CornerRadius::same(12),
                    Stroke::new(1.5, p.accent),
                    StrokeKind::Inside,
                );
            }
            if chip.clicked() {
                out.push(Action::Problems);
            }
            if !self.has_pdf() {
                return;
            }
            // Thin vertical divider between issues chip and pager.
            let (vr, _) = ui.allocate_exact_size(vec2(1.0, 18.0), Sense::hover());
            ui.painter().rect_filled(vr, 0, p.border);
            let n = self.view.page_count();
            let cur = self.view.current_page;
            if self.pages_in_bar {
                let box_w = if width <= 480.0 { 30.0 } else { 38.0 };
                group(ui, |ui| {
                    if ghost(ui, p, icons::CHEVRON_LEFT, "Previous page", 16.0)
                        .enabled(cur > 1)
                        .show_icon(ui, p)
                        .clicked()
                    {
                        self.view.goto_page(cur - 1);
                    }
                    self.page_box(ui, p, cur, box_w);
                    ui.label(
                        egui::RichText::new(format!("/ {n}"))
                            .size(12.0)
                            .color(p.muted),
                    );
                    if ghost(ui, p, icons::CHEVRON_RIGHT, "Next page", 16.0)
                        .enabled(cur < n)
                        .show_icon(ui, p)
                        .clicked()
                    {
                        self.view.goto_page(cur + 1);
                    }
                });
            }
            group(ui, |ui| {
                if self.zoom_btns_in_bar
                    && ghost(ui, p, icons::ZOOM_OUT, "Zoom out", 16.0)
                        .show_icon(ui, p)
                        .clicked()
                {
                    self.view.zoom_by(1.0 / 1.15);
                }
                let text = match self.view.fit {
                    Fit::Width => "Fit width".to_owned(),
                    Fit::Page => "Fit page".to_owned(),
                    Fit::Zoom(_) => format!("{}%", self.view.percent),
                };
                let z = Button::new(&text)
                    .kind(Kind::Ghost)
                    .size(Size::Sm)
                    .trailing_icon(icons::CHEVRON_DOWN)
                    .icon_size(13.0)
                    .min_width(64.0)
                    .show(ui, p);
                ui.ctx()
                    .accesskit_node_builder(z.id, |n| n.set_label("Zoom level"));
                if z.clicked() {
                    self.menu = toggle(&self.menu, Menu::Zoom(z.rect));
                }
                if self.zoom_btns_in_bar
                    && ghost(ui, p, icons::ZOOM_IN, "Zoom in", 16.0)
                        .show_icon(ui, p)
                        .clicked()
                {
                    self.view.zoom_by(1.15);
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if show_more {
                    let m = ghost(ui, p, icons::ELLIPSIS, "More", 15.0)
                        .show_icon(ui, p)
                        .on_hover_text("More");
                    if m.clicked() {
                        self.menu = toggle(&self.menu, Menu::More(m.rect));
                    }
                }
                if ghost(ui, p, icons::DOWNLOAD, "Export PDF", 15.0)
                    .show_icon(ui, p)
                    .on_hover_text("Export PDF")
                    .clicked()
                {
                    out.push(Action::Export);
                }
                if self.side_btns_in_bar
                    && ghost(ui, p, icons::EXTERNAL_LINK, "Open in external viewer", 15.0)
                        .show_icon(ui, p)
                        .on_hover_text("Open in the system's PDF viewer")
                        .clicked()
                {
                    out.push(Action::OpenExternally);
                }
                if self.present_in_bar
                    && ghost(ui, p, icons::PRESENTATION, "Present full screen", 15.0)
                        .show_icon(ui, p)
                        .on_hover_text("Present full screen")
                        .clicked()
                {
                    out.push(Action::Present);
                }
                if self.side_btns_in_bar {
                    let thumbs = self.side == Some(Side::Thumbs);
                    if ghost(ui, p, icons::PANEL_LEFT, "Page thumbnails", 15.0)
                        .on(thumbs)
                        .show_icon(ui, p)
                        .on_hover_text("Page thumbnails")
                        .clicked()
                    {
                        self.toggle_side(Side::Thumbs);
                    }
                    let outline = self.side == Some(Side::Outline);
                    if ghost(ui, p, icons::LIST_TREE, "Document outline", 15.0)
                        .on(outline)
                        .show_icon(ui, p)
                        .on_hover_text("Document outline")
                        .clicked()
                    {
                        self.toggle_side(Side::Outline);
                    }
                }
                let tip = format!("Search in the PDF ({})", display(keys_of("pdf-search")));
                if ghost(ui, p, icons::SEARCH, "Search in the PDF", 15.0)
                    .on(self.search_open)
                    .show_icon(ui, p)
                    .on_hover_text(tip)
                    .clicked()
                {
                    if self.search_open {
                        self.close_search();
                    } else {
                        self.open_search();
                    }
                }
            });
        });
    }

    fn toggle_side(&mut self, s: Side) {
        self.side = if self.side == Some(s) { None } else { Some(s) };
    }

    /// `.page-input`: 38×24 (30×24 on narrow panes), the current page as its placeholder; Enter goes there.
    fn page_box(&mut self, ui: &mut Ui, p: &Palette, cur: usize, box_w: f32) {
        let fill = ui.painter().add(egui::Shape::Noop);
        let hint = ui.painter().add(egui::Shape::Noop);
        let r = egui::TextEdit::singleline(&mut self.page_input)
            .font(FontId::proportional(12.0))
            .text_color(p.ink)
            .frame(egui::Frame::NONE)
            .align(egui::Align2::CENTER_CENTER)
            .min_size(vec2(box_w, 24.0))
            .desired_width(box_w)
            .show(ui)
            .response
            .response;
        ui.ctx()
            .accesskit_node_builder(r.id, |n| n.set_label("Page"));
        let focused = r.has_focus();
        // The placeholder is the current page in ink, faint while typing (egui greys hints).
        if self.page_input.is_empty() {
            let color = if focused { p.faint } else { p.ink };
            let g = ui
                .painter()
                .layout_no_wrap(cur.to_string(), FontId::proportional(12.0), color);
            ui.painter().set(
                hint,
                egui::Shape::galley(r.rect.center() - g.size() / 2.0, g, color),
            );
        }
        let border = if focused { p.accent } else { p.border_strong };
        ui.painter().set(
            fill,
            egui::Shape::rect_filled(r.rect, CornerRadius::same(theme::R_SM), p.surface),
        );
        ui.painter().rect_stroke(
            r.rect,
            CornerRadius::same(theme::R_SM),
            Stroke::new(1.0, border),
            StrokeKind::Inside,
        );
        if r.lost_focus() {
            if ui.input(|i| i.key_pressed(egui::Key::Enter))
                && let Ok(n) = self.page_input.trim().parse::<usize>()
                && n > 0
            {
                self.view.goto_page(n);
            }
            // Clicking away drops what was typed: the box shows the page the viewer is on.
            self.page_input.clear();
        }
    }

    fn search_row(&mut self, ui: &mut Ui, p: &Palette) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let count = self.view.match_count();
            if ghost(ui, p, icons::X, "Close search", 15.0)
                .show_icon(ui, p)
                .clicked()
            {
                self.close_search();
                return;
            }
            if ghost(ui, p, icons::CHEVRON_DOWN, "Next match", 15.0)
                .enabled(count > 0)
                .show_icon(ui, p)
                .clicked()
            {
                self.view.search_step(1);
            }
            if ghost(ui, p, icons::CHEVRON_UP, "Previous match", 15.0)
                .enabled(count > 0)
                .show_icon(ui, p)
                .clicked()
            {
                self.view.search_step(-1);
            }
            let text = if self.query.trim().is_empty() {
                String::new()
            } else if count > 0 {
                format!("{} of {count}", self.view.match_index)
            } else {
                "No matches".into()
            };
            let (r, _) = ui.allocate_exact_size(vec2(64.0, 20.0), Sense::hover());
            ui.painter().text(
                r.right_center(),
                egui::Align2::RIGHT_CENTER,
                text,
                FontId::proportional(12.0),
                p.muted,
            );
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                icons::show(ui, icons::SEARCH, 14.0, p.muted);
                let fill = ui.painter().add(egui::Shape::Noop);
                let out = egui::TextEdit::singleline(&mut self.query)
                    .hint_text(egui::RichText::new("Search the PDF").color(p.faint))
                    .font(FontId::proportional(13.0))
                    .text_color(p.ink)
                    .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 0)))
                    .align(egui::Align2::LEFT_CENTER)
                    .min_size(vec2(0.0, 26.0))
                    .desired_width(ui.available_width())
                    .show(ui);
                let r = out.response.response;
                ui.ctx()
                    .accesskit_node_builder(r.id, |n| n.set_label("Search the PDF"));
                ui.painter().set(
                    fill,
                    egui::Shape::rect_filled(r.rect, CornerRadius::same(theme::R_SM), p.surface),
                );
                let border = if r.has_focus() {
                    p.accent
                } else {
                    p.border_strong
                };
                ui.painter().rect_stroke(
                    r.rect,
                    CornerRadius::same(theme::R_SM),
                    Stroke::new(1.0, border),
                    StrokeKind::Inside,
                );
                if std::mem::take(&mut self.focus_search) {
                    r.request_focus();
                }
                self.search_focused = r.has_focus();
                if r.changed() {
                    self.query_at = Some(ui.input(|i| i.time));
                }
                if r.has_focus() || r.lost_focus() {
                    let (esc, enter, shift) = ui.input_mut(|i| {
                        (
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                            i.key_pressed(egui::Key::Enter),
                            i.modifiers.shift,
                        )
                    });
                    if esc {
                        self.close_search();
                    } else if enter {
                        if self.query_at.take().is_some() {
                            let q = self.query.clone();
                            self.view.search(&q, true);
                        } else {
                            self.view.search_step(if shift { -1 } else { 1 });
                        }
                        r.request_focus();
                    }
                }
            });
        });
    }

    /// PdfOutline.svelte: the PDF's own bookmarks as a tree.
    fn outline(&mut self, ui: &mut Ui, p: &Palette) -> Option<Action> {
        if self.view.outline.is_empty() {
            ui.add_space(6.0);
            widgets::Rich::new()
                .add(
                    "This PDF has no bookmarks. Load ",
                    FontId::proportional(12.0),
                    p.muted,
                )
                .add("hyperref", FontId::monospace(11.5), p.ink_2)
                .add(
                    " to get them from your sections.",
                    FontId::proportional(12.0),
                    p.muted,
                )
                .line_height(18.0)
                .wrap(ui.available_width() - 12.0)
                .show(ui);
            return None;
        }
        let mut go = None;
        egui::ScrollArea::vertical()
            .id_salt("pdf-outline")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                outline_nodes(
                    ui,
                    p,
                    &self.view.outline,
                    &mut Vec::new(),
                    &mut self.outline_closed,
                    &mut go,
                );
            });
        if let Some(t) = go
            && let Some(url) = self.view.goto_target(&t)
            && super::view::is_link_to_open(&url)
        {
            return Some(Action::OpenUrl(url));
        }
        None
    }

    /// PdfThumbnails.svelte: a small render of every page, drawn as it scrolls into view.
    fn thumbnails(&mut self, ui: &mut Ui, p: &Palette) {
        let ctx = ui.ctx().clone();
        let current = self.view.current_page;
        let follow = std::mem::replace(&mut self.thumb_page, current) != current;
        egui::ScrollArea::vertical()
            .id_salt("pdf-thumbs")
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let w = ui.available_width().min(176.0);
                let x = ui.max_rect().left() + (ui.available_width() - w) / 2.0;
                let origin = ui.max_rect().min;
                let (rects, content_y) = self.view.thumbnail_geometry(w);
                let near = viewport.expand2(vec2(0.0, viewport.height() * 0.5));
                let first = rects.partition_point(|r| r.bottom() <= near.min.y);
                let last = rects.partition_point(|r| r.top() < near.max.y);
                if follow && (1..=rects.len()).contains(&current) {
                    let all = rects[current - 1].translate(vec2(x, origin.y));
                    ui.scroll_to_rect(all, None);
                }
                for i in first..last {
                    #[cfg(test)]
                    PDF_THUMB_WIDGETS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let all = rects[i].translate(vec2(x, origin.y));
                    let img = Rect::from_min_size(all.min, vec2(w, all.height() - 18.0));
                    let resp = ui.interact(all, ui.scope_id().with(("thumb", i)), Sense::click());
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            true,
                            format!("Page {}", i + 1),
                        )
                    });
                    let painter = ui.painter();
                    painter.add(
                        egui::Shadow {
                            offset: [0, 1],
                            blur: 3,
                            spread: 0,
                            color: Color32::from_black_alpha(51),
                        }
                        .as_shape(img, 2),
                    );
                    painter.rect_filled(img, 2, Color32::WHITE);
                    if let Some((tex, _)) = self.view.thumbnail(&ctx, i) {
                        painter.image(
                            tex,
                            img.shrink(2.0),
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    let is = i + 1 == current;
                    let border = if is {
                        p.accent
                    } else if resp.hovered() {
                        p.border_strong
                    } else {
                        Color32::TRANSPARENT
                    };
                    painter.rect_stroke(img, 2, Stroke::new(2.0, border), StrokeKind::Inside);
                    painter.text(
                        pos2(img.center().x, img.bottom() + 3.0),
                        egui::Align2::CENTER_TOP,
                        (i + 1).to_string(),
                        FontId::proportional(11.0),
                        if is { p.accent } else { p.muted },
                    );
                    if resp.clicked() {
                        self.view.goto_page(i + 1);
                    }
                }
                ui.allocate_rect(
                    Rect::from_min_max(origin, pos2(ui.max_rect().right(), origin.y + content_y)),
                    Sense::hover(),
                );
            });
    }

    fn menus(&mut self, ctx: &egui::Context, p: &Palette, out: &mut Vec<Action>) {
        let Some(menu) = self.menu.clone() else {
            return;
        };
        let id = egui::Id::unique("pdf-menu");
        match menu {
            Menu::Zoom(anchor) => {
                let l = self.view.layout;
                let fit = self.view.fit;
                let mut items = vec![
                    MenuItem::check("Fit width", fit == Fit::Width),
                    MenuItem::check("Fit page", fit == Fit::Page),
                    MenuItem::Separator,
                    MenuItem::check("Single page", l == Layout::Single),
                    MenuItem::check("Two pages", l == Layout::Spread),
                    MenuItem::check("Start with two pages", l == Layout::SpreadFirst),
                    MenuItem::Separator,
                ];
                const ZOOMS: [(f32, &str); 8] = [
                    (0.5, "50%"),
                    (0.75, "75%"),
                    (1.0, "100%"),
                    (1.25, "125%"),
                    (1.5, "150%"),
                    (2.0, "200%"),
                    (3.0, "300%"),
                    (4.0, "400%"),
                ];
                items.extend(ZOOMS.iter().map(|(z, t)| {
                    let on = matches!(fit, Fit::Zoom(cur) if (cur - *z).abs() < 0.005);
                    MenuItem::check(t, on)
                }));
                match widgets::drop_menu(ctx, p, id, anchor, false, &items) {
                    MenuResult::Open => {}
                    MenuResult::Closed => self.menu = None,
                    MenuResult::Clicked(i) => {
                        self.menu = None;
                        match i {
                            0 => self.view.set_fit(Fit::Width),
                            1 => self.view.set_fit(Fit::Page),
                            3..=5 => {
                                let (layout, name) = [
                                    (Layout::Single, "single"),
                                    (Layout::Spread, "spread"),
                                    (Layout::SpreadFirst, "spread-first"),
                                ][i - 3];
                                self.view.set_layout(layout);
                                out.push(Action::Layout(name));
                            }
                            i if i >= 7 => self.view.set_fit(Fit::Zoom(ZOOMS[i - 7].0)),
                            _ => {}
                        }
                    }
                }
            }
            Menu::More(anchor) => {
                #[derive(Clone, Copy)]
                enum MoreAct {
                    ZoomIn,
                    ZoomOut,
                    Outline,
                    Thumbs,
                    Present,
                    External,
                    PrevPage,
                    NextPage,
                }
                let (cur, n) = (self.view.current_page, self.view.page_count());
                let place = format!("{cur} / {n}");
                let mut items = Vec::new();
                let mut actions: Vec<Option<MoreAct>> = Vec::new();
                let push_sep = |items: &mut Vec<MenuItem<'_>>, acts: &mut Vec<Option<MoreAct>>| {
                    if !items.is_empty() && !matches!(items.last(), Some(MenuItem::Separator)) {
                        items.push(MenuItem::Separator);
                        acts.push(None);
                    }
                };
                if !self.zoom_btns_in_bar {
                    items.push(MenuItem::new(icons::ZOOM_IN, "Zoom in"));
                    actions.push(Some(MoreAct::ZoomIn));
                    items.push(MenuItem::new(icons::ZOOM_OUT, "Zoom out"));
                    actions.push(Some(MoreAct::ZoomOut));
                }
                if !self.side_btns_in_bar {
                    push_sep(&mut items, &mut actions);
                    items.push(MenuItem::check(
                        "Document outline",
                        self.side == Some(Side::Outline),
                    ));
                    actions.push(Some(MoreAct::Outline));
                    items.push(MenuItem::check(
                        "Page thumbnails",
                        self.side == Some(Side::Thumbs),
                    ));
                    actions.push(Some(MoreAct::Thumbs));
                }
                if !self.present_in_bar || !self.side_btns_in_bar {
                    push_sep(&mut items, &mut actions);
                    if !self.present_in_bar {
                        items.push(MenuItem::new(icons::PRESENTATION, "Present full screen"));
                        actions.push(Some(MoreAct::Present));
                    }
                    if !self.side_btns_in_bar {
                        items.push(MenuItem::new(
                            icons::EXTERNAL_LINK,
                            "Open in external viewer",
                        ));
                        actions.push(Some(MoreAct::External));
                    }
                }
                if !self.pages_in_bar {
                    push_sep(&mut items, &mut actions);
                    items.push(
                        MenuItem::new(icons::CHEVRON_LEFT, "Previous page")
                            .hint(&place)
                            .enabled(cur > 1),
                    );
                    actions.push(Some(MoreAct::PrevPage));
                    items.push(
                        MenuItem::new(icons::CHEVRON_RIGHT, "Next page")
                            .hint(&place)
                            .enabled(cur < n),
                    );
                    actions.push(Some(MoreAct::NextPage));
                }
                match widgets::drop_menu(ctx, p, id, anchor, true, &items) {
                    MenuResult::Open => {}
                    MenuResult::Closed => self.menu = None,
                    MenuResult::Clicked(i) => {
                        self.menu = None;
                        if let Some(Some(act)) = actions.get(i).copied() {
                            match act {
                                MoreAct::ZoomIn => self.view.zoom_by(1.15),
                                MoreAct::ZoomOut => self.view.zoom_by(1.0 / 1.15),
                                MoreAct::Outline => self.toggle_side(Side::Outline),
                                MoreAct::Thumbs => self.toggle_side(Side::Thumbs),
                                MoreAct::Present => out.push(Action::Present),
                                MoreAct::External => out.push(Action::OpenExternally),
                                MoreAct::PrevPage => {
                                    self.view.goto_page(cur.saturating_sub(1).max(1))
                                }
                                MoreAct::NextPage => self.view.goto_page((cur + 1).min(n)),
                            }
                        }
                    }
                }
            }
        }
    }
}

fn toggle(cur: &Option<Menu>, next: Menu) -> Option<Menu> {
    let same = matches!(
        (cur, &next),
        (Some(Menu::Zoom(_)), Menu::Zoom(_)) | (Some(Menu::More(_)), Menu::More(_))
    );
    if same { None } else { Some(next) }
}

fn ghost<'a>(_ui: &Ui, _p: &Palette, icon: Icon, label: &'a str, size: f32) -> Button<'a> {
    Button::icon(icon, label).size(Size::Sm).icon_size(size)
}

/// `.group`: buttons 2 px apart.
fn group(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        add(ui);
    });
}

fn outline_nodes(
    ui: &mut Ui,
    p: &Palette,
    nodes: &[OutlineNode],
    path: &mut Vec<usize>,
    closed: &mut HashSet<Vec<usize>>,
    go: &mut Option<super::engine::Target>,
) {
    for (i, n) in nodes.iter().enumerate() {
        path.push(i);
        let depth = path.len() - 1;
        let open = !closed.contains(path);
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
        let row = row.with_min_x(row.left() + depth as f32 * 14.0);
        let twist = Rect::from_min_size(row.min, vec2(18.0, 22.0));
        if !n.children.is_empty() {
            let r = ui.interact(
                twist,
                ui.scope_id().with(("twist", path.clone())),
                Sense::click(),
            );
            r.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Button,
                    true,
                    if open { "Collapse" } else { "Expand" },
                )
            });
            let angle = if open {
                std::f32::consts::FRAC_PI_2
            } else {
                0.0
            };
            icons::paint_rotated(
                ui.painter(),
                Rect::from_center_size(twist.center(), vec2(12.0, 12.0)),
                icons::CHEVRON_RIGHT,
                p.muted,
                angle,
            );
            if r.clicked() {
                if open {
                    closed.insert(path.clone());
                } else {
                    closed.remove(path);
                }
            }
        }
        let title = Rect::from_min_max(pos2(twist.right() + 2.0, row.top()), row.max);
        let r = ui.interact(
            title,
            ui.scope_id().with(("title", path.clone())),
            Sense::click(),
        );
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &n.title));
        if r.hovered() {
            ui.painter()
                .rect_filled(title, CornerRadius::same(theme::R_SM), p.accent_soft);
        }
        let g = ui
            .painter()
            .layout_no_wrap(n.title.clone(), FontId::proportional(12.5), p.ink);
        let clip = ui.painter().with_clip_rect(title.shrink2(vec2(6.0, 0.0)));
        clip.galley(
            pos2(title.left() + 6.0, title.center().y - g.size().y / 2.0),
            g,
            p.ink,
        );
        let r = r.on_hover_text(&n.title);
        if r.clicked() {
            *go = n.target.clone();
        }
        if open && !n.children.is_empty() {
            outline_nodes(ui, p, &n.children, path, closed, go);
        }
        path.pop();
    }
}

/// `.empty`: what the pane says before there is a PDF. True when "Show logs" was clicked.
fn empty_state(ui: &mut Ui, p: &Palette, b: &Build) -> bool {
    let area = ui.max_rect();
    let w = 340.0f32.min(area.width() * 0.8);
    let top = area.top() + area.height() * 0.34;
    let mut c = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(
                pos2(area.center().x - w / 2.0, top),
                vec2(w, area.bottom() - top),
            ))
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    c.spacing_mut().item_spacing.y = 8.0;
    let para = |c: &mut Ui, bold: &str, rest: &str| {
        let mut r = widgets::Rich::new();
        if !bold.is_empty() {
            r = r.add(bold, FontId::new(13.0, theme::font::sans_600()), p.ink_2);
        }
        r.add(rest, FontId::proportional(13.0), p.muted)
            .line_height(20.8)
            .wrap(w)
            .centered()
            .show(c);
    };
    let mut clicked = false;
    if b.building {
        para(
            &mut c,
            "Compiling…",
            " your PDF appears here when it's done.",
        );
    } else if b.failed {
        para(
            &mut c,
            "No PDF was produced.",
            " The last compile failed before it could write one.",
        );
        para(
            &mut c,
            "",
            "Open the logs to see what went wrong; fixing the first error is usually enough.",
        );
        clicked = Button::new("Show logs")
            .size(Size::Sm)
            .show(&mut c, p)
            .clicked();
    } else {
        para(&mut c, "Your PDF appears here", " after the first compile.");
        let keys = display(keys_of("sync"));
        para(
            &mut c,
            "",
            &format!(
                "Double-click in the PDF to jump to the source. Press {keys} in the editor to find the current line here."
            ),
        );
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thumbnail_harness() -> egui_kittest::Harness<'static, Pane> {
        let p = crate::theme::LIGHT;
        let mut pane = Pane::new(&egui::Context::default());
        pane.view.pages = vec![vec2(600.0, 800.0); 1_000];
        egui_kittest::Harness::builder()
            .with_size([220.0, 600.0])
            .with_context_setup(move |ctx| {
                crate::theme::install_fonts(ctx);
                crate::theme::apply(ctx, &p);
            })
            .wgpu()
            .build_ui_state(
                move |ui, pane: &mut Pane| {
                    pane.thumbnails(ui, &p);
                },
                pane,
            )
    }

    #[test]
    fn a_page_typed_but_not_entered_is_dropped_when_focus_leaves() {
        use egui_kittest::kittest::Queryable;
        let p = crate::theme::LIGHT;
        let mut pane = Pane::new(&egui::Context::default());
        pane.view.pages = vec![vec2(600.0, 800.0); 20];
        let mut h = egui_kittest::Harness::builder()
            .with_size([300.0, 200.0])
            .with_context_setup(crate::theme::install_fonts)
            .build_ui_state(
                move |ui, pane: &mut Pane| {
                    pane.page_box(ui, &p, 3, 38.0);
                },
                pane,
            );
        h.run();
        h.get_by_label("Page").click();
        h.run();
        h.get_by_label("Page").type_text("12");
        h.run();
        assert_eq!(h.state().page_input, "12");
        for pressed in [true, false] {
            h.input_mut().events.push(egui::Event::PointerButton {
                pos: pos2(250.0, 150.0),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.run();
        }
        assert_eq!(h.state().page_input, "");
    }

    #[test]
    fn unchanged_thumbnail_frames_create_only_nearby_widgets() {
        let mut h = thumbnail_harness();
        h.run();
        PDF_THUMB_WIDGETS.store(0, std::sync::atomic::Ordering::Relaxed);
        h.step();
        assert!(PDF_THUMB_WIDGETS.load(std::sync::atomic::Ordering::Relaxed) < 100);
    }

    #[test]
    #[ignore]
    fn unchanged_frames_in_a_1000_page_thumbnail_list_stay_fast() {
        let mut h = thumbnail_harness();
        h.run();
        let mut frames: Vec<_> = (0..50)
            .map(|_| {
                let start = std::time::Instant::now();
                h.step();
                start.elapsed().as_secs_f64() * 1000.0
            })
            .collect();
        frames.sort_by(f64::total_cmp);
        eprintln!(
            "1k-page unchanged thumbnails: median {:.3} ms, p95 {:.3} ms",
            frames[25], frames[47]
        );
    }

    #[test]
    fn pdf_bar_progressive_collapse_keeps_controls_visible_without_overlap() {
        use egui_kittest::kittest::Queryable;
        for w in [700.0, 590.0, 560.0, 525.0, 480.0, 430.0, 360.0, 320.0] {
            let mut h = crate::testing::cpu_harness_state(
                [w, 400.0],
                crate::theme::LIGHT,
                |ui, pane: &mut Option<Pane>| {
                    let pane = pane.get_or_insert_with(|| {
                        let mut pn = Pane::new(ui.ctx());
                        pn.view.pages = vec![vec2(600.0, 800.0); 12];
                        pn.view.current_page = 2;
                        pn
                    });
                    let b = Build {
                        diagnostics: &[],
                        building: false,
                        note: None,
                        failed: false,
                        status: None,
                        id: 0,
                        problems_open: false,
                        raw_log_open: false,
                        source: &|_, _, _| Vec::new(),
                    };
                    pane.show(ui, &crate::theme::LIGHT, &b);
                },
                None,
            );
            h.run_steps(2);
            crate::testing::assert_ui_geometry(&h, &format!("pdf bar w={w}"));

            // Verify progressive visibility.
            assert_eq!(h.query_by_label("Zoom in").is_some(), w > 580.0, "w={w}");
            assert_eq!(
                h.query_by_label("Document outline").is_some(),
                w > 520.0,
                "w={w}"
            );
            assert_eq!(
                h.query_by_label("Present full screen").is_some(),
                w > 450.0,
                "w={w}"
            );
            assert_eq!(h.query_by_label("More").is_some(), w <= 520.0, "w={w}");

            // Verify left-side controls (Zoom level / Zoom in) NEVER overlap right-side buttons!
            let left_edge = h
                .query_by_label("Zoom in")
                .or_else(|| h.query_by_label("Zoom level"))
                .unwrap()
                .rect()
                .right();
            let right_edge = [
                "Search in PDF",
                "Document outline",
                "Page thumbnails",
                "Present full screen",
                "Export PDF",
                "More",
            ]
            .into_iter()
            .filter_map(|lbl| h.query_by_label(lbl).map(|n| n.rect().left()))
            .fold(f32::INFINITY, f32::min);
            assert!(
                left_edge <= right_edge + 1.0,
                "PDF toolbar controls overlapped at w={w}: left_edge={left_edge} > right_edge={right_edge}"
            );
        }
    }
}
