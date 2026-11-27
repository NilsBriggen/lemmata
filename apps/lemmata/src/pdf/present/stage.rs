//! Audience stage, Slide Overview Grid, and Lemmata-themed Presenter Console UI.

use super::data::{
    DeckItem, PaceState, PenColor, Region, ScreenMode, StageTool, format_elapsed, format_remaining,
};
use super::{PresentAction, Presentation};
use crate::icons::{self, Icon};
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, ChipTone, Kind, Size};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui,
    pos2, vec2,
};

/// Renders the full-screen audience presentation surface.
pub(super) fn audience_ui(
    p: &mut Presentation,
    ctx: &egui::Context,
    pal: &Palette,
    keep: &mut bool,
) {
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::unique("presentation"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, r) = ui.allocate_exact_size(screen.size(), Sense::click());
            let aud_page = p.audience_page();
            r.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Image,
                    true,
                    format!("Slide {} of {}", aud_page, p.total()),
                )
            });

            let bg = match p.screen {
                ScreenMode::Whiteout => Color32::WHITE,
                ScreenMode::Normal | ScreenMode::Blackout => Color32::BLACK,
            };
            ui.painter().rect_filled(rect, 0, bg);

            let hud_rect = audience_hud_rect(rect);
            let ptr_pos = ui.input(|i| i.pointer.hover_pos().or_else(|| i.pointer.interact_pos()));
            let pointer_in_window = ptr_pos.is_some_and(|pos| rect.contains(pos));
            let pointer_in_hud = ptr_pos.is_some_and(|pos| hud_rect.contains(pos));
            let since_move = ui.input(|i| i.pointer.time_since_last_movement());
            let recently_moved = pointer_in_window && since_move <= 2.2;
            let show_hud = !p.overview
                && !p.shortcuts_open
                && if p.tool == StageTool::None {
                    pointer_in_hud || recently_moved
                } else {
                    p.active_stroke.is_none() && pointer_in_hud
                };

            let mut consumed_by_tool = false;
            if p.overview && p.total() > 0 {
                overview_grid(p, ui, pal, rect);
            } else if p.screen != ScreenMode::Normal {
                // Audience sees pure black or pure white screen.
            } else if let Some(e) = &p.error {
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    e,
                    FontId::proportional(14.0),
                    Color32::from_gray(200),
                );
            } else if p.total() > 0
                && let Some((slide_rect, _)) = p.draw_deck_item(ui, rect, aud_page)
            {
                let allow_draw = p.frozen_page.is_none();
                if !(show_hud && pointer_in_hud) {
                    consumed_by_tool =
                        p.interact_stage(ui, slide_rect, aud_page, "audience-stage", allow_draw);
                }
                p.paint_overlays(ui, slide_rect, aud_page, p.frozen_page.is_none());
            }

            // Jump-to-slide digit buffer pill.
            if !p.jump_digits.is_empty() {
                let text = format!("Go to slide: {}", p.jump_digits);
                let g = ui
                    .painter()
                    .layout_no_wrap(text, FontId::proportional(14.0), pal.ink);
                let pill = Rect::from_center_size(
                    pos2(rect.center().x, rect.bottom() - 64.0),
                    g.size() + vec2(26.0, 14.0),
                );
                ui.painter()
                    .add(pal.shadow.as_shape(pill, CornerRadius::same(theme::R_LG)));
                ui.painter().rect(
                    pill,
                    CornerRadius::same(theme::R_LG),
                    pal.surface,
                    Stroke::new(1.0, pal.accent),
                    StrokeKind::Inside,
                );
                ui.painter()
                    .galley(pill.center() - g.size() / 2.0, g, pal.ink);
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(250));
            }

            // Auto-hiding interactive floating HUD when pointer moves on the audience window.
            // Visibility is checked geometrically against `rect` / `hud_rect` (never `r.hovered()`),
            // so hovering HUD buttons does not drop `r.hovered()` and oscillate the HUD on/off.
            if show_hud && hud_rect.width() >= 24.0 && hud_rect.height() >= 24.0 {
                audience_hud(p, ui, pal, hud_rect, keep);
                if recently_moved && !pointer_in_hud {
                    let wait = (2.25 - since_move).clamp(0.05, 2.3);
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_secs_f32(wait));
                }
            }

            if p.shortcuts_open && !p.presenter {
                shortcuts_overlay(p, ui, pal, rect);
            }

            // Click on left third goes back, right two thirds forward (disabled during tools/HUD/overview).
            if !p.overview
                && !p.shortcuts_open
                && !consumed_by_tool
                && p.tool == StageTool::None
                && !(show_hud && pointer_in_hud)
                && r.clicked()
                && let Some(pos) = r.interact_pointer_pos()
            {
                let step = if (pos.x - rect.left()) > rect.width() / 3.0 {
                    1.0
                } else {
                    -1.0
                };
                p.go(p.page as f64 + step);
            }

            if !show_hud && !p.overview && !p.shortcuts_open && p.tool == StageTool::None {
                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
            }
        });
}

fn audience_hud_rect(rect: Rect) -> Rect {
    let hud_h = 42.0_f32.min(rect.height().max(0.0));
    let hud_w = (rect.width() - 40.0).clamp(0.0, 476.0);
    Rect::from_center_size(
        pos2(
            rect.center().x,
            (rect.bottom() - 32.0).max(rect.top() + hud_h * 0.5),
        ),
        vec2(hud_w, hud_h),
    )
}

/// Floating bottom-center HUD pill on the audience screen on pointer hover, styled with Lemmata's
/// active `Palette`.
fn audience_hud(p: &mut Presentation, ui: &mut Ui, pal: &Palette, hud: Rect, keep: &mut bool) {
    // Capture clicks on the pill surface before placing child buttons so gaps inside the HUD
    // never fall through to the slide-advance click handler underneath.
    let _pill_blocker = ui.interact(hud, ui.scope_id().with("audience-hud-bg"), Sense::click());
    let rad = CornerRadius::same(12);
    ui.painter().add(pal.shadow.as_shape(hud, rad));
    ui.painter().rect(
        hud,
        rad,
        pal.surface,
        Stroke::new(1.0, pal.border_strong),
        StrokeKind::Inside,
    );

    let mut c = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(hud.shrink2(vec2(10.0, 4.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    c.spacing_mut().item_spacing.x = 6.0;
    let total = p.total();

    if Button::icon(icons::CHEVRON_LEFT, "Previous slide")
        .size(Size::Sm)
        .enabled(p.page > 1)
        .show_icon(&mut c, pal)
        .on_hover_text("Previous slide")
        .clicked()
    {
        p.go(p.page as f64 - 1.0);
    }
    c.label(
        RichText::new(format!("{} / {}", p.audience_page(), total.max(1)))
            .font(FontId::new(12.5, theme::font::mono_500()))
            .color(pal.ink),
    );
    if Button::icon(icons::CHEVRON_RIGHT, "Next slide")
        .size(Size::Sm)
        .enabled(p.page < total)
        .show_icon(&mut c, pal)
        .on_hover_text("Next slide")
        .clicked()
    {
        p.go(p.page as f64 + 1.0);
    }
    sep_v(&mut c, pal, 18.0);
    if Button::new("Presenter")
        .with_icon(icons::PRESENTATION)
        .kind(if p.presenter {
            Kind::Default
        } else {
            Kind::Ghost
        })
        .on(p.presenter)
        .size(Size::Sm)
        .icon_size(14.0)
        .show(&mut c, pal)
        .on_hover_text("Open presenter view (P)")
        .clicked()
    {
        if p.presenter {
            p.close_presenter_console();
        } else {
            p.presenter = true;
        }
    }
    if Button::icon(icons::LAYOUT_GRID, "Slide overview (G)")
        .size(Size::Sm)
        .on(p.overview)
        .enabled(total > 0)
        .show_icon(&mut c, pal)
        .on_hover_text("Slide overview (G)")
        .clicked()
    {
        p.overview = !p.overview;
    }
    if Button::icon(icons::LOCATE, "Laser pointer (L)")
        .size(Size::Sm)
        .on(p.tool == StageTool::Laser)
        .show_icon(&mut c, pal)
        .on_hover_text("Click & hold on slide for laser pointer (L)")
        .clicked()
    {
        p.toggle_tool(StageTool::Laser);
    }
    if Button::icon(icons::PENCIL, "Annotate slide (D)")
        .size(Size::Sm)
        .on(p.tool == StageTool::Pen)
        .show_icon(&mut c, pal)
        .on_hover_text("Annotate slide (D)")
        .clicked()
    {
        p.toggle_tool(StageTool::Pen);
    }
    c.with_layout(egui::Layout::right_to_left(egui::Align::Center), |c| {
        if Button::icon(icons::X, "End presentation (Esc)")
            .size(Size::Sm)
            .ink(pal.error)
            .show_icon(c, pal)
            .on_hover_text("End presentation (Esc)")
            .clicked()
        {
            *keep = false;
        }
    });
}

/// Scrollable slide overview grid (`G`) with section titles and annotated-slide dots.
fn overview_grid(p: &mut Presentation, ui: &mut Ui, pal: &Palette, rect: Rect) {
    ui.painter().rect_filled(rect, 0, pal.app);
    let total = p.total();
    let header_h = 50.0_f32.min(rect.height());
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), header_h));
    ui.painter().rect_filled(header, 0, pal.surface);
    ui.painter().hline(
        header.x_range(),
        header.bottom() - 0.5,
        Stroke::new(1.0, pal.border),
    );
    if header.width() >= 48.0 && header.height() >= 20.0 {
        let mut h = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(header.shrink2(vec2(18.0_f32.min(header.width() * 0.2), 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        h.spacing_mut().item_spacing.x = 10.0;
        icons::show(&mut h, icons::LAYOUT_GRID, 15.0, pal.accent);
        h.label(
            RichText::new(format!("Slide overview · {total} slides"))
                .font(FontId::new(14.0, theme::font::sans_600()))
                .color(pal.ink),
        );
        if header.width() >= 520.0 {
            h.label(
                RichText::new("Click or use arrows + Enter to jump · Esc to close")
                    .size(12.5)
                    .color(pal.muted),
            );
        }
        h.with_layout(egui::Layout::right_to_left(egui::Align::Center), |h| {
            if Button::new("Close")
                .with_icon(icons::X)
                .kind(Kind::Default)
                .size(Size::Sm)
                .icon_size(14.0)
                .show(h, pal)
                .clicked()
            {
                p.overview = false;
            }
        });
    }

    // Optional clickable section quick-jump strip when the PDF has outline bookmarks.
    let deck_secs: Vec<(usize, String)> = p
        .deck_sections()
        .into_iter()
        .map(|(s, t)| (s, t.to_owned()))
        .collect();
    let sec_bar_h = if !deck_secs.is_empty() && rect.height() > header_h + 120.0 {
        38.0
    } else {
        0.0
    };
    if sec_bar_h > 0.0 {
        let sec_bar = Rect::from_min_size(
            pos2(rect.left(), header.bottom()),
            vec2(rect.width(), sec_bar_h),
        );
        ui.painter().rect_filled(sec_bar, 0, pal.surface_2);
        ui.painter().hline(
            sec_bar.x_range(),
            sec_bar.bottom() - 0.5,
            Stroke::new(1.0, pal.border),
        );
        let mut s_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(sec_bar.shrink2(vec2(18.0_f32.min(sec_bar.width() * 0.2), 4.0))),
        );
        let mut jump_to = None;
        egui::ScrollArea::horizontal()
            .id_salt("overview-sections-strip")
            .auto_shrink([false, false])
            .show(&mut s_ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    widgets::label(ui, pal, "SECTIONS");
                    for &(slide, ref title) in &deck_secs {
                        let active = p.section_for_deck(p.page) == Some(title.as_str());
                        if Button::new(&format!("{slide} · {title}"))
                            .kind(if active { Kind::Default } else { Kind::Ghost })
                            .on(active)
                            .size(Size::Sm)
                            .show(ui, pal)
                            .clicked()
                        {
                            jump_to = Some(slide);
                        }
                    }
                });
            });
        if let Some(s) = jump_to {
            p.go(s as f64);
            p.overview = false;
            return;
        }
    }

    let body_outer = Rect::from_min_max(pos2(rect.left(), header.bottom() + sec_bar_h), rect.max);
    let pad = 18.0_f32.min(body_outer.width().min(body_outer.height()) * 0.25);
    let body = body_outer.shrink(pad);
    if body.width() < 24.0 || body.height() < 24.0 {
        return;
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
    let mut picked = None;
    let deck = p.deck_items();
    egui::ScrollArea::vertical()
        .id_salt("present-overview")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            let w = ui.available_width().max(1.0);
            let gap = 16.0;
            let cols = (((w + gap) / 236.0).floor() as usize).clamp(1, 6);
            let cell_w = ((w - gap * (cols as f32 - 1.0)) / cols as f32).max(110.0);
            let thumb_h = cell_w * 0.60;
            let cell_h = thumb_h + 32.0;
            let rows = total.div_ceil(cols);
            let (grid_rect, _) =
                ui.allocate_exact_size(vec2(w, rows as f32 * (cell_h + gap)), Sense::hover());
            for (i, item) in deck.iter().copied().enumerate() {
                let (r, c) = (i / cols, i % cols);
                let cell = Rect::from_min_size(
                    grid_rect.min + vec2(c as f32 * (cell_w + gap), r as f32 * (cell_h + gap)),
                    vec2(cell_w, cell_h),
                );
                if !ui.is_rect_visible(cell) {
                    continue;
                }
                let slide_num = i + 1;
                let resp = ui
                    .interact(cell, ui.scope_id().with(("ov", i)), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                resp.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Button,
                        true,
                        format!("Slide {slide_num}"),
                    )
                });
                let thumb = Rect::from_min_size(cell.min, vec2(cell_w, thumb_h));
                let is_cur = slide_num == p.page;
                let is_frozen = p.frozen_page == Some(slide_num);
                let rad = CornerRadius::same(theme::R_MD);
                ui.painter().add(pal.shadow_card.as_shape(thumb, rad));
                ui.painter().rect_filled(thumb, rad, pal.pdf_ground);
                if let Some((sr, _)) = p.draw_deck_item(ui, thumb.shrink(6.0), slide_num) {
                    p.paint_overlays(ui, sr, slide_num, false);
                }
                let stroke = if is_cur {
                    Stroke::new(2.0, pal.accent)
                } else if is_frozen {
                    Stroke::new(2.0, pal.warn)
                } else if resp.hovered() {
                    Stroke::new(1.5, pal.border_strong)
                } else {
                    Stroke::new(1.0, pal.card_hairline)
                };
                ui.painter()
                    .rect_stroke(thumb, rad, stroke, StrokeKind::Inside);

                if is_frozen {
                    let badge_g = ui.painter().layout_no_wrap(
                        "FROZEN".to_owned(),
                        FontId::new(9.5, theme::font::sans_600()),
                        pal.warn,
                    );
                    let badge_r = Rect::from_min_size(
                        pos2(thumb.left() + 6.0, thumb.top() + 6.0),
                        badge_g.size() + vec2(10.0, 5.0),
                    );
                    ui.painter()
                        .rect_filled(badge_r, CornerRadius::same(4), pal.warn_soft);
                    ui.painter()
                        .galley(badge_r.center() - badge_g.size() * 0.5, badge_g, pal.warn);
                }

                // Rehearsal duration pill in bottom-right of thumbnail when >= 1s recorded.
                let elapsed = p.slide_elapsed(slide_num);
                if elapsed >= 1.0 {
                    let tg = ui.painter().layout_no_wrap(
                        format_elapsed(elapsed),
                        FontId::new(10.0, theme::font::mono_500()),
                        pal.ink,
                    );
                    let tr = Rect::from_min_size(
                        pos2(
                            thumb.right() - tg.size().x - 14.0,
                            thumb.bottom() - tg.size().y - 10.0,
                        ),
                        tg.size() + vec2(8.0, 4.0),
                    );
                    ui.painter().rect(
                        tr,
                        CornerRadius::same(4),
                        pal.surface,
                        Stroke::new(1.0, pal.border),
                        StrokeKind::Inside,
                    );
                    ui.painter()
                        .galley(tr.center() - tg.size() * 0.5, tg, pal.ink);
                }

                let has_ink = p.has_strokes_on_slide(slide_num);
                let has_note = p.effective_note(slide_num).is_some();
                let dot_space = match (has_ink, has_note) {
                    (true, true) => 30.0,
                    (true, false) | (false, true) => 18.0,
                    (false, false) => 4.0,
                };
                let caption = match item {
                    DeckItem::Whiteboard(_) => format!("{slide_num} · Whiteboard"),
                    DeckItem::Pdf(idx) => match p.sections.get(&(idx + 1)) {
                        Some(sec) => format!("{slide_num} · {sec}"),
                        None => format!("Slide {slide_num}"),
                    },
                };
                let mut job = egui::text::LayoutJob::simple_singleline(
                    caption,
                    if is_cur {
                        FontId::new(12.0, theme::font::sans_600())
                    } else {
                        FontId::proportional(12.0)
                    },
                    if is_cur { pal.ink } else { pal.muted },
                );
                job.wrap.max_width = (cell_w - dot_space).max(10.0);
                job.wrap.max_rows = 1;
                job.wrap.overflow_character = Some('…');
                let g = ui.fonts_mut(|f| f.layout_job(job));
                ui.painter()
                    .galley(pos2(cell.left() + 2.0, thumb.bottom() + 6.0), g, pal.ink);
                let mut dot_x = cell.right() - 6.0;
                if has_ink {
                    ui.painter()
                        .circle_filled(pos2(dot_x, thumb.bottom() + 13.0), 3.5, pal.error);
                    dot_x -= 11.0;
                }
                if has_note {
                    ui.painter()
                        .circle_filled(pos2(dot_x, thumb.bottom() + 13.0), 3.5, pal.accent);
                }
                if resp.clicked() {
                    picked = Some(slide_num);
                }
            }
        });
    if let Some(slide) = picked {
        p.go(slide as f64);
        p.overview = false;
    }
}

/// Lemmata-themed three-pane Presenter Console UI. Returns `false` when the presentation ends.
pub(super) fn presenter_ui(p: &mut Presentation, ui: &mut Ui, app_pal: &Palette) -> bool {
    let pal = p.effective_palette(app_pal);
    let mut keep = true;
    let full = ui.max_rect();
    // Paint Lemmata's quiet stone ground across the whole presenter window.
    ui.painter().rect_filled(full, 0, pal.app);

    let head_h = 48.0_f32.min(full.height());
    let foot_h = 52.0_f32.min((full.height() - head_h).max(0.0));
    let show_pace = p.total() > 1 || p.store.target_minutes.is_some();
    let pace_h = if show_pace && full.height() > head_h + foot_h + 10.0 {
        3.0
    } else {
        0.0
    };
    let head = Rect::from_min_size(full.min, vec2(full.width(), head_h));
    let pace_bar =
        Rect::from_min_size(pos2(full.left(), head.bottom()), vec2(full.width(), pace_h));
    let foot = Rect::from_min_max(pos2(full.left(), full.bottom() - foot_h), full.max);

    let avail_mid = (foot.top() - pace_bar.bottom()).max(0.0);
    let filmstrip_h = if p.store.filmstrip && p.total() > 0 && avail_mid > 160.0 {
        92.0
    } else {
        0.0
    };
    let filmstrip_rect = Rect::from_min_max(
        pos2(full.left(), foot.top() - filmstrip_h),
        pos2(full.right(), foot.top()),
    );

    // Subtle pacing progress bar (only when multi-slide or target duration is set).
    if pace_h > 0.0 {
        paint_pacing_bar(p, ui, pal, pace_bar);
    }

    // Header directly on `pal.app` stone ground (matching Lemmata's workspace header).
    let (target_anchor, export_anchor, sections_anchor) = presenter_header(p, ui, pal, head);

    // Main stage area: Overview grid OR customizable three-pane stage (Notes | Current | Next).
    let stage_rect = Rect::from_min_max(
        pos2(full.left(), pace_bar.bottom()),
        pos2(full.right(), filmstrip_rect.top()),
    );
    if p.overview && p.total() > 0 {
        overview_grid(p, ui, pal, stage_rect);
    } else {
        let pad_x = 12.0_f32.min(stage_rect.width() * 0.15);
        let pad_top =
            (if pace_h > 0.0 { 8.0_f32 } else { 0.0_f32 }).min(stage_rect.height() * 0.15);
        let pad_bot = 12.0_f32.min(stage_rect.height() * 0.15);
        let inner = Rect::from_min_max(
            pos2(stage_rect.left() + pad_x, stage_rect.top() + pad_top),
            pos2(
                (stage_rect.right() - pad_x).max(stage_rect.left() + pad_x),
                (stage_rect.bottom() - pad_bot).max(stage_rect.top() + pad_top),
            ),
        );
        presenter_stage(p, ui, pal, inner);
    }

    // Optional collapsible slide filmstrip.
    if filmstrip_h > 0.0 {
        presenter_filmstrip(p, ui, pal, filmstrip_rect);
    }

    // Three-zone surface transport footer.
    let view_anchor = presenter_footer(p, ui, pal, foot, &mut keep);

    // Mutually exclusive popovers & overlays with click-absorbing foreground scrims.
    if p.shortcuts_open {
        p.target_menu_open = false;
        p.export_menu_open = false;
        p.view_menu_open = false;
        p.sections_menu_open = false;
        egui::Area::new(egui::Id::unique("presenter-shortcuts"))
            .order(egui::Order::Foreground)
            .fixed_pos(full.min)
            .show(ui.ctx(), |ui| {
                let (rect, _) = ui.allocate_exact_size(full.size(), Sense::hover());
                shortcuts_overlay(p, ui, pal, rect);
            });
    } else if p.target_menu_open
        && let Some(anchor) = target_anchor
    {
        target_duration_popover(p, ui.ctx(), pal, full, anchor);
    } else if p.export_menu_open
        && let Some(anchor) = export_anchor
    {
        export_pdf_popover(p, ui.ctx(), pal, full, anchor);
    } else if p.sections_menu_open
        && let Some(anchor) = sections_anchor
    {
        sections_popover(p, ui.ctx(), pal, full, anchor);
    } else if p.view_menu_open
        && let Some(anchor) = view_anchor
    {
        view_popover(p, ui.ctx(), pal, app_pal.dark, full, anchor);
    }

    // Transient status toast (bottom-right above footer).
    if let Some((msg, at)) = p.status_toast.clone() {
        if at.elapsed().as_secs_f32() < 4.0 {
            let g = ui
                .painter()
                .layout_no_wrap(msg, FontId::proportional(12.5), pal.ink);
            let pill = Rect::from_min_size(
                pos2(
                    (full.right() - g.size().x - 40.0).max(full.left() + 12.0),
                    (foot.top() - filmstrip_h - 44.0).max(full.top() + 12.0),
                ),
                g.size() + vec2(24.0, 14.0),
            );
            ui.painter()
                .add(pal.shadow.as_shape(pill, CornerRadius::same(theme::R_MD)));
            ui.painter().rect(
                pill,
                CornerRadius::same(theme::R_MD),
                pal.surface,
                Stroke::new(1.0, pal.accent),
                StrokeKind::Inside,
            );
            ui.painter()
                .galley(pill.center() - g.size() / 2.0, g, pal.ink);
        } else {
            p.status_toast = None;
        }
    }

    keep
}

fn paint_pacing_bar(p: &Presentation, ui: &Ui, pal: &Palette, rect: Rect) {
    ui.painter().rect_filled(rect, 0, pal.border);
    let total = p.total();
    if total > 0 {
        let frac = (p.page as f32 / total as f32).clamp(0.0, 1.0);
        let fill_color = match p.timer.pace(p.store.target_minutes) {
            PaceState::Overtime => pal.error,
            PaceState::Warning => pal.warn,
            PaceState::OnPace | PaceState::Idle => pal.accent,
        };
        let fill = Rect::from_min_size(rect.min, vec2(rect.width() * frac, rect.height()));
        ui.painter().rect_filled(fill, 0, fill_color);
    }
    if let Some(mins) = p.store.target_minutes.filter(|&m| m > 0) {
        let time_frac = (p.timer.elapsed() / (mins as f64 * 60.0)).clamp(0.0, 1.0) as f32;
        let x = rect.left() + rect.width() * time_frac;
        let marker = Rect::from_center_size(pos2(x, rect.center().y), vec2(4.0, 7.0));
        let col = match p.timer.pace(Some(mins)) {
            PaceState::Overtime => pal.error,
            PaceState::Warning => pal.warn,
            _ => pal.ok,
        };
        ui.painter().rect_filled(marker, CornerRadius::same(2), col);
    }
}

/// Top workspace-style header on `pal.app`: Live status + Sections left | Timer center | Utilities & clock right.
fn presenter_header(
    p: &mut Presentation,
    ui: &mut Ui,
    pal: &Palette,
    head: Rect,
) -> (Option<Rect>, Option<Rect>, Option<Rect>) {
    let mut target_anchor = None;
    let mut export_anchor = None;
    let mut sections_anchor = None;
    if head.width() < 48.0 || head.height() < 20.0 {
        return (None, None, None);
    }
    let compact = head.width() < 860.0;
    let total = p.total();
    let pad_x = 14.0_f32.min(head.width() * 0.15);
    let inner = head.shrink2(vec2(pad_x, 0.0));

    // Right zone: Wall clock (+ ETA), Fullscreen, Shortcuts (?), Save PDF.
    let now = chrono::Local::now();
    let clock = now.format("%H:%M").to_string();
    let show_eta = head.width() >= 1060.0;
    let eta = show_eta
        .then(|| {
            p.store.target_minutes.filter(|&m| m > 0).map(|mins| {
                let rem = (mins as f64 * 60.0 - p.timer.elapsed()).max(0.0) as i64;
                let end = now + chrono::Duration::seconds(rem);
                format!("Ends {}", end.format("%H:%M"))
            })
        })
        .flatten();
    let ann_count = p.store.annotated_slide_count();
    let show_save_text = head.width() >= 900.0;
    let right_w = (if show_save_text {
        if eta.is_some() { 320.0_f32 } else { 256.0_f32 }
    } else if head.width() >= 540.0 {
        172.0_f32
    } else {
        104.0_f32
    })
    .min(inner.width());
    let right_rect = Rect::from_min_max(pos2(inner.right() - right_w, inner.top()), inner.max);
    {
        let mut r_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(right_rect)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        r_ui.set_clip_rect(right_rect);
        r_ui.spacing_mut().item_spacing.x = 6.0;
        r_ui.label(
            RichText::new(&clock)
                .font(FontId::new(14.0, theme::font::mono_500()))
                .color(pal.ink),
        );
        if let Some(eta_text) = eta {
            r_ui.label(
                RichText::new(eta_text)
                    .font(FontId::monospace(11.5))
                    .color(pal.muted),
            );
        }
        if head.width() >= 540.0 {
            sep_v(&mut r_ui, pal, 16.0);
            if Button::icon(icons::MONITOR, "Toggle presenter fullscreen")
                .size(Size::Sm)
                .on(p.presenter_fullscreen)
                .show_icon(&mut r_ui, pal)
                .on_hover_text("Toggle presenter fullscreen")
                .clicked()
            {
                p.presenter_fullscreen = !p.presenter_fullscreen;
                p.presenter_fullscreen_dirty = true;
            }
            if Button::icon(icons::INFO, "Presenter shortcuts (?)")
                .size(Size::Sm)
                .on(p.shortcuts_open)
                .show_icon(&mut r_ui, pal)
                .on_hover_text("Presenter shortcuts (?)")
                .clicked()
            {
                p.shortcuts_open = !p.shortcuts_open;
                if p.shortcuts_open {
                    p.target_menu_open = false;
                    p.export_menu_open = false;
                    p.view_menu_open = false;
                    p.sections_menu_open = false;
                }
            }
            let save_lbl = if ann_count > 0 {
                format!("Save PDF ({ann_count})")
            } else {
                "Save PDF".into()
            };
            let save_active = p.export_menu_open || ann_count > 0;
            let sr = if show_save_text {
                Button::new(&save_lbl)
                    .with_icon(icons::DOWNLOAD)
                    .kind(if save_active {
                        Kind::Default
                    } else {
                        Kind::Ghost
                    })
                    .on(save_active)
                    .size(Size::Sm)
                    .icon_size(14.0)
                    .enabled(total > 0)
                    .show(&mut r_ui, pal)
            } else {
                Button::icon(icons::DOWNLOAD, "Save PDF")
                    .size(Size::Sm)
                    .on(save_active)
                    .enabled(total > 0)
                    .show_icon(&mut r_ui, pal)
            }
            .on_hover_text("Save slides with annotations and speaker notes as a PDF");
            export_anchor = Some(sr.rect);
            if sr.clicked() {
                p.export_menu_open = !p.export_menu_open;
                p.target_menu_open = false;
                p.view_menu_open = false;
                p.sections_menu_open = false;
            }
        }
    }

    // Center & Left geometry: mathematically disjoint at every viewport width.
    let center_w = if compact { 206.0 } else { 226.0 };
    let left_min_w = if compact { 82.0 } else { 126.0 };
    let gap = 8.0;
    let center_min_x = inner.left() + left_min_w + gap;
    let center_max_x = right_rect.left() - gap;
    let center_rect = if center_max_x - center_min_x >= center_w {
        let ideal_left = head.center().x - center_w * 0.5;
        let left = ideal_left.clamp(center_min_x, center_max_x - center_w);
        Some(Rect::from_min_size(
            pos2(left, inner.top()),
            vec2(center_w, inner.height()),
        ))
    } else {
        None
    };

    // Left zone: Live / Screen status chip + optional section title.
    let left_right = center_rect
        .map(|c| c.left() - gap)
        .unwrap_or(right_rect.left() - gap)
        .max(inner.left());
    let left_rect = Rect::from_min_max(inner.min, pos2(left_right, inner.bottom()));
    if left_rect.width() >= 48.0 {
        let mut l_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(left_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        l_ui.set_clip_rect(left_rect);
        l_ui.spacing_mut().item_spacing.x = 8.0;

        match p.screen {
            ScreenMode::Blackout => {
                if widgets::chip_button(
                    &mut l_ui,
                    pal,
                    Some(icons::MOON),
                    if compact {
                        "Blackout"
                    } else {
                        "Blackout · Resume"
                    },
                    "Resume from blackout",
                    ChipTone::Error,
                )
                .on_hover_text("Audience screen is black · click or press B to resume")
                .clicked()
                {
                    p.set_screen(ScreenMode::Normal);
                }
            }
            ScreenMode::Whiteout => {
                if widgets::chip_button(
                    &mut l_ui,
                    pal,
                    Some(icons::SUN),
                    if compact {
                        "Whiteout"
                    } else {
                        "Whiteout · Resume"
                    },
                    "Resume from whiteout",
                    ChipTone::Warn,
                )
                .on_hover_text("Audience screen is white · click or press W to resume")
                .clicked()
                {
                    p.set_screen(ScreenMode::Normal);
                }
            }
            ScreenMode::Normal => {
                if let Some(fp) = p.frozen_page {
                    if widgets::chip_button(
                        &mut l_ui,
                        pal,
                        Some(icons::PIN),
                        &format!("Frozen · {fp}"),
                        "Unfreeze audience slide",
                        ChipTone::Warn,
                    )
                    .on_hover_text(
                        "Audience is locked on this slide · click or press F to unfreeze",
                    )
                    .clicked()
                    {
                        p.frozen_page = None;
                    }
                } else {
                    live_status_chip(
                        &mut l_ui,
                        pal,
                        if left_rect.width() >= 132.0 {
                            "Live on stage"
                        } else {
                            "Live"
                        },
                    );
                }
            }
        }

        if left_rect.width() >= 210.0
            && p.frozen_page.is_none()
            && p.screen == ScreenMode::Normal
            && !p.sections.is_empty()
        {
            let sec_lbl = p
                .section_for_deck(p.page)
                .map(|s| {
                    let chars: Vec<char> = s.chars().collect();
                    if chars.len() > 22 {
                        let cut: String = chars[..21].iter().collect();
                        format!("{cut}…")
                    } else {
                        s.to_owned()
                    }
                })
                .unwrap_or_else(|| "Sections".into());
            let sr = Button::new(&sec_lbl)
                .trailing_icon(icons::CHEVRON_DOWN)
                .kind(if p.sections_menu_open {
                    Kind::Default
                } else {
                    Kind::Ghost
                })
                .on(p.sections_menu_open)
                .size(Size::Sm)
                .icon_size(12.5)
                .show(&mut l_ui, pal)
                .on_hover_text("Jump to section or appendix (Shift+← / Shift+→)");
            sections_anchor = Some(sr.rect);
            if sr.clicked() {
                p.sections_menu_open = !p.sections_menu_open;
                p.target_menu_open = false;
                p.export_menu_open = false;
                p.view_menu_open = false;
            }
        }
    }

    // Center zone: Timer (click to toggle Elapsed/Remaining), Start/Pause, Reset, Target.
    if let Some(center_rect) = center_rect {
        let mut m = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(center_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        m.set_clip_rect(center_rect);
        m.spacing_mut().item_spacing.x = 6.0;
        let pace = p.timer.pace(p.store.target_minutes);
        let timer_color = match pace {
            PaceState::Idle => pal.ink,
            PaceState::OnPace => pal.ink,
            PaceState::Warning => pal.warn,
            PaceState::Overtime => pal.error,
        };
        let timer_str = if p.store.countdown
            && let Some(mins) = p.store.target_minutes.filter(|&x| x > 0)
        {
            format_remaining(mins as f64 * 60.0 - p.timer.elapsed())
        } else {
            format_elapsed(p.timer.elapsed())
        };
        let t_resp = m
            .add(
                egui::Label::new(
                    RichText::new(timer_str)
                        .font(FontId::new(
                            if compact { 16.5 } else { 19.0 },
                            theme::font::mono_500(),
                        ))
                        .color(timer_color),
                )
                .sense(Sense::click()),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(if p.store.target_minutes.is_some() {
                "Click to toggle Elapsed / Remaining"
            } else {
                "Set a target duration to enable countdown"
            });
        if t_resp.clicked() && p.store.target_minutes.is_some() {
            p.store.countdown = !p.store.countdown;
            p.store_dirty = true;
        }
        m.add_space(2.0);
        let start_lbl = if p.timer.running {
            "Pause"
        } else if p.timer.started.is_some() {
            "Resume"
        } else {
            "Start"
        };
        if Button::new(start_lbl)
            .kind(if p.timer.running {
                Kind::Default
            } else {
                Kind::Primary
            })
            .size(Size::Sm)
            .show(&mut m, pal)
            .on_hover_text("Start or pause timer (T)")
            .clicked()
        {
            p.toggle_timer();
        }
        if Button::icon(icons::ROTATE_CCW, "Reset timer (R)")
            .size(Size::Sm)
            .show_icon(&mut m, pal)
            .on_hover_text("Reset timer (R)")
            .clicked()
        {
            p.reset_timer();
        }
        let target_lbl = match p.store.target_minutes {
            Some(mins) if mins > 0 => format!("{mins}m"),
            _ => "Target".into(),
        };
        let target_on = p.target_menu_open || p.store.target_minutes.is_some();
        let tr = Button::new(&target_lbl)
            .kind(if target_on {
                Kind::Default
            } else {
                Kind::Ghost
            })
            .on(target_on)
            .size(Size::Sm)
            .show(&mut m, pal)
            .on_hover_text("Set target talk duration for countdown & pacing");
        target_anchor = Some(tr.rect);
        if tr.clicked() {
            p.target_menu_open = !p.target_menu_open;
            p.export_menu_open = false;
            p.view_menu_open = false;
            p.sections_menu_open = false;
        }
    }

    (target_anchor, export_anchor, sections_anchor)
}

/// Customizable three-pane horizontal stage: [Speaker Notes] | Current Slide | [Next Slide].
/// Hiding Speaker Notes or Next Slide immediately expands Current Slide across the reclaimed space.
fn presenter_stage(p: &mut Presentation, ui: &mut Ui, pal: &Palette, main: Rect) {
    if main.width() < 24.0 || main.height() < 24.0 {
        return;
    }
    if let Some(e) = &p.error {
        ui.painter().text(
            main.left_top() + vec2(20.0, 20.0),
            Align2::LEFT_TOP,
            e,
            FontId::proportional(14.0),
            pal.error,
        );
        return;
    }

    let total = p.total();
    let cur_notes = match p.deck_item(p.page) {
        Some(DeckItem::Pdf(idx)) => p.page_regions(idx).1,
        _ => None,
    };

    // Hide side panels gracefully on extreme tiny viewports so the center slide always stays usable.
    let show_notes = p.notes_visible() && main.width() >= 480.0;
    let show_next = p.store.show_next && main.width() >= 420.0;
    let split_gap = 10.0_f32.min(main.width() * 0.08);
    let gaps = (if show_notes { 1 } else { 0 }) + (if show_next { 1 } else { 0 });
    let avail_w = (main.width() - split_gap * gaps as f32).max(1.0);
    let min_center = (avail_w * 0.36).min(300.0);

    let notes_w = if show_notes {
        let max_n = (avail_w - min_center - if show_next { 160.0 } else { 0.0 }).max(140.0);
        (avail_w * p.store.notes_width).clamp(170.0_f32.min(max_n), max_n)
    } else {
        0.0
    };
    let next_w = if show_next {
        let max_nx = (avail_w - notes_w - min_center).max(120.0);
        (avail_w * p.store.next_width).clamp(150.0_f32.min(max_nx), max_nx)
    } else {
        0.0
    };
    let cur_w = (avail_w - notes_w - next_w).max(1.0);

    let mut x = main.left();
    let (left_is_notes, left_w, right_w) = if p.store.notes_on_right {
        (false, next_w, notes_w)
    } else {
        (true, notes_w, next_w)
    };
    let show_left = left_w > 0.0;
    let show_right = right_w > 0.0;

    let left_rect = if show_left {
        let r = Rect::from_min_size(pos2(x, main.top()), vec2(left_w, main.height()));
        x = r.right();
        Some(r)
    } else {
        None
    };
    let left_split = if show_left {
        let r = Rect::from_min_size(pos2(x, main.top()), vec2(split_gap, main.height()));
        x = r.right();
        Some(r)
    } else {
        None
    };
    let cur_rect = Rect::from_min_size(pos2(x, main.top()), vec2(cur_w, main.height()));
    x = cur_rect.right();
    let right_split = if show_right {
        let r = Rect::from_min_size(pos2(x, main.top()), vec2(split_gap, main.height()));
        x = r.right();
        Some(r)
    } else {
        None
    };
    let right_rect = if show_right {
        Some(Rect::from_min_max(pos2(x, main.top()), main.max))
    } else {
        None
    };

    // Draggable left splitter.
    if let Some(l_split) = left_split {
        let resp = ui
            .interact(
                l_split,
                ui.scope_id().with("pres-left-split"),
                Sense::drag(),
            )
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        if resp.dragged() && avail_w > 320.0 {
            let next_frac = (left_w + resp.drag_delta().x) / avail_w;
            if left_is_notes {
                p.store.notes_width = next_frac.clamp(0.16, 0.40);
            } else {
                p.store.next_width = next_frac.clamp(0.14, 0.36);
            }
            p.store_dirty = true;
        }
        if resp.hovered() || resp.dragged() {
            let grip =
                Rect::from_center_size(l_split.center(), vec2(3.0, 44.0_f32.min(l_split.height())));
            ui.painter()
                .rect_filled(grip, CornerRadius::same(2), pal.accent);
        }
    }

    // Draggable right splitter.
    if let Some(r_split) = right_split {
        let resp = ui
            .interact(
                r_split,
                ui.scope_id().with("pres-right-split"),
                Sense::drag(),
            )
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        if resp.dragged() && avail_w > 320.0 {
            let next_frac = (right_w - resp.drag_delta().x) / avail_w;
            if left_is_notes {
                p.store.next_width = next_frac.clamp(0.14, 0.36);
            } else {
                p.store.notes_width = next_frac.clamp(0.16, 0.40);
            }
            p.store_dirty = true;
        }
        if resp.hovered() || resp.dragged() {
            let grip =
                Rect::from_center_size(r_split.center(), vec2(3.0, 44.0_f32.min(r_split.height())));
            ui.painter()
                .rect_filled(grip, CornerRadius::same(2), pal.accent);
        }
    }

    let (notes_rect, next_rect) = if left_is_notes {
        (left_rect, right_rect)
    } else {
        (right_rect, left_rect)
    };

    // Speaker Notes Card (when visible).
    if let Some(nr) = notes_rect {
        presenter_notes_card(p, ui, pal, nr, cur_notes, total);
    }

    // Center pane: Hero Current Slide Card.
    presenter_current_card(p, ui, pal, cur_rect, total);

    // Next Slide Preview Card (when visible).
    if let Some(nx) = next_rect {
        presenter_next_card(p, ui, pal, nx, total);
    }
}

fn presenter_current_card(
    p: &mut Presentation,
    ui: &mut Ui,
    pal: &Palette,
    card: Rect,
    total: usize,
) {
    if card.width() < 24.0 || card.height() < 24.0 {
        return;
    }
    widgets::paint_card(ui, pal, card);
    // Semantic alert outline around the stage card when the audience screen is blanked or frozen.
    let alert_stroke = match p.screen {
        ScreenMode::Blackout => Some(pal.error),
        ScreenMode::Whiteout => Some(pal.warn),
        ScreenMode::Normal if p.frozen_page.is_some() => Some(pal.warn),
        ScreenMode::Normal => None,
    };
    if let Some(col) = alert_stroke {
        ui.painter().rect_stroke(
            card,
            CornerRadius::same(theme::R_LG),
            Stroke::new(1.5, col.gamma_multiply(0.85)),
            StrokeKind::Inside,
        );
    }

    let wb_id = p.current_whiteboard_id();
    let head_h = 40.0_f32.min(card.height());
    let head = Rect::from_min_size(card.min, vec2(card.width(), head_h));
    if head_h >= 24.0 {
        ui.painter().hline(
            head.x_range(),
            head.bottom() - 0.5,
            Stroke::new(1.0, pal.border),
        );
        let pad_x = 12.0_f32.min(head.width() * 0.2);
        let mut sbar = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(head.shrink2(vec2(pad_x, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        sbar.set_clip_rect(head);
        sbar.spacing_mut().item_spacing.x = 6.0;
        if head.width() >= 280.0 {
            let label = if wb_id.is_some() {
                "WHITEBOARD"
            } else if p.frozen_page.is_some_and(|fp| fp != p.page) {
                "PREVIEW"
            } else {
                "CURRENT SLIDE"
            };
            widgets::label(&mut sbar, pal, label);
        }
        if let Some(id) = wb_id
            && head.width() >= 360.0
        {
            let grid_on = p.store.whiteboard(id).is_some_and(|w| w.grid);
            if Button::new("Grid")
                .with_icon(icons::TABLE)
                .kind(if grid_on { Kind::Default } else { Kind::Ghost })
                .on(grid_on)
                .size(Size::Sm)
                .icon_size(13.0)
                .show(&mut sbar, pal)
                .on_hover_text("Toggle drawing grid on this whiteboard")
                .clicked()
            {
                p.toggle_whiteboard_grid(id);
            }
            if Button::icon(icons::TRASH, "Remove whiteboard slide")
                .size(Size::Sm)
                .ink(pal.error)
                .show_icon(&mut sbar, pal)
                .on_hover_text("Remove this blank whiteboard slide")
                .clicked()
            {
                p.remove_whiteboard(id);
                return;
            }
        }
        if p.timer.running && head.width() >= 440.0 {
            let slide_secs = p.slide_elapsed(p.page);
            sbar.label(
                RichText::new(format!("· {}", format_elapsed(slide_secs)))
                    .font(FontId::new(11.5, theme::font::mono_500()))
                    .color(pal.muted),
            )
            .on_hover_text("Cumulative time spent on this slide");
        }
        if let Some(fp) = p.frozen_page
            && fp != p.page
            && head.width() >= 540.0
            && widgets::chip_button(
                &mut sbar,
                pal,
                Some(icons::PIN),
                "Push slide",
                "Show this slide to the audience while staying frozen",
                ChipTone::Warn,
            )
            .on_hover_text("Update the frozen audience screen to show this slide")
            .clicked()
        {
            p.frozen_page = Some(p.page);
        }

        // Right-aligned live stage tools: segmented track + ink swatches + Undo/Clear.
        sbar.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let icon_only_tools = head.width() < 620.0;
            stage_tools_track(p, ui, pal, icon_only_tools);

            let is_ink_tool = matches!(p.tool, StageTool::Pen | StageTool::Highlighter);
            if is_ink_tool && head.width() >= 380.0 {
                for color in PenColor::ALL.into_iter().rev() {
                    let active = p.pen_color == color;
                    let (cr, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                    let resp = resp
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(color.label());
                    if active {
                        ui.painter()
                            .circle_stroke(cr.center(), 9.0, Stroke::new(1.8, pal.ink));
                    }
                    ui.painter()
                        .circle_filled(cr.center(), 6.5, color.color32());
                    if resp.clicked() {
                        p.pen_color = color;
                    }
                }
            }
            let has_strokes = p.has_strokes_on_slide(p.page);
            if has_strokes && head.width() >= 300.0 {
                if Button::icon(icons::TRASH, "Clear annotations on this slide (C)")
                    .size(Size::Sm)
                    .show_icon(ui, pal)
                    .on_hover_text("Clear annotations on this slide (C)")
                    .clicked()
                {
                    p.clear_slide_strokes();
                }
                if Button::icon(icons::UNDO_2, "Undo last stroke (Ctrl+Z)")
                    .size(Size::Sm)
                    .show_icon(ui, pal)
                    .on_hover_text("Undo last stroke (Ctrl+Z)")
                    .clicked()
                {
                    p.undo_stroke();
                }
            }
        });
    }

    // Slide well in Lemmata's `pdf_ground` (matches the main PDF pane!).
    let body = Rect::from_min_max(
        pos2(card.left() + 1.0, head.bottom()),
        pos2(card.right() - 1.0, card.bottom() - 1.0),
    );
    if body.width() < 12.0 || body.height() < 12.0 {
        return;
    }
    let well_rad = CornerRadius {
        nw: 0,
        ne: 0,
        sw: theme::R_LG.saturating_sub(1),
        se: theme::R_LG.saturating_sub(1),
    };
    ui.painter().rect_filled(body, well_rad, pal.pdf_ground);

    let pad = 16.0_f32.min(body.width().min(body.height()) * 0.15);
    let slide_box = body.shrink(pad);
    if total > 0
        && slide_box.width() > 4.0
        && slide_box.height() > 4.0
        && let Some((sr, _)) = p.draw_deck_item(ui, slide_box, p.page)
    {
        p.interact_stage(ui, sr, p.page, "presenter-stage", true);
        p.paint_overlays(ui, sr, p.page, true);
    }

    // Quiet hint pill when a pointer or ink tool is armed and idle.
    let hint = match p.tool {
        StageTool::Laser if p.pointer_uv.is_none() => Some("Click & hold on slide to aim laser"),
        StageTool::Spotlight if p.pointer_uv.is_none() => {
            Some("Click & hold on slide to spotlight")
        }
        StageTool::Zoom if p.pointer_uv.is_none() => Some("Click & hold on slide to magnify"),
        StageTool::Pen | StageTool::Highlighter
            if p.active_stroke.is_none() && !p.has_strokes_on_slide(p.page) =>
        {
            Some("Drag to draw · Hold Shift for straight lines")
        }
        _ => None,
    };
    if let Some(hint) = hint
        && body.width() >= 280.0
        && body.height() >= 120.0
    {
        let g = ui
            .painter()
            .layout_no_wrap(hint.to_owned(), FontId::proportional(11.5), pal.ink);
        let pill = Rect::from_center_size(
            pos2(body.center().x, body.bottom() - 18.0),
            g.size() + vec2(20.0, 10.0),
        );
        ui.painter()
            .add(pal.shadow_card.as_shape(pill, CornerRadius::same(99)));
        ui.painter().rect(
            pill,
            CornerRadius::same(99),
            pal.surface,
            Stroke::new(1.0, pal.border_strong),
            StrokeKind::Inside,
        );
        ui.painter()
            .galley(pill.center() - g.size() * 0.5, g, pal.ink);
    }
}

/// Recessed segmented-style toggle track (`pal.surface_3`) for Laser, Spotlight, Zoom, Pen, and
/// Highlighter. Clicking an already active tool toggles it back off (`StageTool::None`).
fn stage_tools_track(p: &mut Presentation, ui: &mut Ui, pal: &Palette, icon_only: bool) {
    let tools = [
        (
            StageTool::Laser,
            icons::LOCATE,
            "Laser",
            "Click & hold on slide for synced red laser pointer (L)",
        ),
        (
            StageTool::Spotlight,
            icons::SUN,
            "Spotlight",
            "Click & hold on slide to spotlight a region (S)",
        ),
        (
            StageTool::Zoom,
            icons::ZOOM_IN,
            "Zoom",
            "Click & hold on slide to magnify plots or equations (Z)",
        ),
        (
            StageTool::Pen,
            icons::PENCIL,
            "Pen",
            "Draw ink annotations · hold Shift for straight lines (D)",
        ),
        (
            StageTool::Highlighter,
            icons::SQUARE_PEN,
            "Highlight",
            "Translucent highlighter · hold Shift for straight underlines (U)",
        ),
    ];
    let font = FontId::new(12.0, theme::font::sans_500());
    let galleys: Vec<_> = tools
        .iter()
        .map(|(_, _, lbl, _)| {
            let txt = if icon_only {
                String::new()
            } else {
                (*lbl).to_owned()
            };
            ui.painter()
                .layout_no_wrap(txt, font.clone(), Color32::PLACEHOLDER)
        })
        .collect();
    let widths: Vec<f32> = galleys
        .iter()
        .map(|g| {
            if icon_only {
                26.0
            } else {
                16.0 + 13.5 + 4.5 + g.size().x
            }
        })
        .collect();
    let total_w = widths.iter().sum::<f32>() + 2.0 * (tools.len() as f32 - 1.0) + 6.0;
    let (track, _) = ui.allocate_exact_size(vec2(total_w, 30.0), Sense::hover());
    let painter = ui.painter().clone();
    painter.rect_filled(track, CornerRadius::same(theme::R_MD), pal.surface_3);

    let mut x = track.left() + 3.0;
    let mut toggled = None;
    for (((tool, icon, lbl, tip), galley), w) in tools.iter().zip(galleys).zip(&widths) {
        let rect = Rect::from_min_size(pos2(x, track.top() + 3.0), vec2(*w, 24.0));
        let resp = ui
            .interact(
                rect,
                ui.scope_id().with(("stage-tool", *lbl)),
                Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(*tip);
        let on = p.tool == *tool;
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, *lbl));
        let hovered = resp.hovered();
        if on {
            painter.add(pal.shadow_card.as_shape(rect, CornerRadius::same(theme::R)));
            painter.rect(
                rect,
                CornerRadius::same(theme::R),
                pal.surface,
                Stroke::new(1.0, pal.card_hairline),
                StrokeKind::Inside,
            );
        } else if hovered {
            painter.rect_filled(rect, CornerRadius::same(theme::R), pal.surface_2);
        }
        let icon_col = if on {
            match tool {
                StageTool::Laser => Color32::from_rgb(0xe0, 0x3b, 0x3b),
                StageTool::Spotlight => pal.warn,
                StageTool::Zoom => pal.accent,
                StageTool::Pen | StageTool::Highlighter => p.pen_color.color32(),
                StageTool::None => pal.accent,
            }
        } else if hovered {
            pal.ink
        } else {
            pal.muted
        };
        let text_col = if on || hovered { pal.ink } else { pal.muted };
        if icon_only {
            icons::paint(
                &painter,
                Rect::from_center_size(rect.center(), vec2(13.5, 13.5)),
                icon,
                icon_col,
            );
        } else {
            let cx = rect.left() + 8.0;
            icons::paint(
                &painter,
                Rect::from_min_size(pos2(cx, rect.center().y - 6.75), vec2(13.5, 13.5)),
                icon,
                icon_col,
            );
            painter.galley(
                pos2(cx + 13.5 + 4.5, rect.center().y - galley.size().y * 0.5),
                galley,
                text_col,
            );
        }
        widgets::focus_ring(ui, &resp, rect, pal, theme::R);
        if resp.clicked() {
            toggled = Some(*tool);
        }
        x += w + 2.0;
    }
    if let Some(tool) = toggled {
        p.toggle_tool(tool);
    }
}

fn presenter_next_card(p: &mut Presentation, ui: &mut Ui, pal: &Palette, card: Rect, total: usize) {
    if card.width() < 24.0 || card.height() < 24.0 {
        return;
    }
    widgets::paint_card(ui, pal, card);
    let head_h = 40.0_f32.min(card.height());
    let head = Rect::from_min_size(card.min, vec2(card.width(), head_h));
    if head_h >= 24.0 {
        ui.painter().hline(
            head.x_range(),
            head.bottom() - 0.5,
            Stroke::new(1.0, pal.border),
        );
        let title = if p.page < total {
            match p.deck_item(p.page + 1) {
                Some(DeckItem::Whiteboard(_)) => "NEXT · WHITEBOARD".into(),
                _ => format!("NEXT · SLIDE {}", p.page + 1),
            }
        } else {
            "NEXT".into()
        };
        let mut h = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(head.shrink2(vec2(12.0_f32.min(head.width() * 0.2), 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        widgets::label(&mut h, pal, &title);
    }

    let body = Rect::from_min_max(pos2(card.left(), head.bottom()), card.max);
    let pad = 10.0_f32.min(body.width().min(body.height()) * 0.25);
    let inner = body.shrink(pad);
    if inner.width() < 16.0 || inner.height() < 16.0 {
        return;
    }
    if p.page < total {
        let well_h = (inner.width() * 0.78).clamp(80.0_f32.min(inner.height()), inner.height());
        let next_well = Rect::from_min_size(inner.min, vec2(inner.width(), well_h));
        let rad = CornerRadius::same(theme::R_MD);
        ui.painter().rect(
            next_well,
            rad,
            pal.pdf_ground,
            Stroke::new(1.0, pal.border),
            StrokeKind::Inside,
        );
        let slide_pad = 8.0_f32.min(next_well.width().min(next_well.height()) * 0.2);
        if let Some((nr, _)) = p.draw_deck_item(ui, next_well.shrink(slide_pad), p.page + 1) {
            p.paint_overlays(ui, nr, p.page + 1, false);
        }
        let nresp = ui
            .interact(
                next_well,
                ui.scope_id().with("next-slide-click"),
                Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Click to advance to next slide");
        if nresp.hovered() {
            ui.painter().rect_stroke(
                next_well,
                rad,
                Stroke::new(1.5, pal.accent),
                StrokeKind::Inside,
            );
        }
        if nresp.clicked() {
            p.go(p.page as f64 + 1.0);
        }
        let mut next_y = next_well.bottom() + 8.0;
        if let Some(sec) = p.section_for_deck(p.page + 1)
            && inner.bottom() - next_y >= 18.0
        {
            let mut job = egui::text::LayoutJob::simple_singleline(
                sec.to_owned(),
                FontId::new(12.0, theme::font::sans_500()),
                pal.ink_2,
            );
            job.wrap.max_width = inner.width();
            job.wrap.max_rows = 1;
            job.wrap.overflow_character = Some('…');
            let g = ui.fonts_mut(|f| f.layout_job(job));
            ui.painter()
                .galley(pos2(inner.left() + 2.0, next_y), g, pal.ink_2);
            next_y += 20.0;
        }
        // Preview up to 3 lines of the next slide's speaker notes for smooth transitions.
        if let Some(next_note) = p.effective_note(p.page + 1)
            && inner.bottom() - next_y >= 54.0
        {
            next_y += 6.0;
            ui.painter()
                .hline(inner.x_range(), next_y, Stroke::new(1.0, pal.border));
            next_y += 10.0;
            let preview_rect = Rect::from_min_max(pos2(inner.left(), next_y), inner.max);
            let mut pv = ui.new_child(egui::UiBuilder::new().max_rect(preview_rect));
            pv.set_clip_rect(preview_rect);
            pv.spacing_mut().item_spacing.y = 4.0;
            widgets::label(&mut pv, pal, "UP NEXT NOTES");
            let teaser: String = next_note
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .take(3)
                .collect::<Vec<_>>()
                .join("\n");
            pv.add(egui::Label::new(RichText::new(teaser).size(12.0).color(pal.muted)).wrap());
        }
    } else if total > 0 {
        let box_h = (inner.width() * 0.72).clamp(64.0_f32.min(inner.height()), inner.height());
        let empty = Rect::from_min_size(inner.min, vec2(inner.width(), box_h));
        ui.painter().rect(
            empty,
            CornerRadius::same(theme::R_MD),
            pal.surface_2,
            Stroke::new(1.0, pal.border_strong),
            StrokeKind::Inside,
        );
        ui.painter().text(
            empty.center(),
            Align2::CENTER_CENTER,
            "End of slides",
            FontId::proportional(12.5),
            pal.muted,
        );

        // Quiet talk wrap-up stats when the timer ran or annotations/notes were created.
        let ann_count = p.store.annotated_slide_count();
        let has_summary =
            p.timer.started.is_some() || ann_count > 0 || p.store.has_any_custom_notes();
        let summary_top = empty.bottom() + 14.0;
        if has_summary && inner.bottom() - summary_top >= 76.0 {
            let sum_rect = Rect::from_min_max(pos2(inner.left(), summary_top), inner.max);
            let mut su = ui.new_child(egui::UiBuilder::new().max_rect(sum_rect));
            su.set_clip_rect(sum_rect);
            su.spacing_mut().item_spacing.y = 6.0;
            widgets::label(&mut su, pal, "TALK SUMMARY");
            if p.timer.started.is_some() {
                su.label(
                    RichText::new(format!("Elapsed · {}", format_elapsed(p.timer.elapsed())))
                        .font(FontId::new(12.0, theme::font::mono_500()))
                        .color(pal.ink_2),
                );
                let slowest = p.slowest_slides(3);
                if !slowest.is_empty() {
                    su.add_space(4.0);
                    widgets::label(&mut su, pal, "LONGEST SLIDES");
                    for (slide, secs) in slowest {
                        su.label(
                            RichText::new(format!("Slide {slide} · {}", format_elapsed(secs)))
                                .font(FontId::new(11.5, theme::font::mono_500()))
                                .color(pal.muted),
                        );
                    }
                }
            }
            if ann_count > 0 {
                su.label(
                    RichText::new(format!(
                        "{ann_count} annotated {}",
                        if ann_count == 1 { "slide" } else { "slides" }
                    ))
                    .size(12.0)
                    .color(pal.muted),
                );
            }
            if (ann_count > 0 || p.store.has_any_custom_notes() || !p.store.whiteboards.is_empty())
                && Button::new("Export PDF…")
                    .with_icon(icons::DOWNLOAD)
                    .kind(Kind::Default)
                    .size(Size::Sm)
                    .icon_size(13.0)
                    .show(&mut su, pal)
                    .clicked()
            {
                p.export_menu_open = true;
                p.target_menu_open = false;
                p.view_menu_open = false;
                p.sections_menu_open = false;
            }
        }
    }
}

fn presenter_notes_card(
    p: &mut Presentation,
    ui: &mut Ui,
    pal: &Palette,
    card: Rect,
    cur_notes: Option<Region>,
    total: usize,
) {
    if card.width() < 24.0 || card.height() < 24.0 {
        return;
    }
    let cur_pdf_page = p.current_pdf_page();
    if cur_notes.is_some() || cur_pdf_page.is_none() {
        p.editing_notes = false;
    }
    widgets::paint_card(ui, pal, card);
    let head_h = 40.0_f32.min(card.height());
    let head = Rect::from_min_size(card.min, vec2(card.width(), head_h));
    ui.painter().hline(
        head.x_range(),
        head.bottom() - 0.5,
        Stroke::new(1.0, pal.border),
    );

    if head.width() > 36.0 && head.height() >= 22.0 {
        let pad_x = 12.0_f32.min(head.width() * 0.2);
        let mut h = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(head.shrink2(vec2(pad_x, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        h.set_clip_rect(head);
        h.spacing_mut().item_spacing.x = 6.0;
        widgets::label(&mut h, pal, "SPEAKER NOTES");
        if cur_notes.is_none()
            && head.width() >= 260.0
            && let Some(origin) = p.note_origin(p.page)
        {
            widgets::chip(&mut h, pal, None, origin, ChipTone::Neutral);
        }
        h.with_layout(egui::Layout::right_to_left(egui::Align::Center), |h| {
            h.spacing_mut().item_spacing.x = 4.0;
            if cur_notes.is_some() {
                if Button::new("Swap L/R")
                    .with_icon(icons::ARROW_UP_DOWN)
                    .kind(if p.store.swap_notes_half {
                        Kind::Default
                    } else {
                        Kind::Ghost
                    })
                    .on(p.store.swap_notes_half)
                    .size(Size::Sm)
                    .icon_size(13.0)
                    .show(h, pal)
                    .on_hover_text("Swap slide and Beamer notes halves (for second screen=left)")
                    .clicked()
                {
                    p.store.swap_notes_half = !p.store.swap_notes_half;
                    p.store_dirty = true;
                }
            } else if let Some(pdf_num) = cur_pdf_page {
                if Button::new(if p.editing_notes { "Done" } else { "Edit" })
                    .with_icon(if p.editing_notes {
                        icons::CHECK
                    } else {
                        icons::PENCIL
                    })
                    .kind(if p.editing_notes {
                        Kind::Primary
                    } else {
                        Kind::Default
                    })
                    .size(Size::Sm)
                    .icon_size(13.0)
                    .show(h, pal)
                    .on_hover_text(
                        "Edit speaker notes for this slide (Ctrl+Enter or Esc when done)",
                    )
                    .clicked()
                {
                    p.editing_notes = !p.editing_notes;
                    if p.editing_notes {
                        let fallback = p.tex_notes.get(&pdf_num).cloned().unwrap_or_default();
                        p.store.notes.entry(pdf_num).or_insert(fallback);
                    }
                }
                if p.editing_notes && p.can_reset_note_to_tex(p.page) {
                    if Button::new("Reset")
                        .kind(Kind::Ghost)
                        .size(Size::Sm)
                        .show(h, pal)
                        .on_hover_text("Revert to the note extracted from your .tex file")
                        .clicked()
                    {
                        p.store.notes.remove(&pdf_num);
                        p.editing_notes = false;
                        p.store_dirty = true;
                    }
                } else if head.width() >= 220.0 {
                    if Button::new("A+")
                        .kind(Kind::Ghost)
                        .size(Size::Sm)
                        .show(h, pal)
                        .on_hover_text("Larger notes text")
                        .clicked()
                    {
                        p.store.notes_font_size = (p.store.notes_font_size + 2.0).min(28.0);
                        p.store_dirty = true;
                    }
                    if Button::new("A−")
                        .kind(Kind::Ghost)
                        .size(Size::Sm)
                        .show(h, pal)
                        .on_hover_text("Smaller notes text")
                        .clicked()
                    {
                        p.store.notes_font_size = (p.store.notes_font_size - 2.0).max(13.0);
                        p.store_dirty = true;
                    }
                }
            }
        });
    }

    let body = Rect::from_min_max(pos2(card.left(), head.bottom()), card.max);
    if body.width() < 24.0 || body.height() < 20.0 {
        return;
    }
    if let Some(region) = cur_notes
        && let Some(pdf_num) = cur_pdf_page
        && total > 0
    {
        let well = body.shrink(8.0_f32.min(body.width().min(body.height()) * 0.25));
        ui.painter()
            .rect_filled(well, CornerRadius::same(theme::R_MD), pal.pdf_ground);
        let inner = well.shrink(6.0_f32.min(well.width().min(well.height()) * 0.25));
        p.draw(ui, inner, pdf_num - 1, region);
        return;
    }

    let pad = 14.0_f32.min(body.width().min(body.height()) * 0.25);
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(body.shrink(pad)));
    let Some(pdf_num) = cur_pdf_page else {
        c.add(
            egui::Label::new(
                RichText::new(
                    "Blank whiteboard slide. Draw with Pen (D) or Highlighter (U), hold Shift for straight lines, and toggle Grid in the stage header.",
                )
                .size(13.0)
                .color(pal.muted),
            )
            .wrap(),
        );
        return;
    };
    let font_size = p.store.notes_font_size;
    if p.editing_notes {
        let fallback = p.tex_notes.get(&pdf_num).cloned().unwrap_or_default();
        let entry = p.store.notes.entry(pdf_num).or_insert(fallback);
        let avail = c.available_size();
        egui::ScrollArea::vertical()
            .id_salt(("notes-edit", pdf_num))
            .show(&mut c, |ui| {
                let r = ui.add_sized(
                    vec2(avail.x.max(1.0), (avail.y - 4.0).max(40.0)),
                    egui::TextEdit::multiline(entry)
                        .font(FontId::proportional(font_size.min(18.0)))
                        .text_color(pal.ink)
                        .hint_text(
                            "Type talking points (# headings, - bullets)… Ctrl+Enter or Esc when done",
                        ),
                );
                if r.changed() {
                    p.store_dirty = true;
                    p.dirty_for_export = true;
                }
            });
    } else {
        let note = p.effective_note(p.page).map(ToOwned::to_owned);
        let read_click = ui.interact(
            body.shrink(pad),
            ui.scope_id().with(("notes-dblclick", pdf_num)),
            Sense::click(),
        );
        if read_click.double_clicked() {
            p.editing_notes = true;
            let fallback = p.tex_notes.get(&pdf_num).cloned().unwrap_or_default();
            p.store.notes.entry(pdf_num).or_insert(fallback);
        }
        egui::ScrollArea::vertical()
            .id_salt(("notes-read", p.page))
            .auto_shrink([false, false])
            .show(&mut c, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                if let Some(text) = note {
                    for line in text.lines() {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            ui.add_space(4.0);
                        } else if let Some(heading) = trimmed
                            .strip_prefix("## ")
                            .or_else(|| trimmed.strip_prefix("# "))
                        {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(heading)
                                        .font(FontId::new(
                                            (font_size + 1.5).min(30.0),
                                            theme::font::sans_600(),
                                        ))
                                        .color(pal.ink),
                                )
                                .wrap(),
                            );
                        } else if let Some(item) = trimmed
                            .strip_prefix("- ")
                            .or_else(|| trimmed.strip_prefix("* "))
                        {
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                ui.label(
                                    RichText::new("•")
                                        .size(font_size)
                                        .color(pal.accent),
                                );
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(item).size(font_size).color(pal.ink),
                                    )
                                    .wrap(),
                                );
                            });
                        } else {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(trimmed).size(font_size).color(pal.ink),
                                )
                                .wrap(),
                            );
                        }
                    }
                } else {
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                "No speaker notes for this slide yet. Click Edit (or double-click here) to add talking points, or compile Beamer notes with second screen enabled.",
                            )
                            .size(13.0)
                            .color(pal.muted),
                        )
                        .wrap(),
                    );
                }
            });
    }
}

fn presenter_filmstrip(p: &mut Presentation, ui: &mut Ui, pal: &Palette, rect: Rect) {
    ui.painter().rect_filled(rect, 0, pal.surface);
    ui.painter().hline(
        rect.x_range(),
        rect.top() + 0.5,
        Stroke::new(1.0, pal.border),
    );
    let total = p.total();
    let pad_x = 12.0_f32.min(rect.width() * 0.15);
    let pad_y = 7.0_f32.min(rect.height() * 0.15);
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(pad_x, pad_y))));
    let mut picked = None;
    egui::ScrollArea::horizontal()
        .id_salt("presenter-filmstrip")
        .auto_shrink([false, false])
        .show(&mut c, |ui| {
            let gap = 10.0;
            let thumb_h = 58.0;
            let thumb_w = 94.0;
            let total_w = total as f32 * (thumb_w + gap);
            let (strip_rect, _) =
                ui.allocate_exact_size(vec2(total_w, thumb_h + 18.0), Sense::hover());
            let deck = p.deck_items();
            let scroll_to_active = std::mem::take(&mut p.filmstrip_scroll_pending);
            for (i, item) in deck.iter().copied().enumerate() {
                let slide_num = i + 1;
                let cell = Rect::from_min_size(
                    pos2(
                        strip_rect.left() + i as f32 * (thumb_w + gap),
                        strip_rect.top(),
                    ),
                    vec2(thumb_w, thumb_h + 18.0),
                );
                let is_cur = slide_num == p.page;
                if scroll_to_active && is_cur {
                    ui.scroll_to_rect(cell, Some(egui::Align::Center));
                }
                if !ui.is_rect_visible(cell) {
                    continue;
                }
                let thumb = Rect::from_min_size(cell.min, vec2(thumb_w, thumb_h));
                let resp = ui
                    .interact(cell, ui.scope_id().with(("fs", i)), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let is_frz = p.frozen_page == Some(slide_num);
                let rad = CornerRadius::same(theme::R);
                ui.painter().rect_filled(thumb, rad, pal.pdf_ground);
                if let Some((sr, _)) = p.draw_deck_item(ui, thumb.shrink(3.0), slide_num) {
                    p.paint_overlays(ui, sr, slide_num, false);
                }
                let stroke = if is_cur {
                    Stroke::new(2.0, pal.accent)
                } else if is_frz {
                    Stroke::new(2.0, pal.warn)
                } else if resp.hovered() {
                    Stroke::new(1.5, pal.border_strong)
                } else {
                    Stroke::new(1.0, pal.border)
                };
                ui.painter()
                    .rect_stroke(thumb, rad, stroke, StrokeKind::Inside);
                let has_ink = p.has_strokes_on_slide(slide_num);
                let has_note = p.effective_note(slide_num).is_some();
                let lbl = match item {
                    DeckItem::Whiteboard(_) => format!("{slide_num} · W"),
                    DeckItem::Pdf(_) => format!("{slide_num}"),
                };
                ui.painter().text(
                    pos2(cell.center().x, thumb.bottom() + 3.0),
                    Align2::CENTER_TOP,
                    lbl,
                    FontId::new(11.0, theme::font::mono_500()),
                    if is_cur { pal.accent_text } else { pal.muted },
                );
                let mut dot_x = thumb.right() - 6.0;
                if has_ink {
                    ui.painter()
                        .circle_filled(pos2(dot_x, thumb.top() + 6.0), 3.5, pal.error);
                    dot_x -= 9.0;
                }
                if has_note {
                    ui.painter()
                        .circle_filled(pos2(dot_x, thumb.top() + 6.0), 3.0, pal.accent);
                }
                if resp.clicked() {
                    picked = Some(slide_num);
                }
            }
        });
    if let Some(s) = picked {
        p.go(s as f64);
    }
}

/// Three-zone Pitch-inspired transport footer on `pal.surface`:
/// Left: View popover + Overview + audience screen toggles | Center: < 1 / N > | Right: End presentation.
fn presenter_footer(
    p: &mut Presentation,
    ui: &mut Ui,
    pal: &Palette,
    foot: Rect,
    keep: &mut bool,
) -> Option<Rect> {
    let mut view_anchor = None;
    ui.painter().rect_filled(foot, 0, pal.surface);
    ui.painter().hline(
        foot.x_range(),
        foot.top() + 0.5,
        Stroke::new(1.0, pal.border),
    );
    if foot.width() < 48.0 || foot.height() < 20.0 {
        return None;
    }

    let total = p.total();
    let pad_x = 14.0_f32.min(foot.width() * 0.15);
    let inner = foot.shrink2(vec2(pad_x, 0.0));
    let show_end_full = foot.width() >= 480.0;
    let right_w = (if show_end_full { 142.0 } else { 64.0_f32 }).min(inner.width());
    let right_rect = Rect::from_min_max(pos2(inner.right() - right_w, inner.top()), inner.max);

    // Right zone: End presentation.
    {
        let mut r_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(right_rect)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        r_ui.set_clip_rect(right_rect);
        if end_presentation_btn(&mut r_ui, pal, !show_end_full).clicked() {
            *keep = false;
        }
    }

    // Center transport zone: <  1 / N  >  [+ blank slide] (mathematically disjoint from left and right).
    let show_blank_btn = foot.width() >= 760.0;
    let center_w = if show_blank_btn { 184.0 } else { 148.0 };
    let left_min_w = 72.0;
    let gap = 8.0;
    let center_min_x = inner.left() + left_min_w + gap;
    let center_max_x = right_rect.left() - gap;
    let center_rect = if center_max_x - center_min_x >= center_w {
        let ideal_left = foot.center().x - center_w * 0.5;
        let left = ideal_left.clamp(center_min_x, center_max_x - center_w);
        Some(Rect::from_min_size(
            pos2(left, inner.top()),
            vec2(center_w, inner.height()),
        ))
    } else {
        None
    };

    if let Some(cr) = center_rect {
        let mut c = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(cr)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        c.set_clip_rect(cr);
        c.spacing_mut().item_spacing.x = 6.0;
        let content_w = if show_blank_btn { 172.0 } else { 134.0 };
        let pad_left = ((cr.width() - content_w) * 0.5).max(0.0);
        if pad_left > 0.0 {
            c.add_space(pad_left);
        }
        if Button::icon(icons::CHEVRON_LEFT, "Back")
            .kind(Kind::Default)
            .size(Size::Sm)
            .enabled(p.page > 1)
            .show_icon(&mut c, pal)
            .on_hover_text("Previous slide (←)")
            .clicked()
        {
            p.go(p.page as f64 - 1.0);
        }
        if p.editing_slide_jump {
            let resp = c.add(
                egui::TextEdit::singleline(&mut p.slide_jump_text)
                    .desired_width(52.0)
                    .font(FontId::new(13.0, theme::font::mono_500()))
                    .hint_text("1.."),
            );
            resp.request_focus();
            let commit = c.input(|i| i.key_pressed(egui::Key::Enter)) || resp.lost_focus();
            if commit {
                if let Ok(n) = p.slide_jump_text.trim().parse::<usize>() {
                    p.go(n as f64);
                }
                p.editing_slide_jump = false;
            }
        } else {
            let slide_lbl = format!(
                "{} / {}",
                p.page,
                if total > 0 {
                    total.to_string()
                } else {
                    "…".into()
                }
            );
            if slide_counter_pill(&mut c, pal, &slide_lbl, total == 0)
                .on_hover_text("Click or type slide number + Enter to jump")
                .clicked()
            {
                p.editing_slide_jump = true;
                p.slide_jump_text = p.page.to_string();
            }
        }
        if Button::icon(icons::CHEVRON_RIGHT, "Next")
            .kind(Kind::Default)
            .size(Size::Sm)
            .enabled(p.page < total)
            .show_icon(&mut c, pal)
            .on_hover_text("Next slide (→)")
            .clicked()
        {
            p.go(p.page as f64 + 1.0);
        }
        if show_blank_btn {
            c.add_space(4.0);
            if Button::icon(icons::FILE_PLUS, "Add blank whiteboard slide (K)")
                .kind(Kind::Ghost)
                .size(Size::Sm)
                .show_icon(&mut c, pal)
                .on_hover_text("Insert a blank drawable whiteboard slide after this slide (K)")
                .clicked()
            {
                p.insert_whiteboard_after_current();
            }
        }
    }

    // Left zone: View popover + Overview + Blackout / Whiteout / Freeze.
    let left_right = center_rect
        .map(|c| c.left() - gap)
        .unwrap_or(right_rect.left() - gap)
        .max(inner.left());
    let left_rect = Rect::from_min_max(inner.min, pos2(left_right, inner.bottom()));
    if left_rect.width() >= 36.0 {
        let mut l = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(left_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        l.set_clip_rect(left_rect);
        l.spacing_mut().item_spacing.x = 4.0;

        let show_view_text = left_rect.width() >= 110.0;
        let vr = if show_view_text {
            Button::new("View")
                .with_icon(icons::LAYOUT_TEMPLATE)
                .kind(if p.view_menu_open {
                    Kind::Default
                } else {
                    Kind::Ghost
                })
                .on(p.view_menu_open)
                .size(Size::Sm)
                .icon_size(14.0)
                .show(&mut l, pal)
        } else {
            Button::icon(icons::LAYOUT_TEMPLATE, "View")
                .size(Size::Sm)
                .on(p.view_menu_open)
                .show_icon(&mut l, pal)
        }
        .on_hover_text("Customize visible panels (Speaker notes, Next slide, Filmstrip)");
        view_anchor = Some(vr.rect);
        if vr.clicked() {
            p.view_menu_open = !p.view_menu_open;
            p.target_menu_open = false;
            p.export_menu_open = false;
            p.sections_menu_open = false;
        }

        if left_rect.width() >= 148.0 {
            let show_ov_text = left_rect.width() >= 250.0;
            let ov_resp = if show_ov_text {
                Button::new("Overview")
                    .with_icon(icons::LAYOUT_GRID)
                    .kind(if p.overview {
                        Kind::Default
                    } else {
                        Kind::Ghost
                    })
                    .on(p.overview)
                    .size(Size::Sm)
                    .icon_size(14.0)
                    .enabled(total > 0)
                    .show(&mut l, pal)
            } else {
                Button::icon(icons::LAYOUT_GRID, "Overview")
                    .size(Size::Sm)
                    .on(p.overview)
                    .enabled(total > 0)
                    .show_icon(&mut l, pal)
            }
            .on_hover_text("Toggle slide overview grid (G)");
            if ov_resp.clicked() {
                p.overview = !p.overview;
                if p.overview {
                    p.editing_notes = false;
                    p.editing_slide_jump = false;
                }
            }
        }

        if left_rect.width() >= 210.0 {
            sep_v(&mut l, pal, 16.0);
            let show_screen_labels = left_rect.width() >= 430.0;

            let blackout_on = p.screen == ScreenMode::Blackout;
            let black_btn = if show_screen_labels {
                let mut b = Button::new(if blackout_on { "Resume" } else { "Blackout" })
                    .with_icon(icons::MOON)
                    .kind(Kind::Ghost)
                    .on(blackout_on)
                    .size(Size::Sm)
                    .icon_size(14.0);
                if blackout_on {
                    b = b.fill(pal.error_soft).ink(pal.error);
                }
                b.show(&mut l, pal)
            } else {
                let mut b = Button::icon(icons::MOON, "Blackout")
                    .size(Size::Sm)
                    .on(blackout_on);
                if blackout_on {
                    b = b.fill(pal.error_soft).ink(pal.error);
                }
                b.show_icon(&mut l, pal)
            }
            .on_hover_text("Toggle black audience screen (B or .)");
            if black_btn.clicked() {
                p.toggle_blackout();
            }

            let whiteout_on = p.screen == ScreenMode::Whiteout;
            let white_btn = if show_screen_labels {
                let mut b = Button::new("Whiteout")
                    .with_icon(icons::SUN)
                    .kind(Kind::Ghost)
                    .on(whiteout_on)
                    .size(Size::Sm)
                    .icon_size(14.0);
                if whiteout_on {
                    b = b.fill(pal.warn_soft).ink(pal.warn);
                }
                b.show(&mut l, pal)
            } else {
                let mut b = Button::icon(icons::SUN, "Whiteout")
                    .size(Size::Sm)
                    .on(whiteout_on);
                if whiteout_on {
                    b = b.fill(pal.warn_soft).ink(pal.warn);
                }
                b.show_icon(&mut l, pal)
            }
            .on_hover_text("Toggle white audience screen (W)");
            if white_btn.clicked() {
                p.toggle_whiteout();
            }

            let freeze_on = p.frozen_page.is_some();
            let freeze_btn = if show_screen_labels {
                let mut b = Button::new(if freeze_on { "Unfreeze" } else { "Freeze" })
                    .with_icon(icons::PIN)
                    .kind(Kind::Ghost)
                    .on(freeze_on)
                    .size(Size::Sm)
                    .icon_size(14.0)
                    .enabled(total > 0);
                if freeze_on {
                    b = b.fill(pal.warn_soft).ink(pal.warn);
                }
                b.show(&mut l, pal)
            } else {
                let mut b = Button::icon(icons::PIN, "Freeze")
                    .size(Size::Sm)
                    .on(freeze_on)
                    .enabled(total > 0);
                if freeze_on {
                    b = b.fill(pal.warn_soft).ink(pal.warn);
                }
                b.show_icon(&mut l, pal)
            }
            .on_hover_text("Hold audience on current slide while browsing freely (F)");
            if freeze_btn.clicked() {
                p.toggle_freeze();
            }
        }
    }

    view_anchor
}

/// Pitch-inspired `View` popover anchored above the bottom-left footer button.
/// Lets the presenter toggle Speaker notes, Next slide, Filmstrip, side placement, and dark stage.
fn view_popover(
    p: &mut Presentation,
    ctx: &egui::Context,
    pal: &Palette,
    app_is_dark: bool,
    bounds: Rect,
    anchor: Rect,
) {
    let margin = 12.0_f32.min(bounds.width().min(bounds.height()) * 0.25);
    let custom_widths =
        (p.store.notes_width - 0.24).abs() > 0.01 || (p.store.next_width - 0.20).abs() > 0.01;
    let show_dark_toggle = !app_is_dark || p.store.force_dark;
    let base_h = 208.0_f32
        + if show_dark_toggle { 30.0 } else { 0.0 }
        + if custom_widths { 34.0 } else { 0.0 };
    let pop_w = 258.0_f32.min((bounds.width() - margin * 2.0).max(1.0));
    let pop_h = base_h.min((bounds.height() - margin * 2.0).max(1.0));
    let min_x = bounds.left() + margin;
    let max_x = (bounds.right() - pop_w - margin).max(min_x);
    let min_y = bounds.top() + margin;
    let max_y = (bounds.bottom() - pop_h - margin).max(min_y);
    let pop = Rect::from_min_size(
        pos2(
            anchor.left().clamp(min_x, max_x),
            (anchor.top() - pop_h - 8.0).clamp(min_y, max_y),
        ),
        vec2(pop_w, pop_h),
    );
    egui::Area::new(egui::Id::unique("presenter-view-popover"))
        .order(egui::Order::Foreground)
        .fixed_pos(bounds.min)
        .show(ctx, |ui| {
            let (_, scrim) = ui.allocate_exact_size(bounds.size(), Sense::click());
            let _card_blocker =
                ui.interact(pop, ui.scope_id().with("view-pop-card"), Sense::click());
            let rad = CornerRadius::same(theme::R_LG);
            ui.painter().add(pal.shadow.as_shape(pop, rad));
            ui.painter().rect(
                pop,
                rad,
                pal.surface,
                Stroke::new(1.0, pal.border),
                StrokeKind::Inside,
            );
            if pop.width() > 36.0 && pop.height() > 36.0 {
                let inner = pop.shrink(12.0);
                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                c.set_clip_rect(inner);
                c.spacing_mut().item_spacing.y = 8.0;
                widgets::label(&mut c, pal, "LAYOUT & APPEARANCE");

                let mut notes_on = p.notes_visible();
                if view_switch_row(
                    &mut c,
                    pal,
                    icons::SCROLL_TEXT,
                    "Speaker notes",
                    Some("N"),
                    &mut notes_on,
                ) {
                    p.store.show_notes = Some(notes_on);
                    if !notes_on {
                        p.editing_notes = false;
                    }
                    p.store_dirty = true;
                }

                let mut next_on = p.store.show_next;
                if view_switch_row(
                    &mut c,
                    pal,
                    icons::PRESENTATION,
                    "Next slide",
                    None,
                    &mut next_on,
                ) {
                    p.store.show_next = next_on;
                    p.store_dirty = true;
                }

                let mut film_on = p.store.filmstrip;
                if view_switch_row(
                    &mut c,
                    pal,
                    icons::COLUMNS_2,
                    "Filmstrip",
                    Some("M"),
                    &mut film_on,
                ) {
                    p.store.filmstrip = film_on;
                    p.store_dirty = true;
                }

                let mut on_right = p.store.notes_on_right;
                if view_switch_row(
                    &mut c,
                    pal,
                    icons::PANEL_LEFT,
                    "Notes on right",
                    None,
                    &mut on_right,
                ) {
                    p.store.notes_on_right = on_right;
                    p.store_dirty = true;
                }

                if show_dark_toggle {
                    let mut dark_on = p.store.force_dark;
                    if view_switch_row(&mut c, pal, icons::MOON, "Dark stage", None, &mut dark_on) {
                        p.store.force_dark = dark_on;
                        p.store_dirty = true;
                    }
                }

                if custom_widths {
                    c.add_space(2.0);
                    if Button::new("Reset panel widths")
                        .kind(Kind::Ghost)
                        .size(Size::Sm)
                        .min_width(c.available_width())
                        .show(&mut c, pal)
                        .clicked()
                    {
                        p.store.notes_width = 0.24;
                        p.store.next_width = 0.20;
                        p.store_dirty = true;
                    }
                }

                c.add_space(2.0);
                if Button::new("Add blank whiteboard (K)")
                    .with_icon(icons::FILE_PLUS)
                    .kind(Kind::Default)
                    .size(Size::Sm)
                    .icon_size(13.5)
                    .min_width(c.available_width())
                    .show(&mut c, pal)
                    .clicked()
                {
                    p.insert_whiteboard_after_current();
                }
            }
            if scrim.clicked() {
                p.view_menu_open = false;
            }
        });
}

/// Quick-jump popover listing every PDF outline section/appendix with its slide number.
fn sections_popover(
    p: &mut Presentation,
    ctx: &egui::Context,
    pal: &Palette,
    bounds: Rect,
    anchor: Rect,
) {
    let items: Vec<(usize, String)> = p
        .deck_sections()
        .into_iter()
        .map(|(s, t)| (s, t.to_owned()))
        .collect();
    if items.is_empty() {
        p.sections_menu_open = false;
        return;
    }
    let margin = 12.0_f32.min(bounds.width().min(bounds.height()) * 0.25);
    let pop_w = 280.0_f32.min((bounds.width() - margin * 2.0).max(1.0));
    let desired_h = (52.0 + items.len() as f32 * 34.0).min(340.0);
    let pop_h = desired_h.min((bounds.height() - margin * 2.0).max(1.0));
    let min_x = bounds.left() + margin;
    let max_x = (bounds.right() - pop_w - margin).max(min_x);
    let min_y = bounds.top() + margin;
    let max_y = (bounds.bottom() - pop_h - margin).max(min_y);
    let pop = Rect::from_min_size(
        pos2(
            anchor.left().clamp(min_x, max_x),
            (anchor.bottom() + 8.0).clamp(min_y, max_y),
        ),
        vec2(pop_w, pop_h),
    );
    egui::Area::new(egui::Id::unique("presenter-sections-popover"))
        .order(egui::Order::Foreground)
        .fixed_pos(bounds.min)
        .show(ctx, |ui| {
            let (_, scrim) = ui.allocate_exact_size(bounds.size(), Sense::click());
            let _card_blocker =
                ui.interact(pop, ui.scope_id().with("sections-pop-card"), Sense::click());
            let rad = CornerRadius::same(theme::R_LG);
            ui.painter().add(pal.shadow.as_shape(pop, rad));
            ui.painter().rect(
                pop,
                rad,
                pal.surface,
                Stroke::new(1.0, pal.border),
                StrokeKind::Inside,
            );
            if pop.width() > 36.0 && pop.height() > 36.0 {
                let inner = pop.shrink(12.0);
                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                c.set_clip_rect(inner);
                c.spacing_mut().item_spacing.y = 6.0;
                widgets::label(&mut c, pal, "JUMP TO SECTION");
                let active_sec = p.section_for_deck(p.page).map(ToOwned::to_owned);
                let mut jump = None;
                egui::ScrollArea::vertical()
                    .id_salt("presenter-sections-scroll")
                    .auto_shrink([false, false])
                    .show(&mut c, |ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        for (slide, title) in &items {
                            let is_cur = active_sec.as_deref() == Some(title.as_str());
                            let trail = format!("{slide}");
                            if widgets::side_row(
                                ui,
                                pal,
                                Some(icons::LIST_TREE),
                                None,
                                title,
                                is_cur,
                                Some(&trail),
                            )
                            .clicked()
                            {
                                jump = Some(*slide);
                            }
                        }
                    });
                if let Some(s) = jump {
                    p.go(s as f64);
                    p.sections_menu_open = false;
                }
            }
            if scrim.clicked() {
                p.sections_menu_open = false;
            }
        });
}

fn view_switch_row(
    ui: &mut Ui,
    pal: &Palette,
    icon: Icon,
    label: &str,
    shortcut: Option<&str>,
    value: &mut bool,
) -> bool {
    let before = *value;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        icons::show(ui, icon, 14.5, pal.muted);
        ui.label(
            RichText::new(label)
                .font(FontId::new(13.0, theme::font::sans_500()))
                .color(pal.ink),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            widgets::switch(ui, pal, value, label);
            if let Some(k) = shortcut {
                ui.label(
                    RichText::new(k)
                        .font(FontId::new(11.0, theme::font::mono_500()))
                        .color(pal.faint),
                );
            }
        });
    });
    *value != before
}

fn target_duration_popover(
    p: &mut Presentation,
    ctx: &egui::Context,
    pal: &Palette,
    bounds: Rect,
    anchor: Rect,
) {
    let margin = 12.0_f32.min(bounds.width().min(bounds.height()) * 0.25);
    let pop_w = 256.0_f32.min((bounds.width() - margin * 2.0).max(1.0));
    let pop_h = 156.0_f32.min((bounds.height() - margin * 2.0).max(1.0));
    let min_x = bounds.left() + margin;
    let max_x = (bounds.right() - pop_w - margin).max(min_x);
    let min_y = bounds.top() + margin;
    let max_y = (bounds.bottom() - pop_h - margin).max(min_y);
    let pop = Rect::from_min_size(
        pos2(
            (anchor.center().x - pop_w * 0.5).clamp(min_x, max_x),
            (anchor.bottom() + 8.0).clamp(min_y, max_y),
        ),
        vec2(pop_w, pop_h),
    );
    egui::Area::new(egui::Id::unique("presenter-target-popover"))
        .order(egui::Order::Foreground)
        .fixed_pos(bounds.min)
        .show(ctx, |ui| {
            let (_, scrim) = ui.allocate_exact_size(bounds.size(), Sense::click());
            let _card_blocker =
                ui.interact(pop, ui.scope_id().with("target-pop-card"), Sense::click());
            let rad = CornerRadius::same(theme::R_LG);
            ui.painter().add(pal.shadow.as_shape(pop, rad));
            ui.painter().rect(
                pop,
                rad,
                pal.surface,
                Stroke::new(1.0, pal.border),
                StrokeKind::Inside,
            );
            if pop.width() > 28.0 && pop.height() > 28.0 {
                let inner = pop.shrink(12.0);
                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                c.set_clip_rect(inner);
                c.spacing_mut().item_spacing.y = 8.0;
                widgets::label(&mut c, pal, "TARGET TALK DURATION");
                c.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
                    for (lbl, val) in [
                        ("Off", None),
                        ("5m", Some(5)),
                        ("10m", Some(10)),
                        ("15m", Some(15)),
                        ("20m", Some(20)),
                        ("30m", Some(30)),
                        ("45m", Some(45)),
                        ("60m", Some(60)),
                    ] {
                        let active = p.store.target_minutes == val;
                        if Button::new(lbl)
                            .kind(Kind::Default)
                            .on(active)
                            .size(Size::Sm)
                            .show(ui, pal)
                            .clicked()
                        {
                            p.store.target_minutes = val;
                            if val.is_some() {
                                p.store.countdown = true;
                            }
                            p.store_dirty = true;
                            p.target_menu_open = false;
                        }
                    }
                });
                c.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let cur = p.store.target_minutes.unwrap_or(15);
                    if Button::new("−1m")
                        .kind(Kind::Ghost)
                        .size(Size::Sm)
                        .show(ui, pal)
                        .clicked()
                    {
                        p.store.target_minutes = Some(cur.saturating_sub(1).max(1));
                        p.store.countdown = true;
                        p.store_dirty = true;
                    }
                    if Button::new("+1m")
                        .kind(Kind::Ghost)
                        .size(Size::Sm)
                        .show(ui, pal)
                        .clicked()
                    {
                        p.store.target_minutes = Some((cur + 1).min(240));
                        p.store.countdown = true;
                        p.store_dirty = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mode_lbl = if p.store.countdown {
                            "Remaining"
                        } else {
                            "Elapsed"
                        };
                        if Button::new(mode_lbl)
                            .kind(Kind::Ghost)
                            .size(Size::Sm)
                            .show(ui, pal)
                            .clicked()
                        {
                            p.store.countdown = !p.store.countdown;
                            p.store_dirty = true;
                        }
                    });
                });
            }
            if scrim.clicked() {
                p.target_menu_open = false;
            }
        });
}

fn export_pdf_popover(
    p: &mut Presentation,
    ctx: &egui::Context,
    pal: &Palette,
    bounds: Rect,
    anchor: Rect,
) {
    let margin = 12.0_f32.min(bounds.width().min(bounds.height()) * 0.25);
    let pop_w = 276.0_f32.min((bounds.width() - margin * 2.0).max(1.0));
    let pop_h = 114.0_f32.min((bounds.height() - margin * 2.0).max(1.0));
    let min_x = bounds.left() + margin;
    let max_x = (bounds.right() - pop_w - margin).max(min_x);
    let min_y = bounds.top() + margin;
    let max_y = (bounds.bottom() - pop_h - margin).max(min_y);
    let pop = Rect::from_min_size(
        pos2(
            (anchor.right() - pop_w).clamp(min_x, max_x),
            (anchor.bottom() + 8.0).clamp(min_y, max_y),
        ),
        vec2(pop_w, pop_h),
    );
    egui::Area::new(egui::Id::unique("presenter-export-popover"))
        .order(egui::Order::Foreground)
        .fixed_pos(bounds.min)
        .show(ctx, |ui| {
            let (_, scrim) = ui.allocate_exact_size(bounds.size(), Sense::click());
            let _card_blocker =
                ui.interact(pop, ui.scope_id().with("export-pop-card"), Sense::click());
            let rad = CornerRadius::same(theme::R_LG);
            ui.painter().add(pal.shadow.as_shape(pop, rad));
            ui.painter().rect(
                pop,
                rad,
                pal.surface,
                Stroke::new(1.0, pal.border),
                StrokeKind::Inside,
            );
            if pop.width() > 28.0 && pop.height() > 28.0 {
                let inner = pop.shrink(12.0);
                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                c.set_clip_rect(inner);
                c.spacing_mut().item_spacing.y = 6.0;
                widgets::label(&mut c, pal, "EXPORT ANNOTATED PDF");
                if widgets::side_row(
                    &mut c,
                    pal,
                    Some(icons::FILE_TEXT),
                    Some(pal.accent),
                    "Slides + speaker notes (handout)…",
                    false,
                    None,
                )
                .clicked()
                {
                    p.finish_active_stroke();
                    p.flush_store();
                    p.export_menu_open = false;
                    p.push_action(PresentAction::ExportPdf {
                        include_notes: true,
                    });
                }
                if widgets::side_row(
                    &mut c,
                    pal,
                    Some(icons::PRESENTATION),
                    None,
                    "Annotated slides only…",
                    false,
                    None,
                )
                .clicked()
                {
                    p.finish_active_stroke();
                    p.flush_store();
                    p.export_menu_open = false;
                    p.push_action(PresentAction::ExportPdf {
                        include_notes: false,
                    });
                }
            }
            if scrim.clicked() {
                p.export_menu_open = false;
            }
        });
}

fn shortcuts_overlay(p: &mut Presentation, ui: &mut Ui, pal: &Palette, full: Rect) {
    let scrim = ui.interact(
        full,
        ui.scope_id().with("presenter-shortcuts-scrim"),
        Sense::click(),
    );
    let scrim_alpha = if pal.dark { 175 } else { 96 };
    ui.painter()
        .rect_filled(full, 0, Color32::from_black_alpha(scrim_alpha));
    let w = (full.width() - 48.0).clamp(0.0, 560.0);
    let h = (full.height() - 48.0).clamp(0.0, 540.0);
    let card = Rect::from_center_size(full.center(), vec2(w, h));
    let _card_blocker = ui.interact(
        card,
        ui.scope_id().with("presenter-shortcuts-card"),
        Sense::click(),
    );
    let rad = CornerRadius::same(theme::R_XL);
    ui.painter().add(pal.shadow_lg.as_shape(card, rad));
    ui.painter().rect(
        card,
        rad,
        pal.surface,
        Stroke::new(1.0, pal.border),
        StrokeKind::Inside,
    );
    if scrim.clicked() {
        p.shortcuts_open = false;
    }
    if w < 64.0 || h < 64.0 {
        return;
    }
    let inner = card.shrink(22.0);
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    c.set_clip_rect(inner);
    c.spacing_mut().item_spacing.y = 12.0;
    c.horizontal(|ui| {
        ui.label(
            RichText::new("Presenter shortcuts")
                .font(FontId::new(17.0, theme::font::sans_600()))
                .color(pal.ink),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if Button::icon(icons::X, "Close shortcuts (Esc)")
                .size(Size::Sm)
                .show_icon(ui, pal)
                .on_hover_text("Close shortcuts (Esc)")
                .clicked()
            {
                p.shortcuts_open = false;
            }
        });
    });

    let groups: [(&str, &[(&str, &str)]); 3] = [
        (
            "NAVIGATION",
            &[
                ("→ / Space / Click", "Next slide"),
                ("← / Backspace", "Previous slide"),
                ("Shift + ← / →", "Previous / next section"),
                ("0–9 then Enter", "Jump to slide number"),
                ("Home / End", "First / last slide"),
                ("G", "Slide overview grid (Enter selects)"),
            ],
        ),
        (
            "STAGE & TOOLS",
            &[
                ("L / S / Z", "Laser / Spotlight / Zoom lens (click & hold)"),
                ("D / U", "Draw pen / Translucent highlighter (Shift = line)"),
                ("Ctrl+Z / C", "Undo stroke / Clear annotations"),
                ("K", "Insert blank drawable whiteboard slide"),
                ("B or . / W / F", "Blackout / Whiteout / Freeze audience"),
            ],
        ),
        (
            "VIEW & TIMING",
            &[
                ("P", "Open presenter console"),
                ("N / M", "Toggle speaker notes / Bottom filmstrip"),
                ("T / R", "Start-pause / Reset timer & slide timings"),
                ("? or Esc", "This sheet / Exit presentation"),
            ],
        ),
    ];
    egui::ScrollArea::vertical()
        .id_salt("presenter-shortcuts-scroll")
        .auto_shrink([false, false])
        .show(&mut c, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            for (heading, items) in groups {
                widgets::label(ui, pal, heading);
                egui::Grid::new(("presenter-shortcuts-grid", heading))
                    .num_columns(2)
                    .spacing(vec2(18.0, 7.0))
                    .show(ui, |ui| {
                        for (keys, desc) in items {
                            widgets::kbd(ui, pal, keys);
                            ui.label(RichText::new(*desc).size(13.0).color(pal.ink));
                            ui.end_row();
                        }
                    });
            }
        });
}

// ── Small Lemmata-themed presenter helpers ───────────────────────────────────

fn sep_v(ui: &mut Ui, pal: &Palette, h: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(6.0, h), Sense::hover());
    ui.painter()
        .vline(r.center().x, r.y_range(), Stroke::new(1.0, pal.border));
}

fn live_status_chip(ui: &mut Ui, pal: &Palette, text: &str) {
    let g = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::new(12.0, theme::font::sans_600()),
        pal.ok,
    );
    let width = 10.0 + 6.5 + 6.0 + g.size().x + 10.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, 26.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(7), pal.ok_soft);
        ui.painter()
            .circle_filled(pos2(rect.left() + 13.0, rect.center().y), 3.25, pal.ok);
        ui.painter().galley(
            pos2(rect.left() + 22.5, rect.center().y - g.size().y * 0.5),
            g,
            pal.ok,
        );
    }
}

fn slide_counter_pill(ui: &mut Ui, pal: &Palette, text: &str, disabled: bool) -> egui::Response {
    let g = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::new(13.0, theme::font::mono_500()),
        pal.ink,
    );
    let width = (g.size().x + 22.0).max(58.0);
    let sense = if disabled {
        Sense::hover()
    } else {
        Sense::click()
    };
    let (rect, r) = ui.allocate_exact_size(vec2(width, 26.0), sense);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, !disabled, text));
    if ui.is_rect_visible(rect) {
        let fill = if r.hovered() && !disabled {
            pal.surface_3
        } else {
            pal.surface_2
        };
        let stroke = if r.hovered() && !disabled {
            pal.border_strong
        } else {
            pal.border
        };
        ui.painter().rect(
            rect,
            CornerRadius::same(theme::R_MD),
            fill,
            Stroke::new(1.0, stroke),
            StrokeKind::Inside,
        );
        ui.painter().galley(
            rect.center() - g.size() * 0.5,
            g,
            if disabled { pal.muted } else { pal.ink },
        );
        widgets::focus_ring(ui, &r, rect, pal, theme::R_MD);
    }
    if disabled {
        r
    } else {
        r.on_hover_cursor(egui::CursorIcon::PointingHand)
    }
}

fn end_presentation_btn(ui: &mut Ui, pal: &Palette, compact: bool) -> egui::Response {
    let visible = if compact { "End" } else { "End presentation" };
    let g = ui.painter().layout_no_wrap(
        visible.to_owned(),
        FontId::new(13.0, theme::font::sans_500()),
        pal.error,
    );
    let (rect, r) = ui.allocate_exact_size(vec2(g.size().x + 22.0, 30.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "End presentation"));
    if ui.is_rect_visible(rect) {
        let fill = if r.hovered() {
            pal.error_soft.lerp_to_gamma(pal.error, 0.12)
        } else {
            pal.error_soft
        };
        let border = pal.error.gamma_multiply(if pal.dark { 0.45 } else { 0.28 });
        ui.painter().rect(
            rect,
            CornerRadius::same(theme::R_MD),
            fill,
            Stroke::new(1.0, border),
            StrokeKind::Inside,
        );
        ui.painter()
            .galley(rect.center() - g.size() * 0.5, g, pal.error);
        widgets::focus_ring(ui, &r, rect, pal, theme::R_MD);
    }
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}
