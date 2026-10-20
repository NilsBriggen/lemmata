//! The editor screen's frame: Toolbar.svelte, Sidebar.svelte with Tree.svelte and Outline.svelte,
//! TabStrip.svelte, EditorToolbar.svelte, the status bar of App.svelte, and their menus.

use super::{Action, Menu, Tone, Workspace, tree};
use crate::backend::{Backend, Msg, Save};
use crate::editor::commands::{self as cmd, Indent};
use crate::editor::view;
use crate::icons::{self, Icon};
use crate::shortcuts::{display, keys_of};
use crate::store::Settings;
use crate::theme::{self, Palette};
use crate::widgets::{self, Button, Kind, MenuItem, MenuResult, Size};
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

// ── Toolbar ────────────────────────────────────────────────────────────────

pub fn toolbar(
    ui: &mut Ui,
    p: &Palette,
    ws: &mut Workspace,
    settings: &mut Settings,
    backend: &Backend,
) -> Option<Action> {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, p.chrome);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, p.chrome_border),
    );
    let mut action = None;
    let inner = rect.shrink2(vec2(10.0, 0.0));

    // Centre: the sidebar toggle and the layout switch.
    let narrow = rect.width() < 1100.0;
    let layout_options = [
        (Some(icons::FILE_CODE), "Editor"),
        (Some(icons::COLUMNS_2), "Both"),
        (Some(icons::FILE_TEXT), "PDF"),
    ];
    let center_w = 30.0 + 6.0 + widgets::segmented_width(ui, &layout_options, narrow);
    let center =
        Rect::from_center_size(pos2(rect.center().x, rect.center().y), vec2(center_w, 30.0));
    let mut c = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(center)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    c.spacing_mut().item_spacing.x = 6.0;
    if Button::icon(icons::PANEL_LEFT, "Show files and outline")
        .kind(Kind::Chrome)
        .on(settings.sidebar_open)
        .icon_size(17.0)
        .show_icon(&mut c, p)
        .clicked()
    {
        settings.sidebar_open = !settings.sidebar_open;
        action = Some(Action::Settings);
    }
    let layouts = ["editor", "split", "pdf"];
    let current = layouts
        .iter()
        .position(|l| *l == settings.layout)
        .unwrap_or(1);
    if let Some(i) = widgets::segmented(&mut c, p, &layout_options, current, true, narrow) {
        settings.layout = layouts[i].into();
        action = Some(Action::Settings);
    }

    // Left: menu, brand, project name.
    let left = Rect::from_min_max(inner.min, pos2(center.left() - 12.0, inner.bottom()));
    let mut l = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(left)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    l.spacing_mut().item_spacing.x = 6.0;
    let menu = Button::icon(icons::MENU, "Menu")
        .kind(Kind::Chrome)
        .icon_size(18.0)
        .show_icon(&mut l, p);
    if menu.clicked() {
        ws.menu = if matches!(ws.menu, Some(Menu::Main(_))) {
            None
        } else {
            Some(Menu::Main(menu.rect))
        };
    }
    let brand = l.add(
        egui::Label::new(
            egui::RichText::new("Lemmata")
                .font(FontId::new(17.0, theme::font::display()))
                .color(p.chrome_ink),
        )
        .sense(Sense::click()),
    );
    brand.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, true, "Back to all projects")
    });
    if brand.on_hover_text("Back to all projects").clicked() {
        action = Some(Action::Library);
    }
    l.label(egui::RichText::new("/").color(p.chrome_muted));
    l.add(
        egui::Label::new(
            egui::RichText::new(&ws.info.name)
                .font(FontId::new(13.0, theme::font::sans_500()))
                .color(p.chrome_ink),
        )
        .truncate(),
    );

    // Right, from the right edge: the compile button, settings, Git, History, Comments.
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
            ne: theme::R,
            se: theme::R,
        })
        .icon_size(15.0)
        .show_icon(&mut r, p);
    // The 1 px light line between the halves.
    r.painter().vline(
        caret.rect.left(),
        caret.rect.y_range(),
        Stroke::new(1.0, Color32::from_white_alpha(64)),
    );
    if caret.clicked() {
        ws.menu = if matches!(ws.menu, Some(Menu::Compile(_))) {
            None
        } else {
            Some(Menu::Compile(caret.rect))
        };
    }
    // While building, the button stops the build, so it says so; its icon spins.
    let building = ws.build.building;
    let angle = if building {
        r.ctx().request_repaint();
        (r.input(|i| i.time) as f32 * std::f32::consts::TAU).rem_euclid(std::f32::consts::TAU)
    } else {
        0.0
    };
    let recompile = Button::new(if building { "Stop" } else { "Recompile" })
        .with_icon(icons::REFRESH_CW)
        .icon_angle(angle)
        .icon_size(15.0)
        .kind(Kind::Primary)
        .min_width(128.0)
        .radius(CornerRadius {
            nw: theme::R,
            sw: theme::R,
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
    r.add_space(4.0 + 6.0);
    r.spacing_mut().item_spacing.x = 6.0;
    let _ = Button::icon(icons::SETTINGS, "Project settings")
        .kind(Kind::Chrome)
        .icon_size(17.0)
        .show_icon(&mut r, p);
    for (icon, label) in [
        (icons::GIT_BRANCH, "Git"),
        (icons::HISTORY, "History"),
        (icons::MESSAGE_SQUARE, "Comments"),
    ] {
        // shortcut: the Git, History and Comments panels land in M6.
        let _ = Button::new(label)
            .with_icon(icon)
            .kind(Kind::Chrome)
            .show(&mut r, p);
    }
    action
}

// ── Sidebar ────────────────────────────────────────────────────────────────

/// `.title`: 11 px, 600, uppercase, +0.08em, `--chrome-muted`.
fn section_title(ui: &Ui, at: egui::Pos2, text: &str, p: &Palette) {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &text.to_uppercase(),
        0.0,
        egui::TextFormat {
            font_id: FontId::new(11.0, theme::font::sans_600()),
            color: p.chrome_muted,
            extra_letter_spacing: 0.88,
            ..Default::default()
        },
    );
    let g = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter()
        .galley(pos2(at.x, at.y - g.size().y / 2.0), g, p.chrome_muted);
}

pub fn sidebar(
    ui: &mut Ui,
    p: &Palette,
    ws: &mut Workspace,
    settings: &mut Settings,
    backend: &Backend,
) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, p.chrome_2);
    ui.painter().vline(
        rect.right() - 0.5,
        rect.y_range(),
        Stroke::new(1.0, p.chrome_border),
    );
    // Files take 60 %, the outline 40 % (or just its header when closed).
    let outline_h = if ws.outline_open {
        (rect.height() * 0.4).max(36.0)
    } else {
        36.0
    };
    let files = Rect::from_min_max(
        rect.min,
        pos2(rect.right() - 1.0, rect.bottom() - outline_h),
    );
    let outline = Rect::from_min_max(
        pos2(rect.left(), files.bottom()),
        pos2(rect.right() - 1.0, rect.bottom()),
    );

    // Files header.
    let head = Rect::from_min_size(files.min, vec2(files.width(), 36.0));
    section_title(ui, pos2(head.left() + 14.0, head.center().y), "Files", p);
    let mut acts = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(head.shrink2(vec2(6.0, 0.0)))
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
    );
    acts.spacing_mut().item_spacing.x = 1.0;
    let mut sort_rect = None;
    for (icon, label) in [
        (icons::SEARCH, "Search in project"),
        (icons::ARROW_UP_DOWN, "Sort and view"),
        (icons::UPLOAD, "Add files"),
        (icons::FOLDER_PLUS, "New folder"),
        (icons::FILE_PLUS, "New file"),
    ] {
        let b = Button::icon(icon, label)
            .kind(Kind::Chrome)
            .size(Size::Sm)
            .icon_size(15.0)
            .ink(p.chrome_muted)
            .show_icon(&mut acts, p);
        if label == "Sort and view" && b.clicked() {
            sort_rect = Some(b.rect);
        }
        // shortcut: new file/folder, adding files and project search land with the file actions (M6).
    }
    if let Some(r) = sort_rect {
        ws.menu = if matches!(ws.menu, Some(Menu::Sort(_))) {
            None
        } else {
            Some(Menu::Sort(r))
        };
    }

    // The tree.
    let body = Rect::from_min_max(pos2(files.left(), head.bottom()), files.max);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
    egui::ScrollArea::vertical()
        .id_salt("tree")
        .auto_shrink(false)
        .show(&mut child, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            tree_rows(ui, p, ws, settings, backend);
        });

    // Outline.
    ui.painter().hline(
        outline.x_range(),
        outline.top() + 0.5,
        Stroke::new(1.0, p.chrome_border),
    );
    let ohead = Rect::from_min_size(outline.min + vec2(0.0, 1.0), vec2(outline.width(), 35.0));
    let r = ui.interact(ohead, egui::Id::unique("outline-toggle"), Sense::click());
    r.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, true, ws.outline_open, "Outline")
    });
    if r.hovered() {
        ui.painter().rect_filled(ohead, 0, p.chrome_hover);
    }
    section_title(
        ui,
        pos2(ohead.left() + 14.0, ohead.center().y),
        "Outline",
        p,
    );
    let chev = Rect::from_center_size(
        pos2(ohead.right() - 6.0 - 8.0 - 7.0, ohead.center().y),
        Vec2::splat(14.0),
    );
    icons::paint_rotated(
        ui.painter(),
        chev,
        icons::CHEVRON_DOWN,
        p.chrome_muted,
        if ws.outline_open {
            0.0
        } else {
            -std::f32::consts::FRAC_PI_2
        },
    );
    if r.clicked() {
        ws.outline_open = !ws.outline_open;
    }
    if ws.outline_open {
        let body = Rect::from_min_max(pos2(outline.left(), ohead.bottom()), outline.max);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        egui::ScrollArea::vertical()
            .id_salt("outline")
            .auto_shrink(false)
            .show(&mut child, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                outline_rows(ui, p, ws, backend);
            });
    }
}

fn kind_icon(k: tree::Kind, p: &Palette) -> (Icon, Color32) {
    match k {
        tree::Kind::Tex => (icons::FILE_TEXT, p.accent_on_dark),
        tree::Kind::Bib => (icons::BOOK_OPEN, Color32::from_rgb(0xc7, 0xa2, 0xe0)),
        tree::Kind::Image => (icons::FILE_IMAGE, Color32::from_rgb(0x8f, 0xcf, 0x9f)),
        tree::Kind::Code => (icons::BRACES, p.chrome_muted),
        tree::Kind::Config => (icons::SETTINGS, p.chrome_muted),
        tree::Kind::File => (icons::FILE, p.chrome_muted),
    }
}

fn tree_rows(ui: &mut Ui, p: &Palette, ws: &mut Workspace, settings: &Settings, backend: &Backend) {
    let sorted = tree::sort_entries(
        &ws.info.tree,
        &settings.tree_sort,
        settings.tree_folders_first,
    );
    let rows: Vec<(lemmata_project::Entry, usize)> = tree::flatten(&sorted, &ws.collapsed)
        .into_iter()
        .map(|(e, d)| (e.clone(), d))
        .collect();
    let w = ui.available_width();
    let mut open = None;
    let mut toggle = None;
    for (e, depth) in &rows {
        let (rect, r) = ui.allocate_exact_size(vec2(w, 28.0), Sense::click());
        let active = ws.active.as_deref() == Some(e.path.as_str());
        let cursor = ws.tree_cursor.as_deref() == Some(e.path.as_str());
        r.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::TreeItem, true, active || cursor, &e.name)
        });
        let painter = ui.painter();
        if active {
            painter.rect_filled(rect, 0, p.chrome_active);
            painter.rect_filled(
                Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
                0,
                p.accent_on_dark,
            );
        } else if r.hovered() {
            painter.rect_filled(rect, 0, p.chrome_hover);
        }
        let opens = tree::is_text_file(&e.path) || tree::is_viewer_file(&e.path);
        let ink = if active {
            Color32::WHITE
        } else if !e.is_dir && !opens {
            p.chrome_muted
        } else {
            p.chrome_ink
        };
        let mut x = rect.left() + 8.0 + *depth as f32 * 14.0;
        if e.is_dir {
            let open_dir = !ws.collapsed.contains(&e.path);
            icons::paint_rotated(
                painter,
                Rect::from_min_size(pos2(x, rect.center().y - 6.5), Vec2::splat(13.0)),
                icons::CHEVRON_RIGHT,
                p.chrome_muted,
                if open_dir {
                    std::f32::consts::FRAC_PI_2
                } else {
                    0.0
                },
            );
            x += 13.0 + 6.0;
            icons::paint(
                painter,
                Rect::from_min_size(pos2(x, rect.center().y - 7.5), Vec2::splat(15.0)),
                if open_dir {
                    icons::FOLDER_OPEN
                } else {
                    icons::FOLDER
                },
                Color32::from_rgb(0xd9, 0xb5, 0x6a),
            );
            x += 15.0 + 6.0;
            painter.text(
                pos2(x, rect.center().y),
                Align2::LEFT_CENTER,
                &e.name,
                FontId::proportional(13.0),
                ink,
            );
            if !open_dir {
                // .count: how many files the folded folder holds.
                let n = tree::count_files(e).to_string();
                let g = painter.layout_no_wrap(n, FontId::proportional(11.0), p.chrome_muted);
                let name_w = painter
                    .layout_no_wrap(e.name.clone(), FontId::proportional(13.0), ink)
                    .size()
                    .x;
                let b = Rect::from_min_size(
                    pos2(x + name_w + 6.0, rect.center().y - 8.0),
                    vec2(g.size().x + 12.0, 16.0),
                );
                painter.rect_filled(b, CornerRadius::same(8), p.chrome_3);
                painter.galley(b.center() - g.size() / 2.0, g, p.chrome_muted);
            }
            if r.clicked() {
                toggle = Some(e.path.clone());
            }
        } else {
            x += 15.0;
            let (icon, color) = kind_icon(tree::kind(&e.name), p);
            icons::paint(
                painter,
                Rect::from_min_size(pos2(x, rect.center().y - 7.5), Vec2::splat(15.0)),
                icon,
                color,
            );
            x += 15.0 + 6.0;
            let name = painter.layout_no_wrap(e.name.clone(), FontId::proportional(13.0), ink);
            let name_w = name.size().x;
            painter.galley(pos2(x, rect.center().y - name.size().y / 2.0), name, ink);
            if e.path == ws.info.main {
                // The MAIN badge.
                let mut job = egui::text::LayoutJob::default();
                job.append(
                    "MAIN",
                    0.0,
                    egui::TextFormat {
                        font_id: FontId::new(9.5, theme::font::sans_600()),
                        color: p.accent_on_dark,
                        extra_letter_spacing: 0.76,
                        ..Default::default()
                    },
                );
                let g = ui.fonts_mut(|f| f.layout_job(job));
                let b = Rect::from_min_size(
                    pos2(
                        rect.right() - 10.0 - g.size().x - 8.0,
                        rect.center().y - 7.0,
                    ),
                    vec2(g.size().x + 8.0, 14.0),
                );
                let _ = name_w;
                painter.rect_stroke(
                    b,
                    CornerRadius::same(3),
                    Stroke::new(1.0, p.accent_on_dark.gamma_multiply(0.45)),
                    StrokeKind::Inside,
                );
                painter.galley(b.center() - g.size() / 2.0, g, p.accent_on_dark);
            }
            let tip = format!(
                "{}{}{}",
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
        if r.clicked() {
            ws.tree_cursor = Some(e.path.clone());
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

fn outline_rows(ui: &mut Ui, p: &Palette, ws: &mut Workspace, backend: &Backend) {
    if ws.headings.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.label(
                egui::RichText::new("Add a \\section to see the outline here.")
                    .size(12.0)
                    .color(p.chrome_muted),
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
    // Words per heading, with its subsections.
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
    let key = |h: &lemmata_project::Heading| format!("{}:{}", h.file, h.line);
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
        let folded = ws.outline_folded.contains(&key(h));
        if folded && has_children(i) {
            hide_below = Some(h.level);
        }
        let (rect, _) = ui.allocate_exact_size(vec2(w, 26.0), Sense::hover());
        let indent = 4.0 + (h.level.saturating_sub(min_level)) as f32 * 13.0;
        let twist = Rect::from_min_size(pos2(rect.left() + indent, rect.top()), vec2(14.0, 26.0));
        let entry = Rect::from_min_max(pos2(twist.right(), rect.top()), rect.max);
        if has_children(i) {
            let tr = ui.interact(twist, ui.make_persistent_id(("twist", i)), Sense::click());
            tr.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Button,
                    true,
                    if folded { "Expand" } else { "Collapse" },
                )
            });
            let g =
                ui.painter()
                    .layout_no_wrap("▾".into(), FontId::proportional(11.0), p.chrome_muted);
            let center = twist.center();
            let rot = if folded {
                -std::f32::consts::FRAC_PI_2
            } else {
                0.0
            };
            ui.painter().add(
                egui::epaint::TextShape::new(center - g.size() / 2.0, g, p.chrome_muted)
                    .with_angle_and_anchor(rot, Align2::CENTER_CENTER),
            );
            if tr.clicked() {
                fold = Some(key(h));
            }
        }
        let er = ui.interact(entry, ui.make_persistent_id(("heading", i)), Sense::click());
        let title = if h.title.is_empty() {
            "(untitled)".to_owned()
        } else {
            h.title.clone()
        };
        er.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &title));
        let on = current == Some(i);
        if on {
            ui.painter().rect_filled(entry, 0, p.chrome_active);
            ui.painter().rect_filled(
                Rect::from_min_size(entry.min, vec2(3.0, entry.height())),
                0,
                p.accent_on_dark,
            );
        } else if er.hovered() {
            ui.painter().rect_filled(entry, 0, p.chrome_hover);
        }
        let ink = if on {
            Color32::WHITE
        } else if er.hovered() || h.level == min_level {
            p.chrome_ink
        } else {
            p.chrome_muted
        };
        let n = totals[i].to_string();
        let ng = ui.painter().layout_no_wrap(
            n,
            FontId::monospace(10.5),
            p.chrome_muted.gamma_multiply(0.8),
        );
        let nx = entry.right() - 12.0 - ng.size().x;
        ui.painter().galley(
            pos2(nx, entry.center().y - ng.size().y / 2.0),
            ng,
            p.chrome_muted,
        );
        let mut job =
            egui::text::LayoutJob::simple_singleline(title, FontId::proportional(12.5), ink);
        job.wrap.max_width = nx - entry.left() - 4.0 - 8.0;
        job.wrap.max_rows = 1;
        job.wrap.overflow_character = Some('…');
        let tg = ui.fonts_mut(|f| f.layout_job(job));
        ui.painter().galley(
            pos2(entry.left() + 4.0, entry.center().y - tg.size().y / 2.0),
            tg,
            ink,
        );
        let er = er.on_hover_text(format!("{}:{}", h.file, h.line));
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
    ui.painter().rect_filled(rect, 0, p.surface_2);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, p.border),
    );
    let mut x = rect.left();
    let mut select = None;
    let mut close = None;
    let mut pin = None;
    let drag_id = egui::Id::unique("tab-drag");
    let dragging: Option<String> = ui.data(|d| d.get_temp(drag_id));
    let mut drop_at = None;
    let tabs = ws.tabs.clone();
    let mut tab_rects = Vec::new();
    for t in &tabs {
        let name = tree::base_name(t).to_owned();
        let viewer = tree::is_viewer_file(t);
        let g = ui
            .painter()
            .layout_no_wrap(name.clone(), FontId::proportional(13.0), p.muted);
        let pinned = ws.pinned.contains(t);
        let w = ((if pinned { 10.0 } else { 14.0 })
            + if viewer { 18.0 } else { 0.0 }
            + g.size().x
            + 4.0
            + 26.0
            + 4.0
            + 1.0)
            .min(220.0);
        let tr = Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height() - 1.0));
        tab_rects.push(tr);
        let on = ws.active.as_deref() == Some(t.as_str());
        let r = ui.interact(
            tr,
            ui.make_persistent_id(("tab", t)),
            Sense::click_and_drag(),
        );
        r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, on, &name));
        let painter = ui.painter();
        if on {
            painter.rect_filled(tr.with_max_y(rect.bottom()), 0, p.code_bg);
            painter.rect_filled(
                Rect::from_min_size(tr.min, vec2(tr.width(), 2.0)),
                0,
                p.accent,
            );
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
        let mut job =
            egui::text::LayoutJob::simple_singleline(name.clone(), FontId::proportional(13.0), ink);
        job.wrap.max_width = tr.right() - 4.0 - 26.0 - 4.0 - tx;
        job.wrap.max_rows = 1;
        job.wrap.overflow_character = Some('…');
        let g = ui.fonts_mut(|f| f.layout_job(job));
        painter.galley(pos2(tx, tr.center().y - g.size().y / 2.0), g, ink);
        // The close button, or the pin; a dot while unsaved.
        let xr = Rect::from_min_size(
            pos2(tr.right() - 1.0 - 4.0 - 26.0, tr.center().y - 13.0),
            Vec2::splat(26.0),
        );
        let xresp = ui.interact(xr, ui.make_persistent_id(("tab-x", t)), Sense::click());
        let dirty = ws.buffers.is_dirty(t);
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
                    .circle_filled(xr.right_top() + vec2(-6.5, 6.5), 3.5, p.accent);
            }
        } else if dirty && !hover {
            ui.painter().circle_filled(xr.center(), 3.5, p.accent);
        } else if hover || (on && !dirty) {
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

/// True when "Go to PDF" was clicked.
pub fn edit_bar(ui: &mut Ui, p: &Palette, ws: &mut Workspace) -> bool {
    let mut sync = false;
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, p.surface);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, p.border),
    );
    let disabled = !ws.active.as_deref().is_some_and(tree::is_tex);
    let mut c = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(10.0, 4.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    c.spacing_mut().item_spacing.x = 2.0;
    let tip = |label: &str, id: &str| format!("{label} ({})", display(keys_of(id)));
    let ibtn = |ui: &mut Ui, icon: Icon, label: &str| {
        Button::icon(icon, label)
            .size(Size::Sm)
            .enabled(!disabled)
            .show_icon(ui, p)
    };
    let sep = |ui: &mut Ui| {
        ui.add_space(4.0 + 6.0 - 2.0);
        let (r, _) = ui.allocate_exact_size(vec2(1.0, 18.0), Sense::hover());
        ui.painter().rect_filled(r, 0, p.border);
        ui.add_space(6.0 - 2.0 + 4.0);
    };
    #[allow(clippy::type_complexity)]
    let mut todo: Option<Box<dyn FnOnce(&mut Workspace)>> = None;
    let indent = Indent::Spaces(2);
    if ibtn(&mut c, icons::BOLD, "Bold")
        .on_hover_text(tip("Bold", "bold"))
        .clicked()
    {
        todo = Some(Box::new(|ws| {
            with_editor(ws, |e, _| cmd::toggle_wrap(&mut e.doc, "textbf"))
        }));
    }
    if ibtn(&mut c, icons::ITALIC, "Italic")
        .on_hover_text(tip("Italic", "italic"))
        .clicked()
    {
        todo = Some(Box::new(|ws| {
            with_editor(ws, |e, _| cmd::toggle_wrap(&mut e.doc, "textit"))
        }));
    }
    // `.heading`: the H and a small chevron, 2 px apart.
    let (hrect, h) = c.allocate_exact_size(
        vec2(9.0 + 16.0 + 2.0 + 12.0 + 9.0, 26.0),
        if disabled {
            Sense::hover()
        } else {
            Sense::click()
        },
    );
    h.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, !disabled, "Heading"));
    if !disabled && h.hovered() {
        c.painter()
            .rect_filled(hrect, CornerRadius::same(theme::R), p.surface_3);
    }
    let ink = if disabled {
        p.ink.gamma_multiply(0.5)
    } else {
        p.ink
    };
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
    if h.on_hover_text("Heading").clicked() {
        ws.menu = Some(Menu::Heading(hrect));
    }
    sep(&mut c);
    if ibtn(&mut c, icons::SIGMA, "Inline maths").clicked() {
        todo = Some(Box::new(|ws| {
            with_editor(ws, |e, _| wrap_with(&mut e.doc, "$", "$"))
        }));
    }
    // shortcut: Figure, Table and Cite open their dialogs in M6.
    if ibtn(&mut c, icons::PI, "Symbols")
        .on_hover_text(tip("Symbols", "symbols"))
        .clicked()
    {
        todo = Some(Box::new(|ws| ws.symbols = Some(Default::default())));
    }
    if ibtn(&mut c, icons::LINK, "Link").clicked() {
        todo = Some(Box::new(move |ws| {
            with_editor(ws, |e, i| e.insert_template("\\href{${https://}}{${}}", i))
        }));
    }
    let _ = ibtn(&mut c, icons::IMAGE, "Figure");
    let _ = ibtn(&mut c, icons::TABLE, "Table");
    let _ = Button::new("Cite")
        .with_icon(icons::BOOK_OPEN_TEXT)
        .kind(Kind::Ghost)
        .size(Size::Sm)
        .enabled(!disabled)
        .show(&mut c, p)
        .on_hover_text(tip("Insert a citation", "cite"));
    sep(&mut c);
    for (icon, label, env) in [
        (icons::LIST, "Bulleted list", "itemize"),
        (icons::LIST_ORDERED, "Numbered list", "enumerate"),
    ] {
        if ibtn(&mut c, icon, label).clicked() {
            let t = format!("\\begin{{{env}}}\n\t\\item ${{}}\n\\end{{{env}}}");
            todo = Some(Box::new(move |ws| {
                with_editor(ws, |e, i| e.insert_template(&t, i))
            }));
        }
    }
    sep(&mut c);
    // shortcut: comments land in M6.
    let _ = Button::new("Comment")
        .with_icon(icons::MESSAGE_SQUARE_PLUS)
        .kind(Kind::Ghost)
        .size(Size::Sm)
        .enabled(ws.active.is_some())
        .show(&mut c, p)
        .on_hover_text(tip("Add a comment on the selection", "add-comment"));
    if Button::new("Go to PDF")
        .with_icon(icons::LOCATE)
        .kind(Kind::Ghost)
        .size(Size::Sm)
        .enabled(!disabled)
        .show(&mut c, p)
        .on_hover_text(tip("Go to PDF", "sync"))
        .clicked()
    {
        sync = true;
    }
    let _ = indent;
    if let Some(f) = todo {
        f(ws);
    }
    sync
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
    ui.painter().rect_filled(rect, 0, p.surface_2);
    ui.painter()
        .hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, p.border));
    let mut c = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(12.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    c.spacing_mut().item_spacing.x = 16.0;
    let small = |t: String, color: Color32| egui::RichText::new(t).size(12.0).color(color);
    let line = if ws.cursor.0.is_empty() {
        1
    } else {
        ws.cursor.1
    };
    c.label(small(format!("Line {line}"), p.muted));
    if ws.selection_words > 0 {
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
            small(text.into(), p.error).font(FontId::new(12.0, theme::font::sans_600()))
        } else {
            small(text.into(), p.muted)
        };
        c.label(rt);
    }
    c.with_layout(egui::Layout::right_to_left(egui::Align::Center), |c| {
        c.spacing_mut().item_spacing.x = 16.0;
        // shortcut: the notification bell (M7), spell language (M6) and completion status (M5) come later.
        if let Some(o) = ws.build.outcome.as_ref().filter(|_| !ws.build.building) {
            use lemmata_tex::Status;
            if o.status == Status::Success {
                // shortcut: the "Finished at" time on hover needs local time formatting; left out.
                c.label(small(format!("Compiled in {:.1} s", o.duration_ms as f64 / 1000.0), p.ok));
            } else {
                // The failure opens the logs that explain it.
                let errors = ws.build.errors();
                let text = match o.status {
                    Status::Timeout => "Stopped: took too long".to_owned(),
                    Status::Cancelled => "Stopped".to_owned(),
                    _ if errors > 0 => format!("{errors} error{}", if errors == 1 { "" } else { "s" }),
                    _ => "Compile failed".to_owned(),
                };
                if status_link(c, text, p.error, "Show the logs", true).clicked() {
                    ws.build.logs_open = true;
                    if settings.layout == "editor" {
                        settings.layout = "split".into();
                        changed = true;
                    }
                }
            }
        }
        status_link(c, engine_name(ws.info.config.engine).into(), p.muted, "Compiler: change it in Project settings", false);
        if ws.total_words > 0 {
            status_link(c, format!("{} words", group_digits(ws.total_words)), p.muted, "Words in the document body, without commands, maths or comments. Click for the count per file.", false);
        }
        if ws.info.config.draft
            && status_link(c, "Draft mode".into(), p.warn, "Figures are drawn as boxes. Click to build normally again.", false).clicked()
        {
            ws.set_config(backend, settings, |c| c.draft = false);
        }
        if ws.tex_outdated {
            c.label(small("Old TeX Live".into(), p.warn)).on_hover_text("Some packages may lack newer features");
        }
        match &ws.lsp.state {
            crate::lsp::State::Starting => {
                // .starting: a small spinning ring, then the words.
                let r = c.label(small("Starting completion".into(), p.muted)).on_hover_text("The language server is starting");
                let ring = Rect::from_center_size(pos2(r.rect.left() - 10.0, r.rect.center().y), vec2(8.0, 8.0));
                let t = c.input(|i| i.time) as f32;
                c.painter().add(egui::Shape::Path(egui::epaint::PathShape::line(
                    (0..=12).map(|k| {
                        let a = t * std::f32::consts::TAU / 0.8 + k as f32 * 0.4;
                        ring.center() + vec2(a.cos(), a.sin()) * 3.5
                    }).collect(),
                    Stroke::new(1.5, p.muted),
                )));
                c.add_space(8.0);
                c.ctx().request_repaint();
            }
            crate::lsp::State::Failed(_)
                if status_link(c, "No completion".into(), p.muted, "Why is completion off?", false).clicked() =>
            {
                {
                    ws.notice("Completion, hover and go to definition come from texlab, which Lemmata downloads the first time it's needed but couldn't start. Check the internet connection, or install texlab yourself (your package manager, or cargo install texlab), then reopen the project.", Tone::Info);
                }
            }
            _ => {}
        }
    });
    changed
}

/// `.status-link`: plain text until hovered, then underlined (`.bad` ones always are).
fn status_link(ui: &mut Ui, text: String, color: Color32, tip: &str, bad: bool) -> egui::Response {
    let r = ui.add(
        egui::Label::new(egui::RichText::new(text).size(12.0).color(color)).sense(Sense::click()),
    );
    if bad || r.hovered() {
        ui.painter().hline(
            r.rect.x_range(),
            r.rect.bottom() - 1.0,
            Stroke::new(1.0, color),
        );
    }
    r.on_hover_text(tip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// 1,177 with the separator for thousands, as Intl.NumberFormat() in English.
fn group_digits(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
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
    let id = egui::Id::unique("workspace-menu");
    match menu {
        Menu::Main(anchor) => {
            let focus = if ws.focus_mode {
                "Leave focus mode"
            } else {
                "Focus mode"
            };
            let snapshot = display(keys_of("snapshot"));
            let focus_key = display(keys_of("focus"));
            let shortcuts_key = display(keys_of("shortcuts"));
            let palette_key = display(keys_of("palette"));
            let items = [
                MenuItem::Heading("Project"),
                MenuItem::plain("All projects"),
                MenuItem::plain("Project settings"),
                MenuItem::plain("Save as template…"),
                MenuItem::plain("Word count"),
                MenuItem::Separator,
                MenuItem::Heading("Tools"),
                MenuItem::plain("Git"),
                MenuItem::plain("History"),
                MenuItem::plain("Save a snapshot").hint(&snapshot),
                MenuItem::plain("Comments"),
                MenuItem::plain("TODOs"),
                MenuItem::plain("Bibliography manager…"),
                MenuItem::plain("Format document"),
                MenuItem::plain("Reindent lines").hint("the selection, or the whole file"),
                MenuItem::plain(focus).hint(&focus_key),
                MenuItem::Separator,
                MenuItem::Heading("Export"),
                MenuItem::plain("Export PDF…"),
                MenuItem::plain("Export project (.zip)…"),
                MenuItem::plain("Export for arXiv (.zip)…"),
                MenuItem::plain("Export for arXiv as one .tex file (.zip)…"),
                MenuItem::Separator,
                MenuItem::Heading("Help"),
                MenuItem::plain("Preferences"),
                MenuItem::plain("Keyboard shortcuts").hint(&shortcuts_key),
                MenuItem::plain("User guide"),
                MenuItem::plain("Report a problem…"),
                MenuItem::plain("Command palette").hint(&palette_key),
            ];
            match widgets::drop_menu(ctx, p, id, anchor, false, &items) {
                MenuResult::Open => {}
                MenuResult::Closed => ws.menu = None,
                MenuResult::Clicked(i) => {
                    ws.menu = None;
                    match i {
                        1 => *action = Some(Action::Library),
                        15 => ws.focus_mode = !ws.focus_mode,
                        18 => ws.export(backend, settings, Save::Pdf),
                        19 => ws.export(backend, settings, Save::Zip),
                        20 => ws.export(backend, settings, Save::Arxiv { flatten: false }),
                        21 => ws.export(backend, settings, Save::Arxiv { flatten: true }),
                        27 => {
                            const GUIDE: &str =
                                "https://github.com/NilsBriggen/lemmata/blob/main/docs/guide.md";
                            backend.spawn(|shell| Msg::Done(shell.open_url(GUIDE.into())));
                        }
                        // shortcut: the other entries open panels and dialogs that land in M4–M7.
                        _ => ws.notice("Not in the native app yet.", Tone::Info),
                    }
                }
            }
        }
        Menu::Compile(anchor) => {
            let cfg = ws.info.config.clone();
            let mut items = vec![
                MenuItem::Heading("Compile"),
                MenuItem::check("After every save", settings.auto_compile),
                MenuItem::check("Only when I recompile", !settings.auto_compile),
                MenuItem::Separator,
                MenuItem::Heading("Compiler"),
            ];
            for (e, label) in ENGINES {
                items.push(MenuItem::check(label, cfg.engine == e));
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
                MenuResult::Clicked(i) => match i {
                    1 | 2 => {
                        settings.auto_compile = i == 1;
                        *action = Some(Action::Settings);
                    }
                    5..=8 => ws.set_config(backend, settings, |c| c.engine = ENGINES[i - 5].0),
                    12 | 13 => ws.set_config(backend, settings, |c| c.draft = i == 13),
                    17 | 18 => ws.set_config(backend, settings, |c| c.stop_on_error = i == 18),
                    20 => {
                        ws.menu = None;
                        ws.clean(backend);
                    }
                    21 => {
                        ws.menu = None;
                        ws.stop_build(backend);
                    }
                    _ => {}
                },
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
                MenuItem::check("Collapse all folders", false),
                MenuItem::check("Expand all folders", false),
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
        Menu::Tab(path, anchor) => {
            let pinned = ws.pinned.contains(&path);
            let i = ws.tabs.iter().position(|t| *t == path).unwrap_or(0);
            let last = i + 1 == ws.tabs.len();
            let reopen = ws
                .closed
                .first()
                .map(|c| format!("Reopen {}", tree::base_name(c)));
            let mut items = vec![
                MenuItem::plain(if pinned { "Unpin" } else { "Pin" }),
                MenuItem::Separator,
                MenuItem::plain("Close"),
                MenuItem::plain("Close others").enabled(ws.tabs.len() >= 2),
                MenuItem::plain("Close to the right").enabled(!last),
                MenuItem::plain("Close all"),
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
