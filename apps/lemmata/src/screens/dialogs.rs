//! The library's dialogs: New project (NewProjectDialog.svelte), Clone a repository
//! (CloneDialog.svelte), Import from Overleaf (ImportDialog.svelte) and Save as template
//! (SaveTemplateDialog.svelte). Each keeps its own fields; the app runs what they ask for.

use crate::icons::{self, Icon};
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2,
};
use lemmata_shell::projects::{Template, ZipInfo};
use std::collections::HashMap;

#[derive(Debug, PartialEq)]
pub enum Action {
    Close,
    /// The answer to "Do you trust this project?": true trusts it, false opens it restricted.
    Trust(bool),
    /// Choose the folder the project goes in.
    ChooseParent,
    Submit,
    DeleteTemplate(String),
    RetryOnline,
}

/// Writes a field row: the text input stretches, the Choose… button sits at its end.
fn location(
    ui: &mut Ui,
    p: &Palette,
    label: &str,
    parent: &mut String,
    enabled: bool,
    action: &mut Option<Action>,
) {
    widgets::field(ui, p, label, |ui| {
        // The button keeps its width; the input takes the rest.
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 32.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if Button::new("Choose…")
                    .with_icon(icons::FOLDER_OPEN)
                    .enabled(enabled)
                    .show(ui, p)
                    .clicked()
                {
                    *action = Some(Action::ChooseParent);
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(enabled, |ui| widgets::text_input(ui, p, parent, "", true));
                });
            },
        );
    });
}

fn error_line(ui: &mut Ui, p: &Palette, error: &Option<String>) {
    if let Some(e) = error {
        widgets::paragraph(
            ui,
            e,
            FontId::proportional(13.0),
            p.error,
            1.45,
            ui.available_width(),
        );
    }
}

// ── New project ─────────────────────────────────────────────────────────────

/// A big button per everyday kind of document, a few templates each. The user's own templates and
/// the publishers' (IEEE, ACM, Springer, …) sit in smaller bars below, keeping the grid at six.
const ORDER: [&str; 6] = [
    "Papers",
    "School",
    "Theses & reports",
    "Presentations",
    "Posters",
    "Letters & CV",
];
const MINE: &str = "My templates";
const FOLDER: &str = "Journals & conferences";

fn category_icon(c: &str) -> Icon {
    match c {
        "School" => icons::GRADUATION_CAP,
        "Theses & reports" => icons::BOOK_OPEN,
        "Presentations" => icons::PRESENTATION,
        "Posters" => icons::LAYOUT_PANEL_TOP,
        "Letters & CV" => icons::MAIL,
        _ => icons::FILE_TEXT,
    }
}

pub struct NewProject {
    pub templates: Vec<Template>,
    pub chosen: String,
    pub name: String,
    pub parent: String,
    pub busy: bool,
    pub error: Option<String>,
    /// The selected category filter; None shows all bundled + user templates.
    pub shown: Option<String>,
    /// Filter query inside the template picker.
    pub query: String,
    /// The template collection is being fetched.
    pub online_loading: bool,
    /// Why the collection couldn't be fetched.
    pub online_error: Option<String>,
    /// From a lemmata://new link: the template to choose, bundled or from the collection.
    pub initial: String,
    previews: HashMap<String, Option<egui::TextureHandle>>,
    focused: bool,
}

impl NewProject {
    pub fn new(parent: String, initial: String) -> Self {
        Self {
            templates: Vec::new(),
            chosen: "article".into(),
            name: String::new(),
            parent,
            busy: false,
            error: None,
            shown: None,
            query: String::new(),
            online_loading: true,
            online_error: None,
            initial,
            previews: HashMap::new(),
            focused: false,
        }
    }

    fn cat_of(t: &Template) -> &str {
        if t.user {
            MINE
        } else if t.category.is_empty() {
            "Other"
        } else {
            &t.category
        }
    }

    fn in_cat(&self, c: &str) -> Vec<&Template> {
        self.templates
            .iter()
            .filter(|t| Self::cat_of(t) == c)
            .collect()
    }

    fn categories(&self) -> Vec<String> {
        let mut cats: Vec<String> = Vec::new();
        for t in &self.templates {
            let c = Self::cat_of(t);
            if c != FOLDER && c != MINE && !cats.iter().any(|x| x == c) {
                cats.push(c.into());
            }
        }
        let rank = |c: &str| ORDER.iter().position(|o| *o == c).map_or(99, |i| i + 1);
        cats.sort_by_key(|c| rank(c));
        cats
    }

    /// The list arrived (or changed): choose the linked template, or the first one.
    pub fn set_templates(&mut self, templates: Vec<Template>, first: bool) {
        self.templates = templates;
        let linked = self
            .templates
            .iter()
            .find(|t| t.id == self.initial || t.id == format!("online:{}", self.initial))
            .map(|t| t.id.clone());
        match linked {
            Some(id) if !self.initial.is_empty() => {
                self.chosen = id;
                // A template chosen by a link opens its category.
                let cat = self
                    .templates
                    .iter()
                    .find(|t| t.id == self.chosen)
                    .map(|t| Self::cat_of(t).to_owned());
                self.shown = cat;
            }
            _ if first => {
                if let Some(t) = self.templates.first() {
                    self.chosen = t.id.clone();
                }
            }
            _ => {}
        }
    }

    /// After deleting a template: leave an emptied category and fall back to another choice.
    pub fn removed(&mut self, id: &str) {
        self.templates.retain(|t| t.id != id);
        if let Some(c) = &self.shown
            && self.in_cat(c).is_empty()
        {
            self.shown = None;
        }
        if self.chosen == id {
            self.chosen = self
                .shown
                .as_deref()
                .and_then(|c| self.in_cat(c).first().map(|t| t.id.clone()))
                .unwrap_or_else(|| "article".into());
        }
    }

    fn show_cat(&mut self, c: &str) {
        self.shown = Some(c.into());
        let first = self.in_cat(c).first().map(|t| t.id.clone());
        if !self.in_cat(c).iter().any(|t| t.id == self.chosen)
            && let Some(f) = first
        {
            self.chosen = f;
        }
    }

    pub fn can_create(&self) -> bool {
        !self.busy
            // A template is chosen from the start: no need to open a category first.
            && self.templates.iter().any(|t| t.id == self.chosen)
            && !self.name.trim().is_empty()
            && !self.parent.is_empty()
    }
}

/// Text clamped to `lines` lines, the last one ending in "…" when cut.
fn clamped(
    ui: &Ui,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
    lines: usize,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple(text.into(), font, color, width);
    job.wrap.max_rows = lines;
    job.wrap.overflow_character = Some('…');
    for s in &mut job.sections {
        s.format.line_height = Some(11.5 * 1.4);
    }
    ui.fonts_mut(|f| f.layout_job(job))
}

pub fn new_project(ctx: &egui::Context, p: &Palette, st: &mut NewProject) -> Option<Action> {
    let mut action = None;
    let can = st.can_create();
    let busy = st.busy;
    let chosen_name = st
        .templates
        .iter()
        .find(|t| t.id == st.chosen)
        .map(|t| t.name.clone());

    let screen = ctx.content_rect();
    let w = 940.0f32.min(screen.width() - 40.0).max(360.0);
    let h = 640.0f32.min(screen.height() - 40.0).max(380.0);
    let compact = w < 700.0;

    let previous_focus = widgets::modal_focus_enter(ctx, "new-project-modal");
    let any_popup_open = egui::Popup::is_any_open(ctx);
    let _backdrop = egui::Area::new(egui::Id::unique("new-project-backdrop"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, r) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter()
                .rect_filled(rect, 0, Color32::from_rgba_unmultiplied(12, 17, 22, 92));
            r
        });
    let mut closed = false;

    let area = egui::Area::new(egui::Id::unique("new-project-modal"))
        .kind(egui::UiKind::Modal)
        .order(egui::Order::Foreground)
        .pivot(Align2::CENTER_CENTER)
        .fixed_pos(screen.center());
    let layer = area.layer();
    ctx.memory_mut(|m| m.set_modal_layer(layer));
    let panel_resp = area.show(ctx, |ui| {
        let (outer, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
        let rad = CornerRadius::same(theme::R_XL);
        ui.painter().add(p.shadow_lg.as_shape(outer, rad));
        ui.painter().rect(
            outer,
            rad,
            p.surface,
            Stroke::new(1.0, p.border),
            StrokeKind::Inside,
        );
        let inner = outer.shrink(1.0);
        let foot_h = if st.error.is_some() { 150.0 } else { 126.0 };
        let foot = Rect::from_min_max(pos2(inner.left(), inner.bottom() - foot_h), inner.max);
        let top = Rect::from_min_max(inner.min, pos2(inner.right(), foot.top()));

        // Footer surface_2 bar with Folder name, Location, selected template, Cancel & Create.
        ui.painter().rect_filled(
            foot,
            CornerRadius {
                nw: 0,
                ne: 0,
                sw: theme::R_XL - 1,
                se: theme::R_XL - 1,
            },
            p.surface_2,
        );
        ui.painter()
            .hline(foot.x_range(), foot.top() + 0.5, Stroke::new(1.0, p.border));

        // Left category nav (210 px) + Right template gallery (or stacked when compact).
        let (nav_rect, main_rect) = if compact {
            let nr = Rect::from_min_size(top.min, Vec2::new(top.width(), 88.0));
            (
                nr,
                Rect::from_min_max(pos2(top.left(), nr.bottom()), top.max),
            )
        } else {
            let nr = Rect::from_min_size(top.min, Vec2::new(232.0, top.height()));
            (nr, Rect::from_min_max(pos2(nr.right(), top.top()), top.max))
        };
        ui.painter().rect_filled(
            nav_rect,
            CornerRadius {
                nw: theme::R_XL - 1,
                ne: if compact { theme::R_XL - 1 } else { 0 },
                sw: 0,
                se: 0,
            },
            p.surface_2,
        );
        if compact {
            ui.painter().hline(
                nav_rect.x_range(),
                nav_rect.bottom() - 0.5,
                Stroke::new(1.0, p.border),
            );
        } else {
            ui.painter().vline(
                nav_rect.right() - 0.5,
                nav_rect.y_range(),
                Stroke::new(1.0, p.border),
            );
        }

        // Populate left category navigation.
        let cats = st.categories();
        let mine_n = st.in_cat(MINE).len();
        let online_n = st.in_cat(FOLDER).len();
        let all_n = st
            .templates
            .iter()
            .filter(|t| NewProject::cat_of(t) != FOLDER)
            .count();
        let mut nav_ui =
            ui.new_child(egui::UiBuilder::new().max_rect(nav_rect.shrink2(Vec2::new(12.0, 14.0))));
        if busy {
            nav_ui.disable();
        }
        if compact {
            nav_ui.spacing_mut().item_spacing.y = 6.0;
            nav_ui.label(
                egui::RichText::new("New project")
                    .font(FontId::new(18.0, theme::font::display()))
                    .color(p.ink),
            );
            egui::ScrollArea::horizontal()
                .id_salt("np-cats-h")
                .show(&mut nav_ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if Button::new("All templates")
                            .kind(Kind::Ghost)
                            .size(widgets::Size::Sm)
                            .on(st.shown.is_none())
                            .show(ui, p)
                            .clicked()
                        {
                            st.shown = None;
                        }
                        for c in &cats {
                            if Button::new(c)
                                .kind(Kind::Ghost)
                                .size(widgets::Size::Sm)
                                .on(st.shown.as_deref() == Some(c.as_str()))
                                .show(ui, p)
                                .clicked()
                            {
                                st.show_cat(c);
                            }
                        }
                        if mine_n > 0
                            && Button::new(MINE)
                                .kind(Kind::Ghost)
                                .size(widgets::Size::Sm)
                                .on(st.shown.as_deref() == Some(MINE))
                                .show(ui, p)
                                .clicked()
                        {
                            st.show_cat(MINE);
                        }
                        if (online_n > 0 || st.online_loading || st.online_error.is_some())
                            && Button::new(FOLDER)
                                .kind(Kind::Ghost)
                                .size(widgets::Size::Sm)
                                .on(st.shown.as_deref() == Some(FOLDER))
                                .show(ui, p)
                                .clicked()
                        {
                            st.show_cat(FOLDER);
                        }
                    });
                });
        } else {
            nav_ui.spacing_mut().item_spacing.y = 2.0;
            nav_ui.horizontal(|ui| {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("New project")
                        .font(FontId::new(20.0, theme::font::display()))
                        .color(p.ink),
                );
            });
            nav_ui.horizontal(|ui| {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Pick a template")
                        .size(12.0)
                        .color(p.muted),
                );
            });
            nav_ui.add_space(10.0);
            let all_s = all_n.to_string();
            if widgets::side_row(
                &mut nav_ui,
                p,
                Some(icons::LAYOUT_GRID),
                None,
                "All templates",
                st.shown.is_none(),
                Some(all_s.as_str()),
            )
            .clicked()
            {
                st.shown = None;
            }
            for c in &cats {
                let cnt = st.in_cat(c).len().to_string();
                if widgets::side_row(
                    &mut nav_ui,
                    p,
                    Some(category_icon(c)),
                    None,
                    c,
                    st.shown.as_deref() == Some(c.as_str()),
                    Some(cnt.as_str()),
                )
                .clicked()
                {
                    st.show_cat(c);
                }
            }
            if mine_n > 0 {
                nav_ui.add_space(8.0);
                let cnt = mine_n.to_string();
                if widgets::side_row(
                    &mut nav_ui,
                    p,
                    Some(icons::STAR),
                    Some(p.warn),
                    MINE,
                    st.shown.as_deref() == Some(MINE),
                    Some(cnt.as_str()),
                )
                .clicked()
                {
                    st.show_cat(MINE);
                }
            }
            if online_n > 0 || st.online_loading || st.online_error.is_some() {
                nav_ui.add_space(8.0);
                let cnt = if online_n > 0 {
                    Some(online_n.to_string())
                } else if st.online_loading {
                    Some("…".into())
                } else {
                    None
                };
                if widgets::side_row(
                    &mut nav_ui,
                    p,
                    Some(icons::FOLDER_OPEN),
                    None,
                    FOLDER,
                    st.shown.as_deref() == Some(FOLDER),
                    cnt.as_deref(),
                )
                .clicked()
                {
                    st.show_cat(FOLDER);
                }
            }
        }

        // Right gallery pane: top search + close button, then scrollable template cards.
        let mut main_ui = ui.new_child(egui::UiBuilder::new().max_rect(main_rect));
        let topbar = Rect::from_min_size(main_rect.min, Vec2::new(main_rect.width(), 52.0));
        let mut tb = main_ui.new_child(
            egui::UiBuilder::new()
                .max_rect(topbar.shrink2(Vec2::new(16.0, 10.0)))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        tb.spacing_mut().item_spacing.x = 8.0;
        if Button::icon(icons::X, "Close")
            .kind(Kind::Ghost)
            .size(widgets::Size::Md)
            .enabled(!busy)
            .show_icon(&mut tb, p)
            .clicked()
        {
            closed = true;
        }
        tb.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let sw = ui.available_width().min(320.0);
            ui.allocate_ui(Vec2::new(sw, 32.0), |ui| {
                widgets::search_input(ui, p, &mut st.query, "Search templates");
            });
        });

        let gallery_rect =
            Rect::from_min_max(pos2(main_rect.left(), topbar.bottom()), main_rect.max);
        let mut gal_ui = main_ui.new_child(egui::UiBuilder::new().max_rect(gallery_rect));
        if busy {
            gal_ui.disable();
        }
        egui::ScrollArea::vertical()
            .id_salt("np-templates")
            .auto_shrink([false, false])
            .show(&mut gal_ui, |ui| {
                egui::Frame::NONE
                    .inner_margin(egui::Margin {
                        left: 16,
                        right: 16,
                        top: 4,
                        bottom: 16,
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 10.0;
                        gallery_grid(ui, p, st, &mut action);
                    });
            });

        // Footer controls: Folder name + Location row, error line, Cancel + Create row.
        let mut fui =
            ui.new_child(egui::UiBuilder::new().max_rect(foot.shrink2(Vec2::new(18.0, 12.0))));
        fui.spacing_mut().item_spacing.y = 8.0;
        fui.columns(2, |cols| {
            cols[0].spacing_mut().item_spacing.y = 4.0;
            widgets::field(&mut cols[0], p, "Folder name", |ui| {
                let r = ui
                    .add_enabled_ui(!busy, |ui| {
                        widgets::text_input(ui, p, &mut st.name, "master-thesis", false)
                    })
                    .inner;
                if !st.focused {
                    r.request_focus();
                    st.focused = true;
                }
                if r.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && st.can_create()
                    && !busy
                {
                    action = Some(Action::Submit);
                }
            });
            cols[1].spacing_mut().item_spacing.y = 4.0;
            location(
                &mut cols[1],
                p,
                "Location",
                &mut st.parent,
                !busy,
                &mut action,
            );
        });
        error_line(&mut fui, p, &st.error);
        fui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if Button::new("Create project")
                .kind(Kind::Primary)
                .enabled(can && !busy)
                .show(ui, p)
                .clicked()
            {
                action = Some(Action::Submit);
            }
            if Button::new("Cancel").enabled(!busy).show(ui, p).clicked() {
                action = Some(Action::Close);
            }
            if let Some(name) = &chosen_name {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("Template: {name}"))
                            .size(12.5)
                            .color(p.muted),
                    );
                });
            }
        });
    });

    panel_resp
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Dialog, true, "New project"));
    if !busy
        && ctx.memory(|m| m.top_modal_layer() == Some(layer))
        && !any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        closed = true;
    }
    if closed || matches!(action, Some(Action::Close)) {
        widgets::modal_focus_restore(ctx, "new-project-modal", previous_focus);
        action = Some(Action::Close);
    }
    action
}

fn gallery_grid(ui: &mut Ui, p: &Palette, st: &mut NewProject, action: &mut Option<Action>) {
    let q = st.query.trim().to_lowercase();
    let list: Vec<Template> = if !q.is_empty() {
        st.templates
            .iter()
            .filter(|t| {
                t.name.to_lowercase().contains(&q)
                    || t.description.to_lowercase().contains(&q)
                    || t.category.to_lowercase().contains(&q)
            })
            .cloned()
            .collect()
    } else if let Some(cat) = st.shown.as_deref() {
        st.in_cat(cat).into_iter().cloned().collect()
    } else {
        st.templates
            .iter()
            .filter(|t| NewProject::cat_of(t) != FOLDER)
            .cloned()
            .collect()
    };

    if st.shown.as_deref() == Some(FOLDER) && q.is_empty() {
        ui.label(
            egui::RichText::new(
                "Templates with a publisher's own class, for submitting to their journals and conferences.",
            )
            .size(12.0)
            .color(p.muted),
        );
    }

    if list.is_empty() && !st.online_loading {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(if !q.is_empty() {
                format!("No templates match “{}”.", st.query)
            } else {
                "No templates in this category.".into()
            })
            .color(p.muted),
        );
    }

    let w = ui.available_width() - 4.0;
    let cols = (((w + 12.0) / 184.0).floor() as usize).max(1);
    let cw = (w - 12.0 * (cols as f32 - 1.0)) / cols as f32;
    for row in list.chunks(cols) {
        let descs: Vec<_> = row
            .iter()
            .map(|t| {
                let d = if t.description.is_empty() && t.user {
                    "Your template"
                } else {
                    &t.description
                };
                clamped(ui, d, FontId::proportional(11.5), p.muted, cw - 20.0, 2)
            })
            .collect();
        let h = 8.0
            + 108.0
            + 8.0
            + 18.0
            + 4.0
            + descs.iter().map(|g| g.size().y).fold(0.0, f32::max)
            + 10.0;
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            ui.spacing_mut().item_spacing.x = 12.0;
            for (t, desc) in row.iter().zip(descs) {
                let (rect, r) = ui.allocate_exact_size(Vec2::new(cw, h), Sense::click());
                let on = st.chosen == t.id;
                r.widget_info(|| {
                    egui::WidgetInfo::selected(egui::Role::RadioButton, true, on, &t.name)
                });
                let painter = ui.painter();
                let rad = CornerRadius::same(theme::R_MD);
                if on {
                    painter.rect_stroke(
                        rect.expand(1.5),
                        CornerRadius::same(theme::R_MD + 2),
                        Stroke::new(3.0, Palette::mix(p.accent, 18.0)),
                        StrokeKind::Outside,
                    );
                } else {
                    painter.add(p.shadow_card.as_shape(rect, rad));
                }
                let border = if on {
                    Stroke::new(2.0, p.accent)
                } else if r.hovered() {
                    Stroke::new(1.0, p.border_strong)
                } else {
                    Stroke::new(1.0, p.border)
                };
                painter.rect(rect, rad, p.surface, border, StrokeKind::Inside);

                let thumb =
                    Rect::from_min_size(rect.min + Vec2::splat(8.0), Vec2::new(cw - 16.0, 108.0));
                painter.rect_filled(thumb, CornerRadius::same(theme::R_SM), p.surface_2);
                match preview(ui.ctx(), &mut st.previews, t) {
                    Some(tex) => {
                        let [tw, th] = tex.size().map(|v| v as f32);
                        let uv_h = ((thumb.height() / thumb.width()) * tw / th).min(1.0);
                        painter.rect_filled(thumb, CornerRadius::same(theme::R_SM), Color32::WHITE);
                        painter.image(
                            tex.id(),
                            thumb,
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, uv_h)),
                            Color32::WHITE,
                        );
                    }
                    None => {
                        // Crisp miniature page silhouette for templates without a PNG preview.
                        let pw = (thumb.width() * 0.54).min(68.0);
                        let page = Rect::from_min_max(
                            pos2(thumb.center().x - pw / 2.0, thumb.top() + 10.0),
                            pos2(thumb.center().x + pw / 2.0, thumb.bottom()),
                        );
                        painter.add(p.shadow_card.as_shape(page, 2));
                        let page_bg = if p.dark { p.surface } else { Color32::WHITE };
                        let title_bar = if p.dark {
                            p.faint
                        } else {
                            Color32::from_rgb(0x9a, 0xa3, 0xad)
                        };
                        let line_bar = if p.dark {
                            p.border_strong
                        } else {
                            Color32::from_rgb(0xdf, 0xe3, 0xe7)
                        };
                        painter.rect_filled(page, 2, page_bg);
                        let mut ly = page.top() + 14.0;
                        painter.rect_filled(
                            Rect::from_min_size(
                                pos2(page.left() + 10.0, ly),
                                Vec2::new(pw - 24.0, 5.0),
                            ),
                            2,
                            title_bar,
                        );
                        ly += 12.0;
                        for k in 0..4 {
                            let lw = pw - 18.0 - if k == 3 { 14.0 } else { 0.0 };
                            painter.rect_filled(
                                Rect::from_min_size(
                                    pos2(page.left() + 9.0, ly),
                                    Vec2::new(lw.max(10.0), 3.0),
                                ),
                                1,
                                line_bar,
                            );
                            ly += 9.0;
                        }
                    }
                }
                let y = thumb.bottom() + 8.0;
                let mut nj = egui::text::LayoutJob::simple_singleline(
                    t.name.clone(),
                    FontId::new(13.0, theme::font::sans_600()),
                    p.ink,
                );
                nj.wrap.max_width = cw - 20.0;
                nj.wrap.max_rows = 1;
                nj.wrap.overflow_character = Some('…');
                let ng = ui.fonts_mut(|f| f.layout_job(nj));
                ui.painter().galley(pos2(rect.left() + 10.0, y), ng, p.ink);
                ui.painter()
                    .galley(pos2(rect.left() + 10.0, y + 20.0), desc, p.muted);
                widgets::focus_ring(ui, &r, rect, p, theme::R_MD);
                if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    st.chosen = t.id.clone();
                }
                if t.user {
                    let del = Rect::from_min_size(
                        pos2(rect.right() - 6.0 - 24.0, rect.top() + 6.0),
                        Vec2::splat(24.0),
                    );
                    let dr = ui.interact(
                        del,
                        ui.make_persistent_id(("del-template", &t.id)),
                        Sense::click(),
                    );
                    let label = format!("Delete template {}", t.name);
                    dr.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
                    let hover = dr.hovered();
                    ui.painter().rect_filled(
                        del,
                        CornerRadius::same(theme::R_SM),
                        if hover { p.surface_2 } else { p.surface },
                    );
                    icons::paint(
                        ui.painter(),
                        Rect::from_center_size(del.center(), Vec2::splat(14.0)),
                        icons::TRASH,
                        if hover { p.error } else { p.faint },
                    );
                    if dr.on_hover_text("Delete this template").clicked() {
                        *action = Some(Action::DeleteTemplate(t.id.clone()));
                    }
                }
            }
        });
    }

    if st.online_loading {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Getting more templates…")
                .size(12.0)
                .color(p.muted),
        );
    } else if let Some(e) = &st.online_error {
        let err_tip = e.clone();
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(
                egui::RichText::new(
                    "More templates are online, but they couldn't be loaded right now.",
                )
                .size(12.0)
                .color(p.muted),
            )
            .on_hover_text(&err_tip);
            let retry = ui
                .add(
                    egui::Label::new(
                        egui::RichText::new("Retry")
                            .size(12.0)
                            .color(p.accent_text)
                            .underline(),
                    )
                    .sense(Sense::click()),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            retry.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Retry"));
            if retry.clicked() {
                st.online_loading = true;
                st.online_error = None;
                *action = Some(Action::RetryOnline);
            }
        });
    }
}

/// A template's preview.png, from its `data:image/png;base64,` URL.
fn preview(
    ctx: &egui::Context,
    cache: &mut HashMap<String, Option<egui::TextureHandle>>,
    t: &Template,
) -> Option<egui::TextureHandle> {
    cache
        .entry(t.id.clone())
        .or_insert_with(|| {
            use base64::Engine;
            let data = t
                .preview
                .as_deref()?
                .strip_prefix("data:image/png;base64,")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .ok()?;
            let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
                .ok()?
                .to_rgba8();
            let size = [img.width() as usize, img.height() as usize];
            Some(ctx.load_texture(
                format!("template:{}", t.id),
                egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()),
                egui::TextureOptions::LINEAR,
            ))
        })
        .clone()
}

// ── Clone ──────────────────────────────────────────────────────────────────

pub struct Clone {
    pub url: String,
    /// Typed by the user; while unedited, the name follows the URL.
    pub typed_name: String,
    /// True once the user has edited the folder name (so clearing it does not snap back).
    pub name_edited: bool,
    pub parent: String,
    pub busy: bool,
    pub error: Option<String>,
    focused: bool,
}

/// The repository's name from its URL: the last path part without `.git`.
pub fn repo_name(url: &str) -> String {
    let u = url.trim().trim_end_matches('/');
    let u = u.strip_suffix(".git").unwrap_or(u);
    u.rsplit(['/', ':', '\\']).next().unwrap_or("").into()
}

/// git.overleaf.com URLs need a token; the dialog says how to get one.
pub fn is_overleaf(url: &str) -> bool {
    url.to_lowercase().contains("git.overleaf.com")
}

impl Clone {
    pub fn new(parent: String, url: String) -> Self {
        Self {
            url,
            typed_name: String::new(),
            name_edited: false,
            parent,
            busy: false,
            error: None,
            focused: false,
        }
    }
    pub fn name(&self) -> String {
        if !self.name_edited && self.typed_name.is_empty() {
            repo_name(&self.url)
        } else {
            self.typed_name.clone()
        }
    }
}

pub fn clone(ctx: &egui::Context, p: &Palette, st: &mut Clone) -> Option<Action> {
    let mut action = None;
    let mut body_action = None;
    let busy = st.busy;
    let can =
        !busy && !st.url.trim().is_empty() && !st.parent.is_empty() && !st.name().trim().is_empty();
    let r = widgets::modal_with(
        ctx,
        p,
        "Clone a repository",
        Some("Copies a git repository to this computer and opens it"),
        540.0,
        |ui| {
            widgets::field(ui, p, "Repository URL", |ui| {
                let r = ui
                    .add_enabled_ui(!busy, |ui| {
                        widgets::text_input(
                            ui,
                            p,
                            &mut st.url,
                            "https://github.com/you/thesis.git",
                            true,
                        )
                    })
                    .inner;
                if !st.focused {
                    r.request_focus();
                    st.focused = true;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && can {
                    body_action = Some(Action::Submit);
                }
                ui.add_space(4.0 - 5.0);
                widgets::Rich::new()
                    .add(
                        "https or ssh, such as ",
                        FontId::proportional(12.0),
                        p.muted,
                    )
                    .add(
                        "git@github.com:you/thesis.git",
                        FontId::monospace(12.0),
                        p.muted,
                    )
                    .add(
                        ". Lemmata uses the system git and its saved credentials.",
                        FontId::proportional(12.0),
                        p.muted,
                    )
                    .line_height(12.0 * 1.45)
                    .wrap(ui.available_width())
                    .show(ui);
            });
            if is_overleaf(&st.url) {
                egui::Frame::new()
                    .fill(p.surface_2)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(theme::R)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        let f = FontId::proportional(12.0);
                        let m = FontId::monospace(12.0);
                        widgets::Rich::new()
                            .add("Overleaf's git access needs a token: create one under Overleaf account settings → Git integration. Enter the URL as ", f.clone(), p.ink)
                            .add("https://git:TOKEN@git.overleaf.com/ID", m.clone(), p.ink)
                            .add(", or sign in once with ", f.clone(), p.ink)
                            .add("git clone", m, p.ink)
                            .add(" in a terminal so your credential manager remembers it. A token in the URL is kept in the project's git settings.", f, p.ink)
                            .line_height(12.0 * 1.45)
                            .wrap(ui.available_width())
                            .show(ui);
                    });
            }
            location(ui, p, "Location", &mut st.parent, !busy, &mut body_action);
            widgets::field(ui, p, "Folder name", |ui| {
                let mut name = st.name();
                let nr = ui
                    .add_enabled_ui(!busy, |ui| widgets::text_input(ui, p, &mut name, "", false))
                    .inner;
                if nr.changed() {
                    st.name_edited = true;
                    st.typed_name = name;
                }
                if nr.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && can {
                    body_action = Some(Action::Submit);
                }
            });
            if busy {
                ui.label(
                    egui::RichText::new("Cloning… this can take a minute for a large repository.")
                        .color(p.muted),
                );
            }
            error_line(ui, p, &st.error);
        },
        Some(&mut |ui: &mut Ui| {
            if Button::new(if busy { "Cloning…" } else { "Clone" })
                .kind(Kind::Primary)
                .enabled(can)
                .show(ui, p)
                .clicked()
            {
                action = Some(Action::Submit);
            }
            // A clone can wait minutes on a server that doesn't answer, so it can be left; one
            // that finishes later still lands in the library (see App::on_dialog_msg).
            if Button::new("Cancel").show(ui, p).clicked() {
                action = Some(Action::Close);
            }
        }),
    );
    if r.closed {
        action = Some(Action::Close);
    }
    action.or(body_action)
}

// ── Import from Overleaf ───────────────────────────────────────────────────

pub struct Import {
    pub zip: String,
    pub info: Option<ZipInfo>,
    pub name: String,
    pub parent: String,
    pub busy: bool,
    pub error: Option<String>,
}

impl Import {
    pub fn new(zip: String, parent: String) -> Self {
        Self {
            zip,
            info: None,
            name: String::new(),
            parent,
            busy: false,
            error: None,
        }
    }
    pub fn set_info(&mut self, info: ZipInfo) {
        self.name = info.projects.first().cloned().unwrap_or_default();
        self.info = Some(info);
    }
}

pub fn import(ctx: &egui::Context, p: &Palette, st: &mut Import) -> Option<Action> {
    let mut action = None;
    let mut body_action = None;
    let file_name = st
        .zip
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&st.zip)
        .to_owned();
    let busy = st.busy;
    let bulk = st.info.as_ref().map(|i| i.bulk);
    let can = !st.busy
        && st.info.is_some()
        && !st.parent.is_empty()
        && (bulk == Some(true) || !st.name.trim().is_empty());
    let label = match (&st.info, st.busy) {
        (_, true) => "Importing…".to_owned(),
        (Some(i), _) if i.bulk => format!("Import {} projects", i.projects.len()),
        _ => "Import".into(),
    };
    let r = widgets::modal_with(
        ctx,
        p,
        "Import from Overleaf",
        Some(&file_name),
        520.0,
        |ui| {
            match &st.info {
                None if st.error.is_none() => {
                    ui.label(egui::RichText::new("Reading the zip…").color(p.muted));
                }
                Some(info) if info.bulk => {
                    widgets::Rich::new()
                        .add("This is a download of ", FontId::proportional(13.0), p.ink)
                        .add(
                            &format!("{} projects", info.projects.len()),
                            FontId::new(13.0, theme::font::sans_600()),
                            p.ink,
                        )
                        .add(
                            ". Each one gets its own folder.",
                            FontId::proportional(13.0),
                            p.ink,
                        )
                        .wrap(ui.available_width())
                        .show(ui);
                    let projects = info.projects.clone();
                    egui::Frame::new()
                        .fill(p.surface_2)
                        .stroke(Stroke::new(1.0, p.border))
                        .corner_radius(theme::R)
                        .inner_margin(6)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            egui::ScrollArea::vertical()
                                .max_height(210.0 - 12.0)
                                .show(ui, |ui| {
                                    let col = (ui.available_width() - 8.0) / 2.0;
                                    egui::Grid::new("import-projects")
                                        .spacing([8.0, 2.0])
                                        .min_col_width(col)
                                        .max_col_width(col)
                                        .show(ui, |ui| {
                                            for (i, name) in projects.iter().enumerate() {
                                                ui.horizontal(|ui| {
                                                    ui.set_min_height(26.0);
                                                    ui.add_space(6.0);
                                                    ui.spacing_mut().item_spacing.x = 6.0;
                                                    icons::show(
                                                        ui,
                                                        icons::FILE_ARCHIVE,
                                                        14.0,
                                                        p.muted,
                                                    );
                                                    ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(name)
                                                                .color(p.ink_2),
                                                        )
                                                        .truncate(),
                                                    );
                                                });
                                                if i % 2 == 1 {
                                                    ui.end_row();
                                                }
                                            }
                                        });
                                });
                        });
                }
                Some(_) => {
                    widgets::field(ui, p, "Folder name", |ui| {
                        let nr = ui
                            .add_enabled_ui(!busy, |ui| {
                                widgets::text_input(ui, p, &mut st.name, "", false)
                            })
                            .inner;
                        if nr.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && can {
                            body_action = Some(Action::Submit);
                        }
                    });
                }
                None => {}
            }
            if st.info.is_some() {
                let label = if bulk == Some(true) {
                    "Import into"
                } else {
                    "Location"
                };
                location(ui, p, label, &mut st.parent, !busy, &mut body_action);
            }
            error_line(ui, p, &st.error);
        },
        Some(&mut |ui: &mut Ui| {
            if Button::new(&label)
                .kind(Kind::Primary)
                .enabled(can)
                .show(ui, p)
                .clicked()
            {
                action = Some(Action::Submit);
            }
            if Button::new("Cancel").enabled(!busy).show(ui, p).clicked() {
                action = Some(Action::Close);
            }
        }),
    );
    if r.closed {
        action = Some(Action::Close);
    }
    action.or(body_action)
}

// ── Save as template ───────────────────────────────────────────────────────

/// Asked before a project the user hasn't answered for opens: nothing of it runs until then.
pub struct Trust {
    /// The folder as asked to open.
    pub path: String,
    /// Its canonical form, the key the answer is kept under.
    pub root: String,
}

pub fn trust(ctx: &egui::Context, p: &Palette, st: &Trust) -> Option<Action> {
    let mut action = None;
    let r = widgets::modal_with(
        ctx,
        p,
        "Do you trust the authors of this project?",
        Some(&st.path),
        500.0,
        |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            for text in [
                "A LaTeX project can run programs on your computer: a latexmkrc file is a Perl program, shell escape runs commands, and a Git repository can bring hooks that run when it is used.",
                "Trust projects you made yourself or got from people you trust. A restricted project still builds and shows its PDF, but nothing of its own runs, and the Git panel is off.",
                "You can change this later in the project's settings.",
            ] {
                widgets::paragraph(
                    ui,
                    text,
                    FontId::proportional(13.0),
                    p.ink,
                    1.45,
                    ui.available_width(),
                );
            }
        },
        Some(&mut |ui: &mut Ui| {
            if Button::new("Trust")
                .kind(Kind::Primary)
                .show(ui, p)
                .clicked()
            {
                action = Some(Action::Trust(true));
            }
            if Button::new("Open restricted").show(ui, p).clicked() {
                action = Some(Action::Trust(false));
            }
        }),
    );
    if r.closed {
        action = Some(Action::Close);
    }
    action
}

pub struct SaveTemplate {
    /// The project folder.
    pub path: String,
    pub name: String,
    pub description: String,
    /// Then move the project to the trash.
    pub trash: bool,
    /// The project is open in the editor: it can't be trashed from under it.
    pub open: bool,
    pub busy: bool,
    pub error: Option<String>,
    focused: bool,
}

impl SaveTemplate {
    pub fn new(path: String, project_name: String) -> Self {
        Self {
            path,
            name: project_name,
            description: String::new(),
            trash: false,
            open: false,
            busy: false,
            error: None,
            focused: false,
        }
    }
}

pub fn save_template(ctx: &egui::Context, p: &Palette, st: &mut SaveTemplate) -> Option<Action> {
    let mut action = None;
    let mut body_action = None;
    let busy = st.busy;
    let can = !st.busy && !st.name.trim().is_empty();
    let r = widgets::modal_with(
        ctx,
        p,
        "Save as template",
        Some(
            "New projects can start from a copy of this one: all its files, except build output and history.",
        ),
        480.0,
        |ui| {
            ui.spacing_mut().item_spacing.y = 12.0;
            widgets::field(ui, p, "Name", |ui| {
                let r = ui
                    .add_enabled_ui(!busy, |ui| {
                        widgets::text_input(ui, p, &mut st.name, "", false)
                    })
                    .inner;
                if !st.focused {
                    r.request_focus();
                    st.focused = true;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && can {
                    body_action = Some(Action::Submit);
                }
            });
            widgets::field(ui, p, "Description (optional)", |ui| {
                let r = ui
                    .add_enabled_ui(!busy, |ui| {
                        widgets::text_input(
                            ui,
                            p,
                            &mut st.description,
                            "Exercise sheets with my usual header",
                            false,
                        )
                    })
                    .inner;
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && can {
                    body_action = Some(Action::Submit);
                }
            });
            if !st.open {
                ui.add_enabled_ui(!busy, |ui| {
                    widgets::checkbox(ui, p, &mut st.trash, "Then move the project to the trash");
                });
            }
            widgets::paragraph(
                ui,
                "Remove any text you don't want in every new project before saving. Templates you save appear when you create a new project.",
                FontId::proportional(12.5),
                p.muted,
                1.45,
                ui.available_width(),
            );
            error_line(ui, p, &st.error);
        },
        Some(&mut |ui: &mut Ui| {
            if Button::new("Save template")
                .kind(Kind::Primary)
                .enabled(can)
                .show(ui, p)
                .clicked()
            {
                action = Some(Action::Submit);
            }
            if Button::new("Cancel").enabled(!busy).show(ui, p).clicked() {
                action = Some(Action::Close);
            }
        }),
    );
    if r.closed {
        action = Some(Action::Close);
    }
    action.or(body_action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, name: &str, cat: &str, user: bool) -> Template {
        Template {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            user,
            category: cat.into(),
            tags: vec![],
            preview: None,
        }
    }

    /// The mock backend's templates.
    fn mock_templates() -> Vec<Template> {
        let d = |id: &str, name: &str, desc: &str, cat: &str| Template {
            id: id.into(),
            name: name.into(),
            description: desc.into(),
            user: false,
            category: cat.into(),
            tags: vec![],
            preview: None,
        };
        vec![
            d(
                "article",
                "Article",
                "A paper with sections, maths, figures and a biblatex bibliography.",
                "Papers",
            ),
            d(
                "thesis",
                "Thesis",
                "A report with a title page, chapters in separate files and a bibliography.",
                "Theses & reports",
            ),
            d(
                "slides",
                "Slides",
                "A Beamer presentation with the clean metropolis theme.",
                "Presentations",
            ),
            d(
                "cv",
                "CV",
                "A clean one-page curriculum vitae.",
                "Letters & CV",
            ),
            d(
                "letter",
                "Letter",
                "A formal letter with the KOMA-Script scrlttr2 class.",
                "Letters & CV",
            ),
            d(
                "homework",
                "Homework",
                "Exercise sheets: shaded problem boxes, solutions, and a header with your name and course.",
                "School",
            ),
            d(
                "online:preprint",
                "Preprint",
                "A calm single-column preprint for arXiv or a lab website.",
                "Papers",
            ),
            d(
                "online:lab-report",
                "Lab report",
                "A lab report with measurements, a fitted plot and units.",
                "School",
            ),
            d(
                "online:lecture-notes",
                "Lecture notes",
                "Course notes with boxed definitions and theorems.",
                "School",
            ),
            d(
                "online:poster",
                "Research poster",
                "An A0 conference poster.",
                "Posters",
            ),
            d(
                "online:ieee",
                "IEEE conference",
                "A two-column paper with IEEEtran.",
                "Journals & conferences",
            ),
            d(
                "online:acm",
                "ACM sigconf",
                "A conference paper with acmart.",
                "Journals & conferences",
            ),
            Template {
                user: true,
                ..d("user:course-sheet", "Course sheet", "", "")
            },
        ]
    }

    fn snap(
        name: &str,
        p: Palette,
        mut dialog: impl FnMut(&egui::Context, &Palette) + 'static,
    ) -> egui_kittest::SnapshotResult {
        use crate::testing;
        let mut h = testing::harness(p, move |ui| {
            testing::library_behind(ui, &p);
            dialog(ui.ctx(), &p);
        });
        h.run();
        h.try_snapshot(name)
    }

    #[test]
    fn dialogs_match_the_references() {
        let mut results = egui_kittest::SnapshotResults::new();
        for (p, scheme) in [(theme::LIGHT, "light"), (theme::DARK, "dark")] {
            let mut np = NewProject::new("/home/you/Documents/papers".into(), String::new());
            np.set_templates(mock_templates(), true);
            np.online_loading = false;
            results.add(snap(
                &format!("library-new-project-{scheme}"),
                p,
                move |ctx, p| {
                    new_project(ctx, p, &mut np);
                },
            ));
            let mut np = NewProject::new("/home/you/Documents/papers".into(), String::new());
            np.set_templates(mock_templates(), true);
            np.online_loading = false;
            np.show_cat("Papers");
            results.add(snap(
                &format!("library-new-project-papers-{scheme}"),
                p,
                move |ctx, p| {
                    new_project(ctx, p, &mut np);
                },
            ));
            let mut cl = Clone::new("/home/you/Documents/papers".into(), String::new());
            results.add(snap(
                &format!("library-clone-{scheme}"),
                p,
                move |ctx, p| {
                    clone(ctx, p, &mut cl);
                },
            ));
            let mut im = Import::new(
                "/home/you/Downloads/Overleaf Projects (6 items).zip".into(),
                "/home/you/Documents/papers".into(),
            );
            im.set_info(ZipInfo {
                bulk: true,
                projects: [
                    "CA_Assignement_5",
                    "INF_SEC_Assignement_3",
                    "NET_assignment_4",
                    "Homework",
                    "CA_Assignement_4",
                    "DB_Assignement_3",
                ]
                .map(String::from)
                .to_vec(),
            });
            results.add(snap(
                &format!("library-import-{scheme}"),
                p,
                move |ctx, p| {
                    import(ctx, p, &mut im);
                },
            ));
            let mut sv = SaveTemplate::new(
                "/home/you/Documents/papers/rl-thesis".into(),
                "rl-thesis".into(),
            );
            results.add(snap(
                &format!("library-save-template-{scheme}"),
                p,
                move |ctx, p| {
                    save_template(ctx, p, &mut sv);
                },
            ));
        }
    }

    #[test]
    fn repo_names_come_from_the_url() {
        assert_eq!(repo_name("https://github.com/you/thesis.git"), "thesis");
        assert_eq!(repo_name("git@github.com:you/thesis.git/"), "thesis");
        assert_eq!(repo_name("https://git.overleaf.com/abc123"), "abc123");
        assert_eq!(repo_name(""), "");
        assert!(is_overleaf("https://git:TOKEN@git.overleaf.com/ID"));
    }

    #[test]
    fn clone_dialog_keeps_user_cleared_folder_name() {
        let mut st = Clone::new("/tmp".into(), "https://github.com/you/thesis.git".into());
        assert_eq!(st.name(), "thesis");
        st.name_edited = true;
        st.typed_name.clear();
        assert_eq!(
            st.name(),
            "",
            "clearing folder name must not snap back to repo_name"
        );
    }

    #[test]
    fn categories_follow_the_fixed_order_and_keep_mine_and_publishers_apart() {
        let mut st = NewProject::new("/p".into(), String::new());
        st.set_templates(
            vec![
                t("cv", "CV", "Letters & CV", false),
                t("article", "Article", "Papers", false),
                t("user:x", "X", "", true),
                t("online:ieee", "IEEE", FOLDER, false),
                t("odd", "Odd", "Other", false),
            ],
            true,
        );
        assert_eq!(st.categories(), ["Papers", "Letters & CV", "Other"]);
        assert_eq!(st.chosen, "cv");
        st.show_cat("Papers");
        assert_eq!(st.chosen, "article");
        st.removed("article");
        assert_eq!(st.shown, None);
    }

    #[test]
    fn create_is_possible_on_the_category_screen() {
        let mut st = NewProject::new("/p".into(), String::new());
        st.set_templates(vec![t("article", "Article", "Papers", false)], true);
        st.name = "thesis".into();
        assert!(st.shown.is_none() && st.can_create());
    }

    #[test]
    fn a_linked_template_opens_its_category() {
        let mut st = NewProject::new("/p".into(), "preprint".into());
        st.set_templates(
            vec![
                t("article", "Article", "Papers", false),
                t("online:preprint", "Preprint", "Papers", false),
            ],
            true,
        );
        assert_eq!(
            (st.chosen.as_str(), st.shown.as_deref()),
            ("online:preprint", Some("Papers"))
        );
    }
}
