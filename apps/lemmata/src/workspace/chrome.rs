//! The editor screen's frame: Toolbar.svelte, Sidebar.svelte with Tree.svelte and Outline.svelte,
//! TabStrip.svelte, EditorToolbar.svelte, the status bar of App.svelte, and their menus.

use super::panels::SidePanel;
use super::{Action, Menu, Tone, Workspace, tree};
use crate::backend::Backend;
use crate::editor::commands::{self as cmd, Indent};
use crate::editor::view;
use crate::icons::{self, Icon};
use crate::shortcuts::{display, keys_of};
use crate::store::Settings;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, ChipTone, Kind, MenuItem, MenuResult, Size};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2,
    vec2,
};
use lemmata_project::Engine;

const ENGINES: [(Engine, &str); 4] = [
    (Engine::Pdflatex, "pdfLaTeX"),
    (Engine::Xelatex, "XeLaTeX"),
    (Engine::Lualatex, "LuaLaTeX"),
    (Engine::Tectonic, "Tectonic"),
];

fn engine_name(e: Engine) -> &'static str {
    ENGINES
        .iter()
        .find(|(x, _)| *x == e)
        .map_or("pdfLaTeX", |(_, n)| n)
}

// ── Left Icon Rail ─────────────────────────────────────────────────────────

pub fn rail(
    ui: &mut Ui,
    p: &Palette,
    ws: &mut Workspace,
    settings: &mut Settings,
    backend: &Backend,
) -> Option<Action> {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, p.app);
    let mut action = None;

    // Top logo mark (Version 2 Typeset Split) + 8 panel buttons.
    let top_rect = Rect::from_min_max(
        pos2(rect.left(), rect.top() + 8.0),
        pos2(rect.right(), rect.bottom() - 128.0),
    );
    let mut top = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(top_rect)
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    top.spacing_mut().item_spacing.y = 4.0;
    let logo_resp = widgets::logo_button(&mut top, p, 32.0, "Lemmata");
    if logo_resp.on_hover_text("Back to all projects").clicked() {
        action = Some(Action::Library);
    }
    top.add_space(10.0);

    let errors = ws.build.errors();
    let warnings = ws.build.warnings();
    let issues = errors + warnings;
    let git_changes = ws.git.status.len();
    let open_comments = ws.comments.threads.iter().filter(|t| !t.resolved).count();
    let open = settings.sidebar_open && !ws.focus_mode;

    let nav_items: [(Icon, &str, &str, SidePanel); 3] = [
        (icons::FILES, "Files", "Project files", SidePanel::Files),
        (
            icons::LIST_TREE,
            "Outline",
            "Document outline",
            SidePanel::Outline,
        ),
        (
            icons::SEARCH,
            "Search",
            "Search in project (Ctrl+Shift+F)",
            SidePanel::Search,
        ),
    ];
    for (icon, label, tip, panel) in nav_items {
        if widgets::rail_button(
            &mut top,
            p,
            icon,
            label,
            open && ws.side_panel == panel,
            true,
            None,
        )
        .on_hover_text(tip)
        .clicked()
            && ws.toggle_side(panel, backend, settings)
        {
            action = Some(Action::Settings);
        }
    }

    // Subtle 22 px horizontal divider between navigation and tools.
    top.add_space(3.0);
    let (sep_r, _) = top.allocate_exact_size(vec2(22.0, 1.0), Sense::hover());
    top.painter().hline(
        sep_r.x_range(),
        sep_r.center().y,
        Stroke::new(1.0, p.border),
    );
    top.add_space(3.0);

    let prob_badge = (issues > 0).then_some((
        issues,
        if errors > 0 {
            ChipTone::Error
        } else {
            ChipTone::Warn
        },
    ));
    let git_badge = (git_changes > 0).then_some((git_changes, ChipTone::Accent));
    let comment_badge = (open_comments > 0).then_some((open_comments, ChipTone::Accent));
    let git_tip = if ws.trusted {
        "Commit, pull and push with git"
    } else {
        "Git (disabled in restricted projects)"
    };
    type ToolItem<'a> = (Icon, &'a str, &'a str, SidePanel, Option<(usize, ChipTone)>);
    let tool_items: [ToolItem<'_>; 5] = [
        (
            icons::CIRCLE_ALERT,
            "Problems",
            "Build errors and warnings",
            SidePanel::Problems,
            prob_badge,
        ),
        (icons::GIT_BRANCH, "Git", git_tip, SidePanel::Git, git_badge),
        (
            icons::HISTORY,
            "History",
            "Snapshots of the project, to compare and restore",
            SidePanel::History,
            None,
        ),
        (
            icons::MESSAGE_SQUARE,
            "Comments",
            "Review comments",
            SidePanel::Comments,
            comment_badge,
        ),
        (
            icons::CIRCLE_CHECK,
            "To-dos",
            "TODO and FIXME notes",
            SidePanel::Todos,
            None,
        ),
    ];
    for (icon, label, tip, panel, badge) in tool_items {
        if widgets::rail_button(
            &mut top,
            p,
            icon,
            label,
            open && ws.side_panel == panel,
            true,
            badge,
        )
        .on_hover_text(tip)
        .clicked()
            && ws.toggle_side(panel, backend, settings)
        {
            action = Some(Action::Settings);
        }
    }

    // Bottom pinned utilities: Notifications bell, All projects, Settings/Preferences.
    let bot_rect = Rect::from_min_max(
        pos2(rect.left(), rect.bottom() - 126.0),
        pos2(rect.right(), rect.bottom() - 10.0),
    );
    let mut bot = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(bot_rect)
            .layout(egui::Layout::bottom_up(egui::Align::Center)),
    );
    bot.spacing_mut().item_spacing.y = 4.0;
    if widgets::rail_button(
        &mut bot,
        p,
        icons::SETTINGS,
        "Preferences",
        false,
        false,
        None,
    )
    .on_hover_text("Settings & preferences")
    .clicked()
    {
        action = Some(Action::Preferences);
    }
    if widgets::rail_button(
        &mut bot,
        p,
        icons::LAYOUT_GRID,
        "Back to all projects",
        false,
        false,
        None,
    )
    .on_hover_text("All projects")
    .clicked()
    {
        action = Some(Action::Library);
    }
    let bell_badge = (ws.unread_notices > 0).then_some((ws.unread_notices, ChipTone::Accent));
    let bell = widgets::rail_button(
        &mut bot,
        p,
        icons::BELL,
        "Notifications",
        matches!(ws.menu, Some(Menu::Notices(_))),
        true,
        bell_badge,
    )
    .on_hover_text(if ws.notice_log.is_empty() {
        "Notifications (none yet)"
    } else {
        "Notifications"
    });
    if bell.clicked() {
        ws.menu = if matches!(ws.menu, Some(Menu::Notices(_))) {
            None
        } else {
            ws.unread_notices = 0;
            Some(Menu::Notices(bell.rect))
        };
    }

    action
}

// ── Toolbar ────────────────────────────────────────────────────────────────

pub fn toolbar(
    ui: &mut Ui,
    p: &Palette,
    ws: &mut Workspace,
    settings: &mut Settings,
    backend: &Backend,
) -> Option<Action> {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, p.app);
    let mut action = None;
    let inner = Rect::from_min_max(
        pos2(rect.left() + 4.0, rect.top()),
        pos2(rect.right() - 8.0, rect.bottom()),
    );

    // Center: Editor | Split | PDF segmented control.
    let narrow = rect.width() < 960.0;
    let layout_options = [
        (Some(icons::FILE_CODE), "Editor"),
        (Some(icons::COLUMNS_2), "Split"),
        (Some(icons::FILE_TEXT), "PDF"),
    ];
    let center_w = widgets::segmented_width(ui, &layout_options, narrow);
    let center =
        Rect::from_center_size(pos2(rect.center().x, rect.center().y), vec2(center_w, 32.0));
    let mut c = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(center)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let layouts = ["editor", "split", "pdf"];
    let current = layouts
        .iter()
        .position(|l| *l == settings.layout)
        .unwrap_or(1);
    if let Some(i) = widgets::segmented(&mut c, p, &layout_options, current, narrow) {
        settings.layout = layouts[i].into();
        action = Some(Action::Settings);
    }

    // Left: Project menu trigger (project name + chevron) + / + active file path.
    let left = Rect::from_min_max(inner.min, pos2(center.left() - 12.0, inner.bottom()));
    let mut l = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(left)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    l.spacing_mut().item_spacing.x = 6.0;
    let proj_font = FontId::new(13.5, theme::font::sans_600());
    let proj_w = (l
        .painter()
        .layout_no_wrap(ws.info.name.clone(), proj_font.clone(), p.ink)
        .size()
        .x
        + 32.0)
        .min((left.width() * 0.65).max(80.0));
    let (prect, presp) = l.allocate_exact_size(vec2(proj_w, 30.0), Sense::click());
    presp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Project menu"));
    let menu_on = matches!(ws.menu, Some(Menu::Main(_)));
    if menu_on || presp.hovered() {
        l.painter().rect_filled(
            prect,
            CornerRadius::same(theme::R_MD),
            if menu_on { p.rail_active } else { p.surface_3 },
        );
    }
    let mut pjob = egui::text::LayoutJob::simple_singleline(ws.info.name.clone(), proj_font, p.ink);
    pjob.wrap.max_width = (prect.width() - 30.0).max(20.0);
    pjob.wrap.max_rows = 1;
    pjob.wrap.overflow_character = Some('…');
    let pg = l.fonts_mut(|f| f.layout_job(pjob));
    let pg_w = pg.size().x;
    l.painter().galley(
        pos2(prect.left() + 8.0, prect.center().y - pg.size().y / 2.0),
        pg,
        p.ink,
    );
    icons::paint(
        l.painter(),
        Rect::from_center_size(
            pos2(
                (prect.left() + 8.0 + pg_w + 10.0).min(prect.right() - 10.0),
                prect.center().y,
            ),
            Vec2::splat(13.0),
        ),
        icons::CHEVRON_DOWN,
        p.muted,
    );
    widgets::focus_ring(&l, &presp, prect, p, theme::R_MD);
    if presp
        .on_hover_text("Project menu")
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        ws.menu = if menu_on {
            None
        } else {
            Some(Menu::Main(prect))
        };
    }
    if let Some(active) = ws.active.as_deref()
        && l.available_width() > 64.0
    {
        l.label(egui::RichText::new("/").size(13.0).color(p.faint));
        l.add(egui::Label::new(egui::RichText::new(active).size(12.5).color(p.muted)).truncate());
    }

    // Right (right-to-left): Recompile split button, Build targets (if configured),
    // Build status indicator, Command palette pill (Ctrl+Shift+P).
    let right = Rect::from_min_max(pos2(center.right() + 12.0, inner.top()), inner.max);
    let mut r = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(right)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    r.spacing_mut().item_spacing.x = 0.0;
    let caret = Button::icon(icons::CHEVRON_DOWN, "Compile options")
        .kind(Kind::Primary)
        .radius(CornerRadius {
            nw: 0,
            sw: 0,
            ne: theme::R_MD,
            se: theme::R_MD,
        })
        .icon_size(14.0)
        .show_icon(&mut r, p);
    let div_col =
        Color32::from_rgba_unmultiplied(p.accent_ink.r(), p.accent_ink.g(), p.accent_ink.b(), 64);
    r.painter().vline(
        caret.rect.left(),
        caret.rect.y_range(),
        Stroke::new(1.0, div_col),
    );
    if caret.clicked() {
        ws.menu = if matches!(ws.menu, Some(Menu::Compile(_))) {
            None
        } else {
            Some(Menu::Compile(caret.rect))
        };
    }
    let building = ws.build.building;
    let angle = if building {
        r.ctx().request_repaint_after(theme::ANIMATION_FRAME);
        (r.input(|i| i.time) as f32 * std::f32::consts::TAU).rem_euclid(std::f32::consts::TAU)
    } else {
        0.0
    };
    let recompile = Button::new(if building { "Stop" } else { "Recompile" })
        .with_icon(icons::REFRESH_CW)
        .icon_angle(angle)
        .icon_size(14.0)
        .kind(Kind::Primary)
        .min_width(114.0)
        .radius(CornerRadius {
            nw: theme::R_MD,
            sw: theme::R_MD,
            ne: 0,
            se: 0,
        })
        .show(&mut r, p);
    if recompile.clicked() {
        if building {
            ws.stop_build(backend);
        } else {
            ws.recompile(backend, settings);
        }
    }
    let _ = recompile.on_hover_text(if building {
        "Stop compiling".to_owned()
    } else {
        format!("Recompile ({})", display(keys_of("recompile")))
    });

    r.add_space(8.0);
    r.spacing_mut().item_spacing.x = 8.0;

    // Optional build targets picker (only when lemmata.toml defines targets).
    if !ws.info.config.targets.is_empty() && r.available_width() > 96.0 {
        let active_target = ws
            .info
            .config
            .targets
            .iter()
            .find(|t| t.main == ws.info.main && t.engine.is_none_or(|e| e == ws.info.config.engine))
            .map(|t| t.name.as_str())
            .unwrap_or_else(|| tree::base_name(&ws.info.main));
        let tbtn = Button::new(&format!("{active_target} ▾"))
            .kind(Kind::Ghost)
            .size(Size::Sm)
            .on(matches!(ws.menu, Some(Menu::Targets(_))))
            .show(&mut r, p)
            .on_hover_text("Switch build target");
        if tbtn.clicked() {
            ws.menu = if matches!(ws.menu, Some(Menu::Targets(_))) {
                None
            } else {
                Some(Menu::Targets(tbtn.rect))
            };
        }
    }

    // Build status indicator in the top bar.
    if let Some(o) = ws.build.outcome.as_ref().filter(|_| !ws.build.building)
        && r.available_width() > 90.0
    {
        use lemmata_tex::Status;
        if o.status == Status::Success {
            let secs = o.duration_ms as f64 / 1000.0;
            let visual = format!("Compiled {secs:.1} s");
            let a11y = format!("Compiled in {secs:.1} s");
            let g = r.painter().layout_no_wrap(
                visual.clone(),
                FontId::new(12.0, theme::font::sans_500()),
                p.ok,
            );
            let (srect, sresp) =
                r.allocate_exact_size(vec2(14.0 + g.size().x, 24.0), Sense::hover());
            sresp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &a11y));
            r.painter()
                .circle_filled(pos2(srect.left() + 4.0, srect.center().y), 3.5, p.ok);
            r.painter().galley(
                pos2(srect.left() + 13.0, srect.center().y - g.size().y / 2.0),
                g,
                p.ok,
            );
            if let Some(at) = ws
                .build
                .finished_at
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            {
                sresp.on_hover_text(format!(
                    "Finished at {}",
                    crate::when::time_ms(at.as_millis() as f64)
                ));
            }
        } else {
            let errors = ws.build.errors();
            let text = match o.status {
                Status::Timeout => "Stopped: took too long".to_owned(),
                Status::Cancelled => "Stopped".to_owned(),
                _ if errors > 0 => {
                    format!("{errors} error{}", if errors == 1 { "" } else { "s" })
                }
                _ => "Compile failed".to_owned(),
            };
            if status_link(&mut r, text, p.error, "Build status: show problems", true).clicked()
                && ws.open_side(SidePanel::Problems, backend, settings)
            {
                action = Some(Action::Settings);
            }
        }
    }

    // Command palette pill (opens Command Palette — kept separate from Quick Open).
    let shortcut = display(keys_of("palette"));
    let kbd_g = r
        .painter()
        .layout_no_wrap(shortcut.clone(), FontId::monospace(10.5), p.faint);
    let pill_w = (26.0 + 74.0 + 8.0 + kbd_g.size().x + 16.0).max(204.0);
    if rect.width() >= 1140.0 && r.available_width() >= pill_w {
        let (prect, presp) = r.allocate_exact_size(vec2(pill_w, 30.0), Sense::click());
        presp
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Command palette"));
        let fill = if presp.hovered() {
            p.surface_2
        } else {
            p.surface
        };
        r.painter().rect(
            prect,
            CornerRadius::same(theme::R_MD),
            fill,
            Stroke::new(1.0, p.border),
            StrokeKind::Inside,
        );
        icons::paint(
            r.painter(),
            Rect::from_center_size(
                pos2(prect.left() + 15.0, prect.center().y),
                Vec2::splat(13.0),
            ),
            icons::SEARCH,
            p.faint,
        );
        let kbd_r = Rect::from_min_size(
            pos2(
                prect.right() - 7.0 - kbd_g.size().x - 8.0,
                prect.center().y - 9.0,
            ),
            vec2(kbd_g.size().x + 8.0, 18.0),
        );
        r.painter()
            .rect_filled(kbd_r, CornerRadius::same(4), p.surface_2);
        r.painter()
            .galley(kbd_r.center() - kbd_g.size() / 2.0, kbd_g, p.faint);
        let lbl_clip = Rect::from_min_max(
            pos2(prect.left() + 26.0, prect.top()),
            pos2(kbd_r.left() - 6.0, prect.bottom()),
        );
        r.painter().with_clip_rect(lbl_clip).text(
            pos2(prect.left() + 26.0, prect.center().y),
            Align2::LEFT_CENTER,
            "Commands…",
            FontId::proportional(12.0),
            p.muted,
        );
        widgets::focus_ring(&r, &presp, prect, p, theme::R_MD);
        if presp
            .on_hover_text(format!("Command palette ({shortcut})"))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            ws.open_palette(super::palette::Kind::Commands, settings);
        }
    }

    action
}

// ── Files & Outline Sidebar Panels ─────────────────────────────────────────

pub fn files_panel(
    ui: &mut Ui,
    p: &Palette,
    ws: &mut Workspace,
    settings: &mut Settings,
    backend: &Backend,
) -> bool {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, p.app);

    let mut sort_rect = None;
    let mut act_click: Option<&'static str> = None;
    let rest = super::panels::side_head(ui, p, "Files", |ui| {
        for (icon, label) in [
            (icons::ARROW_UP_DOWN, "Sort and view"),
            (icons::UPLOAD, "Add files"),
            (icons::FOLDER_PLUS, "New folder"),
            (icons::FILE_PLUS, "New file"),
        ] {
            let b = Button::icon(icon, label)
                .kind(Kind::Ghost)
                .size(Size::Sm)
                .icon_size(14.0)
                .show_icon(ui, p);
            if b.clicked() {
                if label == "Sort and view" {
                    sort_rect = Some(b.rect);
                } else {
                    act_click = Some(label);
                }
            }
        }
    });
    match act_click {
        Some("Add files") => ws.upload(backend, String::new()),
        Some("New folder") => ws.new_item(true, String::new()),
        Some("New file") => ws.new_item(false, String::new()),
        _ => {}
    }
    if let Some(r) = sort_rect {
        ws.menu = if matches!(ws.menu, Some(Menu::Sort(_))) {
            None
        } else {
            Some(Menu::Sort(r))
        };
    }

    // Optional bottom Git branch footer when the project is a git repo.
    let branch_info = if ws.trusted {
        match &ws.git.overview {
            Some(lemmata_git::Overview::Repo(r)) => Some((
                r.branch.clone().unwrap_or_else(|| "detached".into()),
                ws.git.status.len(),
            )),
            _ => None,
        }
    } else {
        None
    };
    let footer_h = if branch_info.is_some() { 34.0 } else { 0.0 };
    let body = Rect::from_min_max(rest.min, pos2(rest.right(), rest.bottom() - footer_h));

    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
    egui::ScrollArea::vertical()
        .id_salt("tree")
        .auto_shrink(false)
        .show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            tree_rows(ui, p, ws, settings, backend);
            let rest_sp = ui.available_rect_before_wrap();
            let r = ui.interact(rest_sp, ui.scope_id().with("tree-space"), Sense::click());
            if r.clicked() {
                ws.sel.selected.clear();
                ws.sel.focused = true;
                ui.ctx().memory_mut(|m| {
                    if let Some(f) = m.focused() {
                        m.surrender_focus(f);
                    }
                });
            }
            if r.secondary_clicked()
                && let Some(at) = r.interact_pointer_pos()
            {
                ws.menu = Some(Menu::Tree(None, Rect::from_min_size(at, Vec2::ZERO)));
            }
        });

    if let Some((branch, changes)) = branch_info {
        let foot = Rect::from_min_max(pos2(rest.left(), body.bottom()), rest.max);
        ui.painter()
            .hline(foot.x_range(), foot.top() + 0.5, Stroke::new(1.0, p.border));
        let btn_r = foot.shrink2(vec2(6.0, 3.0));
        let resp = ui
            .interact(btn_r, ui.scope_id().with("files-git-foot"), Sense::click())
            .on_hover_text("Open Git panel")
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.hovered() {
            ui.painter()
                .rect_filled(btn_r, CornerRadius::same(6), p.rail_active);
        }
        icons::paint(
            ui.painter(),
            Rect::from_center_size(
                pos2(btn_r.left() + 14.0, btn_r.center().y),
                Vec2::splat(13.0),
            ),
            icons::GIT_BRANCH,
            p.muted,
        );
        ui.painter().text(
            pos2(btn_r.left() + 26.0, btn_r.center().y),
            Align2::LEFT_CENTER,
            &branch,
            FontId::new(12.0, theme::font::mono_500()),
            p.ink_2,
        );
        if changes > 0 {
            ui.painter().text(
                pos2(btn_r.right() - 8.0, btn_r.center().y),
                Align2::RIGHT_CENTER,
                format!("{changes}"),
                FontId::new(11.0, theme::font::sans_600()),
                p.accent_text,
            );
        }
        widgets::focus_ring(ui, &resp, btn_r, p, theme::R);
        if resp.clicked() {
            ws.open_side(SidePanel::Git, backend, settings);
            return true;
        }
    }
    false
}

pub fn outline_panel(ui: &mut Ui, p: &Palette, ws: &mut Workspace, backend: &Backend) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, p.app);
    let words = ws.total_words;
    let rest = super::panels::side_head(ui, p, "Outline", |ui| {
        if words > 0 {
            ui.label(
                egui::RichText::new(format!("{}w", super::sheets::group_digits(words)))
                    .font(FontId::monospace(11.0))
                    .color(p.faint),
            );
        }
    });
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rest));
    egui::ScrollArea::vertical()
        .id_salt("outline")
        .auto_shrink(false)
        .show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            outline_rows(ui, p, ws, backend);
        });
}

fn kind_icon(k: tree::Kind, p: &Palette) -> (Icon, Color32) {
    match k {
        tree::Kind::Tex => (icons::FILE_TEXT, p.accent_text),
        tree::Kind::Bib => (icons::BOOK_OPEN, p.atom),
        tree::Kind::Image => (icons::FILE_IMAGE, p.ok),
        tree::Kind::Code => (icons::BRACES, p.muted),
        tree::Kind::Config => (icons::SETTINGS, p.muted),
        tree::Kind::File => (icons::FILE, p.muted),
    }
}

fn tree_rows(ui: &mut Ui, p: &Palette, ws: &mut Workspace, settings: &Settings, backend: &Backend) {
    // Sorting and flattening a big tree every frame costs more than drawing it.
    let (sort, first) = (&settings.tree_sort, settings.tree_folders_first);
    if !ws
        .tree_rows
        .as_ref()
        .is_some_and(|r| r.fits(sort, first, &ws.collapsed))
    {
        ws.tree_rows = Some(tree::Rows::new(&ws.info.tree, sort, first, &ws.collapsed));
    }
    let rows = ws
        .tree_rows
        .as_ref()
        .map(|r| r.rows.clone())
        .unwrap_or_default();
    let w = ui.available_width();
    let mut open = None;
    let mut toggle = None;
    let mut click = None;
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let os_hover = ui.input(|i| !i.raw.hovered_files.is_empty());
    let dropped: Vec<String> = ui.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_string_lossy().into_owned())
            .collect()
    });
    // What the pointer is over this frame, for the next one's highlight.
    let mut over_folder: Option<String> = None;
    for (e, depth, count) in rows.iter() {
        let (rect, r) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click_and_drag());
        if ws.tree_cursor.as_deref() == Some(e.path.as_str()) {
            ws.sel.cursor_rect = Some(rect);
        }
        // Rows scrolled out of the panel only keep their place.
        if !ui.is_rect_visible(rect) {
            continue;
        }
        // A folder takes what is dropped on it; a file's row stands for its folder.
        let folder = if e.is_dir {
            e.path.clone()
        } else {
            super::files::parent_of(&e.path).to_owned()
        };
        if pointer.is_some_and(|pt| rect.contains(pt) && ui.clip_rect().contains(pt)) {
            over_folder = Some(folder.clone());
        }
        if r.drag_started() && !ui.input(|i| i.modifiers.command || i.modifiers.shift) {
            let paths = ws.targets(Some(&e.path));
            let name = if paths.len() > 1 {
                format!("{} items", paths.len())
            } else {
                e.name.clone()
            };
            ws.tree_drag = Some(super::files::TreeDrag {
                paths,
                name,
                target: None,
            });
        }
        let dragging = ws
            .tree_drag
            .as_ref()
            .is_some_and(|d| d.paths.contains(&e.path));
        let drop_here = e.is_dir
            && (ws
                .tree_drag
                .as_ref()
                .is_some_and(|d| d.target.as_deref() == Some(e.path.as_str()))
                || ws.os_drop.as_deref() == Some(e.path.as_str()));
        let row_pill = rect.shrink2(vec2(6.0, 0.0));
        if drop_here {
            ui.painter().rect(
                row_pill,
                CornerRadius::same(6),
                p.accent_soft,
                Stroke::new(1.0, p.accent),
                StrokeKind::Inside,
            );
        }
        let active = ws.active.as_deref() == Some(e.path.as_str());
        let cursor = ws.tree_cursor.as_deref() == Some(e.path.as_str());
        let selected = ws.sel.is_selected(&e.path);
        r.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::TreeItem,
                true,
                active || cursor || selected,
                &e.name,
            )
        });
        let painter = ui.painter();
        if selected && !active {
            painter.rect_filled(row_pill, CornerRadius::same(6), p.accent_soft);
        }
        if active {
            painter.rect_filled(row_pill, CornerRadius::same(6), p.rail_active);
        } else if r.hovered() {
            painter.rect_filled(row_pill, CornerRadius::same(6), p.surface_3);
        }
        let opens = tree::is_text_file(&e.path) || tree::is_viewer_file(&e.path);
        let ink = if active {
            p.ink
        } else if !e.is_dir && !opens {
            p.faint
        } else {
            p.ink_2
        };
        let mut x = rect.left() + 12.0 + *depth as f32 * 14.0;
        if e.is_dir {
            let open_dir = !ws.collapsed.contains(&e.path);
            icons::paint_rotated(
                painter,
                Rect::from_min_size(pos2(x, rect.center().y - 6.0), Vec2::splat(12.0)),
                icons::CHEVRON_RIGHT,
                p.faint,
                if open_dir {
                    std::f32::consts::FRAC_PI_2
                } else {
                    0.0
                },
            );
            x += 12.0 + 5.0;
            icons::paint(
                painter,
                Rect::from_min_size(pos2(x, rect.center().y - 7.0), Vec2::splat(14.0)),
                if open_dir {
                    icons::FOLDER_OPEN
                } else {
                    icons::FOLDER
                },
                p.warn,
            );
            x += 14.0 + 6.0;
            let has_git_dot = ws
                .git
                .status
                .iter()
                .any(|(path, _)| path.starts_with(&format!("{}/", e.path)));
            let count_g = (!open_dir).then(|| {
                painter.layout_no_wrap(count.to_string(), FontId::proportional(11.0), p.faint)
            });
            let right_reserve = if has_git_dot { 24.0 } else { 14.0 }
                + count_g.as_ref().map_or(0.0, |g| g.size().x + 18.0);
            let max_name_w = (rect.right() - right_reserve - x).max(20.0);
            let mut job = egui::text::LayoutJob::simple_singleline(
                e.name.clone(),
                FontId::new(
                    12.5,
                    if active {
                        theme::font::sans_500()
                    } else {
                        egui::FontFamily::Proportional
                    },
                ),
                ink,
            );
            job.wrap.max_width = max_name_w;
            job.wrap.max_rows = 1;
            job.wrap.overflow_character = Some('…');
            let ng = ui.fonts_mut(|f| f.layout_job(job));
            let name_w = ng.size().x.min(max_name_w);
            let painter = ui.painter();
            painter.galley(pos2(x, rect.center().y - ng.size().y / 2.0), ng, ink);
            if has_git_dot {
                painter.circle_filled(pos2(rect.right() - 16.0, rect.center().y), 3.0, p.warn);
            }
            if let Some(g) = count_g {
                let b = Rect::from_min_size(
                    pos2(x + name_w + 6.0, rect.center().y - 8.0),
                    vec2(g.size().x + 10.0, 16.0),
                );
                painter.rect_filled(b, CornerRadius::same(8), p.surface_3);
                painter.galley(b.center() - g.size() / 2.0, g, p.faint);
            }
            if r.clicked() {
                toggle = Some(e.path.clone());
            }
        } else {
            x += 14.0;
            let (icon, color) = kind_icon(tree::kind(&e.name), p);
            icons::paint(
                painter,
                Rect::from_min_size(pos2(x, rect.center().y - 7.0), Vec2::splat(14.0)),
                icon,
                color,
            );
            x += 14.0 + 6.0;
            let git_entry = ws.git.status.iter().find(|(path, _)| *path == e.path);
            let is_main = e.path == ws.info.main;
            let right_reserve = 14.0
                + if is_main { 46.0 } else { 0.0 }
                + if git_entry.is_some() { 18.0 } else { 0.0 };
            let max_name_w = (rect.right() - right_reserve - x).max(20.0);
            let mut job = egui::text::LayoutJob::simple_singleline(
                e.name.clone(),
                FontId::new(
                    12.5,
                    if active {
                        theme::font::sans_500()
                    } else {
                        egui::FontFamily::Proportional
                    },
                ),
                ink,
            );
            job.wrap.max_width = max_name_w;
            job.wrap.max_rows = 1;
            job.wrap.overflow_character = Some('…');
            let name = ui.fonts_mut(|f| f.layout_job(job));
            let painter = ui.painter();
            painter.galley(pos2(x, rect.center().y - name.size().y / 2.0), name, ink);
            let mut git_note = String::new();
            if let Some((_, st)) = git_entry {
                let (c, title) = match st.as_str() {
                    "M" => (p.warn, "Changed since the last git commit"),
                    "R" => (p.warn, "Renamed, not committed yet"),
                    "A" => (p.ok, "Added to git, not committed yet"),
                    "?" => (p.ok, "Not tracked by git"),
                    "D" => (p.error, "Deleted, not committed yet"),
                    _ => (p.error, "Merge conflict"),
                };
                let right = if is_main {
                    rect.right() - 60.0
                } else {
                    rect.right() - 12.0
                };
                let gr =
                    Rect::from_min_size(pos2(right - 14.0, rect.top()), vec2(14.0, rect.height()));
                painter.text(
                    gr.center(),
                    Align2::CENTER_CENTER,
                    st,
                    FontId::new(10.5, theme::font::mono_500()),
                    c,
                );
                git_note = format!("\n{title}");
            }
            if is_main {
                let mut job = egui::text::LayoutJob::default();
                job.append(
                    "MAIN",
                    0.0,
                    egui::TextFormat {
                        font_id: FontId::new(9.5, theme::font::sans_600()),
                        color: p.accent_text,
                        extra_letter_spacing: 0.65,
                        ..Default::default()
                    },
                );
                let g = ui.fonts_mut(|f| f.layout_job(job));
                let b = Rect::from_min_size(
                    pos2(
                        rect.right() - 12.0 - g.size().x - 8.0,
                        rect.center().y - 7.5,
                    ),
                    vec2(g.size().x + 8.0, 15.0),
                );
                let painter = ui.painter();
                painter.rect_filled(b, CornerRadius::same(4), p.accent_soft);
                painter.galley(b.center() - g.size() / 2.0, g, p.accent_text);
            }
            let tip = format!(
                "{}{}{}{git_note}",
                e.path,
                if opens { "" } else { " (not a text file)" },
                if e.size > 0 {
                    format!("\n{}", tree::format_size(e.size))
                } else {
                    String::new()
                }
            );
            if r.clone().on_hover_text(tip).clicked() && opens {
                open = Some(e.path.clone());
            }
        }
        if dragging {
            ui.painter()
                .rect_filled(row_pill, CornerRadius::same(6), p.app.gamma_multiply(0.55));
        }
        if r.clicked() {
            click = Some((e.path.clone(), ui.input(|i| i.modifiers)));
        }
        if r.secondary_clicked()
            && let Some(at) = r.interact_pointer_pos()
        {
            ws.menu = Some(Menu::Tree(
                Some(e.path.clone()),
                Rect::from_min_size(at, Vec2::ZERO),
            ));
        }
    }
    tree_drops(ui, p, ws, backend, settings, over_folder, os_hover, dropped);
    if let Some((path, mods)) = click {
        let paths: Vec<String> = rows.iter().map(|(e, ..)| e.path.clone()).collect();
        ws.tree_click(&path, &paths, mods);
        ui.ctx().memory_mut(|m| {
            if let Some(f) = m.focused() {
                m.surrender_focus(f);
            }
        });
        // With Ctrl or Shift a click only selects.
        if mods.command || mods.shift {
            open = None;
            toggle = None;
        }
    }
    if let Some(t) = toggle
        && !ws.collapsed.remove(&t)
    {
        ws.collapsed.insert(t);
    }
    if let Some(path) = open {
        ws.open_file(backend, &path);
    }
}

/// The pointer's part of dragging: where files dragged in the tree or from the computer would go,
/// and what happens when they are let go. `over` is the folder of the row under the pointer;
/// below the rows is the project root.
#[allow(clippy::too_many_arguments)]
fn tree_drops(
    ui: &mut Ui,
    p: &Palette,
    ws: &mut Workspace,
    backend: &Backend,
    settings: &Settings,
    over: Option<String>,
    os_hover: bool,
    dropped: Vec<String>,
) {
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let rest = ui.available_rect_before_wrap();
    let in_tree = pointer.is_some_and(|pt| ui.clip_rect().contains(pt));
    let folder = over
        .or_else(|| (in_tree && pointer.is_some_and(|pt| pt.y >= rest.top())).then(String::new));
    if let Some(d) = &mut ws.tree_drag {
        d.target = folder
            .clone()
            .filter(|f| super::files::can_move(&d.paths, f));
        if ui.input(|i| !i.pointer.any_down()) {
            let d = ws.tree_drag.take().expect("dragging");
            if let Some(f) = d.target {
                ws.move_paths(backend, settings, d.paths, f);
            }
        } else if let Some(pt) = pointer {
            // The ghost: the name next to the pointer.
            egui::Area::new(egui::Id::unique("tree-ghost"))
                .order(egui::Order::Tooltip)
                .fixed_pos(pt + vec2(12.0, 8.0))
                .interactable(false)
                .show(ui.ctx(), |ui| {
                    egui::Frame::new()
                        .fill(p.surface)
                        .stroke(Stroke::new(1.0, p.border))
                        .corner_radius(crate::theme::R_MD)
                        .shadow(p.shadow)
                        .inner_margin(egui::Margin::symmetric(9, 4))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(&d.name).size(12.5).color(p.ink))
                        });
                });
        }
    }
    ws.os_drop = if os_hover { folder.clone() } else { None };
    if ws
        .tree_drag
        .as_ref()
        .is_some_and(|d| d.target.as_deref() == Some(""))
        || ws.os_drop.as_deref() == Some("")
    {
        ui.painter().rect_stroke(
            ui.clip_rect(),
            CornerRadius::same(theme::R_MD),
            Stroke::new(2.0, p.accent),
            StrokeKind::Inside,
        );
    }
    if !dropped.is_empty()
        && let Some(f) = folder
    {
        ws.add_dropped(backend, dropped, f);
    }
}

fn outline_rows(ui: &mut Ui, p: &Palette, ws: &mut Workspace, backend: &Backend) {
    if ws.headings.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new("Add a \\section to see the outline here.")
                    .size(12.0)
                    .color(p.muted),
            );
        });
        return;
    }
    let min_level = ws
        .headings
        .iter()
        .map(|h| h.level)
        .min()
        .unwrap_or(0)
        .min(4);
    let totals: Vec<u32> = (0..ws.headings.len())
        .map(|i| {
            let h = &ws.headings[i];
            ws.headings[i..]
                .iter()
                .enumerate()
                .take_while(|(k, x)| *k == 0 || x.level > h.level)
                .map(|(_, x)| x.words)
                .sum()
        })
        .collect();
    let current = ws.current_heading();
    let keys: Vec<String> = {
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        ws.headings
            .iter()
            .map(|h| {
                let base = format!("{}:{}:{}", h.file, h.level, h.title);
                let n = seen.entry(base.clone()).or_default();
                *n += 1;
                format!("{base}:{n}")
            })
            .collect()
    };
    let has_children =
        |i: usize| i + 1 < ws.headings.len() && ws.headings[i + 1].level > ws.headings[i].level;
    let w = ui.available_width();
    let mut hide_below: Option<u8> = None;
    let mut jump = None;
    let mut fold = None;
    for (i, h) in ws.headings.iter().enumerate() {
        if let Some(l) = hide_below {
            if h.level > l {
                continue;
            }
            hide_below = None;
        }
        let folded = ws.outline_folded.contains(&keys[i]);
        if folded && has_children(i) {
            hide_below = Some(h.level);
        }
        let (rect, _) = ui.allocate_exact_size(vec2(w, 27.0), Sense::hover());
        if !ui.is_rect_visible(rect) {
            continue;
        }
        let row_pill = rect.shrink2(vec2(6.0, 0.0));
        let indent = 10.0 + (h.level.saturating_sub(min_level)) as f32 * 13.0;
        let twist = Rect::from_min_size(pos2(rect.left() + indent, rect.top()), vec2(14.0, 27.0));
        let entry = Rect::from_min_max(pos2(twist.right(), rect.top()), rect.max);
        let er = ui.interact(entry, ui.make_persistent_id(("heading", i)), Sense::click());
        let title = if h.title.is_empty() {
            "(untitled)".to_owned()
        } else {
            h.title.clone()
        };
        er.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &title));
        let on = current == Some(i);
        if on {
            ui.painter()
                .rect_filled(row_pill, CornerRadius::same(6), p.rail_active);
        } else if er.hovered() {
            ui.painter()
                .rect_filled(row_pill, CornerRadius::same(6), p.surface_3);
        }
        if has_children(i) {
            let tr = ui.interact(twist, ui.make_persistent_id(("twist", i)), Sense::click());
            tr.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Button,
                    true,
                    if folded { "Expand" } else { "Collapse" },
                )
            });
            let g = ui
                .painter()
                .layout_no_wrap("▾".into(), FontId::proportional(11.0), p.faint);
            let center = twist.center();
            let rot = if folded {
                -std::f32::consts::FRAC_PI_2
            } else {
                0.0
            };
            ui.painter().add(
                egui::epaint::TextShape::new(center - g.size() / 2.0, g, p.faint)
                    .with_angle_and_anchor(rot, Align2::CENTER_CENTER),
            );
            if tr.clicked() {
                fold = Some(keys[i].clone());
            }
        }
        let ink = if on {
            p.ink
        } else if er.hovered() || h.level == min_level {
            p.ink_2
        } else {
            p.muted
        };
        let n = format!("{}w", totals[i]);
        let ng = ui
            .painter()
            .layout_no_wrap(n, FontId::monospace(10.5), p.faint);
        let nx = row_pill.right() - 8.0 - ng.size().x;
        ui.painter()
            .galley(pos2(nx, entry.center().y - ng.size().y / 2.0), ng, p.faint);
        let mut job = egui::text::LayoutJob::simple_singleline(
            title,
            FontId::new(
                12.5,
                if on || h.level == min_level {
                    theme::font::sans_500()
                } else {
                    egui::FontFamily::Proportional
                },
            ),
            ink,
        );
        job.wrap.max_width = (nx - entry.left() - 4.0 - 8.0).max(16.0);
        job.wrap.max_rows = 1;
        job.wrap.overflow_character = Some('…');
        let tg = ui.fonts_mut(|f| f.layout_job(job));
        ui.painter().galley(
            pos2(entry.left() + 4.0, entry.center().y - tg.size().y / 2.0),
            tg,
            ink,
        );
        let er = er
            .on_hover_text(format!("{}:{} · {} words", h.file, h.line, totals[i]))
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if er.clicked() {
            jump = Some((h.file.clone(), h.line as usize));
        }
    }
    if let Some(k) = fold
        && !ws.outline_folded.remove(&k)
    {
        ws.outline_folded.insert(k);
    }
    if let Some((file, line)) = jump {
        ws.goto(backend, &file, line);
    }
}

// ── Tabs ───────────────────────────────────────────────────────────────────

pub fn tabs(ui: &mut Ui, p: &Palette, ws: &mut Workspace, settings: &Settings, backend: &Backend) {
    let rect = ui.max_rect();
    ui.set_clip_rect(ui.clip_rect().intersect(rect));
    ui.painter().rect_filled(
        rect,
        CornerRadius {
            nw: theme::R_LG - 1,
            ne: theme::R_LG - 1,
            sw: 0,
            se: 0,
        },
        p.surface_2,
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, p.border),
    );
    let mut select = None;
    let mut close = None;
    let mut pin = None;
    let drag_id = egui::Id::unique("tab-drag");
    let dragging: Option<String> = ui.data(|d| d.get_temp(drag_id));
    let mut drop_at = None;
    let tabs = ws.tabs.clone();
    let mut tab_rects = Vec::new();
    let names: Vec<String> = tabs
        .iter()
        .map(|t| {
            let base = tree::base_name(t);
            if tabs.iter().filter(|o| tree::base_name(o) == base).count() > 1
                && !super::files::parent_of(t).is_empty()
            {
                format!("{}/{base}", tree::base_name(super::files::parent_of(t)))
            } else {
                base.to_owned()
            }
        })
        .collect();
    let naturals: Vec<f32> = tabs
        .iter()
        .zip(&names)
        .map(|(t, name)| {
            let g = ui
                .painter()
                .layout_no_wrap(name.clone(), FontId::proportional(12.5), p.muted);
            let dirty_w = if ws.buffers.is_dirty(t) { 10.0 } else { 0.0 };
            ((if ws.pinned.contains(t) { 10.0 } else { 14.0 })
                + if tree::is_viewer_file(t) { 18.0 } else { 0.0 }
                + g.size().x
                + dirty_w
                + 4.0
                + 24.0
                + 6.0)
                .min(220.0)
        })
        .collect();
    let total: f32 = naturals.iter().sum();
    let widths: Vec<f32> = if total > rect.width() {
        let k = rect.width() / total;
        naturals
            .iter()
            .map(|w| (w * k).max(72.0_f32.min(*w)))
            .collect()
    } else {
        naturals
    };
    let full: f32 = widths.iter().sum();
    let scroll_id = ui.make_persistent_id("tab-scroll");
    let (mut off, seen): (f32, Option<String>) =
        ui.data(|d| d.get_temp(scroll_id)).unwrap_or((0.0, None));
    if seen != ws.active
        && let Some(i) = tabs.iter().position(|t| Some(t) == ws.active.as_ref())
    {
        let (a0, a1) = (
            widths[..i].iter().sum::<f32>(),
            widths[..=i].iter().sum::<f32>(),
        );
        off = off.max(a1 - rect.width()).min(a0);
    }
    if full > rect.width() && ui.rect_contains_pointer(rect) {
        let d = ui.input(|i| i.smooth_scroll_delta);
        off -= d.x + d.y;
    }
    off = off.clamp(0.0, (full - rect.width()).max(0.0));
    ui.data_mut(|d| d.insert_temp(scroll_id, (off, ws.active.clone())));
    let mut x = rect.left() - off;
    for (idx, t) in tabs.iter().enumerate() {
        let name = names[idx].clone();
        let viewer = tree::is_viewer_file(t);
        let pinned = ws.pinned.contains(t);
        let dirty = ws.buffers.is_dirty(t);
        let w = widths[idx];
        let tr = Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height() - 1.0));
        tab_rects.push(tr);
        let on = ws.active.as_deref() == Some(t.as_str());
        let r = ui.interact(
            tr,
            ui.make_persistent_id(("tab", t)),
            Sense::click_and_drag(),
        );
        r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, on, &name));
        r.clone().on_hover_text(t.as_str());
        let painter = ui.painter();
        if on {
            let active_r = tr.with_max_y(rect.bottom());
            let nw = if idx == 0 && off < 1.0 {
                theme::R_LG - 1
            } else {
                0
            };
            painter.rect_filled(
                active_r,
                CornerRadius {
                    nw,
                    ne: 0,
                    sw: 0,
                    se: 0,
                },
                p.code_bg,
            );
        } else if r.hovered() {
            painter.rect_filled(tr, 0, p.surface_3.gamma_multiply(0.55));
        }
        if dragging.as_deref() == Some(t.as_str()) {
            painter.rect_filled(tr, 0, p.surface_2.gamma_multiply(0.5));
        }
        painter.vline(tr.right() - 0.5, tr.y_range(), Stroke::new(1.0, p.border));
        let ink = if on { p.ink } else { p.muted };
        let mut tx = tr.left() + if pinned { 10.0 } else { 14.0 };
        if viewer {
            icons::paint(
                painter,
                Rect::from_min_size(pos2(tx, tr.center().y - 6.5), Vec2::splat(13.0)),
                icons::FILE_IMAGE,
                ink,
            );
            tx += 18.0;
        }
        let dirty_space = if dirty { 10.0 } else { 0.0 };
        let mut job = egui::text::LayoutJob::simple_singleline(
            name.clone(),
            FontId::new(
                12.5,
                if on {
                    theme::font::sans_500()
                } else {
                    egui::FontFamily::Proportional
                },
            ),
            ink,
        );
        job.wrap.max_width = (tr.right() - 4.0 - 24.0 - dirty_space - 4.0 - tx).max(16.0);
        job.wrap.max_rows = 1;
        job.wrap.overflow_character = Some('…');
        let g = ui.fonts_mut(|f| f.layout_job(job));
        let gw = g.size().x;
        painter.galley(pos2(tx, tr.center().y - g.size().y / 2.0), g, ink);
        if dirty && !pinned {
            // Dirty warn dot shown beside filename before the close button.
            painter.circle_filled(
                pos2((tx + gw + 7.0).min(tr.right() - 32.0), tr.center().y),
                3.0,
                p.warn,
            );
        }
        let xr = Rect::from_min_size(
            pos2(tr.right() - 1.0 - 4.0 - 24.0, tr.center().y - 12.0),
            Vec2::splat(24.0),
        );
        let xresp = ui.interact(xr, ui.make_persistent_id(("tab-x", t)), Sense::click());
        let label = if pinned {
            format!("Unpin {t}")
        } else if dirty {
            format!("{t} has unsaved changes; close")
        } else {
            format!("Close {t}")
        };
        xresp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
        if xresp.hovered() {
            ui.painter()
                .rect_filled(xr, CornerRadius::same(theme::R_SM), p.surface_3);
        }
        let hover = r.hovered() || xresp.hovered();
        if pinned {
            icons::paint(
                ui.painter(),
                Rect::from_center_size(xr.center(), Vec2::splat(12.0)),
                icons::PIN,
                p.accent,
            );
            if dirty {
                ui.painter()
                    .circle_filled(xr.right_top() + vec2(-5.5, 5.5), 3.0, p.warn);
            }
        } else if hover || on {
            icons::paint(
                ui.painter(),
                Rect::from_center_size(xr.center(), Vec2::splat(13.0)),
                icons::X,
                if xresp.hovered() { p.ink } else { p.faint },
            );
        }
        if xresp.clicked() {
            if pinned {
                pin = Some(t.clone())
            } else {
                close = Some(t.clone())
            }
        } else if r.clicked() {
            select = Some(t.clone());
        }
        if r.middle_clicked() && !pinned {
            close = Some(t.clone());
        }
        if r.secondary_clicked() {
            let at = r.interact_pointer_pos().unwrap_or(tr.left_bottom());
            ws.menu = Some(Menu::Tab(t.clone(), Rect::from_min_size(at, Vec2::ZERO)));
        }
        if r.drag_started() {
            ui.data_mut(|d| d.insert_temp(drag_id, t.clone()));
        }
        x = tr.right();
    }
    // Drag to reorder: a 2 px accent marker where the tab will land.
    if let Some(dragged) = dragging {
        if let Some(pos) = ui.ctx().pointer_interact_pos() {
            let over = tab_rects
                .iter()
                .position(|r| pos.x < r.center().x)
                .unwrap_or(tab_rects.len());
            let mx = tab_rects.get(over).map_or(x, |r| r.left());
            ui.painter()
                .vline(mx, rect.y_range(), Stroke::new(2.0, p.accent));
            drop_at = Some((dragged.clone(), over));
        }
        if !ui.input(|i| i.pointer.any_down()) {
            ui.data_mut(|d| d.remove::<String>(drag_id));
            if let Some((path, mut to)) = drop_at
                && let Some(from) = ws.tabs.iter().position(|t| *t == path)
            {
                let t = ws.tabs.remove(from);
                if to > from {
                    to -= 1;
                }
                ws.tabs.insert(to.min(ws.tabs.len()), t);
                ws.pin_first();
            }
        }
    }
    if let Some(t) = select {
        ws.open_file(backend, &t);
    }
    if let Some(t) = close {
        ws.close_tab(backend, &t, settings);
    }
    if let Some(t) = pin {
        ws.toggle_pin(&t);
    }
}

// ── Formatting bar ─────────────────────────────────────────────────────────

/// What the edit bar asked for that needs more than the editor: "Go to PDF", "Comment".
pub fn edit_bar(ui: &mut Ui, p: &Palette, ws: &mut Workspace) -> (bool, bool, bool) {
    let mut sync = false;
    let mut cite = false;
    let mut comment = false;
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, p.surface);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, p.border),
    );
    let disabled = !ws.active.as_deref().is_some_and(tree::is_tex);
    let has_editor = ws
        .active
        .as_deref()
        .is_some_and(|a| ws.editors.contains_key(a));
    let avail_w = rect.width() - 20.0;
    // Compact mode uses icons for Cite/Comment/Show in PDF so nothing clips in Split layout;
    // overflow mode moves secondary insert/list buttons into a `…` menu at narrow widths (< 490 px).
    let compact_labels = avail_w < 680.0;
    let overflow = avail_w < 490.0;
    let mut bar = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(10.0, 4.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    bar.spacing_mut().item_spacing.x = 2.0;
    let tip = |label: &str, id: &str| format!("{label} ({})", display(keys_of(id)));
    let ibtn = |ui: &mut Ui, icon: Icon, label: &str| {
        Button::icon(icon, label)
            .size(Size::Sm)
            .enabled(!disabled)
            .show_icon(ui, p)
    };
    let sep = |ui: &mut Ui| {
        let pad = if overflow { 3.0 } else { 5.0 };
        ui.add_space(pad);
        let (r, _) = ui.allocate_exact_size(vec2(1.0, 18.0), Sense::hover());
        ui.painter().rect_filled(r, 0, p.border);
        ui.add_space(pad);
    };
    #[allow(clippy::type_complexity)]
    let mut todo: Option<Box<dyn FnOnce(&mut Workspace)>> = None;
    let c = &mut bar;
    // Style picker ("Paragraph ▾" or current heading level; AccessKit label "Heading").
    let style_label = ws
        .active
        .as_deref()
        .and_then(|a| ws.editors.get(a))
        .map(|ed| {
            let line = ed.doc.line(ed.doc.line_of(ed.doc.selection.main().head));
            let trimmed = line.trim_start();
            for (cmd, name) in [
                ("\\chapter", "Chapter"),
                ("\\section", "Section"),
                ("\\subsection", "Subsection"),
                ("\\subsubsection", "Subsubsection"),
                ("\\paragraph", "Paragraph"),
            ] {
                if let Some(rest) = trimmed.strip_prefix(cmd)
                    && rest.trim_start_matches('*').starts_with(['[', '{'])
                {
                    return name;
                }
            }
            "Paragraph"
        })
        .unwrap_or("Paragraph");
    let show_style_text = avail_w >= 410.0;
    let style_g = show_style_text.then(|| {
        c.painter().layout_no_wrap(
            style_label.into(),
            FontId::new(12.0, theme::font::sans_500()),
            p.ink,
        )
    });
    let hw = match &style_g {
        Some(g) => 9.0 + g.size().x + 4.0 + 12.0 + 8.0,
        None => 9.0 + 16.0 + 2.0 + 12.0 + 9.0,
    };
    let (hrect, h) = c.allocate_exact_size(
        vec2(hw, 26.0),
        if disabled {
            Sense::hover()
        } else {
            Sense::click()
        },
    );
    h.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, !disabled, "Heading"));
    if !disabled && (h.hovered() || matches!(ws.menu, Some(Menu::Heading(_)))) {
        c.painter()
            .rect_filled(hrect, CornerRadius::same(theme::R), p.surface_3);
    }
    let ink = if disabled {
        p.ink.gamma_multiply(0.5)
    } else {
        p.ink
    };
    if let Some(g) = style_g {
        let gw = g.size().x;
        c.painter().galley(
            pos2(hrect.left() + 9.0, hrect.center().y - g.size().y / 2.0),
            g,
            ink,
        );
        icons::paint(
            c.painter(),
            Rect::from_min_size(
                pos2(hrect.left() + 9.0 + gw + 4.0, hrect.center().y - 6.0),
                Vec2::splat(12.0),
            ),
            icons::CHEVRON_DOWN,
            if disabled { ink } else { p.muted },
        );
    } else {
        icons::paint(
            c.painter(),
            Rect::from_min_size(
                pos2(hrect.left() + 9.0, hrect.center().y - 8.0),
                Vec2::splat(16.0),
            ),
            icons::HEADING,
            ink,
        );
        icons::paint(
            c.painter(),
            Rect::from_min_size(
                pos2(hrect.left() + 27.0, hrect.center().y - 6.0),
                Vec2::splat(12.0),
            ),
            icons::CHEVRON_DOWN,
            ink,
        );
    }
    if h.on_hover_text("Heading / text style").clicked() {
        ws.menu = if matches!(ws.menu, Some(Menu::Heading(_))) {
            None
        } else {
            Some(Menu::Heading(hrect))
        };
    }
    sep(c);
    if ibtn(c, icons::BOLD, "Bold")
        .on_hover_text(tip("Bold", "bold"))
        .clicked()
    {
        todo = Some(Box::new(|ws| {
            with_editor(ws, |e, _| cmd::toggle_wrap(&mut e.doc, "textbf"))
        }));
    }
    if ibtn(c, icons::ITALIC, "Italic")
        .on_hover_text(tip("Italic", "italic"))
        .clicked()
    {
        todo = Some(Box::new(|ws| {
            with_editor(ws, |e, _| cmd::toggle_wrap(&mut e.doc, "textit"))
        }));
    }
    sep(c);
    if ibtn(c, icons::SIGMA, "Inline math ($…$)")
        .on_hover_text("Inline math ($…$)")
        .clicked()
    {
        todo = Some(Box::new(|ws| {
            with_editor(ws, |e, _| wrap_with(&mut e.doc, "$", "$"))
        }));
    }
    if ibtn(c, icons::PI, "Symbols")
        .on_hover_text(tip("Math & LaTeX symbols", "symbols"))
        .clicked()
    {
        todo = Some(Box::new(|ws| ws.symbols = Some(Default::default())));
    }
    if !overflow {
        if ibtn(c, icons::LINK, "Link")
            .on_hover_text("Insert link (\\href)")
            .clicked()
        {
            todo = Some(Box::new(move |ws| {
                with_editor(ws, |e, i| e.insert_template("\\href{${https://}}{${}}", i))
            }));
        }
        if ibtn(c, icons::IMAGE, "Figure")
            .on_hover_text("Insert figure")
            .clicked()
        {
            todo = Some(Box::new(|ws| ws.open_figure()));
        }
        if ibtn(c, icons::TABLE, "Table")
            .on_hover_text("Insert table")
            .clicked()
        {
            todo = Some(Box::new(|ws| ws.open_table()));
        }
        let code = ibtn(c, icons::CODE, "Code block");
        let at = code.rect;
        if code.on_hover_text("Code block").clicked() {
            todo = Some(Box::new(move |ws| {
                ws.menu = if matches!(ws.menu, Some(Menu::Code(_))) {
                    None
                } else {
                    Some(Menu::Code(at))
                };
            }));
        }
    }
    let cite_btn = if compact_labels {
        Button::icon(icons::BOOK_OPEN_TEXT, "Cite")
            .size(Size::Sm)
            .enabled(!disabled)
            .show_icon(c, p)
    } else {
        Button::new("Cite")
            .with_icon(icons::BOOK_OPEN_TEXT)
            .kind(Kind::Ghost)
            .size(Size::Sm)
            .enabled(!disabled)
            .show(c, p)
    };
    if cite_btn
        .on_hover_text(tip("Insert a citation", "cite"))
        .clicked()
    {
        cite = true;
    }
    if !overflow {
        sep(c);
        for (icon, label, env) in [
            (icons::LIST, "Bulleted list", "itemize"),
            (icons::LIST_ORDERED, "Numbered list", "enumerate"),
        ] {
            if ibtn(c, icon, label).on_hover_text(label).clicked() {
                let t = format!("\\begin{{{env}}}\n\t\\item ${{}}\n\\end{{{env}}}");
                todo = Some(Box::new(move |ws| {
                    with_editor(ws, |e, i| e.insert_template(&t, i))
                }));
            }
        }
    } else {
        let more = Button::icon(icons::ELLIPSIS, "More formatting")
            .size(Size::Sm)
            .enabled(!disabled)
            .on(matches!(ws.menu, Some(Menu::EditMore(_))))
            .show_icon(c, p);
        let at = more.rect;
        if more.on_hover_text("More formatting and inserts").clicked() {
            ws.menu = if matches!(ws.menu, Some(Menu::EditMore(_))) {
                None
            } else {
                Some(Menu::EditMore(at))
            };
        }
    }
    sep(c);
    let comment_btn = if compact_labels {
        Button::icon(icons::MESSAGE_SQUARE_PLUS, "Comment")
            .size(Size::Sm)
            .enabled(has_editor)
            .show_icon(c, p)
    } else {
        Button::new("Comment")
            .with_icon(icons::MESSAGE_SQUARE_PLUS)
            .kind(Kind::Ghost)
            .size(Size::Sm)
            .enabled(has_editor)
            .show(c, p)
    };
    if comment_btn
        .on_hover_text(tip("Add a comment on the selection", "add-comment"))
        .clicked()
    {
        comment = true;
    }
    let sync_btn = if compact_labels {
        Button::icon(icons::LOCATE, "Go to PDF")
            .size(Size::Sm)
            .enabled(!disabled)
            .show_icon(c, p)
    } else {
        let b = Button::new("Show in PDF")
            .with_icon(icons::LOCATE)
            .kind(Kind::Ghost)
            .size(Size::Sm)
            .enabled(!disabled)
            .show(c, p);
        c.ctx()
            .accesskit_node_builder(b.id, |n| n.set_label("Go to PDF"));
        b
    };
    if sync_btn.on_hover_text(tip("Show in PDF", "sync")).clicked() {
        sync = true;
    }
    if let Some(f) = todo {
        f(ws);
    }
    (sync, comment, cite)
}

/// Runs a command on the open file's editor with its indentation, and shows the result.
fn with_editor(ws: &mut Workspace, f: impl FnOnce(&mut view::Editor, Indent)) {
    let indent = ws.indent;
    if let Some(ed) = ws.active_editor() {
        f(ed, indent);
        view::after_command(ed);
    }
}

/// Surrounds each selection with `open` and `close`, or removes them when already there.
fn wrap_with(d: &mut crate::editor::buffer::Doc, open: &str, close: &str) {
    use crate::editor::buffer::{Change, EditKind, Range};
    let (no, nc) = (open.chars().count(), close.chars().count());
    let (c, s) = d.change_by_range(|d, r| {
        let before = d.slice(r.from().saturating_sub(no), r.from());
        let after = d.slice(r.to(), r.to() + nc);
        if before == open && after == close {
            return (
                vec![
                    Change::delete(r.from() - no, r.from()),
                    Change::delete(r.to(), r.to() + nc),
                ],
                Range::new(r.from() - no, r.to() - no),
            );
        }
        (
            vec![
                Change::insert(r.from(), open),
                Change::insert(r.to(), close),
            ],
            Range::new(r.from() + no, r.to() + no),
        )
    });
    d.apply(c, s, EditKind::Other);
}

// ── Status bar ─────────────────────────────────────────────────────────────

/// True when a setting changed.
pub fn status_bar(
    ui: &mut Ui,
    p: &Palette,
    ws: &mut Workspace,
    settings: &mut Settings,
    backend: &Backend,
) -> bool {
    let mut changed = false;
    let rect = ui.max_rect();
    ui.painter().rect_filled(
        rect,
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: theme::R_LG - 1,
            se: theme::R_LG - 1,
        },
        p.surface_2,
    );
    ui.painter()
        .hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p.border));
    let mut c = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(12.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let tight = rect.width() < 340.0;
    let medium = rect.width() < 430.0;
    c.spacing_mut().item_spacing.x = if tight { 10.0 } else { 14.0 };
    let small = |t: String, color: Color32| egui::RichText::new(t).size(11.5).color(color);
    let is_editor = ws
        .active
        .as_deref()
        .is_some_and(|a| ws.editors.contains_key(a));
    if is_editor {
        let line = if ws.cursor.0.is_empty() {
            1
        } else {
            ws.cursor.1
        };
        let col = ws.cursor_col.max(1);
        c.label(small(format!("Ln {line}, Col {col}"), p.muted));
        if ws.selection_words > 0 && !tight {
            c.label(small(format!("{} selected", ws.selection_words), p.muted))
                .on_hover_text("Words in the selection");
        }
        if let Some(a) = ws.active.clone() {
            let failed = ws.buffers.failed.contains(&a);
            let text = if failed {
                "Not saved"
            } else if ws.buffers.is_dirty(&a) {
                "Unsaved"
            } else {
                "Saved"
            };
            let rt = if failed {
                small(text.into(), p.error).font(FontId::new(11.5, theme::font::sans_600()))
            } else {
                small(text.into(), p.faint)
            };
            c.label(rt);
        }
    } else if let Some(a) = ws.active.as_deref() {
        c.label(small(tree::base_name(a).to_owned(), p.muted));
    }
    let errors = ws.build.errors();
    let warnings = ws.build.warnings();
    if !tight && (errors > 0 || warnings > 0) {
        let mut parts = Vec::new();
        if errors > 0 {
            parts.push(format!(
                "{errors} error{}",
                if errors == 1 { "" } else { "s" }
            ));
        }
        if warnings > 0 {
            parts.push(format!(
                "{warnings} warning{}",
                if warnings == 1 { "" } else { "s" }
            ));
        }
        let col = if errors > 0 { p.error } else { p.warn };
        if status_link(&mut c, parts.join(" · "), col, "Show problems", errors > 0).clicked()
            && ws.open_side(SidePanel::Problems, backend, settings)
        {
            changed = true;
        }
    } else if !tight
        && ws.build.failed()
        && status_link(
            &mut c,
            "Build failed".into(),
            p.error,
            "Show problems",
            true,
        )
        .clicked()
        && ws.open_side(SidePanel::Problems, backend, settings)
    {
        changed = true;
    }
    c.with_layout(egui::Layout::right_to_left(egui::Align::Center), |c| {
        c.spacing_mut().item_spacing.x = if tight { 10.0 } else { 14.0 };
        if !tight
            && status_link(
                c,
                engine_name(ws.info.config.engine).into(),
                p.muted,
                "Compiler: change it in Project settings",
                false,
            )
            .clicked()
        {
            ws.open_project_settings();
        }
        if !tight
            && ws.total_words > 0
            && status_link(
                c,
                format!("{} words", super::sheets::group_digits(ws.total_words)),
                p.muted,
                "Words in the document body, without commands, maths or comments. Click for the count per file.",
                false,
            )
            .clicked()
        {
            ws.open_word_count(backend);
        }
        let lang = ws.spell_language(settings);
        if !medium
            && lang != "off"
            && status_link(
                c,
                lang,
                p.muted,
                "Spell-check language: change it in Project settings",
                false,
            )
            .clicked()
        {
            ws.open_project_settings();
        }
        if !medium
            && !ws.trusted
            && widgets::chip_button(
                c,
                p,
                None,
                "Restricted",
                "Restricted project: click to manage trust",
                ChipTone::Warn,
            )
            .on_hover_text("Restricted project: external tools and git are disabled. Click to change in Project settings.")
            .clicked()
        {
            ws.open_project_trust_settings();
        }
        if !medium
            && ws.info.config.draft
            && status_link(
                c,
                "Draft mode".into(),
                p.warn,
                "Figures are drawn as boxes. Click to build normally again.",
                false,
            )
            .clicked()
        {
            ws.set_config(backend, settings, |c| c.draft = false);
        }
        if !medium
            && let Some(err) = ws.checks.grammar.error.clone()
            && status_link(
                c,
                "Grammar error".into(),
                p.warn,
                &format!("LanguageTool failed: {err}. Click to retry."),
                false,
            )
            .clicked()
        {
            ws.checks.grammar.error = None;
            ws.checks.grammar.asked.clear();
            ws.checks.grammar.generation += 1;
            ws.notice(format!("Retrying grammar check ({err})"), Tone::Info);
        }
        if !medium && ws.tex_outdated {
            c.label(small("Old TeX Live".into(), p.warn))
                .on_hover_text("Some packages may lack newer features");
        }
        if !medium {
            match &ws.lsp.state {
                crate::lsp::State::Starting => {
                    let r = c
                        .label(small("Starting completion".into(), p.muted))
                        .on_hover_text("The language server is starting");
                    let ring = Rect::from_center_size(
                        pos2(r.rect.left() - 10.0, r.rect.center().y),
                        vec2(8.0, 8.0),
                    );
                    let t = c.input(|i| i.time) as f32;
                    c.painter().add(egui::Shape::Path(egui::epaint::PathShape::line(
                        (0..=12)
                            .map(|k| {
                                let a = t * std::f32::consts::TAU / 0.8 + k as f32 * 0.4;
                                ring.center() + vec2(a.cos(), a.sin()) * 3.5
                            })
                            .collect(),
                        Stroke::new(1.5, p.muted),
                    )));
                    c.add_space(8.0);
                    c.ctx().request_repaint_after(theme::ANIMATION_FRAME);
                }
                crate::lsp::State::Failed(_)
                    if status_link(
                        c,
                        "No completion".into(),
                        p.muted,
                        "Why is completion off?",
                        false,
                    )
                    .clicked() =>
                {
                    ws.notice("Completion, hover and go to definition come from texlab, which Lemmata downloads the first time it's needed but couldn't start. Check the internet connection, or install texlab yourself (your package manager, or cargo install texlab), then reopen the project.", Tone::Info);
                }
                _ => {}
            }
        }
    });
    changed
}

struct StatusLink(egui::Response, bool);

impl StatusLink {
    fn clicked(&self) -> bool {
        self.0.clicked() || self.1
    }
}

/// `.status-link`: plain text until hovered or focused, then underlined (`.bad` ones always are).
fn status_link(ui: &mut Ui, text: String, color: Color32, tip: &str, bad: bool) -> StatusLink {
    let r = ui.add(
        egui::Label::new(egui::RichText::new(text).size(12.0).color(color)).sense(Sense::click()),
    );
    let keyboard = r.has_focus()
        && ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space));
    if bad || r.hovered() || r.has_focus() {
        ui.painter().hline(
            r.rect.x_range(),
            r.rect.bottom() - 1.0,
            Stroke::new(1.0, color),
        );
    }
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    StatusLink(
        r.on_hover_text(tip)
            .on_hover_cursor(egui::CursorIcon::PointingHand),
        keyboard,
    )
}

// ── Menus ──────────────────────────────────────────────────────────────────

pub fn menus(
    ctx: &egui::Context,
    p: &Palette,
    ws: &mut Workspace,
    settings: &mut Settings,
    backend: &Backend,
    action: &mut Option<Action>,
) {
    let Some(menu) = ws.menu.clone() else { return };
    let id = egui::Id::unique(("workspace-menu", std::mem::discriminant(&menu)));
    match menu {
        Menu::Main(anchor) => {
            use super::palette::label;
            // The palette's commands, by id; "" is a separator, "#…" a heading.
            const MENU: [&str; 29] = [
                "#Project",
                "close",
                "project-settings",
                "save-template",
                "words",
                "",
                "#Tools",
                "git",
                "history",
                "snapshot",
                "comments",
                "todos",
                "bib-manager",
                "format",
                "reindent",
                "focus",
                "",
                "#Export",
                "export-pdf",
                "export-zip",
                "export-arxiv",
                "export-arxiv-flat",
                "",
                "#Help",
                "preferences",
                "shortcuts",
                "guide",
                "report",
                "palette",
            ];
            let hints: Vec<String> = MENU
                .iter()
                .map(|id| match *id {
                    "snapshot" | "focus" | "shortcuts" | "palette" | "reindent" => {
                        display(keys_of(id))
                    }
                    _ => String::new(),
                })
                .collect();
            let items: Vec<MenuItem> = MENU
                .iter()
                .zip(&hints)
                .map(|(id, hint)| match *id {
                    "" => MenuItem::Separator,
                    h if h.starts_with('#') => MenuItem::Heading(&h[1..]),
                    "palette" => MenuItem::plain("Command palette").hint(hint),
                    id if hint.is_empty() => MenuItem::plain(label(id, ws.focus_mode)),
                    id => MenuItem::plain(label(id, ws.focus_mode)).hint(hint),
                })
                .collect();
            match widgets::drop_menu(ctx, p, id, anchor, false, &items) {
                MenuResult::Open => {}
                MenuResult::Closed => ws.menu = None,
                MenuResult::Clicked(i) => {
                    ws.menu = None;
                    match MENU[i] {
                        "palette" => ws.open_palette(super::palette::Kind::Commands, settings),
                        id => {
                            if let Some(a) = ws.run_command(ctx, id, backend, settings) {
                                *action = Some(a);
                            }
                        }
                    }
                }
            }
        }
        Menu::Compile(anchor) => {
            let cfg = ws.info.config.clone();
            let mut items = vec![
                MenuItem::Heading("Compile"),
                MenuItem::check("As I type", settings.auto_compile),
                MenuItem::check("With Ctrl+S", !settings.auto_compile),
                MenuItem::Separator,
                MenuItem::Heading("Compiler"),
            ];
            for (e, label) in ENGINES {
                if e == Engine::Tectonic && !ws.tectonic_installed && cfg.engine != Engine::Tectonic
                {
                    items.push(
                        MenuItem::check(label, false)
                            .hint("not installed")
                            .enabled(false),
                    );
                } else {
                    items.push(MenuItem::check(label, cfg.engine == e));
                }
            }
            items.extend([
                MenuItem::Separator,
                MenuItem::Heading("Mode"),
                MenuItem::check("Normal", !cfg.draft),
                MenuItem::check("Fast (draft)", cfg.draft),
                MenuItem::Separator,
                MenuItem::Heading("When there's an error"),
                MenuItem::check("Keep going", !cfg.stop_on_error),
                MenuItem::check("Stop at the first error", cfg.stop_on_error),
                MenuItem::Separator,
                MenuItem::plain("Clear cached files"),
            ]);
            if ws.build.building {
                items.push(MenuItem::plain("Stop compiling"));
            }
            match widgets::drop_menu(ctx, p, id, anchor, true, &items) {
                MenuResult::Open => {}
                MenuResult::Closed => ws.menu = None,
                // By label: the positions shift with the engine list and the Stop item.
                MenuResult::Clicked(i) => {
                    if let MenuItem::Item { label, .. } = items[i] {
                        match label {
                            "As I type" | "With Ctrl+S" => {
                                settings.auto_compile = label == "As I type";
                                *action = Some(Action::Settings);
                            }
                            "Normal" | "Fast (draft)" => ws.set_config(backend, settings, |c| {
                                c.draft = label == "Fast (draft)"
                            }),
                            "Keep going" | "Stop at the first error" => {
                                ws.set_config(backend, settings, |c| {
                                    c.stop_on_error = label == "Stop at the first error"
                                })
                            }
                            "Clear cached files" => {
                                ws.menu = None;
                                ws.clean(backend);
                            }
                            "Stop compiling" => {
                                ws.menu = None;
                                ws.stop_build(backend);
                            }
                            _ => {
                                if let Some((e, _)) = ENGINES.iter().find(|(_, l)| *l == label) {
                                    let e = *e;
                                    ws.set_config(backend, settings, |c| c.engine = e);
                                }
                            }
                        }
                    }
                }
            }
        }
        Menu::Targets(anchor) => {
            let targets = ws.info.config.targets.clone();
            let hints: Vec<String> = targets
                .iter()
                .map(|t| match t.engine {
                    Some(e) => format!("{} · {}", t.main, engine_name(e)),
                    None => t.main.clone(),
                })
                .collect();
            let mut items: Vec<MenuItem> = vec![MenuItem::Heading("Build targets")];
            for (t, h) in targets.iter().zip(&hints) {
                let on =
                    t.main == ws.info.main && t.engine.is_none_or(|e| e == ws.info.config.engine);
                items.push(MenuItem::check(&t.name, on).hint(h));
            }
            match widgets::drop_menu(ctx, p, id, anchor, false, &items) {
                MenuResult::Open => {}
                MenuResult::Closed => ws.menu = None,
                MenuResult::Clicked(i) => {
                    ws.menu = None;
                    if let Some(target) = targets.get(i.saturating_sub(1)).cloned() {
                        ws.select_target(backend, settings, target);
                    }
                }
            }
        }
        Menu::Notices(anchor) => {
            let screen = ctx.content_rect();
            let width = 360.0f32.min(screen.width() - 24.0);
            let max_h = 340.0f32.min(screen.height() - 80.0);
            let pos = pos2(
                (anchor.right() + 8.0).clamp(screen.left() + 8.0, screen.right() - width - 8.0),
                anchor
                    .bottom()
                    .clamp(screen.top() + 80.0, screen.bottom() - 8.0),
            );
            let seen_id = id.with("notices-seen");
            let frame = ctx.cumulative_frame_nr();
            let first = ctx
                .data(|d| d.get_temp::<u64>(seen_id))
                .is_none_or(|f| frame > f + 1);
            ctx.data_mut(|d| d.insert_temp(seen_id, frame));
            let mut close = !ws.modal_open()
                && !ws.app_modal_open
                && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            let mut clear = false;
            let area = egui::Area::new(egui::Id::unique("notices-drawer"))
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::LEFT_BOTTOM)
                .fixed_pos(pos)
                .show(ctx, |ui| {
                    ui.set_width(width);
                    egui::Frame::new()
                        .fill(p.surface)
                        .stroke(Stroke::new(1.0, p.border))
                        .corner_radius(theme::R_LG)
                        .shadow(p.shadow_lg)
                        .inner_margin(egui::Margin::same(10))
                        .show(ui, |ui| {
                            ui.set_width(width - 20.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Notifications")
                                        .font(FontId::new(13.0, theme::font::sans_600()))
                                        .color(p.ink),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if !ws.notice_log.is_empty()
                                            && Button::new("Clear")
                                                .kind(Kind::Ghost)
                                                .size(Size::Sm)
                                                .show(ui, p)
                                                .clicked()
                                        {
                                            clear = true;
                                        }
                                    },
                                );
                            });
                            ui.add_space(6.0);
                            if ws.notice_log.is_empty() {
                                ui.label(
                                    egui::RichText::new("No notifications yet.")
                                        .size(12.5)
                                        .color(p.muted),
                                );
                            } else {
                                egui::ScrollArea::vertical()
                                    .id_salt("notice-history")
                                    .max_height(max_h - 48.0)
                                    .auto_shrink([false, true])
                                    .show(ui, |ui| {
                                        ui.spacing_mut().item_spacing.y = 6.0;
                                        for entry in &ws.notice_log {
                                            let (icon, color) = match entry.tone {
                                                Tone::Ok => (icons::CIRCLE_CHECK, p.ok),
                                                Tone::Error => (icons::CIRCLE_ALERT, p.error),
                                                Tone::Info => (icons::INFO, p.accent),
                                            };
                                            egui::Frame::new()
                                                .fill(p.surface_2)
                                                .corner_radius(theme::R)
                                                .inner_margin(egui::Margin::symmetric(8, 6))
                                                .show(ui, |ui| {
                                                    ui.set_width(ui.available_width());
                                                    ui.horizontal_top(|ui| {
                                                        ui.spacing_mut().item_spacing.x = 8.0;
                                                        icons::show(ui, icon, 14.0, color);
                                                        ui.vertical(|ui| {
                                                            ui.spacing_mut().item_spacing.y = 2.0;
                                                            ui.add(
                                                                egui::Label::new(
                                                                    egui::RichText::new(
                                                                        &entry.text,
                                                                    )
                                                                    .size(12.5)
                                                                    .color(p.ink),
                                                                )
                                                                .wrap(),
                                                            );
                                                            ui.label(
                                                                egui::RichText::new(
                                                                    crate::when::time_ms(
                                                                        entry.time_ms,
                                                                    ),
                                                                )
                                                                .size(11.0)
                                                                .color(p.muted),
                                                            );
                                                        });
                                                    });
                                                });
                                        }
                                    });
                            }
                        });
                });
            if clear {
                ws.notice_log.clear();
                ws.unread_notices = 0;
            }
            if !first
                && ctx.input(|i| i.pointer.any_pressed())
                && ctx
                    .pointer_interact_pos()
                    .is_some_and(|pt| !area.response.rect.contains(pt) && !anchor.contains(pt))
            {
                close = true;
            }
            if close {
                ctx.data_mut(|d| d.remove::<u64>(seen_id));
                ws.menu = None;
            }
        }
        Menu::EditMore(anchor) => {
            let items = [
                MenuItem::new(icons::LINK, "Link (\\href)"),
                MenuItem::new(icons::IMAGE, "Insert figure…"),
                MenuItem::new(icons::TABLE, "Insert table…"),
                MenuItem::new(icons::CODE, "Code block…"),
                MenuItem::Separator,
                MenuItem::new(icons::LIST, "Bulleted list"),
                MenuItem::new(icons::LIST_ORDERED, "Numbered list"),
            ];
            match widgets::drop_menu(ctx, p, id, anchor, false, &items) {
                MenuResult::Open => {}
                MenuResult::Closed => ws.menu = None,
                MenuResult::Clicked(i) => {
                    ws.menu = None;
                    match i {
                        0 => with_editor(ws, |e, ind| {
                            e.insert_template("\\href{${https://}}{${}}", ind)
                        }),
                        1 => ws.open_figure(),
                        2 => ws.open_table(),
                        3 => ws.menu = Some(Menu::Code(anchor)),
                        5 => with_editor(ws, |e, ind| {
                            e.insert_template("\\begin{itemize}\n\t\\item ${}\n\\end{itemize}", ind)
                        }),
                        6 => with_editor(ws, |e, ind| {
                            e.insert_template(
                                "\\begin{enumerate}\n\t\\item ${}\n\\end{enumerate}",
                                ind,
                            )
                        }),
                        _ => {}
                    }
                }
            }
        }
        Menu::Sort(anchor) => {
            let sorts = [
                ("name", "Name"),
                ("type", "Type"),
                ("modified", "Last modified"),
                ("size", "Size"),
            ];
            let mut items = vec![MenuItem::Heading("Sort by")];
            for (k, l) in sorts {
                items.push(MenuItem::check(l, settings.tree_sort == k));
            }
            items.extend([
                MenuItem::Separator,
                MenuItem::check("Folders first", settings.tree_folders_first),
                MenuItem::Separator,
                MenuItem::plain("Collapse all folders"),
                MenuItem::plain("Expand all folders"),
            ]);
            match widgets::drop_menu(ctx, p, id, anchor, true, &items) {
                MenuResult::Open => {}
                MenuResult::Closed => ws.menu = None,
                MenuResult::Clicked(i) => match i {
                    1..=4 => {
                        settings.tree_sort = sorts[i - 1].0.into();
                        *action = Some(Action::Settings);
                    }
                    6 => {
                        settings.tree_folders_first = !settings.tree_folders_first;
                        *action = Some(Action::Settings);
                    }
                    8 => {
                        fn dirs(
                            list: &[lemmata_project::Entry],
                            out: &mut std::collections::HashSet<String>,
                        ) {
                            for e in list.iter().filter(|e| e.is_dir) {
                                out.insert(e.path.clone());
                                dirs(&e.children, out);
                            }
                        }
                        dirs(&ws.info.tree, &mut ws.collapsed);
                        ws.menu = None;
                    }
                    9 => {
                        ws.collapsed.clear();
                        ws.menu = None;
                    }
                    _ => {}
                },
            }
        }
        Menu::Heading(anchor) => {
            let hs = [
                ("section", "Section"),
                ("subsection", "Subsection"),
                ("subsubsection", "Subsubsection"),
                ("paragraph", "Paragraph"),
                ("chapter", "Chapter"),
            ];
            let items: Vec<MenuItem> = hs.iter().map(|(_, l)| MenuItem::plain(l)).collect();
            match widgets::drop_menu(ctx, p, id, anchor, false, &items) {
                MenuResult::Open => {}
                MenuResult::Closed => ws.menu = None,
                MenuResult::Clicked(i) => {
                    ws.menu = None;
                    let name = hs[i].0;
                    with_editor(ws, |e, ind| heading(e, name, ind));
                }
            }
        }
        Menu::Code(anchor) => {
            let langs = crate::editor::complete::CODE_LANGUAGES;
            let mut items = vec![MenuItem::Heading("Code block")];
            items.extend(langs.iter().map(|(l, _, _)| MenuItem::plain(l)));
            match widgets::drop_menu(ctx, p, id, anchor, false, &items) {
                MenuResult::Open => {}
                MenuResult::Closed => ws.menu = None,
                MenuResult::Clicked(i) => {
                    ws.menu = None;
                    let lang = langs[i - 1].0;
                    // minted when the document already loads it; listings needs no shell escape.
                    Workspace::job(
                        backend,
                        |shell| shell.has_package("minted".into()).unwrap_or(false),
                        move |ws, b, minted| {
                            let (t, pkg) = crate::editor::complete::code_block(lang, minted);
                            with_editor(ws, |e, i| e.insert_template(&t, i));
                            ws.needs_package(b, pkg.into());
                        },
                    );
                }
            }
        }
        Menu::Tree(entry, at) => ws.tree_menu(ctx, p, backend, settings, entry, at),
        Menu::Tab(path, anchor) => {
            let pinned = ws.pinned.contains(&path);
            let i = ws.tabs.iter().position(|t| *t == path).unwrap_or(0);
            let has_unpinned_others = ws.tabs.iter().any(|t| *t != path && !ws.pinned.contains(t));
            let has_unpinned_right = ws
                .tabs
                .get(i + 1..)
                .is_some_and(|s| s.iter().any(|t| !ws.pinned.contains(t)));
            let has_unpinned_any = ws.tabs.iter().any(|t| !ws.pinned.contains(t));
            let reopen = ws
                .closed
                .first()
                .map(|c| format!("Reopen {}", tree::base_name(c)));
            let mut items = vec![
                MenuItem::plain(if pinned { "Unpin" } else { "Pin" }),
                MenuItem::Separator,
                MenuItem::plain("Close"),
                MenuItem::plain("Close others").enabled(has_unpinned_others),
                MenuItem::plain("Close to the right").enabled(has_unpinned_right),
                MenuItem::plain("Close all").enabled(has_unpinned_any),
            ];
            if let Some(r) = &reopen {
                items.push(MenuItem::Separator);
                items.push(MenuItem::plain(r));
            }
            match widgets::drop_menu(ctx, p, id, anchor, false, &items) {
                MenuResult::Open => {}
                MenuResult::Closed => ws.menu = None,
                MenuResult::Clicked(k) => {
                    ws.menu = None;
                    // Closing "the others" or "all" leaves pinned tabs alone.
                    let unpinned = |ws: &Workspace, list: Vec<String>| {
                        list.into_iter()
                            .filter(|t| !ws.pinned.contains(t))
                            .collect::<Vec<_>>()
                    };
                    let victims = match k {
                        0 => {
                            ws.toggle_pin(&path);
                            vec![]
                        }
                        2 => vec![path.clone()],
                        3 => unpinned(
                            ws,
                            ws.tabs.iter().filter(|t| **t != path).cloned().collect(),
                        ),
                        4 => unpinned(ws, ws.tabs[i + 1..].to_vec()),
                        5 => unpinned(ws, ws.tabs.clone()),
                        7 => {
                            ws.reopen_closed(backend);
                            vec![]
                        }
                        _ => vec![],
                    };
                    for t in victims {
                        ws.close_tab(backend, &t, settings);
                    }
                }
            }
        }
    }
}

/// A heading command on the cursor's line (changing the one there), or a new one (Editor.svelte heading).
fn heading(e: &mut view::Editor, name: &str, indent: Indent) {
    let d = &mut e.doc;
    let n = d.line_of(d.selection.main().head);
    let text = d.line(n);
    let trimmed = text.trim_start();
    let ws = text.chars().count() - trimmed.chars().count();
    for h in [
        "part",
        "chapter",
        "section",
        "subsection",
        "subsubsection",
        "paragraph",
    ] {
        if let Some(rest) = trimmed.strip_prefix(&format!("\\{h}"))
            && rest.trim_start_matches('*').starts_with(['[', '{'])
        {
            use crate::editor::buffer::{Change, ChangeSet, EditKind};
            let from = d.line_start(n) + ws + 1;
            let sel = d.selection.clone();
            d.apply(
                ChangeSet::new(vec![Change {
                    from,
                    to: from + h.len(),
                    insert: name.into(),
                }]),
                sel,
                EditKind::Other,
            );
            return;
        }
    }
    if d.selection.main().is_empty() {
        e.insert_template(&format!("\\{name}{{${{}}}}"), indent);
    } else {
        cmd::toggle_wrap(d, name);
    }
}
