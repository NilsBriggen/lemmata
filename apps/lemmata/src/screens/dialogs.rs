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
    /// Choose the folder the project goes in.
    ChooseParent,
    Submit,
    DeleteTemplate(String),
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
    /// The category whose templates are shown; None shows the categories.
    pub shown: Option<String>,
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
            && self.shown.is_some()
            && !self.name.trim().is_empty()
            && !self.parent.is_empty()
    }
}

/// "Article, Preprint, Two-column paper, …".
fn summary(list: &[&Template]) -> String {
    let names: Vec<&str> = list.iter().take(3).map(|t| t.name.as_str()).collect();
    names.join(", ") + if list.len() > 3 { ", …" } else { "" }
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
        s.format.line_height = Some(12.0 * 1.4);
    }
    ui.fonts_mut(|f| f.layout_job(job))
}

pub fn new_project(ctx: &egui::Context, p: &Palette, st: &mut NewProject) -> Option<Action> {
    let mut action = None;
    let mut body_action = None;
    let can = st.can_create();
    let busy = st.busy;
    let r = widgets::modal_with(
        ctx,
        p,
        "New project",
        Some("Start from a template. It's a normal folder you can open in any editor."),
        720.0,
        |ui| {
            ui.spacing_mut().item_spacing.y = 16.0;
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                match st.shown.clone() {
                    None => gallery_categories(ui, p, st),
                    Some(c) => gallery_templates(ui, p, st, &c, &mut body_action),
                }
            });
            widgets::field(ui, p, "Folder name", |ui| {
                let r = widgets::text_input(ui, p, &mut st.name, "master-thesis", false);
                if !st.focused {
                    r.request_focus();
                    st.focused = true;
                }
                if r.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && st.can_create()
                {
                    body_action = Some(Action::Submit);
                }
            });
            location(ui, p, "Location", &mut st.parent, true, &mut body_action);
            error_line(ui, p, &st.error);
        },
        Some(&mut |ui: &mut Ui| {
            if Button::new("Create project")
                .kind(Kind::Primary)
                .enabled(can && !busy)
                .show(ui, p)
                .clicked()
            {
                action = Some(Action::Submit);
            }
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

fn gallery_categories(ui: &mut Ui, p: &Palette, st: &mut NewProject) {
    let cats = st.categories();
    // grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 8px
    let w = ui.available_width() - 4.0;
    let cols = (((w + 8.0) / 208.0).floor() as usize).max(1);
    let cw = (w - 8.0 * (cols as f32 - 1.0)) / cols as f32;
    let mut clicked = None;
    for row in cats.chunks(cols) {
        let descs: Vec<_> = row
            .iter()
            .map(|c| {
                clamped(
                    ui,
                    &summary(&st.in_cat(c)),
                    FontId::proportional(12.0),
                    p.muted,
                    cw - 28.0,
                    3,
                )
            })
            .collect();
        let h = 14.0
            + 20.0
            + 4.0
            + 4.0
            + 18.0
            + 4.0
            + descs.iter().map(|g| g.size().y).fold(0.0, f32::max)
            + 14.0;
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            ui.spacing_mut().item_spacing.x = 8.0;
            for (c, desc) in row.iter().zip(descs) {
                let (rect, r) = ui.allocate_exact_size(Vec2::new(cw, h), Sense::click());
                r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, c));
                let t = ui
                    .ctx()
                    .animate_bool_with_time(r.id, r.hovered(), theme::EASE_SECS);
                let painter = ui.painter();
                painter.rect(
                    rect,
                    CornerRadius::same(theme::R_LG),
                    p.surface,
                    Stroke::new(1.0, p.border.lerp_to_gamma(p.accent, t)),
                    StrokeKind::Inside,
                );
                icons::paint(
                    painter,
                    Rect::from_min_size(rect.min + Vec2::splat(14.0), Vec2::splat(20.0)),
                    category_icon(c),
                    p.accent,
                );
                painter.text(
                    rect.min + Vec2::new(14.0, 14.0 + 28.0),
                    Align2::LEFT_TOP,
                    c,
                    FontId::new(13.0, theme::font::sans_600()),
                    p.ink,
                );
                painter.galley(
                    rect.min + Vec2::new(14.0, 14.0 + 28.0 + 22.0),
                    desc,
                    p.muted,
                );
                widgets::focus_ring(ui, &r, rect, p, theme::R_LG);
                if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    clicked = Some(c.clone());
                }
            }
        });
    }
    let mine = st.in_cat(MINE);
    if !mine.is_empty() && folder_bar(ui, p, icons::STAR, MINE, &summary(&mine)).clicked() {
        clicked = Some(MINE.into());
    }
    let n = st.in_cat(FOLDER).len();
    if n > 0 {
        let text = format!(
            "IEEE, ACM, Springer, Elsevier and more: {n} templates with a publisher's own class"
        );
        if folder_bar(ui, p, icons::FOLDER_OPEN, FOLDER, &text).clicked() {
            clicked = Some(FOLDER.into());
        }
    }
    if st.online_loading {
        ui.label(
            egui::RichText::new("Getting more templates…")
                .size(12.0)
                .color(p.muted),
        );
    } else if let Some(e) = &st.online_error {
        ui.label(
            egui::RichText::new(
                "More templates are online, but they couldn't be loaded right now.",
            )
            .size(12.0)
            .color(p.muted),
        )
        .on_hover_text(e);
    }
    if let Some(c) = clicked {
        st.show_cat(&c);
    }
}

/// `.folder`: a full-width bar with an accent icon, a title and a one-line description.
fn folder_bar(ui: &mut Ui, p: &Palette, icon: Icon, title: &str, desc: &str) -> egui::Response {
    let w = ui.available_width() - 4.0;
    let (rect, r) =
        ui.allocate_exact_size(Vec2::new(w, 36.0 + 2.0 * 0.0 + 16.0 - 2.0), Sense::click());
    let rect = rect.translate(Vec2::new(2.0, 0.0));
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(theme::R_LG),
        p.surface,
        Stroke::new(
            1.0,
            if r.hovered() {
                p.border_strong
            } else {
                p.border
            },
        ),
        StrokeKind::Inside,
    );
    icons::paint(
        painter,
        Rect::from_center_size(
            pos2(rect.left() + 12.0 + 9.0, rect.center().y),
            Vec2::splat(18.0),
        ),
        icon,
        p.accent,
    );
    painter.text(
        pos2(rect.left() + 40.0, rect.top() + 8.0),
        Align2::LEFT_TOP,
        title,
        FontId::new(13.0, theme::font::sans_600()),
        p.ink,
    );
    painter.text(
        pos2(rect.left() + 40.0, rect.top() + 8.0 + 18.0),
        Align2::LEFT_TOP,
        desc,
        FontId::proportional(12.0),
        p.muted,
    );
    widgets::focus_ring(ui, &r, rect, p, theme::R_LG);
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
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

fn gallery_templates(
    ui: &mut Ui,
    p: &Palette,
    st: &mut NewProject,
    cat: &str,
    action: &mut Option<Action>,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let back = ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let (rect, r) =
                ui.allocate_exact_size(Vec2::new(12.0 + 14.0 + 4.0 + 84.0, 22.0), Sense::click());
            if r.hovered() {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(theme::R_SM), p.surface_2);
            }
            icons::paint(
                ui.painter(),
                Rect::from_min_size(
                    pos2(rect.left() + 6.0, rect.center().y - 7.0),
                    Vec2::splat(14.0),
                ),
                icons::ARROW_LEFT,
                p.accent,
            );
            ui.painter().text(
                pos2(rect.left() + 24.0, rect.center().y),
                Align2::LEFT_CENTER,
                "All categories",
                FontId::proportional(13.0),
                p.accent,
            );
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "All categories"));
            r
        });
        if back.inner.clicked() {
            st.shown = None;
        }
        ui.label(
            egui::RichText::new(cat)
                .font(FontId::new(14.0, theme::font::sans_600()))
                .color(p.ink),
        );
    });
    if cat == FOLDER {
        ui.label(egui::RichText::new("Templates with a publisher's own class, for submitting to their journals and conferences.").color(p.muted));
    }
    // grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 10px; padding: 2px
    let w = ui.available_width() - 4.0;
    let cols = (((w + 10.0) / 160.0).floor() as usize).max(1);
    let cw = (w - 10.0 * (cols as f32 - 1.0)) / cols as f32;
    let list: Vec<Template> = st.in_cat(cat).into_iter().cloned().collect();
    for row in list.chunks(cols) {
        let descs: Vec<_> = row
            .iter()
            .map(|t| {
                let d = if t.description.is_empty() && t.user {
                    "Your template"
                } else {
                    &t.description
                };
                clamped(ui, d, FontId::proportional(12.0), p.muted, cw - 16.0, 3)
            })
            .collect();
        let h = 8.0
            + 92.0
            + 4.0
            + 4.0
            + 18.0
            + 4.0
            + descs.iter().map(|g| g.size().y).fold(0.0, f32::max)
            + 8.0;
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            ui.spacing_mut().item_spacing.x = 10.0;
            for (t, desc) in row.iter().zip(descs) {
                let (rect, r) = ui.allocate_exact_size(Vec2::new(cw, h), Sense::click());
                let on = st.chosen == t.id;
                r.widget_info(|| {
                    egui::WidgetInfo::selected(egui::Role::RadioButton, true, on, &t.name)
                });
                let painter = ui.painter();
                if on {
                    painter.rect_stroke(
                        rect.expand(1.5),
                        CornerRadius::same(theme::R_LG + 2),
                        Stroke::new(3.0, Palette::mix(p.accent, 18.0)),
                        StrokeKind::Outside,
                    );
                }
                let border = if on {
                    p.accent
                } else if r.hovered() {
                    p.border_strong
                } else {
                    p.border
                };
                painter.rect(
                    rect,
                    CornerRadius::same(theme::R_LG),
                    p.surface,
                    Stroke::new(1.0, border),
                    StrokeKind::Inside,
                );
                let thumb =
                    Rect::from_min_size(rect.min + Vec2::splat(8.0), Vec2::new(cw - 16.0, 92.0));
                painter.rect_filled(thumb, CornerRadius::same(theme::R_SM), p.surface_2);
                match preview(ui.ctx(), &mut st.previews, t) {
                    Some(tex) => {
                        // object-fit: cover; object-position: top, on white.
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
                    None => icons::paint(
                        painter,
                        Rect::from_center_size(thumb.center(), Vec2::splat(28.0)),
                        if t.user {
                            icons::STAR
                        } else {
                            icons::FILE_TEXT
                        },
                        p.faint,
                    ),
                }
                let y = thumb.bottom() + 8.0;
                painter.text(
                    pos2(rect.left() + 8.0, y),
                    Align2::LEFT_TOP,
                    &t.name,
                    FontId::new(13.0, theme::font::sans_600()),
                    p.ink,
                );
                painter.galley(pos2(rect.left() + 8.0, y + 22.0), desc, p.muted);
                widgets::focus_ring(ui, &r, rect, p, theme::R_LG);
                if r.clicked() {
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
}

// ── Clone ──────────────────────────────────────────────────────────────────

pub struct Clone {
    pub url: String,
    /// Typed by the user; while empty, the name follows the URL.
    pub typed_name: String,
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
            parent,
            busy: false,
            error: None,
            focused: false,
        }
    }
    pub fn name(&self) -> String {
        if self.typed_name.is_empty() {
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
                if ui
                    .add_enabled_ui(!busy, |ui| widgets::text_input(ui, p, &mut name, "", false))
                    .inner
                    .changed()
                {
                    st.typed_name = name;
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
                        widgets::text_input(ui, p, &mut st.name, "", false)
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
                location(ui, p, label, &mut st.parent, true, &mut body_action);
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

// ── Save as template ───────────────────────────────────────────────────────

pub struct SaveTemplate {
    /// The project folder.
    pub path: String,
    pub name: String,
    pub description: String,
    /// Then move the project to the trash.
    pub trash: bool,
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
            busy: false,
            error: None,
            focused: false,
        }
    }
}

pub fn save_template(ctx: &egui::Context, p: &Palette, st: &mut SaveTemplate) -> Option<Action> {
    let mut action = None;
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
                let r = widgets::text_input(ui, p, &mut st.name, "", false);
                if !st.focused {
                    r.request_focus();
                    st.focused = true;
                }
            });
            widgets::field(ui, p, "Description (optional)", |ui| {
                widgets::text_input(
                    ui,
                    p,
                    &mut st.description,
                    "Exercise sheets with my usual header",
                    false,
                )
            });
            widgets::checkbox(ui, p, &mut st.trash, "Then move the project to the trash");
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
            if Button::new("Cancel").show(ui, p).clicked() {
                action = Some(Action::Close);
            }
        }),
    );
    if r.closed {
        action = Some(Action::Close);
    }
    action
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
