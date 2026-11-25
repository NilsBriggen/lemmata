//! First-run guided setup (`Welcome`): confirms TeX works, lets the user pick a visual theme &
//! editor typography, tunes writing workflow preferences, and launches their first project.
//! Also exports the shared two-column guided shell used by `onboarding.rs` so TeX setup and
//! first-run welcome feel like one cohesive experience.

use crate::icons::{self, Icon};
use crate::shortcuts::{display, keys_of};
use crate::spell::LANGUAGES;
use crate::store::Settings;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind, Rich, Size};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Margin, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
    pos2, vec2,
};
use lemmata_tex::detect::{Detection, Distribution};

#[derive(Debug, PartialEq)]
pub enum Action {
    /// "Skip" or "Go to my projects".
    Done,
    Example,
    NewProject,
    OpenFolder,
    Import,
    /// A setting changed; save it.
    Settings,
}

/// One entry in the left guided step rail.
pub(super) struct StepItem {
    pub title: &'static str,
    pub subtitle: &'static str,
}

const WELCOME_STEPS: [StepItem; 4] = [
    StepItem {
        title: "Welcome",
        subtitle: "TeX & workspace",
    },
    StepItem {
        title: "Appearance",
        subtitle: "Theme & typography",
    },
    StepItem {
        title: "Workflow",
        subtitle: "Save, spell & helpers",
    },
    StepItem {
        title: "Start",
        subtitle: "Your first project",
    },
];

const EXTRAS: [(&str, &str); 4] = [
    ("synctex", "Jump between PDF and source"),
    ("biber", "biblatex bibliographies"),
    ("xelatex", "XeLaTeX, for system fonts"),
    ("lualatex", "LuaLaTeX"),
];

fn found(d: &Detection, name: &str) -> bool {
    d.tools.iter().any(|t| t.name == name && t.path.is_some())
}

/// Smooth cubic ease-out `0.0..=1.0` for step entrance transitions. Snaps to `1.0` immediately
/// when reduced motion is enabled or under tests so snapshots stay deterministic.
fn step_entrance(ctx: &egui::Context, salt: &str, step: usize) -> f32 {
    if cfg!(test) || theme::ease_secs() == 0.0 {
        return 1.0;
    }
    let id = egui::Id::unique(("guide-step-anim", salt));
    let now = ctx.input(|i| i.time);
    let (last_step, started) = ctx.data_mut(|d| {
        let cur = d.get_temp::<(usize, f64)>(id);
        match cur {
            Some((s, t)) if s == step => (s, t),
            _ => {
                d.insert_temp(id, (step, now));
                (step, now)
            }
        }
    });
    let _ = last_step;
    let raw = ((now - started) as f32 / 0.22).clamp(0.0, 1.0);
    if raw < 1.0 {
        ctx.request_repaint();
    }
    1.0 - (1.0 - raw).powi(3)
}

/// Shared two-column guided setup shell (left step rail + right scrollable stage + pinned footer),
/// collapsing cleanly into a compact top stepper bar on narrow windows (`< 760 px`).
#[allow(clippy::too_many_arguments)]
pub(super) fn guide_shell(
    ui: &mut Ui,
    p: &Palette,
    salt: &str,
    rail_subtitle: &str,
    steps: &[StepItem],
    active_step: usize,
    clickable_steps: bool,
    show_skip: bool,
    body: impl FnOnce(&mut Ui),
    footer: impl FnOnce(&mut Ui),
) -> (Option<usize>, bool) {
    let mut picked_step = None;
    let mut skipped = false;
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0, p.app);

    // Subtle ambient top accent glow (gently breathes when animations are enabled).
    let breath = if cfg!(test) || theme::ease_secs() == 0.0 {
        1.0
    } else {
        let t = ui.input(|i| i.time) as f32;
        0.92 + 0.08 * (t * 0.7).sin()
    };
    widgets::radial_glow(
        ui.painter(),
        pos2(full.center().x, full.top() - 0.1 * full.height()),
        Vec2::new(1240.0 * breath, 520.0 * breath),
        Palette::mix(p.accent, 13.0),
        0.72,
    );

    let w = 940.0f32.min(full.width() - 48.0).max(320.0);
    let h = 684.0f32.min(full.height() - 48.0).max(420.0);
    let compact = w < 760.0;
    let outer = Rect::from_center_size(full.center(), vec2(w, h));
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
    let rail_w = 244.0;
    let (rail_rect, stage_rect) = if compact {
        let rh = 68.0;
        (
            Rect::from_min_size(inner.min, vec2(inner.width(), rh)),
            Rect::from_min_max(pos2(inner.left(), inner.top() + rh), inner.max),
        )
    } else {
        (
            Rect::from_min_size(inner.min, vec2(rail_w, inner.height())),
            Rect::from_min_max(pos2(inner.left() + rail_w, inner.top()), inner.max),
        )
    };

    let rail_rad = if compact {
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
    ui.painter().rect_filled(rail_rect, rail_rad, p.surface_2);
    if compact {
        ui.painter().hline(
            rail_rect.x_range(),
            rail_rect.bottom() - 0.5,
            Stroke::new(1.0, p.border),
        );
    } else {
        ui.painter().vline(
            rail_rect.right() - 0.5,
            rail_rect.y_range(),
            Stroke::new(1.0, p.border),
        );
    }

    // Left / top step rail.
    if compact {
        let mut rui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rail_rect.shrink2(vec2(16.0, 12.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        rui.spacing_mut().item_spacing.x = 10.0;
        let _ = widgets::logo_mark(&mut rui, 26.0);
        rui.label(
            egui::RichText::new("Lemmata")
                .font(FontId::new(19.0, theme::font::display()))
                .color(p.ink),
        );
        rui.add_space(8.0);
        for (i, item) in steps.iter().enumerate() {
            let color = if i == active_step {
                p.accent
            } else if i < active_step {
                p.ok
            } else {
                p.border_strong
            };
            let (dot, r) = rui.allocate_exact_size(
                vec2(if i == active_step { 18.0 } else { 8.0 }, 8.0),
                if clickable_steps {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );
            rui.painter().rect_filled(dot, CornerRadius::same(4), color);
            if clickable_steps && r.on_hover_text(item.title).clicked() {
                picked_step = Some(i);
            }
        }
        rui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if show_skip
                && Button::new("Skip")
                    .kind(Kind::Ghost)
                    .size(Size::Sm)
                    .ink(p.muted)
                    .show(ui, p)
                    .clicked()
            {
                skipped = true;
            }
        });
    } else {
        let rail_inner = rail_rect.shrink2(vec2(18.0, 22.0));
        let mut rui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rail_inner)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        rui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let _ = widgets::logo_mark(ui, 30.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(
                    egui::RichText::new("Lemmata")
                        .font(FontId::new(20.0, theme::font::display()))
                        .color(p.ink),
                );
                ui.label(egui::RichText::new(rail_subtitle).size(11.5).color(p.faint));
            });
        });
        rui.add_space(28.0);

        // Animated vertical progress track behind step badges.
        let step_h = 54.0;
        let badge_x = rail_inner.left() + 18.0;
        if steps.len() > 1 {
            let first_y = rui.cursor().top() + 20.0;
            let last_y = first_y + (steps.len() - 1) as f32 * step_h;
            let prog_target = (active_step as f32 / (steps.len() - 1) as f32).clamp(0.0, 1.0);
            let prog = if cfg!(test) || theme::ease_secs() == 0.0 {
                prog_target
            } else {
                rui.ctx().animate_value_with_time(
                    egui::Id::unique(("guide-prog", salt)),
                    prog_target,
                    0.26,
                )
            };
            rui.painter()
                .vline(badge_x, first_y..=last_y, Stroke::new(2.0, p.border));
            if prog > 0.001 {
                let fill_y = first_y + (last_y - first_y) * prog;
                rui.painter()
                    .vline(badge_x, first_y..=fill_y, Stroke::new(2.0, p.ok));
            }
        }

        for (i, item) in steps.iter().enumerate() {
            let is_cur = i == active_step;
            let is_done = i < active_step;
            let sense = if clickable_steps {
                Sense::click()
            } else {
                Sense::hover()
            };
            let (row_r, resp) =
                rui.allocate_exact_size(vec2(rail_inner.width(), step_h - 6.0), sense);
            if clickable_steps {
                resp.widget_info(|| {
                    egui::WidgetInfo::selected(egui::Role::Button, true, is_cur, item.title)
                });
            }
            let hovered = clickable_steps && resp.hovered();
            if is_cur {
                rui.painter().add(
                    p.shadow_card
                        .as_shape(row_r, CornerRadius::same(theme::R_MD)),
                );
                rui.painter().rect(
                    row_r,
                    CornerRadius::same(theme::R_MD),
                    p.surface,
                    Stroke::new(1.0, p.card_hairline),
                    StrokeKind::Inside,
                );
            } else if hovered {
                rui.painter()
                    .rect_filled(row_r, CornerRadius::same(theme::R_MD), p.rail_active);
            }

            let center = pos2(badge_x, row_r.top() + 20.0);
            if is_cur {
                rui.painter()
                    .circle_filled(center, 14.5, Palette::mix(p.accent, 16.0));
                rui.painter().circle_filled(center, 11.0, p.accent);
                rui.painter().text(
                    center,
                    Align2::CENTER_CENTER,
                    format!("{}", i + 1),
                    FontId::new(11.0, theme::font::sans_600()),
                    p.accent_ink,
                );
            } else if is_done {
                rui.painter().circle_filled(center, 11.0, p.ok_soft);
                rui.painter()
                    .circle_stroke(center, 11.0, Stroke::new(1.5, p.ok));
                icons::paint(
                    rui.painter(),
                    Rect::from_center_size(center, Vec2::splat(11.0)),
                    icons::CHECK,
                    p.ok,
                );
            } else {
                rui.painter().circle_filled(center, 11.0, p.surface_2);
                rui.painter()
                    .circle_stroke(center, 11.0, Stroke::new(1.5, p.border_strong));
                rui.painter().text(
                    center,
                    Align2::CENTER_CENTER,
                    format!("{}", i + 1),
                    FontId::new(11.0, theme::font::sans_500()),
                    p.faint,
                );
            }

            let tx = badge_x + 20.0;
            rui.painter().text(
                pos2(tx, row_r.top() + 9.0),
                Align2::LEFT_TOP,
                item.title,
                FontId::new(
                    13.0,
                    if is_cur {
                        theme::font::sans_600()
                    } else {
                        theme::font::sans_500()
                    },
                ),
                if is_cur || is_done { p.ink } else { p.muted },
            );
            rui.painter().text(
                pos2(tx, row_r.top() + 26.0),
                Align2::LEFT_TOP,
                item.subtitle,
                FontId::proportional(11.5),
                if is_cur { p.muted } else { p.faint },
            );
            if clickable_steps {
                widgets::focus_ring(&rui, &resp, row_r, p, theme::R_MD);
                if resp
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    picked_step = Some(i);
                }
            }
            rui.add_space(6.0);
        }

        // Bottom rail utility strip: Skip setup + version.
        let bot_rect = Rect::from_min_max(
            pos2(rail_inner.left(), rail_inner.bottom() - 56.0),
            rail_inner.max,
        );
        let mut bui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bot_rect)
                .layout(egui::Layout::bottom_up(egui::Align::Min)),
        );
        bui.spacing_mut().item_spacing.y = 8.0;
        let ver_label = if cfg!(test) {
            "Lemmata"
        } else {
            concat!("Lemmata ", env!("CARGO_PKG_VERSION"))
        };
        bui.label(egui::RichText::new(ver_label).size(11.5).color(p.faint));
        if show_skip {
            let r = bui
                .add(
                    egui::Label::new(
                        egui::RichText::new("Skip setup")
                            .font(FontId::new(12.5, theme::font::sans_500()))
                            .color(p.muted),
                    )
                    .sense(Sense::click()),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Skip"));
            if r.clicked() {
                skipped = true;
            }
        }
    }

    // Right content stage: scrollable body + pinned bottom footer.
    let foot_h = 64.0;
    let body_rect = Rect::from_min_max(
        stage_rect.min,
        pos2(stage_rect.right(), stage_rect.bottom() - foot_h),
    );
    let foot_rect = Rect::from_min_max(
        pos2(stage_rect.left(), stage_rect.bottom() - foot_h),
        stage_rect.max,
    );

    let ease = step_entrance(ui.ctx(), salt, active_step);
    let slide_y = (1.0 - ease) * 10.0;
    let mut stage_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(body_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    stage_ui.multiply_opacity(0.25 + 0.75 * ease);
    egui::ScrollArea::vertical()
        .id_salt(("guide-body", salt, active_step))
        .auto_shrink([false, false])
        .show(&mut stage_ui, |ui| {
            egui::Frame::NONE
                .inner_margin(Margin {
                    left: if compact { 20 } else { 36 },
                    right: if compact { 20 } else { 36 },
                    top: (if compact { 20.0 } else { 28.0 } + slide_y).round() as i8,
                    bottom: 20,
                })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 14.0;
                    body(ui);
                });
        });

    ui.painter().hline(
        foot_rect.x_range(),
        foot_rect.top() + 0.5,
        Stroke::new(1.0, p.border),
    );
    let mut foot_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(foot_rect.shrink2(vec2(if compact { 20.0 } else { 36.0 }, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    footer(&mut foot_ui);

    (picked_step, skipped)
}

/// Shared eyebrow + Young Serif heading + crisp lede for both Welcome and Onboarding.
pub(super) fn step_heading(ui: &mut Ui, p: &Palette, eyebrow: &str, title: &str, lede: &str) {
    let mut ej = egui::text::LayoutJob::default();
    ej.append(
        eyebrow,
        0.0,
        egui::TextFormat {
            font_id: FontId::new(11.0, theme::font::sans_600()),
            color: p.accent,
            extra_letter_spacing: 0.08 * 11.0,
            ..Default::default()
        },
    );
    ui.label(ej);
    ui.add_space(-8.0);
    ui.label(
        egui::RichText::new(title)
            .font(FontId::new(28.0, theme::font::display()))
            .color(p.ink)
            .line_height(Some(36.0)),
    );
    ui.add_space(-4.0);
    widgets::paragraph(
        ui,
        lede,
        FontId::proportional(14.0),
        p.muted,
        1.5,
        ui.available_width().min(540.0),
    );
    ui.add_space(2.0);
}

pub fn show(
    ui: &mut Ui,
    p: &Palette,
    d: &Detection,
    step: &mut usize,
    settings: &mut Settings,
) -> Option<Action> {
    let mut body_action = None;
    let mut foot_action = None;
    let cur = (*step).min(WELCOME_STEPS.len() - 1);
    let mut next_step = cur;

    let (picked, skipped) = guide_shell(
        ui,
        p,
        "welcome",
        "First-run setup",
        &WELCOME_STEPS,
        cur,
        true,
        cur < WELCOME_STEPS.len() - 1,
        |ui| match cur {
            0 => step_welcome(ui, p, d),
            1 => step_appearance(ui, p, settings, &mut body_action),
            2 => step_workflow(ui, p, settings, &mut body_action),
            _ => step_start(ui, p, &mut body_action),
        },
        |ui| {
            if cur > 0
                && Button::new("Back")
                    .kind(Kind::Ghost)
                    .size(Size::Lg)
                    .show(ui, p)
                    .clicked()
            {
                next_step = cur - 1;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if cur < WELCOME_STEPS.len() - 1 {
                    if Button::new("Continue")
                        .trailing_icon(icons::ARROW_RIGHT)
                        .kind(Kind::Primary)
                        .size(Size::Lg)
                        .show(ui, p)
                        .clicked()
                    {
                        next_step = cur + 1;
                    }
                } else if Button::new("Go to my projects")
                    .kind(Kind::Primary)
                    .size(Size::Lg)
                    .show(ui, p)
                    .clicked()
                {
                    foot_action = Some(Action::Done);
                }
            });
        },
    );

    let mut action = body_action.or(foot_action);

    if let Some(s) = picked {
        *step = s;
    } else if next_step != cur {
        *step = next_step;
    }
    if skipped {
        action = Some(Action::Done);
    }
    action
}

/// Step 1: Animated miniature split-workspace showcase + TeX environment status.
fn step_welcome(ui: &mut Ui, p: &Palette, d: &Detection) {
    step_heading(
        ui,
        p,
        "STEP 1 OF 4 · WELCOME",
        "Welcome to Lemmata",
        "Write LaTeX on the left, read the live PDF on the right — fast, local, and stored as ordinary folders.",
    );

    // Animated miniature split-workspace preview card.
    paint_workspace_hero(ui, p);

    let distribution = match &d.distribution {
        Distribution::TexLive { year } => format!(
            "TeX Live {}",
            year.map(|y| y.to_string()).unwrap_or_default()
        ),
        Distribution::Miktex { version } => {
            format!("MiKTeX {}", version.clone().unwrap_or_default())
        }
        Distribution::Unknown => "TeX".into(),
    };
    egui::Frame::new()
        .fill(p.surface_2)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(theme::R_LG)
        .inner_margin(Margin::symmetric(16, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                icons::show(ui, icons::CIRCLE_CHECK, 18.0, p.ok);
                let mut text = Rich::new()
                    .add(
                        distribution.trim(),
                        FontId::new(13.0, theme::font::sans_600()),
                        p.ink,
                    )
                    .add(" is ready", FontId::proportional(13.0), p.ink);
                if let Some(bin) = &d.bin_dir {
                    text = text.add(
                        &format!(" in {}", bin.display()),
                        FontId::monospace(12.0),
                        p.muted,
                    );
                }
                text.show(ui);
            });
            // Only listed for MiKTeX on Windows, which doesn't ship the Perl latexmk runs on.
            if d.tools.iter().any(|t| t.name == "perl") && !found(d, "perl") {
                egui::Frame::new()
                    .fill(Palette::mix(p.warn, 14.0))
                    .corner_radius(theme::R_SM)
                    .inner_margin(Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        Rich::new()
                            .add(
                                "Perl is missing.",
                                FontId::new(12.5, theme::font::sans_600()),
                                p.ink,
                            )
                            .add(
                                " latexmk needs it and MiKTeX doesn't include it. Run ",
                                FontId::proportional(12.5),
                                p.ink,
                            )
                            .add(
                                "winget install StrawberryPerl.StrawberryPerl",
                                FontId::monospace(12.5),
                                p.ink,
                            )
                            .add(
                                " in a terminal, then restart Lemmata.",
                                FontId::proportional(12.5),
                                p.ink,
                            )
                            .line_height(12.5 * 1.45)
                            .wrap(ui.available_width())
                            .show(ui);
                    });
            }
            ui.horizontal(|ui| {
                ui.add_space(28.0);
                let avail = ui.available_width();
                let cols = if avail < 400.0 { 1 } else { 2 };
                let col = if cols == 2 {
                    (avail - 16.0) / 2.0
                } else {
                    avail
                };
                egui::Grid::new("welcome-extras")
                    .spacing([16.0, 6.0])
                    .min_col_width(col)
                    .max_col_width(col)
                    .show(ui, |ui| {
                        for (i, (tool, what)) in EXTRAS.iter().enumerate() {
                            let ok = found(d, tool);
                            let draw = |ui: &mut Ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                let icon = if ok {
                                    icons::CIRCLE_CHECK
                                } else {
                                    icons::CIRCLE
                                };
                                icons::show(ui, icon, 14.0, if ok { p.ok } else { p.faint });
                                ui.label(egui::RichText::new(*what).size(12.5).color(p.ink_2));
                                if !ok {
                                    ui.label(
                                        egui::RichText::new("optional").size(11.0).color(p.faint),
                                    );
                                }
                            };
                            if cols == 1 {
                                ui.horizontal_wrapped(draw);
                            } else {
                                ui.horizontal(draw);
                            }
                            if cols == 1 || i % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
            });
        });
}

/// Animated miniature split-workspace showcase: LaTeX editor on left + SyncTeX pulse + PDF card on right.
fn paint_workspace_hero(ui: &mut Ui, p: &Palette) {
    let w = ui.available_width();
    let h = 148.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let painter = ui.painter();
    painter.rect(
        rect,
        CornerRadius::same(theme::R_LG),
        p.app,
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );

    let animate = !cfg!(test) && theme::ease_secs() > 0.0;
    let t = if animate {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
        ui.input(|i| i.time) as f32
    } else {
        0.55
    };

    let pad = 12.0;
    let gap = 14.0;
    let pane_w = (rect.width() - pad * 2.0 - gap) / 2.0;
    let ed_r = Rect::from_min_size(
        pos2(rect.left() + pad, rect.top() + pad),
        vec2(pane_w, rect.height() - pad * 2.0),
    );
    let pdf_r = Rect::from_min_size(
        pos2(ed_r.right() + gap, rect.top() + pad),
        vec2(pane_w, rect.height() - pad * 2.0),
    );

    // Miniature editor card.
    painter.add(
        p.shadow_card
            .as_shape(ed_r, CornerRadius::same(theme::R_MD)),
    );
    painter.rect(
        ed_r,
        CornerRadius::same(theme::R_MD),
        p.code_bg,
        Stroke::new(1.0, p.card_hairline),
        StrokeKind::Inside,
    );
    let ed_bar = Rect::from_min_size(ed_r.min, vec2(ed_r.width(), 22.0));
    painter.rect_filled(
        ed_bar,
        CornerRadius {
            nw: theme::R_MD,
            ne: theme::R_MD,
            sw: 0,
            se: 0,
        },
        p.surface_2,
    );
    painter.hline(
        ed_bar.x_range(),
        ed_bar.bottom(),
        Stroke::new(1.0, p.border),
    );
    painter.text(
        pos2(ed_bar.left() + 10.0, ed_bar.center().y),
        Align2::LEFT_CENTER,
        "main.tex",
        FontId::new(10.5, theme::font::mono_500()),
        p.muted,
    );

    // Highlight active equation line in editor.
    let eq_line_y = ed_bar.bottom() + 44.0;
    let eq_hl = Rect::from_min_size(
        pos2(ed_r.left() + 1.0, eq_line_y - 2.0),
        vec2(ed_r.width() - 2.0, 18.0),
    );
    painter.rect_filled(eq_hl, 0, p.line_hl);

    let code_lines: [(&str, &str, Color32); 5] = [
        ("1", r"\section{Energy}", p.accent),
        ("2", r"\begin{equation}", p.code_comment),
        ("3", r"  E = \sqrt{m^2 c^4 + p^2 c^2}", p.ink),
        ("4", r"\end{equation}", p.code_comment),
        ("5", r"See Equation~\ref{eq:rel}.", p.muted),
    ];
    let code_clip = painter.with_clip_rect(ed_r.shrink(2.0));
    for (idx, (num, text, col)) in code_lines.iter().enumerate() {
        let ly = ed_bar.bottom() + 10.0 + idx as f32 * 17.0;
        code_clip.text(
            pos2(ed_r.left() + 10.0, ly),
            Align2::LEFT_TOP,
            *num,
            FontId::monospace(10.0),
            p.faint,
        );
        code_clip.text(
            pos2(ed_r.left() + 26.0, ly),
            Align2::LEFT_TOP,
            *text,
            FontId::monospace(10.5),
            *col,
        );
    }
    // Animated typing caret at end of equation line.
    let caret_alpha = if animate {
        ((t * 3.2).sin() * 0.5 + 0.5).clamp(0.15, 1.0)
    } else {
        1.0
    };
    let eq_w = painter
        .layout_no_wrap(code_lines[2].1.to_owned(), FontId::monospace(10.5), p.ink)
        .size()
        .x;
    let caret_x = (ed_r.left() + 26.0 + eq_w + 2.0).min(ed_r.right() - 8.0);
    code_clip.vline(
        caret_x,
        (eq_line_y + 1.0)..=(eq_line_y + 13.0),
        Stroke::new(1.5, p.accent.gamma_multiply(caret_alpha)),
    );

    // Miniature PDF card.
    painter.add(
        p.shadow_card
            .as_shape(pdf_r, CornerRadius::same(theme::R_MD)),
    );
    painter.rect(
        pdf_r,
        CornerRadius::same(theme::R_MD),
        p.surface_2,
        Stroke::new(1.0, p.card_hairline),
        StrokeKind::Inside,
    );
    let pdf_bar = Rect::from_min_size(pdf_r.min, vec2(pdf_r.width(), 22.0));
    painter.hline(
        pdf_bar.x_range(),
        pdf_bar.bottom(),
        Stroke::new(1.0, p.border),
    );
    painter.text(
        pos2(pdf_bar.left() + 10.0, pdf_bar.center().y),
        Align2::LEFT_CENTER,
        "1 / 1  ·  Live PDF",
        FontId::proportional(10.0),
        p.faint,
    );
    let page_r = Rect::from_min_max(
        pos2(pdf_r.left() + 18.0, pdf_bar.bottom() + 8.0),
        pos2(pdf_r.right() - 18.0, pdf_r.bottom() - 8.0),
    );
    let page_bg = if p.dark { p.surface } else { Color32::WHITE };
    painter.add(p.shadow_card.as_shape(page_r, CornerRadius::same(3)));
    painter.rect(
        page_r,
        CornerRadius::same(3),
        page_bg,
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
    painter.text(
        pos2(page_r.left() + 12.0, page_r.top() + 10.0),
        Align2::LEFT_TOP,
        "1  Energy",
        FontId::new(11.0, theme::font::display()),
        p.ink,
    );
    let pdf_eq_r = Rect::from_min_size(
        pos2(page_r.left() + 10.0, page_r.top() + 30.0),
        vec2(page_r.width() - 20.0, 26.0),
    );
    let pulse = if animate {
        ((t * 1.4).sin() * 0.5 + 0.5).clamp(0.0, 1.0)
    } else {
        0.5
    };
    painter.rect(
        pdf_eq_r,
        CornerRadius::same(4),
        Palette::mix(p.accent, 8.0 + 8.0 * pulse),
        Stroke::new(1.0, p.accent.gamma_multiply(0.35 + 0.35 * pulse)),
        StrokeKind::Inside,
    );
    painter.text(
        pdf_eq_r.center(),
        Align2::CENTER_CENTER,
        "E = √(m²c⁴ + p²c²)   (1)",
        FontId::new(10.5, theme::font::sans_500()),
        p.ink,
    );
    for (k, frac) in [(0, 0.82), (1, 0.58)] {
        let ly = pdf_eq_r.bottom() + 9.0 + k as f32 * 8.0;
        if ly + 3.0 < page_r.bottom() - 4.0 {
            painter.rect_filled(
                Rect::from_min_size(
                    pos2(page_r.left() + 12.0, ly),
                    vec2((page_r.width() - 24.0) * frac, 3.0),
                ),
                1,
                p.border_strong,
            );
        }
    }

    // Animated SyncTeX link dot bridging editor equation -> PDF equation.
    let link_y = eq_hl.center().y;
    let p0 = pos2(ed_r.right() - 2.0, link_y);
    let p1 = pos2(pdf_r.left() + 2.0, link_y);
    painter.line_segment([p0, p1], Stroke::new(1.5, p.accent.gamma_multiply(0.55)));
    let travel = if animate { (t * 0.65).fract() } else { 0.5 };
    let dot_pos = pos2(p0.x + (p1.x - p0.x) * travel, link_y);
    painter.circle_filled(dot_pos, 4.5, Palette::mix(p.accent, 28.0));
    painter.circle_filled(dot_pos, 2.5, p.accent);
}

/// Step 2: Visual 3-card Theme picker (Motion/Linear pattern) + live editor typography preview.
fn step_appearance(ui: &mut Ui, p: &Palette, s: &mut Settings, action: &mut Option<Action>) {
    step_heading(
        ui,
        p,
        "STEP 2 OF 4 · APPEARANCE",
        "Choose your style",
        "Pick a theme and editor typography. Changes apply immediately and can be adjusted anytime in Settings.",
    );

    let mut changed = false;
    let gap = 12.0;
    let card_w = ((ui.available_width() - gap * 2.0) / 3.0).max(120.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (key, label, sub) in [
            ("system", "System", "Matches your OS"),
            ("light", "Light", "Warm stone & paper"),
            ("dark", "Dark", "Deep slate"),
        ] {
            if theme_preview_card(ui, p, card_w, key, label, sub, s.theme == key).clicked() {
                s.theme = key.into();
                changed = true;
            }
        }
    });

    ui.add_space(4.0);
    // Editor font + text size quick controls with live LaTeX snippet preview.
    egui::Frame::new()
        .fill(p.surface_2)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(theme::R_LG)
        .inner_margin(Margin::symmetric(16, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 12.0;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(12.0, 8.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    widgets::label(ui, p, "Editor font");
                    let cur_font = usize::from(s.font_family == "system");
                    if let Some(i) = widgets::segmented(
                        ui,
                        p,
                        &[(None, "IBM Plex Mono"), (None, "System monospace")],
                        cur_font,
                        false,
                    ) {
                        s.font_family = if i == 0 { "plex" } else { "system" }.into();
                        changed = true;
                    }
                });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    widgets::label(ui, p, "Text size");
                    let sizes: [f64; 4] = [12.0, 13.0, 14.0, 15.0];
                    let cur_sz = sizes
                        .iter()
                        .position(|v| (*v - s.font_size).abs() < 0.5)
                        .unwrap_or(1);
                    if let Some(i) = widgets::segmented(
                        ui,
                        p,
                        &[
                            (None, "12 px"),
                            (None, "13 px"),
                            (None, "14 px"),
                            (None, "15 px"),
                        ],
                        cur_sz,
                        false,
                    ) {
                        s.font_size = sizes[i];
                        changed = true;
                    }
                });
            });

            // Live typography preview box.
            egui::Frame::new()
                .fill(p.code_bg)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(theme::R_MD)
                .inner_margin(Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let fam = theme::editor_family(ui.ctx(), &s.font_family);
                    let f = FontId::new(s.font_size as f32, fam);
                    Rich::new()
                        .add(r"\theorem", f.clone(), p.accent)
                        .add(" ", f.clone(), p.ink)
                        .add(
                            r"$\int_{-\infty}^{\infty} e^{-x^2}\,dx = \sqrt{\pi}$",
                            f,
                            p.ink,
                        )
                        .show(ui);
                });
        });

    if changed {
        *action = Some(Action::Settings);
    }
}

/// Visual theme card with miniature Lemmata split workspace preview and animated hover/selection.
fn theme_preview_card(
    ui: &mut Ui,
    p: &Palette,
    width: f32,
    key: &str,
    title: &str,
    subtitle: &str,
    selected: bool,
) -> egui::Response {
    let id = ui.make_persistent_id(("welcome-theme", key));
    let hovered = if cfg!(test) || theme::ease_secs() == 0.0 {
        0.0
    } else {
        ui.ctx().animate_bool_with_time(
            id,
            ui.ctx().read_response(id).is_some_and(|r| r.hovered()),
            theme::ease_secs(),
        )
    };
    let h = 136.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, h), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, selected, title));
    let rect = rect.translate(vec2(0.0, -hovered * 2.0));
    let painter = ui.painter();
    let rad = CornerRadius::same(theme::R_LG);

    if selected {
        painter.rect_stroke(
            rect.expand(1.5),
            CornerRadius::same(theme::R_LG + 2),
            Stroke::new(3.0, Palette::mix(p.accent, 20.0)),
            StrokeKind::Outside,
        );
    } else if hovered > 0.01 {
        painter.add(p.shadow.as_shape(rect, rad));
    } else {
        painter.add(p.shadow_card.as_shape(rect, rad));
    }
    let border = if selected {
        Stroke::new(2.0, p.accent)
    } else {
        Stroke::new(1.0, p.border.lerp_to_gamma(p.border_strong, hovered))
    };
    painter.rect(rect, rad, p.surface, border, StrokeKind::Inside);

    // Miniature workspace artwork inside top area of card.
    let art = Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(width - 16.0, 76.0));
    let art_rad = CornerRadius::same(theme::R);
    let paint_mini = |clip: &egui::Painter, pal: &Palette| {
        clip.rect_filled(art, art_rad, pal.app);
        // Rail strip
        clip.rect_filled(
            Rect::from_min_size(art.min, vec2(12.0, art.height())),
            0,
            pal.app,
        );
        // Editor mini-card
        let mid_x = art.left() + 14.0 + (art.width() - 22.0) * 0.52;
        let ed = Rect::from_min_max(
            pos2(art.left() + 14.0, art.top() + 12.0),
            pos2(mid_x - 3.0, art.bottom() - 6.0),
        );
        let pdf = Rect::from_min_max(
            pos2(mid_x + 3.0, art.top() + 12.0),
            pos2(art.right() - 6.0, art.bottom() - 6.0),
        );
        clip.rect_filled(ed, CornerRadius::same(3), pal.code_bg);
        clip.rect_filled(
            pdf,
            CornerRadius::same(3),
            if pal.dark {
                pal.surface
            } else {
                Color32::WHITE
            },
        );
        // Mini code lines
        for (i, frac) in [(0, 0.65), (1, 0.85), (2, 0.55), (3, 0.72)] {
            let ly = ed.top() + 8.0 + i as f32 * 9.0;
            let col = if i == 0 {
                pal.accent
            } else {
                pal.border_strong
            };
            clip.rect_filled(
                Rect::from_min_size(
                    pos2(ed.left() + 6.0, ly),
                    vec2((ed.width() - 12.0) * frac, 3.0),
                ),
                1,
                col,
            );
        }
        // Mini PDF lines
        clip.rect_filled(
            Rect::from_min_size(
                pos2(pdf.left() + 8.0, pdf.top() + 10.0),
                vec2((pdf.width() - 16.0) * 0.6, 4.0),
            ),
            1,
            pal.faint,
        );
        for i in 0..3 {
            clip.rect_filled(
                Rect::from_min_size(
                    pos2(pdf.left() + 8.0, pdf.top() + 22.0 + i as f32 * 8.0),
                    vec2(pdf.width() - 16.0, 2.5),
                ),
                1,
                pal.border_strong,
            );
        }
    };

    match key {
        "light" => paint_mini(&painter.with_clip_rect(art), &theme::LIGHT),
        "dark" => paint_mini(&painter.with_clip_rect(art), &theme::DARK),
        _ => {
            let left_half = Rect::from_min_max(art.min, pos2(art.center().x, art.bottom()));
            let right_half = Rect::from_min_max(pos2(art.center().x, art.top()), art.max);
            paint_mini(&painter.with_clip_rect(left_half), &theme::LIGHT);
            paint_mini(&painter.with_clip_rect(right_half), &theme::DARK);
            painter.vline(
                art.center().x,
                art.y_range(),
                Stroke::new(1.0, p.border_strong),
            );
        }
    }
    painter.rect_stroke(art, art_rad, Stroke::new(1.0, p.border), StrokeKind::Inside);

    if selected {
        let badge = pos2(art.right() - 10.0, art.top() + 10.0);
        painter.circle_filled(badge, 8.0, p.accent);
        icons::paint(
            painter,
            Rect::from_center_size(badge, Vec2::splat(9.0)),
            icons::CHECK,
            p.accent_ink,
        );
    }

    painter.text(
        pos2(rect.left() + 12.0, art.bottom() + 9.0),
        Align2::LEFT_TOP,
        title,
        FontId::new(13.0, theme::font::sans_600()),
        p.ink,
    );
    painter.text(
        pos2(rect.left() + 12.0, art.bottom() + 27.0),
        Align2::LEFT_TOP,
        subtitle,
        FontId::proportional(11.5),
        p.muted,
    );
    widgets::focus_ring(ui, &resp, rect, p, theme::R_LG);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Step 3: Workflow preferences (Save, Spell check, Smart LaTeX helpers) + key shortcut strip.
fn step_workflow(ui: &mut Ui, p: &Palette, s: &mut Settings, action: &mut Option<Action>) {
    step_heading(
        ui,
        p,
        "STEP 3 OF 4 · WORKFLOW",
        "Tune your writing flow",
        "All optional — you can adjust every preference later in Settings.",
    );

    let mut changed = false;
    egui::Frame::new()
        .fill(p.surface_2)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(theme::R_LG)
        .inner_margin(Margin::symmetric(18, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;

            workflow_row(
                ui,
                p,
                true,
                "Save",
                "As I type saves automatically after a short pause.",
                |ui| {
                    let current = usize::from(s.autosave_ms == 0.0);
                    if let Some(i) = widgets::segmented(
                        ui,
                        p,
                        &[(None, "As I type"), (None, "With Ctrl+S")],
                        current,
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

            workflow_row(
                ui,
                p,
                false,
                "Spell check",
                "Underline misspelled prose outside LaTeX commands.",
                |ui| {
                    let mut options: Vec<(&str, &str)> =
                        vec![("auto", "Match the document's language")];
                    options.extend(LANGUAGES.iter().copied());
                    options.push(("off", "Off"));
                    ui.allocate_ui(vec2(236.0, 32.0), |ui| {
                        changed |=
                            widgets::select(ui, p, "Spell check", &mut s.spell_default, &options);
                    });
                },
            );

            workflow_row(
                ui,
                p,
                false,
                "Suggestions while typing",
                "Commands, environments, labels, citations and snippets.",
                |ui| {
                    changed |=
                        widgets::switch(ui, p, &mut s.autocomplete, "Suggestions while typing")
                            .clicked();
                },
            );

            workflow_row(
                ui,
                p,
                false,
                "Rename matching \\end{…}",
                "Editing \\begin{…} updates its closing \\end{…} automatically.",
                |ui| {
                    changed |= widgets::switch(ui, p, &mut s.env_sync, "Rename matching \\end{…}")
                        .clicked();
                },
            );
        });

    if !matches!(
        s.spell_default.as_str(),
        "auto" | "off" | "en" | "de" | "fr"
    ) {
        ui.label(
            egui::RichText::new(
                "This dictionary downloads automatically the first time you open a project.",
            )
            .size(12.0)
            .color(p.muted),
        );
    }

    // Compact essential shortcuts strip.
    let cp = display(keys_of("palette"));
    let qo = display(keys_of("quick-open"));
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(theme::R_MD)
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(14.0, 6.0);
                for (keys, desc) in [
                    (cp.as_str(), "Commands"),
                    (qo.as_str(), "Quick open"),
                    ("Ctrl+Click", "SyncTeX jump"),
                ] {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        widgets::kbd(ui, p, keys);
                        ui.label(egui::RichText::new(desc).size(12.0).color(p.muted));
                    });
                }
            });
        });

    if changed {
        *action = Some(Action::Settings);
    }
}

fn workflow_row(
    ui: &mut Ui,
    p: &Palette,
    first: bool,
    title: &str,
    desc: &str,
    control: impl FnOnce(&mut Ui),
) {
    if !first {
        let y = ui.cursor().top();
        ui.painter()
            .hline(ui.max_rect().x_range(), y, Stroke::new(1.0, p.border));
    }
    egui::Frame::NONE
        .inner_margin(Margin::symmetric(0, 10))
        .show(ui, |ui| {
            let w = ui.available_width();
            ui.set_width(w);
            ui.horizontal(|ui| {
                let ctl_w = 240.0f32.min(w * 0.45);
                let lbl_w = (w - ctl_w - 16.0).max(120.0);
                ui.allocate_ui_with_layout(
                    vec2(lbl_w, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(lbl_w);
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(
                            egui::RichText::new(title)
                                .font(FontId::new(13.0, theme::font::sans_500()))
                                .color(p.ink),
                        );
                        ui.add(
                            egui::Label::new(egui::RichText::new(desc).size(12.0).color(p.muted))
                                .wrap(),
                        );
                    },
                );
                ui.allocate_ui_with_layout(
                    vec2(ctl_w, 32.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| control(ui),
                );
            });
        });
}

/// Step 4: Start writing — 3 elevated animated action cards + secondary example banner.
fn step_start(ui: &mut Ui, p: &Palette, action: &mut Option<Action>) {
    step_heading(
        ui,
        p,
        "STEP 4 OF 4 · START WRITING",
        "Ready to write",
        "Create a new project from a template, open an existing folder, or import from Overleaf.",
    );
    let tiles = [
        (
            icons::PLUS,
            "New project",
            "Article, thesis, slides, CV, letter, homework, or your own templates.",
            Action::NewProject,
        ),
        (
            icons::FOLDER_OPEN,
            "Open folder",
            "Open any local folder containing a .tex document.",
            Action::OpenFolder,
        ),
        (
            icons::FILE_ARCHIVE,
            "Import from Overleaf",
            "Import a single zip export or several projects at once.",
            Action::Import,
        ),
    ];
    let gap = 12.0;
    let w = ((ui.available_width() - gap * 2.0) / 3.0).max(120.0);
    let tile_h = tiles
        .iter()
        .map(|(_, _, body, _)| {
            let g = ui.painter().layout(
                (*body).into(),
                FontId::proportional(12.0),
                p.muted,
                (w - 28.0).max(40.0),
            );
            16.0 + 36.0 + 12.0 + 18.0 + 6.0 + g.size().y + 18.0
        })
        .fold(138.0f32, f32::max);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (icon, title, body, act) in tiles {
            if start_tile(ui, p, w, tile_h, icon, title, body).clicked() {
                *action = Some(act);
            }
        }
    });

    ui.add_space(4.0);
    // Quiet secondary banner card for opening the example document.
    let (ex_r, ex_resp) = ui.allocate_exact_size(vec2(ui.available_width(), 54.0), Sense::click());
    ex_resp.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, true, "Open an example document")
    });
    let ex_hov = if cfg!(test) || theme::ease_secs() == 0.0 {
        0.0
    } else {
        ui.ctx()
            .animate_bool_with_time(ex_resp.id, ex_resp.hovered(), theme::ease_secs())
    };
    let painter = ui.painter();
    painter.rect(
        ex_r,
        CornerRadius::same(theme::R_LG),
        p.surface_2,
        Stroke::new(1.0, p.border.lerp_to_gamma(p.accent, ex_hov)),
        StrokeKind::Inside,
    );
    let ico = Rect::from_center_size(pos2(ex_r.left() + 26.0, ex_r.center().y), Vec2::splat(30.0));
    painter.rect_filled(ico, CornerRadius::same(theme::R), p.accent_soft);
    icons::paint(painter, ico.shrink(7.0), icons::SPARKLES, p.accent);
    painter.text(
        pos2(ico.right() + 12.0, ex_r.top() + 12.0),
        Align2::LEFT_TOP,
        "New to LaTeX or Lemmata?",
        FontId::new(13.0, theme::font::sans_600()),
        p.ink,
    );
    painter.text(
        pos2(ico.right() + 12.0, ex_r.top() + 30.0),
        Align2::LEFT_TOP,
        "Open an example document to explore live compilation, citations and math.",
        FontId::proportional(12.0),
        p.muted,
    );
    icons::paint(
        painter,
        Rect::from_center_size(
            pos2(ex_r.right() - 20.0 + ex_hov * 3.0, ex_r.center().y),
            Vec2::splat(15.0),
        ),
        icons::ARROW_RIGHT,
        p.muted.lerp_to_gamma(p.accent, ex_hov),
    );
    widgets::focus_ring(ui, &ex_resp, ex_r, p, theme::R_LG);
    if ex_resp
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        *action = Some(Action::Example);
    }
}

/// Elevated start action card: tinted accent icon, crisp title & description, animated hover lift + arrow nudge.
fn start_tile(
    ui: &mut Ui,
    p: &Palette,
    width: f32,
    height: f32,
    icon: Icon,
    title: &str,
    body: &str,
) -> egui::Response {
    let id = ui.make_persistent_id(("welcome-start-tile", title));
    let hovered = if cfg!(test) || theme::ease_secs() == 0.0 {
        0.0
    } else {
        ui.ctx().animate_bool_with_time(
            id,
            ui.ctx().read_response(id).is_some_and(|r| r.hovered()),
            theme::ease_secs(),
        )
    };
    let body_galley = ui.painter().layout(
        body.into(),
        FontId::proportional(12.0),
        p.muted,
        (width - 28.0).max(40.0),
    );
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
    let rect = rect.translate(vec2(0.0, -hovered * 2.0));
    let painter = ui.painter();
    let rad = CornerRadius::same(theme::R_LG);
    if hovered > 0.01 {
        painter.add(p.shadow.as_shape(rect, rad));
    } else {
        painter.add(p.shadow_card.as_shape(rect, rad));
    }
    painter.rect(
        rect,
        rad,
        p.surface,
        Stroke::new(1.0, p.border.lerp_to_gamma(p.accent, hovered)),
        StrokeKind::Inside,
    );
    let ico = Rect::from_min_size(rect.min + Vec2::splat(14.0), Vec2::splat(36.0));
    painter.rect_filled(ico, CornerRadius::same(theme::R_MD), p.accent_soft);
    icons::paint(painter, ico.shrink(8.0), icon, p.accent);
    // Subtle trailing arrow at top-right that slides right on hover.
    icons::paint(
        painter,
        Rect::from_center_size(
            pos2(rect.right() - 18.0 + hovered * 3.0, ico.center().y),
            Vec2::splat(14.0),
        ),
        icons::ARROW_RIGHT,
        p.faint.lerp_to_gamma(p.accent, hovered),
    );
    let mut y = ico.bottom() + 12.0;
    painter.text(
        pos2(rect.left() + 14.0, y),
        Align2::LEFT_TOP,
        title,
        FontId::new(13.0, theme::font::sans_600()),
        p.ink,
    );
    y += 22.0;
    painter.galley(
        pos2(rect.left() + 14.0, y),
        body_galley,
        Color32::PLACEHOLDER,
    );
    widgets::focus_ring(ui, &response, rect, p, theme::R_LG);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    fn render(p: Palette, step: usize, name: &str) -> egui_kittest::SnapshotResult {
        let d = testing::detection();
        let mut settings = Settings::default();
        let mut step = step;
        let mut h = testing::harness(p, move |ui| {
            show(ui, &p, &d, &mut step, &mut settings);
        });
        h.run();
        h.try_snapshot(name)
    }

    #[test]
    fn welcome_steps_light_and_dark() {
        let mut results = egui_kittest::SnapshotResults::new();
        for (p, scheme) in [(theme::LIGHT, "light"), (theme::DARK, "dark")] {
            for step in 0..WELCOME_STEPS.len() {
                results.add(render(p, step, &format!("welcome-{}-{scheme}", step + 1)));
            }
        }
    }
}
