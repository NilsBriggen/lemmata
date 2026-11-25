//! Unified Settings window: app-wide preferences (Lemmata scope) and per-project settings
//! (`lemmata.toml`, Project scope), plus the user's snippets sub-dialog.

use crate::icons::{self, Icon};
use crate::store::{Settings, Snippet};
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind, Size};
use crate::workspace::project_settings::{self, ProjectSection, ProjectSettings};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Margin, Rect, RichText, Sense, Stroke, StrokeKind,
    Ui, pos2, vec2,
};
use lemmata_spell::Available;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    App,
    Project,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AppSection {
    #[default]
    Appearance,
    Editing,
    Language,
    Files,
    Tex,
    Updates,
}

impl AppSection {
    const ALL: [Self; 6] = [
        Self::Appearance,
        Self::Editing,
        Self::Language,
        Self::Files,
        Self::Tex,
        Self::Updates,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Editing => "Editing",
            Self::Language => "Language",
            Self::Files => "Files",
            Self::Tex => "TeX",
            Self::Updates => "Updates",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::Appearance => icons::SUN,
            Self::Editing => icons::SQUARE_PEN,
            Self::Language => icons::SPARKLES,
            Self::Files => icons::FILES,
            Self::Tex => icons::SIGMA,
            Self::Updates => icons::DOWNLOAD,
        }
    }
}

pub struct Prefs {
    pub scope: Scope,
    pub app_section: AppSection,
    pub project_section: ProjectSection,
    /// Shown when switching from dirty Project scope to Lemmata scope.
    pub unsaved_prompt: bool,
    /// Close the window once an async project settings save succeeds; false when "Save & switch".
    pub close_after_project_save: bool,
    /// Spell-check languages; the ones not installed are fetched when chosen.
    pub dicts: Vec<Available>,
    pub downloading: Option<String>,
    pub spell_error: Option<String>,
    pub grammar_status: Option<String>,
    pub snippets: Option<Vec<Snippet>>,
    /// Opened for the snippets alone (the palette's "Edit snippets…"): closing them closes all.
    pub only_snippets: bool,
    pub checking_updates: bool,
    pub update_message: Option<String>,
    grammar_url_dirty: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            scope: Scope::App,
            app_section: AppSection::Appearance,
            project_section: ProjectSection::Build,
            unsaved_prompt: false,
            close_after_project_save: true,
            dicts: Vec::new(),
            downloading: None,
            spell_error: None,
            grammar_status: None,
            snippets: None,
            only_snippets: false,
            checking_updates: false,
            update_message: None,
            grammar_url_dirty: false,
        }
    }
}

/// Per-project context passed when a workspace is open.
pub struct ProjectCtx<'a> {
    pub name: &'a str,
    pub settings: Option<&'a mut ProjectSettings>,
    pub candidates: &'a [String],
    pub trusted: bool,
    pub was_shell_escape: bool,
}

pub enum Out {
    /// A Lemmata-wide setting changed: save it.
    Changed,
    Close,
    /// Flush pending edits (such as grammar_url) and close.
    CloseAndSave,
    ChooseTex,
    AutoTex,
    Download(String),
    TestGrammar,
    CheckUpdates,
    /// User clicked the project scope tab while no project working copy was open yet.
    OpenProjectScope,
    /// User clicked Save (or Save & switch) for the project working copy.
    SaveProject,
    /// User switched from Project scope back to Lemmata scope (clean or discarding edits).
    DiscardProjectAndSwitchApp,
}

const INDENTS: [(&str, &str); 4] = [
    ("2", "2 spaces"),
    ("4", "4 spaces"),
    ("8", "8 spaces"),
    ("tab", "Tab"),
];

/// `.setting`: left title + description, right-aligned control (stacks on narrow widths), with a
/// subtle bottom hairline unless `last` is true.
fn setting_row<R>(
    ui: &mut Ui,
    p: &Palette,
    title: &str,
    desc: Option<&str>,
    last: bool,
    control: impl FnOnce(&mut Ui) -> R,
) -> R {
    let w = ui.available_width();
    let stack = w < 460.0;
    let out = egui::Frame::new()
        .inner_margin(Margin::symmetric(0, 9))
        .show(ui, |ui| {
            ui.set_width(w);
            if stack {
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.label(
                    RichText::new(title)
                        .font(FontId::new(13.5, theme::font::sans_500()))
                        .color(p.ink),
                );
                if let Some(d) = desc {
                    ui.add(egui::Label::new(RichText::new(d).size(12.5).color(p.muted)).wrap());
                }
                ui.horizontal(|ui| control(ui)).inner
            } else {
                let ctl_w = (w * 0.39).clamp(190.0, 250.0);
                let label_w = (w - ctl_w - 20.0).max(140.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 20.0;
                    ui.allocate_ui_with_layout(
                        vec2(label_w, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(label_w);
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.label(
                                RichText::new(title)
                                    .font(FontId::new(13.5, theme::font::sans_500()))
                                    .color(p.ink),
                            );
                            if let Some(d) = desc {
                                ui.add(
                                    egui::Label::new(RichText::new(d).size(12.5).color(p.muted))
                                        .wrap(),
                                );
                            }
                        },
                    );
                    ui.allocate_ui_with_layout(
                        vec2(ctl_w, 32.0),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| control(ui),
                    )
                    .inner
                })
                .inner
            }
        })
        .inner;
    if !last {
        let y = ui.cursor().top();
        ui.painter()
            .hline(ui.max_rect().x_range(), y, Stroke::new(1.0, p.border));
    }
    out
}

fn setting_switch(
    ui: &mut Ui,
    p: &Palette,
    title: &str,
    desc: Option<&str>,
    value: &mut bool,
    a11y_label: &str,
    last: bool,
) -> bool {
    setting_row(ui, p, title, desc, last, |ui| {
        widgets::switch(ui, p, value, a11y_label).clicked()
    })
}

#[allow(clippy::too_many_arguments)]
fn setting_select(
    ui: &mut Ui,
    p: &Palette,
    title: &str,
    desc: Option<&str>,
    a11y_label: &str,
    value: &mut String,
    options: &[(&str, &str)],
    last: bool,
) -> bool {
    setting_row(ui, p, title, desc, last, |ui| {
        ui.allocate_ui(vec2(ui.available_width(), 32.0), |ui| {
            widgets::select(ui, p, a11y_label, value, options)
        })
        .inner
    })
}

/// Equal-width two-option scope switcher (`Lemmata` | `<project>`) matching `.scope`.
fn scope_switch(ui: &mut Ui, p: &Palette, project_name: &str, scope: Scope) -> Option<Scope> {
    let w = ui.available_width();
    let (track, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(track, CornerRadius::same(9), p.surface_3);
    let btn_w = ((w - 8.0) / 2.0).max(40.0);
    let mut picked = None;
    for (i, (sc, text)) in [(Scope::App, "Lemmata"), (Scope::Project, project_name)]
        .into_iter()
        .enumerate()
    {
        let rect = Rect::from_min_size(
            pos2(
                track.left() + 3.0 + i as f32 * (btn_w + 2.0),
                track.top() + 3.0,
            ),
            vec2(btn_w, 26.0),
        );
        let on = scope == sc;
        let r = ui.interact(rect, ui.scope_id().with(("scope", i)), Sense::click());
        r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, text));
        let hovered = r.hovered();
        if on {
            painter.add(p.shadow_card.as_shape(rect, CornerRadius::same(theme::R)));
            painter.rect(
                rect,
                CornerRadius::same(theme::R),
                p.surface,
                Stroke::new(1.0, p.card_hairline),
                StrokeKind::Inside,
            );
        }
        let ink = if on || hovered { p.ink } else { p.muted };
        let mut job = egui::text::LayoutJob::simple_singleline(
            text.to_owned(),
            FontId::new(12.0, theme::font::sans_500()),
            ink,
        );
        job.wrap.max_width = (rect.width() - 12.0).max(16.0);
        job.wrap.max_rows = 1;
        job.wrap.overflow_character = Some('…');
        let g = ui.fonts_mut(|f| f.layout_job(job));
        painter.galley(rect.center() - g.size() / 2.0, g, ink);
        widgets::focus_ring(ui, &r, rect, p, theme::R);
        if r.on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(text)
            .clicked()
        {
            picked = Some(sc);
        }
    }
    picked
}

pub fn show(
    ctx: &egui::Context,
    p: &Palette,
    st: &mut Prefs,
    s: &mut Settings,
    tex: Option<&lemmata_shell::TexStatus>,
    system_dark: bool,
    mut project: Option<ProjectCtx<'_>>,
) -> Option<Out> {
    if st.snippets.is_some() {
        return snippets(ctx, p, st, s);
    }
    if st.only_snippets {
        return Some(Out::Close);
    }
    if project.is_none() {
        st.scope = Scope::App;
        st.unsaved_prompt = false;
    }

    let project_busy = project
        .as_ref()
        .and_then(|pr| pr.settings.as_deref())
        .is_some_and(|ps| ps.busy);
    let project_valid = project
        .as_ref()
        .and_then(|pr| pr.settings.as_deref())
        .is_none_or(|ps| ps.is_valid());
    let project_dirty = project
        .as_ref()
        .and_then(|pr| pr.settings.as_deref())
        .is_some_and(|ps| ps.is_dirty());
    let in_project_scope = st.scope == Scope::Project && project.is_some();
    // Preserve modal_editing safety on Project scope: clicking the scrim never discards edits.
    let backdrop_closes = !in_project_scope;

    let screen = ctx.content_rect();
    let w = 920.0f32.min(screen.width() - 40.0).max(320.0);
    let h = 660.0f32.min(screen.height() - 40.0).max(360.0);
    let compact = w < 740.0;

    let previous_focus = widgets::modal_focus_enter(ctx, "settings-modal");
    let any_popup_open = egui::Popup::is_any_open(ctx);
    let backdrop = egui::Area::new(egui::Id::unique("settings-backdrop"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, r) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter()
                .rect_filled(rect, 0, Color32::from_rgba_unmultiplied(12, 17, 22, 82));
            r
        });

    let mut out: Option<Out> = None;
    let mut close_requested = false;

    let section_title = match st.scope {
        Scope::App => st.app_section.title(),
        Scope::Project => match st.project_section {
            ProjectSection::Build => "Build",
            ProjectSection::Trust => "Trust & safety",
        },
    };
    let subtitle = match st.scope {
        Scope::App => "These apply to every project.".to_owned(),
        Scope::Project => format!(
            "Only for {}.",
            project.as_ref().map_or("this project", |pr| pr.name)
        ),
    };

    let area = egui::Area::new(egui::Id::unique("settings-modal"))
        .kind(egui::UiKind::Modal)
        .order(egui::Order::Foreground)
        .pivot(Align2::CENTER_CENTER)
        .fixed_pos(screen.center());
    let layer = area.layer();
    ctx.memory_mut(|m| m.set_modal_layer(layer));
    let panel = area.show(ctx, |ui| {
        let (outer, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
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
        let nav_w = 224.0;
        let (nav_rect, main_rect) = if compact {
            let nav_h = if project.is_some() { 94.0 } else { 52.0 };
            (
                Rect::from_min_size(inner.min, vec2(inner.width(), nav_h)),
                Rect::from_min_max(pos2(inner.left(), inner.top() + nav_h), inner.max),
            )
        } else {
            (
                Rect::from_min_size(inner.min, vec2(nav_w, inner.height())),
                Rect::from_min_max(pos2(inner.left() + nav_w, inner.top()), inner.max),
            )
        };

        // Sidebar nav surface (`surface_2`).
        let nav_rad = if compact {
            CornerRadius {
                nw: theme::R_XL - 1,
                ne: theme::R_XL - 1,
                sw: 0,
                se: 0,
            }
        } else {
            CornerRadius {
                nw: theme::R_XL - 1,
                ne: 0,
                sw: theme::R_XL - 1,
                se: 0,
            }
        };
        ui.painter().rect_filled(nav_rect, nav_rad, p.surface_2);
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

        // Left/top navigation content.
        let nav_inner = nav_rect.shrink2(if compact {
            vec2(12.0, 10.0)
        } else {
            vec2(12.0, 16.0)
        });
        let mut nav = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(nav_inner)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        if !compact {
            nav.horizontal(|ui| {
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Settings")
                        .font(FontId::new(20.0, theme::font::display()))
                        .color(p.ink),
                );
            });
            nav.add_space(10.0);
        }
        if let Some(pr) = project.as_ref() {
            let picked = nav
                .add_enabled_ui(!project_busy, |ui| scope_switch(ui, p, pr.name, st.scope))
                .inner;
            if let Some(picked) = picked
                && picked != st.scope
            {
                match picked {
                    Scope::Project => {
                        st.unsaved_prompt = false;
                        if pr.settings.is_some() {
                            st.scope = Scope::Project;
                        } else {
                            out = Some(Out::OpenProjectScope);
                        }
                    }
                    Scope::App => {
                        if project_dirty {
                            st.unsaved_prompt = true;
                        } else {
                            st.scope = Scope::App;
                            st.unsaved_prompt = false;
                            out = Some(Out::DiscardProjectAndSwitchApp);
                        }
                    }
                }
            }
            nav.add_space(if compact { 8.0 } else { 12.0 });
        }
        if compact {
            egui::ScrollArea::horizontal()
                .id_salt("settings-nav-h")
                .show(&mut nav, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        match st.scope {
                            Scope::App => {
                                for sec in AppSection::ALL {
                                    if Button::new(sec.title())
                                        .with_icon(sec.icon())
                                        .kind(Kind::Ghost)
                                        .size(Size::Sm)
                                        .on(st.app_section == sec)
                                        .show(ui, p)
                                        .clicked()
                                    {
                                        st.app_section = sec;
                                    }
                                }
                            }
                            Scope::Project => {
                                for (sec, icon, name) in [
                                    (ProjectSection::Build, icons::REFRESH_CW, "Build"),
                                    (ProjectSection::Trust, icons::FLAG, "Trust & safety"),
                                ] {
                                    if Button::new(name)
                                        .with_icon(icon)
                                        .kind(Kind::Ghost)
                                        .size(Size::Sm)
                                        .on(st.project_section == sec)
                                        .show(ui, p)
                                        .clicked()
                                    {
                                        st.project_section = sec;
                                    }
                                }
                            }
                        }
                    });
                });
        } else {
            nav.spacing_mut().item_spacing.y = 2.0;
            match st.scope {
                Scope::App => {
                    for sec in AppSection::ALL {
                        if widgets::side_row(
                            &mut nav,
                            p,
                            Some(sec.icon()),
                            None,
                            sec.title(),
                            st.app_section == sec,
                            None,
                        )
                        .clicked()
                        {
                            st.app_section = sec;
                        }
                    }
                }
                Scope::Project => {
                    for (sec, icon, name) in [
                        (ProjectSection::Build, icons::REFRESH_CW, "Build"),
                        (ProjectSection::Trust, icons::FLAG, "Trust & safety"),
                    ] {
                        if widgets::side_row(
                            &mut nav,
                            p,
                            Some(icon),
                            None,
                            name,
                            st.project_section == sec,
                            None,
                        )
                        .clicked()
                        {
                            st.project_section = sec;
                        }
                    }
                }
            }
            // Version footer pinned at bottom of nav rail.
            let ver_rect = Rect::from_min_max(
                pos2(nav_inner.left() + 8.0, nav_inner.bottom() - 22.0),
                nav_inner.max,
            );
            ui.painter().text(
                pos2(ver_rect.left(), ver_rect.center().y),
                Align2::LEFT_CENTER,
                format!("Lemmata {}", env!("CARGO_PKG_VERSION")),
                FontId::proportional(12.0),
                p.faint,
            );
        }

        // Right main pane: header, scrollable body, footer.
        let head_h = 64.0;
        let foot_h = 52.0;
        let head_rect = Rect::from_min_size(main_rect.min, vec2(main_rect.width(), head_h));
        let foot_rect = Rect::from_min_max(
            pos2(main_rect.left(), main_rect.bottom() - foot_h),
            main_rect.max,
        );
        let body_rect = Rect::from_min_max(
            pos2(main_rect.left(), head_rect.bottom()),
            pos2(main_rect.right(), foot_rect.top()),
        );

        // Header (`s-head`).
        let mut head = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    pos2(head_rect.left() + 26.0, head_rect.top() + 18.0),
                    pos2(head_rect.right() - 18.0, head_rect.bottom() - 8.0),
                ))
                .layout(egui::Layout::left_to_right(egui::Align::Min)),
        );
        head.vertical(|ui| {
            ui.set_width((ui.available_width() - 40.0).max(120.0));
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(
                RichText::new(section_title)
                    .font(FontId::new(17.0, theme::font::sans_600()))
                    .color(p.ink),
            );
            ui.label(RichText::new(&subtitle).size(13.0).color(p.muted));
        });
        head.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            if Button::icon(icons::X, "Close")
                .enabled(!project_busy)
                .show_icon(ui, p)
                .clicked()
            {
                close_requested = true;
            }
        });

        // Scrollable body (`s-body`).
        let body_pad = Rect::from_min_max(
            pos2(body_rect.left() + 26.0, body_rect.top()),
            pos2(body_rect.right() - 18.0, body_rect.bottom()),
        );
        let mut body_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(body_pad)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        egui::ScrollArea::vertical()
            .id_salt((
                "settings-body",
                st.scope as u8,
                st.app_section as u8,
                st.project_section as u8,
            ))
            .auto_shrink([false, false])
            .show(&mut body_ui, |ui| {
                let content_w = (ui.available_width() - 8.0).max(160.0);
                ui.set_width(content_w);
                ui.spacing_mut().item_spacing.y = 0.0;

                if !project_dirty {
                    st.unsaved_prompt = false;
                }
                if st.unsaved_prompt && in_project_scope && project_dirty {
                    egui::Frame::new()
                        .fill(p.warn_soft)
                        .stroke(Stroke::new(1.0, p.warn.gamma_multiply(0.35)))
                        .corner_radius(theme::R_MD)
                        .inner_margin(Margin::symmetric(12, 10))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                                icons::show(ui, icons::TRIANGLE_ALERT, 15.0, p.warn);
                                ui.label(
                                    RichText::new("You have unsaved project settings.")
                                        .font(FontId::new(12.5, theme::font::sans_500()))
                                        .color(p.ink),
                                );
                                if Button::new("Save & switch")
                                    .kind(Kind::Primary)
                                    .size(Size::Sm)
                                    .enabled(project_valid && !project_busy)
                                    .show(ui, p)
                                    .clicked()
                                {
                                    st.close_after_project_save = false;
                                    out = Some(Out::SaveProject);
                                }
                                if Button::new("Discard & switch")
                                    .kind(Kind::Ghost)
                                    .size(Size::Sm)
                                    .enabled(!project_busy)
                                    .show(ui, p)
                                    .clicked()
                                {
                                    st.scope = Scope::App;
                                    st.unsaved_prompt = false;
                                    out = Some(Out::DiscardProjectAndSwitchApp);
                                }
                            });
                        });
                    ui.add_space(10.0);
                }

                match st.scope {
                    Scope::App => {
                        show_app_section(ui, p, st, s, tex, system_dark, &mut out);
                    }
                    Scope::Project => {
                        ui.spacing_mut().item_spacing.y = 14.0;
                        if let Some(pr) = project.as_mut()
                            && let Some(ps) = pr.settings.as_deref_mut()
                        {
                            if let Some(err_msg) = ps.error.clone() {
                                egui::Frame::new()
                                    .fill(p.error_soft)
                                    .stroke(Stroke::new(1.0, p.error.gamma_multiply(0.35)))
                                    .corner_radius(theme::R_MD)
                                    .inner_margin(Margin::symmetric(12, 10))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.horizontal_wrapped(|ui| {
                                            ui.spacing_mut().item_spacing = vec2(8.0, 4.0);
                                            icons::show(ui, icons::CIRCLE_ALERT, 15.0, p.error);
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(err_msg)
                                                        .size(12.5)
                                                        .color(p.error),
                                                )
                                                .wrap(),
                                            );
                                        });
                                    });
                            }
                            ui.add_enabled_ui(!project_busy, |ui| match st.project_section {
                                ProjectSection::Build => {
                                    project_settings::show_build_section(ui, p, ps, pr.candidates)
                                }
                                ProjectSection::Trust => project_settings::show_trust_section(
                                    ui,
                                    p,
                                    ps,
                                    pr.trusted,
                                    pr.was_shell_escape,
                                ),
                            });
                        }
                    }
                }
                ui.add_space(16.0);
            });

        // Footer (`s-foot`).
        ui.painter().hline(
            foot_rect.x_range(),
            foot_rect.top() + 0.5,
            Stroke::new(1.0, p.border),
        );
        let mut foot = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_max(
                    pos2(foot_rect.left() + 26.0, foot_rect.top()),
                    pos2(foot_rect.right() - 20.0, foot_rect.bottom()),
                ))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        foot.spacing_mut().item_spacing.x = 8.0;
        if in_project_scope {
            let save_lbl = if project_busy { "Saving…" } else { "Save" };
            if Button::new(save_lbl)
                .kind(Kind::Primary)
                .enabled(project_valid && !project_busy)
                .show(&mut foot, p)
                .clicked()
            {
                st.close_after_project_save = true;
                out = Some(Out::SaveProject);
            }
            if Button::new("Cancel")
                .kind(Kind::Ghost)
                .enabled(!project_busy)
                .show(&mut foot, p)
                .clicked()
            {
                close_requested = true;
            }
            foot.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new("Saved in lemmata.toml, so they travel with the project.")
                            .size(12.5)
                            .color(p.faint),
                    )
                    .truncate(),
                );
            });
        } else {
            if Button::new("Done")
                .kind(Kind::Primary)
                .show(&mut foot, p)
                .clicked()
            {
                close_requested = true;
            }
            foot.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new("Changes apply at once.")
                        .size(12.5)
                        .color(p.faint),
                );
            });
        }
    });

    panel
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Dialog, true, "Settings"));

    if backdrop_closes
        && !project_busy
        && !any_popup_open
        && backdrop.inner.clicked()
        && !panel
            .response
            .rect
            .contains(backdrop.inner.interact_pointer_pos().unwrap_or_default())
    {
        close_requested = true;
    }
    if !project_busy
        && ctx.memory(|m| m.top_modal_layer() == Some(layer))
        && !any_popup_open
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        close_requested = true;
    }

    if close_requested {
        widgets::modal_focus_restore(ctx, "settings-modal", previous_focus);
        if st.grammar_url_dirty || matches!(out, Some(Out::Changed)) {
            s.grammar_url = s.grammar_url.trim().to_owned();
            st.grammar_url_dirty = false;
            return Some(Out::CloseAndSave);
        }
        return Some(Out::Close);
    }
    out
}

fn show_app_section(
    ui: &mut Ui,
    p: &Palette,
    st: &mut Prefs,
    s: &mut Settings,
    tex: Option<&lemmata_shell::TexStatus>,
    system_dark: bool,
    out: &mut Option<Out>,
) {
    let mut changed = false;
    match st.app_section {
        AppSection::Appearance => {
            setting_row(ui, p, "Theme", None, false, |ui| {
                let opts = [
                    (Some(icons::MONITOR), "System"),
                    (Some(icons::SUN), "Light"),
                    (Some(icons::MOON), "Dark"),
                ];
                let sel = ["system", "light", "dark"]
                    .iter()
                    .position(|t| *t == s.theme)
                    .unwrap_or(0);
                if let Some(i) = widgets::segmented(ui, p, &opts, sel, false) {
                    s.theme = ["system", "light", "dark"][i].into();
                    changed = true;
                }
            });
            let mut fonts: Vec<(String, String)> = vec![
                ("plex".into(), "IBM Plex Mono".into()),
                ("system".into(), "System monospace".into()),
            ];
            if !["plex", "system"].contains(&s.font_family.as_str()) {
                fonts.push((s.font_family.clone(), s.font_family.clone()));
            }
            for f in [
                "JetBrains Mono",
                "Fira Code",
                "Cascadia Code",
                "Source Code Pro",
                "Menlo",
                "Consolas",
            ] {
                if f != s.font_family {
                    fonts.push((f.into(), format!("{f} (if installed)")));
                }
            }
            let opts: Vec<(&str, &str)> = fonts
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            changed |= setting_select(
                ui,
                p,
                "Editor font",
                Some("Any monospace font installed on this computer."),
                "Font",
                &mut s.font_family,
                &opts,
                false,
            );
            let mut size = format!("{}", s.font_size);
            let sizes: Vec<(String, String)> = [11, 12, 13, 14, 15, 16, 18, 20]
                .iter()
                .map(|n| (n.to_string(), format!("{n} px")))
                .collect();
            let opts: Vec<(&str, &str)> = sizes
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            if setting_select(
                ui,
                p,
                "Editor text size",
                None,
                "Editor text size",
                &mut size,
                &opts,
                false,
            ) {
                s.font_size = size.parse().unwrap_or(13.0);
                changed = true;
            }
            let mut lh = format!("{}", s.line_height);
            if setting_select(
                ui,
                p,
                "Line height",
                None,
                "Line height",
                &mut lh,
                &[
                    ("1.4", "1.4"),
                    ("1.5", "1.5"),
                    ("1.65", "1.65"),
                    ("1.8", "1.8"),
                    ("2", "2"),
                ],
                false,
            ) {
                s.line_height = lh.parse().unwrap_or(1.65);
                changed = true;
            }
            let dark = s.dark(system_dark);
            let dark_desc = if dark {
                "Inverts the PDF pages while the dark theme is on."
            } else {
                "Only available when a dark theme is active."
            };
            let dark_a11y = if dark {
                "Dark PDF pages"
            } else {
                "Dark PDF pages (only in a dark theme)"
            };
            ui.add_enabled_ui(dark, |ui| {
                let mut on = s.pdf_dark;
                if setting_switch(
                    ui,
                    p,
                    "Dark PDF pages",
                    Some(dark_desc),
                    &mut on,
                    dark_a11y,
                    false,
                ) {
                    s.pdf_dark = on;
                    changed = true;
                }
            });
            changed |= setting_switch(
                ui,
                p,
                "Reduce motion",
                Some("No fades or smooth scrolling."),
                &mut s.reduce_motion,
                "Reduce motion: no fades or smooth scrolling",
                true,
            );
        }

        AppSection::Editing => {
            ui.push_id("save-mode", |ui| {
                setting_row(
                    ui,
                    p,
                    "Save",
                    Some("As I type saves automatically after a short pause."),
                    false,
                    |ui| {
                        let sel = usize::from(s.autosave_ms == 0.0);
                        if let Some(i) = widgets::segmented(
                            ui,
                            p,
                            &[(None, "As I type"), (None, "With Ctrl+S")],
                            sel,
                            false,
                        ) {
                            s.autosave_ms = if i == 0 {
                                Settings::default().autosave_ms
                            } else {
                                0.0
                            };
                            changed = true;
                        }
                    },
                );
            });
            if s.autosave_ms > 0.0 {
                let mut delay = format!("{}", s.autosave_ms);
                if setting_select(
                    ui,
                    p,
                    "Save delay",
                    Some("Save after typing stops for this long."),
                    "Save after typing stops for",
                    &mut delay,
                    &[
                        ("300", "0.3 seconds"),
                        ("600", "0.6 seconds"),
                        ("1000", "1 second"),
                        ("2000", "2 seconds"),
                        ("5000", "5 seconds"),
                    ],
                    false,
                ) {
                    s.autosave_ms = delay.parse().unwrap_or(Settings::default().autosave_ms);
                    changed = true;
                }
            }
            ui.push_id("compile-mode", |ui| {
                setting_row(ui, p, "Compile", None, false, |ui| {
                    let sel = usize::from(!s.auto_compile);
                    if let Some(i) = widgets::segmented(
                        ui,
                        p,
                        &[(None, "As I type"), (None, "With Ctrl+S")],
                        sel,
                        false,
                    ) {
                        s.auto_compile = i == 0;
                        changed = true;
                    }
                });
            });
            changed |= setting_switch(
                ui,
                p,
                "Suggestions while typing",
                Some("Commands, environments, labels and citations."),
                &mut s.autocomplete,
                "Suggest commands, environments, labels and citations while typing",
                false,
            );
            setting_row(
                ui,
                p,
                "Snippets",
                Some("Short triggers that expand into LaTeX templates when you press Tab."),
                false,
                |ui| {
                    let label = if s.snippets.is_empty() {
                        "Add…".to_owned()
                    } else {
                        format!("Edit {}…", s.snippets.len())
                    };
                    let b = Button::new(&label)
                        .kind(Kind::Ghost)
                        .size(Size::Sm)
                        .enabled(s.autocomplete)
                        .show(ui, p);
                    let b = if s.autocomplete {
                        b
                    } else {
                        b.on_disabled_hover_text(
                            "Snippets are offered by the suggestions, which are off",
                        )
                    };
                    if b.clicked() {
                        st.snippets = Some(s.snippets.clone());
                    }
                },
            );
            changed |= setting_switch(
                ui,
                p,
                "Rename matching \\end{…}",
                Some("Editing \\begin{…} renames its \\end{…}, and the other way round."),
                &mut s.env_sync,
                "Rename \\end{…} when \\begin{…} changes, and the other way round",
                false,
            );
            changed |= setting_switch(
                ui,
                p,
                "Close brackets and $ automatically",
                None,
                &mut s.auto_close,
                "Close brackets and dollar signs automatically",
                false,
            );
            changed |= setting_switch(
                ui,
                p,
                "Code folding",
                Some("Fold sections and environments from the gutter."),
                &mut s.folding,
                "Code folding for sections and environments",
                false,
            );
            let mut indent = match &s.indent {
                serde_json::Value::String(t) if t == "tab" => "tab".to_owned(),
                v => v.as_u64().unwrap_or(2).to_string(),
            };
            if setting_select(
                ui,
                p,
                "Indentation",
                None,
                "Indentation",
                &mut indent,
                &INDENTS,
                false,
            ) {
                s.indent = if indent == "tab" {
                    serde_json::Value::from("tab")
                } else {
                    serde_json::Value::from(indent.parse::<u64>().unwrap_or(2))
                };
                changed = true;
            }
            changed |= setting_switch(
                ui,
                p,
                "Wrap long lines",
                None,
                &mut s.word_wrap,
                "Wrap long lines",
                false,
            );
            changed |= setting_switch(
                ui,
                p,
                "Line numbers",
                None,
                &mut s.line_numbers,
                "Line numbers",
                true,
            );
        }

        AppSection::Language => {
            let mut spell: Vec<(String, String)> =
                vec![("auto".into(), "Match the document's language".into())];
            for (code, label) in crate::writing::spell::LANGUAGES {
                let note = st
                    .dicts
                    .iter()
                    .find(|d| d.code == *code && !d.installed)
                    .map(|d| {
                        format!(
                            " (downloads {} MB)",
                            (d.size_kb as f32 / 1024.0).round().max(1.0)
                        )
                    })
                    .unwrap_or_default();
                spell.push(((*code).into(), format!("{label}{note}")));
            }
            spell.push(("off".into(), "Off".into()));
            let opts: Vec<(&str, &str)> = spell
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            let mut lang = s.spell_default.clone();
            let picked = ui
                .add_enabled_ui(st.downloading.is_none(), |ui| {
                    setting_select(
                        ui,
                        p,
                        "Spell check",
                        Some("Used when a project does not set its own language."),
                        "Spell check",
                        &mut lang,
                        &opts,
                        false,
                    )
                })
                .inner;
            if picked {
                st.spell_error = None;
                if matches!(lang.as_str(), "auto" | "off")
                    || !st.dicts.iter().any(|d| d.code == lang && !d.installed)
                {
                    s.spell_default = lang;
                    changed = true;
                } else {
                    st.downloading = Some(lang.clone());
                    *out = Some(Out::Download(lang));
                }
            }
            if let Some(d) = &st.downloading {
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "Downloading the {} dictionary…",
                        crate::writing::spell::label(d)
                    ))
                    .size(12.5)
                    .color(p.muted),
                );
            } else if let Some(e) = &st.spell_error {
                ui.add_space(6.0);
                ui.label(RichText::new(e).size(12.5).color(p.error));
            }
            let grammar_off = s.grammar == "off";
            if setting_select(
                ui,
                p,
                "Grammar check",
                Some("Check prose with LanguageTool (optional)."),
                "Grammar check",
                &mut s.grammar,
                &[
                    ("off", "Off"),
                    ("local", "LanguageTool on this computer"),
                    ("custom", "LanguageTool at an address…"),
                ],
                grammar_off,
            ) {
                st.grammar_status = None;
                changed = true;
            }
            if s.grammar == "custom" {
                setting_row(ui, p, "Server address", None, false, |ui| {
                    let r = widgets::text_input(ui, p, &mut s.grammar_url, "", true);
                    if r.changed() {
                        st.grammar_url_dirty = true;
                    }
                    if r.lost_focus() {
                        s.grammar_url = s.grammar_url.trim().to_owned();
                        st.grammar_url_dirty = false;
                        changed = true;
                    }
                });
            }
            if s.grammar != "off" {
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let to = if s.grammar == "local" {
                        "the LanguageTool server on this computer (http://localhost:8081)"
                    } else {
                        "this server"
                    };
                    ui.label(
                        RichText::new(format!("Your text goes to {to} and nowhere else. "))
                            .size(12.5)
                            .color(p.muted),
                    );
                    let link = ui.add(
                        egui::Label::new(
                            RichText::new("Test the connection")
                                .size(12.5)
                                .underline()
                                .color(p.accent_text),
                        )
                        .sense(egui::Sense::click()),
                    );
                    if link
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        st.grammar_status = Some("checking…".into());
                        *out = Some(Out::TestGrammar);
                    }
                    if let Some(g) = &st.grammar_status {
                        ui.label(RichText::new(format!(" — {g}")).size(12.5).color(p.muted));
                    }
                });
            }
        }

        AppSection::Files => {
            let desc = if s.trash_mode != "system" {
                "Deleted files stay in the project folder for 7 days (Recently deleted), then are removed for good."
            } else {
                "Deleted files go straight to your system trash."
            };
            setting_row(ui, p, "Deleted files", Some(desc), true, |ui| {
                let sel = usize::from(s.trash_mode == "system");
                if let Some(i) = widgets::segmented(
                    ui,
                    p,
                    &[(None, "Keep for Undo"), (None, "System trash")],
                    sel,
                    false,
                ) {
                    s.trash_mode = if i == 0 { "project" } else { "system" }.into();
                    changed = true;
                }
            });
        }

        AppSection::Tex => {
            let det = tex.map(|t| &t.detection);
            let dist = match det.map(|d| &d.distribution) {
                Some(lemmata_tex::detect::Distribution::TexLive { year }) => format!(
                    "TeX Live {}",
                    year.map(|y| y.to_string()).unwrap_or_default()
                ),
                Some(lemmata_tex::detect::Distribution::Miktex { version }) => {
                    format!("MiKTeX {}", version.clone().unwrap_or_default())
                }
                _ => "No TeX distribution found".into(),
            };
            let dir = det
                .and_then(|d| d.bin_dir.as_ref())
                .map(|b| format!(" in {}", b.display()))
                .unwrap_or_default();
            let desc = format!("{}{dir}", dist.trim_end());
            setting_row(ui, p, "TeX installation", Some(&desc), true, |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if s.tex_dir.is_some()
                    && Button::new("Find automatically")
                        .kind(Kind::Ghost)
                        .size(Size::Sm)
                        .show(ui, p)
                        .clicked()
                {
                    *out = Some(Out::AutoTex);
                }
                if Button::new("Choose TeX folder…")
                    .with_icon(icons::FOLDER_OPEN)
                    .size(Size::Sm)
                    .show(ui, p)
                    .clicked()
                {
                    *out = Some(Out::ChooseTex);
                }
            });
        }

        AppSection::Updates => {
            changed |= setting_switch(
                ui,
                p,
                "Check for updates automatically",
                Some("Looks for new Lemmata releases on startup."),
                &mut s.check_for_updates,
                "Check for updates automatically",
                false,
            );
            let ver = format!("Lemmata {}", env!("CARGO_PKG_VERSION"));
            setting_row(ui, p, "Current version", Some(&ver), true, |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let label = if st.checking_updates {
                    "Checking…"
                } else {
                    "Check now"
                };
                if Button::new(label)
                    .kind(Kind::Ghost)
                    .size(Size::Sm)
                    .enabled(!st.checking_updates)
                    .show(ui, p)
                    .clicked()
                {
                    *out = Some(Out::CheckUpdates);
                }
            });
            if let Some(m) = &st.update_message {
                ui.add_space(6.0);
                ui.label(RichText::new(m).size(12.5).color(p.muted));
            }
        }
    }
    if changed && out.is_none() {
        *out = Some(Out::Changed);
    }
}

/// SnippetsDialog.svelte: a trigger, what it inserts and a note, per snippet.
fn snippets(ctx: &egui::Context, p: &Palette, st: &mut Prefs, s: &mut Settings) -> Option<Out> {
    let list = st.snippets.as_mut()?;
    let invalid = |list: &[Snippet], i: usize| {
        let sn = &list[i];
        let trimmed = sn.trigger.trim();
        !crate::editor::complete::valid_trigger(trimmed)
            || sn.body.trim().is_empty()
            || list.iter().position(|o| o.trigger.trim() == trimmed) != Some(i)
    };
    // A snippet nobody filled in is dropped without a word; the others must be put right to save.
    let blank = |sn: &Snippet| sn.trigger.trim().is_empty() && sn.body.trim().is_empty();
    let problems = (0..list.len())
        .filter(|&i| invalid(list, i) && !blank(&list[i]))
        .count();
    let (mut save, mut cancel) = (false, false);
    let r = widgets::modal_editing(
        ctx,
        p,
        "Snippets",
        Some("Type a trigger in the editor and pick it from the completion list."),
        640.0,
        |ui| {
            let code = |t: &str| {
                RichText::new(t)
                    .font(FontId::monospace(12.0))
                    .background_color(p.surface_2)
                    .color(p.ink)
            };
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let t = |s: &str| RichText::new(s).color(p.muted);
                ui.label(t("Use "));
                ui.label(code("${}"));
                ui.label(t(
                    " for tab stops (Tab jumps to the next one), for example ",
                ));
                ui.label(code("\\begin{theorem}${}\\end{theorem}"));
                ui.label(t(". Triggers are two or more letters, digits or dashes."));
            });
            if list.is_empty() {
                ui.label(RichText::new("No snippets yet.").color(p.muted));
            }
            let vh = ui.ctx().content_rect().height();
            let max_h = (vh - 250.0).clamp(140.0, vh * 0.5);
            let w = ui.available_width();
            let mut remove = None;
            let bad: Vec<bool> = (0..list.len())
                .map(|i| !blank(&list[i]) && invalid(list, i))
                .collect();
            ui.allocate_ui(vec2(w, max_h), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(max_h)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 12.0;
                        for (i, sn) in list.iter_mut().enumerate() {
                            egui::Frame::new()
                                .stroke(Stroke::new(1.0, if bad[i] { p.warn } else { p.border }))
                                .corner_radius(theme::R_MD)
                                .inner_margin(Margin::same(10))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.spacing_mut().item_spacing.y = 6.0;
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        ui.allocate_ui(vec2(130.0, 32.0), |ui| {
                                            let r = widgets::text_input(
                                                ui,
                                                p,
                                                &mut sn.trigger,
                                                "trigger",
                                                true,
                                            );
                                            ui.ctx().accesskit_node_builder(r.id, |n| {
                                                n.set_label("Trigger")
                                            });
                                        });
                                        ui.allocate_ui(
                                            vec2(ui.available_width() - 38.0, 32.0),
                                            |ui| {
                                                let r = widgets::text_input(
                                                    ui,
                                                    p,
                                                    &mut sn.description,
                                                    "Description (optional)",
                                                    false,
                                                );
                                                ui.ctx().accesskit_node_builder(r.id, |n| {
                                                    n.set_label("Description")
                                                });
                                            },
                                        );
                                        if Button::icon(icons::TRASH, "Delete snippet")
                                            .show_icon(ui, p)
                                            .clicked()
                                        {
                                            remove = Some(i);
                                        }
                                    });
                                    let r = widgets::text_area(
                                        ui,
                                        p,
                                        &mut sn.body,
                                        "\\begin{figure}[t]\n  ${}\n\\end{figure}",
                                        3,
                                        true,
                                    );
                                    ui.ctx().accesskit_node_builder(r.id, |n| {
                                        n.set_label("Text to insert")
                                    });
                                });
                        }
                    });
            });
            if let Some(i) = remove {
                list.remove(i);
            }
            if Button::new("New snippet")
                .with_icon(icons::PLUS)
                .show(ui, p)
                .clicked()
            {
                list.push(Snippet {
                    trigger: String::new(),
                    body: String::new(),
                    description: String::new(),
                });
            }
        },
        Some(&mut |ui: &mut Ui| {
            if Button::new("Save")
                .kind(Kind::Primary)
                .enabled(problems == 0)
                .show(ui, p)
                .clicked()
            {
                save = true;
            }
            if Button::new("Cancel").show(ui, p).clicked() {
                cancel = true;
            }
            if problems > 0 {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let t = format!(
                        "{problems} snippet{} incomplete or repeated: fix {} to save.",
                        if problems == 1 { " is" } else { "s are" },
                        if problems == 1 { "it" } else { "them" }
                    );
                    ui.label(RichText::new(t).size(12.5).color(p.warn));
                });
            }
        }),
    );
    if save {
        widgets::modal_focus_restore_by_salt(ctx, "Snippets");
        let list = st.snippets.take().unwrap_or_default();
        s.snippets = (0..list.len())
            .filter(|&i| !invalid(&list, i))
            .map(|i| Snippet {
                trigger: list[i].trigger.trim().to_owned(),
                ..list[i].clone()
            })
            .collect();
        return Some(Out::Changed);
    }
    if r.closed || cancel {
        if cancel && !r.closed {
            widgets::modal_focus_restore_by_salt(ctx, "Snippets");
        }
        st.snippets = None;
    }
    None
}
