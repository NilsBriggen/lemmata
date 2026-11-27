//! Audience stage, Slide Overview Grid, and Studio-Dark Presenter Console UI.

use super::data::{
    PaceState, PenColor, Region, ScreenMode, StageTool, format_elapsed, format_remaining,
};
use super::{PresentAction, Presentation};
use crate::icons::{self, Icon};
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Margin, Rect, RichText, Sense, Stroke, StrokeKind,
    Ui, pos2, vec2,
};

// Studio-dark 4.0 stage palette (always dark in presenter mode to avoid stage glare).
pub(crate) const BG: Color32 = Color32::from_rgb(0x0b, 0x0f, 0x14);
const BAR: Color32 = Color32::from_rgb(0x13, 0x19, 0x20);
const CARD: Color32 = Color32::from_rgb(0x16, 0x1d, 0x26);
const WELL: Color32 = Color32::from_rgb(0x07, 0x0a, 0x0e);
const LINE: Color32 = Color32::from_rgb(0x25, 0x30, 0x3c);
const LINE_SUBTLE: Color32 = Color32::from_rgb(0x1d, 0x26, 0x31);
const BTN: Color32 = Color32::from_rgb(0x1d, 0x26, 0x31);
const BTN_HOVER: Color32 = Color32::from_rgb(0x27, 0x33, 0x42);
const BTN_ACTIVE: Color32 = Color32::from_rgb(0x1b, 0x35, 0x4d);
const INK: Color32 = Color32::from_rgb(0xe6, 0xec, 0xf2);
const MUTED: Color32 = Color32::from_rgb(0x8d, 0x9b, 0xaa);
const FAINT: Color32 = Color32::from_rgb(0x5c, 0x6b, 0x7a);
const ACCENT: Color32 = Color32::from_rgb(0x6f, 0xb3, 0xe3);
const OK: Color32 = Color32::from_rgb(0x4e, 0xc9, 0x86);
const WARN: Color32 = Color32::from_rgb(0xe8, 0xb3, 0x4a);
const DANGER: Color32 = Color32::from_rgb(0xe8, 0x63, 0x63);
const DANGER_BG: Color32 = Color32::from_rgb(0x47, 0x1f, 0x22);
const DANGER_HOVER: Color32 = Color32::from_rgb(0x5c, 0x27, 0x2b);

/// Renders the full-screen audience presentation surface.
pub(super) fn audience_ui(p: &mut Presentation, ctx: &egui::Context, keep: &mut bool) {
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
                overview_grid(p, ui, rect);
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
            } else if p.total() > 0 {
                let page_idx = aud_page - 1;
                let (slide, _) = p.page_regions(page_idx);
                if let Some(slide_rect) = p.draw(ui, rect, page_idx, slide) {
                    let allow_draw = p.frozen_page.is_none();
                    if !(show_hud && pointer_in_hud) {
                        consumed_by_tool = p.interact_stage(
                            ui,
                            slide_rect,
                            aud_page,
                            "audience-stage",
                            allow_draw,
                        );
                    }
                    p.paint_overlays(ui, slide_rect, aud_page, p.frozen_page.is_none());
                }
            }

            // Jump-to-slide digit buffer pill.
            if !p.jump_digits.is_empty() {
                let text = format!("Go to slide: {}", p.jump_digits);
                let g =
                    ui.painter()
                        .layout_no_wrap(text, FontId::proportional(15.0), Color32::WHITE);
                let pill = Rect::from_center_size(
                    pos2(rect.center().x, rect.bottom() - 62.0),
                    g.size() + vec2(26.0, 14.0),
                );
                ui.painter().rect(
                    pill,
                    CornerRadius::same(10),
                    Color32::from_rgba_unmultiplied(15, 20, 27, 225),
                    Stroke::new(1.0, ACCENT),
                    StrokeKind::Inside,
                );
                ui.painter()
                    .galley(pill.center() - g.size() / 2.0, g, Color32::WHITE);
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(250));
            }

            // Auto-hiding interactive floating HUD when pointer moves on the audience window.
            // Visibility is checked geometrically against `rect` / `hud_rect` (never `r.hovered()`),
            // so hovering HUD buttons does not drop `r.hovered()` and oscillate the HUD on/off.
            if show_hud && hud_rect.width() >= 24.0 && hud_rect.height() >= 24.0 {
                audience_hud(p, ui, hud_rect, keep);
                if recently_moved && !pointer_in_hud {
                    let wait = (2.25 - since_move).clamp(0.05, 2.3);
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_secs_f32(wait));
                }
            }

            if p.shortcuts_open && !p.presenter {
                shortcuts_overlay(p, ui, rect);
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
    let hud_h = 40.0_f32.min(rect.height().max(0.0));
    let hud_w = (rect.width() - 40.0).clamp(0.0, 460.0);
    Rect::from_center_size(
        pos2(
            rect.center().x,
            (rect.bottom() - 32.0).max(rect.top() + hud_h * 0.5),
        ),
        vec2(hud_w, hud_h),
    )
}

/// Sleek floating bottom-center HUD pill on the audience screen on pointer hover.
fn audience_hud(p: &mut Presentation, ui: &mut Ui, hud: Rect, keep: &mut bool) {
    // Capture clicks on the pill surface before placing child buttons so gaps inside the HUD
    // never fall through to the slide-advance click handler underneath.
    let _pill_blocker = ui.interact(hud, ui.scope_id().with("audience-hud-bg"), Sense::click());
    ui.painter().add(
        egui::Shadow {
            offset: [0, 6],
            blur: 20,
            spread: 0,
            color: Color32::from_black_alpha(140),
        }
        .as_shape(hud, CornerRadius::same(12)),
    );
    ui.painter().rect(
        hud,
        CornerRadius::same(12),
        Color32::from_rgba_unmultiplied(19, 25, 32, 232),
        Stroke::new(1.0, LINE),
        StrokeKind::Inside,
    );

    let mut c = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(hud.shrink2(vec2(10.0, 4.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    c.spacing_mut().item_spacing.x = 6.0;
    let total = p.total();

    if icon_btn(
        &mut c,
        icons::CHEVRON_LEFT,
        "Previous slide",
        false,
        p.page <= 1,
    )
    .clicked()
    {
        p.go(p.page as f64 - 1.0);
    }
    c.label(
        RichText::new(format!("{} / {}", p.audience_page(), total.max(1)))
            .font(FontId::new(12.5, theme::font::mono_500()))
            .color(INK),
    );
    if icon_btn(
        &mut c,
        icons::CHEVRON_RIGHT,
        "Next slide",
        false,
        p.page >= total,
    )
    .clicked()
    {
        p.go(p.page as f64 + 1.0);
    }
    sep_v(&mut c, 18.0);
    if pill_btn(
        &mut c,
        Some(icons::PRESENTATION),
        "Presenter",
        p.presenter,
        false,
    )
    .on_hover_text("Open presenter view (P)")
    .clicked()
    {
        if p.presenter {
            p.close_presenter_console();
        } else {
            p.presenter = true;
        }
    }
    if icon_btn(
        &mut c,
        icons::LAYOUT_GRID,
        "Slide overview (G)",
        p.overview,
        total == 0,
    )
    .clicked()
    {
        p.overview = !p.overview;
    }
    if icon_btn(
        &mut c,
        icons::PENCIL,
        "Annotate slide (D)",
        p.tool == StageTool::Pen,
        false,
    )
    .clicked()
    {
        p.toggle_tool(StageTool::Pen);
    }
    c.with_layout(egui::Layout::right_to_left(egui::Align::Center), |c| {
        if icon_btn(c, icons::X, "End presentation (Esc)", false, false).clicked() {
            *keep = false;
        }
    });
}

/// Scrollable slide overview grid (`G`) with section titles and annotated-slide dots.
fn overview_grid(p: &mut Presentation, ui: &mut Ui, rect: Rect) {
    let total = p.total();
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), 52.0));
    ui.painter().rect_filled(header, 0, BAR);
    ui.painter().hline(
        header.x_range(),
        header.bottom() - 0.5,
        Stroke::new(1.0, LINE),
    );
    let mut h = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(header.shrink2(vec2(20.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    h.spacing_mut().item_spacing.x = 10.0;
    icons::show(&mut h, icons::LAYOUT_GRID, 16.0, ACCENT);
    h.label(
        RichText::new(format!("Slide overview · {total} slides"))
            .font(FontId::new(14.5, theme::font::sans_600()))
            .color(INK),
    );
    h.label(
        RichText::new("Click any slide to jump · Esc to close")
            .size(12.5)
            .color(MUTED),
    );
    h.with_layout(egui::Layout::right_to_left(egui::Align::Center), |h| {
        if pill_btn(h, Some(icons::X), "Close", false, false).clicked() {
            p.overview = false;
        }
    });

    let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max).shrink(20.0);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
    let mut picked = None;
    egui::ScrollArea::vertical()
        .id_salt("present-overview")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            let w = ui.available_width();
            let gap = 18.0;
            let cols = (((w + gap) / 236.0).floor() as usize).clamp(1, 6);
            let cell_w = ((w - gap * (cols as f32 - 1.0)) / cols as f32).max(130.0);
            let thumb_h = cell_w * 0.60;
            let cell_h = thumb_h + 30.0;
            let rows = total.div_ceil(cols);
            let (grid_rect, _) =
                ui.allocate_exact_size(vec2(w, rows as f32 * (cell_h + gap)), Sense::hover());
            for i in 0..total {
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
                let stroke = if is_cur {
                    Stroke::new(2.5, ACCENT)
                } else if is_frozen {
                    Stroke::new(2.0, WARN)
                } else if resp.hovered() {
                    Stroke::new(1.5, INK)
                } else {
                    Stroke::new(1.0, LINE)
                };
                ui.painter().rect_filled(thumb, CornerRadius::same(6), WELL);
                let (slide, _) = p.page_regions(i);
                if let Some(sr) = p.draw(ui, thumb.shrink(3.0), i, slide) {
                    p.paint_overlays(ui, sr, slide_num, false);
                }
                ui.painter()
                    .rect_stroke(thumb, CornerRadius::same(6), stroke, StrokeKind::Outside);
                // Bottom caption: slide number + optional PDF bookmark section + annotated dot.
                let has_ink = p
                    .store
                    .annotations
                    .get(&slide_num)
                    .is_some_and(|v| !v.is_empty());
                let caption = match p.sections.get(&slide_num) {
                    Some(sec) => format!("{slide_num} · {sec}"),
                    None => format!("Slide {slide_num}"),
                };
                let mut job = egui::text::LayoutJob::simple_singleline(
                    caption,
                    if is_cur {
                        FontId::new(12.0, theme::font::sans_600())
                    } else {
                        FontId::proportional(12.0)
                    },
                    if is_cur { INK } else { MUTED },
                );
                job.wrap.max_width = cell_w - if has_ink { 18.0 } else { 4.0 };
                job.wrap.max_rows = 1;
                job.wrap.overflow_character = Some('…');
                let g = ui.fonts_mut(|f| f.layout_job(job));
                ui.painter()
                    .galley(pos2(cell.left() + 2.0, thumb.bottom() + 6.0), g, INK);
                if has_ink {
                    ui.painter().circle_filled(
                        pos2(cell.right() - 6.0, thumb.bottom() + 13.0),
                        4.0,
                        DANGER,
                    );
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

/// Studio-dark Presenter Console UI. Returns `false` when the presentation should end.
pub(super) fn presenter_ui(p: &mut Presentation, ui: &mut Ui) -> bool {
    let mut keep = true;
    let full = ui.max_rect();
    // Always paint the full studio-dark stage background (eliminates any white outer gap).
    ui.painter().rect_filled(full, 0, BG);

    let head = Rect::from_min_size(full.min, vec2(full.width(), 54.0));
    let pace_bar = Rect::from_min_size(pos2(full.left(), head.bottom()), vec2(full.width(), 3.0));
    let foot = Rect::from_min_max(pos2(full.left(), full.bottom() - 52.0), full.max);
    let filmstrip_h = if p.store.filmstrip && p.total() > 0 {
        94.0
    } else {
        0.0
    };
    let filmstrip_rect = Rect::from_min_max(
        pos2(full.left(), foot.top() - filmstrip_h),
        pos2(full.right(), foot.top()),
    );

    // Header & footer chrome.
    ui.painter().rect_filled(head, 0, BAR);
    ui.painter().rect_filled(foot, 0, BAR);
    ui.painter()
        .hline(foot.x_range(), foot.top() + 0.5, Stroke::new(1.0, LINE));

    // Dual-track pacing bar (slide progress + target time marker).
    paint_pacing_bar(p, ui, pace_bar);

    // Header content: Slide jump & section | Timer & Target | Fullscreen & Wall Clock.
    let target_anchor = presenter_header(p, ui, head);

    // Main stage area (Overview grid OR Resizable Current Slide + Next/Notes split).
    let stage_rect = Rect::from_min_max(
        pos2(full.left(), pace_bar.bottom()),
        pos2(full.right(), filmstrip_rect.top()),
    );
    if p.overview && p.total() > 0 {
        overview_grid(p, ui, stage_rect);
    } else {
        presenter_stage(p, ui, stage_rect.shrink(14.0));
    }

    // Optional collapsible slide filmstrip.
    if filmstrip_h > 0.0 {
        presenter_filmstrip(p, ui, filmstrip_rect);
    }

    // Footer controls.
    let export_anchor = presenter_footer(p, ui, foot, &mut keep);

    // Popovers & overlays (mutually exclusive foreground Areas with click-absorbing scrims).
    if p.shortcuts_open {
        p.target_menu_open = false;
        p.export_menu_open = false;
        egui::Area::new(egui::Id::unique("presenter-shortcuts"))
            .order(egui::Order::Foreground)
            .fixed_pos(full.min)
            .show(ui.ctx(), |ui| {
                let (rect, _) = ui.allocate_exact_size(full.size(), Sense::hover());
                shortcuts_overlay(p, ui, rect);
            });
    } else if p.target_menu_open
        && let Some(anchor) = target_anchor
    {
        target_duration_popover(p, ui.ctx(), full, anchor);
    } else if p.export_menu_open
        && let Some(anchor) = export_anchor
    {
        export_pdf_popover(p, ui.ctx(), full, anchor);
    }

    // Transient status toast (bottom-right above footer).
    if let Some((msg, at)) = p.status_toast.clone() {
        if at.elapsed().as_secs_f32() < 4.0 {
            let g = ui
                .painter()
                .layout_no_wrap(msg, FontId::proportional(12.5), INK);
            let pill = Rect::from_min_size(
                pos2(
                    full.right() - g.size().x - 40.0,
                    foot.top() - filmstrip_h - 44.0,
                ),
                g.size() + vec2(24.0, 14.0),
            );
            ui.painter().rect(
                pill,
                CornerRadius::same(8),
                CARD,
                Stroke::new(1.0, ACCENT),
                StrokeKind::Inside,
            );
            ui.painter().galley(pill.center() - g.size() / 2.0, g, INK);
        } else {
            p.status_toast = None;
        }
    }

    keep
}

fn paint_pacing_bar(p: &Presentation, ui: &Ui, rect: Rect) {
    ui.painter().rect_filled(rect, 0, LINE_SUBTLE);
    let total = p.total();
    if total > 0 {
        let frac = (p.page as f32 / total as f32).clamp(0.0, 1.0);
        let fill_color = match p.timer.pace(p.store.target_minutes) {
            PaceState::Overtime => DANGER,
            PaceState::Warning => WARN,
            PaceState::OnPace | PaceState::Idle => ACCENT,
        };
        let fill = Rect::from_min_size(rect.min, vec2(rect.width() * frac, rect.height()));
        ui.painter().rect_filled(fill, 0, fill_color);
    }
    // Time-pace marker tick when a target duration is set.
    if let Some(mins) = p.store.target_minutes.filter(|&m| m > 0) {
        let time_frac = (p.timer.elapsed() / (mins as f64 * 60.0)).clamp(0.0, 1.0) as f32;
        let x = rect.left() + rect.width() * time_frac;
        let marker = Rect::from_center_size(pos2(x, rect.center().y), vec2(4.0, 7.0));
        let col = match p.timer.pace(Some(mins)) {
            PaceState::Overtime => DANGER,
            PaceState::Warning => WARN,
            _ => OK,
        };
        ui.painter().rect_filled(marker, CornerRadius::same(2), col);
    }
}

fn presenter_header(p: &mut Presentation, ui: &mut Ui, head: Rect) -> Option<Rect> {
    let mut target_anchor = None;
    let total = p.total();
    if head.width() < 48.0 || head.height() < 16.0 {
        return None;
    }
    let compact = head.width() < 760.0;
    let inner = head.shrink2(vec2(16.0_f32.min(head.width() * 0.2), 0.0));

    // Right side: Wall clock (+ optional ETA when wide enough) & Fullscreen toggle.
    let now = chrono::Local::now();
    let clock = now.format("%H:%M").to_string();
    let show_eta = head.width() >= 860.0;
    let eta = show_eta
        .then(|| {
            p.store.target_minutes.filter(|&m| m > 0).map(|mins| {
                let rem = (mins as f64 * 60.0 - p.timer.elapsed()).max(0.0) as i64;
                let end = now + chrono::Duration::seconds(rem);
                format!("Ends {}", end.format("%H:%M"))
            })
        })
        .flatten();
    let right_w = (if eta.is_some() { 172.0 } else { 104.0_f32 }).min(inner.width());
    let right_rect = Rect::from_min_max(pos2(inner.right() - right_w, inner.top()), inner.max);
    let mut r_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(right_rect)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    r_ui.spacing_mut().item_spacing.x = 10.0;
    r_ui.label(
        RichText::new(&clock)
            .font(FontId::new(
                if compact { 15.0 } else { 17.5 },
                theme::font::mono_500(),
            ))
            .color(INK),
    );
    if let Some(eta_text) = eta {
        r_ui.label(
            RichText::new(eta_text)
                .font(FontId::monospace(11.5))
                .color(MUTED),
        );
    }
    if icon_btn(
        &mut r_ui,
        icons::MONITOR,
        "Toggle presenter fullscreen",
        p.presenter_fullscreen,
        false,
    )
    .clicked()
    {
        p.presenter_fullscreen = !p.presenter_fullscreen;
        p.presenter_fullscreen_dirty = true;
    }

    // Center & Left geometry: mathematically disjoint at every viewport width.
    let center_w = if compact { 256.0 } else { 360.0 };
    let left_min_w = 112.0;
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

    // Left side: Slide counter (click to jump) + active section + audience status badge.
    let left_right = center_rect
        .map(|c| c.left() - gap)
        .unwrap_or(right_rect.left() - gap)
        .max(inner.left());
    let left_rect = Rect::from_min_max(inner.min, pos2(left_right, inner.bottom()));
    if left_rect.width() >= 40.0 {
        let mut l_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(left_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        l_ui.set_clip_rect(left_rect);
        l_ui.spacing_mut().item_spacing.x = 8.0;
        if p.editing_slide_jump {
            let resp = l_ui.add(
                egui::TextEdit::singleline(&mut p.slide_jump_text)
                    .desired_width(54.0)
                    .font(FontId::new(14.0, theme::font::mono_500()))
                    .hint_text("1.."),
            );
            resp.request_focus();
            let commit = l_ui.input(|i| i.key_pressed(egui::Key::Enter)) || resp.lost_focus();
            if commit {
                if let Ok(n) = p.slide_jump_text.trim().parse::<usize>() {
                    p.go(n as f64);
                }
                p.editing_slide_jump = false;
            }
        } else {
            let slide_lbl = format!(
                "Slide {} / {}",
                p.page,
                if total > 0 {
                    total.to_string()
                } else {
                    "…".into()
                }
            );
            if pill_btn(&mut l_ui, None, &slide_lbl, false, total == 0)
                .on_hover_text("Click or type slide number + Enter to jump")
                .clicked()
            {
                p.editing_slide_jump = true;
                p.slide_jump_text = p.page.to_string();
            }
        }

        // Audience state alert badges (Blackout / Whiteout / Frozen).
        if left_rect.width() >= 180.0 {
            match p.screen {
                ScreenMode::Blackout => {
                    if status_badge(&mut l_ui, "BLACKOUT", DANGER, DANGER_BG)
                        .on_hover_text("Audience screen is black · click or press B to resume")
                        .clicked()
                    {
                        p.set_screen(ScreenMode::Normal);
                    }
                }
                ScreenMode::Whiteout => {
                    if status_badge(
                        &mut l_ui,
                        "WHITEOUT",
                        WARN,
                        Color32::from_rgb(0x3a, 0x2e, 0x16),
                    )
                    .on_hover_text("Audience screen is white · click or press W to resume")
                    .clicked()
                    {
                        p.set_screen(ScreenMode::Normal);
                    }
                }
                ScreenMode::Normal => {}
            }
            if let Some(fp) = p.frozen_page {
                if status_badge(
                    &mut l_ui,
                    &format!("FROZEN · Slide {fp}"),
                    WARN,
                    Color32::from_rgb(0x3a, 0x2e, 0x16),
                )
                .on_hover_text("Audience is locked on this slide · click or press F to unfreeze")
                .clicked()
                {
                    p.frozen_page = None;
                }
            } else if !compact && let Some(sec) = p.section_for(p.page) {
                l_ui.add(egui::Label::new(RichText::new(sec).size(12.5).color(MUTED)).truncate());
            }
        }
    }

    // Center: Timer (click to toggle Elapsed/Remaining), Start/Pause, Reset, Target duration.
    if let Some(center_rect) = center_rect {
        let mut m = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(center_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        m.spacing_mut().item_spacing.x = 8.0;
        let pace = p.timer.pace(p.store.target_minutes);
        let timer_color = match pace {
            PaceState::Idle => MUTED,
            PaceState::OnPace => INK,
            PaceState::Warning => WARN,
            PaceState::Overtime => DANGER,
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
                            if compact { 20.0 } else { 25.0 },
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

        let start_lbl = if p.timer.running {
            "Pause"
        } else if p.timer.started.is_some() {
            "Resume"
        } else {
            "Start"
        };
        if pill_btn(&mut m, None, start_lbl, p.timer.running, false)
            .on_hover_text("Start or pause timer (T)")
            .clicked()
        {
            p.timer.toggle();
        }
        if icon_btn(&mut m, icons::ROTATE_CCW, "Reset timer (R)", false, false).clicked() {
            p.timer.reset();
        }
        let target_lbl = match p.store.target_minutes {
            Some(mins) if mins > 0 => format!("{mins}m"),
            _ => "Target".into(),
        };
        let tr = pill_btn(
            &mut m,
            None,
            &target_lbl,
            p.target_menu_open || p.store.target_minutes.is_some(),
            false,
        )
        .on_hover_text("Set target talk duration for countdown & pacing");
        target_anchor = Some(tr.rect);
        if tr.clicked() {
            p.target_menu_open = !p.target_menu_open;
            p.export_menu_open = false;
        }
    }

    target_anchor
}

fn presenter_stage(p: &mut Presentation, ui: &mut Ui, main: Rect) {
    if main.width() < 24.0 || main.height() < 24.0 {
        return;
    }
    if let Some(e) = &p.error {
        ui.painter().text(
            main.left_top() + vec2(20.0, 20.0),
            Align2::LEFT_TOP,
            e,
            FontId::proportional(14.0),
            DANGER,
        );
        return;
    }

    let split_gap = 12.0_f32.min(main.width() * 0.1);
    let avail_w = (main.width() - split_gap).max(0.0);
    let min_cur_w = 240.0_f32.min(avail_w * 0.5);
    let max_cur_w = (avail_w - 220.0_f32.min(avail_w * 0.45)).max(min_cur_w);
    let cur_w = (avail_w * p.store.split_x).clamp(min_cur_w, max_cur_w);
    let cur = Rect::from_min_size(main.min, vec2(cur_w, main.height()));
    let vsplit = Rect::from_min_size(
        pos2(cur.right(), main.top()),
        vec2(split_gap, main.height()),
    );
    let side = Rect::from_min_max(pos2(vsplit.right(), main.top()), main.max);

    // Draggable vertical splitter (Current Slide vs Right Column).
    let vresp = ui
        .interact(vsplit, ui.scope_id().with("pres-vsplit"), Sense::drag())
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    if vresp.dragged() && main.width() > 300.0 {
        let next_x = (cur_w + vresp.drag_delta().x) / (main.width() - split_gap);
        p.store.split_x = next_x.clamp(0.38, 0.78);
        p.store_dirty = true;
    }
    if vresp.hovered() || vresp.dragged() {
        let grip =
            Rect::from_center_size(vsplit.center(), vec2(3.0, 44.0_f32.min(vsplit.height())));
        ui.painter()
            .rect_filled(grip, CornerRadius::same(2), ACCENT);
    }

    // ── Left: Current Slide Stage ─────────────────────────────────────────────
    ui.painter().rect(
        cur,
        CornerRadius::same(10),
        WELL,
        Stroke::new(1.0, LINE_SUBTLE),
        StrokeKind::Inside,
    );
    let stage_top_h = 38.0_f32.min(cur.height());
    let stage_top = Rect::from_min_size(cur.min, vec2(cur.width(), stage_top_h));
    let pad_x = 12.0_f32.min(cur.width() * 0.25);
    let pad_y = 12.0_f32.min((cur.height() - stage_top_h).max(0.0) * 0.25);
    let slide_box = Rect::from_min_max(
        pos2(cur.left() + pad_x, stage_top.bottom()),
        pos2(
            cur.right() - pad_x,
            (cur.bottom() - pad_y).max(stage_top.bottom()),
        ),
    );

    // Top-of-stage status & live annotation tool strip.
    {
        let mut sbar = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(stage_top.shrink2(vec2(12.0, 4.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        sbar.spacing_mut().item_spacing.x = 6.0;
        let (dot_col, tag) = if let Some(fp) = p.frozen_page {
            (WARN, format!("PREVIEW · Audience frozen on {fp}"))
        } else if p.screen == ScreenMode::Blackout {
            (DANGER, "PREVIEW · Audience blacked out".into())
        } else if p.screen == ScreenMode::Whiteout {
            (WARN, "PREVIEW · Audience whiteout".into())
        } else {
            (OK, "LIVE ON STAGE".into())
        };
        let (dot_r, _) = sbar.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
        sbar.painter().circle_filled(dot_r.center(), 3.5, dot_col);
        sbar.label(
            RichText::new(tag)
                .font(FontId::new(11.0, theme::font::sans_600()))
                .color(MUTED),
        );

        // Right-aligned live stage tools: Laser, Spotlight, Pen + ink colors + Undo/Clear.
        sbar.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            let has_strokes = p
                .store
                .annotations
                .get(&p.page)
                .is_some_and(|v| !v.is_empty());
            if has_strokes {
                if icon_btn(
                    ui,
                    icons::TRASH,
                    "Clear annotations on this slide (C)",
                    false,
                    false,
                )
                .clicked()
                {
                    p.clear_slide_strokes();
                }
                if icon_btn(ui, icons::UNDO_2, "Undo last stroke (Ctrl+Z)", false, false).clicked()
                {
                    p.undo_stroke();
                }
            }
            if p.tool == StageTool::Pen {
                for color in PenColor::ALL.into_iter().rev() {
                    let active = p.pen_color == color;
                    let (cr, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                    let resp = resp
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(color.label());
                    if active {
                        ui.painter()
                            .circle_stroke(cr.center(), 9.0, Stroke::new(1.8, INK));
                    }
                    ui.painter()
                        .circle_filled(cr.center(), 6.5, color.color32());
                    if resp.clicked() {
                        p.pen_color = color;
                    }
                }
            }
            if small_tool_btn(ui, icons::PENCIL, "Pen", p.tool == StageTool::Pen)
                .on_hover_text("Draw annotations on slide (D)")
                .clicked()
            {
                p.toggle_tool(StageTool::Pen);
            }
            if small_tool_btn(ui, icons::SUN, "Spotlight", p.tool == StageTool::Spotlight)
                .on_hover_text("Click & hold on slide to spotlight a region (S)")
                .clicked()
            {
                p.toggle_tool(StageTool::Spotlight);
            }
            if small_tool_btn(ui, icons::LOCATE, "Laser", p.tool == StageTool::Laser)
                .on_hover_text("Click & hold on slide for synced red laser pointer (L)")
                .clicked()
            {
                p.toggle_tool(StageTool::Laser);
            }
        });
    }

    let total = p.total();
    let (cur_slide, cur_notes) = if total > 0 {
        p.page_regions(p.page - 1)
    } else {
        (Region::Full, None)
    };
    if total > 0
        && let Some(sr) = p.draw(ui, slide_box, p.page - 1, cur_slide)
    {
        p.interact_stage(ui, sr, p.page, "presenter-stage", true);
        p.paint_overlays(ui, sr, p.page, true);
    }

    // ── Right Column: Next Slide (top) + Resizable Splitter + Speaker Notes (bottom) ──
    if side.width() < 24.0 || side.height() < 24.0 {
        return;
    }
    let hsplit_h = 12.0_f32.min(side.height() * 0.1);
    let avail_h = (side.height() - hsplit_h).max(0.0);
    let min_next_h = 120.0_f32.min(avail_h * 0.45);
    let max_next_h = (avail_h - 140.0_f32.min(avail_h * 0.5)).max(min_next_h);
    let next_h = (avail_h * p.store.split_y).clamp(min_next_h, max_next_h);
    let next_card = Rect::from_min_size(side.min, vec2(side.width(), next_h));
    let hsplit = Rect::from_min_size(
        pos2(side.left(), next_card.bottom()),
        vec2(side.width(), hsplit_h),
    );
    let notes_card = Rect::from_min_max(pos2(side.left(), hsplit.bottom()), side.max);

    let hresp = ui
        .interact(hsplit, ui.scope_id().with("pres-hsplit"), Sense::drag())
        .on_hover_cursor(egui::CursorIcon::ResizeVertical);
    if hresp.dragged() && side.height() > 200.0 {
        let next_y = (next_h + hresp.drag_delta().y) / (side.height() - hsplit_h);
        p.store.split_y = next_y.clamp(0.22, 0.72);
        p.store_dirty = true;
    }
    if hresp.hovered() || hresp.dragged() {
        let grip = Rect::from_center_size(hsplit.center(), vec2(44.0_f32.min(hsplit.width()), 3.0));
        ui.painter()
            .rect_filled(grip, CornerRadius::same(2), ACCENT);
    }

    // Next Slide Card (clicking advances to next slide).
    ui.painter().rect(
        next_card,
        CornerRadius::same(10),
        CARD,
        Stroke::new(1.0, LINE_SUBTLE),
        StrokeKind::Inside,
    );
    let next_head_h = 30.0_f32.min(next_card.height());
    let next_head = Rect::from_min_size(next_card.min, vec2(next_card.width(), next_head_h));
    if next_head_h >= 16.0 {
        ui.painter().text(
            pos2(next_head.left() + 12.0, next_head.center().y),
            Align2::LEFT_CENTER,
            if p.page < total {
                format!("NEXT · SLIDE {}", p.page + 1)
            } else {
                "NEXT".into()
            },
            FontId::new(10.5, theme::font::sans_600()),
            MUTED,
        );
    }
    let well_pad = 8.0_f32.min(next_card.width() * 0.25);
    let next_well = Rect::from_min_max(
        pos2(next_card.left() + well_pad, next_head.bottom()),
        pos2(
            next_card.right() - well_pad,
            (next_card.bottom() - well_pad).max(next_head.bottom()),
        ),
    );
    if next_well.width() >= 12.0 && next_well.height() >= 12.0 {
        ui.painter()
            .rect_filled(next_well, CornerRadius::same(6), WELL);
        if p.page < total {
            let (next_slide, _) = p.page_regions(p.page);
            let inner_well =
                next_well.shrink(6.0_f32.min(next_well.width().min(next_well.height()) * 0.25));
            if let Some(nr) = p.draw(ui, inner_well, p.page, next_slide) {
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
            if nresp.clicked() {
                p.go(p.page as f64 + 1.0);
            }
        } else if total > 0 {
            ui.painter().text(
                next_well.center(),
                Align2::CENTER_CENTER,
                "End of slides",
                FontId::proportional(13.5),
                FAINT,
            );
        }
    }

    // Speaker Notes Card.
    presenter_notes_card(p, ui, notes_card, cur_notes, total);
}

fn presenter_notes_card(
    p: &mut Presentation,
    ui: &mut Ui,
    card: Rect,
    cur_notes: Option<Region>,
    total: usize,
) {
    if card.width() < 24.0 || card.height() < 24.0 {
        return;
    }
    if cur_notes.is_some() {
        p.editing_notes = false;
    }
    ui.painter().rect(
        card,
        CornerRadius::same(10),
        CARD,
        Stroke::new(1.0, LINE_SUBTLE),
        StrokeKind::Inside,
    );
    let head_h = 34.0_f32.min(card.height());
    let head = Rect::from_min_size(card.min, vec2(card.width(), head_h));
    ui.painter().hline(
        head.x_range(),
        head.bottom() - 0.5,
        Stroke::new(1.0, LINE_SUBTLE),
    );

    if head.width() > 28.0 && head.height() >= 20.0 {
        let mut h = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(head.shrink2(vec2(12.0_f32.min(head.width() * 0.2), 2.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        h.label(
            RichText::new("SPEAKER NOTES")
                .font(FontId::new(10.5, theme::font::sans_600()))
                .color(MUTED),
        );
        h.with_layout(egui::Layout::right_to_left(egui::Align::Center), |h| {
            h.spacing_mut().item_spacing.x = 4.0;
            if cur_notes.is_some() {
                if small_tool_btn(h, icons::ARROW_UP_DOWN, "Swap L/R", p.store.swap_notes_half)
                    .on_hover_text("Swap slide and Beamer notes halves (for second screen=left)")
                    .clicked()
                {
                    p.store.swap_notes_half = !p.store.swap_notes_half;
                    p.store_dirty = true;
                }
            } else {
                if small_tool_btn(
                    h,
                    if p.editing_notes {
                        icons::CHECK
                    } else {
                        icons::PENCIL
                    },
                    if p.editing_notes { "Done" } else { "Edit" },
                    p.editing_notes,
                )
                .on_hover_text(
                    "Edit speaker notes for this slide (saved in .lemmata/presenter.json)",
                )
                .clicked()
                {
                    p.editing_notes = !p.editing_notes;
                    if p.editing_notes {
                        let fallback = p.tex_notes.get(&p.page).cloned().unwrap_or_default();
                        p.store.notes.entry(p.page).or_insert(fallback);
                    }
                }
                if tiny_text_btn(h, "A+")
                    .on_hover_text("Larger notes text")
                    .clicked()
                {
                    p.store.notes_font_size = (p.store.notes_font_size + 2.0).min(28.0);
                    p.store_dirty = true;
                }
                if tiny_text_btn(h, "A−")
                    .on_hover_text("Smaller notes text")
                    .clicked()
                {
                    p.store.notes_font_size = (p.store.notes_font_size - 2.0).max(13.0);
                    p.store_dirty = true;
                }
            }
        });
    }

    let body = Rect::from_min_max(pos2(card.left(), head.bottom()), card.max);
    if body.width() < 24.0 || body.height() < 20.0 {
        return;
    }
    if let Some(region) = cur_notes
        && total > 0
    {
        let well = body.shrink(8.0_f32.min(body.width().min(body.height()) * 0.25));
        ui.painter().rect_filled(well, CornerRadius::same(6), WELL);
        let inner = well.shrink(4.0_f32.min(well.width().min(well.height()) * 0.25));
        p.draw(ui, inner, p.page - 1, region);
        return;
    }

    let pad = 12.0_f32.min(body.width().min(body.height()) * 0.25);
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(body.shrink(pad)));
    let font_size = p.store.notes_font_size;
    if p.editing_notes {
        let fallback = p.tex_notes.get(&p.page).cloned().unwrap_or_default();
        let entry = p.store.notes.entry(p.page).or_insert(fallback);
        let avail = c.available_size();
        egui::ScrollArea::vertical()
            .id_salt(("notes-edit", p.page))
            .show(&mut c, |ui| {
                let r = ui.add_sized(
                    vec2(avail.x.max(1.0), (avail.y - 4.0).max(40.0)),
                    egui::TextEdit::multiline(entry)
                        .font(FontId::proportional(font_size.min(18.0)))
                        .text_color(INK)
                        .hint_text("Type talking points for this slide…"),
                );
                if r.changed() {
                    p.store_dirty = true;
                    p.dirty_for_export = true;
                }
            });
    } else {
        let note = p.effective_note(p.page).map(ToOwned::to_owned);
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
                        } else if let Some(item) = trimmed
                            .strip_prefix("- ")
                            .or_else(|| trimmed.strip_prefix("* "))
                        {
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                ui.label(
                                    RichText::new("•")
                                        .size(font_size)
                                        .color(ACCENT),
                                );
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(item).size(font_size).color(INK),
                                    )
                                    .wrap(),
                                );
                            });
                        } else {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(trimmed).size(font_size).color(INK),
                                )
                                .wrap(),
                            );
                        }
                    }
                } else {
                    ui.add(
                        egui::Label::new(
                            RichText::new(
                                "No speaker notes for this slide yet. Click Edit to add talking points (included when you export an annotated PDF), or compile with \\setbeameroption{show notes on second screen=right}.",
                            )
                            .size(13.0)
                            .color(MUTED),
                        )
                        .wrap(),
                    );
                }
            });
    }
}

fn presenter_filmstrip(p: &mut Presentation, ui: &mut Ui, rect: Rect) {
    ui.painter()
        .rect_filled(rect, 0, Color32::from_rgb(0x0f, 0x14, 0x1a));
    ui.painter().hline(
        rect.x_range(),
        rect.top() + 0.5,
        Stroke::new(1.0, LINE_SUBTLE),
    );
    let total = p.total();
    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(12.0, 8.0))));
    let mut picked = None;
    egui::ScrollArea::horizontal()
        .id_salt("presenter-filmstrip")
        .auto_shrink([false, false])
        .show(&mut c, |ui| {
            let gap = 10.0;
            let thumb_h = 60.0;
            let thumb_w = 96.0;
            let total_w = total as f32 * (thumb_w + gap);
            let (strip_rect, _) =
                ui.allocate_exact_size(vec2(total_w, thumb_h + 18.0), Sense::hover());
            for i in 0..total {
                let slide_num = i + 1;
                let cell = Rect::from_min_size(
                    pos2(
                        strip_rect.left() + i as f32 * (thumb_w + gap),
                        strip_rect.top(),
                    ),
                    vec2(thumb_w, thumb_h + 18.0),
                );
                if !ui.is_rect_visible(cell) {
                    continue;
                }
                let thumb = Rect::from_min_size(cell.min, vec2(thumb_w, thumb_h));
                let resp = ui
                    .interact(cell, ui.scope_id().with(("fs", i)), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let is_cur = slide_num == p.page;
                let is_frz = p.frozen_page == Some(slide_num);
                let stroke = if is_cur {
                    Stroke::new(2.0, ACCENT)
                } else if is_frz {
                    Stroke::new(2.0, WARN)
                } else if resp.hovered() {
                    Stroke::new(1.5, INK)
                } else {
                    Stroke::new(1.0, LINE)
                };
                ui.painter().rect_filled(thumb, CornerRadius::same(4), WELL);
                let (slide, _) = p.page_regions(i);
                if let Some(sr) = p.draw(ui, thumb.shrink(2.0), i, slide) {
                    p.paint_overlays(ui, sr, slide_num, false);
                }
                ui.painter()
                    .rect_stroke(thumb, CornerRadius::same(4), stroke, StrokeKind::Outside);
                let has_ink = p
                    .store
                    .annotations
                    .get(&slide_num)
                    .is_some_and(|v| !v.is_empty());
                ui.painter().text(
                    pos2(cell.center().x, thumb.bottom() + 3.0),
                    Align2::CENTER_TOP,
                    format!("{slide_num}"),
                    FontId::new(11.0, theme::font::mono_500()),
                    if is_cur { INK } else { MUTED },
                );
                if has_ink {
                    ui.painter().circle_filled(
                        pos2(thumb.right() - 6.0, thumb.top() + 6.0),
                        3.5,
                        DANGER,
                    );
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

fn presenter_footer(
    p: &mut Presentation,
    ui: &mut Ui,
    foot: Rect,
    keep: &mut bool,
) -> Option<Rect> {
    let mut export_anchor = None;
    let total = p.total();
    let compact = foot.width() < 1060.0;
    let mut f = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(foot.shrink2(vec2(16.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    f.spacing_mut().item_spacing.x = 7.0;

    if pill_btn(
        &mut f,
        Some(icons::CHEVRON_LEFT),
        "Back",
        false,
        p.page <= 1,
    )
    .clicked()
    {
        p.go(p.page as f64 - 1.0);
    }
    if pill_btn(
        &mut f,
        Some(icons::CHEVRON_RIGHT),
        "Next",
        false,
        p.page >= total,
    )
    .clicked()
    {
        p.go(p.page as f64 + 1.0);
    }
    sep_v(&mut f, 20.0);

    if pill_btn(
        &mut f,
        Some(icons::MOON),
        if p.screen == ScreenMode::Blackout {
            "Resume"
        } else {
            "Blackout"
        },
        p.screen == ScreenMode::Blackout,
        false,
    )
    .on_hover_text("Toggle black audience screen (B or .)")
    .clicked()
    {
        p.toggle_blackout();
    }
    if !compact
        && pill_btn(
            &mut f,
            Some(icons::SUN),
            "Whiteout",
            p.screen == ScreenMode::Whiteout,
            false,
        )
        .on_hover_text("Toggle white audience screen (W)")
        .clicked()
    {
        p.toggle_whiteout();
    }
    if pill_btn(
        &mut f,
        Some(icons::PIN),
        if p.frozen_page.is_some() {
            "Unfreeze"
        } else {
            "Freeze"
        },
        p.frozen_page.is_some(),
        total == 0,
    )
    .on_hover_text("Hold audience on current slide while browsing freely (F)")
    .clicked()
    {
        p.toggle_freeze();
    }
    sep_v(&mut f, 20.0);
    if pill_btn(
        &mut f,
        Some(icons::COLUMNS_2),
        "Filmstrip",
        p.store.filmstrip,
        total == 0,
    )
    .on_hover_text("Toggle bottom slide filmstrip (M)")
    .clicked()
    {
        p.store.filmstrip = !p.store.filmstrip;
        p.store_dirty = true;
    }
    if pill_btn(
        &mut f,
        Some(icons::LAYOUT_GRID),
        "Overview",
        p.overview,
        total == 0,
    )
    .on_hover_text("Toggle slide overview grid (G)")
    .clicked()
    {
        p.overview = !p.overview;
        if p.overview {
            p.editing_notes = false;
            p.editing_slide_jump = false;
        }
    }

    f.with_layout(egui::Layout::right_to_left(egui::Align::Center), |f| {
        f.spacing_mut().item_spacing.x = 8.0;
        if end_button(f).clicked() {
            *keep = false;
        }
        if icon_btn(
            f,
            icons::INFO,
            "Presenter shortcuts (?)",
            p.shortcuts_open,
            false,
        )
        .clicked()
        {
            p.shortcuts_open = !p.shortcuts_open;
            if p.shortcuts_open {
                p.target_menu_open = false;
                p.export_menu_open = false;
            }
        }
        let ann_count = p.store.annotated_slide_count();
        let save_lbl = if ann_count > 0 {
            format!("Save PDF ({ann_count})")
        } else {
            "Save PDF".into()
        };
        let sr = pill_btn(
            f,
            Some(icons::DOWNLOAD),
            &save_lbl,
            p.export_menu_open || ann_count > 0,
            total == 0,
        )
        .on_hover_text("Save slides with annotations and speaker notes as a PDF");
        export_anchor = Some(sr.rect);
        if sr.clicked() {
            p.export_menu_open = !p.export_menu_open;
            p.target_menu_open = false;
        }
    });

    export_anchor
}

fn target_duration_popover(p: &mut Presentation, ctx: &egui::Context, bounds: Rect, anchor: Rect) {
    let margin = 12.0_f32.min(bounds.width().min(bounds.height()) * 0.25);
    let pop_w = 248.0_f32.min((bounds.width() - margin * 2.0).max(1.0));
    let pop_h = 152.0_f32.min((bounds.height() - margin * 2.0).max(1.0));
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
            ui.painter().add(
                egui::Shadow {
                    offset: [0, 8],
                    blur: 24,
                    spread: 0,
                    color: Color32::from_black_alpha(160),
                }
                .as_shape(pop, CornerRadius::same(10)),
            );
            ui.painter().rect(
                pop,
                CornerRadius::same(10),
                CARD,
                Stroke::new(1.0, LINE),
                StrokeKind::Inside,
            );
            if pop.width() > 28.0 && pop.height() > 28.0 {
                let inner = pop.shrink(12.0);
                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                c.set_clip_rect(inner);
                c.spacing_mut().item_spacing.y = 8.0;
                c.label(
                    RichText::new("TARGET TALK DURATION")
                        .font(FontId::new(10.5, theme::font::sans_600()))
                        .color(MUTED),
                );
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
                        if pill_btn(ui, None, lbl, active, false).clicked() {
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
                    if tiny_text_btn(ui, "−1m").clicked() {
                        p.store.target_minutes = Some(cur.saturating_sub(1).max(1));
                        p.store.countdown = true;
                        p.store_dirty = true;
                    }
                    if tiny_text_btn(ui, "+1m").clicked() {
                        p.store.target_minutes = Some((cur + 1).min(240));
                        p.store.countdown = true;
                        p.store_dirty = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mode_lbl = if p.store.countdown {
                            "Showing remaining"
                        } else {
                            "Showing elapsed"
                        };
                        if tiny_text_btn(ui, mode_lbl).clicked() {
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

fn export_pdf_popover(p: &mut Presentation, ctx: &egui::Context, bounds: Rect, anchor: Rect) {
    let margin = 12.0_f32.min(bounds.width().min(bounds.height()) * 0.25);
    let pop_w = 270.0_f32.min((bounds.width() - margin * 2.0).max(1.0));
    let pop_h = 122.0_f32.min((bounds.height() - margin * 2.0).max(1.0));
    let min_x = bounds.left() + margin;
    let max_x = (bounds.right() - pop_w - margin).max(min_x);
    let min_y = bounds.top() + margin;
    let max_y = (bounds.bottom() - pop_h - margin).max(min_y);
    let pop = Rect::from_min_size(
        pos2(
            (anchor.right() - pop_w).clamp(min_x, max_x),
            (anchor.top() - pop_h - 8.0).clamp(min_y, max_y),
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
            ui.painter().add(
                egui::Shadow {
                    offset: [0, 8],
                    blur: 24,
                    spread: 0,
                    color: Color32::from_black_alpha(160),
                }
                .as_shape(pop, CornerRadius::same(10)),
            );
            ui.painter().rect(
                pop,
                CornerRadius::same(10),
                CARD,
                Stroke::new(1.0, LINE),
                StrokeKind::Inside,
            );
            if pop.width() > 28.0 && pop.height() > 28.0 {
                let inner = pop.shrink(12.0);
                let mut c = ui.new_child(egui::UiBuilder::new().max_rect(inner));
                c.set_clip_rect(inner);
                c.spacing_mut().item_spacing.y = 8.0;
                c.label(
                    RichText::new("EXPORT ANNOTATED PDF")
                        .font(FontId::new(10.5, theme::font::sans_600()))
                        .color(MUTED),
                );
                if menu_row(
                    &mut c,
                    icons::FILE_TEXT,
                    "Slides + speaker notes (handout)…",
                    true,
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
                if menu_row(&mut c, icons::PRESENTATION, "Annotated slides only…", false).clicked()
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

fn shortcuts_overlay(p: &mut Presentation, ui: &mut Ui, full: Rect) {
    let scrim = ui.interact(
        full,
        ui.scope_id().with("presenter-shortcuts-scrim"),
        Sense::click(),
    );
    ui.painter()
        .rect_filled(full, 0, Color32::from_rgba_unmultiplied(6, 9, 13, 185));
    let w = (full.width() - 48.0).clamp(0.0, 560.0);
    let h = (full.height() - 48.0).clamp(0.0, 520.0);
    let card = Rect::from_center_size(full.center(), vec2(w, h));
    let _card_blocker = ui.interact(
        card,
        ui.scope_id().with("presenter-shortcuts-card"),
        Sense::click(),
    );
    ui.painter().rect(
        card,
        CornerRadius::same(14),
        CARD,
        Stroke::new(1.0, LINE),
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
                .color(INK),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_btn(ui, icons::X, "Close shortcuts (Esc)", false, false).clicked() {
                p.shortcuts_open = false;
            }
        });
    });

    let items: [(&str, &str); 14] = [
        ("→ / Space / Click", "Next slide"),
        ("← / Backspace", "Previous slide"),
        ("0–9 then Enter", "Jump to slide number"),
        ("Home / End", "First / last slide"),
        ("P", "Open presenter console"),
        ("G", "Slide overview grid"),
        ("M", "Toggle bottom filmstrip"),
        ("B or .", "Blackout audience screen"),
        ("W", "Whiteout audience screen"),
        ("F", "Freeze audience on current slide"),
        ("L / S", "Laser pointer / Spotlight"),
        ("D / Ctrl+Z / C", "Draw pen / Undo stroke / Clear slide"),
        ("T / R", "Start-pause / Reset timer"),
        ("? or Esc", "This sheet / Exit presentation"),
    ];
    egui::ScrollArea::vertical()
        .id_salt("presenter-shortcuts-scroll")
        .auto_shrink([false, false])
        .show(&mut c, |ui| {
            egui::Grid::new("presenter-shortcuts-grid")
                .num_columns(2)
                .spacing(vec2(18.0, 8.0))
                .show(ui, |ui| {
                    for (keys, desc) in items {
                        egui::Frame::new()
                            .fill(BTN)
                            .stroke(Stroke::new(1.0, LINE))
                            .corner_radius(CornerRadius::same(5))
                            .inner_margin(Margin::symmetric(8, 3))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(keys)
                                        .font(FontId::new(11.5, theme::font::mono_500()))
                                        .color(ACCENT),
                                );
                            });
                        ui.label(RichText::new(desc).size(13.0).color(INK));
                        ui.end_row();
                    }
                });
        });
}

// ── Studio-dark control primitives ───────────────────────────────────────────

fn sep_v(ui: &mut Ui, h: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(6.0, h), Sense::hover());
    ui.painter()
        .vline(r.center().x, r.y_range(), Stroke::new(1.0, LINE));
}

fn status_badge(ui: &mut Ui, text: &str, fg: Color32, bg: Color32) -> egui::Response {
    let g = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::new(11.0, theme::font::sans_600()),
        fg,
    );
    let (rect, r) = ui.allocate_exact_size(g.size() + vec2(16.0, 10.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    ui.painter().rect(
        rect,
        CornerRadius::same(99),
        bg,
        Stroke::new(1.0, fg.gamma_multiply(0.65)),
        StrokeKind::Inside,
    );
    ui.painter().galley(rect.center() - g.size() / 2.0, g, fg);
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn pill_btn(
    ui: &mut Ui,
    icon: Option<Icon>,
    text: &str,
    active: bool,
    disabled: bool,
) -> egui::Response {
    let g = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::new(13.0, theme::font::sans_500()),
        INK,
    );
    let icon_w = if icon.is_some() { 18.0 } else { 0.0 };
    let (rect, r) = ui.allocate_exact_size(
        vec2(g.size().x + icon_w + 20.0, 30.0),
        if disabled {
            Sense::hover()
        } else {
            Sense::click()
        },
    );
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, !disabled, text));
    let alpha = if disabled { 0.38 } else { 1.0 };
    let (fill, border, fg) = if active {
        (BTN_ACTIVE, ACCENT.gamma_multiply(0.8), INK)
    } else if r.hovered() && !disabled {
        (BTN_HOVER, Color32::from_rgb(0x3a, 0x49, 0x59), INK)
    } else {
        (BTN, LINE, INK)
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(7),
        fill.gamma_multiply(alpha),
        Stroke::new(1.0, border.gamma_multiply(alpha)),
        StrokeKind::Inside,
    );
    let content_w = g.size().x + icon_w;
    let mut x = rect.center().x - content_w * 0.5;
    if let Some(ic) = icon {
        let ir = Rect::from_center_size(pos2(x + 7.0, rect.center().y), vec2(14.0, 14.0));
        icons::paint(
            ui.painter(),
            ir,
            ic,
            (if active { ACCENT } else { fg }).gamma_multiply(alpha),
        );
        x += icon_w;
    }
    ui.painter().galley(
        pos2(x, rect.center().y - g.size().y * 0.5),
        g,
        fg.gamma_multiply(alpha),
    );
    if disabled {
        r
    } else {
        r.on_hover_cursor(egui::CursorIcon::PointingHand)
    }
}

fn small_tool_btn(ui: &mut Ui, icon: Icon, text: &str, active: bool) -> egui::Response {
    let g = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::new(11.5, theme::font::sans_500()),
        INK,
    );
    let (rect, r) = ui.allocate_exact_size(vec2(g.size().x + 30.0, 24.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    let (fill, border, ic_col) = if active {
        (BTN_ACTIVE, ACCENT, ACCENT)
    } else if r.hovered() {
        (BTN_HOVER, LINE, INK)
    } else {
        (BTN, LINE_SUBTLE, MUTED)
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(6),
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    let ir = Rect::from_center_size(pos2(rect.left() + 13.0, rect.center().y), vec2(12.5, 12.5));
    icons::paint(ui.painter(), ir, icon, ic_col);
    ui.painter().galley(
        pos2(rect.left() + 23.0, rect.center().y - g.size().y * 0.5),
        g,
        if active { INK } else { MUTED },
    );
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn icon_btn(ui: &mut Ui, icon: Icon, label: &str, active: bool, disabled: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(
        vec2(30.0, 30.0),
        if disabled {
            Sense::hover()
        } else {
            Sense::click()
        },
    );
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, !disabled, label));
    let alpha = if disabled { 0.35 } else { 1.0 };
    let (fill, border, col) = if active {
        (BTN_ACTIVE, ACCENT, ACCENT)
    } else if r.hovered() && !disabled {
        (BTN_HOVER, LINE, INK)
    } else {
        (BTN, LINE_SUBTLE, MUTED)
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(7),
        fill.gamma_multiply(alpha),
        Stroke::new(1.0, border.gamma_multiply(alpha)),
        StrokeKind::Inside,
    );
    icons::paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), vec2(14.5, 14.5)),
        icon,
        col.gamma_multiply(alpha),
    );
    let r = r.on_hover_text(label);
    if disabled {
        r
    } else {
        r.on_hover_cursor(egui::CursorIcon::PointingHand)
    }
}

fn tiny_text_btn(ui: &mut Ui, text: &str) -> egui::Response {
    let g = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::new(11.5, theme::font::sans_500()),
        MUTED,
    );
    let (rect, r) = ui.allocate_exact_size(vec2(g.size().x + 12.0, 22.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    ui.painter().rect(
        rect,
        CornerRadius::same(5),
        if r.hovered() { BTN_HOVER } else { BTN },
        Stroke::new(1.0, LINE_SUBTLE),
        StrokeKind::Inside,
    );
    ui.painter().galley(
        rect.center() - g.size() / 2.0,
        g,
        if r.hovered() { INK } else { MUTED },
    );
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn menu_row(ui: &mut Ui, icon: Icon, text: &str, primary: bool) -> egui::Response {
    let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    let fill = if r.hovered() {
        BTN_HOVER
    } else if primary {
        BTN_ACTIVE
    } else {
        BTN
    };
    let border = if primary {
        ACCENT.gamma_multiply(0.6)
    } else {
        LINE_SUBTLE
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(7),
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    icons::paint(
        ui.painter(),
        Rect::from_center_size(pos2(rect.left() + 16.0, rect.center().y), vec2(14.0, 14.0)),
        icon,
        if primary { ACCENT } else { MUTED },
    );
    ui.painter().text(
        pos2(rect.left() + 30.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::new(12.5, theme::font::sans_500()),
        INK,
    );
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn end_button(ui: &mut Ui) -> egui::Response {
    let text = "End presentation";
    let g = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::new(13.0, theme::font::sans_500()),
        INK,
    );
    let (rect, r) = ui.allocate_exact_size(vec2(g.size().x + 24.0, 30.0), Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    ui.painter().rect(
        rect,
        CornerRadius::same(7),
        if r.hovered() { DANGER_HOVER } else { DANGER_BG },
        Stroke::new(1.0, DANGER.gamma_multiply(0.65)),
        StrokeKind::Inside,
    );
    ui.painter().galley(rect.center() - g.size() / 2.0, g, INK);
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}
