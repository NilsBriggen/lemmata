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
    pub logs_open: bool,
    /// The source of a project file, for the excerpts under errors.
    pub source: &'a dyn Fn(&str) -> Option<String>,
}

/// What the pane asks of the workspace.
pub enum Action {
    Logs(bool),
    SourceClick(usize, f32, f32),
    Goto(String, usize),
    AddPackage(String),
    Export,
    OpenExternally,
    Present,
    Layout(&'static str),
    /// Opens the log through the backend; answered with `set_log`.
    ReadLog,
    OpenTerminal(String),
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
    query: String,
    /// When the query last changed; it is searched 180 ms later.
    query_at: Option<f64>,
    side: Option<Side>,
    outline_closed: HashSet<Vec<usize>>,
    menu: Option<Menu>,
    /// The page whose thumbnail was last scrolled into view.
    thumb_page: usize,
}

impl Pane {
    pub fn new(ctx: &egui::Context) -> Self {
        Self {
            view: PdfView::new(ctx, egui::Id::unique("pdf-view")),
            problems: Problems::default(),
            page_input: String::new(),
            search_open: false,
            focus_search: false,
            query: String::new(),
            query_at: None,
            side: None,
            outline_closed: HashSet::new(),
            menu: None,
            thumb_page: 0,
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

        // Ctrl+F while the PDF has focus searches the PDF, not the editor.
        if self.view_focused(&ctx) && crate::shortcuts::consume(&ctx, "pdf-search") {
            out.push(Action::Logs(false));
            self.open_search();
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
            ctx.request_repaint();
        }
        let mut y = progress.bottom();

        if self.search_open && !b.logs_open {
            let r = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), 35.0));
            ui.painter().rect_filled(r, 0, p.surface);
            ui.painter()
                .hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.border));
            self.search_row(
                &mut ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(8.0, 4.0)))),
                p,
            );
            y = r.bottom();
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
        if let Some(side) = self.side.filter(|_| !b.logs_open) {
            let r = Rect::from_min_size(body.min, vec2(220.0, body.height()));
            ui.painter().rect_filled(r, 0, p.surface);
            ui.painter()
                .vline(r.right() - 0.5, r.y_range(), Stroke::new(1.0, p.border));
            let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(6.0, 8.0))));
            match side {
                Side::Thumbs => self.thumbnails(&mut c, p),
                Side::Outline => self.outline(&mut c, p),
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
        if b.failed && has_pdf && !b.logs_open && !b.building {
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
            if Button::new("Show logs")
                .size(Size::Sm)
                .show(&mut c, p)
                .clicked()
            {
                out.push(Action::Logs(true));
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
        if b.logs_open {
            out.extend(self.problems.show(&mut c, p, b));
        } else if has_pdf {
            if let Some(super::view::Event::SourceClick(n, x, y)) = self.view.show(&mut c, p) {
                out.push(Action::SourceClick(n, x, y));
            }
        } else {
            self.view.show(&mut c, p);
            if empty_state(&mut c, p, b) {
                out.push(Action::Logs(true));
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
        ui.spacing_mut().item_spacing.x = 10.0;
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
            if logs_button(ui, p, b.logs_open, errors, warnings).clicked() {
                out.push(Action::Logs(!b.logs_open));
            }
            if b.logs_open || !self.has_pdf() {
                return;
            }
            let narrow = width <= 560.0;
            let n = self.view.page_count();
            let cur = self.view.current_page;
            if width > 420.0 {
                group(ui, |ui| {
                    if ghost(ui, p, icons::CHEVRON_LEFT, "Previous page", 16.0)
                        .enabled(cur > 1)
                        .show_icon(ui, p)
                        .clicked()
                    {
                        self.view.goto_page(cur - 1);
                    }
                    self.page_box(ui, p, cur);
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
                if !narrow
                    && ghost(ui, p, icons::ZOOM_OUT, "Zoom out", 16.0)
                        .show_icon(ui, p)
                        .clicked()
                {
                    self.view.zoom_by(1.0 / 1.15);
                }
                let text = format!("{}%", self.view.percent);
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
                if !narrow
                    && ghost(ui, p, icons::ZOOM_IN, "Zoom in", 16.0)
                        .show_icon(ui, p)
                        .clicked()
                {
                    self.view.zoom_by(1.15);
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if narrow {
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
                if !narrow {
                    if ghost(ui, p, icons::EXTERNAL_LINK, "Open in external viewer", 15.0)
                        .show_icon(ui, p)
                        .on_hover_text("Open in the system's PDF viewer")
                        .clicked()
                    {
                        out.push(Action::OpenExternally);
                    }
                    if ghost(ui, p, icons::PRESENTATION, "Present full screen", 15.0)
                        .show_icon(ui, p)
                        .on_hover_text("Present full screen")
                        .clicked()
                    {
                        out.push(Action::Present);
                    }
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

    /// `.page-input`: 38×24, the current page as its placeholder; Enter goes there.
    fn page_box(&mut self, ui: &mut Ui, p: &Palette, cur: usize) {
        let fill = ui.painter().add(egui::Shape::Noop);
        let hint = ui.painter().add(egui::Shape::Noop);
        let r = egui::TextEdit::singleline(&mut self.page_input)
            .font(FontId::proportional(12.0))
            .text_color(p.ink)
            .frame(egui::Frame::NONE)
            .align(egui::Align2::CENTER_CENTER)
            .min_size(vec2(38.0, 24.0))
            .desired_width(38.0)
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
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if let Ok(n) = self.page_input.trim().parse::<usize>()
                && n > 0
            {
                self.view.goto_page(n);
            }
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
                if r.changed() {
                    self.query_at = Some(ui.input(|i| i.time));
                }
                if r.has_focus() || r.lost_focus() {
                    let (esc, enter, shift) = ui.input(|i| {
                        (
                            i.key_pressed(egui::Key::Escape),
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
    fn outline(&mut self, ui: &mut Ui, p: &Palette) {
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
            return;
        }
        let nodes = self.view.outline.clone();
        let mut go = None;
        egui::ScrollArea::vertical()
            .id_salt("pdf-outline")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                outline_nodes(
                    ui,
                    p,
                    &nodes,
                    &mut Vec::new(),
                    &mut self.outline_closed,
                    &mut go,
                );
            });
        if let Some(t) = go
            && let Some(url) = self.view.goto_target(&t)
            && (url.starts_with("http://") || url.starts_with("https://"))
        {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
    }

    /// PdfThumbnails.svelte: a small render of every page, drawn as it scrolls into view.
    fn thumbnails(&mut self, ui: &mut Ui, p: &Palette) {
        let ctx = ui.ctx().clone();
        let n = self.view.page_count();
        let current = self.view.current_page;
        let follow = std::mem::replace(&mut self.thumb_page, current) != current;
        egui::ScrollArea::vertical()
            .id_salt("pdf-thumbs")
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let w = ui.available_width().min(176.0);
                let x = ui.max_rect().left() + (ui.available_width() - w) / 2.0;
                let mut y = ui.max_rect().top();
                let near = viewport
                    .translate(ui.max_rect().min.to_vec2())
                    .expand2(vec2(0.0, viewport.height() * 0.5));
                for i in 0..n {
                    let size = self.view.pages[i];
                    let h = w * size.y / size.x;
                    let img = Rect::from_min_size(pos2(x, y), vec2(w, h));
                    let all =
                        Rect::from_min_max(img.min, pos2(img.right(), img.bottom() + 3.0 + 15.0));
                    let resp = ui.interact(all, ui.scope_id().with(("thumb", i)), Sense::click());
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            true,
                            format!("Page {}", i + 1),
                        )
                    });
                    if follow && i + 1 == current {
                        ui.scroll_to_rect(all, None);
                    }
                    if near.intersects(all) {
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
                    }
                    if resp.clicked() {
                        self.view.goto_page(i + 1);
                    }
                    y = all.bottom() + 10.0;
                }
                ui.allocate_rect(
                    Rect::from_min_max(ui.max_rect().min, pos2(ui.max_rect().right(), y)),
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
                let mut items = vec![
                    MenuItem::plain("Fit width"),
                    MenuItem::plain("Fit page"),
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
                items.extend(ZOOMS.iter().map(|(_, t)| MenuItem::plain(t)));
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
                let items = [
                    MenuItem::new(icons::ZOOM_IN, "Zoom in"),
                    MenuItem::new(icons::ZOOM_OUT, "Zoom out"),
                    MenuItem::check("Document outline", self.side == Some(Side::Outline)),
                    MenuItem::check("Page thumbnails", self.side == Some(Side::Thumbs)),
                    MenuItem::new(icons::PRESENTATION, "Present full screen"),
                    MenuItem::new(icons::EXTERNAL_LINK, "Open in external viewer"),
                ];
                match widgets::drop_menu(ctx, p, id, anchor, true, &items) {
                    MenuResult::Open => {}
                    MenuResult::Closed => self.menu = None,
                    MenuResult::Clicked(i) => {
                        self.menu = None;
                        match i {
                            0 => self.view.zoom_by(1.15),
                            1 => self.view.zoom_by(1.0 / 1.15),
                            2 => self.toggle_side(Side::Outline),
                            3 => self.toggle_side(Side::Thumbs),
                            4 => out.push(Action::Present),
                            _ => out.push(Action::OpenExternally),
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

/// `.btn.sm.logs`: "Logs" with the error and warning counts, accent while the logs show.
fn logs_button(
    ui: &mut Ui,
    p: &Palette,
    on: bool,
    errors: usize,
    warnings: usize,
) -> egui::Response {
    let font = FontId::new(12.0, theme::font::sans_500());
    let text = ui
        .painter()
        .layout_no_wrap("Logs".into(), font, Color32::PLACEHOLDER);
    let badge_w = |n: usize| {
        ui.painter()
            .layout_no_wrap(
                n.to_string(),
                FontId::new(11.0, theme::font::sans_600()),
                Color32::WHITE,
            )
            .size()
            .x
            + 12.0
    };
    let mut w = 9.0 + 15.0 + 6.0 + text.size().x + 9.0;
    for n in [errors, warnings].into_iter().filter(|n| *n > 0) {
        w += 6.0 + badge_w(n);
    }
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, "Logs"));
    let resp = resp
        .on_hover_text("Logs and problems")
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let t = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), theme::EASE_SECS);
    let (fill, stroke, ink) = if on {
        (p.accent_soft, p.accent, p.accent)
    } else {
        (
            p.surface.lerp_to_gamma(p.surface_2, t),
            p.border_strong.lerp_to_gamma(p.faint, t),
            p.ink,
        )
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(theme::R),
        fill,
        Stroke::new(1.0, stroke),
        StrokeKind::Inside,
    );
    let mut x = rect.left() + 9.0;
    icons::paint(
        painter,
        Rect::from_min_size(pos2(x, rect.center().y - 7.5), vec2(15.0, 15.0)),
        icons::SCROLL_TEXT,
        ink,
    );
    x += 21.0;
    painter.galley(
        pos2(x, rect.center().y - text.size().y / 2.0),
        text.clone(),
        ink,
    );
    x += text.size().x + 6.0;
    for (n, fill) in [(errors, p.error), (warnings, p.warn)] {
        if n == 0 {
            continue;
        }
        let g = painter.layout_no_wrap(
            n.to_string(),
            FontId::new(11.0, theme::font::sans_600()),
            Color32::WHITE,
        );
        let r = Rect::from_min_size(
            pos2(x, rect.center().y - 9.0),
            vec2(g.size().x + 12.0, 18.0),
        );
        painter.rect_filled(r, CornerRadius::same(9), fill);
        painter.galley(r.center() - g.size() / 2.0, g, Color32::WHITE);
        x = r.right() + 6.0;
    }
    resp
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
        let row = row.with_min_x(row.left() + depth as f32 * 4.0);
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
